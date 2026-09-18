//! 凍結したレビューを配るソース（R-SESSION）。復元では元の入力（コミット範囲、
//! worktree、manifest）を読み直さず、セッションの写しだけからメタデータと内容を返す。
//! 由来だけは、記録した範囲の sha を使って元のワークスペースのリポジトリから計算する。

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::review::{FileEntry, GroupBy, ReviewMeta, Status};
use crate::session::{SessionCopy, SessionInfo, SessionMode};
use crate::source::origin::{file_origin, OriginPaths, OriginRange};

use super::{FileContent, FileOrigin, ReviewSource, SourceError};

/// セッションの写しからレビューを配る。
pub struct FrozenSource {
    workspace: PathBuf,
    range: Option<(String, String)>,
    startup_unit: Option<GroupBy>,
    units: Vec<(Option<GroupBy>, ReviewMeta)>,
    contents: BTreeMap<String, FileContent>,
}

impl FrozenSource {
    pub fn new(info: &SessionInfo, copy: &SessionCopy) -> Self {
        let range = match &info.mode {
            SessionMode::Range {
                from_sha, to_sha, ..
            } => Some((from_sha.clone(), to_sha.clone())),
            _ => None,
        };
        FrozenSource {
            workspace: info.workspace.clone(),
            range,
            startup_unit: copy.startup_unit,
            units: copy
                .units
                .iter()
                .map(|frozen| (frozen.unit, frozen.review.clone()))
                .collect(),
            contents: copy.contents.clone(),
        }
    }

    fn startup_review(&self) -> Result<ReviewMeta, SourceError> {
        self.units
            .iter()
            .find(|(unit, _)| *unit == self.startup_unit)
            .or_else(|| self.units.first())
            .map(|(_, review)| review.clone())
            .ok_or_else(|| SourceError::Git("the session has no review".to_string()))
    }

    /// 最終形のファイルの、凍結したメタデータから導ける左右のパス（R-ORIGIN）。
    fn origin_paths(&self, file_id: &str) -> Option<OriginPaths> {
        let (_, review) = self
            .units
            .iter()
            .find(|(unit, _)| *unit == Some(GroupBy::File))?;
        let file: &FileEntry = review
            .groups
            .iter()
            .flat_map(|group| &group.files)
            .find(|file| file.id == file_id)?;
        Some(OriginPaths {
            old: (file.status != Status::Add)
                .then(|| PathBuf::from(file.old_path.clone().unwrap_or_else(|| file.path.clone()))),
            new: (file.status != Status::Delete).then(|| PathBuf::from(file.path.clone())),
        })
    }
}

