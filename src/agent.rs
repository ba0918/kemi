//! `kemi wait` と `kemi reply`（agent-channel.md の R-AGENT-CLI）。
//!
//! 動いているレビューの `<id>.endpoint` からエージェント用の API のポートとトークンを読み、
//! ループバックへ平文の HTTP/1.1 の要求を送る（`kemi wait` は待つ要求と受け取りの知らせの 2 つ）。
//! 送るのはこの形だけなので、HTTP クライアントのクレートは足さない。

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::path::PathBuf;

use kemi_core::session::{Endpoint, SessionStore, is_valid_id, read_endpoint};

pub const USAGE: &str = "\
kemi: usage:
  kemi wait <id> [--timeout <seconds>]   wait for what the reviewer hands over, then print it
  kemi reply <id>                        write replies and messages read as JSON from stdin
  (to open a manifest named wait or reply, write it as a path: kemi ./wait)";

/// 最初の引数で選ぶサブコマンド。
pub enum Command {
    Wait { id: String, timeout: Option<u64> },
    Reply { id: String },
}

/// `wait` / `reply` の後ろの引数を読む。ほかのフラグや入力モードとは組まない。
pub fn parse(name: &str, rest: &[String]) -> Result<Command, String> {
    let mut id = None;
    let mut timeout = None;
    let mut index = 0;
    while index < rest.len() {
        let arg = rest[index].as_str();
        match arg {
            "--timeout" if name == "wait" => {
                index += 1;
                let value = rest
                    .get(index)
                    .ok_or_else(|| "--timeout requires a number of seconds".to_string())?;
                timeout = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("--timeout requires a number of seconds: {value}"))?,
                );
            }
            other if other.starts_with('-') => {
                return Err(format!("kemi {name} does not take {other}"));
            }
            other => {
                if id.is_some() {
                    return Err(format!("kemi {name} takes one review id"));
                }
                id = Some(other.to_string());
            }
        }
        index += 1;
    }
    let id = id.ok_or_else(|| format!("kemi {name} requires a review id"))?;
    Ok(match name {
        "wait" => Command::Wait { id, timeout },
        _ => Command::Reply { id },
    })
}

/// 走らせて終了コードを返す。stdout には契約の JSON だけを出す。
pub fn run(command: Command, sessions: Option<PathBuf>, endpoints: Option<PathBuf>) -> i32 {
    match command {
        Command::Wait { id, timeout } => {
            let endpoint = match find(&id, sessions, endpoints) {
                Ok(endpoint) => endpoint,
                Err(code) => return code,
            };
            let body = serde_json::json!({ "timeout_ms": timeout.map(|seconds| seconds * 1000) });
            match post(&endpoint, "wait", body.to_string().as_bytes()) {
                Ok((200, answer)) => match acknowledge(&endpoint, &answer) {
                    Ok(()) => print_events(&id, &answer),
                    Err(message) => failed(&message),
                },
                Ok((_, answer)) => refused(&answer),
                Err(message) => failed(&message),
            }
        }
        Command::Reply { id } => {
            let mut input = Vec::new();
            if let Err(error) = std::io::stdin().read_to_end(&mut input) {
                return failed(&format!("cannot read the writes from stdin: {error}"));
            }
            let endpoint = match find(&id, sessions, endpoints) {
                Ok(endpoint) => endpoint,
                Err(code) => return code,
            };
            match post(&endpoint, "reply", &input) {
                Ok((200, answer)) => {
                    let ids = answer.get("ids").cloned().unwrap_or_default();
                    print_json(&serde_json::json!({ "ids": ids }));
                    0
                }
                Ok((_, answer)) => refused(&answer),
                Err(message) => failed(&message),
            }
        }
    }
}

/// 動いているレビューのつなぎ先。無いか、ロックが死んでいれば（強制終了の残り）無いものと
/// して扱う（R-AGENT-LINK）。submit で終わった id と存在しない id は見分けられないので、
/// どちらにも結果の読み方を案内する。
fn find(id: &str, sessions: Option<PathBuf>, endpoints: Option<PathBuf>) -> Result<Endpoint, i32> {
    let not_running = || {
        failed(&format!(
            "no running review {id}; if it was submitted, print its result with kemi --result"
        ))
    };
    if !is_valid_id(id) {
        return Err(not_running());
    }
    let (Some(sessions), Some(endpoints)) = (sessions, endpoints) else {
        return Err(not_running());
    };
    match read_endpoint(&endpoints, id) {
        Ok(Some(endpoint)) if SessionStore::new(sessions).is_running(id) => Ok(endpoint),
        Ok(_) => Err(not_running()),
        Err(error) => Err(failed(&format!("cannot read the review endpoint: {error}"))),
    }
}

