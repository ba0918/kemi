//! ルーティングとハンドラ（R-SERVE, R-SUBMIT, R-COMMENT の API）。
//!
//! 内部 API の JSON 形は D7 としてここで決める。

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use kemi_core::domain::comment::{self, CommentError};
use kemi_core::domain::content;
use kemi_core::domain::diff::{self, DisplayRow, Line, Row, Segment};
use kemi_core::domain::review::{Comment, FileEntry, LineRange, ReviewMeta, Side, Suggestion};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_stream::wrappers::BroadcastStream;

use crate::session::{comment_json, Session};
use crate::{AppState, ServeOutcome, ServerError, SubmitState};

/// 1 回の展開要求で返す行数の上限。巨大な折りたたみを一度に読まないため。
const EXPAND_LIMIT: usize = 5_000;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/s/{token}/", get(index))
        .route("/s/{token}/assets/{*path}", get(asset))
        .route("/s/{token}/api/review", get(review))
        .route("/s/{token}/api/file/{id}", get(file))
        .route("/s/{token}/api/comment", post(comment_api))
        .route("/s/{token}/api/state", post(state_api))
        .route("/s/{token}/api/submit", post(submit))
        .route("/s/{token}/api/events", get(events))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, guard))
}

/// token・Host・Origin の検証は本文の解釈より先に行う。
async fn guard(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let token = path
        .strip_prefix("/s/")
        .and_then(|rest| rest.split('/').next());
    if token != Some(state.token.as_str()) {
        return ApiError::not_found("ページが見つかりません").into_response();
    }
    if request.method() == Method::POST {
        if let Err(error) = validate_post(&state, request.headers()) {
            return error.into_response();
        }
    }
    next.run(request).await
}

pub fn session_url(listener: &TcpListener, token: &str) -> Result<String, ServerError> {
    let address = listener.local_addr().map_err(ServerError::Io)?;
    Ok(format!("http://127.0.0.1:{}/s/{token}/", address.port()))
}

pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

