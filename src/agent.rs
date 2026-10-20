//! `kemi wait` / `reply` / `explain`（R-AGENT-CLI、R-EXPLANATION-CLI）。
//!
//! 動いているレビューの `<id>.endpoint` からエージェント用の API のポートとトークンを読み、
//! ループバックへ平文の HTTP/1.1 の要求を送る（`kemi wait` は待つ要求と受け取りの知らせの 2 つ）。
//! 送るのはこの形だけなので、HTTP クライアントのクレートは足さない。

use std::io::{Read, Write};
use std::path::PathBuf;

use kemi_core::domain::submit::{RESULT_CONTRACT_VERSION, Verdict};
use kemi_core::session::{Endpoint, SessionStore, is_valid_id, read_endpoint};

mod http;

use http::post;

use crate::report;

pub const USAGE: &str = "\
kemi: usage:
  kemi wait <id> [--timeout <seconds>]   wait for what the reviewer hands over, then print it
  kemi reply <id>                        write replies and messages read as JSON from stdin
  kemi explain <id> [--file <file>]     replace the explanation from JSON
  kemi explain <id> --context           print the current reference map without reading stdin
  (to open a manifest named wait, reply or explain, write it as a path: kemi ./explain)";

/// 最初の引数で選ぶサブコマンド。
pub enum Command {
    Explain {
        id: String,
        file: Option<PathBuf>,
        context: bool,
    },
    Wait {
        id: String,
        timeout: Option<u64>,
    },
    Reply {
        id: String,
    },
}

