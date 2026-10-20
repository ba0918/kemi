//! `kemi --resume`: 保留したセッションの選択・一覧・復元（R-SESSION、R-PAGE-SESSION）。

use std::io::IsTerminal;
use std::sync::Arc;

use kemi_core::session::{PageSnapshot, SessionMode, SessionStore, SessionSummary};
use kemi_core::source::{LiveSource, ReviewSource};
use kemi_server::StoredSession;

use crate::cli::Cli;
use crate::report::{self, fail, warn};
use crate::results::{results_unset_reason, sessions_dir};
use crate::review::{LiveRun, run_review};
use crate::session;

/// `--resume`: セッションを開き、凍結したレビューをサーブする（R-SESSION）。
pub(crate) async fn run_resume(cli: &Cli) -> ! {
    let Some(dir) = sessions_dir() else {
        fail(&format!(
            "cannot determine where sessions are stored ({})",
            results_unset_reason()
        ));
    };
    let store = SessionStore::new(dir);
    let id = match &cli.resume {
        Some(Some(id)) => id.clone(),
        Some(None) => choose_session(&store),
        None => unreachable!("run_resume is called only with --resume"),
    };
    let stored = match StoredSession::resume(&store, &id) {
        Ok(stored) => stored,
        Err(error) => fail(&error.to_string()),
    };
    // `--live` は写しを持たず、今の作業ツリーを読み直し、同じページにつなぎ直す
    // （R-PAGE-SESSION）。
    let info = stored.info();
    let mut live = None;
    let source = match info.mode {
        SessionMode::Live { page, root } => {
            let source = Arc::new(LiveSource::new(&page, &root)) as Arc<dyn ReviewSource>;
            live = Some(LiveRun {
                code_view: kemi_core::source::git::repo_root(&root).is_ok(),
                page,
                root,
                snapshots: restored_snapshots(&stored),
                last_snapshot_number: stored.last_snapshot_number(),
            });
            source
        }
        SessionMode::Worktree
        | SessionMode::Staged
        | SessionMode::Manifest
        | SessionMode::Range { .. } => match stored.frozen_source() {
            Some(source) => Arc::new(source) as Arc<dyn ReviewSource>,
            None => fail(&format!("session {id} cannot be resumed")),
        },
    };
    run_review(
        cli,
        source,
        Some(Arc::new(stored)),
        info.workspace_key,
        live,
        None,
    )
    .await
}

/// `<id>.files/` のスナップショットを読み戻す。読めないものは飛ばして警告し、復元は続ける。
fn restored_snapshots(stored: &StoredSession) -> Vec<PageSnapshot> {
    stored
        .read_snapshots()
        .into_iter()
        .filter_map(|read| match read {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                warn(&format!("could not restore a snapshot: {error}"));
                None
            }
        })
        .collect()
}

/// `id` なしの起動。端末なら選択画面、端末でなければ一覧を出して終わる（R-SESSION）。
fn choose_session(store: &SessionStore) -> String {
    let sessions = match store.list() {
        Ok(sessions) => sessions,
        Err(error) => fail(&error.to_string()),
    };
    if sessions.is_empty() {
        fail(&no_resumable_reason(store));
    }
    if !std::io::stdin().is_terminal() {
        list_sessions(store, &sessions);
    }
    pick_session(&sessions)
}

/// 復元できるセッションが 1 つも無い理由（R-SESSION）。読めない版のセッションしか
/// 無いときは、1 つも保留していない場合と同じ案内で終わらせない。
fn no_resumable_reason(store: &SessionStore) -> String {
    if store.has_unreadable_version() {
        format!(
            "no resumable session: {} holds sessions saved in a format version this kemi cannot read",
            store.dir().display()
        )
    } else {
        "no resumable session".to_string()
    }
}

/// 端末の選択画面。Esc と Ctrl+C では何も変えず終了コード 130（R-SESSION）。
fn pick_session(sessions: &[SessionSummary]) -> String {
    use inquire::{InquireError, Select};
    let labels = session::select_labels(sessions);
    match Select::new("Select a session to resume (Esc cancels)", labels).raw_prompt() {
        Ok(chosen) => sessions[chosen.index].id.clone(),
        Err(InquireError::OperationCanceled) | Err(InquireError::OperationInterrupted) => {
            std::process::exit(report::INTERRUPTED)
        }
        Err(error) => fail(&format!("cannot choose a session: {error}")),
    }
}

/// 復元できるセッションの一覧を stdout に出す（R-SESSION）。0 件なら理由を出して 2。
fn list_sessions(store: &SessionStore, sessions: &[SessionSummary]) -> ! {
    if sessions.is_empty() {
        fail(&no_resumable_reason(store));
    }
    let lines: String = sessions
        .iter()
        .map(|summary| format!("{}\n", session::list_line(summary)))
        .collect();
    report::write_stdout(&lines);
    std::process::exit(report::SUCCESS);
}
