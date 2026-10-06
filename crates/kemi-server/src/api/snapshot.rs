//! スナップショットの受け取りと配信（live.md の R-PAGE-SNAPSHOT）。ページが取った HTML を
//! メモリに持ち、比べる相手として返す。`--live` でないレビューには無い（404）。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::AppState;
use crate::live::{LiveInfo, Snapshot, SnapshotKind};

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
        "id": snapshot.id,
        "page": snapshot.page,
        "width": snapshot.width,
        "kind": snapshot.kind.as_str(),
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
    let mut snapshots = live.snapshots.lock().expect("snapshots poisoned");
    let snapshot = Snapshot {
        id: format!("s{}", snapshots.len() + 1),
        page: request.page,
        width: request.width,
        kind,
        html: request.html,
        description: request.description,
    };
    let value = summary(&snapshot);
    snapshots.push(snapshot);
    Ok(Json(value))
}

pub(super) async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let snapshots = live.snapshots.lock().expect("snapshots poisoned");
    Ok(Json(json!({
        "snapshots": snapshots.iter().map(summary).collect::<Vec<_>>(),
    })))
}

pub(super) async fn get_snapshot(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let snapshots = live.snapshots.lock().expect("snapshots poisoned");
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.id == id)
        .ok_or_else(|| ApiError::not_found("no such snapshot"))?;
    let mut value = summary(snapshot);
    value["html"] = json!(snapshot.html);
    value["description"] = snapshot.description.clone().unwrap_or(Value::Null);
    Ok(Json(value))
}