/// `wait` / `reply` の後ろの引数を読む。ほかのフラグや入力モードとは組まない。
pub fn parse(name: &str, rest: &[String]) -> Result<Command, String> {
    let mut id = None;
    let mut timeout = None;
    let mut file = None;
    let mut context = false;
    let mut index = 0;
    while index < rest.len() {
        let arg = rest[index].as_str();
        match arg {
            "--file" if name == "explain" => {
                if file.is_some() {
                    return Err("--file may be given only once".into());
                }
                index += 1;
                file = Some(PathBuf::from(
                    rest.get(index).ok_or("--file requires a path")?,
                ));
            }
            "--context" if name == "explain" => {
                if context {
                    return Err("--context may be given only once".into());
                }
                context = true;
            }
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
    if context && file.is_some() {
        return Err("--context and --file cannot be used together".into());
    }
    Ok(match name {
        "wait" => Command::Wait { id, timeout },
        "explain" => Command::Explain { id, file, context },
        _ => Command::Reply { id },
    })
}

/// 走らせて終了コードを返す。stdout には契約の JSON だけを出す。
pub fn run(command: Command, sessions: Option<PathBuf>, endpoints: Option<PathBuf>) -> i32 {
    match command {
        Command::Explain { id, file, context } => {
            let endpoint = match find(&id, sessions, endpoints) {
                Ok(endpoint) => endpoint,
                Err(code) => return code,
            };
            let input = if context {
                b"{}".to_vec()
            } else if let Some(file) = file {
                match std::fs::read(file) {
                    Ok(bytes) => bytes,
                    Err(error) => return failed(&format!("cannot read the explanation: {error}")),
                }
            } else {
                let mut bytes = Vec::new();
                if let Err(error) = std::io::stdin().read_to_end(&mut bytes) {
                    return failed(&format!("cannot read the explanation from stdin: {error}"));
                }
                bytes
            };
            match post(
                &endpoint,
                if context { "context" } else { "explain" },
                &input,
            ) {
                Ok((200, answer)) => {
                    print_json(&answer);
                    report::SUCCESS
                }
                Ok((_, answer)) => refused(&answer),
                Err(message) => failed(&message),
            }
        }
        Command::Wait { id, timeout } => {
            let endpoint = match find(&id, sessions, endpoints) {
                Ok(endpoint) => endpoint,
                Err(code) => return code,
            };
            let body = serde_json::json!({ "timeout_ms": timeout.map(|seconds| seconds.saturating_mul(1000)) });
            match post(&endpoint, "wait", body.to_string().as_bytes()) {
                Ok((200, answer)) => {
                    finish_wait(&id, &answer, &mut std::io::stdout().lock(), || {
                        acknowledge(&endpoint, &answer)
                    })
                }
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
                    report::SUCCESS
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

/// 起きたことを stdout に出し終えてから、受け取ったことをレビューに知らせる。知らせるまで、
/// レビューは起きたことを消さずに持ち、出せずに終わった `kemi wait` の分は次の `kemi wait` が
/// 受け取る。出し終えた後に知らせが届かなくても（知らせる前に submit されると、レビューは
/// もうつなぎを受けない）、出したものは失われていないので、出したものに合った終了コードで
/// 終わる。知らせを先にすると、その submit で起きたことが誰にも渡らずに消える。
fn finish_wait(
    id: &str,
    answer: &serde_json::Value,
    out: &mut dyn Write,
    acknowledge: impl FnOnce() -> Result<(), String>,
) -> i32 {
    let code = match print_events(id, answer, out) {
        Ok(code) => code,
        Err(error) => return failed(&format!("cannot print the events: {error}")),
    };
    // 知らせが届かなかったら、次の `kemi wait` が同じ起きたことをもう一度受け取ることがある。
    let _ = acknowledge();
    code
}

/// 受け取りの知らせ。時間切れと、submit を含む応答（レビューは止まり始めている）では
/// 知らせない。
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

fn print_events(id: &str, answer: &serde_json::Value, out: &mut dyn Write) -> std::io::Result<i32> {
    if answer.get("timeout").and_then(serde_json::Value::as_bool) == Some(true) {
        return Ok(report::TIMEOUT);
    }
    let events = answer.get("events").cloned().unwrap_or_default();
    // submit が入っていれば、終了コードは submit と同じ（承認 0 / 変更要求 1）。
    let verdict = events.as_array().and_then(|events| {
        events
            .iter()
            .find(|event| event["type"] == "submitted")
            .and_then(|event| event["result"]["verdict"].as_str())
            .and_then(Verdict::parse)
    });
    let printed = serde_json::json!({
        "kemi": RESULT_CONTRACT_VERSION,
        "review": id,
        "events": events,
    });
    writeln!(out, "{printed}")?;
    out.flush()?;
    Ok(verdict.map_or(report::SUCCESS, Verdict::exit_code))
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
    report::warn(message);
    report::ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(name: &str, args: &[&str]) -> Result<Command, String> {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse(name, &args)
    }

    #[test]
    fn each_command_takes_one_id_and_only_its_own_flags() {
        assert!(matches!(
            parse_args("wait", &["ID"]),
            Ok(Command::Wait { id, timeout: None }) if id == "ID"
        ));
        assert!(matches!(
            parse_args("wait", &["ID", "--timeout", "5"]),
            Ok(Command::Wait {
                timeout: Some(5),
                ..
            })
        ));
        assert!(matches!(
            parse_args("reply", &["ID"]),
            Ok(Command::Reply { id }) if id == "ID"
        ));
        assert!(matches!(
            parse_args("explain", &["ID"]),
            Ok(Command::Explain {
                file: None,
                context: false,
                ..
            })
        ));
        assert!(matches!(
            parse_args("explain", &["ID", "--file", "update.json"]),
            Ok(Command::Explain {
                file: Some(_),
                context: false,
                ..
            })
        ));
        assert!(matches!(
            parse_args("explain", &["ID", "--context"]),
            Ok(Command::Explain {
                file: None,
                context: true,
                ..
            })
        ));

        let refused: &[(&str, &[&str])] = &[
            ("wait", &[]),
            ("wait", &["ID", "extra"]),
            ("wait", &["--timeout", "1"]),
            ("wait", &["ID", "--timeout"]),
            ("wait", &["ID", "--timeout", "soon"]),
            ("wait", &["ID", "--worktree"]),
            ("wait", &["ID", "--no-open"]),
            ("wait", &["ID", "--explanation", "e.json"]),
            ("wait", &["ID", "--file", "update.json"]),
            ("wait", &["ID", "--context"]),
            ("reply", &[]),
            ("reply", &["ID", "extra"]),
            ("reply", &["ID", "--timeout", "1"]),
            ("reply", &["ID", "--no-open"]),
            ("reply", &["ID", "--explanation", "e.json"]),
            ("reply", &["ID", "--file", "update.json"]),
            ("reply", &["ID", "--context"]),
            ("explain", &[]),
            ("explain", &["ID", "extra"]),
            ("explain", &["ID", "--file"]),
            ("explain", &["ID", "--file", "a.json", "--file", "b.json"]),
            ("explain", &["ID", "--context", "--context"]),
            ("explain", &["ID", "--context", "--file", "update.json"]),
            ("explain", &["ID", "--timeout", "1"]),
            ("explain", &["ID", "--explanation", "e.json"]),
        ];
        for (name, args) in refused {
            assert!(parse_args(name, args).is_err(), "kemi {name} {args:?}");
        }
    }

    fn handed_answer() -> serde_json::Value {
        serde_json::json!({
            "events": [{ "type": "handed", "comments": [], "replies": [], "messages": [] }],
            "through": 1
        })
    }

    /// 受け取りの知らせが届かない（submit でレビューが止まり始め、つなげない）ときも、
    /// 受け取った起きたことは出して、それに合った終了コードで終わる。
    #[test]
    fn events_are_printed_even_if_the_review_stopped_before_the_acknowledgement() {
        let answer = handed_answer();
        let mut out = Vec::new();

        let code = finish_wait("01K5", &answer, &mut out, || {
            Err("cannot reach the review".to_string())
        });

        assert_eq!(code, 0);
        let printed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            printed,
            serde_json::json!({ "kemi": 3, "review": "01K5", "events": answer["events"] })
        );
    }

    struct ClosedStdout;

    impl Write for ClosedStdout {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    /// 出せなかった起きたことは受け取ったと知らせず、次の `kemi wait` に残す。
    #[test]
    fn events_that_could_not_be_printed_are_not_acknowledged() {
        let mut acknowledged = false;

        let code = finish_wait("01K5", &handed_answer(), &mut ClosedStdout, || {
            acknowledged = true;
            Ok(())
        });

        assert_eq!(code, 2);
        assert!(!acknowledged);
    }
}
