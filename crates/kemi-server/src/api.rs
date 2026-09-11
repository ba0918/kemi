//! ルーティングとハンドラ（R-SERVE, R-SUBMIT, R-COMMENT の API）。
//!
//! 内部 API の JSON 形は D7 としてここで決める。

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use kemi_core::domain::comment::{self, CommentError};
use kemi_core::domain::content;
use kemi_core::domain::diff::{self, DisplayRow, Line, Row, Segment};
use kemi_core::domain::origin::Unknown;
use kemi_core::domain::review::{
    Comment, FileEntry, GroupBy, LineRange, ReviewMeta, Side, Suggestion,
};
use kemi_core::source::{FileOrigin, SourceError};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_stream::wrappers::BroadcastStream;

use crate::highlight::{self, HighlightedLine, Highlighter};
use crate::session::{comment_json, Session};
use crate::units::{self, Unavailable};
use crate::{stop_with_error, AppState, Event, ServerError, Stop, SubmitState};

/// 1 ファイル分の左右のハイライト結果。
struct Highlighted {
    old: Vec<HighlightedLine>,
    new: Vec<HighlightedLine>,
}

/// 1 回の展開要求で返す行数の上限。巨大な折りたたみを一度に読まないため。
const EXPAND_LIMIT: usize = 5_000;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/s/{token}/", get(index))
        .route("/s/{token}/assets/{*path}", get(asset))
        .route("/s/{token}/api/review", get(review))
        .route("/s/{token}/api/file/{id}", get(file))
        .route("/s/{token}/api/origin/{id}", get(origin))
        .route("/s/{token}/api/unit", post(unit_api))
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
        .map_err(|error| runtime_error(state, error))?
        .map_err(|error| runtime_error(state, error))
}

/// 内容を読む。一覧にある id でも、再取得や裏での単位の作り直しで内容の計画が先に
/// 差し替わると、内容の側はその id をもう知らないことがある。これは git や I/O の
/// 失敗ではないので、レビューは終えず None を返す（一覧に無い id と同じ扱い）。
async fn source_content(
    state: &AppState,
    file_id: &str,
) -> Result<Option<kemi_core::source::FileContent>, ApiError> {
    let source = state.source.clone();
    let file_id = file_id.to_string();
    let result = tokio::task::spawn_blocking(move || source.content(&file_id))
        .await
        .map_err(|error| runtime_error(state, error))?;
    match result {
        Ok(content) => Ok(Some(content)),
        Err(SourceError::UnknownFileId(_)) => Ok(None),
        Err(error) => Err(runtime_error(state, error)),
    }
}

/// レビュー中の git・I/O 失敗はサーバを止め、CLI を終了コード 2 にする（R-SUBMIT）。
fn runtime_error(state: &AppState, error: impl std::fmt::Display) -> ApiError {
    let message = error.to_string();
    stop_with_error(state, message.clone());
    ApiError::internal(message)
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
    serve_asset(&state, &format!("assets/{path}"))
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

#[derive(Debug, Deserialize)]
struct ReviewQuery {
    #[serde(default)]
    refresh: Option<String>,
    /// `file` か `commit`。省略時は起動時の単位。
    #[serde(default)]
    unit: Option<String>,
}

fn parse_unit(unit: &str) -> Result<GroupBy, ApiError> {
    match unit {
        "file" => Ok(GroupBy::File),
        "commit" => Ok(GroupBy::Commit),
        _ => Err(ApiError::bad_request("unit は file か commit です")),
    }
}

async fn review(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Query(query): Query<ReviewQuery>,
) -> Result<Json<Value>, ApiError> {
    let unit = query.unit.as_deref().map(parse_unit).transpose()?;
    if query.refresh.as_deref() == Some("1") {
        let _guard = state.refresh.lock().await;
        let startup = Arc::new(source_review(&state).await?);
        state.review.write().expect("review lock poisoned").startup = startup;
        units::refresh_other(&state).await;
    }
    let body = {
        let review = state.review.read().expect("review lock poisoned");
        let (shown, meta) = review.meta(unit).map_err(|reason| match reason {
            Unavailable::NoSuchUnit => ApiError::not_found("その単位はありません"),
            Unavailable::NotReady => ApiError::conflict("その単位はまだ作っていません"),
        })?;
        let session = state.session.lock().expect("session poisoned");
        let mut body = review_json(meta.as_ref(), &session);
        body["unit"] = json!(shown.map(GroupBy::as_str));
        body["units"] = review.units_json();
        body
    };
    // 起動時の単位を返した後に、もう片方を裏で作り始める（R-UNIT, R-SERVE）。
    units::start_if_waiting(&state);
    Ok(Json(body))
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
        "comments": session.comments.iter().map(comment_json).collect::<Vec<_>>(),
    })
}

