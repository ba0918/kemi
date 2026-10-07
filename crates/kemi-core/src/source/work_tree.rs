//! worktree の監視の範囲（R-LIVE）。作業ツリーのうち見張るディレクトリと、あるパスを
//! git が無視するかを git に聞く。無視したディレクトリの下には潜らない。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::SourceError;
use super::git::{git_raw, path_bytes, path_from_bytes, split_z};

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
    /// 辿る途中で見つけた、git が無視するか判定を断ったディレクトリ（その下は辿っていない）。
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
        self.walk(vec![self.root.clone()], limit, &self.tracked()?)
    }

    /// このリポジトリの `info/exclude`（無視の規則のうち `.git` の中にあるもの）。linked
    /// worktree では `.git` がファイルで、`info/exclude` は共通の git ディレクトリにある。
    pub fn exclude_file(&self) -> Result<PathBuf, SourceError> {
        let output = git_raw(&self.root, &["rev-parse", "--git-path", "info/exclude"])?;
        let path = path_from_bytes(output.trim_ascii_end());
        Ok(self.root.join(path))
    }

    /// `start` とその下の、見張るディレクトリ。`start` が作業ツリーの外・`.git` の中・
    /// git が無視するものなら空。監視中に作られたディレクトリを足すのに使う。
    pub fn directories_under(
        &self,
        start: &Path,
        limit: usize,
    ) -> Result<WatchDirectories, SourceError> {
        // symlink は辿らない（辿った先は作業ツリーの外かもしれず、git もその下を判定しない）。
        let is_directory = std::fs::symlink_metadata(start).is_ok_and(|meta| meta.is_dir());
        if !self.inside(start) || !is_directory {
            return Ok(WatchDirectories::default());
        }
        let tracked = self.tracked()?;
        if !self
            .ignored_except(&[start.to_path_buf()], &tracked)?
            .is_empty()
        {
            return Ok(WatchDirectories {
                ignored: vec![start.to_path_buf()],
                ..WatchDirectories::default()
            });
        }
        self.walk(vec![start.to_path_buf()], limit, &tracked)
    }

    /// `paths`（作業ツリーの中の絶対パス）のうち、git が無視するもの。追跡している
    /// ファイルは無視されない。作業ツリーの外と `.git` の中のパスは聞かずに外す。
    pub fn ignored(&self, paths: &[PathBuf]) -> Result<HashSet<PathBuf>, SourceError> {
        self.ignored_except(paths, &self.tracked()?)
    }

    /// `paths` のうち git が無視するもので、`tracked`（追跡しているファイルとそれを持つ
    /// ディレクトリ）に入らないもの。
    ///
    /// 索引を読む `check-ignore` は使わない。1 パスごとに索引を舐めるので、追跡ファイル 1 万個・
    /// ディレクトリ 5 万個で 5 秒かかった（読まなければ 0.3 秒）。また submodule の中のパスを
    /// 「submodule の中」と断る。索引を読まないと追跡しているものも無視と答えるので、それは
    /// `ls-files` から求めて外す（索引を読むときの git の答えと同じになる）。
    fn ignored_except(
        &self,
        paths: &[PathBuf],
        tracked: &HashSet<Vec<u8>>,
    ) -> Result<HashSet<PathBuf>, SourceError> {
        Ok(self
            .ask(paths)?
            .into_iter()
            .filter(|path| {
                path.strip_prefix(&self.root)
                    .is_ok_and(|relative| !tracked.contains(&git_path(relative)))
            })
            .collect())
    }

    /// `paths` を `git check-ignore` に聞き、無視されるものを返す。git が判定を断ったパス
    /// （symlink の先など）も見張らないので、無視されるものに含める。
    fn ask(&self, paths: &[PathBuf]) -> Result<HashSet<PathBuf>, SourceError> {
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
        let relatives: Vec<&[u8]> = asked
            .iter()
            .map(|(_, relative)| relative.as_slice())
            .collect();
        let matched = match check_ignore(&self.root, &relatives)? {
            Answer::Matched(matched) => matched,
            Answer::Refused(reason) => {
                // 何も渡さなくても断るなら、パスではなく git かリポジトリの問題。
                if let Answer::Refused(_) = check_ignore(&self.root, &[])? {
                    return Err(SourceError::Git(format!(
                        "git check-ignore failed: {reason}"
                    )));
                }
                self.ask_each_half(&relatives)?
            }
        };
        Ok(asked
            .into_iter()
            .filter(|(_, relative)| matched.contains(relative.as_slice()))
            .map(|(path, _)| path.clone())
            .collect())
    }

    /// git が 1 つのパスを断ると、まとめて渡した全部が答えをもらえない。半分ずつ聞き直し、
    /// 断られたパスだけを無視されるもの（見張らない）にする。
    fn ask_each_half(&self, relatives: &[&[u8]]) -> Result<HashSet<Vec<u8>>, SourceError> {
        match check_ignore(&self.root, relatives)? {
            Answer::Matched(matched) => Ok(matched),
            Answer::Refused(_) if relatives.len() == 1 => {
                Ok(HashSet::from([relatives[0].to_vec()]))
            }
            Answer::Refused(_) => {
                let (left, right) = relatives.split_at(relatives.len() / 2);
                let mut matched = self.ask_each_half(left)?;
                matched.extend(self.ask_each_half(right)?);
                Ok(matched)
            }
        }
    }

    /// 追跡しているファイルと、それを持つディレクトリ（根からの相対、`/` 区切り）。
    /// submodule の中で追跡しているものも含める。
    fn tracked(&self) -> Result<HashSet<Vec<u8>>, SourceError> {
        let listed = git_raw(&self.root, &["ls-files", "-z", "--recurse-submodules"])?;
        let mut tracked = HashSet::new();
        for file in split_z(&listed) {
            for (position, byte) in file.iter().enumerate() {
                if *byte == b'/' {
                    tracked.insert(file[..position].to_vec());
                }
            }
            tracked.insert(file.to_vec());
        }
        Ok(tracked)
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
    fn walk(
        &self,
        starts: Vec<PathBuf>,
        limit: usize,
        tracked: &HashSet<Vec<u8>>,
    ) -> Result<WatchDirectories, SourceError> {
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
            let ignored = self.ignored_except(&children, tracked)?;
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

/// `git check-ignore` の答え。
enum Answer {
    /// 無視されるパス（作業ツリーの根からの相対）。
    Matched(HashSet<Vec<u8>>),
    /// git が判定を断った（終了コード 128）。理由は git の stderr。
    Refused(String),
}

/// `git check-ignore -z --stdin` に NUL 区切りのパスを渡し、無視されるパスを返す。
/// 1 つも無視されないとき git は終了コード 1 を返すので、それは空の結果として扱う。
///
/// パスは `./` を付けて渡す。付けないと `:(glob)x` のような名前を pathspec の指定として読む。
fn check_ignore(root: &Path, relatives: &[&[u8]]) -> Result<Answer, SourceError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let io_error = |source| SourceError::Io {
        path: root.to_path_buf(),
        source,
    };
    let mut input = Vec::new();
    for relative in relatives {
        input.extend_from_slice(b"./");
        input.extend_from_slice(relative);
        input.push(0);
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "-z", "--stdin", "--no-index"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    // 出力が詰まって書き込みが止まらないよう、入力は別スレッドで書く。
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().map_err(io_error)?;
    let _ = writer.join();
    match output.status.code() {
        Some(0) => Ok(Answer::Matched(
            split_z(&output.stdout)
                .into_iter()
                .map(|path| path.strip_prefix(b"./").unwrap_or(path).to_vec())
                .collect(),
        )),
        Some(1) => Ok(Answer::Matched(HashSet::new())),
        Some(128) => Ok(Answer::Refused(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )),
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
    fn an_ignored_directory_that_holds_a_tracked_file_is_watched_but_its_other_children_are_not() {
        let repo = TempRepo::new();
        repo.write(".gitignore", "vendor/\n");
        repo.write("vendor/kept/a.txt", "a\n");
        repo.git(&["add", "-f", ".gitignore", "vendor/kept/a.txt"]);
        repo.git(&["commit", "-q", "-m", "base"]);
        repo.write("vendor/other/b.txt", "b\n");
        let tree = WorkTree::new(repo.path.clone());

        let found = tree.directories(WATCH_DIRECTORY_LIMIT).unwrap();

        assert_eq!(
            sorted(found.directories),
            vec![
                repo.path.clone(),
                repo.path.join("vendor"),
                repo.path.join("vendor/kept"),
            ]
        );
        assert_eq!(found.ignored, vec![repo.path.join("vendor/other")]);
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

    // Windows はパスに `:` を使えない。
    #[cfg(unix)]
    #[test]
    fn a_directory_named_like_pathspec_magic_is_judged_by_its_plain_name() {
        let repo = repo_with_ignored_build();
        repo.write(".gitignore", "build/\n*.log\n:(glob)ignored\n");
        std::fs::create_dir_all(repo.path.join(":(glob)x/inner")).unwrap();
        std::fs::create_dir_all(repo.path.join(":(glob)ignored")).unwrap();
        let tree = WorkTree::new(repo.path.clone());

        let found = tree.directories(WATCH_DIRECTORY_LIMIT).unwrap();

        assert!(
            found
                .directories
                .contains(&repo.path.join(":(glob)x/inner"))
        );
        assert!(found.ignored.contains(&repo.path.join(":(glob)ignored")));
    }

    #[cfg(unix)]
    #[test]
    fn directories_under_a_symlink_to_a_directory_are_empty() {
        let repo = repo_with_ignored_build();
        let outside = TempRepo::new();
        std::fs::create_dir_all(outside.path.join("deep")).unwrap();
        std::os::unix::fs::symlink(&outside.path, repo.path.join("link")).unwrap();
        let tree = WorkTree::new(repo.path.clone());

        let found = tree
            .directories_under(&repo.path.join("link"), WATCH_DIRECTORY_LIMIT)
            .unwrap();

        assert_eq!(found, WatchDirectories::default());
    }

    #[cfg(unix)]
    #[test]
    fn a_path_git_refuses_is_not_watched_and_the_others_are_still_judged() {
        let repo = repo_with_ignored_build();
        let outside = TempRepo::new();
        std::os::unix::fs::symlink(&outside.path, repo.path.join("link")).unwrap();
        let tree = WorkTree::new(repo.path.clone());

        let ignored = tree
            .ignored(&[
                repo.path.join("link/x"),
                repo.path.join("debug.log"),
                repo.path.join("src/a.rs"),
            ])
            .unwrap();

        assert_eq!(
            ignored,
            HashSet::from([repo.path.join("link/x"), repo.path.join("debug.log")])
        );
    }

    /// `sub` に submodule を持つリポジトリ。submodule は `gen/` を無視し、`lib/a` を追跡する。
    fn repo_with_submodule() -> (TempRepo, TempRepo) {
        let module = TempRepo::new();
        module.write(".gitignore", "gen/\n");
        module.write("lib/a", "a\n");
        module.add_and_commit("module");
        let repo = repo_with_ignored_build();
        let source = module.path.to_string_lossy().into_owned();
        repo.git(&[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &source,
            "sub",
        ]);
        (repo, module)
    }

    #[test]
    fn ignored_inside_a_submodule_follows_the_submodule_rules() {
        let (repo, _module) = repo_with_submodule();
        repo.write("sub/gen/f", "f\n");
        repo.write("sub/new.txt", "n\n");
        let tree = WorkTree::new(repo.path.clone());

        let ignored = tree
            .ignored(&[
                repo.path.join("sub/gen/f"),
                repo.path.join("sub/new.txt"),
                repo.path.join("sub/lib/a"),
            ])
            .unwrap();

        assert_eq!(ignored, HashSet::from([repo.path.join("sub/gen/f")]));
    }

    #[test]
    fn directories_under_a_new_directory_in_a_submodule_list_it() {
        let (repo, _module) = repo_with_submodule();
        std::fs::create_dir_all(repo.path.join("sub/fresh")).unwrap();
        let tree = WorkTree::new(repo.path.clone());

        let found = tree
            .directories_under(&repo.path.join("sub/fresh"), WATCH_DIRECTORY_LIMIT)
            .unwrap();

        assert_eq!(found.directories, vec![repo.path.join("sub/fresh")]);
    }

    #[test]
    fn the_exclude_file_of_a_linked_worktree_is_in_the_common_git_directory() {
        let repo = repo_with_ignored_build();
        let linked = repo.path.with_extension("linked");
        let linked_arg = linked.to_string_lossy().into_owned();
        repo.git(&["worktree", "add", "-q", &linked_arg]);
        let tree = WorkTree::new(linked.clone());

        let exclude = tree.exclude_file();

        std::fs::remove_dir_all(&linked).unwrap();
        assert_eq!(
            canonical(exclude.unwrap()),
            canonical(repo.path.join(".git/info/exclude"))
        );
    }

    fn canonical(path: PathBuf) -> PathBuf {
        let parent = std::fs::canonicalize(path.parent().unwrap()).unwrap();
        parent.join(path.file_name().unwrap())
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
