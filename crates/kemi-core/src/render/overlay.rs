//! 旧新のブロックを行の整列で対応づけ、各ブロックの印と、消したブロックの差し込み
//! 位置を決める（R-RENDER の「変更の見せ方」）。
//!
//! 新側のブロック列が骨格。旧側は、変更のあるブロックだけを見て、構造が同じ文の
//! ブロックに一対一で対応するものは新側に重ね、それ以外は対応する新側のブロックの
//! 前（対応が無ければ整列で決まる位置）に消したブロックとして差し込む。

use std::collections::BTreeSet;

use crate::domain::diff::{self, RowKind};

use super::inline::WordMarks;
use super::markdown::{BlockInfo, Plan, SideDoc};
use super::Mark;

/// 行ごとの整列の結果。添字は 1 始まりの行番号で、0 は使わない。
struct Aligned {
    old_changed: Vec<bool>,
    new_changed: Vec<bool>,
    /// 旧の行に対応する新の行（等しい行と書き換えの行）。
    new_of_old: Vec<Option<u32>>,
    /// 消した旧の行を差し込む先の新の行。文書の末尾なら None。
    anchor_of_old: Vec<Option<u32>>,
}

/// ブロックの行番号は構文木の span から、整列の行番号は表示用の行の分割から来るので、
/// 入力によってはブロックの行番号が整列の行数を超える。超えた行は、対応する行が無く
/// 変わってもいないものとして読む。
impl Aligned {
    fn old_changed(&self, line: u32) -> bool {
        self.old_changed
            .get(line as usize)
            .copied()
            .unwrap_or(false)
    }

    fn new_changed(&self, line: u32) -> bool {
        self.new_changed
            .get(line as usize)
            .copied()
            .unwrap_or(false)
    }

    fn new_of_old(&self, line: u32) -> Option<u32> {
        self.new_of_old.get(line as usize).copied().flatten()
    }

    fn anchor_of_old(&self, line: u32) -> Option<u32> {
        self.anchor_of_old.get(line as usize).copied().flatten()
    }
}

fn align(old: &[String], new: &[String]) -> Aligned {
    let rows = diff::align(old, new);
    let mut aligned = Aligned {
        old_changed: vec![false; old.len() + 1],
        new_changed: vec![false; new.len() + 1],
        new_of_old: vec![None; old.len() + 1],
        anchor_of_old: vec![None; old.len() + 1],
    };
    let mut pending: Vec<u32> = Vec::new();
    for row in &rows {
        let old_number = row.old.as_ref().map(|line| line.number);
        let new_number = row.new.as_ref().map(|line| line.number);
        match row.kind {
            RowKind::Equal => {}
            RowKind::Replace => {
                aligned.old_changed[old_number.unwrap_or(0) as usize] = true;
                aligned.new_changed[new_number.unwrap_or(0) as usize] = true;
            }
            RowKind::Delete => {
                aligned.old_changed[old_number.unwrap_or(0) as usize] = true;
                pending.extend(old_number);
                continue;
            }
            RowKind::Insert => {
                aligned.new_changed[new_number.unwrap_or(0) as usize] = true;
            }
        }
        if let (Some(old_number), Some(new_number)) = (old_number, new_number) {
            aligned.new_of_old[old_number as usize] = Some(new_number);
        }
        for old_number in pending.drain(..) {
            aligned.anchor_of_old[old_number as usize] = new_number;
        }
    }
    aligned
}

/// 新の行番号からブロックの添字を引く表。
fn block_at(blocks: &[BlockInfo<'_, '_>], total: usize) -> Vec<Option<usize>> {
    let mut table = vec![None; total + 1];
    for (index, block) in blocks.iter().enumerate() {
        for line in block.start..=block.end {
            if let Some(slot) = table.get_mut(line as usize) {
                *slot = Some(index);
            }
        }
    }
    table
}