#[derive(Debug, Deserialize)]
struct ExpandQuery {
    #[serde(default)]
    from: Option<usize>,
    #[serde(default)]
    to: Option<usize>,
    /// "on" で上限を無視して有効化、"off" で無効化。
    #[serde(default)]
    highlight: Option<String>,
    /// "1" で暗いテーマ。
    #[serde(default)]
    dark: Option<String>,
}

async fn file(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Json<Value>, ApiError> {
    let (file, _) = find_file(&state, &id)?;

    let dark = query.dark.as_deref() == Some("1");

    if file.binary {
        return Ok(Json(json!({
            "id": id,
            "binary": true,
            "old_size": file.old_size,
            "new_size": file.new_size,
            "rows": [],
            "comments": comments_for(&state, &id),
            "highlight": { "capable": false, "enabled": false, "dark": dark },
        })));
    }

    let content = source_content(&state, &id)
        .await?
        .ok_or_else(file_not_found)?;
    let old_text = content
        .old
        .as_deref()
        .map(|bytes| content::normalize(&String::from_utf8_lossy(bytes)));
    let new_text = content
        .new
        .as_deref()
        .map(|bytes| content::normalize(&String::from_utf8_lossy(bytes)));
    let old_lines = side_lines(&content.old);
    let new_lines = side_lines(&content.new);
    let rows = diff::align(&old_lines, &new_lines);

    let capable = Highlighter::capable(old_text.as_deref(), new_text.as_deref());
    let forced = query.highlight.as_deref() == Some("on");
    let enabled = forced || (capable && query.highlight.as_deref() != Some("off"));
    let highlighted = enabled.then(|| {
        let highlighter = state.highlighter.get_or_init(Highlighter::new);
        Highlighted {
            old: highlighter.highlight(&file.path, old_text.as_deref().unwrap_or(""), dark),
            new: highlighter.highlight(&file.path, new_text.as_deref().unwrap_or(""), dark),
        }
    });
    let highlight_info = json!({ "capable": capable, "enabled": enabled, "dark": dark });

    update_outdated(&state, &id, &old_lines, &new_lines);

    if let (Some(from), Some(to)) = (query.from, query.to) {
        let from = from.min(rows.len());
        let to = to.min(rows.len()).max(from);
        let end = to.min(from + EXPAND_LIMIT);
        let slice: Vec<Value> = rows[from..end]
            .iter()
            .map(|row| row_json(row, highlighted.as_ref()))
            .collect();
        let next = if end < to { Some(end) } else { None };
        return Ok(Json(json!({
            "id": id,
            "binary": false,
            "rows": slice,
            "next": next,
            "comments": comments_for(&state, &id),
            "highlight": highlight_info,
        })));
    }

    // 行コメントの付いた行は、変更の行と同じく畳まない。止まる場所と吹き出しは見えている
    // 行にしか置けないので、畳むと、展開したかどうかで止まる場所の数が変わり、コメント
    // 一覧からも移れなくなる（R-NAV, R-VIEW）。
    let (old_commented, new_commented) = commented_lines(&state, &id);
    let display = diff::collapse(&rows, diff::DEFAULT_CONTEXT, |row| {
        row.old
            .as_ref()
            .is_some_and(|line| old_commented.contains(&line.number))
            || row
                .new
                .as_ref()
                .is_some_and(|line| new_commented.contains(&line.number))
    });
    let rows_json = display
        .iter()
        .map(|row| display_row_json(row, &rows, highlighted.as_ref()))
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "id": id,
        "binary": false,
        "old_total": old_lines.len(),
        "new_total": new_lines.len(),
        "rows": rows_json,
        "comments": comments_for(&state, &id),
        "highlight": highlight_info,
    })))
}

