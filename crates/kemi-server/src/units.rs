//! グループ単位ごとのレビューの状態（R-UNIT）。起動時の単位はサーブ前に作り、
//! コミット範囲のもう片方の単位は最初の api/review の後に裏で作る。

use std::sync::Arc;

use kemi_core::domain::review::{FileEntry, GroupBy, ReviewMeta};
use kemi_core::source::{ReviewSource, SourceError};
use serde_json::{Value, json};

use crate::{AppState, Event};

/// もう片方の単位の作成の状態。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BuildState {
    /// 最初の api/review を待っている。
    Waiting,
    Building,
    Ready,
    Failed(String),
}

#[derive(Clone)]
pub(crate) struct OtherUnit {
    pub unit: GroupBy,
    /// 最後に作り終えたメタデータ。作り直している間や失敗した後も、コメントや見たの
    /// ファイル id を引けるように残す。
    pub meta: Option<Arc<ReviewMeta>>,
    pub state: BuildState,
    /// 異なる表示世代の完成結果を現在へ混ぜない。初回凍結には当時の作成対象へ渡す。
    pub generation: u128,
}

pub(crate) struct ReviewState {
    pub code_error: Option<String>,
    /// 起動時の単位。コミット範囲以外は None（単位は 1 つだけ）。
    pub startup_unit: Option<GroupBy>,
    pub startup: Arc<ReviewMeta>,
    pub other: Option<OtherUnit>,
}

/// 指定された単位を今は返せない理由。
pub(crate) enum Unavailable {
    NoSuchUnit,
    NotReady,
}

impl ReviewState {
    pub fn new(units: &[GroupBy], startup: ReviewMeta) -> Self {
        ReviewState {
            code_error: None,
            startup_unit: units.first().copied(),
            startup: Arc::new(startup),
            other: units.get(1).map(|unit| OtherUnit {
                unit: *unit,
                meta: None,
                state: BuildState::Waiting,
                generation: 1,
            }),
        }
    }

    /// 指定した単位（None は起動時の単位）のメタデータ。
    pub fn meta(
        &self,
        unit: Option<GroupBy>,
    ) -> Result<(Option<GroupBy>, Arc<ReviewMeta>), Unavailable> {
        match (unit, &self.other) {
            (None, _) => Ok((self.startup_unit, self.startup.clone())),
            (Some(unit), _) if Some(unit) == self.startup_unit => {
                Ok((self.startup_unit, self.startup.clone()))
            }
            (Some(unit), Some(other)) if other.unit == unit => match (&other.state, &other.meta) {
                (BuildState::Ready, Some(meta)) => Ok((Some(unit), meta.clone())),
                _ => Err(Unavailable::NotReady),
            },
            _ => Err(Unavailable::NoSuchUnit),
        }
    }

    /// どちらかの単位にあるファイルと、そのグループの title。
    pub fn find_file(&self, id: &str) -> Option<(FileEntry, String)> {
        let other = self.other.as_ref().and_then(|other| other.meta.as_ref());
        std::iter::once(&self.startup)
            .chain(other)
            .flat_map(|meta| meta.groups.iter())
            .find_map(|group| {
                group
                    .files
                    .iter()
                    .find(|file| file.id == id)
                    .map(|file| (file.clone(), group.title.clone()))
            })
    }

    /// ページに渡す単位の一覧。コミット範囲以外は空。
    pub fn units_json(&self) -> Value {
        let (Some(startup), Some(other)) = (self.startup_unit, &self.other) else {
            return json!([]);
        };
        let (state, error) = match &other.state {
            BuildState::Waiting | BuildState::Building => ("building", None),
            BuildState::Ready => ("ready", None),
            BuildState::Failed(reason) => ("failed", Some(reason.clone())),
        };
        json!([
            { "unit": startup.as_str(), "state": "ready", "error": null },
            { "unit": other.unit.as_str(), "state": state, "error": error },
        ])
    }
}

/// 最初の api/review の後に、もう片方の単位を作り始める。既に始めていれば何もしない。
pub(crate) fn start_if_waiting(state: &Arc<AppState>) {
    let generation = {
        let mut review = state.review.write().expect("review lock poisoned");
        let Some(other) = review.other.as_mut() else {
            return;
        };
        if other.state != BuildState::Waiting {
            return;
        }
        other.state = BuildState::Building;
        other.generation
    };
    {
        let mut initial = state
            .initial_review
            .lock()
            .expect("initial review poisoned");
        if let Some(initial) = initial.as_mut()
            && initial.generation == generation
            && let Some(other) = initial.other.as_mut()
        {
            other.state = BuildState::Building;
        }
    }
    spawn_build(state.clone(), state.sources.current(), generation);
}

/// 失敗した単位を作り直す。失敗していなければ false。
pub(crate) fn retry(state: &Arc<AppState>, unit: GroupBy) -> bool {
    let generation = {
        let mut review = state.review.write().expect("review lock poisoned");
        let Some(other) = review.other.as_mut() else {
            return false;
        };
        if other.unit != unit || !matches!(other.state, BuildState::Failed(_)) {
            return false;
        }
        other.state = BuildState::Building;
        other.generation
    };
    let _ = state.events.send(Event::Unit);
    spawn_build(state.clone(), state.sources.current(), generation);
    true
}

