//! git を読む入力ソース（R-INPUT-2〜4）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::domain::content;
use crate::domain::noise::{classify, linguist_generated, NoiseInput};
use crate::domain::review::{FileEntry, Group, ReviewMeta, Status};
use crate::source::{Plan, PlanStore, PlannedFile, ReviewSource, SideRef, SourceError};

/// 内容を読まずに統計だけを出す untracked の上限（D6）。
pub const UNTRACKED_LIMIT: u64 = 1_048_576;

#[derive(Clone, Debug)]
pub enum GitMode {
    Worktree,
    Staged,
    Range {
        from: String,
        to: String,
        group_by: GroupBy,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupBy {
    Commit,
    File,
}

#[derive(Clone, Debug)]
struct DiffEntry {
    path: String,
    old_path: Option<String>,
    status: Status,
    add: u64,
    del: u64,
    binary: bool,
}

pub struct GitSource {
    repo: PathBuf,
    mode: GitMode,
    store: PlanStore,
    ids: Mutex<FileIds>,
}

/// 再取得しても同じファイルには同じ id を返す（`?refresh=1` でコメントの
/// file_id が別ファイルを指さないようにする）。
#[derive(Default)]
struct FileIds {
    map: HashMap<String, String>,
    next: usize,
}

impl FileIds {
    fn get(&mut self, group_id: &str, path: &str) -> String {
        let key = format!("{group_id}\n{path}");
        if let Some(id) = self.map.get(&key) {
            return id.clone();
        }
        self.next += 1;
        let id = format!("f{}", self.next);
        self.map.insert(key, id.clone());
        id
    }
}

impl GitSource {
    pub fn new(repo: PathBuf, mode: GitMode) -> Self {
        GitSource {
            repo,
            mode,
            store: PlanStore::new(),
            ids: Mutex::new(FileIds::default()),
        }
    }

    pub fn open(mode: GitMode) -> Result<Self, SourceError> {
        let repo = repo_root(Path::new("."))?;
        Ok(GitSource::new(repo, mode))
    }

    fn review_worktree(&self) -> Result<(ReviewMeta, Plan), SourceError> {
        let attributes = read_attributes(&self.repo);
        let mut entries = worktree_diff_entries(&self.repo)?;

        let untracked = git_raw(
            &self.repo,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )?;
        for path in split_z(&untracked) {
            let full = self.repo.join(path);
            let size = std::fs::metadata(&full).map_or(0, |meta| meta.len());
            let (add, binary) = if size > UNTRACKED_LIMIT {
                (0, true)
            } else {
                let bytes = std::fs::read(&full).map_err(|source| SourceError::Io {
                    path: full.clone(),
                    source,
                })?;
                if content::is_binary(&bytes) {
                    (0, true)
                } else {
                    (
                        content::lines(&String::from_utf8_lossy(&bytes)).len() as u64,
                        false,
                    )
                }
            };
            entries.push(DiffEntry {
                path: path.to_string(),
                old_path: None,
                status: Status::Add,
                add,
                del: 0,
                binary,
            });
        }

        let files = self.build_entries(&entries, &attributes, "worktree", |entry| {
            let old = match entry.status {
                Status::Add => SideRef::Absent,
                _ => SideRef::Git {
                    repo: self.repo.clone(),
                    spec: format!("HEAD:{}", entry.old_path.as_deref().unwrap_or(&entry.path)),
                },
            };
            let new = match entry.status {
                Status::Delete => SideRef::Absent,
                _ => SideRef::Disk(self.repo.join(&entry.path)),
            };
            (old, new)
        });

        let group_id = "worktree".to_string();
        Ok((
            ReviewMeta {
                title: "作業ツリーの変更".to_string(),
                subtitle: String::new(),
                meta: serde_json::Value::Null,
                groups: vec![Group {
                    id: group_id.clone(),
                    title: "作業ツリーの変更".to_string(),
                    why: String::new(),
                    watch: String::new(),
                    files: files.0,
                }],
                approval: Vec::new(),
            },
            Plan { files: files.1 },
        ))
    }

    fn review_staged(&self) -> Result<(ReviewMeta, Plan), SourceError> {
        let attributes = read_attributes(&self.repo);
        let entries = diff_entries(&self.repo, &["--cached"])?;
        let files = self.build_entries(&entries, &attributes, "staged", |entry| {
            let old = match entry.status {
                Status::Add => SideRef::Absent,
                _ => SideRef::Git {
                    repo: self.repo.clone(),
                    spec: format!("HEAD:{}", entry.old_path.as_deref().unwrap_or(&entry.path)),
                },
            };
            let new = match entry.status {
                Status::Delete => SideRef::Absent,
                _ => SideRef::Git {
                    repo: self.repo.clone(),
                    spec: format!(":{}", entry.path),
                },
            };
            (old, new)
        });

        let group_id = "staged".to_string();
        Ok((
            ReviewMeta {
                title: "ステージ済みの変更".to_string(),
                subtitle: String::new(),
                meta: serde_json::Value::Null,
                groups: vec![Group {
                    id: group_id.clone(),
                    title: "ステージ済みの変更".to_string(),
                    why: String::new(),
                    watch: String::new(),
                    files: files.0,
                }],
                approval: Vec::new(),
            },
            Plan { files: files.1 },
        ))
    }

    fn review_range(
        &self,
        from: &str,
        to: &str,
        group_by: GroupBy,
    ) -> Result<(ReviewMeta, Plan), SourceError> {
        verify_ref(&self.repo, from)?;
        verify_ref(&self.repo, to)?;
        let attributes = read_attributes(&self.repo);

        match group_by {
            GroupBy::Commit => {
                let range = format!("{from}..{to}");
                let shas = git_text(
                    &self.repo,
                    &["rev-list", "--no-merges", "--reverse", &range],
                )?;
                let mut groups = Vec::new();
                let mut plan_files = Vec::new();
                for sha in shas.lines().filter(|line| !line.trim().is_empty()) {
                    let revision = format!("{sha}^!");
                    let entries = diff_entries(&self.repo, &[&revision])?;
                    let (title, why) = commit_message(&self.repo, sha)?;
                    let files = self.build_entries(&entries, &attributes, sha, |entry| {
                        let old = match entry.status {
                            Status::Add => SideRef::Absent,
                            _ => SideRef::Git {
                                repo: self.repo.clone(),
                                spec: format!(
                                    "{sha}^:{}",
                                    entry.old_path.as_deref().unwrap_or(&entry.path)
                                ),
                            },
                        };
                        let new = match entry.status {
                            Status::Delete => SideRef::Absent,
                            _ => SideRef::Git {
                                repo: self.repo.clone(),
                                spec: format!("{sha}:{}", entry.path),
                            },
                        };
                        (old, new)
                    });
                    plan_files.extend(files.1);
                    groups.push(Group {
                        id: sha.to_string(),
                        title,
                        why,
                        watch: String::new(),
                        files: files.0,
                    });
                }
                Ok((
                    ReviewMeta {
                        title: format!("{from}..{to}"),
                        subtitle: String::new(),
                        meta: serde_json::Value::Null,
                        groups,
                        approval: Vec::new(),
                    },
                    Plan { files: plan_files },
                ))
            }
            GroupBy::File => {
                let merge_base = git_text(&self.repo, &["merge-base", from, to])?
                    .trim()
                    .to_string();
                let range = format!("{from}...{to}");
                let entries = diff_entries(&self.repo, &[&range])?;
                let files_h = self.build_entries(&entries, &attributes, "all", |entry| {
                    let old = match entry.status {
                        Status::Add => SideRef::Absent,
                        _ => SideRef::Git {
                            repo: self.repo.clone(),
                            spec: format!(
                                "{merge_base}:{}",
                                entry.old_path.as_deref().unwrap_or(&entry.path)
                            ),
                        },
                    };
                    let new = match entry.status {
                        Status::Delete => SideRef::Absent,
                        _ => SideRef::Git {
                            repo: self.repo.clone(),
                            spec: format!("{to}:{}", entry.path),
                        },
                    };
                    (old, new)
                });
                Ok((
                    ReviewMeta {
                        title: format!("{from}..{to}"),
                        subtitle: String::new(),
                        meta: serde_json::Value::Null,
                        groups: vec![Group {
                            id: "all".to_string(),
                            title: format!("{from}...{to}"),
                            why: String::new(),
                            watch: String::new(),
                            files: files_h.0,
                        }],
                        approval: Vec::new(),
                    },
                    Plan { files: files_h.1 },
                ))
            }
        }
    }

    fn build_entries(
        &self,
        entries: &[DiffEntry],
        attributes: &str,
        group_id: &str,
        sides: impl Fn(&DiffEntry) -> (SideRef, SideRef),
    ) -> (Vec<FileEntry>, Vec<PlannedFile>) {
        let mut files = Vec::new();
        let mut planned = Vec::new();
        for entry in entries {
            let id = self
                .ids
                .lock()
                .expect("file id lock poisoned")
                .get(group_id, &entry.path);
            let (old, new) = sides(entry);
            let (old_size, new_size) = if entry.binary {
                (side_size(&old), side_size(&new))
            } else {
                (0, 0)
            };
            let noise = classify(&NoiseInput {
                path: &entry.path,
                linguist_generated: linguist_generated(attributes, &entry.path),
                binary: entry.binary,
            })
            .is_some();
            files.push(FileEntry {
                id: id.clone(),
                group_id: group_id.to_string(),
                path: entry.path.clone(),
                old_path: entry.old_path.clone(),
                status: entry.status,
                add: entry.add as u32,
                del: entry.del as u32,
                binary: entry.binary,
                old_size,
                new_size,
                focus: false,
                note: String::new(),
                noise,
            });
            planned.push(PlannedFile { id, old, new });
        }
        (files, planned)
    }
}

impl ReviewSource for GitSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        let (review, plan) = match &self.mode {
            GitMode::Worktree => self.review_worktree()?,
            GitMode::Staged => self.review_staged()?,
            GitMode::Range { from, to, group_by } => self.review_range(from, to, *group_by)?,
        };
        self.store.update(plan);
        Ok(review)
    }

