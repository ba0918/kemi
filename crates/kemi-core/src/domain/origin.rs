//! 由来（R-ORIGIN）。最終形の各変更ブロックに、その変更を入れたコミットを割り当てる。
//!
//! 行ごとのコミットの対応（どのコミットがその行を最後に変えたか・消したか）は
//! source が git から求める。ここでは整列済みの行とその対応だけから決める。

use std::collections::HashMap;
use std::ops::Range;

use crate::domain::diff::{Row, RowKind};
use crate::domain::review::Side;

/// 由来の範囲（`--from..--to`）に入るコミット。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeCommit {
    pub sha: String,
    pub merge: bool,
}

/// ある行を変えた（または消した）コミットと、そのコミット時点の行の位置。
/// 新側の行ではそのコミットの新側の行、削除された行ではそのコミットの旧側の行を指す。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineCommit {
    pub sha: String,
    pub path: String,
    pub line: u32,
}

/// 変更ブロックのうち、範囲内のコミットに当たらなかった行の多さ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    None,
    Some,
    All,
}

/// 「このコミットで見る」の移り先。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginTarget {
    pub path: String,
    pub side: Side,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginEntry {
    pub sha: String,
    pub merge: bool,
    /// マージの由来は理由を開くだけで、移り先を持たない。
    pub target: Option<OriginTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockOrigin {
    /// 変更ブロックの最初の行の、整列済みの行の中の位置。
    pub row: usize,
    /// 新しい順。
    pub entries: Vec<OriginEntry>,
    pub unknown: Unknown,
}

/// 変更ブロック（追加・削除・書き換えの行が途切れずに続くまとまり）の範囲。
pub fn change_blocks(rows: &[Row]) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        if rows[index].kind == RowKind::Equal {
            index += 1;
            continue;
        }
        let start = index;
        while index < rows.len() && rows[index].kind != RowKind::Equal {
            index += 1;
        }
        blocks.push(start..index);
    }
    blocks
}

/// 各変更ブロックに由来を割り当てる。
///
/// - `new_lines[n - 1]` は新側の n 行目を最後に変えたコミット（通常 0 か 1 個）。
/// - `old_lines[n - 1]` は旧側の n 行目を消したコミット（0 個以上）。
/// - `range` は範囲内のコミットを新しい順に並べたもの。ここに無いコミットは由来に出さない。
///
/// 新側の行を持つブロックは新側の行から、削除だけのブロックは消えた行から決める。
pub fn assign_origins(
    rows: &[Row],
    new_lines: &[Vec<LineCommit>],
    old_lines: &[Vec<LineCommit>],
    range: &[RangeCommit],
) -> Vec<BlockOrigin> {
    let ranks: HashMap<&str, usize> = range
        .iter()
        .enumerate()
        .map(|(position, commit)| (commit.sha.as_str(), position))
        .collect();
    change_blocks(rows)
        .into_iter()
        .map(|block| {
            let block_rows = &rows[block.clone()];
            let has_new_side = block_rows.iter().any(|row| row.new.is_some());
            let (side, line_commits): (Side, Vec<&[LineCommit]>) = if has_new_side {
                (
                    Side::New,
                    block_rows
                        .iter()
                        .filter_map(|row| row.new.as_ref())
                        .map(|line| commits_at(new_lines, line.number))
                        .collect(),
                )
            } else {
                (
                    Side::Old,
                    block_rows
                        .iter()
                        .filter_map(|row| row.old.as_ref())
                        .map(|line| commits_at(old_lines, line.number))
                        .collect(),
                )
            };
            origin_of_block(block.start, side, &line_commits, range, &ranks)
        })
        .collect()
}

