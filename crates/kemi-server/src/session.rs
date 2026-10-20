//! セッション状態（R-SERVE）とセッションへの保存（R-SESSION）。
//! コメント、見た、折りたたみ、解決、返信、発言、往復の続きをメモリに持ち、変更のたびに
//! 保存先へも書く。

use kemi_core::session::SessionState;

use crate::{AppState, Notice};

#[derive(Default, Clone)]
pub struct Session {
    pub state: SessionState,
    /// `channel.events` からこれまでに外した起きたことの数。`kemi wait` の受け取りの
    /// 知らせ（何件目まで受け取ったか）を、重なっても二度外さないための起点。メモリだけに
    /// 置き、復元では 0 から数え直す（それより前の知らせは止まったサーバへのもので届かない）。
    pub received: u64,
}

impl Session {
    /// 保存された状態から読み戻す。
    pub fn from_state(state: SessionState) -> Self {
        Session {
            state: state.prepare_for_restore(),
            received: 0,
        }
    }

    /// 保存する状態の写し。
    pub(crate) fn snapshot(&self) -> SessionState {
        self.state.clone()
    }
}

/// 今の状態を保存先へ書く。失敗は警告だけで、レビューは終わらせない（R-SESSION）。
pub(crate) fn persist(state: &AppState) {
    let Some(sink) = &state.session_sink else {
        return;
    };
    // スナップショットと保存を 1 つずつ進め、遅い保存が新しい状態を上書きしないように
    // する。登録ページと画面が保持する説明の版も守るため、セッション mutex より広い
    // 範囲を守る。呼び出し側はセッション mutex を離してから来るので、ロックは persist →
    // session の一方向に保たれる。
    let _persist = state.persist.lock().expect("persist poisoned");
    let snapshot = state.session.lock().expect("session poisoned").snapshot();
    if let Err(error) = sink.save_state(snapshot) {
        state.notices.notify(Notice::SessionNotSaved(error));
    }
}

/// submit の確定後にセッションを消す。結果ファイルの保存に失敗していても消す（R-SESSION）。
pub(crate) fn delete_stored(state: &AppState) {
    let Some(sink) = &state.session_sink else {
        return;
    };
    if let Err(error) = sink.delete() {
        state.notices.notify(Notice::SessionNotDeleted(error));
    }
}
