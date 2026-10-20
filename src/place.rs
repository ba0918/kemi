//! 起動した場所と、そこから決まるリポジトリの根（R-RESULT）。

use std::path::{Path, PathBuf};

/// 起動したディレクトリなど、レビューを識別する場所。git の作業ツリーの根は要るときに
/// 1 回だけ求める（`git rev-parse` はプロセスの起動で、起動 1 回で何度も呼ぶと遅い）。
pub(crate) struct LaunchPlace {
    path: PathBuf,
    repo: std::cell::OnceCell<Result<PathBuf, String>>,
}

impl LaunchPlace {
    pub(crate) fn at(path: PathBuf) -> Self {
        LaunchPlace {
            path,
            repo: std::cell::OnceCell::new(),
        }
    }

    /// git の作業ツリーの根。git の外では誤り（コミット範囲の解決やソースに要る）。
    pub(crate) fn repo(&self) -> Result<&Path, String> {
        self.repo
            .get_or_init(|| {
                kemi_core::source::git::repo_root(&self.path).map_err(|error| error.to_string())
            })
            .as_deref()
            .map_err(Clone::clone)
    }

    /// 結果とセッションを識別するリポジトリのトップ（R-RESULT）。git の外ならその場所そのもの。
    pub(crate) fn workspace(&self) -> PathBuf {
        match self.repo() {
            Ok(repo) => repo.to_path_buf(),
            Err(_) => std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone()),
        }
    }
}
