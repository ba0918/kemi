//! 配る資産（R-WS）。release ではバイナリに埋め込み、debug ではディスクから読む。

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]

use std::borrow::Cow;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web"]
#[exclude = "**/*.test.js"]
#[exclude = "package.json"]
#[exclude = "tsconfig.json"]
struct WebAssets;

pub fn get(path: &str) -> Option<(Cow<'static, [u8]>, &'static str)> {
    let file = WebAssets::get(path)?;
    Some((file.data, mime_for(path)))
}

fn mime_for(path: &str) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_assets_and_license_notices_remain_available() {
        for path in [
            "index.html",
            "assets/app.js",
            "assets/model.js",
            "live/page.js",
            "assets/diagrams/LICENSE",
        ] {
            assert!(get(path).is_some(), "missing runtime asset: {path}");
        }
    }

    #[test]
    fn development_tests_and_configuration_are_not_published_as_assets() {
        for path in [
            "assets/model.test.js",
            "assets/explanation-model.test.js",
            "assets/live-diff.test.js",
            "assets/live-model.test.js",
            "package.json",
            "tsconfig.json",
        ] {
            assert!(
                get(path).is_none(),
                "development file was published: {path}"
            );
        }
        assert!(WebAssets::iter().all(|path| !path.ends_with(".test.js")
            && path != "package.json"
            && path != "tsconfig.json"));
    }
}
