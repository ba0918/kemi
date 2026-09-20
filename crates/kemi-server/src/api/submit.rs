//! 送信のエンドポイント（`api/submit`、R-SUBMIT）と、結果の JSON の組み立て（R-RESULT）。
//!
//! 送信が通ると、結果を書いてからセッションを消し、最後にサーバを止める（R-SESSION）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use kemi_core::domain::comment;
use kemi_core::domain::review::Side;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{find_file, side_lines, source_content, ApiError};
use crate::session::{comment_json, delete_stored};
use crate::{AppState, Stop, SubmitState};

#[derive(Debug, Deserialize)]
pub(super) struct SubmitRequest {
    verdict: String,
}

pub(super) async fn submit_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.verdict != "approved" && request.verdict != "changes_requested" {
        return Err(ApiError::bad_request(
            "verdict must be approved or changes_requested",
        ));
    }
    {
        let mut submit_state = state.submit_state.lock().expect("submit poisoned");
        match *submit_state {
            SubmitState::Open => *submit_state = SubmitState::Claimed,
            SubmitState::Claimed => return Err(ApiError::conflict("already submitted")),
        }
    }

    match build_submit_document(&state, &request.verdict).await {
        Ok(document) => {
            {
                let mut stop = state.stop.lock().expect("stop poisoned");
                if matches!(*stop, Some(Stop::Failed(_))) {
                    return Err(ApiError::conflict("stopped after a runtime error"));
                }
                *stop = Some(Stop::Submitted(document.clone()));
            }
            let saved = save_result(&state, &document);
            // 結果を書いた後にセッションを消し、その後に停止する（R-SESSION）。
            delete_stored(&state);
            let _ = state.shutdown.send(true);
            Ok(Json(json!({ "result": document, "saved": saved })))
        }
        Err(error) => {
            *state.submit_state.lock().expect("submit poisoned") = SubmitState::Open;
            Err(error)
        }
    }
}

/// 確定した結果を結果ファイルに残す。失敗しても submit の結果と終了コードは変えず、
/// stderr に警告を出して完了画面に知らせるだけにする（R-RESULT）。
fn save_result(state: &AppState, document: &Value) -> Value {
    let Some(sink) = &state.results else {
        return json!({ "dir": null, "path": null, "error": null });
    };
    let text = match serde_json::to_string(document) {
        Ok(json) => format!("{json}\n"),
        Err(error) => {
            return json!({ "dir": sink.location(), "path": null, "error": error.to_string() })
        }
    };
    match sink.save(&text) {
        Ok(path) => json!({
            "dir": sink.location(),
            "path": path.to_string_lossy(),
            "error": null,
        }),
        Err(error) => {
            eprintln!("kemi: could not save the result file: {error}");
            json!({ "dir": sink.location(), "path": null, "error": error })
        }
    }
}

async fn build_submit_document(state: &AppState, verdict: &str) -> Result<Value, ApiError> {
    let review = state
        .review
        .read()
        .expect("review lock poisoned")
        .startup
        .clone();
    let snapshot: Vec<(String, String, Side, String)> = {
        let session = state.session.lock().expect("session poisoned");
        session
            .comments
            .iter()
            .map(|comment| {
                (
                    comment.id.clone(),
                    comment.file_id.clone(),
                    comment.side,
                    comment.content_hash.clone(),
                )
            })
            .collect()
    };
    let mut outdated = std::collections::HashMap::new();
    for (id, file_id, side, created_hash) in snapshot {
        // 再取得で一覧から消えたファイル（履歴の書き換えで消えたグループのものを含む）は
        // 内容を引けないので古い扱いにする。
        if find_file(state, &file_id).is_err() {
            outdated.insert(id, true);
            continue;
        }
        let Some(content) = source_content(state, &file_id).await? else {
            outdated.insert(id, true);
            continue;
        };
        let lines = match side {
            Side::Old => side_lines(&content.old),
            Side::New => side_lines(&content.new),
        };
        outdated.insert(
            id,
            comment::is_outdated(&created_hash, &comment::content_hash(&lines)),
        );
    }
    let mut session = state.session.lock().expect("session poisoned");
    for comment in session.comments.iter_mut() {
        if let Some(is_outdated) = outdated.get(&comment.id) {
            comment.outdated = *is_outdated;
        }
    }
    let comments: Vec<Value> = session.comments.iter().map(comment_json).collect();
    Ok(json!({
        "kemi": 1,
        "title": review.title,
        "verdict": verdict,
        "approval": review.approval.iter().map(|approval| json!({
            "path": approval.path,
            "identity": approval.identity,
        })).collect::<Vec<_>>(),
        "comments": comments,
    }))
}
