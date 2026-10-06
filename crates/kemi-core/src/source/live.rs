//! `--live` のコードの見方（R-PAGE-MODE）。git の作業ツリーの中では worktree と同じ差分を
//! 配り、外ではファイルを持たない。題は見る対象のページで決まる（R-INPUT）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::domain::live::LivePage;
use crate::domain::review::{ReviewMeta, Side};

use super::git::{GitMode, GitSource, repo_root};
use super::{FileContent, ReviewSource, SourceError};

pub struct LiveSource {
    /// git の作業ツリーの外では None（コードの見方を出さない）。
    code: Option<GitSource>,
    title: String,
    /// 外に見せるファイルの id から、作業ツリーの差分が振った id への対応。
    ids: Mutex<HashMap<String, String>>,
}

/// パスから決まるファイルの id。作業ツリーの差分が振る id は見つけた順の番号で、復元で
/// 作り直すと別のファイルを指しうる。`--live` は写しを持たずに作り直すので、コメント・
/// 見た・折りたたみが同じファイルに戻るよう、パスから id を決める（R-PAGE-SESSION）。
/// 表示のパスは lossy で別のファイルが同じ文字列になりうるので、生のパスのバイト列から決める
/// （UTF-8 として正しいパスでは表示のパスと同じバイト列なので、id も変わらない）。
fn stable_id(path: &[u8]) -> String {
    let mut id = String::with_capacity(1 + path.len() * 2);
    id.push('p');
    for byte in path {
        id.push_str(&format!("{byte:02x}"));
    }
    id
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
            ids: Mutex::new(HashMap::new()),
        }
    }

    /// コードの見方を出せるか（git の作業ツリーの中か）。
    pub fn has_code(&self) -> bool {
        self.code.is_some()
    }

    /// 外に見せた id を、作業ツリーの差分の id にする。
    fn inner(&self, file_id: &str) -> Result<(&GitSource, String), SourceError> {
        let unknown = || SourceError::UnknownFileId(file_id.to_string());
        let code = self.code.as_ref().ok_or_else(unknown)?;
        let inner = self
            .ids
            .lock()
            .expect("ids poisoned")
            .get(file_id)
            .cloned()
            .ok_or_else(unknown)?;
        Ok((code, inner))
    }
}

impl ReviewSource for LiveSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        let mut review = match &self.code {
            Some(code) => {
                let mut review = code.review()?;
                let raw_paths = code.raw_paths();
                let mut ids = self.ids.lock().expect("ids poisoned");
                for file in review.groups.iter_mut().flat_map(|group| &mut group.files) {
                    let raw = raw_paths
                        .get(&file.id)
                        .expect("review() records the raw path of every id it hands out");
                    let stable = stable_id(raw);
                    ids.insert(stable.clone(), std::mem::replace(&mut file.id, stable));
                }
                review
            }
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
        let (code, inner) = self.inner(file_id)?;
        code.content(&inner)
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
        if self.code.is_none() {
            return Ok(None);
        }
        let (code, inner) = self.inner(file_id)?;
        code.repository_file(&inner, side, path, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_id_is_decided_by_the_path_alone() {
        assert_eq!(stable_id(b"a.txt"), stable_id(b"a.txt"));
        assert_ne!(stable_id(b"a.txt"), stable_id(b"b.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn files_whose_names_differ_only_in_invalid_utf8_bytes_keep_their_own_ids_and_contents() {
        use std::os::unix::ffi::OsStrExt;
        let repo = crate::source::testutil::TempRepo::new();
        repo.write("a.txt", "a\n");
        repo.add_and_commit("base");
        let first = std::ffi::OsStr::from_bytes(b"b\xffad.txt");
        let second = std::ffi::OsStr::from_bytes(b"b\xfead.txt");
        std::fs::write(repo.path.join(first), "first\n").unwrap();
        std::fs::write(repo.path.join(second), "second\n").unwrap();
        let source = LiveSource::new(&LivePage::File("a.txt".to_string()), &repo.path);

        let review = source.review().unwrap();
        let ids: Vec<&str> = review
            .groups
            .iter()
            .flat_map(|group| &group.files)
            .filter(|file| file.path.ends_with("ad.txt"))
            .map(|file| file.id.as_str())
            .collect();

        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        let mut contents: Vec<Vec<u8>> = ids
            .iter()
            .map(|id| source.content(id).unwrap().new.unwrap())
            .collect();
        contents.sort();
        assert_eq!(contents, vec![b"first\n".to_vec(), b"second\n".to_vec()]);
    }
}
