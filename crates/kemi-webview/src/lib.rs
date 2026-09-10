//! 配る資産（R-WS）。release ではバイナリに埋め込み、debug ではディスクから読む。

use std::borrow::Cow;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web"]
struct WebAssets;

pub fn get(path: &str) -> Option<(Cow<'static, [u8]>, &'static str)> {
    let file = WebAssets::get(path)?;
    Some((file.data, mime_for(path)))
}

pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
