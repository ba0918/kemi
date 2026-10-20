//! 入力ソース（R-INPUT）。manifest / git の読み取りと、表示時の行内容取得。

mod focus;
mod frozen;
pub mod git;
pub mod html_files;
mod live;
pub mod manifest;
mod origin;
mod plan;
pub mod work_tree;

pub use crate::domain::content::FileContent;
pub use focus::FocusSource;
pub use frozen::FrozenSource;
pub use live::LiveSource;
pub(crate) use plan::{Plan, PlanStore, PlannedFile, SideRef, read_side};

#[cfg(test)]
pub(crate) mod testutil;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::domain::explanation::{CodeReferenceEntry, ReferenceUnit};
use crate::domain::focus::FocusTargets;
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
    u32::try_from(crate::domain::content::line_count(bytes)).ok()
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
    let (add, del) = crate::domain::diff::line_stats(&old_lines, &new_lines);
    (false, add, del, old_size, new_size)
}
