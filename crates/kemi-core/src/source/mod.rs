//! 入力ソース（R-INPUT）。manifest / git の読み取りと、表示時の行内容取得。

pub mod git;
pub mod manifest;

#[cfg(test)]
pub(crate) mod testutil;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::domain::focus;
use crate::domain::review::ReviewMeta;

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
                write!(formatter, "{} を読めません: {source}", path.display())
            }
            SourceError::Manifest(message)
            | SourceError::Focus(message)
            | SourceError::Git(message) => formatter.write_str(message),
            SourceError::UnknownFileId(id) => write!(formatter, "不明なファイル id: {id}"),
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

/// レビューの供給元。`review` はメタデータと統計、`content` は表示時の行内容を返す。
pub trait ReviewSource: Send + Sync {
    fn review(&self) -> Result<ReviewMeta, SourceError>;
    fn content(&self, file_id: &str) -> Result<FileContent, SourceError>;
}

/// 表示時に内容を読むための参照。
#[derive(Clone, Debug)]
pub(crate) enum SideRef {
    Absent,
    Inline(Vec<u8>),
    Disk(PathBuf),
    Git { repo: PathBuf, spec: String },
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

impl Plan {
    pub fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        let file = self
            .files
            .iter()
            .find(|file| file.id == file_id)
            .ok_or_else(|| SourceError::UnknownFileId(file_id.to_string()))?;
        Ok(FileContent {
            old: read_side(&file.old)?,
            new: read_side(&file.new)?,
        })
    }
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

/// `review()` が組み立てた内容参照の計画を保持する。
pub(crate) struct PlanStore {
    state: Mutex<Plan>,
}

impl PlanStore {
    pub fn new() -> Self {
        PlanStore {
            state: Mutex::new(Plan::default()),
        }
    }

    pub fn update(&self, plan: Plan) {
        *self.state.lock().expect("plan store poisoned") = plan;
    }

    pub fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.state
            .lock()
            .expect("plan store poisoned")
            .content(file_id)
    }
}

impl Default for PlanStore {
    fn default() -> Self {
        Self::new()
    }
}

/// `--focus` のファイルを読み、レビューへ後付けする。パスは `base` 相対。
pub fn apply_focus_path(
    review: &mut ReviewMeta,
    focus_path: &Path,
    base: &Path,
) -> Result<(), SourceError> {
    let full = base.join(focus_path);
    let text = std::fs::read_to_string(&full).map_err(|source| SourceError::Io {
        path: full.clone(),
        source,
    })?;
    let layer = focus::parse_focus(&text).map_err(|error| SourceError::Focus(error.to_string()))?;
    focus::apply_focus(review, &layer).map_err(|error| match error {
        focus::FocusError::UnknownGroup(id) => {
            SourceError::Focus(format!("focus のグループ id が見つかりません: {id}"))
        }
        focus::FocusError::UnknownPath(path) => {
            SourceError::Focus(format!("focus のパスが見つかりません: {path}"))
        }
        focus::FocusError::Parse(message) => SourceError::Focus(message),
    })
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

    #[test]
    fn focus_file_overrides_watch_and_sets_note() {
        let mut review = review_with_file("src/a.rs");
        let focus_path = Path::new("tests/fixtures/focus.json");

        apply_focus_path(&mut review, focus_path, &base_dir()).unwrap();

        assert_eq!(review.groups[0].watch, "focus の watch");
        assert!(review.groups[0].files[0].focus);
        assert_eq!(review.groups[0].files[0].note, "focus の note");
    }

    #[test]
    fn focus_file_unknown_path_names_the_path() {
        let mut review = review_with_file("src/a.rs");
        let focus_path = Path::new("tests/fixtures/focus-unknown.json");

        let error = apply_focus_path(&mut review, focus_path, &base_dir()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("src/missing.rs"), "{message}");
    }
}