impl ReviewSource for FrozenSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.startup_review()
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.contents
            .get(file_id)
            .cloned()
            .ok_or_else(|| SourceError::UnknownFileId(file_id.to_string()))
    }

    fn units(&self) -> Vec<GroupBy> {
        match self.startup_unit {
            Some(unit) => vec![unit, unit.other()],
            None => Vec::new(),
        }
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        self.units
            .iter()
            .find(|(stored, _)| *stored == Some(unit))
            .map(|(_, review)| review.clone())
            .ok_or_else(|| SourceError::Git(format!("the session has no {} unit", unit.as_str())))
    }

    /// 由来は記録した範囲の sha で、元のワークスペースのリポジトリから計算する。
    /// リポジトリが無い・パスが導けないファイルは `None`（画面は「特定できない」）。
    fn origin(&self, file_id: &str, force: bool) -> Result<Option<FileOrigin>, SourceError> {
        let Some((from, to)) = &self.range else {
            return Ok(None);
        };
        let Some(paths) = self.origin_paths(file_id) else {
            return Ok(None);
        };
        let Some(content) = self.contents.get(file_id) else {
            return Ok(None);
        };
        let Ok(repo) = crate::source::git::repo_root(&self.workspace) else {
            return Ok(None);
        };
        let merge_base = crate::source::git::git_text(&repo, &["merge-base", from, to])?
            .trim()
            .to_string();
        file_origin(
            &repo,
            &OriginRange {
                from: from.clone(),
                to: to.clone(),
                merge_base,
            },
            &paths,
            content,
            force,
        )
    }

    /// 復元したレビューでは監視も更新バッジも行わない（R-LIVE）。
    fn watch_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::FrozenUnit;
    use crate::source::git::{GitMode, GitSource};
    use crate::source::testutil::TempRepo;

    fn fixture() -> (TempRepo, String, String) {
        let repo = TempRepo::new();
        repo.write("src/a.rs", "one\ntwo\nthree\n");
        repo.write("src/b.rs", "bee\n");
        let first = repo.add_and_commit("first");
        repo.write("src/a.rs", "one\nTWO\nthree\n");
        repo.write("src/c.rs", "new\n");
        let second = repo.add_and_commit("second");
        (repo, first, second)
    }

    fn range_source(repo: &TempRepo, from: &str, to: &str) -> GitSource {
        GitSource::new(
            repo.path.clone(),
            GitMode::Range {
                from: from.to_string(),
                to: to.to_string(),
                group_by: GroupBy::File,
            },
        )
    }

    /// 元のソースから、セッションの情報と写しを作る。
    fn freeze(
        repo: &TempRepo,
        from: &str,
        to: &str,
        original: &GitSource,
    ) -> (SessionInfo, SessionCopy) {
        let final_review = original.review().unwrap();
        let commit_review = original.review_unit(GroupBy::Commit).unwrap();
        let contents: BTreeMap<String, FileContent> = final_review
            .groups
            .iter()
            .flat_map(|group| &group.files)
            .chain(commit_review.groups.iter().flat_map(|group| &group.files))
            .map(|file| (file.id.clone(), original.content(&file.id).unwrap()))
            .collect();
        let info = SessionInfo {
            id: "01HF7YAT00AAAAAAAAAAAAAAAA".to_string(),
            created: 1,
            updated: 1,
            workspace: repo.path.clone(),
            workspace_key: "00000000000000aa".to_string(),
            mode: SessionMode::Range {
                from: from.to_string(),
                to: to.to_string(),
                from_sha: from.to_string(),
                to_sha: to.to_string(),
            },
            title: format!("{from}..{to}"),
            total_files: final_review.groups[0].files.len(),
        };
        let copy = SessionCopy {
            startup_unit: Some(GroupBy::File),
            units: vec![
                FrozenUnit {
                    unit: Some(GroupBy::File),
                    review: final_review,
                },
                FrozenUnit {
                    unit: Some(GroupBy::Commit),
                    review: commit_review,
                },
            ],
            contents,
        };
        (info, copy)
    }

    #[test]
    fn frozen_source_returns_the_same_metadata_and_contents() {
        let (repo, from, to) = fixture();
        let original = range_source(&repo, &from, &to);
        let (info, copy) = freeze(&repo, &from, &to, &original);
        let frozen = FrozenSource::new(&info, &copy);

        assert_eq!(frozen.review().unwrap(), original.review().unwrap());
        assert_eq!(
            frozen.review_unit(GroupBy::Commit).unwrap(),
            original.review_unit(GroupBy::Commit).unwrap()
        );
        assert_eq!(frozen.units(), vec![GroupBy::File, GroupBy::Commit]);
        for (id, content) in &copy.contents {
            assert_eq!(frozen.content(id).unwrap(), *content);
        }
        assert!(frozen.watch_paths().is_empty());
    }

    #[test]
    fn frozen_source_returns_the_same_origin() {
        let (repo, from, to) = fixture();
        let original = range_source(&repo, &from, &to);
        let (info, copy) = freeze(&repo, &from, &to, &original);
        let frozen = FrozenSource::new(&info, &copy);
        let files = original.review().unwrap().groups[0].files.clone();

        for file in &files {
            assert_eq!(
                frozen.origin(&file.id, false).unwrap(),
                original.origin(&file.id, false).unwrap(),
                "origin of {}",
                file.path
            );
        }
    }

    #[test]
    fn frozen_source_origin_is_none_without_the_repository() {
        let (repo, from, to) = fixture();
        let original = range_source(&repo, &from, &to);
        let (info, copy) = freeze(&repo, &from, &to, &original);
        let frozen = FrozenSource::new(&info, &copy);
        let file_id = original.review().unwrap().groups[0].files[0].id.clone();

        std::fs::remove_dir_all(&repo.path).unwrap();

        assert_eq!(frozen.origin(&file_id, false).unwrap(), None);
    }

    #[test]
    fn frozen_source_rejects_an_unknown_file_id() {
        let (repo, from, to) = fixture();
        let original = range_source(&repo, &from, &to);
        let (info, copy) = freeze(&repo, &from, &to, &original);
        let frozen = FrozenSource::new(&info, &copy);

        assert!(matches!(
            frozen.content("missing"),
            Err(SourceError::UnknownFileId(_))
        ));
    }
}