    fn content(&self, file_id: &str) -> Result<crate::source::FileContent, SourceError> {
        self.store.content(file_id)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        match &self.mode {
            GitMode::Worktree => self.store.disk_paths(),
            GitMode::Staged => Vec::new(),
            GitMode::Range { to, .. } => ref_watch_paths(&self.repo, to).unwrap_or_default(),
        }
    }
}

/// worktree の統計を求める。変更が多いときはパスで分けて並列に diff する。
/// 状態は安いコマンドとファイルの有無から決める。
fn worktree_diff_entries(repo: &Path) -> Result<Vec<DiffEntry>, SourceError> {
    let numstat = worktree_numstat(repo)?;
    let head_paths: std::collections::HashSet<String> = split_z(&git_raw(
        repo,
        &["ls-tree", "-r", "--name-only", "-z", "HEAD"],
    )?)
    .into_iter()
    .map(str::to_string)
    .collect();

    Ok(numstat
        .into_iter()
        .map(|stat| {
            let status = if stat.old_path.is_some() {
                Status::Rename
            } else if !head_paths.contains(&stat.new_path) {
                Status::Add
            } else if !repo.join(&stat.new_path).exists() {
                Status::Delete
            } else {
                Status::Modify
            };
            DiffEntry {
                path: stat.new_path,
                old_path: stat.old_path,
                status,
                add: stat.add,
                del: stat.del,
                binary: stat.binary,
            }
        })
        .collect())
}

