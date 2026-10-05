//! セッション状態（R-SERVE）とセッションへの保存（R-SESSION）。
//! コメント、見た、折りたたみ、解決、返信、発言、往復の続きをメモリに持ち、変更のたびに
//! 保存先へも書く。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use kemi_core::domain::agent::{AgentEvent, Channel, HandedComment};
use kemi_core::domain::review::{Comment, Message, Reply};
use kemi_core::session::{FrozenUnit, SessionCopy, SessionState};
use kemi_core::source::FileContent;
use serde_json::json;

use crate::{AppState, Notice};

#[derive(Default)]
pub struct Session {
    pub comments: Vec<Comment>,
    pub seen: BTreeSet<String>,
    pub collapsed: BTreeMap<String, bool>,
    pub last_comment: u32,
    pub messages: Vec<Message>,
    pub last_reply: u32,
    pub last_message: u32,
    pub channel: Channel,
}

impl Session {
    /// 保存された状態から読み戻す。コメント・返信・発言の id は再利用しないので、採番は
    /// 保存した最終番号と、残っているものの番号の大きい方から続ける。
    pub fn from_state(state: SessionState) -> Self {
        let from_comments = highest_number(state.comments.iter().map(|comment| &comment.id), 'c');
        let from_replies = highest_number(
            state
                .comments
                .iter()
                .flat_map(|comment| comment.replies.iter().map(|reply| &reply.id)),
            'r',
        );
        let from_messages = highest_number(state.messages.iter().map(|message| &message.id), 'm');
        Session {
            comments: state.comments,
            seen: state.seen,
            collapsed: state.collapsed,
            last_comment: state.last_comment.max(from_comments),
            messages: state.messages,
            last_reply: state.last_reply.max(from_replies),
            last_message: state.last_message.max(from_messages),
            channel: state.channel,
        }
    }

    /// 保存する状態の写し。
    pub(crate) fn snapshot(&self) -> SessionState {
        SessionState {
            comments: self.comments.clone(),
            seen: self.seen.clone(),
            collapsed: self.collapsed.clone(),
            last_comment: self.last_comment,
            messages: self.messages.clone(),
            last_reply: self.last_reply,
            last_message: self.last_message,
            channel: self.channel.clone(),
        }
    }
}

/// `c12` のような id の番号の最大。
fn highest_number<'a>(ids: impl Iterator<Item = &'a String>, prefix: char) -> u32 {
    ids.filter_map(|id| id.strip_prefix(prefix))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
}

/// R-SUBMIT の契約に合わせたコメントの JSON。`content_hash` は出さない。`page` は
/// `--live` のページへのコメントだけが持つので、ここでは常に `null`。
pub fn comment_json(comment: &Comment) -> serde_json::Value {
    json!({
        "id": comment.id,
        "group_id": comment.group_id,
        "group_title": comment.group_title,
        "path": comment.path,
        "side": comment.side.as_str(),
        "start_line": comment.start_line,
        "end_line": comment.end_line,
        "quote": comment.quote,
        "body": comment.body,
        "page": null,
        "replies": comment.replies.iter().map(reply_json).collect::<Vec<_>>(),
        "resolved": comment.resolved,
        "outdated": comment.outdated,
        "suggestion": comment
            .suggestion
            .as_ref()
            .map(|suggestion| json!({ "replacement": suggestion.replacement })),
    })
}

/// R-SUBMIT の契約に合わせた返信の JSON。案（`variants`・`chosen`・`applied`）は
/// `--live` のレビューでだけ持つので、ここでは常に空。
pub fn reply_json(reply: &Reply) -> serde_json::Value {
    json!({
        "id": reply.id,
        "author": reply.author.as_str(),
        "body": reply.body,
        "variants": [],
        "chosen": null,
        "applied": null,
    })
}

/// R-SUBMIT の契約に合わせた発言の JSON。
pub fn message_json(message: &Message) -> serde_json::Value {
    json!({
        "id": message.id,
        "author": message.author.as_str(),
        "body": message.body,
    })
}

/// `kemi wait` が返す起きたこと 1 つ（R-AGENT-EVENTS）。コメント・返信・発言の形は
/// submit の結果と同じ。削除したコメントは id だけを持つ。
pub fn agent_event_json(event: &AgentEvent) -> serde_json::Value {
    match event {
        AgentEvent::Handed(handed) => json!({
            "type": "handed",
            "comments": handed.comments.iter().map(|change| match change {
                HandedComment::Added(comment) => {
                    json!({ "change": "added", "comment": comment_json(comment) })
                }
                HandedComment::Edited(comment) => {
                    json!({ "change": "edited", "comment": comment_json(comment) })
                }
                HandedComment::Deleted(id) => json!({ "change": "deleted", "comment": { "id": id } }),
            }).collect::<Vec<_>>(),
            "replies": handed.replies.iter().map(|handed| json!({
                "comment_id": handed.comment_id,
                "reply": reply_json(&handed.reply),
            })).collect::<Vec<_>>(),
            "messages": handed.messages.iter().map(message_json).collect::<Vec<_>>(),
        }),
    }
}

