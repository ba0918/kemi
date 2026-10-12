//! セッション状態（R-SERVE）とセッションへの保存（R-SESSION）。
//! コメント、見た、折りたたみ、解決、返信、発言、往復の続きをメモリに持ち、変更のたびに
//! 保存先へも書く。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use kemi_core::domain::agent::{AgentEvent, HandedComment};
use kemi_core::domain::review::{Comment, CommentTarget, Message, PageTarget, Reply};
use kemi_core::session::{FrozenUnit, SessionCopy, SessionState};
use kemi_core::source::FileContent;
use serde_json::json;

use crate::{AppState, Notice};

pub(crate) struct FreezeInput {
    pub source: Arc<dyn kemi_core::source::ReviewSource>,
    pub startup_unit: Option<kemi_core::domain::review::GroupBy>,
    pub startup: Arc<kemi_core::domain::review::ReviewMeta>,
    pub other: Option<crate::units::OtherUnit>,
    pub generation: u128,
    pub token: String,
}

#[derive(Default, Clone)]
pub struct Session {
    pub state: SessionState,
    /// `channel.events` からこれまでに外した起きたことの数。`kemi wait` の受け取りの
    /// 知らせ（何件目まで受け取ったか）を、重なっても二度外さないための起点。メモリだけに
    /// 置き、復元では 0 から数え直す（それより前の知らせは止まったサーバへのもので届かない）。
    pub received: u64,
}

impl Session {
    /// 保存された状態から読み戻す。
    pub fn from_state(state: SessionState) -> Self {
        Session {
            state: state.prepare_for_restore(),
            received: 0,
        }
    }

    /// 保存する状態の写し。
    pub(crate) fn snapshot(&self) -> SessionState {
        self.state.clone()
    }
}

/// R-SUBMIT の契約に合わせたコメントの JSON。`content_hash` は出さない。ページへのコメントの
/// 画像のパスは出さない（submit と画面への応答では `null`）。
pub fn comment_json(comment: &Comment) -> serde_json::Value {
    comment_json_with(comment, false)
}

/// `kemi wait` に返すコメントの JSON。ページへのコメントの画像は絶対パスで渡す（R-AGENT-EVENTS）。
fn agent_comment_json(comment: &Comment) -> serde_json::Value {
    comment_json_with(comment, true)
}

fn comment_json_with(comment: &Comment, with_image: bool) -> serde_json::Value {
    let mut value = json!({
        "id": comment.id,
        "group_id": comment.group_id,
        "group_title": comment.group_title,
        "path": null,
        "side": null,
        "start_line": null,
        "end_line": null,
        "quote": [],
        "body": comment.body,
        "page": null,
        "target_type":match &comment.target {CommentTarget::File(_)=>"file",CommentTarget::Page(_)=>"page",CommentTarget::Explanation(_)=>"explanation"},
        "explanation":null,
        "replies": comment.replies.iter().map(reply_json).collect::<Vec<_>>(),
        "resolved": comment.resolved,
        "outdated": comment.outdated,
        "suggestion": null,
    });
    if let CommentTarget::Page(page) = &comment.target {
        value["page"] = page_json(page);
        if with_image {
            value["page"]["image"] = json!(page.image);
        }
    }
    if let CommentTarget::Explanation(target) = &comment.target {
        value["group_id"] = json!(null);
        value["group_title"] = json!(null);
        value["explanation"] = json!(target);
    }
    if let Some(file) = comment.file() {
        value["path"] = json!(file.path);
        value["side"] = json!(file.side.as_str());
        value["start_line"] = json!(file.start_line);
        value["end_line"] = json!(file.end_line);
        value["quote"] = json!(file.quote);
        value["suggestion"] = json!(
            file.suggestion
                .as_ref()
                .map(|suggestion| json!({ "replacement": suggestion.replacement }))
        );
    }
    value
}