pub(super) fn plan(old: &SideDoc<'_>, new: &SideDoc<'_>) -> Plan {
    let aligned = align(old.lines, new.lines);
    let new_at = block_at(&new.blocks, new.lines.len());

    // 旧のブロックごとの、対応する新のブロック。対応の根拠は、行の対応（等しい行と
    // 書き換えの行）と、消した行がブロックの途中に差し込まれること。
    let mut candidates_of_old: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); old.blocks.len()];
    let mut candidates_of_new: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); new.blocks.len()];
    for (a, block) in old.blocks.iter().enumerate() {
        for line in block.start..=block.end {
            let paired = aligned.new_of_old(line).and_then(|n| new_at[n as usize]);
            let interior = aligned
                .anchor_of_old(line)
                .and_then(|n| new_at[n as usize].filter(|b| new.blocks[*b].start < n));
            for b in paired.into_iter().chain(interior) {
                candidates_of_old[a].insert(b);
                candidates_of_new[b].insert(a);
            }
        }
    }

    // ブロックが変わったか。中の行が変わったか、対応先が 2 つ以上に割れた（分割・
    // 結合）か。
    let old_changed: Vec<bool> = old
        .blocks
        .iter()
        .enumerate()
        .map(|(a, block)| {
            (block.start..=block.end).any(|line| aligned.old_changed(line))
                || candidates_of_old[a].len() > 1
        })
        .collect();
    let new_line_changed: Vec<bool> = new
        .blocks
        .iter()
        .enumerate()
        .map(|(b, block)| {
            (block.start..=block.end).any(|line| aligned.new_changed(line))
                || candidates_of_new[b].len() > 1
                || candidates_of_new[b].iter().any(|a| old_changed[*a])
        })
        .collect();
    // 消した行が途中に差し込まれる新のブロックも変わったとみなす。
    let mut new_changed = new_line_changed;
    for (a, candidates) in candidates_of_old.iter().enumerate() {
        if !old_changed[a] {
            continue;
        }
        for b in candidates {
            new_changed[*b] = true;
        }
    }

    let mut plan = Plan::uniform(new.blocks.len(), Mark::Unchanged);
    for (b, changed) in new_changed.iter().enumerate() {
        if *changed {
            plan.decisions[b].mark = Mark::Added;
        }
    }
    for (a, block) in old.blocks.iter().enumerate() {
        if !old_changed[a] {
            continue;
        }
        let candidates = &candidates_of_old[a];
        if let Some(&b) = candidates.iter().next() {
            if candidates.len() == 1
                && candidates_of_new[b].len() == 1
                && overlayable(block, &new.blocks[b])
            {
                let decision = &mut plan.decisions[b];
                decision.mark = Mark::Modified;
                decision.words = Some(word_marks(text_of(block), text_of(&new.blocks[b])));
                continue;
            }
            plan.decisions[b].before.push(a);
            continue;
        }
        match deletion_target(&aligned, &new.blocks, block) {
            Some(b) => plan.decisions[b].before.push(a),
            None => plan.trailing.push(a),
        }
    }
    plan
}

/// 対応する新のブロックが無い、消したブロックを差し込む先。整列で決まる新の行の位置
/// 以降で最初のブロックの前。それも無ければ末尾。
fn deletion_target(
    aligned: &Aligned,
    new_blocks: &[BlockInfo<'_, '_>],
    block: &BlockInfo<'_, '_>,
) -> Option<usize> {
    let anchor = aligned
        .anchor_of_old(block.start)
        .or_else(|| aligned.new_of_old(block.start))?;
    new_blocks
        .iter()
        .position(|candidate| candidate.end >= anchor)
}

fn text_of<'b>(block: &'b BlockInfo<'_, '_>) -> &'b str {
    block
        .inline
        .as_ref()
        .map(|inline| inline.text.as_str())
        .unwrap_or("")
}

/// 旧と新でブロックの種類と、インラインの種類と順番が同じ文のブロックか。
fn overlayable(old: &BlockInfo<'_, '_>, new: &BlockInfo<'_, '_>) -> bool {
    old.kind == new.kind
        && old.kind.is_sentence()
        && match (&old.inline, &new.inline) {
            (Some(old), Some(new)) => old.signature == new.signature,
            _ => false,
        }
}

/// 旧と新のテキスト全体の語の差分から、新側のオフセットで表した印を作る。
fn word_marks(old: &str, new: &str) -> WordMarks {
    let (old_segments, new_segments) = diff::diff_segments(old, new);
    let mut marks = WordMarks {
        inserted: Vec::new(),
        deleted: Vec::new(),
    };
    let (mut i, mut j) = (0, 0);
    let mut offset = 0;
    loop {
        match (old_segments.get(i), new_segments.get(j)) {
            (Some(removed), _) if removed.changed => {
                marks.deleted.push((offset, removed.text.clone()));
                i += 1;
            }
            (_, Some(added)) if added.changed => {
                marks.inserted.push((offset, offset + added.text.len()));
                offset += added.text.len();
                j += 1;
            }
            (Some(_), Some(same)) => {
                offset += same.text.len();
                i += 1;
                j += 1;
            }
            (Some(_), None) => i += 1,
            (None, Some(same)) => {
                offset += same.text.len();
                j += 1;
            }
            (None, None) => break,
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn line_numbers_past_the_aligned_lines_have_no_pair_and_no_change() {
        let aligned = align(&text(&["a", "b"]), &text(&["a", "c"]));

        assert_eq!(aligned.new_of_old(9), None);
        assert_eq!(aligned.anchor_of_old(9), None);
        assert!(!aligned.old_changed(9));
        assert!(!aligned.new_changed(9));
    }

    #[test]
    fn word_marks_place_deleted_words_at_their_new_offset() {
        let marks = word_marks("the brown fox", "the red fox");

        assert_eq!(marks.inserted, vec![(4, 7)]);
        assert_eq!(marks.deleted, vec![(4, "brown".to_string())]);
    }
}
