//! 中継の応答と要求の書き換え（R-PAGE-PROXY）。文字列だけを扱う純粋な関数。

/// HTML に差し込むスクリプトの要素を入れる。`<head>` の直後、無ければ `<body>` の前、
/// それも無ければ doctype の後ろ。doctype より前に入れると互換モードで描かれるため。
pub fn inject_script(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let at = find_tag(&lower, "head")
        .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
        .or_else(|| find_tag(&lower, "body"))
        .or_else(|| {
            lower
                .starts_with("<!doctype")
                .then(|| lower.find('>').map(|end| end + 1))
                .flatten()
        })
        .unwrap_or(0);
    format!("{}{tag}{}", &html[..at], &html[at..])
}

/// `<name` で始まり、名前がそこで切れる開始タグの位置（`<header>` を `<head>` にしない）。
fn find_tag(lower: &str, name: &str) -> Option<usize> {
    let needle = format!("<{name}");
    let mut from = 0;
    while let Some(found) = lower[from..].find(&needle) {
        let start = from + found;
        let next = lower[start + needle.len()..].chars().next();
        if next.is_some_and(|character| character == '>' || character.is_ascii_whitespace()) {
            return Some(start);
        }
        from = start + needle.len();
    }
    None
}

/// CSP の `frame-ancestors` をレビュー画面のオリジンだけにし、スクリプトを縛る指示に
/// 差し込むスクリプトだけを足す。ほかの指示は変えない。`script-src` が無く
/// `default-src` があるときは、`default-src` の値に足した `script-src` を作る。
pub fn rewrite_csp(policy: &str, ancestors: &str, script_source: &str) -> String {
    let mut directives: Vec<String> = Vec::new();
    let mut default_src = None;
    let mut has_script_src = false;
    for directive in policy.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let name = directive
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match name.as_str() {
            "frame-ancestors" => directives.push(format!("frame-ancestors {ancestors}")),
            "script-src" => {
                has_script_src = true;
                directives.push(format!("{directive} {script_source}"));
            }
            _ => {
                if name == "default-src" {
                    default_src = Some(directive["default-src".len()..].trim().to_string());
                }
                directives.push(directive.to_string());
            }
        }
    }
    if !has_script_src && let Some(sources) = default_src {
        directives.push(format!("script-src {sources} {script_source}"));
    }
    directives.join("; ")
}

/// Cookie ヘッダから中継用の cookie を取り出す。返すのはその値と、残りの cookie
/// （無ければ None）。開発サーバへは残りだけを送る。
pub fn take_cookie(header: &str, name: &str) -> (Option<String>, Option<String>) {
    let mut value = None;
    let mut rest = Vec::new();
    for pair in header
        .split(';')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
    {
        match pair.split_once('=') {
            Some((key, found)) if key.trim() == name => value = Some(found.trim().to_string()),
            _ => rest.push(pair),
        }
    }
    let rest = (!rest.is_empty()).then(|| rest.join("; "));
    (value, rest)
}

/// 開発サーバ自身を指す転送先を、中継の中の相対 URL にする。ほかは書き換えない。
pub fn relative_location(location: &str, upstream_authority: &str) -> Option<String> {
    let rest = location.strip_prefix("http://")?;
    let path = rest.strip_prefix(upstream_authority)?;
    if path.is_empty() {
        return Some("/".to_string());
    }
    path.starts_with(['/', '?']).then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAG: &str = r#"<script src="/__kemi/page.js"></script>"#;

    #[test]
    fn the_script_goes_right_after_the_head_tag() {
        let html =
            "<!doctype html><html><HEAD lang=\"en\"><title>x</title></head><body></body></html>";
        assert_eq!(
            inject_script(html, TAG),
            format!(
                "<!doctype html><html><HEAD lang=\"en\">{TAG}<title>x</title></head><body></body></html>"
            )
        );
    }

    #[test]
    fn without_a_head_the_script_goes_before_the_body() {
        let html = "<!doctype html><body><p>x</p></body>";
        assert_eq!(
            inject_script(html, TAG),
            format!("<!doctype html>{TAG}<body><p>x</p></body>")
        );
    }

    #[test]
    fn without_head_or_body_the_script_follows_the_doctype() {
        assert_eq!(
            inject_script("<!DOCTYPE html><p>x</p>", TAG),
            format!("<!DOCTYPE html>{TAG}<p>x</p>")
        );
        assert_eq!(inject_script("<p>x</p>", TAG), format!("{TAG}<p>x</p>"));
    }

    #[test]
    fn a_header_named_like_head_is_not_taken_for_the_head() {
        let html = "<header>x</header><body></body>";
        assert_eq!(
            inject_script(html, TAG),
            format!("<header>x</header>{TAG}<body></body>")
        );
    }

    #[test]
    fn frame_ancestors_becomes_the_review_origin_only() {
        let policy = "default-src 'self'; frame-ancestors 'none'; img-src *";
        assert_eq!(
            rewrite_csp(
                policy,
                "http://127.0.0.1:4000",
                "http://127.0.0.1:5000/__kemi/page.js"
            ),
            "default-src 'self'; frame-ancestors http://127.0.0.1:4000; img-src *; script-src 'self' http://127.0.0.1:5000/__kemi/page.js"
        );
    }

    #[test]
    fn script_src_gets_only_the_injected_script_added() {
        let policy = "script-src 'self' 'unsafe-inline'; style-src 'self'";
        assert_eq!(
            rewrite_csp(policy, "http://h:1", "http://h:2/__kemi/page.js"),
            "script-src 'self' 'unsafe-inline' http://h:2/__kemi/page.js; style-src 'self'"
        );
    }

    #[test]
    fn a_policy_without_script_or_default_src_keeps_scripts_unrestricted() {
        assert_eq!(
            rewrite_csp("img-src 'self'", "http://h:1", "http://h:2/__kemi/page.js"),
            "img-src 'self'"
        );
    }

    #[test]
    fn the_relay_cookie_is_taken_out_and_the_rest_is_kept() {
        assert_eq!(
            take_cookie("a=1; kemi_live_5000=secret; b=2", "kemi_live_5000"),
            (Some("secret".to_string()), Some("a=1; b=2".to_string()))
        );
        assert_eq!(
            take_cookie("kemi_live_5000=secret", "kemi_live_5000"),
            (Some("secret".to_string()), None)
        );
        assert_eq!(
            take_cookie("a=1", "kemi_live_5000"),
            (None, Some("a=1".to_string()))
        );
    }

    #[test]
    fn a_redirect_to_the_dev_server_stays_on_the_relay() {
        assert_eq!(
            relative_location("http://127.0.0.1:5173/next?x=1", "127.0.0.1:5173").as_deref(),
            Some("/next?x=1")
        );
        assert_eq!(
            relative_location("http://example.com/", "127.0.0.1:5173"),
            None
        );
        assert_eq!(relative_location("/already", "127.0.0.1:5173"), None);
    }
}
