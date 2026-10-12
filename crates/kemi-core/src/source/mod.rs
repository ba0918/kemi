//! 入力ソース（R-INPUT）。manifest / git の読み取りと、表示時の行内容取得。

mod frozen;
pub mod git;
pub mod html_files;
mod live;
pub mod manifest;
mod origin;
pub mod work_tree;

pub use frozen::FrozenSource;
pub use live::LiveSource;

#[cfg(test)]
pub(crate) mod testutil;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::domain::explanation::{CodeReferenceEntry, ReferenceUnit};
use crate::domain::focus::{self, FocusTargets};
use crate::domain::origin::{BlockOrigin, RangeCommit};
use crate::domain::review::{GroupBy, ReviewMeta, Side};

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

#[derive(Clone, Debug)]
pub struct ReferenceMetadata {
    pub old_present: bool,
    pub new_present: bool,
    pub old_lines: Option<u32>,
    pub new_lines: Option<u32>,
}

pub(crate) fn known_lines(bytes: Option<&[u8]>) -> Option<u32> {
    let bytes = bytes?;
    if crate::domain::content::is_binary(bytes) {
        return None;
    }
    if bytes.is_empty() {
        return Some(0);
    }
    let separators = bytes
        .iter()
        .enumerate()
        .filter(|(index, byte)| {
            **byte == b'\r' || (**byte == b'\n' && (*index == 0 || bytes[*index - 1] != b'\r'))
        })
        .count();
    let unterminated = usize::from(!matches!(bytes.last(), Some(b'\r' | b'\n')));
    u32::try_from(separators + unterminated).ok()
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
    fn reference_metadata(&self, file_id: &str) -> Option<ReferenceMetadata> {
        let _ = file_id;
        None
    }
    fn select_references(
        &self,
        requested: &[(ReferenceUnit, String, String)],
    ) -> Result<Vec<CodeReferenceEntry>, SourceError> {
        let _ = requested;
        Ok(Vec::new())
    }
    /// 内容参照を変更するソースは、未公開の作成対象を分ける。参照計画が不変のソースはNone。
    fn fork(&self) -> Result<Option<Arc<dyn ReviewSource>>, SourceError> {
        Ok(None)
    }
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
    /// 全体を見張る作業ツリー（R-LIVE の worktree）。None なら `watch_paths` だけを見張る。
    fn work_tree(&self) -> Option<work_tree::WorkTree> {
        None
    }
    /// リポジトリの中のファイルを、描画の時点で読めるか（R-RENDER の相対パス画像）。
    /// git の入力モードだけが真で、manifest と復元は偽。
    fn reads_repository(&self) -> bool {
        false
    }
    /// リポジトリの中のファイルを、レビュー対象ファイル `file_id` の `side` の版で読む
    /// （R-RENDER の相対パス画像）。symlink は辿らず配らず、`limit` を超えるものと無い
    /// ものは None。読めない入力（manifest・復元）は常に None。
    fn repository_file(
        &self,
        file_id: &str,
        side: Side,
        path: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        let _ = (file_id, side, path, limit);
        Ok(None)
    }
}