#[derive(Debug, Deserialize)]
struct OriginQuery {
    /// "1" で上限を超えるファイルでも由来を求める。
    #[serde(default)]
    force: Option<String>,
}

/// 最終形のファイルの由来（R-ORIGIN）。差分とは別に、表示の後から取りに来る。
///
/// 由来の計算の失敗はレビューを終えない（R-SUBMIT の例外）。そのファイルの変更
/// ブロックはどれもコミットを持たない由来として返し、画面は「特定できない」と出す。
async fn origin(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
    Query(query): Query<OriginQuery>,
) -> Result<Json<Value>, ApiError> {
    let (file, _) = find_file(&state, &id)?;
    let force = query.force.as_deref() == Some("1");
    let source = state.source.clone();
    let file_id = id.clone();
    let origin = match tokio::task::spawn_blocking(move || source.origin(&file_id, force)).await {
        Ok(Ok(origin)) => origin,
        // 内容の計画が差し替わる途中の食い違い（source_content と同じ）。
        Ok(Err(SourceError::UnknownFileId(_))) => return Err(file_not_found()),
        Ok(Err(error)) => Some(unknown_origin(&file.path, error)),
        Err(error) => Some(unknown_origin(&file.path, error)),
    };
    Ok(Json(match origin {
        Some(origin) => origin_json(&id, &origin),
        None => json!({ "id": id, "available": false }),
    }))
}

/// 計算に失敗したファイルの由来。どの変更ブロックもコミットを持たない。
fn unknown_origin(path: &str, error: impl std::fmt::Display) -> FileOrigin {
    eprintln!("kemi: {path} の由来を求められませんでした: {error}");
    FileOrigin {
        enabled: true,
        blocks: Vec::new(),
        commits: Vec::new(),
    }
}

