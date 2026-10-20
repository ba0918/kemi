//! リポジトリの中のファイルの読み取り（R-RENDER の相対パス画像）と、経路の安全。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::command::{git_raw_os, object_size, show};
use super::git_spec;
use crate::source::SourceError;

/// `a/b/c.png` の形か。空・`.`・`..`・`\` か `:` を含む区切り（Windows のドライブ接頭辞
/// `C:` はパスの基準を置き換える）と、先頭や末尾の `/` は認めない。
pub(super) fn is_plain_repository_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.contains(['\\', ':'])
        })
}

/// 作業ツリーのファイルを読む。パスのどの部分も symlink なら読まない（辿らない）。
/// 組み立てたパスがリポジトリの外に出たら（絶対パスやドライブ接頭辞の push は基準を
/// 置き換える）、受け口の検査と二重に、読まない。
pub(super) fn read_worktree_file(
    repo: &Path,
    path: &Path,
    limit: u64,
) -> Result<Option<Vec<u8>>, SourceError> {
    let mut full = repo.to_path_buf();
    for component in path.components() {
        full.push(component);
        if !full.starts_with(repo) {
            return Ok(None);
        }
        let Ok(metadata) = std::fs::symlink_metadata(&full) else {
            return Ok(None);
        };
        if metadata.file_type().is_symlink() {
            return Ok(None);
        }
    }
    let Ok(metadata) = std::fs::metadata(&full) else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.len() > limit {
        return Ok(None);
    }
    std::fs::read(&full)
        .map(Some)
        .map_err(|source| SourceError::Io { path: full, source })
}

/// 木（コミットかインデックス）の中のファイルを読む。パスのどの部分も symlink（mode
/// 120000）なら読まない。無いものと上限を超えるものは None。
pub(super) fn read_tree_file(
    repo: &Path,
    revision: &str,
    path: &Path,
    limit: u64,
) -> Result<Option<Vec<u8>>, SourceError> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        let Some(mode) = tree_entry_mode(repo, revision, &prefix)? else {
            return Ok(None);
        };
        if mode == "120000" {
            return Ok(None);
        }
    }
    let spec = git_spec(&format!("{revision}:"), path);
    let Ok(size) = object_size(repo, &spec) else {
        return Ok(None);
    };
    let size = size.unwrap_or(u64::MAX);
    if size > limit {
        return Ok(None);
    }
    show(repo, &spec).map(Some)
}

/// 木の中のエントリの mode（`100644`、`120000` など）。無ければ None。インデックス
/// （空の revision）は `ls-files -s`、コミットは `ls-tree` で引く。
fn tree_entry_mode(
    repo: &Path,
    revision: &str,
    path: &Path,
) -> Result<Option<String>, SourceError> {
    let output = if revision.is_empty() {
        git_raw_os(
            repo,
            &[
                OsStr::new("ls-files"),
                OsStr::new("-s"),
                OsStr::new("-z"),
                OsStr::new("--"),
                path.as_os_str(),
            ],
        )?
    } else {
        git_raw_os(
            repo,
            &[
                OsStr::new("ls-tree"),
                OsStr::new("-z"),
                OsStr::new(revision),
                OsStr::new("--"),
                path.as_os_str(),
            ],
        )?
    };
    // 1 行目の先頭の欄が mode。ディレクトリを ls-files に渡すと中身が並ぶが、mode が
    // 120000 でないことだけ分かればよい。
    Ok(output
        .split(|byte| *byte == 0)
        .next()
        .filter(|record| !record.is_empty())
        .and_then(|record| record.split(|byte| *byte == b' ').next())
        .map(|mode| String::from_utf8_lossy(mode).into_owned()))
}
