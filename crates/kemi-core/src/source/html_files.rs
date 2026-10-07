//! モックに選べる HTML のファイル（live-compare.md の R-PAGE-MOCK）。配れる範囲の中の `.html` と `.htm`
//! を、範囲の根からの相対パス（`/` 区切り）で集める。git の作業ツリーの中なら git が無視しないもの
//! （追跡中と、無視されない未追跡）だけ、外なら範囲の下のすべて。

use std::path::{Path, PathBuf};

use super::SourceError;
use super::git::{git_raw, path_from_bytes, repo_root, split_z};
use crate::domain::live::{is_html, served_path};

/// 配れる範囲 `root`（実体の場所）の中の、モックに割り当てられる HTML のファイル。パスの辞書順。
///
/// 範囲の中かは実体の場所で決め、`.git` の中、範囲の外を指すシンボリックリンク、消えたものは除く
/// （割り当てるときの確かめと同じ）。git のサブモジュールの中は見ない（`ls-files --others` は中を見ない）。
/// git の外では、シンボリックリンクのディレクトリは辿らない。
pub fn html_files(root: &Path) -> Result<Vec<String>, SourceError> {
    let candidates = if repo_root(root).is_ok() {
        // pathspec の `*.html` は大文字小文字を区別して `.HTML` を落とすので、全部を受けてから絞る。
        let listed = git_raw(
            root,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
        )?;
        split_z(&listed).into_iter().map(path_from_bytes).collect()
    } else {
        let mut found = Vec::new();
        walk(root, Path::new(""), &mut found).map_err(|source| SourceError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        found
    };
    let mut files: Vec<String> = candidates
        .into_iter()
        .filter(|relative| relative.to_str().is_some_and(is_html))
        .filter_map(|relative| servable(root, &relative))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// 範囲の中の実体を持つ HTML のファイルなら、その相対パス（`/` 区切り。並べた場所のまま）。
fn servable(root: &Path, relative: &Path) -> Option<String> {
    let real = std::fs::canonicalize(root.join(relative)).ok()?;
    let real_path = served_path(root, &real).ok()?;
    if !is_html(&real_path) || !real.is_file() {
        return None;
    }
    let shown = relative
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?
        .join("/");
    Some(shown)
}

/// git の外で、範囲の下のファイルを集める（`relative` は今いるディレクトリの範囲の根からの相対）。`.git` と
/// シンボリックリンクのディレクトリには入らない。読めないディレクトリは飛ばす（根が読めなければ失敗）。
fn walk(root: &Path, relative: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root.join(relative))? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let path = relative.join(&name);
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let _ = walk(root, &path, found);
        } else {
            found.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::source::testutil::TempRepo;

    /// git の外の一時ディレクトリ。Drop で消す。
    struct Plain(PathBuf);

    impl Plain {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("kemi-html-files-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Plain(std::fs::canonicalize(path).expect("canonicalize temp dir"))
        }

        fn write(&self, relative: &str) {
            write_under(&self.0, relative);
        }
    }

    impl Drop for Plain {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_under(root: &Path, relative: &str) {
        let full = root.join(relative);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("create parent");
        std::fs::write(full, "<p>file</p>").expect("write file");
    }

    fn root_of(repo: &TempRepo) -> PathBuf {
        std::fs::canonicalize(&repo.path).expect("canonicalize repo")
    }

    #[test]
    fn in_a_git_work_tree_tracked_and_unignored_untracked_html_files_are_listed_in_path_order() {
        let repo = TempRepo::new();
        repo.write("tracked.html", "<p>tracked</p>");
        repo.write("pages/Upper.HTML", "<p>upper</p>");
        repo.write("notes.txt", "not html");
        repo.write(".gitignore", "ignored.html\n");
        repo.add_and_commit("base");
        repo.write("untracked.htm", "<p>untracked</p>");
        repo.write("ignored.html", "<p>ignored</p>");
        write_under(&repo.path.join(".git"), "inside.html");

        let files = html_files(&root_of(&repo)).expect("list html files");

        assert_eq!(files, ["pages/Upper.HTML", "tracked.html", "untracked.htm"]);
    }

    #[test]
    fn a_tracked_html_file_deleted_from_the_work_tree_is_not_listed() {
        let repo = TempRepo::new();
        repo.write("kept.html", "<p>kept</p>");
        repo.write("gone.html", "<p>gone</p>");
        repo.add_and_commit("base");
        repo.remove("gone.html");

        assert_eq!(
            html_files(&root_of(&repo)).expect("list html files"),
            ["kept.html"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_an_html_file_outside_the_range_is_not_listed() {
        let outside = Plain::new("outside-target");
        outside.write("secret.html");
        let repo = TempRepo::new();
        repo.write("page.html", "<p>page</p>");
        std::os::unix::fs::symlink(outside.0.join("secret.html"), repo.path.join("link.html"))
            .expect("symlink");

        assert_eq!(
            html_files(&root_of(&repo)).expect("list html files"),
            ["page.html"]
        );
    }

    #[test]
    fn outside_a_git_work_tree_every_html_file_under_the_root_is_listed_but_not_inside_dot_git() {
        let root = Plain::new("plain");
        root.write("index.html");
        root.write("sub/Other.HTM");
        root.write("notes.txt");
        root.write(".git/inside.html");

        assert_eq!(
            html_files(&root.0).expect("list html files"),
            ["index.html", "sub/Other.HTM"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn outside_a_git_work_tree_a_symlinked_directory_is_not_followed() {
        let root = Plain::new("plain-link");
        root.write("sub/page.html");
        std::os::unix::fs::symlink(root.0.join("sub"), root.0.join("linked")).expect("symlink");

        assert_eq!(
            html_files(&root.0).expect("list html files"),
            ["sub/page.html"]
        );
    }
}