fn origin_json(id: &str, origin: &FileOrigin) -> Value {
    let commits: serde_json::Map<String, Value> = origin
        .commits
        .iter()
        .map(|commit| {
            (
                commit.sha.clone(),
                json!({ "subject": commit.subject, "body": commit.body, "merge": commit.merge }),
            )
        })
        .collect();
    json!({
        "id": id,
        "available": true,
        "enabled": origin.enabled,
        "blocks": origin.blocks.iter().map(|block| json!({
            "row": block.row,
            "unknown": match block.unknown {
                Unknown::None => "none",
                Unknown::Some => "some",
                Unknown::All => "all",
            },
            "entries": block.entries.iter().map(|entry| json!({
                "sha": entry.sha,
                "merge": entry.merge,
                "target": entry.target.as_ref().map(|target| json!({
                    "path": target.path,
                    "side": target.side.as_str(),
                    "line": target.line,
                })),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "commits": commits,
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum UnitRequest {
    /// 作れなかった単位を作り直す。
    Retry { unit: String },
}

async fn unit_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<UnitRequest>,
) -> Result<Json<Value>, ApiError> {
    let UnitRequest::Retry { unit } = request;
    if !units::retry(&state, parse_unit(&unit)?) {
        return Err(ApiError::conflict("作り直せる状態ではありません"));
    }
    let units = state
        .review
        .read()
        .expect("review lock poisoned")
        .units_json();
    Ok(Json(json!({ "units": units })))
}

fn comments_for(state: &AppState, file_id: &str) -> Vec<Value> {
    let session = state.session.lock().expect("session poisoned");
    session
        .comments
        .iter()
        .filter(|comment| comment.file_id == file_id)
        .map(comment_json)
        .collect()
}

/// そのファイルの行コメントが範囲に含む行番号（旧側、新側）。
fn commented_lines(state: &AppState, file_id: &str) -> (HashSet<u32>, HashSet<u32>) {
    let session = state.session.lock().expect("session poisoned");
    let (mut old, mut new) = (HashSet::new(), HashSet::new());
    for comment in session
        .comments
        .iter()
        .filter(|comment| comment.file_id == file_id)
    {
        let Some(start) = comment.start_line else {
            continue;
        };
        let end = comment.end_line.unwrap_or(start);
        match comment.side {
            Side::Old => old.extend(start..=end),
            Side::New => new.extend(start..=end),
        }
    }
    (old, new)
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

fn row_json(row: &Row, highlighted: Option<&Highlighted>) -> Value {
    let old_highlight = highlighted.map(|highlight| highlight.old.as_slice());
    let new_highlight = highlighted.map(|highlight| highlight.new.as_slice());
    match row.kind {
        diff::RowKind::Equal => json!({
            "kind": "equal",
            "old": line_json(row.old.as_ref(), old_highlight, &[]),
            "new": line_json(row.new.as_ref(), new_highlight, &[]),
        }),
        diff::RowKind::Insert => json!({
            "kind": "insert",
            "new": line_json(row.new.as_ref(), new_highlight, &[]),
        }),
        diff::RowKind::Delete => json!({
            "kind": "delete",
            "old": line_json(row.old.as_ref(), old_highlight, &[]),
        }),
        diff::RowKind::Replace => json!({
            "kind": "replace",
            "old": line_json(row.old.as_ref(), old_highlight, &row.old_segments),
            "new": line_json(row.new.as_ref(), new_highlight, &row.new_segments),
            "old_segments": segments_json(&row.old_segments),
            "new_segments": segments_json(&row.new_segments),
        }),
    }
}

fn display_row_json(row: &DisplayRow, rows: &[Row], highlighted: Option<&Highlighted>) -> Value {
    match row {
        DisplayRow::Diff(row) => row_json(row, highlighted),
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

fn line_json(
    line: Option<&Line>,
    highlighted: Option<&[HighlightedLine]>,
    segments: &[Segment],
) -> Value {
    match line {
        Some(line) => {
            let mut value = json!({ "number": line.number, "text": line.text });
            if let Some(ranges) = highlighted.and_then(|lines| lines.get(line.number as usize - 1))
            {
                value["html"] = Value::String(highlight::render_line(ranges, segments));
            }
            value
        }
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
            Ok(Json(comment_json(comment)))
        }
        CommentRequest::Delete { id } => {
            let mut session = state.session.lock().expect("session poisoned");
            let index = session
                .comments
                .iter()
                .position(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            // id は再利用しない。採番は next_comment が進むだけで、削除では戻さない。
            session.comments.remove(index);
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

fn comment_not_found() -> ApiError {
    ApiError::not_found("コメントが見つかりません")
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
    find_file(&state, &request.file_id)?;
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
            {
                let mut stop = state.stop.lock().expect("stop poisoned");
                if matches!(*stop, Some(Stop::Failed(_))) {
                    return Err(ApiError::conflict("実行時エラーで停止しています"));
                }
                *stop = Some(Stop::Submitted(document.clone()));
            }
            let saved = save_result(&state, &document);
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
            eprintln!("kemi: 結果ファイルを保存できませんでした: {error}");
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

async fn events(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    let receiver = state.events.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    // submit 後の graceful shutdown は接続が閉じるまで待つので、SSE は停止通知で終端する。
    let stop = async move {
        loop {
            if *shutdown.borrow() {
                break;
            }
            if shutdown.changed().await.is_err() {
                break;
            }
        }
    };
    let stream = BroadcastStream::new(receiver)
        .take_until(stop)
        .filter_map(|event| async move {
            match event {
                Ok(Event::Update) => Some(Ok::<_, Infallible>(
                    SseEvent::default().event("update").data("{}"),
                )),
                Ok(Event::Unit) => Some(Ok(SseEvent::default().event("unit").data("{}"))),
                // 取りこぼした通知は、更新があったものとして知らせる。
                Err(_) => Some(Ok(SseEvent::default().event("update").data("{}"))),
            }
        });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// どちらかのグループ単位にあるファイルと、そのグループの title。
fn find_file(state: &AppState, id: &str) -> Result<(FileEntry, String), ApiError> {
    state
        .review
        .read()
        .expect("review lock poisoned")
        .find_file(id)
        .ok_or_else(file_not_found)
}

fn file_not_found() -> ApiError {
    ApiError::not_found("ファイルが見つかりません")
}

fn side_lines(bytes: &Option<Vec<u8>>) -> Vec<String> {
    bytes
        .as_deref()
        .map(|bytes| content::lines(&String::from_utf8_lossy(bytes)))
        .unwrap_or_default()
}
