//! worktree の監視の範囲（R-LIVE）。作業ツリーのうち見張るディレクトリと、あるパスを
//! git が無視するかを git に聞く。無視したディレクトリの下には潜らない。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::SourceError;
use super::git::path_bytes;

/// 1 レビューで見張るディレクトリの数の上限（R-LIVE）。
pub const WATCH_DIRECTORY_LIMIT: usize = 10_000;

/// 見張る作業ツリー。根は git の作業ツリーの根。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkTree {
    root: PathBuf,
}

/// 見張るディレクトリの一覧。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WatchDirectories {
    /// 見張るディレクトリ。`.git` と git が無視するディレクトリとその下を含まない。
    pub directories: Vec<PathBuf>,
    /// 辿る途中で見つけた、git が無視するディレクトリ（その下は辿っていない）。
    pub ignored: Vec<PathBuf>,
    /// 上限を超えた。超えた時点で辿るのをやめるので、`directories` は全部ではない。
    pub over_limit: bool,
}

impl WorkTree {
    pub fn new(root: PathBuf) -> Self {
        WorkTree { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 作業ツリーの根から辿った、見張るディレクトリ（根を含む）。
    pub fn directories(&self, limit: usize) -> Result<WatchDirectories, SourceError> {
        self.walk(vec![self.root.clone()], limit)
    }

    /// `start` とその下の、見張るディレクトリ。`start` が作業ツリーの外・`.git` の中・
    /// git が無視するものなら空。監視中に作られたディレクトリを足すのに使う。
    pub fn directories_under(
        &self,
        start: &Path,
        limit: usize,
    ) -> Result<WatchDirectories, SourceError> {
        if !self.inside(start) || !start.is_dir() {
            return Ok(WatchDirectories::default());
        }
        if !self.ignored(&[start.to_path_buf()])?.is_empty() {
            return Ok(WatchDirectories {
                ignored: vec![start.to_path_buf()],
                ..WatchDirectories::default()
            });
        }
        self.walk(vec![start.to_path_buf()], limit)
    }

    /// `paths`（作業ツリーの中の絶対パス）のうち、git が無視するもの。追跡している
    /// ファイルは無視されない。作業ツリーの外と `.git` の中のパスは聞かずに外す。
    pub fn ignored(&self, paths: &[PathBuf]) -> Result<HashSet<PathBuf>, SourceError> {
        let asked: Vec<(&PathBuf, Vec<u8>)> = paths
            .iter()
            .filter(|path| self.inside(path))
            .filter_map(|path| {
                let relative = path.strip_prefix(&self.root).ok()?;
                Some((path, git_path(relative)))
            })
            // 根そのものは無視されない（空のパスは git が受け付けない）。
            .filter(|(_, relative)| !relative.is_empty())
            .collect();
        if asked.is_empty() {
            return Ok(HashSet::new());
        }
        let mut input = Vec::new();
        for (_, relative) in &asked {
            input.extend_from_slice(relative);
            input.push(0);
        }
        let output = check_ignore(&self.root, &input)?;
        let matched: HashSet<&[u8]> = output
            .split(|byte| *byte == 0)
            .filter(|token| !token.is_empty())
            .collect();
        Ok(asked
            .into_iter()
            .filter(|(_, relative)| matched.contains(relative.as_slice()))
            .map(|(path, _)| path.clone())
            .collect())
    }

    /// 作業ツリーの中で、`.git` の中でないパスか。
    fn inside(&self, path: &Path) -> bool {
        path.strip_prefix(&self.root).is_ok_and(|relative| {
            !relative
                .components()
                .any(|component| component.as_os_str() == ".git")
        })
    }

    /// `starts` から 1 段ずつ辿る。段ごとに子のディレクトリをまとめて git に聞き、無視される
    /// ものの下には潜らない。git を呼ぶ回数は深さの分だけで、ディレクトリの数に比例しない。
    fn walk(&self, starts: Vec<PathBuf>, limit: usize) -> Result<WatchDirectories, SourceError> {
        let mut found = WatchDirectories::default();
        let mut level = starts;
        found.directories.extend(level.iter().cloned());
        while !level.is_empty() {
            if found.directories.len() > limit {
                found.over_limit = true;
                return Ok(found);
            }
            let children: Vec<PathBuf> = level
                .iter()
                .flat_map(|parent| subdirectories(parent))
                .collect();
            let ignored = self.ignored(&children)?;
            level = Vec::new();
            for child in children {
                if ignored.contains(&child) {
                    found.ignored.push(child);
                } else {
                    found.directories.push(child.clone());
                    level.push(child);
                }
            }
        }
        found.over_limit = found.directories.len() > limit;
        Ok(found)
    }
}

/// git に渡す作業ツリーの中の相対パス。Windows でも区切りは `/` にし、git が返す表記と揃える。
fn git_path(relative: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for component in relative.components() {
        if !bytes.is_empty() {
            bytes.push(b'/');
        }
        bytes.extend(path_bytes(Path::new(component.as_os_str())));
    }
    bytes
}

/// `parent` の直下のディレクトリ（`.git` を除く）。symlink は辿らない。読めないものは飛ばす。
fn subdirectories(parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.file_name() != ".git")
        .map(|entry| entry.path())
        .collect()
}

/// `git check-ignore -z --stdin` に NUL 区切りのパスを渡し、無視されるパスを NUL 区切りで返す。
/// 1 つも無視されないとき git は終了コード 1 を返すので、それは空の結果として扱う。
fn check_ignore(root: &Path, input: &[u8]) -> Result<Vec<u8>, SourceError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let io_error = |source| SourceError::Io {
        path: root.to_path_buf(),
        source,
    };
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "-z", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    // 出力が詰まって書き込みが止まらないよう、入力は別スレッドで書く。
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().map_err(io_error)?;
    let _ = writer.join();
    match output.status.code() {
        Some(0) => Ok(output.stdout),
        Some(1) => Ok(Vec::new()),
        _ => Err(SourceError::Git(format!(
            "git check-ignore failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::TempRepo;

    fn repo_with_ignored_build() -> TempRepo {
        let repo = TempRepo::new();
        repo.write(".gitignore", "build/\n*.log\n");
        repo.write("src/a.rs", "fn a() {}\n");
        repo.add_and_commit("base");
        repo.write("build/out/x.o", "x\n");
        std::fs::create_dir_all(repo.path.join("notes/draft")).unwrap();
        repo
    }

    fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
        paths.sort();
        paths
    }

    #[test]
    fn directories_skip_dot_git_and_ignored_directories_and_what_is_under_them() {
        let repo = repo_with_ignored_build();
        let tree = WorkTree::new(repo.path.clone());

        let found = tree.directories(WATCH_DIRECTORY_LIMIT).unwrap();

        assert!(
            found.directories.iter().all(|directory| {
                !directory.starts_with(repo.path.join(".git"))
                    && !directory.starts_with(repo.path.join("build"))
            }),
            "{:?}",
            found.directories
        );
        assert_eq!(found.ignored, vec![repo.path.join("build")]);
        assert!(!found.over_limit);
    }

    #[test]
    fn directories_list_the_root_tracked_and_untracked_directories() {
        let repo = repo_with_ignored_build();
        let tree = WorkTree::new(repo.path.clone());

        let found = tree.directories(WATCH_DIRECTORY_LIMIT).unwrap();

        assert_eq!(
            sorted(found.directories),
            sorted(vec![
                repo.path.clone(),
                repo.path.join("src"),
                repo.path.join("notes"),
                repo.path.join("notes/draft"),
            ])
        );
    }

    #[test]
    fn directories_tell_when_they_exceed_the_limit() {
        let repo = repo_with_ignored_build();
        let tree = WorkTree::new(repo.path.clone());

        assert!(tree.directories(3).unwrap().over_limit);
        assert!(!tree.directories(4).unwrap().over_limit);
    }

    #[test]
    fn directories_under_a_new_directory_list_it_and_its_children() {
        let repo = repo_with_ignored_build();
        std::fs::create_dir_all(repo.path.join("new/inner")).unwrap();
        let tree = WorkTree::new(repo.path.clone());

        let found = tree
            .directories_under(&repo.path.join("new"), WATCH_DIRECTORY_LIMIT)
            .unwrap();

        assert_eq!(
            sorted(found.directories),
            vec![repo.path.join("new"), repo.path.join("new/inner")]
        );
    }

    #[test]
    fn directories_under_an_ignored_or_dot_git_directory_are_empty() {
        let repo = repo_with_ignored_build();
        let tree = WorkTree::new(repo.path.clone());

        let ignored = tree
            .directories_under(&repo.path.join("build"), WATCH_DIRECTORY_LIMIT)
            .unwrap();
        let git = tree
            .directories_under(&repo.path.join(".git/refs"), WATCH_DIRECTORY_LIMIT)
            .unwrap();

        assert!(ignored.directories.is_empty());
        assert!(git.directories.is_empty());
    }

    #[test]
    fn ignored_reports_the_paths_git_ignores_but_not_tracked_files() {
        let repo = repo_with_ignored_build();
        repo.write("src/debug.log", "x\n");
        repo.write("kept.log", "tracked\n");
        repo.git(&["add", "-f", "kept.log"]);
        let tree = WorkTree::new(repo.path.clone());

        let ignored = tree
            .ignored(&[
                repo.path.join("src/debug.log"),
                repo.path.join("build/out/x.o"),
                repo.path.join("src/a.rs"),
                repo.path.join("kept.log"),
                repo.path.join("src/new.rs"),
            ])
            .unwrap();

        assert_eq!(
            ignored,
            HashSet::from([
                repo.path.join("src/debug.log"),
                repo.path.join("build/out/x.o"),
            ])
        );
    }
}
