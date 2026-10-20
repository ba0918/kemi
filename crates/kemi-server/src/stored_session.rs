//! core のセッションの保存（`OpenSession`）に書く保存先（R-SESSION）。bin のレビューと
//! サーバのテストが同じものを使う。

use std::collections::BTreeSet;
use std::sync::Mutex;

use kemi_core::session::{
    CopyStatus, FilesWrite, OpenSession, PageSnapshot, PreparedCopy, SessionError, SessionInfo,
    SessionState, SessionStore,
};
use kemi_core::source::FrozenSource;

use crate::SessionSink;

/// プロセス 1 つ分の開いているセッション。
pub struct StoredSession {
    open: Mutex<OpenSession>,
    resumed: bool,
}

impl StoredSession {
    /// 新しいセッションを始める。ロックを取り、id を決める。
    pub fn start(store: &SessionStore, info: SessionInfo) -> Result<Self, SessionError> {
        Ok(StoredSession {
            resumed: false,
            open: Mutex::new(store.create(info)?),
        })
    }

    /// 復元できるセッションを開く。ロック中のセッションは `Locked` になる。
    pub fn resume(store: &SessionStore, id: &str) -> Result<Self, SessionError> {
        Ok(StoredSession {
            resumed: true,
            open: Mutex::new(store.open(id)?),
        })
    }

    /// 復元の対象になるなら、その id（`kemi --resume <id>` の id）。
    pub fn resumable_id(&self) -> Option<String> {
        let open = self.open.lock().expect("session poisoned");
        open.is_resumable().then(|| open.id().to_string())
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
        copy: PreparedCopy,
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
        !open.info().mode.is_live() && *open.copy() != CopyStatus::Ready
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
