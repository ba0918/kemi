//! バイトオフセットから行番号への写像と、front matter の切り離し。

/// 正規化済み（LF だけ）のテキストの各行の開始オフセット。
pub(super) struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub(super) fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        LineIndex { starts }
    }

    /// オフセットを含む行（1 始まり）。
    pub(super) fn line_of(&self, offset: usize) -> u32 {
        self.starts.partition_point(|start| *start <= offset) as u32
    }

    /// `[start, end)` の範囲が占める行レンジ（1 始まり、両端を含む）。末尾の改行は
    /// 次の行に数えない。
    pub(super) fn range_of(&self, text: &str, start: usize, end: usize) -> (u32, u32) {
        let mut end = end.max(start);
        while end > start && text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
        }
        let last = if end > start { end - 1 } else { start };
        (self.line_of(start), self.line_of(last))
    }
}

/// 先頭の front matter（1 行目がちょうど `---` で、その後にちょうど `---` だけの行が
/// 現れるまで）を切り離す。閉じる行が無ければ front matter とみなさない。
/// 返すのは（front matter の行数、front matter のテキスト、残り）。
pub(super) fn split_front_matter(text: &str) -> (u32, Option<&str>, &str) {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (0, None, text);
    };
    if first.trim_end_matches('\n') != "---" {
        return (0, None, text);
    }
    let mut consumed = first.len();
    let mut count = 1;
    for line in lines {
        consumed += line.len();
        count += 1;
        if line.trim_end_matches('\n') == "---" {
            return (count, Some(&text[..consumed]), &text[consumed..]);
        }
    }
    (0, None, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_excludes_the_trailing_newline_of_a_span() {
        let text = "ab\ncd\n\nef\n";
        let index = LineIndex::new(text);

        assert_eq!(index.range_of(text, 0, 6), (1, 2));
        assert_eq!(index.range_of(text, 7, 10), (4, 4));
        assert_eq!(index.range_of(text, 3, 3), (2, 2));
    }

    #[test]
    fn front_matter_needs_a_closing_fence() {
        assert_eq!(split_front_matter("---\na: 1\n---\nbody\n").0, 3);
        assert_eq!(split_front_matter("---\na: 1\nbody\n").0, 0);
        assert_eq!(split_front_matter("--- \na: 1\n---\n").0, 0);
        assert_eq!(split_front_matter("body\n").0, 0);
    }
}