fn validate_post(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if host != Some(state.host.as_str()) {
        return Err(ApiError::forbidden("Host が一致しません"));
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if origin != Some(state.origin.as_str()) {
        return Err(ApiError::forbidden("Origin が一致しません"));
    }
    Ok(())
}

async fn source_review(state: &AppState) -> Result<ReviewMeta, ApiError> {
    let source = state.source.clone();
    tokio::task::spawn_blocking(move || source.review())
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

async fn source_content(
    state: &AppState,
    file_id: &str,
) -> Result<kemi_core::source::FileContent, ApiError> {
    let source = state.source.clone();
    let file_id = file_id.to_string();
    tokio::task::spawn_blocking(move || source.content(&file_id))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

async fn index(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Response, ApiError> {
    serve_asset(&state, "index.html")
}

async fn asset(
    State(state): State<Arc<AppState>>,
    Path((_token, path)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    serve_asset(&state, &path)
}

fn serve_asset(state: &AppState, path: &str) -> Result<Response, ApiError> {
    let asset = state
        .assets
        .get(path)
        .ok_or_else(|| ApiError::not_found(format!("{path} が見つかりません")))?;
    let mut response = axum::body::Bytes::from(asset.bytes.into_owned()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(asset.mime),
    );
    Ok(response)
}

async fn review(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let review = source_review(&state).await?;
    *state.meta.write().expect("meta lock poisoned") = Arc::new(review.clone());
    let session = state.session.lock().expect("session poisoned");
    Ok(Json(review_json(&review, &session)))
}

fn review_json(review: &ReviewMeta, session: &Session) -> Value {
    json!({
        "kemi": 1,
        "title": review.title,
        "subtitle": review.subtitle,
        "meta": review.meta,
        "approval": review.approval.iter().map(|approval| json!({
            "path": approval.path,
            "identity": approval.identity,
        })).collect::<Vec<_>>(),
        "groups": review.groups.iter().map(|group| json!({
            "id": group.id,
            "title": group.title,
            "why": group.why,
            "watch": group.watch,
            "files": group.files.iter().map(|file| json!({
                "id": file.id,
                "path": file.path,
                "old_path": file.old_path,
                "status": file.status.as_str(),
                "add": file.add,
                "del": file.del,
                "binary": file.binary,
                "old_size": file.old_size,
                "new_size": file.new_size,
                "focus": file.focus,
                "note": file.note,
                "noise": file.noise,
                "seen": session.seen.contains(&file.id),
                "collapsed": session
                    .collapsed
                    .get(&file.id)
                    .copied()
                    .unwrap_or(file.noise),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[derive(Debug, Deserialize)]
struct ExpandQuery {
    #[serde(default)]
    from: Option<usize>,
    #[serde(default)]
    to: Option<usize>,
}

async fn file(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Json<Value>, ApiError> {
    let review = state.meta.read().expect("meta lock poisoned").clone();
    let file =
        find_file(&review, &id).ok_or_else(|| ApiError::not_found("ファイルが見つかりません"))?;

    if file.binary {
        return Ok(Json(json!({
            "id": id,
            "binary": true,
            "old_size": file.old_size,
            "new_size": file.new_size,
            "rows": [],
        })));
    }

    let content = source_content(&state, &id).await?;
    let old_lines = side_lines(&content.old);
    let new_lines = side_lines(&content.new);
    let rows = diff::align(&old_lines, &new_lines);

    update_outdated(&state, &id, &old_lines, &new_lines);

    if let (Some(from), Some(to)) = (query.from, query.to) {
        let from = from.min(rows.len());
        let to = to.min(rows.len()).max(from);
        let end = to.min(from + EXPAND_LIMIT);
        let slice: Vec<Value> = rows[from..end].iter().map(row_json).collect();
        let next = if end < to { Some(end) } else { None };
        return Ok(Json(json!({
            "id": id,
            "binary": false,
            "rows": slice,
            "next": next,
        })));
    }

    let display = diff::collapse(&rows, diff::DEFAULT_CONTEXT);
    let rows_json = display
        .iter()
        .map(|row| display_row_json(row, &rows))
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "id": id,
        "binary": false,
        "old_total": old_lines.len(),
        "new_total": new_lines.len(),
        "rows": rows_json,
    })))
}

fn update_outdated(state: &AppState, file_id: &str, old_lines: &[String], new_lines: &[String]) {
    let mut session = state.session.lock().expect("session poisoned");
    for comment in session
        .comments
        .iter_mut()
        .filter(|comment| comment.file_id == file_id)
    {
        let lines = match comment.side {
            Side::Old => old_lines,
            Side::New => new_lines,
        };
        comment.outdated =
            comment::is_outdated(&comment.content_hash, &comment::content_hash(lines));
    }
}

fn row_json(row: &Row) -> Value {
    match row.kind {
        diff::RowKind::Equal => json!({
            "kind": "equal",
            "old": line_json(row.old.as_ref()),
            "new": line_json(row.new.as_ref()),
        }),
        diff::RowKind::Insert => json!({
            "kind": "insert",
            "new": line_json(row.new.as_ref()),
        }),
        diff::RowKind::Delete => json!({
            "kind": "delete",
            "old": line_json(row.old.as_ref()),
        }),
        diff::RowKind::Replace => json!({
            "kind": "replace",
            "old": line_json(row.old.as_ref()),
            "new": line_json(row.new.as_ref()),
            "old_segments": segments_json(&row.old_segments),
            "new_segments": segments_json(&row.new_segments),
        }),
    }
}

fn display_row_json(row: &DisplayRow, rows: &[Row]) -> Value {
    match row {
        DisplayRow::Diff(row) => row_json(row),
        DisplayRow::Skip(skip) => {
            let (old_start, new_start) = rows
                .get(skip.from)
                .map(|row| {
                    (
                        row.old.as_ref().map(|line| line.number),
                        row.new.as_ref().map(|line| line.number),
                    )
                })
                .unwrap_or((None, None));
            json!({
                "kind": "skip",
                "count": skip.to - skip.from,
                "from": skip.from,
                "to": skip.to,
                "old_start": old_start,
                "new_start": new_start,
            })
        }
    }
}

fn line_json(line: Option<&Line>) -> Value {
    match line {
        Some(line) => json!({ "number": line.number, "text": line.text }),
        None => Value::Null,
    }
}

fn segments_json(segments: &[Segment]) -> Value {
    Value::Array(
        segments
            .iter()
            .map(|segment| json!({ "text": segment.text, "changed": segment.changed }))
            .collect(),
    )
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum CommentRequest {
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
    Reply {
        id: String,
        body: String,
    },
    Resolve {
        id: String,
        resolved: bool,
    },
}

async fn comment_api(
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
        CommentRequest::Reply { id, body } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(|| ApiError::not_found("コメントが見つかりません"))?;
            comment.replies.push(body);
            let value = comment_json(comment);
            drop(session);
            Ok(Json(value))
        }
        CommentRequest::Resolve { id, resolved } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(|| ApiError::not_found("コメントが見つかりません"))?;
            comment.resolved = resolved;
            let value = comment_json(comment);
            drop(session);
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
                "start_line と end_line は両方指定してください",
            ))
        }
    };
    let review = state.meta.read().expect("meta lock poisoned").clone();
    let file = find_file(&review, &file_id)
        .ok_or_else(|| ApiError::not_found("ファイルが見つかりません"))?;
    let group_title = group_title(&review, &file.group_id);

    let content = source_content(state, &file_id).await?;
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
    session.next_comment += 1;
    let comment = Comment {
        id: format!("c{}", session.next_comment),
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

    let location = match (comment.start_line, comment.end_line) {
        (Some(start), Some(end)) => format!("{start}-{end}"),
        _ => "ファイル全体".to_string(),
    };
    eprintln!(
        "kemi: コメント {} {} {}",
        comment.path,
        comment.side.as_str(),
        location
    );
    Ok(Json(comment_json(&comment)))
}

fn parse_side(side: &str) -> Result<Side, ApiError> {
    match side {
        "new" => Ok(Side::New),
        "old" => Ok(Side::Old),
        _ => Err(ApiError::bad_request("side は new か old です")),
    }
}

fn comment_error_message(error: CommentError) -> String {
    match error {
        CommentError::LineNumberOutOfRange => "行番号は 1 始まりです".to_string(),
        CommentError::ReversedRange => "行レンジが逆転しています".to_string(),
        CommentError::FileWideMustBeNewSide => {
            "ファイル全体のコメントは新側にだけ付けられます".to_string()
        }
        CommentError::SuggestionRequiresNewSide => {
            "suggestion は新側の行コメントにだけ付けられます".to_string()
        }
        CommentError::SuggestionRequiresRange => "suggestion には行レンジが必要です".to_string(),
        CommentError::QuoteOutOfBounds => "行レンジがファイルの範囲を超えています".to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct StateRequest {
    file_id: String,
    #[serde(default)]
    seen: Option<bool>,
    #[serde(default)]
    collapsed: Option<bool>,
}

async fn state_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<StateRequest>,
) -> Result<Json<Value>, ApiError> {
    {
        let review = state.meta.read().expect("meta lock poisoned").clone();
        if find_file(&review, &request.file_id).is_none() {
            return Err(ApiError::not_found("ファイルが見つかりません"));
        }
    }
    let mut session = state.session.lock().expect("session poisoned");
    if let Some(seen) = request.seen {
        if seen {
            session.seen.insert(request.file_id.clone());
        } else {
            session.seen.remove(&request.file_id);
        }
    }
    if let Some(collapsed) = request.collapsed {
        session.collapsed.insert(request.file_id.clone(), collapsed);
    }
    Ok(Json(json!({
        "id": request.file_id,
        "seen": session.seen.contains(&request.file_id),
        "collapsed": session
            .collapsed
            .get(&request.file_id)
            .copied()
            .unwrap_or(false),
    })))
}

#[derive(Debug, Deserialize)]
struct SubmitRequest {
    verdict: String,
}

async fn submit(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.verdict != "approved" && request.verdict != "changes_requested" {
        return Err(ApiError::bad_request(
            "verdict は approved か changes_requested です",
        ));
    }
    {
        let mut submit_state = state.submit_state.lock().expect("submit poisoned");
        match *submit_state {
            SubmitState::Open => *submit_state = SubmitState::Claimed,
            SubmitState::Claimed => return Err(ApiError::conflict("既に送信されています")),
        }
    }

    match build_submit_document(&state, &request.verdict).await {
        Ok(document) => {
            *state.outcome.lock().expect("outcome poisoned") =
                Some(ServeOutcome::Submitted(document.clone()));
            state.shutdown.notify_one();
            Ok(Json(document))
        }
        Err(error) => {
            *state.submit_state.lock().expect("submit poisoned") = SubmitState::Open;
            Err(error)
        }
    }
}

async fn build_submit_document(state: &AppState, verdict: &str) -> Result<Value, ApiError> {
    let review = state.meta.read().expect("meta lock poisoned").clone();
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
        let content = source_content(state, &file_id).await?;
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

async fn events(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let receiver = state.events.subscribe();
    let stream = BroadcastStream::new(receiver)
        .map(|_| Ok::<_, Infallible>(Event::default().event("update").data("{}")));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn find_file<'a>(review: &'a ReviewMeta, id: &str) -> Option<&'a FileEntry> {
    review
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .find(|file| file.id == id)
}

fn group_title(review: &ReviewMeta, group_id: &str) -> String {
    review
        .groups
        .iter()
        .find(|group| group.id == group_id)
        .map(|group| group.title.clone())
        .unwrap_or_default()
}

fn side_lines(bytes: &Option<Vec<u8>>) -> Vec<String> {
    bytes
        .as_deref()
        .map(|bytes| content::lines(&String::from_utf8_lossy(bytes)))
        .unwrap_or_default()
}
