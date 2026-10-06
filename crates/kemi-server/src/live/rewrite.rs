//! 中継の応答と要求の書き換え（R-PAGE-PROXY）と、モックの参照の書き換え（R-PAGE-MOCK）。
//! 文字列だけを扱う純粋な関数。

/// HTML に差し込むスクリプトの要素を入れる。`<head>` の直後、無ければ `<body>` の前、
/// それも無ければ doctype の後ろ。doctype より前に入れると互換モードで描かれるため。
/// バイト列のまま扱う。探すのは ASCII のタグだけなので、ページの文字コードを問わない。
pub fn inject_script(html: &[u8], tag: &str) -> Vec<u8> {
    let lower = html.to_ascii_lowercase();
    let after_open = |start: usize| {
        lower[start..]
            .iter()
            .position(|&byte| byte == b'>')
            .map(|end| start + end + 1)
    };
    let at = find_tag(&lower, b"head")
        .and_then(after_open)
        .or_else(|| find_tag(&lower, b"body"))
        .or_else(|| {
            lower
                .starts_with(b"<!doctype")
                .then(|| after_open(0))
                .flatten()
        })
        .unwrap_or(0);
    [&html[..at], tag.as_bytes(), &html[at..]].concat()
}

/// `<name` で始まり、名前がそこで切れる開始タグの位置（`<header>` を `<head>` にしない）。
fn find_tag(lower: &[u8], name: &[u8]) -> Option<usize> {
    let needle = [b"<", name].concat();
    let mut from = 0;
    while let Some(found) = lower[from..]
        .windows(needle.len())
        .position(|window| window == needle)
    {
        let start = from + found;
        let next = lower.get(start + needle.len());
        if next.is_some_and(|&byte| byte == b'>' || byte.is_ascii_whitespace()) {
            return Some(start);
        }
        from = start + needle.len();
    }
    None
}

/// CSP の `frame-ancestors` をレビュー画面のオリジンだけにし、スクリプトを縛る指示
/// （`script-src-elem` と `script-src`）に差し込むスクリプトだけを足す。ほかの指示は変えない。
/// どちらも無く `default-src` があるときは、`default-src` の値に足した `script-src` を作る。
/// nonce・ハッシュ・'strict-dynamic' を使う指示には、URL ではなく差し込むスクリプトの
/// `nonce` を足す。'strict-dynamic' があると URL での許可は無視されるため。
pub fn rewrite_csp(policy: &str, ancestors: &str, script_url: &str, nonce: &str) -> String {
    let allow = |sources: &str| {
        let lower = sources.to_ascii_lowercase();
        if [
            "'nonce-",
            "'sha256-",
            "'sha384-",
            "'sha512-",
            "'strict-dynamic'",
        ]
        .iter()
        .any(|keyword| lower.contains(keyword))
        {
            format!("'nonce-{nonce}'")
        } else {
            script_url.to_string()
        }
    };
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
            "script-src" | "script-src-elem" => {
                has_script_src = true;
                directives.push(format!("{directive} {}", allow(directive)));
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
        directives.push(format!("script-src {sources} {}", allow(&sources)));
    }
    directives.join("; ")
}

/// URL を持つ属性。`srcset` は候補ごとに URL を持つ。
const URL_ATTRIBUTES: [&[u8]; 5] = [b"src", b"href", b"poster", b"action", b"srcset"];

/// モックの HTML の中の、根からの参照（`/style.css`）をモックの URL（`prefix`）の下に向ける
/// （R-PAGE-MOCK）。モックはレビュー画面のオリジンの `/m/<値>/` の下で配るので、根からの
/// 参照はそのままではモックの範囲に届かない。タグの属性と、CSS の `url()`・`@import` を
/// 書き換える。スクリプトが組み立てる URL は書き換えられない。
pub fn root_relative_html(html: &[u8], prefix: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(html.len());
    let mut index = 0;
    while index < html.len() {
        if html[index] == b'<' && html.get(index + 1).is_some_and(u8::is_ascii_alphabetic) {
            let end = tag_end(html, index);
            rewrite_tag(&html[index..end], prefix, &mut out);
            index = end;
        } else {
            out.push(html[index]);
            index += 1;
        }
    }
    root_relative_css(&out, prefix)
}