/// worktree の numstat。ファイル数が多いときはパスを分割して並列に引く。
/// パス一覧は内容差分を伴わない plumbing から取る（`diff --name-only HEAD` は
/// 全ファイルの差分判定をやり直すため、1 万ファイルでは約 0.5 秒かかる）。
fn worktree_numstat(repo: &Path) -> Result<Vec<Numstat>, SourceError> {
    let mut paths: Vec<String> = Vec::new();
    for listing in [
        git_raw(repo, &["diff-files", "--name-only", "-z", "--no-renames"])?,
        git_raw(
            repo,
            &["diff", "--name-only", "-z", "--no-renames", "--cached"],
        )?,
        git_raw(repo, &["ls-files", "--others", "--exclude-standard", "-z"])?,
    ] {
        paths.extend(split_z(&listing).into_iter().map(str::to_string));
    }
    paths.sort();
    paths.dedup();
    if paths.len() < 256 {
        return Ok(parse_numstat(&git_raw(
            repo,
            &["diff", "--numstat", "-z", "-M", "HEAD"],
        )?));
    }

    let workers = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .clamp(2, 16);
    let chunk = paths.len().div_ceil(workers);
    let mut handles = Vec::new();
    for shard in paths.chunks(chunk) {
        let repo = repo.to_path_buf();
        let shard: Vec<String> = shard.to_vec();
        handles.push(std::thread::spawn(
            move || -> Result<Vec<Numstat>, SourceError> {
                let mut args: Vec<&str> = vec!["diff", "--numstat", "-z", "-M", "HEAD", "--"];
                args.extend(shard.iter().map(String::as_str));
                Ok(parse_numstat(&git_raw(&repo, &args)?))
            },
        ));
    }

    let mut entries = Vec::new();
    for handle in handles {
        let mut shard = handle
            .join()
            .map_err(|_| SourceError::Git("numstat の並列実行に失敗しました".to_string()))??;
        entries.append(&mut shard);
    }
    entries.sort_by(|left, right| left.new_path.cmp(&right.new_path));
    Ok(entries)
}

