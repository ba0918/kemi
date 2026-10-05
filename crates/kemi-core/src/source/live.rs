//! `--live` のコードの見方（R-PAGE-MODE）。git の作業ツリーの中では worktree と同じ差分を
//! 配り、外ではファイルを持たない。題は見る対象のページで決まる（R-INPUT）。

use std::path::{Path, PathBuf};

use crate::domain::live::LivePage;
use crate::domain::review::{ReviewMeta, Side};

use super::git::{GitMode, GitSource, repo_root};
use super::{FileContent, ReviewSource, SourceError};

pub struct LiveSource {
    /// git の作業ツリーの外では None（コードの見方を出さない）。
    code: Option<GitSource>,
    title: String,
}

impl LiveSource {
    /// `root`（配れる範囲の根）が git の作業ツリーの中なら、その作業ツリーの差分を読む。
    /// 復元でも呼ぶので、git の中かどうかはその時点で判定し直す（R-PAGE-SESSION）。
    pub fn new(page: &LivePage, root: &Path) -> Self {
        LiveSource {
            code: repo_root(root)
                .ok()
                .map(|repo| GitSource::new(repo, GitMode::Worktree)),
            title: page.title(),
        }
    }

    /// コードの見方を出せるか（git の作業ツリーの中か）。
    pub fn has_code(&self) -> bool {
        self.code.is_some()
    }

    fn code(&self, file_id: &str) -> Result<&GitSource, SourceError> {
        self.code
            .as_ref()
            .ok_or_else(|| SourceError::UnknownFileId(file_id.to_string()))
    }
}

impl ReviewSource for LiveSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        let mut review = match &self.code {
            Some(code) => code.review()?,
            None => ReviewMeta {
                title: String::new(),
                subtitle: String::new(),
                meta: serde_json::Value::Null,
                groups: Vec::new(),
                approval: Vec::new(),
            },
        };
        review.title = self.title.clone();
        Ok(review)
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.code(file_id)?.content(file_id)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.code
            .as_ref()
            .map(ReviewSource::watch_paths)
            .unwrap_or_default()
    }

    fn reads_repository(&self) -> bool {
        self.code.is_some()
    }

    fn repository_file(
        &self,
        file_id: &str,
        side: Side,
        path: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        match &self.code {
            Some(code) => code.repository_file(file_id, side, path, limit),
            None => Ok(None),
        }
    }
}
