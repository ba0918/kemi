//! エージェント用の API（agent-channel.md の R-AGENT-CLI, R-AGENT-WRITE, R-AGENT-LINK）。
//!
//! ページ用とは別のリスナー（`127.0.0.1` のみ）に、別の `Router` で載せる。ページ用の
//! `Origin` と `Host` の検証とは受理の規則が逆（ここは `Origin` を持たない要求だけを
//! 受ける）なので、同じ `Router` に混ぜると片方の検証を緩めることになる。
//!
//! - `POST /a/<token>/wait`: 前の wait が受け取った後に起きたことを返す。無ければ待つ
//!   （long-poll）。時間切れはサーバが決め、空で `{"timeout": true}` を返す。応答の
//!   `through` は、返した起きたことが何件目までか。
//! - `POST /a/<token>/received`: `kemi wait` が応答を受け取りきったことの知らせ
//!   （`{"through": n}`）。保存した起きたことは、これを受けて初めて外す。
//! - `POST /a/<token>/reply`: 返信と発言を書く。1 件でも誤りがあれば 1 件も書かない。

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::StreamExt;
use kemi_core::domain::agent::{AgentEvent, AgentWrite, validate_writes};
use kemi_core::domain::review::{Author, Message, Reply};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::channel::notify_agent_state;
use crate::session::{agent_event_json, page_comment_json, page_message_json, persist};
use crate::{AppState, Event, Stop, SubmitState};

pub(crate) fn agent_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/a/{token}/wait", post(wait_api))
        .route("/a/{token}/reply", post(reply_api))
        .route("/a/{token}/received", post(received_api))
        // 書き込みの上限は件数と本文ごとの大きさ（R-AGENT-WRITE）で、要求全体の大きさでは
        // ない。上限の中の書き込みをまとめると既定の 2 MB を超えるので、本文の大きさでは
        // 断らない。ここに届くのはトークンを持つ同じマシンの要求だけ（R-AGENT-LINK）。
        .layer(DefaultBodyLimit::disable())
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
    /// 返す起きたことと、そのうち保存されたもの（「渡した」）が何件目までか、と返す渡した 1 回分
    /// （行を作業中にする。R-AGENT-HAND）。
    Events(Vec<Value>, u64, Vec<AgentEvent>),
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
        if agent.is_waiting() {
            return ApiError::conflict("another kemi wait is already waiting for this review")
                .into_response();
        }
        agent.wait_started();
    }
    let guard = WaitGuard {
        state: state.clone(),
        returned: None,
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
            Ready::Events(events, through, handed) => {
                return deliver(events, through, guard.returning(handed));
            }
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
                    Ready::Events(events, through, handed) => {
                        deliver(events, through, guard.returning(handed))
                    }
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
    /// 返した渡した 1 回分。時間切れ・接続が切れた・止まったときは None。
    returned: Option<Vec<AgentEvent>>,
}

impl WaitGuard {
    fn returning(mut self, handed: Vec<AgentEvent>) -> Self {
        self.returned = Some(handed);
        self
    }
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        {
            let mut agent = self.state.agent.lock().expect("agent poisoned");
            let now = kemi_core::session::now_millis();
            match &self.returned {
                Some(handed) => agent.wait_returned(now, handed),
                None => agent.wait_ended_empty(now),
            }
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
    let (mut events, through, handed): (Vec<Value>, u64, Vec<AgentEvent>) = {
        let session = state.session.lock().expect("session poisoned");
        let events: Vec<Value> = session
            .channel
            .events
            .iter()
            .map(agent_event_json)
            .collect();
        let through = session.received + events.len() as u64;
        (events, through, session.channel.events.clone())
    };
    match &*state.stop.lock().expect("stop poisoned") {
        Some(Stop::Submitted(document)) => {
            events.push(json!({ "type": "submitted", "result": document }));
            return Ready::Events(events, through, handed);
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
        Ready::Events(events, through, handed)
    }
}

fn stopped(reason: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": reason })),
    )
        .into_response()
}

/// 起きたことを返す。ここでは保存した起きたことを外さない。本文を書き終えても相手が
/// 受け取りきったとは限らない（途中で切れた `kemi wait` は何も出さずに終わる）ので、外すのは
/// 受け取りの知らせ（[`received_api`]）を受けてから。知らせが来なければ次の wait が
/// もう一度受け取る。待ちの印は本文を書き終えた後に外す。
fn deliver(events: Vec<Value>, through: u64, guard: WaitGuard) -> Response {
    let bytes = Bytes::from(json!({ "events": events, "through": through }).to_string());
    let delivered = futures_util::stream::once(async move {
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
struct ReceivedRequest {
    through: u64,
}

/// `kemi wait` が応答を受け取りきった。その応答で返した起きたことまでを外して保存する。
/// 前に外した分は数えないので、同じ知らせが重なっても、遅れて届いても二度は外さない。
async fn received_api(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let request: ReceivedRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return ApiError::bad_request(format!("invalid request: {error}")).into_response();
        }
    };
    let removed = {
        let mut session = state.session.lock().expect("session poisoned");
        let pending = request.through.saturating_sub(session.received);
        let removed = usize::try_from(pending)
            .unwrap_or(usize::MAX)
            .min(session.channel.events.len());
        session.channel.events.drain(..removed);
        session.received += removed as u64;
        removed
    };
    if removed > 0 {
        persist(&state);
    }
    Json(json!({})).into_response()
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
        // submit は結果を組み立てるときにセッションのロックを取るので、ロックの中で確かめれば、
        // 書けた書き込みは必ず結果に入る。submit が先に始まっていれば、結果に入らないので書かない。
        if matches!(
            *state.submit_state.lock().expect("submit poisoned"),
            SubmitState::Claimed
        ) {
            return Err(ApiError::conflict(
                "the review is being submitted; print its result with kemi --result",
            ));
        }
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
                        seq: session.next_seq(),
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
                        seq: session.next_seq(),
                        author: Author::Agent,
                        body,
                    };
                    ids.push(message.id.clone());
                    messages.push(page_message_json(&message));
                    session.messages.push(message);
                }
            }
        }
        let threads: Vec<Value> = touched
            .iter()
            .filter_map(|id| session.comments.iter().find(|comment| &comment.id == id))
            .map(page_comment_json)
            .collect();
        // 返信のスレッドと発言の行を消し、状態を表のとおりに移す（R-AGENT-STATE, R-AGENT-HAND）。
        // セッションのロックの中で行う（ロックは session → agent の順）。外で行うと、その間に人間が
        // 同じスレッドを渡し直して受け取り待ちにした行を、この古い返信が消してしまう。
        state.agent.lock().expect("agent poisoned").agent_wrote(
            session.channel.called,
            kemi_core::session::now_millis(),
            &touched,
            !messages.is_empty(),
        );
        (ids, threads, messages)
    };
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
