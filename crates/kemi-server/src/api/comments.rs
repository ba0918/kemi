//! コメントの作成・編集・削除・返信・解決のエンドポイント（`api/comment`、R-COMMENT）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use kemi_core::domain::comment::{self, CommentError};
use kemi_core::domain::review::{Comment, LineRange, Side, Suggestion};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{file_not_found, find_file, parse_side, side_lines, source_content, ApiError};
use crate::session::{comment_json, persist};
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub(super) enum CommentRequest {
    Add {
        file_id: String,
        side: String,
        #[serde(default)]
        start_line: Option<u32>,
        #[serde(default)]
        end_line: Option<u32>,
        body: String,
        #[serde(default)]
        suggestion: Option<String>,
    },
    /// 本文と suggestion を書き換える。suggestion を省くか null にすると外す。
    Edit {
        id: String,
        body: String,
        #[serde(default)]
        suggestion: Option<String>,
    },
    Delete {
        id: String,
    },
    Reply {
        id: String,
        body: String,
    },
    Resolve {
        id: String,
        resolved: bool,
    },
}

pub(super) async fn comment_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<CommentRequest>,
) -> Result<Json<Value>, ApiError> {
    match request {
        CommentRequest::Add {
            file_id,
            side,
            start_line,
            end_line,
            body,
            suggestion,
        } => {
            add_comment(
                &state, file_id, &side, start_line, end_line, body, suggestion,
            )
            .await
        }
        CommentRequest::Edit {
            id,
            body,
            suggestion,
        } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            // 行レンジ・side・quote・作成時の内容ハッシュは変えない（R-COMMENT）。
            let range = comment
                .start_line
                .zip(comment.end_line)
                .map(|(start, end)| LineRange { start, end });
            comment::validate_comment(comment.side, range, suggestion.as_deref())
                .map_err(|error| ApiError::bad_request(comment_error_message(error)))?;
            comment.body = body;
            comment.suggestion = suggestion.map(|replacement| Suggestion { replacement });
            let value = comment_json(comment);
            drop(session);
            persist(&state);
            Ok(Json(value))
        }
        CommentRequest::Delete { id } => {
            let mut session = state.session.lock().expect("session poisoned");
            let index = session
                .comments
                .iter()
                .position(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            // id は再利用しない。採番は last_comment が進むだけで、削除では戻さない。
            session.comments.remove(index);
            drop(session);
            persist(&state);
            Ok(Json(json!({ "id": id, "deleted": true })))
        }
        CommentRequest::Reply { id, body } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            comment.replies.push(body);
            let value = comment_json(comment);
            drop(session);
            persist(&state);
            Ok(Json(value))
        }
        CommentRequest::Resolve { id, resolved } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            comment.resolved = resolved;
            let value = comment_json(comment);
            drop(session);
            persist(&state);
            Ok(Json(value))
        }
    }
}

async fn add_comment(
    state: &AppState,
    file_id: String,
    side: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
    body: String,
    suggestion: Option<String>,
) -> Result<Json<Value>, ApiError> {
    let side = parse_side(side)?;
    let range = match (start_line, end_line) {
        (None, None) => None,
        (Some(start), Some(end)) => Some(LineRange { start, end }),
        _ => {
            return Err(ApiError::bad_request(
                "start_line and end_line must both be given",
            ))
        }
    };
    let (file, group_title) = find_file(state, &file_id)?;

    let content = source_content(state, &file_id)
        .await?
        .ok_or_else(file_not_found)?;
    let lines = match side {
        Side::Old => side_lines(&content.old),
        Side::New => side_lines(&content.new),
    };
    comment::validate_comment(side, range, suggestion.as_deref())
        .map_err(|error| ApiError::bad_request(comment_error_message(error)))?;
    let quote = match range {
        Some(range) => comment::quote_for(&lines, range)
            .map_err(|error| ApiError::bad_request(comment_error_message(error)))?,
        None => Vec::new(),
    };
    let content_hash = comment::content_hash(&lines);

    let mut session = state.session.lock().expect("session poisoned");
    session.last_comment += 1;
    let comment = Comment {
        id: format!("c{}", session.last_comment),
        file_id,
        group_id: file.group_id.clone(),
        group_title,
        path: file.path.clone(),
        side,
        start_line: range.map(|range| range.start),
        end_line: range.map(|range| range.end),
        quote,
        body,
        replies: Vec::new(),
        resolved: false,
        outdated: false,
        content_hash,
        suggestion: suggestion.map(|replacement| Suggestion { replacement }),
    };
    session.comments.push(comment.clone());
    drop(session);
    persist(state);

    let location = match (comment.start_line, comment.end_line) {
        (Some(start), Some(end)) => format!("{start}-{end}"),
        _ => "file-wide".to_string(),
    };
    eprintln!(
        "kemi: comment {} {} {}",
        comment.path,
        comment.side.as_str(),
        location
    );
    Ok(Json(comment_json(&comment)))
}

fn comment_not_found() -> ApiError {
    ApiError::not_found("comment not found")
}

fn comment_error_message(error: CommentError) -> String {
    match error {
        CommentError::LineNumberOutOfRange => "line numbers start at 1".to_string(),
        CommentError::ReversedRange => "line range is reversed".to_string(),
        CommentError::FileWideMustBeNewSide => {
            "file-wide comments can only be on the new side".to_string()
        }
        CommentError::SuggestionRequiresNewSide => {
            "suggestions can only be on new-side line comments".to_string()
        }
        CommentError::SuggestionRequiresRange => "suggestion requires a line range".to_string(),
        CommentError::QuoteOutOfBounds => "line range exceeds the file".to_string(),
    }
}