/// 開始タグの終わり（`>` の次）。引用符の中の `>` は数えない。
fn tag_end(html: &[u8], start: usize) -> usize {
    let mut quote = None;
    for (offset, &byte) in html[start..].iter().enumerate() {
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return start + offset + 1,
            None => {}
        }
    }
    html.len()
}

/// 1 つのタグの属性を読み、URL を持つ属性の値だけを書き換えて `out` に足す。
fn rewrite_tag(tag: &[u8], prefix: &str, out: &mut Vec<u8>) {
    let len = tag.len();
    let ends_name = |byte: u8| byte.is_ascii_whitespace() || b"=>/".contains(&byte);
    let mut index = 1;
    while index < len && !ends_name(tag[index]) {
        index += 1;
    }
    out.extend_from_slice(&tag[..index]);
    loop {
        while index < len && (tag[index].is_ascii_whitespace() || tag[index] == b'/') {
            out.push(tag[index]);
            index += 1;
        }
        if index >= len || tag[index] == b'>' {
            break;
        }
        let name_start = index;
        while index < len && !ends_name(tag[index]) {
            index += 1;
        }
        if index == name_start {
            // 名前の無い `=` は飛ばす。
            out.push(tag[index]);
            index += 1;
            continue;
        }
        let name = tag[name_start..index].to_ascii_lowercase();
        out.extend_from_slice(&tag[name_start..index]);
        let mut next = index;
        while next < len && tag[next].is_ascii_whitespace() {
            next += 1;
        }
        if next >= len || tag[next] != b'=' {
            continue;
        }
        next += 1;
        while next < len && tag[next].is_ascii_whitespace() {
            next += 1;
        }
        let quote = tag
            .get(next)
            .copied()
            .filter(|&byte| byte == b'"' || byte == b'\'');
        if quote.is_some() {
            next += 1;
        }
        out.extend_from_slice(&tag[index..next]);
        let value_end = match quote {
            Some(quote) => tag[next..]
                .iter()
                .position(|&byte| byte == quote)
                .map_or(len, |found| next + found),
            None => tag[next..]
                .iter()
                .position(|&byte| byte.is_ascii_whitespace() || byte == b'>')
                .map_or(len, |found| next + found),
        };
        let value = &tag[next..value_end];
        match name.as_slice() {
            b"srcset" => rewrite_srcset(value, prefix, out),
            name if URL_ATTRIBUTES.contains(&name) => push_url(value, prefix, out),
            _ => out.extend_from_slice(value),
        }
        index = value_end;
        if quote.is_some() && index < len {
            out.push(tag[index]);
            index += 1;
        }
    }
    out.extend_from_slice(&tag[index..]);
}

/// 根からの参照（`//` で始まるものを除く）なら、前に `prefix` を付けて足す。
fn push_url(url: &[u8], prefix: &str, out: &mut Vec<u8>) {
    if url.starts_with(b"/") && !url.starts_with(b"//") {
        out.extend_from_slice(prefix.as_bytes());
    }
    out.extend_from_slice(url);
}

/// `srcset` の候補ごとに URL を書き換える。data: の URL はコンマを含むので、あればそのまま。
fn rewrite_srcset(value: &[u8], prefix: &str, out: &mut Vec<u8>) {
    if value
        .to_ascii_lowercase()
        .windows(5)
        .any(|window| window == b"data:")
    {
        out.extend_from_slice(value);
        return;
    }
    for (position, candidate) in value.split(|&byte| byte == b',').enumerate() {
        if position > 0 {
            out.push(b',');
        }
        let blank = candidate
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count();
        out.extend_from_slice(&candidate[..blank]);
        push_url(&candidate[blank..], prefix, out);
    }
}

/// モックの CSS の、根からの `url()` と `@import` をモックの URL（`prefix`）の下に向ける。
pub fn root_relative_css(css: &[u8], prefix: &str) -> Vec<u8> {
    let lower = css.to_ascii_lowercase();
    let mut out = Vec::with_capacity(css.len());
    let mut index = 0;
    while index < css.len() {
        let opened = [b"url(".as_slice(), b"@import".as_slice()]
            .into_iter()
            .find(|keyword| lower[index..].starts_with(keyword));
        let Some(keyword) = opened else {
            out.push(css[index]);
            index += 1;
            continue;
        };
        let mut next = index + keyword.len();
        while next < css.len() && (css[next].is_ascii_whitespace() || b"\"'".contains(&css[next])) {
            next += 1;
        }
        out.extend_from_slice(&css[index..next]);
        if css[next..].starts_with(b"/") && !css[next..].starts_with(b"//") {
            out.extend_from_slice(prefix.as_bytes());
        }
        index = next;
    }
    out
}

