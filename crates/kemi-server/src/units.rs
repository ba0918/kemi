//! グループ単位ごとのレビューの状態（R-UNIT）。起動時の単位はサーブ前に作り、
//! コミット範囲のもう片方の単位は最初の api/review の後に裏で作る。

use std::sync::Arc;

use kemi_core::domain::review::{FileEntry, GroupBy, ReviewMeta};
use kemi_core::source::SourceError;
use serde_json::{json, Value};

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

pub(crate) struct OtherUnit {
    pub unit: GroupBy,
    /// 最後に作り終えたメタデータ。作り直している間や失敗した後も、コメントや見たの
    /// ファイル id を引けるように残す。
    pub meta: Option<Arc<ReviewMeta>>,
    pub state: BuildState,
    /// 再取得のたびに進める。作っている間に進んだら、その結果は捨てて作り直す。
    pub generation: u64,
}

pub(crate) struct ReviewState {
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
            startup_unit: units.first().copied(),
            startup: Arc::new(startup),
            other: units.get(1).map(|unit| OtherUnit {
                unit: *unit,
                meta: None,
                state: BuildState::Waiting,
                generation: 0,
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

    /// 両方のグループ単位のメタデータがそろっているか。凍結はこれが true になるまで待つ
    /// （R-SESSION）。
    pub fn all_units_ready(&self) -> bool {
        self.other
            .as_ref()
            .is_none_or(|other| matches!(other.state, BuildState::Ready))
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
    spawn_build(state.clone(), generation);
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
    spawn_build(state.clone(), generation);
    true
}

/// 再取得の後、作っている途中の結果を捨てさせる。作り終えた結果が再取得後の履歴と
/// 食い違わないよう、今の作成が終わったら作り直す。
pub(crate) fn invalidate_in_flight(state: &AppState) {
    let mut review = state.review.write().expect("review lock poisoned");
    if let Some(other) = review.other.as_mut() {
        if other.state == BuildState::Building {
            other.generation += 1;
        }
    }
}

fn spawn_build(state: Arc<AppState>, generation: u64) {
    tokio::spawn(async move {
        let unit = {
            let review = state.review.read().expect("review lock poisoned");
            match review.other.as_ref() {
                Some(other) => other.unit,
                None => return,
            }
        };
        let source = state.source.clone();
        let result = tokio::task::spawn_blocking(move || source.review_unit(unit)).await;
        let rebuild = {
            let mut review = state.review.write().expect("review lock poisoned");
            let Some(other) = review.other.as_mut() else {
                return;
            };
            if other.generation != generation {
                Some(other.generation)
            } else {
                record(other, result);
                None
            }
        };
        match rebuild {
            Some(next) => spawn_build(state, next),
            None => {
                let _ = state.events.send(Event::Unit);
            }
        }
    });
}

/// 再取得でもう片方の単位を取り直す。作り終えていれば今ここで作り直し、作っている
/// 途中なら作り直させる。まだ作っていない・失敗した単位はそのままにする。
pub(crate) async fn refresh_other(state: &Arc<AppState>) {
    let ready_unit = {
        let review = state.review.read().expect("review lock poisoned");
        review
            .other
            .as_ref()
            .filter(|other| other.state == BuildState::Ready)
            .map(|other| other.unit)
    };
    let Some(unit) = ready_unit else {
        invalidate_in_flight(state);
        return;
    };
    let source = state.source.clone();
    let result = tokio::task::spawn_blocking(move || source.review_unit(unit)).await;
    {
        let mut review = state.review.write().expect("review lock poisoned");
        if let Some(other) = review.other.as_mut() {
            record(other, result);
        }
    }
    let _ = state.events.send(Event::Unit);
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
