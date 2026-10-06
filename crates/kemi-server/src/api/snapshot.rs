//! スナップショットの受け取りと配信（live.md の R-PAGE-SNAPSHOT）。ページが取った HTML を
//! 比べる相手として返す。セッションがあれば `<id>.files/` にも書き、保留と復元をまたぐ
//! （R-PAGE-SESSION）。`--live` でないレビューには無い（404）。

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use kemi_core::session::{FileWritten, PageSnapshot, SnapshotKind};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::live::{LiveInfo, Snapshot};
use crate::{AppState, Event, Notice};

/// 1 つの上限（R-PAGE-SNAPSHOT）。超えるものは取らない。
pub(super) const SNAPSHOT_LIMIT: usize = 2 * 1024 * 1024;

/// 要求の本文の上限。JSON の文字列にすると HTML は伸びるので、上限より広く取り、
/// 上限は HTML のバイト数で見る。
pub(super) const SNAPSHOT_BODY_LIMIT: usize = 4 * SNAPSHOT_LIMIT;

#[derive(Debug, Deserialize)]
pub(super) struct SnapshotRequest {
    page: String,
    width: u32,
    kind: String,
    html: String,
    /// 2 MB の上限は HTML（DOM と CSS の記録）にだけ掛け、記述には掛けない。
    #[serde(default)]
    description: Option<Value>,
}

fn live(state: &AppState) -> Result<&LiveInfo, ApiError> {
    state
        .live
        .as_deref()
        .ok_or_else(|| ApiError::not_found("snapshots exist only in a review of a running page"))
}

fn summary(snapshot: &Snapshot) -> Value {
    json!({
        "id": snapshot.id(),
        "page": snapshot.record.page,
        "width": snapshot.record.width,
        "kind": snapshot.record.kind.as_str(),
        "unsaved": snapshot.unsaved,
    })
}

pub(super) async fn take_snapshot(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<SnapshotRequest>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let kind = SnapshotKind::parse(&request.kind)
        .ok_or_else(|| ApiError::unprocessable("kind must be start, handed or manual"))?;
    if request.html.len() > SNAPSHOT_LIMIT {
        return Err(ApiError::too_large("the snapshot is larger than 2 MB"));
    }
    // コメントの画像の書き込みと同じロックで、`<id>.files/` の規則を 1 つずつ掛ける。
    // ロックは persist → session → snapshots の順。
    let _persist = state.persist.lock().expect("persist poisoned");
    let number = {
        let mut snapshots = live.snapshots.lock().expect("snapshots poisoned");
        snapshots.last_number += 1;
        snapshots.last_number
    };
    let record = PageSnapshot {
        number,
        kind,
        page: request.page,
        width: request.width,
        html: request.html,
        description: request.description,
    };
    let (unsaved, removed) = match &state.session_sink {
        Some(sink) => {
            let write = sink.save_snapshot(&record, &comment_ids(&state));
            let unsaved = match write.written {
                FileWritten::Saved(_) => false,
                FileWritten::NoRoom => true,
                // 書き込みの失敗も、保存していないことは同じなので同じ印を付ける。
                FileWritten::Failed(error) => {
                    state.notices.notify(Notice::SnapshotNotSaved(error));
                    true
                }
            };
            (unsaved, write.removed_snapshots)
        }
        None => (false, Vec::new()),
    };
    let snapshot = Snapshot { record, unsaved };
    let value = summary(&snapshot);
    live.snapshots
        .lock()
        .expect("snapshots poisoned")
        .taken
        .push(snapshot);
    // 一覧の読み直しを促すのは、新しいものを一覧に入れてから。先に知らせると、読み直した
    // 一覧に新しいものが無いことがある。
    forget_removed(&state, live, &removed);
    Ok(Json(value))
}

/// 今あるコメントの id。その画像は 20 MB の規則で消さない（R-PAGE-SESSION）。
pub(super) fn comment_ids(state: &AppState) -> BTreeSet<String> {
    state
        .session
        .lock()
        .expect("session poisoned")
        .comments
        .iter()
        .map(|comment| comment.id.clone())
        .collect()
}

/// 20 MB の規則で `<id>.files/` から消したスナップショットを選択肢からも消し、ページに
/// 一覧を読み直させる（R-PAGE-SESSION）。
pub(super) fn forget_removed(state: &AppState, live: &LiveInfo, removed: &[u32]) {
    if removed.is_empty() {
        return;
    }
    live.snapshots
        .lock()
        .expect("snapshots poisoned")
        .forget(removed);
    let _ = state.events.send(Event::Snapshots);
}

pub(super) async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let snapshots = live.snapshots.lock().expect("snapshots poisoned");
    Ok(Json(json!({
        "snapshots": snapshots.taken.iter().map(summary).collect::<Vec<_>>(),
    })))
}

pub(super) async fn get_snapshot(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let snapshots = live.snapshots.lock().expect("snapshots poisoned");
    let snapshot = snapshots
        .taken
        .iter()
        .find(|snapshot| snapshot.id() == id)
        .ok_or_else(|| ApiError::not_found("no such snapshot"))?;
    let mut value = summary(snapshot);
    value["html"] = json!(snapshot.record.html);
    value["description"] = snapshot.record.description.clone().unwrap_or(Value::Null);
    Ok(Json(value))
}
