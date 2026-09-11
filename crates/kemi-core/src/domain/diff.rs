//! 行の整列と表示行（R-VIEW）。equal / replace / delete / insert と、文脈の折りたたみ。

use similar::{ChangeTag, DiffOp, TextDiff};

/// 折りたたみで変更の前後に残す文脈行数の既定値。
pub const DEFAULT_CONTEXT: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Equal,
    Replace,
    Delete,
    Insert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub number: u32,
    pub text: String,
}

/// 単語単位の強調のための断片。`changed` が true の部分を強調する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub old: Option<Line>,
    pub new: Option<Line>,
    pub old_segments: Vec<Segment>,
    pub new_segments: Vec<Segment>,
}

/// 折りたたまれた等しい行の範囲。`from` は含み、`to` は含まない。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayRow {
    Diff(Row),
    Skip(Skip),
}

impl Row {
    fn equal(old_number: u32, old_text: &str, new_number: u32, new_text: &str) -> Self {
        Row {
            kind: RowKind::Equal,
            old: Some(Line {
                number: old_number,
                text: old_text.to_string(),
            }),
            new: Some(Line {
                number: new_number,
                text: new_text.to_string(),
            }),
            old_segments: Vec::new(),
            new_segments: Vec::new(),
        }
    }

    fn replace(old_number: u32, old_text: &str, new_number: u32, new_text: &str) -> Self {
        let (old_segments, new_segments) = diff_segments(old_text, new_text);
        Row {
            kind: RowKind::Replace,
            old: Some(Line {
                number: old_number,
                text: old_text.to_string(),
            }),
            new: Some(Line {
                number: new_number,
                text: new_text.to_string(),
            }),
            old_segments,
            new_segments,
        }
    }

    fn delete(old_number: u32, old_text: &str) -> Self {
        Row {
            kind: RowKind::Delete,
            old: Some(Line {
                number: old_number,
                text: old_text.to_string(),
            }),
            new: None,
            old_segments: Vec::new(),
            new_segments: Vec::new(),
        }
    }

    fn insert(new_number: u32, new_text: &str) -> Self {
        Row {
            kind: RowKind::Insert,
            old: None,
            new: Some(Line {
                number: new_number,
                text: new_text.to_string(),
            }),
            old_segments: Vec::new(),
            new_segments: Vec::new(),
        }
    }
}

/// 正規化済みの左右の行を表示行に整列する。変更の塊ごとに削除と挿入を
/// 位置で対応させ、対になるものを replace にする。
pub fn align(old: &[String], new: &[String]) -> Vec<Row> {
    let old_refs: Vec<&str> = old.iter().map(String::as_str).collect();
    let new_refs: Vec<&str> = new.iter().map(String::as_str).collect();
    let diff = TextDiff::from_slices(&old_refs, &new_refs);
    let ops = diff.ops();
    let mut rows = Vec::new();
    let mut op_index = 0;

    while op_index < ops.len() {
        if let DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = ops[op_index]
        {
            for offset in 0..len {
                rows.push(Row::equal(
                    (old_index + offset + 1) as u32,
                    old_refs[old_index + offset],
                    (new_index + offset + 1) as u32,
                    new_refs[new_index + offset],
                ));
            }
            op_index += 1;
            continue;
        }

        let mut deleted: Vec<(usize, &str)> = Vec::new();
        let mut inserted: Vec<(usize, &str)> = Vec::new();
        while op_index < ops.len() && !matches!(ops[op_index], DiffOp::Equal { .. }) {
            match ops[op_index] {
                DiffOp::Delete {
                    old_index, old_len, ..
                } => {
                    for offset in 0..old_len {
                        deleted.push((old_index + offset, old_refs[old_index + offset]));
                    }
                }
                DiffOp::Insert {
                    new_index, new_len, ..
                } => {
                    for offset in 0..new_len {
                        inserted.push((new_index + offset, new_refs[new_index + offset]));
                    }
                }
                DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    for offset in 0..old_len {
                        deleted.push((old_index + offset, old_refs[old_index + offset]));
                    }
                    for offset in 0..new_len {
                        inserted.push((new_index + offset, new_refs[new_index + offset]));
                    }
                }
                DiffOp::Equal { .. } => unreachable!("equal was handled above"),
            }
            op_index += 1;
        }

        let pairs = deleted.len().min(inserted.len());
        for k in 0..pairs {
            rows.push(Row::replace(
                (deleted[k].0 + 1) as u32,
                deleted[k].1,
                (inserted[k].0 + 1) as u32,
                inserted[k].1,
            ));
        }
        for &(index, text) in &deleted[pairs..] {
            rows.push(Row::delete((index + 1) as u32, text));
        }
        for &(index, text) in &inserted[pairs..] {
            rows.push(Row::insert((index + 1) as u32, text));
        }
    }

    rows
}

