//! 起動中のレビューの並べ方（R-PS-OUTPUT）。ファイルを読み終えた値だけから決める。

use std::cmp::Ordering;

use kemi_core::session::RunningRecord;

/// 起動中の記録を読んだ結果。
#[derive(Clone, Debug)]
pub(crate) enum RecordRead {
    Found(RunningRecord),
    /// 無い（この変更より前の版で起動したサーバー、記録を書くまでの一瞬）。
    Missing,
    /// 読めない。理由を知らせる。
    Unreadable(String),
}

/// 一覧の 1 行。記録が無い・読めないレビューは `record` が無く、`id` だけを出す。
#[derive(Debug)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) record: Option<RunningRecord>,
}

#[derive(Debug)]
pub(crate) struct Listing {
    pub(crate) rows: Vec<Row>,
    /// 範囲の外の起動中のレビューの数。記録が無い・読めないものは数えない。
    pub(crate) outside: usize,
    /// stderr に出す警告（読めなかった記録の理由）。
    pub(crate) warnings: Vec<String>,
}

/// `running` は起動中のレビューの id と、その記録を読んだ結果。`here` は ps を打った場所の
/// 識別で、`any` でなければ記録の識別がこれと同じものだけを出す。記録が無い・読めないものは
/// ワークスペースが分からないので、範囲を問わず出す。
pub(crate) fn assemble(running: Vec<(String, RecordRead)>, here: &str, any: bool) -> Listing {
    let mut rows = Vec::new();
    let mut outside = 0;
    let mut warnings = Vec::new();
    for (id, read) in running {
        let record = match read {
            RecordRead::Found(record) => Some(record),
            RecordRead::Missing => None,
            RecordRead::Unreadable(reason) => {
                warnings.push(format!("cannot read the running record of {id}: {reason}"));
                None
            }
        };
        if let Some(record) = &record
            && !any
            && record.workspace_key != here
        {
            outside += 1;
            continue;
        }
        rows.push(Row { id, record });
    }
    rows.sort_by(newest_start_first);
    Listing {
        rows,
        outside,
        warnings,
    }
}

/// 起動した時刻の新しい順。時刻の取れないものは最後。時刻が同じか取れないもの同士は id の順。
fn newest_start_first(left: &Row, right: &Row) -> Ordering {
    let started = |row: &Row| row.record.as_ref().map(|record| record.started_at);
    started(right)
        .cmp(&started(left))
        .then_with(|| left.id.cmp(&right.id))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kemi_core::session::{RunningMode, RunningRecord};

    use super::*;

    const HERE: &str = "00000000000000aa";
    const ELSEWHERE: &str = "00000000000000bb";

    fn record(started_at: u128, workspace_key: &str) -> RunningRecord {
        RunningRecord {
            pid: 1,
            url: "http://127.0.0.1:1/s/t/".to_string(),
            live_url: None,
            started_at,
            version: "0.4.1".to_string(),
            mode: RunningMode::Worktree,
            label: "worktree".to_string(),
            workspace: PathBuf::from("/tmp/workspace"),
            workspace_key: workspace_key.to_string(),
        }
    }

    fn ids(listing: &Listing) -> Vec<&str> {
        listing.rows.iter().map(|row| row.id.as_str()).collect()
    }

    #[test]
    fn ps_rows_put_the_newest_start_first_and_unknown_last() {
        let running = vec![
            ("D".to_string(), RecordRead::Missing),
            ("A".to_string(), RecordRead::Found(record(100, HERE))),
            ("C".to_string(), RecordRead::Found(record(300, HERE))),
            ("B".to_string(), RecordRead::Found(record(300, HERE))),
            ("E".to_string(), RecordRead::Unreadable("bad".to_string())),
            ("0".to_string(), RecordRead::Missing),
        ];

        let listing = assemble(running, HERE, false);

        // 時刻が同じもの同士と、時刻の取れないもの同士は id の順。
        assert_eq!(ids(&listing), ["B", "C", "A", "0", "D", "E"]);
    }

    #[test]
    fn ps_rows_outside_the_workspace_are_counted_not_listed() {
        let running = vec![
            ("A".to_string(), RecordRead::Found(record(100, HERE))),
            ("B".to_string(), RecordRead::Found(record(200, ELSEWHERE))),
            ("C".to_string(), RecordRead::Found(record(300, ELSEWHERE))),
        ];

        let here = assemble(running.clone(), HERE, false);
        let any = assemble(running, HERE, true);

        assert_eq!((ids(&here), here.outside), (vec!["A"], 2));
        assert_eq!((ids(&any), any.outside), (vec!["C", "B", "A"], 0));
    }

    #[test]
    fn ps_rows_without_a_record_are_listed_everywhere_and_not_counted() {
        let running = vec![
            ("A".to_string(), RecordRead::Missing),
            (
                "B".to_string(),
                RecordRead::Unreadable("bad json".to_string()),
            ),
        ];

        let listing = assemble(running, HERE, false);

        assert_eq!(ids(&listing), ["A", "B"]);
        assert!(listing.rows.iter().all(|row| row.record.is_none()));
        assert_eq!(listing.outside, 0);
        // 読めないときだけ理由を知らせ、無いときは知らせない。
        assert_eq!(listing.warnings.len(), 1);
        assert!(
            listing.warnings[0].contains('B') && listing.warnings[0].contains("bad json"),
            "{:?}",
            listing.warnings
        );
    }
}