/// 応答を受け取りきったことをレビューに知らせる。知らせるまで、レビューは起きたことを
/// 消さずに持ち、途中で終わった `kemi wait` の分は次の `kemi wait` が受け取る。知らせが
/// 届かなければ何も出さずに終わり、起きたことは次の `kemi wait` に残る。時間切れと、
/// submit を含む応答（レビューは止まり始めている）では知らせない。
fn acknowledge(endpoint: &Endpoint, answer: &serde_json::Value) -> Result<(), String> {
    let submitted = answer
        .get("events")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|events| events.iter().any(|event| event["type"] == "submitted"));
    let Some(through) = answer.get("through").filter(|_| !submitted) else {
        return Ok(());
    };
    let body = serde_json::json!({ "through": through });
    match post(endpoint, "received", body.to_string().as_bytes())? {
        (200, _) => Ok(()),
        (_, answer) => Err(reason(&answer).to_string()),
    }
}

fn print_events(id: &str, answer: &serde_json::Value) -> i32 {
    if answer.get("timeout").and_then(serde_json::Value::as_bool) == Some(true) {
        return 3;
    }
    let events = answer.get("events").cloned().unwrap_or_default();
    // submit が入っていれば、終了コードは submit と同じ（承認 0 / 変更要求 1）。
    let verdict = events.as_array().and_then(|events| {
        events
            .iter()
            .find(|event| event["type"] == "submitted")
            .and_then(|event| event["result"]["verdict"].as_str())
            .map(str::to_string)
    });
    print_json(&serde_json::json!({ "kemi": 2, "review": id, "events": events }));
    match verdict.as_deref() {
        Some("changes_requested") => 1,
        _ => 0,
    }
}

fn print_json(value: &serde_json::Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
}

fn refused(answer: &serde_json::Value) -> i32 {
    failed(reason(answer))
}

/// 断られた応答の理由。
fn reason(answer: &serde_json::Value) -> &str {
    answer
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("the review refused the request")
}

fn failed(message: &str) -> i32 {
    eprintln!("kemi: {message}");
    2
}

/// `POST /a/<token>/<action>` を送り、(状態コード, 本文の JSON) を返す。
fn post(
    endpoint: &Endpoint,
    action: &str,
    body: &[u8],
) -> Result<(u16, serde_json::Value), String> {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port))
        .map_err(|error| format!("cannot reach the review: {error}"))?;
    let head = format!(
        "POST /a/{}/{action} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        endpoint.token,
        endpoint.port,
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|error| format!("cannot reach the review: {error}"))?;
    let mut response = Vec::new();
    // Connection: close なので、サーバが閉じるまで読めば応答の全部になる。
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("the review ended before answering: {error}"))?;
    let (status, body) =
        parse_response(&response).ok_or("the review ended before answering".to_string())?;
    let answer = serde_json::from_slice(&body)
        .map_err(|error| format!("cannot read the answer of the review: {error}"))?;
    Ok((status, answer))
}

/// HTTP/1.1 の応答を状態コードと本文に分ける。本文は `Content-Length` か chunked か、
/// 閉じるまでのどれか。
fn parse_response(response: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&response[..split]).ok()?;
    let rest = &response[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let mut chunked = false;
    let mut length = None;
    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.eq_ignore_ascii_case("chunked");
        } else if name.eq_ignore_ascii_case("content-length") {
            length = value.parse::<usize>().ok();
        }
    }
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(length) = length {
        rest.get(..length)?.to_vec()
    } else {
        rest.to_vec()
    };
    Some((status, body))
}

fn dechunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line_end = rest.windows(2).position(|window| window == b"\r\n")?;
        let size_text = std::str::from_utf8(&rest[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Some(body);
        }
        body.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunked_answer_is_put_back_together() {
        let response =
            b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\n{\"a\":\r\n2\r\n1}\r\n0\r\n\r\n";

        assert_eq!(parse_response(response), Some((200, b"{\"a\":1}".to_vec())));
    }

    #[test]
    fn an_answer_with_a_length_is_cut_at_the_length() {
        let response = b"HTTP/1.1 409 Conflict\r\nContent-Length: 2\r\n\r\n{}trailing";

        assert_eq!(parse_response(response), Some((409, b"{}".to_vec())));
    }

    #[test]
    fn a_cut_off_answer_is_not_read() {
        assert_eq!(parse_response(b"HTTP/1.1 200 OK\r\n"), None);
        assert_eq!(
            parse_response(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\n{\"a"),
            None
        );
    }
}
