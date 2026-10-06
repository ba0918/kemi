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
pub(super) const HTML_LIMIT: usize = 32 * 1024 * 1024;

/// ページとして開かれる HTML が上限を超えたときの応答。
pub(super) fn too_large() -> Response {
    (
        StatusCode::BAD_GATEWAY,
        "kemi: the page is too large to relay",
    )
        .into_response()
}

/// 開発サーバにつながるか、配るファイルがあるか（待っているページが確かめる）。
pub(super) async fn reachable(state: &LiveState, path: Option<&str>) -> bool {
    match &state.target {
        LiveTarget::Url { authority, .. } => TcpStream::connect(authority.as_str()).await.is_ok(),
        LiveTarget::File { .. } => path
            .and_then(|path| path.strip_prefix('/'))
            .is_some_and(|relative| super::files::resolve(&state.info.root, relative).is_some()),
    }
}

pub(super) async fn forward(
    state: &LiveState,
    authority: &str,
    display: &str,
    request: Request,
) -> Response {
    let host = request_host(request.headers());
    let page = opened_as_page(request.headers());
    let stream = match TcpStream::connect(authority).await {
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
    let nonce = nonce();
    let rewrote = rewrite_headers(&mut parts.headers, state, &host, authority, &nonce);
    let html = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/html"));
    if html {
        // 同じ URL でも行き先によって中身が変わるので、キャッシュに区別させる。
        parts.headers.append(
            header::VARY,
            HeaderValue::from_static("sec-fetch-dest, upgrade-insecure-requests"),
        );
    }
    if !html || !page {
        return Response::from_parts(parts, Body::new(body));
    }
    let bytes = match axum::body::to_bytes(Body::new(body), HTML_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => return too_large(),
    };
    let injected = rewrite::inject_script(
        &bytes,
        &script_tag(state, &host, &rewrote, true, Some(&nonce)),
    );
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.remove(header::TRANSFER_ENCODING);
    Response::from_parts(parts, Body::from(injected))
}

/// ページとして開かれる要求か（文書としての移動か iframe への読み込み）。ページの
/// スクリプトが fetch などで取る要求はブラウザが `Sec-Fetch-Dest: empty` を付ける。
/// ブラウザは `Sec-Fetch-Dest` を安全なオリジンにだけ送るので、`--bind` で LAN のアドレスから
/// 平文の HTTP で開くと付かない。そのときは移動にだけ付く `Upgrade-Insecure-Requests: 1` で見分ける。
pub(super) fn opened_as_page(headers: &HeaderMap) -> bool {
    match headers.get("sec-fetch-dest") {
        Some(dest) => matches!(dest.as_bytes(), b"document" | b"iframe" | b"frame"),
        None => headers
            .get("upgrade-insecure-requests")
            .is_some_and(|value| value.as_bytes() == b"1"),
    }
}

/// `X-Frame-Options` を外し、CSP を書き換え、開発サーバ自身への転送先を中継の中に向ける。
/// 書き換えた見出しの名前を返す（ページに書き換えたことを出すため）。
fn rewrite_headers(
    headers: &mut HeaderMap,
    state: &LiveState,
    host: &str,
    authority: &str,
    nonce: &str,
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
            let rewritten = rewrite::rewrite_csp(&policy, &ancestors, &script, nonce);
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
pub(super) fn request_host(headers: &HeaderMap) -> String {
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

/// 応答ごとの、推測できない nonce（CSP で差し込むスクリプトだけを通すため）。
fn nonce() -> String {
    let mut bytes = [0u8; 16];
    // 取れなければ nonce を使う CSP のページでスクリプトが止まるだけで、中継は続けられる。
    if getrandom::fill(&mut bytes).is_err() {
        return String::new();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 差し込むスクリプトの要素。`data-kemi-watch` はファイルの保存で読み込み直すページの印。
/// `nonce` は、ページの CSP に足したものと同じ値。
pub(super) fn script_tag(
    state: &LiveState,
    host: &str,
    rewrote: &[&str],
    reachable: bool,
    nonce: Option<&str>,
) -> String {
    let watch = matches!(state.target, LiveTarget::File { .. });
    let nonce = nonce.map_or_else(String::new, |nonce| format!(r#" nonce="{nonce}""#));
    format!(
        r#"<script src="/__kemi/page.js"{nonce} data-kemi-review="{}" data-kemi-rewrote="{}" data-kemi-reachable="{reachable}" data-kemi-watch="{watch}"></script>"#,
        review_origin(state, host),
        rewrote.join(" "),
    )
}

pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 開発サーバにつながらないときのページ（R-PAGE-MODE）。つながるまで確かめ続け、
/// つながったら読み込み直す。
fn waiting_page(state: &LiveState, host: &str, display: &str) -> Response {
    waiting_response(
        StatusCode::BAD_GATEWAY,
        state,
        host,
        &format!(
            "kemi cannot reach {}. Waiting for the dev server to start&hellip;",
            escape(display)
        ),
    )
}

/// つながるまで（ファイルが戻るまで）待ち、戻ったら読み込み直すページ。`message` は HTML。
pub(super) fn waiting_response(
    status: StatusCode,
    state: &LiveState,
    host: &str,
    message: &str,
) -> Response {
    let html = format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Waiting for the page</title>{}
<style>body {{ font: 15px/1.5 system-ui, sans-serif; margin: 2rem; color: #444; }}</style>
</head><body>
<p id="kemi-unreachable">{message}</p>
<script>
setInterval(async () => {{
  try {{
    const response = await fetch('/__kemi/alive?path=' + encodeURIComponent(location.pathname), {{ cache: 'no-store' }});
    if (response.ok) location.reload();
  }} catch {{}}
}}, 500);
</script>
</body></html>
"#,
        script_tag(state, host, &[], false, None)
    );
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}
