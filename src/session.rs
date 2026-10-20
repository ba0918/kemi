//! セッションの保存先の配線（R-SESSION）。サーバへ渡す sink の実装と、CLI の一覧。

use std::sync::{Mutex, OnceLock};

use std::collections::BTreeSet;

use kemi_core::session::{
    FilesWrite, OpenSession, PageSnapshot, SessionError, SessionInfo, SessionState, SessionStore,
    SessionSummary,
};
use kemi_core::source::FrozenSource;
use kemi_server::SessionSink;

use crate::local_time;

/// プロセス 1 つ分の開いているセッション。
pub struct StoredSession {
    explanation: OnceLock<kemi_core::domain::explanation::ExplanationDocument>,
    open: Mutex<OpenSession>,
    resumed: bool,
}

impl StoredSession {
    /// 新しいセッションを始める。ロックを取り、id を決める。
    pub fn start(store: &SessionStore, info: SessionInfo) -> Result<Self, SessionError> {
        Ok(StoredSession {
            explanation: OnceLock::new(),
            resumed: false,
            open: Mutex::new(store.create(info)?),
        })
    }

    /// 復元できるセッションを開く。ロック中のセッションは `Locked` になる。
    pub fn resume(store: &SessionStore, id: &str) -> Result<Self, SessionError> {
        Ok(StoredSession {
            explanation: OnceLock::new(),
            resumed: true,
            open: Mutex::new(store.open(id)?),
        })
    }

    pub fn resume_line(&self) -> Option<String> {
        let open = self.open.lock().expect("session poisoned");
        open.is_resumable()
            .then(|| format!("kemi: resume with: kemi --resume {}", open.id()))
    }

    pub fn set_initial_explanation(
        &self,
        document: kemi_core::domain::explanation::ExplanationDocument,
    ) {
        self.explanation
            .set(document)
            .expect("initial explanation is assigned only once before serving");
    }

    pub fn info(&self) -> SessionInfo {
        self.open.lock().expect("session poisoned").info().clone()
    }

    /// `<id>.files/` に置いたスナップショット（R-PAGE-SESSION）。読めないものは誤りとして返す。
    pub fn read_snapshots(&self) -> Vec<Result<PageSnapshot, SessionError>> {
        self.open.lock().expect("session poisoned").read_snapshots()
    }

    /// `<id>.files/` にあるスナップショットのファイルの最大の番号。読めないものも数える。
    pub fn last_snapshot_number(&self) -> u32 {
        self.open
            .lock()
            .expect("session poisoned")
            .last_snapshot_number()
    }

    /// 保留で終わるとき。会話の無い `--live` のセッションは `<id>.files/` も残さない。
    pub fn close_suspended(&self) -> Result<(), SessionError> {
        self.open
            .lock()
            .expect("session poisoned")
            .close_suspended()
    }

    /// 凍結したレビューを配るソースを作る。復元で読んだ写しをソースへ移すので、作れるのは
    /// 一度だけ。写しが無ければ None。
    pub fn frozen_source(&self) -> Option<FrozenSource> {
        let mut open = self.open.lock().expect("session poisoned");
        let copy = open.take_copy()?;
        Some(FrozenSource::new(open.info(), copy))
    }
}

impl SessionSink for StoredSession {
    fn save_explanation_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.open
            .lock()
            .expect("session poisoned")
            .save_explanation_state(state)
    }
    fn initial_explanation(&self) -> Option<kemi_core::domain::explanation::ExplanationDocument> {
        self.explanation.get().cloned()
    }
    fn initial_title(&self) -> Option<String> {
        Some(
            self.open
                .lock()
                .expect("session poisoned")
                .info()
                .title
                .clone(),
        )
    }
    fn save_frozen_copy(
        &self,
        copy: kemi_core::session::PreparedCopy,
        state: SessionState,
    ) -> Result<bool, SessionError> {
        self.open
            .lock()
            .expect("session poisoned")
            .save_frozen_copy(copy, state)
    }
    fn review_id(&self) -> Option<String> {
        Some(self.open.lock().expect("session poisoned").id().into())
    }
    fn is_resumed(&self) -> bool {
        self.resumed
    }
    fn save_issued_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.open
            .lock()
            .expect("session poisoned")
            .save_issued_state(state)
    }
    fn initial_state(&self) -> SessionState {
        self.open.lock().expect("session poisoned").state().clone()
    }

    /// `--live` のセッションは写しを持たない（R-PAGE-SESSION）。
    fn needs_copy(&self) -> bool {
        let open = self.open.lock().expect("session poisoned");
        !open.info().mode.is_live() && *open.copy() != kemi_core::session::CopyStatus::Ready
    }

    fn describe_review(&self, title: &str, total_files: usize) {
        self.open
            .lock()
            .expect("session poisoned")
            .describe(title, total_files);
    }

    fn save_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.open
            .lock()
            .expect("session poisoned")
            .save_state(state)
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), SessionError> {
        self.open
            .lock()
            .expect("session poisoned")
            .mark_unresumable(reason)
    }

    fn delete(&self) -> Result<(), SessionError> {
        self.open.lock().expect("session poisoned").delete()
    }

    fn save_image(
        &self,
        comment_id: &str,
        bytes: &[u8],
        comments: &BTreeSet<String>,
    ) -> FilesWrite {
        self.open
            .lock()
            .expect("session poisoned")
            .save_image(comment_id, bytes, comments)
    }

    fn save_snapshot(&self, snapshot: &PageSnapshot, comments: &BTreeSet<String>) -> FilesWrite {
        self.open
            .lock()
            .expect("session poisoned")
            .save_snapshot(snapshot, comments)
    }
}