/// Cookie ヘッダから中継用の cookie を取り出す。返すのはその値と、残りの cookie
/// （無ければ None）。開発サーバへは残りだけを送る。ヘッダは文字列にせずバイト列のまま
/// 扱う。ほかの cookie が ASCII の外のバイトを持っていても、中継の cookie を読み、
/// 残りを変えずに送るため。
pub fn take_cookie(header: &[u8], name: &str) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let mut value = None;
    let mut rest: Vec<&[u8]> = Vec::new();
    for pair in header
        .split(|&byte| byte == b';')
        .map(<[u8]>::trim_ascii)
        .filter(|pair| !pair.is_empty())
    {
        match pair.iter().position(|&byte| byte == b'=') {
            Some(at) if pair[..at].trim_ascii() == name.as_bytes() => {
                value = Some(pair[at + 1..].trim_ascii().to_vec());
            }
            _ => rest.push(pair),
        }
    }
    let rest = (!rest.is_empty()).then(|| rest.join(&b"; "[..]));
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

    fn inject(html: &str) -> String {
        String::from_utf8(inject_script(html.as_bytes(), TAG)).unwrap()
    }

    #[test]
    fn a_page_in_another_encoding_keeps_its_bytes() {
        // Shift_JIS の「日本」と、UTF-8 として読めないバイト。
        let mut html = b"<html><head><meta charset=\"shift_jis\">".to_vec();
        html.extend_from_slice(&[0x93, 0xfa, 0x96, 0x7b, 0xff]);
        html.extend_from_slice(b"</head><body></body></html>");

        let injected = inject_script(&html, TAG);

        let mut expected = b"<html><head>".to_vec();
        expected.extend_from_slice(TAG.as_bytes());
        expected.extend_from_slice(&html[b"<html><head>".len()..]);
        assert_eq!(injected, expected);
    }

    #[test]
    fn the_script_goes_right_after_the_head_tag() {
        let html =
            "<!doctype html><html><HEAD lang=\"en\"><title>x</title></head><body></body></html>";
        assert_eq!(
            inject(html),
            format!(
                "<!doctype html><html><HEAD lang=\"en\">{TAG}<title>x</title></head><body></body></html>"
            )
        );
    }

    #[test]
    fn without_a_head_the_script_goes_before_the_body() {
        let html = "<!doctype html><body><p>x</p></body>";
        assert_eq!(
            inject(html),
            format!("<!doctype html>{TAG}<body><p>x</p></body>")
        );
    }

    #[test]
    fn without_head_or_body_the_script_follows_the_doctype() {
        assert_eq!(
            inject("<!DOCTYPE html><p>x</p>"),
            format!("<!DOCTYPE html>{TAG}<p>x</p>")
        );
        assert_eq!(inject("<p>x</p>"), format!("{TAG}<p>x</p>"));
    }

    #[test]
    fn a_header_named_like_head_is_not_taken_for_the_head() {
        let html = "<header>x</header><body></body>";
        assert_eq!(
            inject(html),
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
                "http://127.0.0.1:5000/__kemi/page.js",
                NONCE
            ),
            "default-src 'self'; frame-ancestors http://127.0.0.1:4000; img-src *; script-src 'self' http://127.0.0.1:5000/__kemi/page.js"
        );
    }

    #[test]
    fn script_src_gets_only_the_injected_script_added() {
        let policy = "script-src 'self' 'unsafe-inline'; style-src 'self'";
        assert_eq!(
            rewrite_csp(policy, "http://h:1", "http://h:2/__kemi/page.js", NONCE),
            "script-src 'self' 'unsafe-inline' http://h:2/__kemi/page.js; style-src 'self'"
        );
    }

    const NONCE: &str = "n0nce";

    fn csp(policy: &str) -> String {
        rewrite_csp(policy, "http://h:1", "http://h:2/__kemi/page.js", NONCE)
    }

    #[test]
    fn script_src_elem_also_gets_the_injected_script_added() {
        assert_eq!(
            csp("script-src-elem 'self'; script-src 'self'"),
            "script-src-elem 'self' http://h:2/__kemi/page.js; script-src 'self' http://h:2/__kemi/page.js"
        );
        assert_eq!(
            csp("default-src 'self'; script-src-elem 'self'"),
            "default-src 'self'; script-src-elem 'self' http://h:2/__kemi/page.js"
        );
    }

    #[test]
    fn a_policy_with_nonces_hashes_or_strict_dynamic_gets_only_the_injected_nonce() {
        assert_eq!(
            csp("script-src 'nonce-page' 'strict-dynamic'"),
            "script-src 'nonce-page' 'strict-dynamic' 'nonce-n0nce'"
        );
        assert_eq!(
            csp("script-src-elem 'sha256-abc='"),
            "script-src-elem 'sha256-abc=' 'nonce-n0nce'"
        );
        assert_eq!(
            csp("default-src 'self' 'nonce-page'"),
            "default-src 'self' 'nonce-page'; script-src 'self' 'nonce-page' 'nonce-n0nce'"
        );
    }

    #[test]
    fn a_policy_without_script_or_default_src_keeps_scripts_unrestricted() {
        assert_eq!(
            rewrite_csp(
                "img-src 'self'",
                "http://h:1",
                "http://h:2/__kemi/page.js",
                NONCE
            ),
            "img-src 'self'"
        );
    }

    const MOCK: &str = "/m/secret";

    fn html(text: &str) -> String {
        String::from_utf8(root_relative_html(text.as_bytes(), MOCK)).unwrap()
    }

    fn css(text: &str) -> String {
        String::from_utf8(root_relative_css(text.as_bytes(), MOCK)).unwrap()
    }

    #[test]
    fn root_relative_references_in_a_mock_point_under_the_mock_url() {
        assert_eq!(
            html(
                r#"<link rel="stylesheet" href="/style.css"><img src='/a.png'><a href=/next.html>x</a>"#
            ),
            r#"<link rel="stylesheet" href="/m/secret/style.css"><img src='/m/secret/a.png'><a href=/m/secret/next.html>x</a>"#
        );
        assert_eq!(
            html(r#"<IMG SRC = "/a.png" srcset="/a.png 1x, /b.png 2x"><video poster="/p.png">"#),
            r#"<IMG SRC = "/m/secret/a.png" srcset="/m/secret/a.png 1x, /m/secret/b.png 2x"><video poster="/m/secret/p.png">"#
        );
    }

    #[test]
    fn other_references_in_a_mock_are_kept() {
        let kept = r##"<img src="a.png"><img src="//cdn.example/a.png"><a href="https://example.com/">x</a><a href="#top">y</a><p>src="/text"</p>"##;
        assert_eq!(html(kept), kept);
    }

    #[test]
    fn root_relative_urls_in_mock_css_point_under_the_mock_url() {
        assert_eq!(
            css(
                r#"@import "/base.css"; a { background: URL( '/a.png' ) } b { background: url(/b.png) } c { background: url(//cdn/c.png) }"#
            ),
            r#"@import "/m/secret/base.css"; a { background: URL( '/m/secret/a.png' ) } b { background: url(/m/secret/b.png) } c { background: url(//cdn/c.png) }"#
        );
        assert_eq!(
            html(
                r#"<style>p { background: url("/a.png") }</style><p style="background: url(/b.png)">x</p>"#
            ),
            r#"<style>p { background: url("/m/secret/a.png") }</style><p style="background: url(/m/secret/b.png)">x</p>"#
        );
    }

    #[test]
    fn the_relay_cookie_is_taken_out_and_the_rest_is_kept() {
        assert_eq!(
            take_cookie(b"a=1; kemi_live_5000=secret; b=2", "kemi_live_5000"),
            (Some(b"secret".to_vec()), Some(b"a=1; b=2".to_vec()))
        );
        assert_eq!(
            take_cookie(b"kemi_live_5000=secret", "kemi_live_5000"),
            (Some(b"secret".to_vec()), None)
        );
        assert_eq!(
            take_cookie(b"a=1", "kemi_live_5000"),
            (None, Some(b"a=1".to_vec()))
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
