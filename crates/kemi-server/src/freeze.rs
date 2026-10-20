//! 写しの凍結（R-SESSION）。最初の `api/review` の応答の後に、両グループ単位のメタデータと
//! 内容を読んで写しを保存し、表示版に凍結した印を付ける裏のタスク。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use kemi_core::session::{FrozenUnit, SessionCopy};
use kemi_core::source::FileContent;

use crate::{AppState, Notice};

pub(crate) struct FreezeInput {
    pub source: Arc<dyn kemi_core::source::ReviewSource>,
    pub startup_unit: Option<kemi_core::domain::review::GroupBy>,
    pub startup: Arc<kemi_core::domain::review::ReviewMeta>,
    pub other: Option<crate::units::OtherUnit>,
    pub generation: u128,
    pub token: String,
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
    let mut shutdown = state.stopping.shutdown.subscribe();
    let Some(input) = wait_for_input(&state, &mut shutdown).await else {
        return;
    };
    let last_error = match read_copy(&input, &mut shutdown).await {
        Ok(Some(copy)) => {
            // 読み取りの間に submit の停止が始まっていたら、消したセッションを
            // 作り直さない（R-SESSION）。
            if *shutdown.borrow() {
                return;
            }
            if let Some(sink) = &state.session_sink {
                save_frozen_copy(&state, sink.as_ref(), copy, &input.token, &shutdown).await;
            }
            return;
        }
        Ok(None) => return,
        Err(error) => error,
    };

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

/// 凍結の入力（初回の両方の単位）がそろうまで待つ。止まるか凍結できないなら None。
async fn wait_for_input(
    state: &AppState,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Option<FreezeInput> {
    loop {
        if *shutdown.borrow() {
            state
                .initial_review
                .lock()
                .expect("initial review poisoned")
                .take();
            return None;
        }
        let failed = {
            let mut initial = state
                .initial_review
                .lock()
                .expect("initial review poisoned");
            let input = initial.as_ref()?;
            if input
                .other
                .as_ref()
                .is_none_or(|other| other.state == crate::units::BuildState::Ready)
            {
                return initial.take();
            }
            input.other.as_ref().and_then(|other| match &other.state {
                crate::units::BuildState::Failed(reason) => Some(reason.clone()),
                crate::units::BuildState::Waiting
                | crate::units::BuildState::Building
                | crate::units::BuildState::Ready => None,
            })
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
            return None;
        }
        // 現在版の他方の単位を待つと、更新後のメタデータを初回写しに混ぜることになる。
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            _ = shutdown.changed() => {state.initial_review.lock().expect("initial review poisoned").take();return None;},
        }
    }
}

/// 写しの内容を読む。失敗したら 3 回まで試す。途中で止まったら `Ok(None)`。
async fn read_copy(
    input: &FreezeInput,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<Option<SessionCopy>, String> {
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
                return Ok(Some(SessionCopy {
                    startup_unit,
                    units,
                    contents,
                }));
            }
            Err(error) => {
                last_error = error;
                if attempt < 2 {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                        _ = shutdown.changed() => return Ok(None),
                    }
                }
            }
        }
    }

    Err(last_error)
}

/// 読んだ写しを保存し、保存できたら表示版に凍結した印を付ける。
async fn save_frozen_copy(
    state: &AppState,
    sink: &dyn crate::SessionSink,
    copy: SessionCopy,
    token: &str,
    shutdown: &tokio::sync::watch::Receiver<bool>,
) {
    let prepared =
        match tokio::task::spawn_blocking(move || kemi_core::session::prepare_copy(copy)).await {
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
        .frozen = Some(token.to_owned());
    match sink.save_frozen_copy(prepared, snapshot) {
        Ok(true) => {
            session
                .state
                .display
                .as_mut()
                .expect("initial display assigned")
                .frozen = Some(token.to_owned())
        }
        Ok(false) => {}
        Err(error) => state.notices.notify(Notice::SessionNotSaved(error)),
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