/// 置換された 1 行の左右を単語単位で対応させ、強調する断片に分ける。
pub fn diff_segments(old: &str, new: &str) -> (Vec<Segment>, Vec<Segment>) {
    let diff = TextDiff::from_words(old, new);
    let mut old_segments: Vec<Segment> = Vec::new();
    let mut new_segments: Vec<Segment> = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                push_segment(&mut old_segments, change.value(), false);
                push_segment(&mut new_segments, change.value(), false);
            }
            ChangeTag::Delete => push_segment(&mut old_segments, change.value(), true),
            ChangeTag::Insert => push_segment(&mut new_segments, change.value(), true),
        }
    }

    (old_segments, new_segments)
}

fn push_segment(segments: &mut Vec<Segment>, text: &str, changed: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = segments.last_mut() {
        if last.changed == changed {
            last.text.push_str(text);
            return;
        }
    }
    segments.push(Segment {
        text: text.to_string(),
        changed,
    });
}

/// 変更の前後に `context` 行だけ残し、長い等しい行の並びを Skip にする。
/// `keep` が真を返す等しい行（コメントの付いた行など）も、変更の行と同じく畳まない。
/// ファイル全体が等しい場合は畳まずに全行を返す。
pub fn collapse(rows: &[Row], context: usize, keep: impl Fn(&Row) -> bool) -> Vec<DisplayRow> {
    if rows.iter().all(|row| row.kind == RowKind::Equal) {
        return rows.iter().cloned().map(DisplayRow::Diff).collect();
    }
    let foldable = |row: &Row| row.kind == RowKind::Equal && !keep(row);
    let mut display = Vec::new();
    let mut index = 0;

    while index < rows.len() {
        if !foldable(&rows[index]) {
            display.push(DisplayRow::Diff(rows[index].clone()));
            index += 1;
            continue;
        }

        let start = index;
        while index < rows.len() && foldable(&rows[index]) {
            index += 1;
        }
        let end = index;
        let shown_before = if start == 0 { 0 } else { context };
        let shown_after = if end == rows.len() { 0 } else { context };

        if end - start <= shown_before + shown_after {
            display.extend(rows[start..end].iter().cloned().map(DisplayRow::Diff));
            continue;
        }
        let (fold_from, fold_to) = (start + shown_before, end - shown_after);
        display.extend(rows[start..fold_from].iter().cloned().map(DisplayRow::Diff));
        display.push(DisplayRow::Skip(Skip {
            from: fold_from,
            to: fold_to,
        }));
        display.extend(rows[fold_to..end].iter().cloned().map(DisplayRow::Diff));
    }

    display
}

