//! エージェント用の API（agent-channel.md の R-AGENT-CLI, R-AGENT-WRITE, R-AGENT-LINK）。
//!
//! ページ用とは別のリスナー（`127.0.0.1` のみ）に、別の `Router` で載せる。ページ用の
//! `Origin` と `Host` の検証とは受理の規則が逆（ここは `Origin` を持たない要求だけを
//! 受ける）なので、同じ `Router` に混ぜると片方の検証を緩めることになる。
//!
//! - `POST /a/<token>/wait`: 前の wait が返した後に起きたことを返す。無ければ待つ
//!   （long-poll）。時間切れはサーバが決め、空で `{"timeout": true}` を返す。
//! - `POST /a/<token>/reply`: 返信と発言を書く。1 件でも誤りがあれば 1 件も書かない。

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::StreamExt;
use kemi_core::domain::agent::{AgentWrite, validate_writes};
use kemi_core::domain::review::{Author, Message, Reply};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::channel::notify_agent_state;
use crate::session::{agent_event_json, comment_json, message_json, persist};
use crate::{AppState, Event, Stop};

pub(crate) fn agent_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/a/{token}/wait", post(wait_api))
        .route("/a/{token}/reply", post(reply_api))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, guard))
}

/// 受理の条件は本文の解釈より先に確かめる。接続元が分からない要求も断る。
async fn guard(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    let token = request
        .uri()
        .path()
        .strip_prefix("/a/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_string();
    let expected = state.agent_token.as_deref().unwrap_or_default();
    match peer {
        Some(peer) => {
            if let Err(error) = admit(peer, request.headers(), &token, expected) {
                return error.into_response();
            }
        }
        None => return ApiError::forbidden("unknown peer").into_response(),
    }
    next.run(request).await
}

/// エージェント用の API を受理するか（R-AGENT-LINK）。ループバックからの、`Origin` を
/// 持たない、トークンの合う要求だけ。`Origin` を断るのは、ブラウザの `POST`（`no-cors`
/// を含む）が必ず `Origin` を付けるから。
fn admit(peer: IpAddr, headers: &HeaderMap, token: &str, expected: &str) -> Result<(), ApiError> {
    if !peer.is_loopback() {
        return Err(ApiError::forbidden(
            "the agent API is only for this machine",
        ));
    }
    if headers.contains_key(header::ORIGIN) {
        return Err(ApiError::forbidden("requests from a browser are refused"));
    }
    if expected.is_empty() || !same_secret(token.as_bytes(), expected.as_bytes()) {
        return Err(ApiError::forbidden("the agent token does not match"));
    }
    Ok(())
}

/// 長さ以外の比べ方で時間が変わらないように、すべてのバイトを比べる。
fn same_secret(given: &[u8], expected: &[u8]) -> bool {
    given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

#[derive(Debug, Deserialize)]
struct WaitRequest {
    /// 省略か null で待ち続ける。
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// 今返せるもの。
enum Ready {
    /// 返す起きたことと、そのうち保存された（「渡した」の）件数。
    Events(Vec<Value>, usize),
    /// レビューが submit 以外で終わった。理由つき。
    Stopped(String),
    Nothing,
}

async fn wait_api(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let request: WaitRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return ApiError::bad_request(format!("invalid request: {error}")).into_response();
        }
    };
    {
        let mut agent = state.agent.lock().expect("agent poisoned");
        if agent.waiting {
            return ApiError::conflict("another kemi wait is already waiting for this review")
                .into_response();
        }
        agent.waiting = true;
    }
    let guard = WaitGuard {
        state: state.clone(),
    };
    mark_called(&state);
    notify_agent_state(&state);

    let deadline = request
        .timeout_ms
        .map(|millis| tokio::time::Instant::now() + Duration::from_millis(millis));
    let mut shutdown = state.shutdown.subscribe();
    loop {
        let notified = state.wake.notified();
        tokio::pin!(notified);
        // 確かめる前に通知を受ける用意をして、確かめた直後の通知を取りこぼさない。
        notified.as_mut().enable();
        match ready(&state) {
            Ready::Events(events, count) => return deliver(state.clone(), events, count, guard),
            Ready::Stopped(reason) => return stopped(&reason),
            Ready::Nothing => {}
        }
        let timed_out = async {
            match deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = &mut notified => {}
            changed = shutdown.changed() => {
                if changed.is_err() {
                    return stopped("the review stopped");
                }
            }
            () = timed_out => {
                // 時間切れと同時に届いたものは、空で返さずに返す。
                return match ready(&state) {
                    Ready::Events(events, count) => deliver(state.clone(), events, count, guard),
                    Ready::Stopped(reason) => stopped(&reason),
                    Ready::Nothing => Json(json!({ "timeout": true })).into_response(),
                };
            }
        }
    }
}

/// 待ちが終わったら（返し終えても、接続が切れても）作業中に戻す（R-AGENT-STATE）。
struct WaitGuard {
    state: Arc<AppState>,
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        {
            let mut agent = self.state.agent.lock().expect("agent poisoned");
            agent.waiting = false;
            agent.last_activity = kemi_core::session::now_millis();
        }
        notify_agent_state(&self.state);
    }
}

/// `kemi wait` が一度でも呼ばれたことをセッション状態に残す。画面の「エージェントに渡す」は
/// これで出る（R-AGENT-STATE）。
fn mark_called(state: &AppState) {
    let changed = {
        let mut session = state.session.lock().expect("session poisoned");
        !std::mem::replace(&mut session.channel.called, true)
    };
    if changed {
        persist(state);
    }
}

