//! ルーティングとハンドラ（R-SERVE, R-SUBMIT, R-COMMENT の API）。
//!
//! 内部 API の JSON 形は D7 として、この階層（`api` と子モジュール）で決める。

mod comments;
mod file;
mod rendered;
mod submit;

use std::convert::Infallible;
use std::net::Ipv4Addr;
use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use kemi_core::domain::content;
use kemi_core::domain::origin::Unknown;
use kemi_core::domain::review::{FileEntry, GroupBy, ReviewMeta, Side};
use kemi_core::source::{FileOrigin, SourceError};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_stream::wrappers::BroadcastStream;

use self::comments::comment_api;
use self::file::file;
use self::rendered::{render_file, repository_image, review_image};
use self::submit::submit;
use crate::session::{comment_json, persist, start_freeze, Session};
use crate::units::{self, Unavailable};
use crate::{stop_with_error, AppState, Event, ServerError};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/s/{token}/", get(index))
        .route("/s/{token}/assets/{*path}", get(asset))
        .route("/s/{token}/api/review", get(review))
        .route("/s/{token}/api/file/{id}", get(file))
        .route("/s/{token}/api/origin/{id}", get(origin))
        .route("/s/{token}/api/render/{id}", get(render_file))
        .route("/s/{token}/api/image/review/{id}/{side}", get(review_image))
        .route(
            "/s/{token}/api/image/repo/{id}/{side}/{*path}",
            get(repository_image),
        )
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
        return ApiError::not_found("page not found").into_response();
    }
    if request.method() == Method::POST {
        if let Err(error) = validate_post(&state, request.headers()) {
            return error.into_response();
        }
    }
    next.run(request).await
}

/// URL に載せるホスト。wildcard (`0.0.0.0`) のときだけ共有アドレスに読み替える（R-SERVE）。
pub fn session_host(bind: Ipv4Addr, share_address: Option<Ipv4Addr>) -> Ipv4Addr {
    if bind.is_unspecified() {
        share_address.unwrap_or(Ipv4Addr::LOCALHOST)
    } else {
        bind
    }
}

pub fn session_url(
    listener: &TcpListener,
    token: &str,
    host: Ipv4Addr,
) -> Result<String, ServerError> {
    let address = listener.local_addr().map_err(ServerError::Io)?;
    Ok(format!("http://{host}:{}/s/{token}/", address.port()))
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

    fn unprocessable(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
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
    let host = state
        .allowed
        .accept(
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok()),
        )
        .ok_or_else(|| ApiError::forbidden("Host does not match"))?;
    let expected_origin = format!("http://{host}");
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if origin != Some(expected_origin.as_str()) {
        return Err(ApiError::forbidden("Origin does not match"));
    }
    Ok(())
}

/// 待ち受けアドレスと共有アドレスから決まる、POST を受理する Host の範囲（R-SERVE）。
pub(crate) struct AllowedHosts {
    bind: Ipv4Addr,
    share_address: Option<Ipv4Addr>,
    port: u16,
}

impl AllowedHosts {
    pub(crate) fn new(bind: Ipv4Addr, share_address: Option<Ipv4Addr>, port: u16) -> Self {
        AllowedHosts {
            bind,
            share_address,
            port,
        }
    }

    /// 待ち受け port を伴い、ホスト部が `127.0.0.0/8`・バインドした具体アドレス・共有
    /// アドレスのいずれかである Host だけを、受理した文字列のまま返す。
    /// wildcard (`0.0.0.0`) は範囲に含めない。
    fn accept<'a>(&self, host: Option<&'a str>) -> Option<&'a str> {
        let host = host?;
        let (address, port) = host.rsplit_once(':')?;
        if port.parse::<u16>().ok()? != self.port {
            return None;
        }
        let address: Ipv4Addr = address.parse().ok()?;
        let allowed = address.is_loopback()
            || (!self.bind.is_unspecified() && address == self.bind)
            || self.share_address == Some(address);
        allowed.then_some(host)
    }
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
        .ok_or_else(|| ApiError::not_found(format!("{path} not found")))?;
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
        _ => Err(ApiError::bad_request("unit must be file or commit")),
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
            Unavailable::NoSuchUnit => ApiError::not_found("no such unit"),
            Unavailable::NotReady => ApiError::conflict("that unit is not ready yet"),
        })?;
        let session = state.session.lock().expect("session poisoned");
        let mut body = review_json(meta.as_ref(), &session);
        body["unit"] = json!(shown.map(GroupBy::as_str));
        body["units"] = review.units_json();
        body
    };
    // 起動時の単位を返した後に、もう片方を裏で作り始める（R-UNIT, R-SERVE）。
    units::start_if_waiting(&state);
    // 応答を返した後に、写しの凍結を裏で始める（R-SESSION）。
    start_freeze(&state);
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

#[derive(Debug, Deserialize)]
struct OriginQuery {
    /// "1" で上限を超えるファイルでも由来を求める。
    #[serde(default)]
    force: Option<String>,
}

/// 最終形のファイルの由来（R-ORIGIN）。差分とは別に、表示の後から取りに来る。
///
/// 由来の計算の失敗はレビューを終えない（R-SUBMIT の例外）。変更ブロックを 1 つも
/// 持たない由来を返し、画面は由来の無いブロックを「特定できない」と出す。
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

/// 計算に失敗したファイルの由来。変更ブロックもコミットも 1 つも持たない。
fn unknown_origin(path: &str, error: impl std::fmt::Display) -> FileOrigin {
    eprintln!("kemi: cannot compute the origin of {path}: {error}");
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
        return Err(ApiError::conflict("cannot be retried now"));
    }
    let units = state
        .review
        .read()
        .expect("review lock poisoned")
        .units_json();
    Ok(Json(json!({ "units": units })))
}

fn parse_side(side: &str) -> Result<Side, ApiError> {
    match side {
        "new" => Ok(Side::New),
        "old" => Ok(Side::Old),
        _ => Err(ApiError::bad_request("side must be new or old")),
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
    let value = json!({
        "id": request.file_id,
        "seen": session.seen.contains(&request.file_id),
        "collapsed": session
            .collapsed
            .get(&request.file_id)
            .copied()
            .unwrap_or(false),
    });
    drop(session);
    persist(&state);
    Ok(Json(value))
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
    ApiError::not_found("file not found")
}

fn side_lines(bytes: &Option<Vec<u8>>) -> Vec<String> {
    bytes
        .as_deref()
        .map(|bytes| content::lines(&String::from_utf8_lossy(bytes)))
        .unwrap_or_default()
}