fn commits_at(lines: &[Vec<LineCommit>], number: u32) -> &[LineCommit] {
    lines
        .get(number as usize - 1)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// 1 つのブロックの行ごとのコミットから、範囲内のコミットを新しい順に 1 回ずつ並べる。
/// 移り先は、そのコミットが当たった最初の行にする。
fn origin_of_block(
    row: usize,
    side: Side,
    line_commits: &[&[LineCommit]],
    range: &[RangeCommit],
    ranks: &HashMap<&str, usize>,
) -> BlockOrigin {
    let mut found: Vec<(usize, &LineCommit)> = Vec::new();
    let mut unknown_lines = 0;
    for commits in line_commits {
        let mut hit = false;
        for commit in commits.iter() {
            if let Some(&position) = ranks.get(commit.sha.as_str()) {
                hit = true;
                if !found.iter().any(|(_, seen)| seen.sha == commit.sha) {
                    found.push((position, commit));
                }
            }
        }
        if !hit {
            unknown_lines += 1;
        }
    }
    found.sort_by_key(|(position, _)| *position);

    let unknown = if unknown_lines == 0 {
        Unknown::None
    } else if unknown_lines == line_commits.len() {
        Unknown::All
    } else {
        Unknown::Some
    };
    let entries = found
        .into_iter()
        .map(|(position, commit)| {
            let merge = range[position].merge;
            OriginEntry {
                sha: commit.sha.clone(),
                merge,
                target: (!merge).then(|| OriginTarget {
                    path: commit.path.clone(),
                    side,
                    line: commit.line,
                }),
            }
        })
        .collect();
    BlockOrigin {
        row,
        entries,
        unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::align;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn commit(sha: &str, path: &str, line: u32) -> Vec<LineCommit> {
        vec![LineCommit {
            sha: sha.to_string(),
            path: path.to_string(),
            line,
        }]
    }

    fn range(shas: &[(&str, bool)]) -> Vec<RangeCommit> {
        shas.iter()
            .map(|(sha, merge)| RangeCommit {
                sha: sha.to_string(),
                merge: *merge,
            })
            .collect()
    }

    fn shas(block: &BlockOrigin) -> Vec<&str> {
        block
            .entries
            .iter()
            .map(|entry| entry.sha.as_str())
            .collect()
    }

    #[test]
    fn origin_change_blocks_are_runs_of_changed_rows() {
        let rows = align(
            &lines(&["a", "b", "c", "d", "e"]),
            &lines(&["a", "B", "c", "e", "f"]),
        );

        let blocks = change_blocks(&rows);

        assert_eq!(blocks.len(), 3, "{rows:?}");
        assert!(blocks.iter().all(|block| rows[block.clone()]
            .iter()
            .all(|row| row.kind != RowKind::Equal)));
        assert_eq!(blocks[0], 1..2);
    }

    #[test]
    fn origin_block_lists_commits_of_its_new_lines_newest_first() {
        let rows = align(&lines(&["a", "b", "c"]), &lines(&["a", "B", "C"]));
        let new_lines = vec![vec![], commit("old1", "f", 2), commit("new1", "f", 3)];

        let origins = assign_origins(
            &rows,
            &new_lines,
            &[],
            &range(&[("new1", false), ("old1", false)]),
        );

        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].row, 1);
        assert_eq!(shas(&origins[0]), vec!["new1", "old1"]);
        assert_eq!(origins[0].unknown, Unknown::None);
    }

    #[test]
    fn origin_target_points_at_the_line_in_that_commit() {
        let rows = align(&lines(&["a", "b"]), &lines(&["a", "B"]));
        let new_lines = vec![vec![], commit("c1", "before/rename.txt", 7)];

        let origins = assign_origins(&rows, &new_lines, &[], &range(&[("c1", false)]));

        assert_eq!(
            origins[0].entries[0].target,
            Some(OriginTarget {
                path: "before/rename.txt".to_string(),
                side: Side::New,
                line: 7,
            })
        );
    }

    #[test]
    fn origin_deletion_block_uses_the_commits_that_removed_its_lines() {
        let rows = align(&lines(&["a", "b", "c"]), &lines(&["a", "c"]));
        let old_lines = vec![vec![], commit("del1", "f", 2), vec![]];

        let origins = assign_origins(&rows, &[], &old_lines, &range(&[("del1", false)]));

        assert_eq!(origins.len(), 1);
        assert_eq!(shas(&origins[0]), vec!["del1"]);
        assert_eq!(
            origins[0].entries[0]
                .target
                .as_ref()
                .map(|target| target.side),
            Some(Side::Old)
        );
        assert_eq!(origins[0].unknown, Unknown::None);
    }

    #[test]
    fn origin_commit_outside_range_is_not_listed_and_counts_as_unknown() {
        let rows = align(&lines(&["a", "b"]), &lines(&["A", "B"]));
        let new_lines = vec![commit("inside", "f", 1), commit("outside", "f", 2)];

        let origins = assign_origins(&rows, &new_lines, &[], &range(&[("inside", false)]));

        assert_eq!(shas(&origins[0]), vec!["inside"]);
        assert_eq!(origins[0].unknown, Unknown::Some);
    }

    #[test]
    fn origin_deletion_with_some_untraced_lines_is_partially_unknown() {
        let rows = align(&lines(&["a", "b", "c", "d"]), &lines(&["a", "d"]));
        let old_lines = vec![vec![], commit("del1", "f", 2), vec![], vec![]];

        let origins = assign_origins(&rows, &[], &old_lines, &range(&[("del1", false)]));

        assert_eq!(shas(&origins[0]), vec!["del1"]);
        assert_eq!(origins[0].unknown, Unknown::Some);
    }

    #[test]
    fn origin_deletion_with_no_traced_line_is_unknown_only() {
        let rows = align(&lines(&["a", "b", "c"]), &lines(&["a", "c"]));

        let origins = assign_origins(&rows, &[], &[], &range(&[("del1", false)]));

        assert!(origins[0].entries.is_empty());
        assert_eq!(origins[0].unknown, Unknown::All);
    }

    #[test]
    fn origin_merge_entry_has_no_target() {
        let rows = align(&lines(&["a", "b"]), &lines(&["a", "B"]));
        let new_lines = vec![vec![], commit("m1", "f", 2)];

        let origins = assign_origins(&rows, &new_lines, &[], &range(&[("m1", true)]));

        assert!(origins[0].entries[0].merge);
        assert_eq!(origins[0].entries[0].target, None);
    }

    #[test]
    fn origin_commit_appears_once_per_block_at_its_first_line() {
        let rows = align(&lines(&["a", "b", "c", "d"]), &lines(&["a", "B", "C", "D"]));
        let new_lines = vec![
            vec![],
            commit("c1", "f", 20),
            commit("c2", "f", 30),
            commit("c1", "f", 40),
        ];

        let origins = assign_origins(
            &rows,
            &new_lines,
            &[],
            &range(&[("c2", false), ("c1", false)]),
        );

        assert_eq!(shas(&origins[0]), vec!["c2", "c1"]);
        assert_eq!(
            origins[0].entries[1]
                .target
                .as_ref()
                .map(|target| target.line),
            Some(20)
        );
    }
}
