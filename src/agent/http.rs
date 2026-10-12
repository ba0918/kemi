//! 動いているレビューへのループバックの HTTP/1.1 の要求と、応答の読み取り。HTTP クライアントの
//! 依存を足さない理由は親モジュールにある。

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};

use kemi_core::session::Endpoint;

/// `POST /a/<token>/<action>` を送り、(状態コード, 本文の JSON) を返す。
pub(super) fn post(
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