fn diff_entries(repo: &Path, range_args: &[&str]) -> Result<Vec<DiffEntry>, SourceError> {
    let mut args = vec!["diff", "--numstat", "-z", "-M"];
    args.extend_from_slice(range_args);
    let numstat = parse_numstat(&git_raw(repo, &args)?);

    let mut args = vec!["diff", "--name-status", "-z", "-M"];
    args.extend_from_slice(range_args);
    let name_status = parse_name_status(&git_raw(repo, &args)?);

    let mut entries = Vec::new();
    for name in name_status {
        let statistics = numstat.iter().find(|stat| {
            let stat_path = match &stat.old_path {
                Some(_) => &stat.new_path,
                None => &stat.path,
            };
            stat_path == &name.path
        });
        entries.push(DiffEntry {
            path: name.path,
            old_path: name.old_path,
            status: name.status,
            add: statistics.map_or(0, |stat| stat.add),
            del: statistics.map_or(0, |stat| stat.del),
            binary: statistics.is_some_and(|stat| stat.binary),
        });
    }
    Ok(entries)
}

struct Numstat {
    path: String,
    old_path: Option<String>,
    new_path: String,
    add: u64,
    del: u64,
    binary: bool,
}

fn parse_numstat(bytes: &[u8]) -> Vec<Numstat> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.is_empty() {
            index += 1;
            continue;
        }
        let fields: Vec<&[u8]> = token.split(|byte| *byte == b'\t').collect();
        if fields.len() >= 3 && !fields[2].is_empty() {
            entries.push(Numstat {
                path: String::from_utf8_lossy(fields[2]).into_owned(),
                old_path: None,
                new_path: String::from_utf8_lossy(fields[2]).into_owned(),
                add: parse_count(fields[0]),
                del: parse_count(fields[1]),
                binary: fields[0] == b"-",
            });
            index += 1;
        } else if index + 2 < tokens.len() {
            let old_path = String::from_utf8_lossy(tokens[index + 1]).into_owned();
            let new_path = String::from_utf8_lossy(tokens[index + 2]).into_owned();
            entries.push(Numstat {
                path: new_path.clone(),
                old_path: Some(old_path),
                new_path,
                add: parse_count(fields.first().copied().unwrap_or_default()),
                del: parse_count(fields.get(1).copied().unwrap_or_default()),
                binary: fields.first().is_some_and(|field| *field == b"-"),
            });
            index += 3;
        } else {
            index += 1;
        }
    }
    entries
}

struct NameStatus {
    path: String,
    old_path: Option<String>,
    status: Status,
}

