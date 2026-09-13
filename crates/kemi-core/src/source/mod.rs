//! 入力ソース（R-INPUT）。manifest / git の読み取りと、表示時の行内容取得。

pub mod git;
pub mod manifest;
mod origin;

#[cfg(test)]
pub(crate) mod testutil;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::domain::focus::{self, FocusTargets};
use crate::domain::origin::{BlockOrigin, RangeCommit};
use crate::domain::review::{GroupBy, ReviewMeta};

#[derive(Debug)]
pub enum SourceError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Manifest(String),
    Focus(String),
    Git(String),
    UnknownFileId(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Io { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            SourceError::Manifest(message)
            | SourceError::Focus(message)
            | SourceError::Git(message) => formatter.write_str(message),
            SourceError::UnknownFileId(id) => write!(formatter, "unknown file id: {id}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// 表示時に取り出す 1 ファイルの内容。None はその側が存在しない（追加・削除）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileContent {
    pub old: Option<Vec<u8>>,
    pub new: Option<Vec<u8>>,
}

/// 最終形の 1 ファイルの由来（R-ORIGIN）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOrigin {
    /// 上限を超えるファイルで、有効にされていないときは false（ブロックは空）。
    pub enabled: bool,
    pub blocks: Vec<BlockOrigin>,
    /// ブロックが指すコミット。件名と本文を画面に出すために添える。
    pub commits: Vec<RangeCommit>,
}

/// レビューの供給元。`review` はメタデータと統計、`content` は表示時の行内容を返す。
pub trait ReviewSource: Send + Sync {
    /// 起動時に表示するグループ単位のメタデータ。
    fn review(&self) -> Result<ReviewMeta, SourceError>;
    fn content(&self, file_id: &str) -> Result<FileContent, SourceError>;
    /// このレビューが持つグループ単位（R-UNIT）。コミット範囲では起動時の単位を先頭に
    /// 2 つ、ほかのモードでは空（単位は 1 つだけ）。
    fn units(&self) -> Vec<GroupBy> {
        Vec::new()
    }
    /// 指定したグループ単位のメタデータ。取得し直しても、もう片方の単位のファイル id は
    /// 引けるままで、同じグループ・パスの id は変わらない。
    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        let _ = unit;
        self.review()
    }
    /// `--focus` の照合で、`review()` の結果のほかに「存在する」とみなすグループ id と
    /// パス（コミット範囲のもう片方の単位のもの）。
    fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
        Ok(FocusTargets::default())
    }
    /// 最終形のファイルの由来。`force` で上限を超えるファイルでも求める。
    /// 由来を持たないファイル（最終形以外）では None。
    fn origin(&self, file_id: &str, force: bool) -> Result<Option<FileOrigin>, SourceError> {
        let _ = (file_id, force);
        Ok(None)
    }
    /// 監視する「新側の供給元」のパス（R-LIVE）。空なら監視しない。
    fn watch_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }
}