/// R-SUBMIT の `page`。画像のパスは `kemi wait` の応答でだけ渡すので、ここでは `null`
/// （submit の確定でファイルごと消える）。
fn page_json(page: &PageTarget) -> serde_json::Value {
    json!({
        "url": page.url,
        "width": page.width,
        "places": page.places.iter().map(|place| json!({
            "n": place.n,
            "kind": place.kind.as_str(),
            "points": place.points.iter().map(|point| json!({ "x": point.x, "y": point.y })).collect::<Vec<_>>(),
            "elements": place.elements.iter().map(|element| json!({
                "selector": element.selector,
                "text": element.text,
                "rect": {
                    "x": element.rect.x,
                    "y": element.rect.y,
                    "w": element.rect.w,
                    "h": element.rect.h,
                },
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "image": null,
    })
}

/// R-SUBMIT の契約に合わせた返信の JSON。案（`variants`・`chosen`・`applied`）は
/// `--live` のレビューでだけ持つので、ここでは常に空。
pub fn reply_json(reply: &Reply) -> serde_json::Value {
    json!({
        "id": reply.id,
        "author": reply.author.as_str(),
        "body": reply.body.plain(),
        "parts":reply.body.parts(),
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
        "body": message.body.plain(),
        "parts":message.body.parts(),
    })
}

/// ページに返すコメントの JSON。R-SUBMIT の形に、会話パネルの並びに使う通し番号
/// （`seq`）をコメントと返信に足す。番号は結果 JSON と `kemi wait` には出さない（R-SESSION）。
pub fn page_comment_json(comment: &Comment) -> serde_json::Value {
    let mut value = comment_json(comment);
    value["seq"] = json!(comment.seq);
    value["replies"] = comment.replies.iter().map(page_reply_json).collect();
    value
}

fn page_reply_json(reply: &Reply) -> serde_json::Value {
    let mut value = reply_json(reply);
    value["seq"] = json!(reply.seq);
    value
}

/// ページに返す発言の JSON。R-SUBMIT の形に通し番号（`seq`）を足す。
pub fn page_message_json(message: &Message) -> serde_json::Value {
    let mut value = message_json(message);
    value["seq"] = json!(message.seq);
    value
}

/// `kemi wait` が返す起きたこと 1 つ（R-AGENT-EVENTS）。コメント・返信・発言の形は
/// submit の結果と同じ。削除したコメントは id だけを持つ。
pub fn agent_event_json(event: &AgentEvent) -> serde_json::Value {
    match event {
        AgentEvent::Handed(handed) => json!({
            "type": "handed",
            "comments": handed.comments.iter().map(|change| match change {
                HandedComment::Added(comment) => {
                    json!({ "change": "added", "comment": agent_comment_json(comment) })
                }
                HandedComment::Edited(comment) => {
                    json!({ "change": "edited", "comment": agent_comment_json(comment) })
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
    crate::units::start_initial_if_waiting(state);
    let state = state.clone();
    tokio::spawn(async move { freeze(state).await });
}

/// 両グループ単位のメタデータと内容がそろったら写しを保存する。そろわない間は待ち、
/// 読み取りに失敗したら数回試してから復元不可の印を付ける。
async fn freeze(state: Arc<AppState>) {
    let mut shutdown = state.shutdown.subscribe();
    let input = loop {
        if *shutdown.borrow() {
            state
                .initial_review
                .lock()
                .expect("initial review poisoned")
                .take();
            return;
        }
        let (ready, failed) = {
            let initial = state
                .initial_review
                .lock()
                .expect("initial review poisoned");
            let Some(initial) = initial.as_ref() else {
                return;
            };
            (
                initial
                    .other
                    .as_ref()
                    .is_none_or(|other| other.state == crate::units::BuildState::Ready),
                initial.other.as_ref().and_then(|other| match &other.state {
                    crate::units::BuildState::Failed(reason) => Some(reason.clone()),
                    crate::units::BuildState::Waiting
                    | crate::units::BuildState::Building
                    | crate::units::BuildState::Ready => None,
                }),
            )
        };
        if let Some(reason) = failed {
            state
                .initial_review
                .lock()
                .expect("initial review poisoned")
                .take();
            let _gate = state.persist.lock().expect("persist poisoned");
            if let Some(sink) = &state.session_sink
                && let Err(error) = sink.mark_unresumable(&reason)
            {
                state.notices.notify(Notice::SessionNotSaved(error));
            }
            return;
        }
        if ready {
            break state
                .initial_review
                .lock()
                .expect("initial review poisoned")
                .take()
                .expect("freeze target present");
        }
        // 現在版の他方の単位を待つと、更新後のメタデータを初回写しに混ぜることになる。
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            _ = shutdown.changed() => {state.initial_review.lock().expect("initial review poisoned").take();return;},
        }
    };

    let (startup_unit, units) = {
        let mut units = vec![FrozenUnit {
            unit: input.startup_unit,
            review: input.startup.as_ref().clone(),
        }];
        if let Some(other) = &input.other
            && let Some(meta) = &other.meta
        {
            units.push(FrozenUnit {
                unit: Some(other.unit),
                review: meta.as_ref().clone(),
            });
        }
        (input.startup_unit, units)
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
        match read_contents(input.source.clone(), ids.clone()).await {
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
                    let prepared = match tokio::task::spawn_blocking(move || {
                        kemi_core::session::prepare_copy(copy)
                    })
                    .await
                    {
                        Ok(Ok(copy)) => copy,
                        Ok(Err(error)) => {
                            state.notices.notify(Notice::SessionNotSaved(error));
                            return;
                        }
                        Err(error) => {
                            state.notices.notify(Notice::SessionNotSaved(
                                kemi_core::session::SessionError::Encode {
                                    reason: error.to_string(),
                                },
                            ));
                            return;
                        }
                    };
                    let _gate = state.persist.lock().expect("persist poisoned");
                    if *shutdown.borrow() {
                        return;
                    }
                    let mut session = state.session.lock().expect("session poisoned");
                    let mut snapshot = session.snapshot();
                    snapshot
                        .display
                        .as_mut()
                        .expect("initial display assigned")
                        .frozen = Some(input.token.clone());
                    match sink.save_frozen_copy(prepared, snapshot) {
                        Ok(true) => {
                            session
                                .state
                                .display
                                .as_mut()
                                .expect("initial display assigned")
                                .frozen = Some(input.token.clone())
                        }
                        Ok(false) => {}
                        Err(error) => state.notices.notify(Notice::SessionNotSaved(error)),
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
        let _gate = state.persist.lock().expect("persist poisoned");
        let reason = format!("cannot read the review contents: {last_error}");
        if let Err(error) = sink.mark_unresumable(&reason) {
            state.notices.notify(Notice::SessionNotSaved(error));
        }
    }
}

async fn read_contents(
    source: Arc<dyn kemi_core::source::ReviewSource>,
    ids: BTreeSet<String>,
) -> Result<BTreeMap<String, FileContent>, String> {
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
