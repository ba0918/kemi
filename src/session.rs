//! セッションの保存先の配線（R-SESSION）。サーバへ渡す sink の実装と、CLI の一覧。

use std::sync::Mutex;

use kemi_core::session::{
    OpenSession, SessionCopy, SessionError, SessionInfo, SessionState, SessionStore, SessionSummary,
};
use kemi_core::source::FrozenSource;
use kemi_server::SessionSink;

use crate::local_time;

/// プロセス 1 つ分の開いているセッション。
pub struct StoredSession {
    open: Mutex<OpenSession>,
}

impl StoredSession {
    /// 新しいセッションを始める。ロックを取り、id を決める。
    pub fn start(store: &SessionStore, info: SessionInfo) -> Result<Self, SessionError> {
        Ok(StoredSession {
            open: Mutex::new(store.create(info)?),
        })
    }

    /// 復元できるセッションを開く。ロック中のセッションは `Locked` になる。
    pub fn resume(store: &SessionStore, id: &str) -> Result<Self, SessionError> {
        Ok(StoredSession {
            open: Mutex::new(store.open(id)?),
        })
    }

    pub fn resume_line(&self) -> Option<String> {
        let open = self.open.lock().expect("session poisoned");
        open.is_resumable()
            .then(|| format!("kemi: resume with: kemi --resume {}", open.id()))
    }

    pub fn info(&self) -> SessionInfo {
        self.open.lock().expect("session poisoned").info().clone()
    }

    /// 凍結したレビューを配るソースを作る。写しが無ければ None。
    pub fn frozen_source(&self) -> Option<FrozenSource> {
        let open = self.open.lock().expect("session poisoned");
        match open.copy() {
            kemi_core::session::CopyState::Ready(copy) => {
                Some(FrozenSource::new(open.info(), copy))
            }
            _ => None,
        }
    }
}

impl SessionSink for StoredSession {
    fn initial_state(&self) -> SessionState {
        self.open.lock().expect("session poisoned").state().clone()
    }

    fn describe_review(&self, title: &str, total_files: usize) {
        self.open
            .lock()
            .expect("session poisoned")
            .describe(title, total_files);
    }

    fn save_state(&self, state: SessionState) -> Result<(), String> {
        self.open
            .lock()
            .expect("session poisoned")
            .save_state(state)
            .map_err(|error| error.to_string())
    }

    fn save_copy(&self, copy: SessionCopy) -> Result<(), String> {
        self.open
            .lock()
            .expect("session poisoned")
            .save_copy(copy)
            .map_err(|error| error.to_string())
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), String> {
        self.open
            .lock()
            .expect("session poisoned")
            .mark_unresumable(reason)
            .map_err(|error| error.to_string())
    }

    fn delete(&self) -> Result<(), String> {
        self.open
            .lock()
            .expect("session poisoned")
            .delete()
            .map_err(|error| error.to_string())
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
