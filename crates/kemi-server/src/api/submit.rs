//! 送信のエンドポイント（`api/submit`、R-SUBMIT）と、結果の JSON の組み立て（R-RESULT）。
//!
//! 送信が通ると、結果を書いてからセッションを消し、最後にサーバを止める（R-SESSION）。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use kemi_core::domain::comment;
use kemi_core::domain::submit::{RESULT_CONTRACT_VERSION, Verdict};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, find_file, side_lines};
use crate::session::{comment_json, delete_stored, message_json};
use crate::{AppState, Notice, Stop, SubmitState};

#[derive(Debug, Deserialize)]
pub(super) struct SubmitRequest {
    verdict: String,
}

pub(super) async fn submit_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<Value>, ApiError> {
    super::ensure_display_persisted(&state)?;
    let verdict = Verdict::parse(&request.verdict)
        .ok_or_else(|| ApiError::bad_request("verdict must be approved or changes_requested"))?;
    {
        let _gate = state.persist.lock().expect("persist poisoned");
        let mut submit_state = state.stopping.submit_state.lock().expect("submit poisoned");
        match *submit_state {
            SubmitState::Open => *submit_state = SubmitState::Claimed,
            SubmitState::Claimed => return Err(ApiError::conflict("already submitted")),
        }
    }

    match build_submit_document(&state, verdict).await {
        Ok(document) => {
            let saved = {
                // suspend が確定済みの submit を返す前に、結果の保存とセッションの削除を終える。
                let _gate = state.persist.lock().expect("persist poisoned");
                {
                    let mut stop = state.stopping.stop.lock().expect("stop poisoned");
                    if matches!(*stop, Some(Stop::Failed(_) | Stop::Suspended)) {
                        return Err(ApiError::conflict("the review is already stopping"));
                    }
                    *stop = Some(Stop::Submitted(document.clone()));
                }
                let saved = save_result(&state, &document);
                // 結果を書いた後にセッションを消し、その後に停止する（R-SESSION）。
                delete_stored(&state);
                saved
            };
            let _ = state.stopping.shutdown.send(true);
            Ok(Json(json!({ "result": document, "saved": saved })))
        }
        Err(error) => {
            *state.stopping.submit_state.lock().expect("submit poisoned") = SubmitState::Open;
            Err(error)
        }
    }
}

/// 確定した結果を結果ファイルに残す。失敗しても submit の結果と終了コードは変えず、
/// 起動側に知らせて完了画面に出すだけにする（R-RESULT）。
fn save_result(state: &AppState, document: &Value) -> Value {
    let Some(sink) = &state.results else {
        return json!({ "dir": null, "path": null, "error": null });
    };
    let text = match serde_json::to_string(document) {
        Ok(json) => format!("{json}\n"),
        Err(error) => {
            return json!({ "dir": sink.location(), "path": null, "error": error.to_string() });
        }
    };
    match sink.save(&text) {
        Ok(path) => json!({
            "dir": sink.location(),
            "path": path.to_string_lossy(),
            "error": null,
        }),
        Err(error) => {
            let saved = json!({ "dir": sink.location(), "path": null, "error": error.to_string() });
            state.notices.notify(Notice::ResultNotSaved(error));
            saved
        }
    }
}

async fn build_submit_document(state: &Arc<AppState>, verdict: Verdict) -> Result<Value, ApiError> {
    let review = state
        .review
        .read()
        .expect("review lock poisoned")
        .startup
        .clone();
    let file_ids: Vec<String> = {
        let session = state.session.lock().expect("session poisoned");
        // ページへのコメントは内容ハッシュを持たず、outdated は常に false（R-SUBMIT）。
        let mut seen = std::collections::HashSet::new();
        session
            .state
            .comments
            .iter()
            .filter_map(|comment| comment.file())
            .filter(|file| seen.insert(file.file_id.as_str()))
            .map(|file| file.file_id.clone())
            .collect()
    };
    for file_id in file_ids {
        // 一覧から消えたファイルは内容を引けないので、全コメントを古い扱いにする。
        if find_file(state, &file_id).is_err() {
            refresh_outdated(state, &file_id, None);
            continue;
        }
        // ファイル全体の行に分けてハッシュを求めるので、行データと同じ枠で async ワーカーの外で行う。
        super::prepare_known_content(state.clone(), file_id, |state, file_id, content| {
            let lines = content.map(|content| (side_lines(&content.old), side_lines(&content.new)));
            refresh_outdated(
                state,
                file_id,
                lines
                    .as_ref()
                    .map(|(old, new)| (old.as_slice(), new.as_slice())),
            );
            Ok(())
        })
        .await?;
    }
    let session = state.session.lock().expect("session poisoned");
    let comments: Vec<Value> = session.state.comments.iter().map(comment_json).collect();
    let messages: Vec<Value> = session.state.messages.iter().map(message_json).collect();
    Ok(json!({
        "kemi": RESULT_CONTRACT_VERSION,
        "diff_display_version":session.state.display.as_ref().map(|d|&d.current),
        "explanation":session.state.explanation,
        "title": review.title,
        "verdict": verdict.as_str(),
        "approval": review.approval.iter().map(|approval| json!({
            "path": approval.path,
            "identity": approval.identity,
        })).collect::<Vec<_>>(),
        "comments": comments,
        "messages": messages,
    }))
}

fn refresh_outdated(state: &AppState, file_id: &str, current: Option<(&[String], &[String])>) {
    comment::refresh_outdated(
        &mut state
            .session
            .lock()
            .expect("session poisoned")
            .state
            .comments,
        file_id,
        current,
    );
}