fn parse_name_status(bytes: &[u8]) -> Vec<NameStatus> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.is_empty() {
            index += 1;
            continue;
        }
        let letter = token[0];
        match letter {
            b'R' | b'C' => {
                if index + 2 < tokens.len() {
                    entries.push(NameStatus {
                        path: String::from_utf8_lossy(tokens[index + 2]).into_owned(),
                        old_path: Some(String::from_utf8_lossy(tokens[index + 1]).into_owned()),
                        status: Status::Rename,
                    });
                    index += 3;
                } else {
                    index += 1;
                }
            }
            b'A' | b'D' | b'M' | b'T' => {
                if index + 1 < tokens.len() {
                    let status = match letter {
                        b'A' => Status::Add,
                        b'D' => Status::Delete,
                        _ => Status::Modify,
                    };
                    entries.push(NameStatus {
                        path: String::from_utf8_lossy(tokens[index + 1]).into_owned(),
                        old_path: None,
                        status,
                    });
                    index += 2;
                } else {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    entries
}

fn parse_count(field: &[u8]) -> u64 {
    if field == b"-" {
        return 0;
    }
    String::from_utf8_lossy(field).parse().unwrap_or(0)
}

fn split_z(bytes: &[u8]) -> Vec<&str> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .map(|token| std::str::from_utf8(token).unwrap_or_default())
        .collect()
}

fn commit_message(repo: &Path, sha: &str) -> Result<(String, String), SourceError> {
    let text = git_text(repo, &["show", "-s", "--format=%s%n%b", sha])?;
    let mut lines = text.lines();
    let subject = lines.next().unwrap_or_default().trim_end().to_string();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    Ok((subject, body))
}

fn side_size(side: &SideRef) -> u64 {
    match side {
        SideRef::Absent => 0,
        SideRef::Inline(bytes) => bytes.len() as u64,
        SideRef::Disk(path) => std::fs::metadata(path).map_or(0, |meta| meta.len()),
        SideRef::Git { repo, spec } => git_text(repo, &["cat-file", "-s", spec])
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0),
    }
}

fn read_attributes(repo: &Path) -> String {
    std::fs::read_to_string(repo.join(".gitattributes")).unwrap_or_default()
}

fn verify_ref(repo: &Path, revision: &str) -> Result<(), SourceError> {
    let spec = format!("{revision}^{{commit}}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", &spec])
        .output()
        .map_err(|source| SourceError::Io {
            path: repo.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(SourceError::Git(format!(
            "ref が見つかりません: {revision}"
        )));
    }
    Ok(())
}

/// `--to` の ref 更新を検知するための監視パス。HEAD と、その参照先の
/// loose ref（または親ディレクトリ）を返す。
pub fn ref_watch_paths(repo: &Path, reference: &str) -> Result<Vec<PathBuf>, SourceError> {
    let full = git_text(repo, &["rev-parse", "--symbolic-full-name", reference])?
        .trim()
        .to_string();
    let git_dir = PathBuf::from(git_text(repo, &["rev-parse", "--absolute-git-dir"])?.trim());
    let mut paths = vec![git_dir.join("HEAD")];
    if !full.is_empty() && full != "HEAD" {
        let ref_path = git_dir.join(&full);
        if let Some(parent) = ref_path.parent() {
            paths.push(parent.to_path_buf());
        }
        if ref_path.exists() {
            paths.push(ref_path);
        }
    }
    Ok(paths)
}

pub fn repo_root(path: &Path) -> Result<PathBuf, SourceError> {
    let text = git_text(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(text.trim()))
}

pub(crate) fn show(repo: &Path, spec: &str) -> Result<Vec<u8>, SourceError> {
    git_raw(repo, &["show", spec])
}