/// 表示時に内容を読むための参照。
#[derive(Clone, Debug)]
pub(crate) enum SideRef {
    Absent,
    Inline(Vec<u8>),
    Disk(PathBuf),
    /// `spec` は `HEAD:src/a.rs` のような git 引数。非 UTF-8 のパスを
    /// そのまま運ぶため OsString で持つ。
    Git {
        repo: PathBuf,
        spec: std::ffi::OsString,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedFile {
    pub id: String,
    pub old: SideRef,
    pub new: SideRef,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Plan {
    pub files: Vec<PlannedFile>,
}

pub(crate) fn read_side(side: &SideRef) -> Result<Option<Vec<u8>>, SourceError> {
    match side {
        SideRef::Absent => Ok(None),
        SideRef::Inline(bytes) => Ok(Some(bytes.clone())),
        SideRef::Disk(path) => read_file(path).map(Some),
        SideRef::Git { repo, spec } => crate::source::git::show(repo, spec).map(Some),
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, SourceError> {
    std::fs::read(path).map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// 左右の生バイトからバイナリ判定と増減数を求める（manifest の整列統計）。
pub(crate) fn text_stats(
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
) -> (bool, u32, u32, u64, u64) {
    let old_size = old.as_ref().map_or(0, |bytes| bytes.len() as u64);
    let new_size = new.as_ref().map_or(0, |bytes| bytes.len() as u64);
    let binary = old
        .as_deref()
        .is_some_and(crate::domain::content::is_binary)
        || new
            .as_deref()
            .is_some_and(crate::domain::content::is_binary);
    if binary {
        return (true, 0, 0, old_size, new_size);
    }

    let old_lines =
        crate::domain::content::lines(&String::from_utf8_lossy(old.as_deref().unwrap_or_default()));
    let new_lines =
        crate::domain::content::lines(&String::from_utf8_lossy(new.as_deref().unwrap_or_default()));
    let rows = crate::domain::diff::align(&old_lines, &new_lines);
    let add = rows
        .iter()
        .filter(|row| {
            matches!(
                row.kind,
                crate::domain::diff::RowKind::Insert | crate::domain::diff::RowKind::Replace
            )
        })
        .count() as u32;
    let del = rows
        .iter()
        .filter(|row| {
            matches!(
                row.kind,
                crate::domain::diff::RowKind::Delete | crate::domain::diff::RowKind::Replace
            )
        })
        .count() as u32;
    (false, add, del, old_size, new_size)
}

/// `review()` が組み立てた内容参照の計画を保持する。コミット範囲では単位ごとに持ち、
/// 片方を差し替えても、もう片方のファイル id は引けるままにする。
pub(crate) struct PlanStore {
    state: Mutex<HashMap<Option<GroupBy>, Plan>>,
}

impl PlanStore {
    pub fn new() -> Self {
        PlanStore {
            state: Mutex::new(HashMap::new()),
        }
    }

    pub fn update(&self, plan: Plan) {
        self.update_unit(None, plan);
    }

    pub fn update_unit(&self, unit: Option<GroupBy>, plan: Plan) {
        self.state
            .lock()
            .expect("plan store poisoned")
            .insert(unit, plan);
    }

    pub fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        let file = self
            .state
            .lock()
            .expect("plan store poisoned")
            .values()
            .flat_map(|plan| plan.files.iter())
            .find(|file| file.id == file_id)
            .cloned()
            .ok_or_else(|| SourceError::UnknownFileId(file_id.to_string()))?;
        Ok(FileContent {
            old: read_side(&file.old)?,
            new: read_side(&file.new)?,
        })
    }

    /// ディスク上の新側ファイル（worktree の監視対象）。
    pub fn disk_paths(&self) -> Vec<PathBuf> {
        let plans = self.state.lock().expect("plan store poisoned");
        plans
            .values()
            .flat_map(|plan| plan.files.iter())
            .filter_map(|file| match &file.new {
                SideRef::Disk(path) => Some(path.clone()),
                _ => None,
            })
            .collect()
    }
}

impl Default for PlanStore {
    fn default() -> Self {
        Self::new()
    }
}

/// `--focus` を後付けする装飾ソース。照合は起動時（最初の `review()`）に 1 回だけ行い、
/// 取得し直したときは照合せずに後付けだけする。
pub struct FocusSource {
    inner: Box<dyn ReviewSource>,
    layer: focus::FocusLayer,
    validated: AtomicBool,
}

impl FocusSource {
    pub fn new(inner: Box<dyn ReviewSource>, layer: focus::FocusLayer) -> Self {
        FocusSource {
            inner,
            layer,
            validated: AtomicBool::new(false),
        }
    }

    /// `--focus` のファイルを読み込む。パスは `base` 相対。
    pub fn from_path(
        inner: Box<dyn ReviewSource>,
        focus_path: &Path,
        base: &Path,
    ) -> Result<Self, SourceError> {
        let full = base.join(focus_path);
        let text = std::fs::read_to_string(&full).map_err(|source| SourceError::Io {
            path: full.clone(),
            source,
        })?;
        let layer =
            focus::parse_focus(&text).map_err(|error| SourceError::Focus(error.to_string()))?;
        Ok(FocusSource::new(inner, layer))
    }

    fn validate_once(&self, review: &ReviewMeta) -> Result<(), SourceError> {
        if self.validated.load(Ordering::SeqCst) {
            return Ok(());
        }
        let mut targets = FocusTargets::of(review);
        targets.extend(self.inner.extra_focus_targets()?);
        focus::validate_focus(&self.layer, &targets)
            .map_err(|error| SourceError::Focus(error.to_string()))?;
        self.validated.store(true, Ordering::SeqCst);
        Ok(())
    }
}

impl ReviewSource for FocusSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        let mut meta = self.inner.review()?;
        self.validate_once(&meta)?;
        focus::apply_focus(&mut meta, &self.layer);
        Ok(meta)
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.inner.content(file_id)
    }

    fn units(&self) -> Vec<GroupBy> {
        self.inner.units()
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        let mut meta = self.inner.review_unit(unit)?;
        focus::apply_focus(&mut meta, &self.layer);
        Ok(meta)
    }

    fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
        self.inner.extra_focus_targets()
    }

    fn origin(&self, file_id: &str, force: bool) -> Result<Option<FileOrigin>, SourceError> {
        self.inner.origin(file_id, force)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.inner.watch_paths()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{FileEntry, Group, Status};
    use serde_json::json;

    fn base_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn review_with_file(path: &str) -> ReviewMeta {
        ReviewMeta {
            title: "変更のレビュー".to_string(),
            subtitle: String::new(),
            meta: json!(null),
            groups: vec![Group {
                id: "g1".to_string(),
                title: String::new(),
                why: String::new(),
                watch: "manifest".to_string(),
                files: vec![FileEntry {
                    id: "f1".to_string(),
                    group_id: "g1".to_string(),
                    path: path.to_string(),
                    old_path: None,
                    status: Status::Modify,
                    add: 1,
                    del: 1,
                    binary: false,
                    old_size: 0,
                    new_size: 0,
                    focus: false,
                    note: String::new(),
                    noise: false,
                }],
            }],
            approval: Vec::new(),
        }
    }

    struct StaticSource {
        meta: ReviewMeta,
    }

    impl ReviewSource for StaticSource {
        fn review(&self) -> Result<ReviewMeta, SourceError> {
            Ok(self.meta.clone())
        }

        fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
            Err(SourceError::UnknownFileId(file_id.to_string()))
        }
    }

    #[test]
    fn focus_file_overrides_watch_and_sets_note() {
        let inner = Box::new(StaticSource {
            meta: review_with_file("src/a.rs"),
        });
        let source =
            FocusSource::from_path(inner, Path::new("tests/fixtures/focus.json"), &base_dir())
                .unwrap();

        let review = source.review().unwrap();
        assert_eq!(review.groups[0].watch, "focus の watch");
        assert!(review.groups[0].files[0].focus);
        assert_eq!(review.groups[0].files[0].note, "focus の note");
    }

    #[test]
    fn focus_file_unknown_path_names_the_path() {
        let inner = Box::new(StaticSource {
            meta: review_with_file("src/a.rs"),
        });
        let source = FocusSource::from_path(
            inner,
            Path::new("tests/fixtures/focus-unknown.json"),
            &base_dir(),
        )
        .unwrap();

        let error = source.review().unwrap_err();
        let message = error.to_string();
        assert!(message.contains("src/missing.rs"), "{message}");
    }
}
