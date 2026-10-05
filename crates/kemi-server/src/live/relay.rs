//! 開発サーバへの転送（R-PAGE-PROXY）。HTTP は 1 要求ごとに接続して送り、WebSocket は
//! upgrade の後にバイト列をそのまま両方向へ流す（中身を解釈しない）。

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use super::{LiveState, LiveTarget, rewrite};

/// 書き換える HTML の上限。これより大きい HTML は書き換えずに断る。
const HTML_LIMIT: usize = 32 * 1024 * 1024;

pub(super) async fn reachable(target: &LiveTarget) -> bool {
    match target {
        LiveTarget::Url { authority, .. } => TcpStream::connect(authority.as_str()).await.is_ok(),
    }
}

pub(super) async fn forward(state: &LiveState, request: Request) -> Response {
    let LiveTarget::Url {
        authority, display, ..
    } = &state.target;
    let host = request_host(request.headers());
    let stream = match TcpStream::connect(authority.as_str()).await {
        Ok(stream) => stream,
        Err(_) => return waiting_page(state, &host, display),
    };
    let (mut sender, connection) =
        match hyper::client::conn::http1::handshake(TokioIo::new(stream)).await {
            Ok(pair) => pair,
            Err(_) => return waiting_page(state, &host, display),
        };
    tokio::spawn(async move {
        let _ = connection.with_upgrades().await;
    });

    let mut request = request;
    let client_upgrade = hyper::upgrade::on(&mut request);
    let (mut parts, body) = request.into_parts();
    if let Ok(value) = HeaderValue::from_str(authority) {
        parts.headers.insert(header::HOST, value);
    }
    // 書き換える HTML を圧縮されずに受け取る。
    parts.headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );
    let outgoing = hyper::Request::from_parts(parts, body);
    let mut response = match sender.send_request(outgoing).await {
        Ok(response) => response,
        Err(_) => return waiting_page(state, &host, display),
    };

    if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        let server_upgrade = hyper::upgrade::on(&mut response);
        tokio::spawn(async move {
            let (client, server) = tokio::join!(client_upgrade, server_upgrade);
            if let (Ok(client), Ok(server)) = (client, server) {
                let (mut client, mut server) = (TokioIo::new(client), TokioIo::new(server));
                let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
            }
        });
        let (parts, body) = response.into_parts();
        return Response::from_parts(parts, Body::new(body));
    }

    let (mut parts, body) = response.into_parts();
    let rewrote = rewrite_headers(&mut parts.headers, state, &host, authority);
    let html = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/html"));
    if !html {
        return Response::from_parts(parts, Body::new(body));
    }
    let bytes = match axum::body::to_bytes(Body::new(body), HTML_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                "kemi: the page is too large to relay",
            )
                .into_response();
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let injected = rewrite::inject_script(&text, &script_tag(state, &host, &rewrote, true));
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.remove(header::TRANSFER_ENCODING);
    Response::from_parts(parts, Body::from(injected))
}

/// `X-Frame-Options` を外し、CSP を書き換え、開発サーバ自身への転送先を中継の中に向ける。
/// 書き換えた見出しの名前を返す（ページに書き換えたことを出すため）。
fn rewrite_headers(
    headers: &mut HeaderMap,
    state: &LiveState,
    host: &str,
    authority: &str,
) -> Vec<&'static str> {
    let mut rewrote = Vec::new();
    if headers.remove(header::X_FRAME_OPTIONS).is_some() {
        rewrote.push("x-frame-options");
    }
    let policies: Vec<String> = headers
        .get_all(header::CONTENT_SECURITY_POLICY)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect();
    if !policies.is_empty() {
        headers.remove(header::CONTENT_SECURITY_POLICY);
        let ancestors = review_origin(state, host);
        let script = format!("http://{host}/__kemi/page.js");
        for policy in policies {
            let rewritten = rewrite::rewrite_csp(&policy, &ancestors, &script);
            if rewritten != policy && !rewrote.contains(&"content-security-policy") {
                rewrote.push("content-security-policy");
            }
            if let Ok(value) = HeaderValue::from_str(&rewritten) {
                headers.append(header::CONTENT_SECURITY_POLICY, value);
            }
        }
    }
    let location = headers
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| rewrite::relative_location(value, authority));
    if let Some(location) = location.and_then(|value| HeaderValue::from_str(&value).ok()) {
        headers.insert(header::LOCATION, location);
    }
    rewrote
}

/// 要求の `Host`。HTML の属性と CSP に入れるので、アドレスとポートに使う文字だけを通す。
fn request_host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| {
            !host.is_empty()
                && host
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || ".:-".contains(character))
        })
        .unwrap_or("127.0.0.1")
        .to_string()
}

/// 中継したページを枠に入れてよいレビュー画面のオリジン。ページと同じホスト名で開かれる。
fn review_origin(state: &LiveState, host: &str) -> String {
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    format!("http://{name}:{}", state.review_port)
}

fn script_tag(state: &LiveState, host: &str, rewrote: &[&str], reachable: bool) -> String {
    format!(
        r#"<script src="/__kemi/page.js" data-kemi-review="{}" data-kemi-rewrote="{}" data-kemi-reachable="{reachable}"></script>"#,
        review_origin(state, host),
        rewrote.join(" "),
    )
}

/// 開発サーバにつながらないときのページ（R-PAGE-MODE）。つながるまで確かめ続け、
/// つながったら読み込み直す。
fn waiting_page(state: &LiveState, host: &str, display: &str) -> Response {
    let shown = display
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let html = format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Waiting for the page</title>{}
<style>body {{ font: 15px/1.5 system-ui, sans-serif; margin: 2rem; color: #444; }}</style>
</head><body>
<p id="kemi-unreachable">kemi cannot reach {shown}. Waiting for the dev server to start&hellip;</p>
<script>
setInterval(async () => {{
  try {{
    const response = await fetch('/__kemi/alive', {{ cache: 'no-store' }});
    if (response.ok) location.reload();
  }} catch {{}}
}}, 500);
</script>
</body></html>
"#,
        script_tag(state, host, &[], false)
    );
    (
        StatusCode::BAD_GATEWAY,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}
