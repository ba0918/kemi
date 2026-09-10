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
