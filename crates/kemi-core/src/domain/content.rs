//! 内容の正規化（CRLF/LF、最終行の改行）

/// CRLF と CR を LF に揃える。改行コードの変更だけを差分として見せないため。
pub fn normalize(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

/// 表示用の行に分割する。最終行の改行の有無は表示しない。
pub fn lines(content: &str) -> Vec<String> {
    let normalized = normalize(content);
    if normalized.is_empty() {
        return Vec::new();
    }
    normalized
        .strip_suffix('\n')
        .unwrap_or(&normalized)
        .split('\n')
        .map(str::to_string)
        .collect()
}

/// 片側がこの行数を超えたら、ハイライトと由来を自動では求めない（R-VIEW, R-ORIGIN）。
pub const AUTO_MAX_LINES: usize = 10_000;
/// 片側がこのバイト数を超えたら、ハイライトと由来を自動では求めない。
pub const AUTO_MAX_BYTES: usize = 1_048_576;

/// 自動で行ごとの計算（ハイライト・由来）をしてよい規模か。片側が行数かバイト数の
/// 上限を超えたら false。無い側は数えない。
pub fn within_auto_limit(old: Option<&str>, new: Option<&str>) -> bool {
    let within =
        |text: &str| text.len() <= AUTO_MAX_BYTES && text.lines().count() <= AUTO_MAX_LINES;
    old.is_none_or(within) && new.is_none_or(within)
}

/// バイナリらしさの判定（D6）。NUL を含むか、UTF-8 として読めなければバイナリ。
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_converts_crlf_and_cr_to_lf() {
        assert_eq!(normalize("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn normalize_keeps_lone_lf() {
        assert_eq!(normalize("a\nb"), "a\nb");
    }

    #[test]
    fn normalize_lines_drops_final_newline_marker() {
        assert_eq!(lines("a\nb\n"), vec!["a", "b"]);
        assert_eq!(lines("a\nb"), vec!["a", "b"]);
    }

    #[test]
    fn normalize_lines_treats_crlf_like_lf() {
        assert_eq!(lines("a\r\nb\r\n"), lines("a\nb\n"));
    }

    #[test]
    fn normalize_lines_of_empty_content_is_empty() {
        assert_eq!(lines(""), Vec::<String>::new());
    }

    #[test]
    fn normalize_lines_keeps_single_blank_line() {
        assert_eq!(lines("\n"), vec![""]);
    }
}