/// 折りたたまれた範囲の元の行を返す。
pub fn expand(rows: &[Row], skip: &Skip) -> Vec<Row> {
    rows[skip.from..skip.to].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn numbered(prefix: &str, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("{prefix}{i}")).collect()
    }

    #[test]
    fn align_keeps_equal_lines_with_numbering() {
        let rows = align(&s(&["a", "b"]), &s(&["a", "b"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(kinds, vec![RowKind::Equal, RowKind::Equal]);
        let first = &rows[0];
        assert_eq!(first.old.as_ref().unwrap().number, 1);
        assert_eq!(first.old.as_ref().unwrap().text, "a");
        assert_eq!(first.new.as_ref().unwrap().number, 1);
        assert_eq!(first.new.as_ref().unwrap().text, "a");
    }

    #[test]
    fn align_marks_inserted_lines_without_old_side() {
        let rows = align(&s(&["a"]), &s(&["a", "b"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(kinds, vec![RowKind::Equal, RowKind::Insert]);
        assert!(rows[1].old.is_none());
        assert_eq!(rows[1].new.as_ref().unwrap().number, 2);
        assert_eq!(rows[1].new.as_ref().unwrap().text, "b");
    }

    #[test]
    fn align_marks_deleted_lines_without_new_side() {
        let rows = align(&s(&["a", "b"]), &s(&["a"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(kinds, vec![RowKind::Equal, RowKind::Delete]);
        assert!(rows[1].new.is_none());
        assert_eq!(rows[1].old.as_ref().unwrap().number, 2);
        assert_eq!(rows[1].old.as_ref().unwrap().text, "b");
    }

    #[test]
    fn align_pairs_replaced_lines() {
        let rows = align(&s(&["a", "b", "c"]), &s(&["a", "x", "c"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(
            kinds,
            vec![RowKind::Equal, RowKind::Replace, RowKind::Equal]
        );
        let replaced = &rows[1];
        assert_eq!(replaced.old.as_ref().unwrap().text, "b");
        assert_eq!(replaced.new.as_ref().unwrap().text, "x");
    }

    #[test]
    fn align_pairs_uneven_replace_then_inserts() {
        let rows = align(&s(&["a", "b"]), &s(&["a", "x", "y"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(
            kinds,
            vec![RowKind::Equal, RowKind::Replace, RowKind::Insert]
        );
        assert_eq!(rows[1].old.as_ref().unwrap().text, "b");
        assert_eq!(rows[1].new.as_ref().unwrap().text, "x");
        assert_eq!(rows[2].new.as_ref().unwrap().text, "y");
    }

    #[test]
    fn align_pairs_uneven_replace_then_deletes() {
        let rows = align(&s(&["a", "b", "c"]), &s(&["a", "x"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert!(
            kinds == vec![RowKind::Equal, RowKind::Replace, RowKind::Delete]
                || kinds == vec![RowKind::Equal, RowKind::Delete, RowKind::Replace],
            "unexpected kinds: {kinds:?}"
        );
    }

    #[test]
    fn align_of_empty_old_side_is_all_inserts() {
        let rows = align(&[], &s(&["a", "b"]));

        let kinds: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(kinds, vec![RowKind::Insert, RowKind::Insert]);
        assert!(rows.iter().all(|row| row.old.is_none()));
    }

    #[test]
    fn align_of_empty_both_sides_is_empty() {
        assert!(align(&[], &[]).is_empty());
    }

    #[test]
    fn segments_marks_changed_words_on_the_replaced_line() {
        let (old_segments, new_segments) = diff_segments("fn old_name() {}", "fn new_name() {}");

        assert!(old_segments.iter().any(|segment| segment.changed));
        assert!(new_segments.iter().any(|segment| segment.changed));
        let old_joined: String = old_segments.iter().map(|s| s.text.as_str()).collect();
        let new_joined: String = new_segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(old_joined, "fn old_name() {}");
        assert_eq!(new_joined, "fn new_name() {}");
    }

    #[test]
    fn segments_of_equal_lines_have_no_changes() {
        let (old_segments, new_segments) = diff_segments("same text", "same text");

        assert!(old_segments.iter().all(|segment| !segment.changed));
        assert!(new_segments.iter().all(|segment| !segment.changed));
    }

    #[test]
    fn segments_keep_common_prefix_unchanged() {
        let (old_segments, _) = diff_segments("value = 1;", "value = 2;");

        assert_eq!(old_segments[0].text, "value = ");
        assert!(!old_segments[0].changed);
        assert!(old_segments.last().is_some_and(|segment| segment.changed));
    }

    #[test]
    fn collapse_keeps_short_equal_runs() {
        let old = s(&["a", "b", "old", "c", "d"]);
        let new = s(&["a", "b", "new", "c", "d"]);
        let rows = align(&old, &new);
        let display = collapse(&rows, 3, |_| false);

        assert_eq!(display.len(), 5);
        assert!(display.iter().all(|row| matches!(row, DisplayRow::Diff(_))));
    }

    #[test]
    fn collapse_hides_long_interior_context_and_keeps_edges() {
        let mut old = numbered("top", 10);
        old.push("old".to_string());
        old.extend(numbered("bottom", 10));
        let mut new = numbered("top", 10);
        new.push("new".to_string());
        new.extend(numbered("bottom", 10));
        let rows = align(&old, &new);

        let display = collapse(&rows, 2, |_| false);
        let skips: Vec<&Skip> = display
            .iter()
            .filter_map(|row| match row {
                DisplayRow::Skip(skip) => Some(skip),
                DisplayRow::Diff(_) => None,
            })
            .collect();

        assert_eq!(skips.len(), 2);
        assert_eq!((skips[0].from, skips[0].to), (0, 8));
        assert_eq!((skips[1].from, skips[1].to), (13, 21));
        assert_eq!(display.len(), 2 + 1 + 2 + 2);
    }

    #[test]
    fn collapse_hides_leading_and_trailing_context_separately() {
        let mut old = numbered("top", 10);
        old.push("old".to_string());
        old.extend(numbered("bottom", 10));
        old.push("old2".to_string());
        old.extend(numbered("tail", 3));
        let mut new = numbered("top", 10);
        new.push("new".to_string());
        new.extend(numbered("bottom", 10));
        new.push("new2".to_string());
        new.extend(numbered("tail", 3));
        let rows = align(&old, &new);

        let display = collapse(&rows, 1, |_| false);
        let first = match &display[0] {
            DisplayRow::Skip(skip) => skip,
            other => panic!("expected leading skip, got {other:?}"),
        };
        assert_eq!((first.from, first.to), (0, 9));
        let last = match display.last().unwrap() {
            DisplayRow::Skip(skip) => skip,
            other => panic!("expected trailing skip, got {other:?}"),
        };
        assert_eq!(last.to, rows.len());
    }

    #[test]
    fn collapse_keeps_all_equal_rows_when_nothing_to_fold_around() {
        let rows = align(&numbered("same", 10), &numbered("same", 10));
        let display = collapse(&rows, 2, |_| false);

        assert_eq!(display.len(), 10);
        assert!(display.iter().all(|row| matches!(row, DisplayRow::Diff(_))));
    }

    #[test]
    fn collapse_keeps_rows_asked_to_keep_like_changes() {
        let old = numbered("line", 40);
        let mut new = old.clone();
        new[4] = "changed".to_string();
        new[29] = "changed".to_string();
        let rows = align(&old, &new);
        let kept = [1, 17, 38];

        let display = collapse(&rows, 3, |row| {
            row.new
                .as_ref()
                .is_some_and(|line| kept.contains(&line.number))
        });

        for number in kept {
            assert!(
                shows_new_line(&display, number),
                "new line {number} is hidden: {display:?}"
            );
        }
    }

    /// 表示行のうち、新側の行番号が `number` の行が畳まれずに出ているか。
    fn shows_new_line(display: &[DisplayRow], number: u32) -> bool {
        display.iter().any(|row| {
            matches!(
                row,
                DisplayRow::Diff(row) if row.new.as_ref().is_some_and(|line| line.number == number)
            )
        })
    }

    /// 変更の行の表示位置の間にある Skip の数。
    fn skips_between_changes(display: &[DisplayRow]) -> usize {
        let changes: Vec<usize> = display
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, DisplayRow::Diff(row) if row.kind != RowKind::Equal))
            .map(|(index, _)| index)
            .collect();
        let (first, last) = (changes[0], changes[changes.len() - 1]);
        display[first..last]
            .iter()
            .filter(|row| matches!(row, DisplayRow::Skip(_)))
            .count()
    }

    #[test]
    fn collapse_between_two_changes_keeps_context_beside_each_and_folds_the_middle() {
        let old = numbered("line", 40);
        let mut new = old.clone();
        new[4] = "changed".to_string();
        new[29] = "changed".to_string();
        let rows = align(&old, &new);

        let display = collapse(&rows, 3, |_| false);

        for number in [4, 6, 29, 31] {
            assert!(
                shows_new_line(&display, number),
                "new line {number} next to a change is hidden: {display:?}"
            );
        }
        assert!(
            !shows_new_line(&display, 18),
            "the middle of the run is shown: {display:?}"
        );
        assert_eq!(skips_between_changes(&display), 1, "{display:?}");
    }

    #[test]
    fn collapse_keeps_context_beside_a_kept_row_inside_a_leading_run() {
        let old = numbered("line", 40);
        let mut new = old.clone();
        new[19] = "changed".to_string();
        let rows = align(&old, &new);

        let display = collapse(&rows, 3, |row| {
            row.new.as_ref().is_some_and(|line| line.number == 1)
        });

        for number in [1, 2, 17, 18, 19] {
            assert!(
                shows_new_line(&display, number),
                "new line {number} next to a kept row or a change is hidden: {display:?}"
            );
        }
        assert!(
            !shows_new_line(&display, 10),
            "the middle of the run is shown: {display:?}"
        );
    }

    #[test]
    fn collapse_keeps_all_equal_rows_even_with_rows_to_keep() {
        let rows = align(&numbered("same", 20), &numbered("same", 20));
        let display = collapse(&rows, 2, |row| {
            row.new.as_ref().is_some_and(|line| line.number == 10)
        });

        assert_eq!(display.len(), 20);
        assert!(display.iter().all(|row| matches!(row, DisplayRow::Diff(_))));
    }

    #[test]
    fn expand_returns_the_hidden_rows() {
        let mut old = numbered("top", 10);
        old.push("old".to_string());
        let new = {
            let mut new = numbered("top", 10);
            new.push("new".to_string());
            new
        };
        let rows = align(&old, &new);
        let display = collapse(&rows, 2, |_| false);
        let skip = match &display[0] {
            DisplayRow::Skip(skip) => skip,
            other => panic!("expected skip, got {other:?}"),
        };

        let expanded = expand(&rows, skip);
        assert_eq!(expanded.len(), skip.to - skip.from);
        assert_eq!(expanded[0].old.as_ref().unwrap().text, "top0");
        assert_eq!(expanded[7].old.as_ref().unwrap().text, "top7");
    }
}
