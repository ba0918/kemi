//! ページ側の、エージェントとの往復のエンドポイント（agent-channel.md の R-AGENT-HAND）。
//! 発言を書く（`api/message`）と、前に渡した後の変化をまとめて渡す（`api/hand`）。
//! 返信と解決は `api/comment` の op が受け持つ。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use kemi_core::domain::agent::{AgentEvent, HandLineAt};
use kemi_core::domain::review::{Author, Message};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::session::{page_message_json, persist};
use crate::{AppState, Event};

#[derive(Debug, Deserialize)]
pub(super) struct MessageRequest {
    body: String,
}

pub(super) async fn message_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<MessageRequest>,
) -> Result<Json<Value>, ApiError> {
    let value = {
        let mut session = state.session.lock().expect("session poisoned");
        session.last_message += 1;
        let message = Message {
            id: format!("m{}", session.last_message),
            seq: session.next_seq(),
            author: Author::Reviewer,
            body: request.body,
        };
        session.channel.note_message(&message.id);
        let value = page_message_json(&message);
        session.messages.push(message);
        value
    };
    persist(&state);
    let _ = state.events.send(Event::Message(value.clone()));
    notify_agent_state(&state);
    Ok(Json(value))
}

pub(super) async fn hand_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let handed = {
        let mut session = state.session.lock().expect("session poisoned");
        let session = &mut *session;
        let handed = session.channel.hand(&session.comments, &session.messages);
        // 渡した 1 回分の行（R-AGENT-HAND）。ロックは session → agent の順。
        if let (true, Some(AgentEvent::Handed(event))) = (handed, session.channel.events.last()) {
            state.agent.lock().expect("agent poisoned").handed(event);
        }
        handed
    };
    // 渡すものが無くても、書いて消したコメントの記録は片付いているので保存する。
    persist(&state);
    if handed {
        state.wake.notify_waiters();
    }
    notify_agent_state(&state);
    Ok(Json(json!({ "handed": handed })))
}

/// エージェントの状態・未渡しの件数・渡した 1 回分の行（R-AGENT-STATE, R-AGENT-HAND）。
/// 行の形は画面とサーバの間だけのもの（`thread` が null なら発言だけの 1 回分の、並びの末尾の行）。
pub(crate) fn agent_json(state: &AppState) -> Value {
    let (called, unhanded) = {
        let session = state.session.lock().expect("session poisoned");
        (
            session.channel.called,
            session
                .channel
                .unhanded_count(&session.comments, &session.messages),
        )
    };
    let agent = state.agent.lock().expect("agent poisoned");
    let status = agent.status(called, kemi_core::session::now_millis());
    let lines: Vec<Value> = agent
        .lines()
        .iter()
        .map(|line| {
            let thread = match &line.at {
                HandLineAt::Thread(id) => Value::String(id.clone()),
                HandLineAt::Messages => Value::Null,
            };
            json!({ "thread": thread, "state": line.state.as_str() })
        })
        .collect();
    json!({ "called": called, "status": status.as_str(), "unhanded": unhanded, "lines": lines })
}

/// エージェントの状態か未渡しの件数が変わったかもしれないとき、ページへ知らせる。
pub(crate) fn notify_agent_state(state: &AppState) {
    let _ = state.events.send(Event::Agent(agent_json(state)));
}

/// 作業中のまま時間がたって応答なしになったことを、ページへ知らせる（R-AGENT-STATE）。
/// 状態は時刻だけでも変わるので、要求を待たずに見に行く。サーバの状態は弱い参照で持つ。
/// 保留ではサーバを止めずにプロセスを終えるので、ここが状態を握るとセッションのロックが
/// 解放されない。
pub(crate) fn start_status_ticker(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    let mut last = agent_json(state)["status"].clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(STATUS_TICK).await;
            let Some(state) = weak.upgrade() else {
                return;
            };
            if *state.shutdown.borrow() {
                return;
            }
            let agent = agent_json(&state);
            if agent["status"] != last {
                last = agent["status"].clone();
                let _ = state.events.send(Event::Agent(agent));
            }
        }
    });
}

/// 応答なしの 10 分に対して、表示が遅れても気にならない間隔。
const STATUS_TICK: std::time::Duration = std::time::Duration::from_secs(15);
