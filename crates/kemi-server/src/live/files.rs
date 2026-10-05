//! `--live <ファイル>` の配信（R-PAGE-MODE）。配れる範囲のファイルを中継のポートで配り、
//! HTML には中継と同じスクリプトを差し込む。範囲の外・`.git` の中・シンボリックリンクの
//! 実体が外のものは返さない。

use std::path::{Path, PathBuf};

use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use kemi_core::domain::live::{is_html, served_path};

use super::{LiveState, relay, rewrite};

/// 要求のパスを、配れる範囲の根からの相対パスにする。`..` を含むものは断る（パーセント
/// 符号化されていても）。
fn requested_path(raw: &str) -> Option<String> {
    let decoded = percent_decode(raw)?;
    let mut parts = Vec::new();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            segment if segment.contains('\\') || segment.contains('\0') => return None,
            segment => parts.push(segment),
        }
    }
    Some(parts.join("/"))
}

pub(super) fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = raw.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// 配れる範囲の中の実在するファイルの実体の場所。範囲の外と `.git` の中は None。
pub(super) fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let real = std::fs::canonicalize(root.join(relative)).ok()?;
    served_path(root, &real).ok()?;
    real.is_file().then_some(real)
}

pub(super) async fn serve(state: &LiveState, root: &Path, request: Request) -> Response {
    let not_found = || (StatusCode::NOT_FOUND, "kemi: not found").into_response();
    let Some(relative) = requested_path(request.uri().path()) else {
        return not_found();
    };
    let Some(real) = resolve(root, &relative) else {
        if is_html(&relative) {
            return waiting_for_the_file(state, &relay::request_host(request.headers()), &relative);
        }
        return not_found();
    };
    let reading = real.clone();
    let bytes = match tokio::task::spawn_blocking(move || std::fs::read(reading)).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) | Err(_) => return not_found(),
    };
    if let Some(watch) = &state.served {
        watch.served(real.clone());
    }
    let mime = mime_for(&relative);
    let headers = [
        (header::CONTENT_TYPE, mime),
        (header::CACHE_CONTROL, "no-store"),
    ];
    if is_html(&relative) {
        let host = relay::request_host(request.headers());
        let text = String::from_utf8_lossy(&bytes);
        let tag = relay::script_tag(state, &host, &[], true);
        return (headers, rewrite::inject_script(&text, &tag)).into_response();
    }
    (headers, bytes).into_response()
}

/// 配るファイルが無いときのページ（R-PAGE-SESSION）。ファイルが戻るまで確かめ続ける。
fn waiting_for_the_file(state: &LiveState, host: &str, relative: &str) -> Response {
    let shown = format!("/{relative}");
    relay::waiting_response(
        StatusCode::NOT_FOUND,
        state,
        host,
        &format!(
            "kemi cannot read {}. Waiting for the file&hellip;",
            relay::escape(&shown)
        ),
    )
}

fn mime_for(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_path_with_parent_segments_is_refused_even_when_encoded() {
        assert_eq!(
            requested_path("/docs/./page.html").as_deref(),
            Some("docs/page.html")
        );
        assert_eq!(requested_path("/a%20b.html").as_deref(), Some("a b.html"));
        assert_eq!(requested_path("/../x"), None);
        assert_eq!(requested_path("/%2e%2e/x"), None);
        assert_eq!(requested_path("/a/..%2fx"), None);
        assert_eq!(requested_path("/a%5cb"), None);
        assert_eq!(requested_path("/bad%zz"), None);
    }
}
