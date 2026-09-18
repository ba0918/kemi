//! セッション状態（R-SERVE）とセッションへの保存（R-SESSION）。
//! コメント、見た、折りたたみ、解決をメモリに持ち、変更のたびに保存先へも書く。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use kemi_core::domain::review::Comment;
use kemi_core::session::{FrozenUnit, SessionCopy, SessionState};
use kemi_core::source::FileContent;
use serde_json::json;

use crate::AppState;

#[derive(Default)]
pub struct Session {
    pub comments: Vec<Comment>,
    pub seen: BTreeSet<String>,
    pub collapsed: BTreeMap<String, bool>,
    pub next_comment: u32,
}

impl Session {
    /// 保存された状態から読み戻す。コメントの id は再利用しないので、採番は既存の
    /// 最大の番号の次から始める。
    pub fn from_state(state: SessionState) -> Self {
        let next_comment = state
            .comments
            .iter()
            .filter_map(|comment| comment.id.strip_prefix('c'))
            .filter_map(|number| number.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        Session {
            comments: state.comments,
            seen: state.seen,
            collapsed: state.collapsed,
            next_comment,
        }
    }
}

/// R-SUBMIT の契約に合わせたコメントの JSON。`content_hash` は出さない。
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
        "replies": comment.replies,
        "resolved": comment.resolved,
        "outdated": comment.outdated,
        "suggestion": comment
            .suggestion
            .as_ref()
            .map(|suggestion| json!({ "replacement": suggestion.replacement })),
    })
}

/// 今の状態を保存先へ書く。失敗は警告だけで、レビューは終わらせない（R-SESSION）。
pub(crate) fn persist(state: &AppState) {
    let Some(sink) = &state.session_sink else {
        return;
    };
    let snapshot = {
        let session = state.session.lock().expect("session poisoned");
        SessionState {
            comments: session.comments.clone(),
            seen: session.seen.clone(),
            collapsed: session.collapsed.clone(),
        }
    };
    if let Err(error) = sink.save_state(snapshot) {
        eprintln!("kemi: could not save the session: {error}");
    }
}

/// submit の確定後にセッションを消す。結果ファイルの保存に失敗していても消す（R-SESSION）。
pub(crate) fn delete_stored(state: &AppState) {
    let Some(sink) = &state.session_sink else {
        return;
    };
    if let Err(error) = sink.delete() {
        eprintln!("kemi: could not delete the session: {error}");
    }
}

/// 最初の `api/review` の応答の後に、凍結のタスクを裏で始める（R-SESSION）。
/// 起動は待たせない。1 度だけ始める。
pub(crate) fn start_freeze(state: &Arc<AppState>) {
    if state.session_sink.is_none() {
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
        if let Some(other) = &review.other {
            if let Some(meta) = &other.meta {
                units.push(FrozenUnit {
                    unit: Some(other.unit),
                    review: meta.as_ref().clone(),
                });
            }
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
                        eprintln!("kemi: could not save the session: {error}");
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
            eprintln!("kemi: could not save the session: {error}");
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
