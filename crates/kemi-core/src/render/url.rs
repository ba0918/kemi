//! リンクの `href` と画像の `src` の判定、相対パス画像の正規化。

/// 判定の結果。前後の空白と制御文字を除いてから決める。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LinkTarget {
    /// `http` / `https`（大小文字を区別しない）。
    External(String),
    /// `#` 始まり。
    Anchor(String),
    /// `//` で始まらない相対パス。
    Relative(String),
    /// それ以外（`javascript:` / `data:` / `//host` など）。
    Invalid,
}

pub(super) fn classify(raw: &str) -> LinkTarget {
    let trimmed = raw.trim_matches(|c: char| c.is_whitespace() || c.is_control());
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return LinkTarget::External(trimmed.to_string());
    }
    if trimmed.starts_with('#') {
        return LinkTarget::Anchor(trimmed.to_string());
    }
    if trimmed.starts_with("//") || has_scheme(trimmed) {
        return LinkTarget::Invalid;
    }
    LinkTarget::Relative(trimmed.to_string())
}

/// `scheme:` で始まるか。scheme は英字で始まり、英数字と `+` `-` `.` だけからなる。
fn has_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme = &value[..colon];
    let mut chars = scheme.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

pub(super) fn has_image_extension(path: &str) -> bool {
    matches!(super::target_of(path), Some(super::Target::Image { .. }))
}

/// 相対パスを Markdown のディレクトリ基準で正規化する。先頭の `/` はリポジトリの根。
/// 根の外に出るなら None。クエリと断片は落とす。
pub(super) fn resolve_relative(markdown_path: &str, relative: &str) -> Option<String> {
    let relative = relative
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_whitespace() || c.is_control());
    let mut segments: Vec<&str> = Vec::new();
    if !relative.starts_with('/') {
        if let Some((directory, _)) = markdown_path.rsplit_once('/') {
            segments.extend(directory.split('/'));
        }
    }
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_other_than_http_are_invalid() {
        assert_eq!(classify(" javascript:alert(1) "), LinkTarget::Invalid);
        assert_eq!(classify("data:text/html,x"), LinkTarget::Invalid);
        assert_eq!(classify("//host/x"), LinkTarget::Invalid);
        assert_eq!(classify("mailto:a@b"), LinkTarget::Invalid);
        assert_eq!(
            classify("\tHTTPS://x.y/z\n"),
            LinkTarget::External("HTTPS://x.y/z".to_string())
        );
        assert_eq!(classify("#top"), LinkTarget::Anchor("#top".to_string()));
        assert_eq!(
            classify("docs/a.md"),
            LinkTarget::Relative("docs/a.md".to_string())
        );
        assert_eq!(classify("c:/x"), LinkTarget::Invalid);
    }

    #[test]
    fn relative_paths_resolve_against_the_markdown_directory() {
        assert_eq!(
            resolve_relative("docs/a.md", "img/x.png"),
            Some("docs/img/x.png".to_string())
        );
        assert_eq!(
            resolve_relative("docs/a.md", "../y.png"),
            Some("y.png".to_string())
        );
        assert_eq!(resolve_relative("docs/a.md", "../../y.png"), None);
        assert_eq!(
            resolve_relative("docs/a.md", "/z.png?x=1"),
            Some("z.png".to_string())
        );
        assert_eq!(
            resolve_relative("README.md", "./a/../b.png"),
            Some("b.png".to_string())
        );
    }

    #[test]
    fn image_extension_is_case_insensitive() {
        assert!(has_image_extension("a/B.PNG"));
        assert!(has_image_extension("x.webp"));
        assert!(!has_image_extension("x.txt"));
        assert!(!has_image_extension(".png"));
        assert!(!has_image_extension("png"));
    }
}