/// 今の状態を保存先へ書く。失敗は警告だけで、レビューは終わらせない（R-SESSION）。
pub(crate) fn persist(state: &AppState) {
    let Some(sink) = &state.session_sink else {
        return;
    };
    // スナップショットと保存を 1 つずつ進め、遅い保存が新しい状態を上書きしないように
    // する。呼び出し側はセッション mutex を離してから来るので、ロックは persist →
    // session の一方向に保たれる。
    let _persist = state.persist.lock().expect("persist poisoned");
    let snapshot = state.session.lock().expect("session poisoned").snapshot();
    if let Err(error) = sink.save_state(snapshot) {
        state.notices.notify(Notice::SessionNotSaved(error));
    }
}

/// submit の確定後にセッションを消す。結果ファイルの保存に失敗していても消す（R-SESSION）。
pub(crate) fn delete_stored(state: &AppState) {
    let Some(sink) = &state.session_sink else {
        return;
    };
    if let Err(error) = sink.delete() {
        state.notices.notify(Notice::SessionNotDeleted(error));
    }
}

/// 最初の `api/review` の応答の後に、凍結のタスクを裏で始める（R-SESSION）。
/// 起動は待たせない。1 度だけ始める。
pub(crate) fn start_freeze(state: &Arc<AppState>) {
    // 復元は同じセッションの続きで、写しは既にディスクにある。作り直しても内容は
    // 同じで、読み直しと再圧縮と書き直しの分だけ払うことになる（R-SESSION）。
    let Some(sink) = &state.session_sink else {
        return;
    };
    if !sink.needs_copy() {
        return;
    }
    if state.freeze_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move { freeze(state).await });
}

/// 両グループ単位のメタデータと内容がそろったら写しを保存する。そろわない間は待ち、
/// 読み取りに失敗したら数回試してから復元不可の印を付ける。
async fn freeze(state: Arc<AppState>) {
    let mut shutdown = state.shutdown.subscribe();
    loop {
        if *shutdown.borrow() {
            return;
        }
        let ready = state
            .review
            .read()
            .expect("review lock poisoned")
            .all_units_ready();
        if ready {
            break;
        }
        // 失敗した単位は retry で作り直されることがあるので、待ち続ける。
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            _ = shutdown.changed() => return,
        }
    }

    let (startup_unit, units) = {
        let review = state.review.read().expect("review lock poisoned");
        let mut units = vec![FrozenUnit {
            unit: review.startup_unit,
            review: review.startup.as_ref().clone(),
        }];
        if let Some(other) = &review.other
            && let Some(meta) = &other.meta
        {
            units.push(FrozenUnit {
                unit: Some(other.unit),
                review: meta.as_ref().clone(),
            });
        }
        (review.startup_unit, units)
    };
    let mut ids = BTreeSet::new();
    for unit in &units {
        for group in &unit.review.groups {
            for file in &group.files {
                ids.insert(file.id.clone());
            }
        }
    }

    let mut last_error = String::new();
    for attempt in 0..3 {
        match read_contents(&state, ids.clone()).await {
            Ok(contents) => {
                // 読み取りの間に submit の停止が始まっていたら、消したセッションを
                // 作り直さない（R-SESSION）。
                if *shutdown.borrow() {
                    return;
                }
                if let Some(sink) = &state.session_sink {
                    let copy = SessionCopy {
                        startup_unit,
                        units,
                        contents,
                    };
                    if let Err(error) = sink.save_copy(copy) {
                        state.notices.notify(Notice::SessionNotSaved(error));
                    }
                }
                return;
            }
            Err(error) => {
                last_error = error;
                if attempt < 2 {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                        _ = shutdown.changed() => return,
                    }
                }
            }
        }
    }

    if *shutdown.borrow() {
        return;
    }
    if let Some(sink) = &state.session_sink {
        let reason = format!("cannot read the review contents: {last_error}");
        if let Err(error) = sink.mark_unresumable(&reason) {
            state.notices.notify(Notice::SessionNotSaved(error));
        }
    }
}

async fn read_contents(
    state: &Arc<AppState>,
    ids: BTreeSet<String>,
) -> Result<BTreeMap<String, FileContent>, String> {
    let source = state.source.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut contents = BTreeMap::new();
        for id in ids {
            contents.insert(id.clone(), source.content(&id)?);
        }
        Ok::<_, kemi_core::source::SourceError>(contents)
    })
    .await;
    match result {
        Ok(Ok(contents)) => Ok(contents),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