pub(crate) fn git_raw(repo: &Path, args: &[&str]) -> Result<Vec<u8>, SourceError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|source| SourceError::Io {
            path: repo.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(SourceError::Git(format!(
            "git {} が失敗しました: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

pub(crate) fn git_text(repo: &Path, args: &[&str]) -> Result<String, SourceError> {
    let bytes = git_raw(repo, args)?;
    String::from_utf8(bytes).map_err(|_| {
        SourceError::Git(format!(
            "git {} の出力が UTF-8 ではありません",
            args.join(" ")
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::TempRepo;

    fn source(repo: &TempRepo, mode: GitMode) -> GitSource {
        GitSource::new(repo.path.clone(), mode)
    }

    fn find_file<'a>(review: &'a ReviewMeta, path: &str) -> &'a FileEntry {
        review
            .groups
            .iter()
            .flat_map(|group| group.files.iter())
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("file not found: {path}"))
    }

    #[test]
    fn worktree_change_has_uncommitted_content() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\ntwo\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        repo.write("a.txt", "one\nTWO\n");
        let review = source.review().unwrap();
        let entry = find_file(&review, "a.txt");

        assert_eq!(entry.status, Status::Modify);
        assert_eq!(entry.group_id, "worktree");
        assert_eq!((entry.add, entry.del), (1, 1));
        assert_eq!(review.title, "作業ツリーの変更");
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old.unwrap(), b"one\ntwo\n");
        assert_eq!(content.new.unwrap(), b"one\nTWO\n");
    }

    #[test]
    fn worktree_includes_untracked_as_add() {
        let repo = TempRepo::new();
        repo.write("a.txt", "a\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        repo.write("new/b.txt", "x\ny\n");
        let review = source.review().unwrap();
        let entry = find_file(&review, "new/b.txt");

        assert_eq!(entry.status, Status::Add);
        assert_eq!((entry.add, entry.del), (2, 0));
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old, None);
        assert_eq!(content.new.unwrap(), b"x\ny\n");
    }

    #[test]
    fn worktree_file_ids_survive_re_review() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        repo.write("b.txt", "one\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        repo.write("a.txt", "two\n");
        repo.write("b.txt", "two\n");
        let first = source.review().unwrap();
        let first_a = find_file(&first, "a.txt").id.clone();
        let first_b = find_file(&first, "b.txt").id.clone();
        assert_ne!(first_a, first_b);

        // a を戻すと一覧から消えるが、残る b の id は変えない。
        repo.write("a.txt", "one\n");
        let second = source.review().unwrap();
        assert_eq!(find_file(&second, "b.txt").id, first_b);

        // 一度消えた a が再び現れても、最初の id を使う。
        repo.write("a.txt", "three\n");
        let third = source.review().unwrap();
        assert_eq!(find_file(&third, "a.txt").id, first_a);
    }

    #[test]
    fn worktree_deleted_file_shows_delete() {
        let repo = TempRepo::new();
        repo.write("a.txt", "a\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        repo.remove("a.txt");
        let review = source.review().unwrap();
        let entry = find_file(&review, "a.txt");

        assert_eq!(entry.status, Status::Delete);
        assert_eq!((entry.add, entry.del), (0, 1));
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old.unwrap(), b"a\n");
        assert_eq!(content.new, None);
    }

    #[test]
    fn worktree_untracked_over_limit_has_stats_without_content() {
        let repo = TempRepo::new();
        repo.write("a.txt", "a\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        let big = "x".repeat(UNTRACKED_LIMIT as usize + 1);
        repo.write("big.bin", &big);
        let review = source.review().unwrap();
        let entry = find_file(&review, "big.bin");

        assert_eq!(entry.status, Status::Add);
        assert!(entry.binary);
        assert_eq!((entry.add, entry.del), (0, 0));
        assert_eq!(entry.new_size, UNTRACKED_LIMIT + 1);
    }

    #[test]
    fn staged_shows_index_side_not_worktree() {
        let repo = TempRepo::new();
        repo.write("a.txt", "base\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Staged);

        repo.write("a.txt", "index\n");
        repo.git(&["add", "a.txt"]);
        repo.write("a.txt", "worktree\n");

        let review = source.review().unwrap();
        let entry = find_file(&review, "a.txt");
        assert_eq!(entry.group_id, "staged");
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old.unwrap(), b"base\n");
        assert_eq!(content.new.unwrap(), b"index\n");
    }

    #[test]
    fn staged_ignores_worktree_only_change() {
        let repo = TempRepo::new();
        repo.write("a.txt", "base\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Staged);

        repo.write("a.txt", "worktree\n");
        let review = source.review().unwrap();

        assert!(review.groups[0].files.is_empty());
    }

    #[test]
    fn range_commit_groups_exclude_merges_and_keep_subject_body() {
        let repo = TempRepo::new();
        repo.write("a.txt", "a\n");
        let base = repo.add_and_commit("base");

        repo.git(&["checkout", "-q", "-b", "feature"]);
        repo.write("b.txt", "b\n");
        let feature = repo.add_and_commit("feat: b\n\nbody line");
        repo.git(&["checkout", "-q", "-"]);
        repo.write("c.txt", "c\n");
        repo.add_and_commit("feat: c");
        repo.git(&["merge", "--no-ff", "-q", "feature", "-m", "merge feature"]);
        let head = repo.head();

        let source = source(
            &repo,
            GitMode::Range {
                from: base.clone(),
                to: head.clone(),
                group_by: GroupBy::Commit,
            },
        );
        let review = source.review().unwrap();

        let ids: Vec<&str> = review
            .groups
            .iter()
            .map(|group| group.id.as_str())
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&feature.as_str()));
        assert!(!ids.iter().any(|id| id.starts_with(&head[..7])));
        assert_eq!(review.title, format!("{base}..{head}"));

        let group = review
            .groups
            .iter()
            .find(|group| group.id == feature)
            .unwrap();
        assert_eq!(group.title, "feat: b");
        assert_eq!(group.why, "body line");
        assert_eq!(group.files.len(), 1);
        assert_eq!(group.files[0].status, Status::Add);
        let content = source.content(&group.files[0].id).unwrap();
        assert_eq!(content.old, None);
        assert_eq!(content.new.unwrap(), b"b\n");
    }

    #[test]
    fn range_commit_detects_rename_and_reads_old_path() {
        let repo = TempRepo::new();
        let body: String = (1..=20).map(|n| format!("line{n}\n")).collect();
        repo.write("old.txt", &body);
        let base = repo.add_and_commit("base");

        repo.git(&["mv", "old.txt", "new.txt"]);
        repo.write(
            "new.txt",
            &(1..=19).map(|n| format!("line{n}\n")).collect::<String>(),
        );
        let head = repo.add_and_commit("rename");

        let source = source(
            &repo,
            GitMode::Range {
                from: base,
                to: head,
                group_by: GroupBy::Commit,
            },
        );
        let review = source.review().unwrap();
        let entry = find_file(&review, "new.txt");

        assert_eq!(entry.status, Status::Rename);
        assert_eq!(entry.old_path.as_deref(), Some("old.txt"));
        let content = source.content(&entry.id).unwrap();
        assert!(content.old.unwrap().starts_with(b"line1\n"));
        assert!(content.new.unwrap().ends_with(b"line19\n"));
    }

    #[test]
    fn range_file_keeps_one_entry_per_path_across_commits() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        let base = repo.add_and_commit("base");
        repo.write("a.txt", "two\n");
        repo.add_and_commit("second");
        repo.write("a.txt", "three\n");
        let head = repo.add_and_commit("third");

        let source = source(
            &repo,
            GitMode::Range {
                from: base.clone(),
                to: head.clone(),
                group_by: GroupBy::File,
            },
        );
        let review = source.review().unwrap();

        assert_eq!(review.groups.len(), 1);
        assert_eq!(review.groups[0].id, "all");
        assert_eq!(review.groups[0].title, format!("{base}...{head}"));
        assert_eq!(review.groups[0].files.len(), 1);
        let content = source.content(&review.groups[0].files[0].id).unwrap();
        assert_eq!(content.old.unwrap(), b"one\n");
        assert_eq!(content.new.unwrap(), b"three\n");
    }
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    use crate::source::testutil::TempRepo;

    #[test]
    fn worktree_watch_paths_lists_new_side_files() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        repo.add_and_commit("base");
        repo.write("a.txt", "two\n");
        let source = GitSource::new(repo.path.clone(), GitMode::Worktree);
        source.review().unwrap();

        assert_eq!(source.watch_paths(), vec![repo.path.join("a.txt")]);
    }
}
