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