pub struct SourceSlot {
    factory: Arc<dyn ReviewSource>,
    current: RwLock<Arc<dyn ReviewSource>>,
}
impl SourceSlot {
    pub fn new(factory: Arc<dyn ReviewSource>, current: Arc<dyn ReviewSource>) -> Self {
        Self {
            factory,
            current: RwLock::new(current),
        }
    }
    pub fn current(&self) -> Arc<dyn ReviewSource> {
        self.current.read().expect("source slot poisoned").clone()
    }
    pub fn prepare(&self) -> Result<Arc<dyn ReviewSource>, SourceError> {
        Ok(self.factory.fork()?.unwrap_or_else(|| self.factory.clone()))
    }
    pub fn publish(&self, candidate: Arc<dyn ReviewSource>) {
        *self.current.write().expect("source slot poisoned") = candidate;
    }
}
impl ReviewSource for SourceSlot {
    fn reference_metadata(&self, file_id: &str) -> Option<ReferenceMetadata> {
        self.current().reference_metadata(file_id)
    }
    fn select_references(
        &self,
        requested: &[(ReferenceUnit, String, String)],
    ) -> Result<Vec<CodeReferenceEntry>, SourceError> {
        self.current().select_references(requested)
    }
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.current().review()
    }
    fn content(&self, id: &str) -> Result<FileContent, SourceError> {
        self.current().content(id)
    }
    fn units(&self) -> Vec<GroupBy> {
        self.current().units()
    }
    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        self.current().review_unit(unit)
    }
    fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
        self.current().extra_focus_targets()
    }
    fn origin(&self, id: &str, force: bool) -> Result<Option<FileOrigin>, SourceError> {
        self.current().origin(id, force)
    }
    fn watch_paths(&self) -> Vec<PathBuf> {
        self.factory
            .watch_paths()
            .into_iter()
            .chain(self.current().watch_paths())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    fn work_tree(&self) -> Option<work_tree::WorkTree> {
        self.current().work_tree()
    }
    fn reads_repository(&self) -> bool {
        self.current().reads_repository()
    }
    fn repository_file(
        &self,
        id: &str,
        side: Side,
        path: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        self.current().repository_file(id, side, path, limit)
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
    pub old_lines: Option<u32>,
    pub new_lines: Option<u32>,
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

/// `read_side` の所有版。手放してよい参照なら、manifest のインラインの内容を写さずに返す。
fn read_owned_side(side: SideRef) -> Result<Option<Vec<u8>>, SourceError> {
    match side {
        SideRef::Inline(bytes) => Ok(Some(bytes)),
        SideRef::Absent | SideRef::Disk(_) | SideRef::Git { .. } => read_side(&side),
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
    metadata: Mutex<HashMap<String, ReferenceMetadata>>,
}

impl PlanStore {
    pub fn reference_metadata(&self, file_id: &str) -> Option<ReferenceMetadata> {
        self.metadata
            .lock()
            .expect("reference metadata poisoned")
            .get(file_id)
            .cloned()
    }
    pub fn new() -> Self {
        PlanStore {
            state: Mutex::new(HashMap::new()),
            metadata: Mutex::new(HashMap::new()),
        }
    }

    pub fn update(&self, plan: Plan) {
        self.update_unit(None, plan);
    }

    pub fn update_unit(&self, unit: Option<GroupBy>, plan: Plan) {
        let mut state = self.state.lock().expect("plan store poisoned");
        let mut metadata = self.metadata.lock().expect("reference metadata poisoned");
        if let Some(old) = state.get(&unit) {
            for file in &old.files {
                metadata.remove(&file.id);
            }
        }
        for file in &plan.files {
            metadata.insert(
                file.id.clone(),
                ReferenceMetadata {
                    old_present: !matches!(file.old, SideRef::Absent),
                    new_present: !matches!(file.new, SideRef::Absent),
                    old_lines: file.old_lines,
                    new_lines: file.new_lines,
                },
            );
        }
        state.insert(unit, plan);
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
            old: read_owned_side(file.old)?,
            new: read_owned_side(file.new)?,
        })
    }

    /// ファイル id の内容参照の計画。
    pub fn planned(&self, file_id: &str) -> Option<PlannedFile> {
        self.state
            .lock()
            .expect("plan store poisoned")
            .values()
            .flat_map(|plan| plan.files.iter())
            .find(|file| file.id == file_id)
            .cloned()
    }

    /// ディスク上の新側ファイル（worktree の監視対象）。
    pub fn disk_paths(&self) -> Vec<PathBuf> {
        let plans = self.state.lock().expect("plan store poisoned");
        plans
            .values()
            .flat_map(|plan| plan.files.iter())
            .filter_map(|file| match &file.new {
                SideRef::Disk(path) => Some(path.clone()),
                SideRef::Absent | SideRef::Inline(_) | SideRef::Git { .. } => None,
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
    inner: Arc<dyn ReviewSource>,
    layer: focus::FocusLayer,
    validated: Arc<AtomicBool>,
}

impl FocusSource {
    pub fn new(inner: Box<dyn ReviewSource>, layer: focus::FocusLayer) -> Self {
        FocusSource {
            inner: Arc::from(inner),
            layer,
            validated: Arc::new(AtomicBool::new(false)),
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
    fn reference_metadata(&self, file_id: &str) -> Option<ReferenceMetadata> {
        self.inner.reference_metadata(file_id)
    }
    fn select_references(
        &self,
        requested: &[(ReferenceUnit, String, String)],
    ) -> Result<Vec<CodeReferenceEntry>, SourceError> {
        self.inner.select_references(requested)
    }
    fn fork(&self) -> Result<Option<Arc<dyn ReviewSource>>, SourceError> {
        Ok(self.inner.fork()?.map(|inner| {
            Arc::new(FocusSource {
                inner,
                layer: self.layer.clone(),
                validated: self.validated.clone(),
            }) as Arc<dyn ReviewSource>
        }))
    }
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

    fn work_tree(&self) -> Option<work_tree::WorkTree> {
        self.inner.work_tree()
    }

    fn reads_repository(&self) -> bool {
        self.inner.reads_repository()
    }

    fn repository_file(
        &self,
        file_id: &str,
        side: Side,
        path: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        self.inner.repository_file(file_id, side, path, limit)
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
                    content_skipped: false,
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
    fn focus_keeps_the_work_tree_of_the_source_it_wraps() {
        let repo = testutil::TempRepo::new();
        let inner = Box::new(git::GitSource::new(
            repo.path.clone(),
            git::GitMode::Worktree,
        ));
        let source = FocusSource::new(inner, focus::parse_focus("{}").unwrap());

        assert_eq!(
            source.work_tree(),
            Some(work_tree::WorkTree::new(repo.path.clone()))
        );
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
