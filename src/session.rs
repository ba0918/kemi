//! セッションの保存先の配線（R-SESSION）。サーバへ渡す sink の実装。

use std::sync::Mutex;

use kemi_core::session::{
    OpenSession, SessionCopy, SessionError, SessionInfo, SessionState, SessionStore,
};
use kemi_server::SessionSink;

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
