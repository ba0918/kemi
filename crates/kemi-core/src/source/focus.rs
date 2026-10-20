//! `--focus` を後付けする装飾ソース（R-FOCUS）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{FileContent, FileOrigin, ReferenceMetadata, ReviewSource, SourceError, work_tree};
use crate::domain::explanation::{CodeReferenceEntry, ReferenceUnit};
use crate::domain::focus::{self, FocusTargets};
use crate::domain::review::{GroupBy, ReviewMeta, Side};

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
    use crate::domain::review::{FileEntry, Group};
    use crate::source::git;
    use crate::testutil;
    use serde_json::json;

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
                    add: 1,
                    del: 1,
                    ..testutil::file_entry(path)
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
        let source = FocusSource::from_path(
            inner,
            Path::new("tests/fixtures/focus.json"),
            &testutil::workspace_root(),
        )
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
            &testutil::workspace_root(),
        )
        .unwrap();

        let error = source.review().unwrap_err();
        let message = error.to_string();
        assert!(message.contains("src/missing.rs"), "{message}");
    }
}