fn ready(state: &AppState) -> Ready {
    let mut events: Vec<Value> = {
        let session = state.session.lock().expect("session poisoned");
        session
            .channel
            .events
            .iter()
            .map(agent_event_json)
            .collect()
    };
    let count = events.len();
    match &*state.stop.lock().expect("stop poisoned") {
        Some(Stop::Submitted(document)) => {
            events.push(json!({ "type": "submitted", "result": document }));
            return Ready::Events(events, count);
        }
        Some(Stop::Failed(message)) => {
            return Ready::Stopped(format!("the review stopped with an error: {message}"));
        }
        Some(Stop::Suspended) => {
            return Ready::Stopped(
                "the review was suspended; it continues after kemi --resume".to_string(),
            );
        }
        None => {}
    }
    if events.is_empty() {
        Ready::Nothing
    } else {
        Ready::Events(events, count)
    }
}

fn stopped(reason: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": reason })),
    )
        .into_response()
}

/// 起きたことを返す。保存した起きたことを外すのは本文を書き終えた後にする。返す途中で
/// 接続が切れたら本文は捨てられ、外さないので、次の wait がもう一度受け取る。
fn deliver(state: Arc<AppState>, events: Vec<Value>, count: usize, guard: WaitGuard) -> Response {
    let bytes = Bytes::from(json!({ "events": events }).to_string());
    let delivered = futures_util::stream::once(async move {
        {
            let mut session = state.session.lock().expect("session poisoned");
            let events = &mut session.channel.events;
            events.drain(..count.min(events.len()));
        }
        if count > 0 {
            persist(&state);
        }
        drop(guard);
    })
    .filter_map(|()| async { None::<Result<Bytes, Infallible>> });
    let body =
        futures_util::stream::once(async move { Ok::<_, Infallible>(bytes) }).chain(delivered);
    let mut response = Response::new(Body::from_stream(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response
}

#[derive(Debug, Deserialize)]
struct ReplyRequest {
    writes: Vec<WriteRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WriteRequest {
    Reply {
        comment_id: String,
        body: String,
        /// 案は動いているページのレビューでだけ受け付ける。キーがあれば形の誤り。
        #[serde(default)]
        variants: Option<Value>,
        #[serde(default)]
        applied: Option<Value>,
    },
    Message {
        body: String,
    },
}

async fn reply_api(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let request: ReplyRequest = serde_json::from_slice(&body)
        .map_err(|error| ApiError::bad_request(format!("invalid writes: {error}")))?;
    let writes = request
        .writes
        .into_iter()
        .map(|write| match write {
            WriteRequest::Reply {
                variants: Some(_), ..
            } => Err(ApiError::bad_request(
                "variants are accepted only in a review of a running page (--live)",
            )),
            WriteRequest::Reply {
                applied: Some(_), ..
            } => Err(ApiError::bad_request(
                "applied is accepted only in a review of a running page (--live)",
            )),
            WriteRequest::Reply {
                comment_id, body, ..
            } => Ok(AgentWrite::Reply { comment_id, body }),
            WriteRequest::Message { body } => Ok(AgentWrite::Message { body }),
        })
        .collect::<Result<Vec<_>, ApiError>>()?;

    let (ids, threads, messages) = {
        let mut session = state.session.lock().expect("session poisoned");
        validate_writes(&writes, &session.comments, &session.messages)
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        let mut ids = Vec::new();
        let mut touched: Vec<String> = Vec::new();
        let mut messages = Vec::new();
        for write in writes {
            match write {
                AgentWrite::Reply { comment_id, body } => {
                    session.last_reply += 1;
                    let reply = Reply {
                        id: format!("r{}", session.last_reply),
                        author: Author::Agent,
                        body,
                    };
                    ids.push(reply.id.clone());
                    if let Some(comment) = session
                        .comments
                        .iter_mut()
                        .find(|comment| comment.id == comment_id)
                    {
                        comment.replies.push(reply);
                    }
                    if !touched.contains(&comment_id) {
                        touched.push(comment_id);
                    }
                }
                AgentWrite::Message { body } => {
                    session.last_message += 1;
                    let message = Message {
                        id: format!("m{}", session.last_message),
                        author: Author::Agent,
                        body,
                    };
                    ids.push(message.id.clone());
                    messages.push(message_json(&message));
                    session.messages.push(message);
                }
            }
        }
        let threads: Vec<Value> = touched
            .iter()
            .filter_map(|id| session.comments.iter().find(|comment| &comment.id == id))
            .map(comment_json)
            .collect();
        (ids, threads, messages)
    };
    state.agent.lock().expect("agent poisoned").last_activity = kemi_core::session::now_millis();
    persist(&state);
    for thread in threads {
        let _ = state.events.send(Event::Thread(thread));
    }
    for message in messages {
        let _ = state.events.send(Event::Message(message));
    }
    notify_agent_state(&state);
    Ok(Json(json!({ "ids": ids })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn a_request_from_another_machine_is_refused() {
        let lan = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

        assert!(admit(lan, &HeaderMap::new(), "secret", "secret").is_err());
        assert!(
            admit(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &HeaderMap::new(),
                "secret",
                "secret"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_request_carrying_an_origin_is_refused() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "null".parse().expect("a valid header"));

        assert!(
            admit(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &headers,
                "secret",
                "secret"
            )
            .is_err()
        );
    }

    #[test]
    fn a_request_with_another_token_is_refused() {
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);

        assert!(admit(local, &HeaderMap::new(), "secreT", "secret").is_err());
        assert!(admit(local, &HeaderMap::new(), "", "").is_err());
    }
}