fn spawn_build(state: Arc<AppState>, source: Arc<dyn ReviewSource>, generation: u128) {
    tokio::spawn(async move {
        let unit = {
            let review = state.review.read().expect("review lock poisoned");
            match review.other.as_ref() {
                Some(other) => other.unit,
                None => return,
            }
        };
        {
            let mut initial = state
                .initial_review
                .lock()
                .expect("initial review poisoned");
            if let Some(initial) = initial.as_mut()
                && initial.generation == generation
                && let Some(other) = initial.other.as_mut()
            {
                other.state = BuildState::Building;
            }
        }
        let result = tokio::task::spawn_blocking(move || source.review_unit(unit)).await;
        let _gate = state.persist.lock().expect("persist poisoned");
        if *state.shutdown.borrow() {
            state
                .initial_review
                .lock()
                .expect("initial review poisoned")
                .take();
            return;
        }
        {
            let mut initial = state
                .initial_review
                .lock()
                .expect("initial review poisoned");
            if let Some(initial) = initial.as_mut()
                && initial.generation == generation
                && let Some(other) = initial.other.as_mut()
            {
                match &result {
                    Ok(Ok(meta)) => {
                        other.meta = Some(Arc::new(meta.clone()));
                        other.state = BuildState::Ready;
                    }
                    Ok(Err(error)) => other.state = BuildState::Failed(error.to_string()),
                    Err(error) => other.state = BuildState::Failed(error.to_string()),
                }
            }
        }
        {
            let mut review = state.review.write().expect("review lock poisoned");
            let Some(other) = review.other.as_mut() else {
                return;
            };
            if other.generation == generation {
                record(other, result);
            }
        }
        let _ = state.events.send(Event::Unit);
    });
}

pub(crate) fn start_initial_if_waiting(state: &Arc<AppState>) {
    let work = {
        let mut initial = state
            .initial_review
            .lock()
            .expect("initial review poisoned");
        initial.as_mut().and_then(|initial| {
            initial
                .other
                .as_mut()
                .filter(|other| other.state == BuildState::Waiting)
                .map(|other| {
                    other.state = BuildState::Building;
                    (initial.source.clone(), initial.generation)
                })
        })
    };
    if let Some((source, generation)) = work {
        spawn_build(state.clone(), source, generation);
    }
}

pub(crate) async fn refresh(state: &Arc<AppState>) -> Result<(), crate::ServerError> {
    let _refresh = state.refresh.lock().await;
    let sources = state.sources.clone();
    let source = tokio::task::spawn_blocking(move || sources.prepare())
        .await
        .map_err(|e| crate::ServerError::Stopped(e.to_string()))?
        .map_err(crate::ServerError::Source)?;
    let prepared = source.clone();
    let startup = tokio::task::spawn_blocking(move || prepared.review())
        .await
        .map_err(|e| crate::ServerError::Stopped(e.to_string()))?
        .map_err(crate::ServerError::Source)?;
    let other = {
        state
            .review
            .read()
            .expect("review lock poisoned")
            .other
            .clone()
    };
    let other_meta = if let Some(other) = &other
        && other.state == BuildState::Ready
    {
        let prepared = source.clone();
        let unit = other.unit;
        Some(Arc::new(
            tokio::task::spawn_blocking(move || prepared.review_unit(unit))
                .await
                .map_err(|e| crate::ServerError::Stopped(e.to_string()))?
                .map_err(crate::ServerError::Source)?,
        ))
    } else {
        None
    };
    let start_build = {
        let _gate = state.persist.lock().expect("persist poisoned");
        if *state.shutdown.borrow()
            || matches!(
                *state.submit_state.lock().expect("submit poisoned"),
                crate::SubmitState::Claimed
            )
        {
            return Err(crate::ServerError::Stopped("the review is stopping".into()));
        }
        let mut review = state.review.write().expect("review lock poisoned");
        let mut session = state.session.lock().expect("session poisoned");
        let display = session
            .state
            .display
            .as_ref()
            .expect("initial display assigned")
            .next(&state.display_id)
            .map_err(|e| crate::ServerError::Stopped(e.to_string()))?;
        let generation = display.issued;
        let mut snapshot = session.snapshot();
        snapshot.display = Some(display.clone());
        if let Some(sink) = &state.session_sink {
            sink.save_issued_state(snapshot)
                .map_err(crate::ServerError::Session)?;
        }
        state.sources.publish(source.clone());
        review.startup = Arc::new(startup);
        review.code_error = None;
        let mut start = false;
        if let Some(current) = review.other.as_mut() {
            current.generation = generation;
            current.meta = other_meta;
            current.state = if current.meta.is_some() {
                BuildState::Ready
            } else {
                start = true;
                BuildState::Building
            };
        }
        session.state.display = Some(display);
        start.then_some(generation)
    };
    if let Some(generation) = start_build {
        spawn_build(state.clone(), source, generation);
    }
    let _ = state.events.send(Event::Unit);
    Ok(())
}

/// 作成の結果を記録する。失敗してもレビューは終えない（R-UNIT, R-SUBMIT）。
fn record(
    other: &mut OtherUnit,
    result: Result<Result<ReviewMeta, SourceError>, tokio::task::JoinError>,
) {
    match result {
        Ok(Ok(meta)) => {
            other.meta = Some(Arc::new(meta));
            other.state = BuildState::Ready;
        }
        Ok(Err(error)) => other.state = BuildState::Failed(error.to_string()),
        Err(error) => other.state = BuildState::Failed(error.to_string()),
    }
}
