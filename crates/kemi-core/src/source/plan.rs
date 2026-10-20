//! 内容参照の計画。`review()` が組み立てた、ファイル id ごとの左右の内容の読み方を持ち、
//! 表示時にそれで読む。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{FileContent, ReferenceMetadata, SourceError};
use crate::domain::review::GroupBy;

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

/// 1 単位の計画と、ファイル id から引く索引。表示のたびに全ファイルを探さないため。
pub(super) struct IndexedPlan {
    files: Vec<PlannedFile>,
    file_positions: HashMap<String, usize>,
}

impl IndexedPlan {
    fn new(plan: Plan) -> Self {
        let mut file_positions = HashMap::with_capacity(plan.files.len());
        for (position, file) in plan.files.iter().enumerate() {
            // 同じ id が複数あっても、従来の Vec 探索と同じ先頭の参照を選ぶ。
            file_positions.entry(file.id.clone()).or_insert(position);
        }
        Self {
            files: plan.files,
            file_positions,
        }
    }

    fn planned(&self, file_id: &str) -> Option<&PlannedFile> {
        self.file_positions
            .get(file_id)
            .map(|&position| &self.files[position])
    }
}

/// `review()` が組み立てた内容参照の計画を保持する。コミット範囲では単位ごとに持ち、
/// 片方を差し替えても、もう片方のファイル id は引けるままにする。
pub(crate) struct PlanStore {
    pub(super) state: Mutex<HashMap<Option<GroupBy>, IndexedPlan>>,
}

impl PlanStore {
    pub fn reference_metadata(&self, file_id: &str) -> Option<ReferenceMetadata> {
        self.state
            .lock()
            .expect("plan store poisoned")
            .values()
            .find_map(|plan| plan.planned(file_id))
            .map(|file| ReferenceMetadata {
                old_present: !matches!(file.old, SideRef::Absent),
                new_present: !matches!(file.new, SideRef::Absent),
                old_lines: file.old_lines,
                new_lines: file.new_lines,
            })
    }
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
            .insert(unit, IndexedPlan::new(plan));
    }

    pub fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        let file = self
            .state
            .lock()
            .expect("plan store poisoned")
            .values()
            .find_map(|plan| plan.planned(file_id))
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
            .find_map(|plan| plan.planned(file_id))
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