/// 一覧の 1 行（R-SESSION）。5 列のタブ区切り。
pub fn list_line(summary: &SessionSummary) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}/{}",
        summary.id,
        local_time::format_local(summary.updated),
        column(&summary.workspace.to_string_lossy()),
        column(&summary.mode_label()),
        summary.seen,
        summary.total_files,
    )
}

/// 選択画面の行（R-SESSION）。列をそろえた 5 列で、タブと改行は空白にする。
pub fn select_labels(sessions: &[SessionSummary]) -> Vec<String> {
    let rows: Vec<[String; 5]> = sessions
        .iter()
        .map(|summary| {
            [
                column(&summary.id),
                local_time::format_local(summary.updated),
                shorten(&column(&summary.workspace.to_string_lossy()), 48),
                shorten(&column(&summary.mode_label()), 40),
                format!("{}/{}", summary.seen, summary.total_files),
            ]
        })
        .collect();
    let widths: [usize; 5] = rows.iter().fold([0; 5], |mut widths, row| {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.chars().count());
        }
        widths
    });
    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(index, value)| format!("{:<width$}", value, width = widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        })
        .collect()
}

/// 選択画面の列を画面に収めるための短縮。末尾に `...` を付ける。
fn shorten(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut shortened: String = value.chars().take(max.saturating_sub(3)).collect();
    shortened.push_str("...");
    shortened
}

/// 一覧の列の値。タブと改行は空白に置き換える（R-SESSION）。
fn column(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\t' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kemi_core::session::SessionMode;
    use std::path::PathBuf;

    fn summary(id: &str, updated: u128, mode: SessionMode, seen: usize) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            updated,
            workspace: PathBuf::from("/tmp/workspace"),
            mode,
            title: "Working tree changes".to_string(),
            seen,
            total_files: 4,
        }
    }

    #[test]
    fn select_labels_align_the_five_columns_and_sanitize_values() {
        let sessions = vec![
            summary(
                "01HF7YAT00AAAAAAAAAAAAAAAA",
                1_700_000_000_000,
                SessionMode::Worktree,
                1,
            ),
            summary(
                "01HF7YAT00BBBBBBBBBBBBBBBB",
                1_700_000_060_000,
                SessionMode::Manifest,
                2,
            ),
        ];
        let mut listed = sessions.clone();
        listed[1].title = "a\tb\nc".to_string();
        listed[1].mode = SessionMode::Manifest;

        let labels = select_labels(&listed);

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].split("  ").count(), 5, "{}", labels[0]);
        assert_eq!(
            labels[0].split("  ").next().unwrap(),
            "01HF7YAT00AAAAAAAAAAAAAAAA"
        );
        assert!(labels[1].contains("a b c"), "{}", labels[1]);
        assert!(labels[1].contains("2/4"), "{}", labels[1]);
        assert!(!labels[1].contains('\t'));
        assert!(!labels[1].contains('\n'));

        let long = vec![{
            let mut summary = summary(
                "01HF7YAT00CCCCCCCCCCCCCCCC",
                1_700_000_120_000,
                SessionMode::Manifest,
                3,
            );
            summary.title = "x".repeat(100);
            summary
        }];
        let label = select_labels(&long).remove(0);
        assert!(label.contains("..."), "{label}");
        assert!(!label.contains(&"x".repeat(50)), "{label}");
    }
}
