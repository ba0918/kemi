//! git を読む入力ソース（R-INPUT-2〜4）。

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::domain::content;
use crate::domain::focus::FocusTargets;
use crate::domain::noise::{classify, linguist_generated, NoiseInput};
use crate::domain::review::{FileEntry, Group, ReviewMeta, Status};
use crate::source::origin::{file_origin, OriginPaths, OriginRange};
use crate::source::{FileOrigin, Plan, PlanStore, PlannedFile, ReviewSource, SideRef, SourceError};

/// 内容を読まずに統計だけを出す untracked の上限（D6）。
pub const UNTRACKED_LIMIT: u64 = 1_048_576;

/// porcelain の `git diff` に渡し、利用者の diff.orderFile を空の並びで打ち消す。
/// その設定は出力の順（一覧の既定の並び、R-VIEW）を変え、指すファイルが無いと
/// diff ごと失敗する。plumbing（diff-files / diff-tree）はこの設定を読まない。
const NO_ORDER_FILE: &str = "-O/dev/null";

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

pub use crate::domain::review::GroupBy;

#[derive(Clone, Debug)]
struct DiffEntry {
    path: PathBuf,
    old_path: Option<PathBuf>,
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
    final_origin: Mutex<Option<FinalOrigin>>,
}

/// 最終形の計画に添える、由来を求めるための範囲とファイルごとのパス。
struct FinalOrigin {
    range: OriginRange,
    files: HashMap<String, OriginPaths>,
}

/// 再取得しても同じファイルには同じ id を返す（`?refresh=1` でコメントの
/// file_id が別ファイルを指さないようにする）。
#[derive(Default)]
struct FileIds {
    map: HashMap<(String, PathBuf), String>,
    next: usize,
}

impl FileIds {
    fn get(&mut self, group_id: &str, path: &Path) -> String {
        let key = (group_id.to_string(), path.to_path_buf());
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
            final_origin: Mutex::new(None),
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
            let full = self.repo.join(path_from_bytes(path));
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
                path: path_from_bytes(path),
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
                    spec: git_spec("HEAD:", entry.old_path.as_deref().unwrap_or(&entry.path)),
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
                    spec: git_spec("HEAD:", entry.old_path.as_deref().unwrap_or(&entry.path)),
                },
            };
            let new = match entry.status {
                Status::Delete => SideRef::Absent,
                _ => SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec(":", &entry.path),
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

    /// コミットごとの単位。マージ以外の各コミットが 1 グループ。コミットの差分は
    /// 並列に求める（R-UNIT）。
    fn review_per_commit(&self, from: &str, to: &str) -> Result<(ReviewMeta, Plan), SourceError> {
        verify_ref(&self.repo, from)?;
        verify_ref(&self.repo, to)?;
        let attributes = read_attributes(&self.repo);
        let commits = range_messages(&self.repo, &format!("{from}..{to}"))?;
        let shas: Vec<String> = commits.iter().map(|commit| commit.sha.clone()).collect();
        let mut diffs = commit_diff_entries(&self.repo, &shas)?;

        let mut groups = Vec::new();
        let mut plan_files = Vec::new();
        for commit in commits {
            let entries = diffs.remove(&commit.sha).unwrap_or_default();
            let sha = commit.sha.as_str();
            let files = self.build_entries(&entries, &attributes, sha, |entry| {
                let old = match entry.status {
                    Status::Add => SideRef::Absent,
                    _ => SideRef::Git {
                        repo: self.repo.clone(),
                        spec: git_spec(
                            &format!("{sha}^:"),
                            entry.old_path.as_deref().unwrap_or(&entry.path),
                        ),
                    },
                };
                let new = match entry.status {
                    Status::Delete => SideRef::Absent,
                    _ => SideRef::Git {
                        repo: self.repo.clone(),
                        spec: git_spec(&format!("{sha}:"), &entry.path),
                    },
                };
                (old, new)
            });
            plan_files.extend(files.1);
            groups.push(Group {
                id: commit.sha,
                title: commit.subject,
                why: commit.body,
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

    /// 最終形の単位。`from...to` の全体を 1 グループにする。
    fn review_final(&self, from: &str, to: &str) -> Result<(ReviewMeta, Plan), SourceError> {
        verify_ref(&self.repo, from)?;
        verify_ref(&self.repo, to)?;
        let attributes = read_attributes(&self.repo);
        // 由来と内容が同じ版を指すよう、範囲の端を sha に解決して使う。
        let from_sha = resolve_ref(&self.repo, from)?;
        let to_sha = resolve_ref(&self.repo, to)?;
        let merge_base = git_text(&self.repo, &["merge-base", &from_sha, &to_sha])?
            .trim()
            .to_string();
        let range = format!("{from_sha}...{to_sha}");
        let entries = diff_entries(&self.repo, &[&range])?;
        let files = self.build_entries(&entries, &attributes, "all", |entry| {
            let old = match entry.status {
                Status::Add => SideRef::Absent,
                _ => SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec(
                        &format!("{merge_base}:"),
                        entry.old_path.as_deref().unwrap_or(&entry.path),
                    ),
                },
            };
            let new = match entry.status {
                Status::Delete => SideRef::Absent,
                _ => SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec(&format!("{to_sha}:"), &entry.path),
                },
            };
            (old, new)
        });
        let origin_files: HashMap<String, OriginPaths> = files
            .1
            .iter()
            .zip(&entries)
            .map(|(file, entry)| {
                (
                    file.id.clone(),
                    OriginPaths {
                        old: (entry.status != Status::Add)
                            .then(|| entry.old_path.clone().unwrap_or(entry.path.clone())),
                        new: (entry.status != Status::Delete).then(|| entry.path.clone()),
                    },
                )
            })
            .collect();
        *self.final_origin.lock().expect("origin lock poisoned") = Some(FinalOrigin {
            range: OriginRange {
                from: from_sha,
                to: to_sha,
                merge_base,
            },
            files: origin_files,
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
                    files: files.0,
                }],
                approval: Vec::new(),
            },
            Plan { files: files.1 },
        ))
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
            // 表示は UTF-8 へ置換してよいが、id と読み取りは元のバイトのまま扱う。
            let display_path = entry.path.to_string_lossy().into_owned();
            let noise = classify(&NoiseInput {
                path: &display_path,
                linguist_generated: linguist_generated(attributes, &display_path),
                binary: entry.binary,
            })
            .is_some();
            files.push(FileEntry {
                id: id.clone(),
                group_id: group_id.to_string(),
                path: display_path,
                old_path: entry
                    .old_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
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
            GitMode::Range { group_by, .. } => return self.review_unit(*group_by),
        };
        self.store.update(plan);
        Ok(review)
    }

    fn units(&self) -> Vec<GroupBy> {
        match &self.mode {
            GitMode::Range { group_by, .. } => vec![*group_by, group_by.other()],
            GitMode::Worktree | GitMode::Staged => Vec::new(),
        }
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        let GitMode::Range { from, to, .. } = &self.mode else {
            return self.review();
        };
        let (review, plan) = match unit {
            GroupBy::File => self.review_final(from, to)?,
            GroupBy::Commit => self.review_per_commit(from, to)?,
        };
        self.store.update_unit(Some(unit), plan);
        Ok(review)
    }

    fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
        let GitMode::Range { from, to, .. } = &self.mode else {
            return Ok(FocusTargets::default());
        };
        let shas = git_text(
            &self.repo,
            &["rev-list", "--no-merges", &format!("{from}..{to}")],
        )?;
        let shas: Vec<String> = shas.split_whitespace().map(str::to_string).collect();
        let mut group_ids: std::collections::BTreeSet<String> = shas.iter().cloned().collect();
        group_ids.insert("all".to_string());

        let final_paths = git_raw(
            &self.repo,
            &[
                "diff",
                NO_ORDER_FILE,
                "--name-only",
                "-z",
                "-M",
                &format!("{from}...{to}"),
            ],
        )?;
        let commit_paths = if shas.is_empty() {
            Vec::new()
        } else {
            git_with_input(
                &self.repo,
                &[
                    "diff-tree",
                    "--stdin",
                    "--no-commit-id",
                    "--name-only",
                    "-r",
                    "-M",
                    "--root",
                    "-z",
                ],
                format!("{}\n", shas.join("\n")).as_bytes(),
            )?
        };
        let paths = split_z(&final_paths)
            .into_iter()
            .chain(split_z(&commit_paths))
            .map(|path| path_from_bytes(path).to_string_lossy().into_owned())
            .collect();
        Ok(FocusTargets { group_ids, paths })
    }

    fn content(&self, file_id: &str) -> Result<crate::source::FileContent, SourceError> {
        self.store.content(file_id)
    }

    fn origin(&self, file_id: &str, force: bool) -> Result<Option<FileOrigin>, SourceError> {
        let (range, paths) = {
            let guard = self.final_origin.lock().expect("origin lock poisoned");
            let Some(context) = guard.as_ref() else {
                return Ok(None);
            };
            let Some(paths) = context.files.get(file_id) else {
                return Ok(None);
            };
            (context.range.clone(), paths.clone())
        };
        let content = self.store.content(file_id)?;
        file_origin(&self.repo, &range, &paths, &content, force)
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
    let head_paths: std::collections::HashSet<PathBuf> = split_z(&git_raw(
        repo,
        &["ls-tree", "-r", "--name-only", "-z", "HEAD"],
    )?)
    .into_iter()
    .map(path_from_bytes)
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
    let mut paths: Vec<PathBuf> = Vec::new();
    // 改名は削除と追加の組で検出される。組の片方だけを pathspec に渡すと改名が
    // 割れるため、削除・追加になり得るパスは 1 つの呼び出しに束ねる。
    let mut rename_candidates: Vec<PathBuf> = Vec::new();
    for listing in [
        git_raw(repo, &["diff-files", "--name-status", "-z", "--no-renames"])?,
        git_raw(
            repo,
            &[
                "diff",
                NO_ORDER_FILE,
                "--name-status",
                "-z",
                "--no-renames",
                "--cached",
            ],
        )?,
    ] {
        for (status, path) in parse_name_status_z(&listing) {
            if matches!(status, b'A' | b'D') {
                rename_candidates.push(path.clone());
            }
            paths.push(path);
        }
    }
    for path in split_z(&git_raw(
        repo,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?) {
        paths.push(path_from_bytes(path));
    }
    paths.sort();
    paths.dedup();
    if paths.len() < 256 {
        return Ok(parse_numstat(&git_raw(
            repo,
            &["diff", NO_ORDER_FILE, "--numstat", "-z", "-M", "HEAD"],
        )?));
    }

    rename_candidates.sort();
    rename_candidates.dedup();
    let regular: Vec<PathBuf> = paths
        .iter()
        .filter(|path| rename_candidates.binary_search(path).is_err())
        .cloned()
        .collect();

    let workers = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .clamp(2, 16);
    let mut shards: Vec<Vec<PathBuf>> = Vec::new();
    if !rename_candidates.is_empty() {
        shards.push(rename_candidates);
    }
    if !regular.is_empty() {
        let chunk = regular.len().div_ceil(workers);
        shards.extend(regular.chunks(chunk).map(<[PathBuf]>::to_vec));
    }
    let mut handles = Vec::new();
    for shard in shards {
        let repo = repo.to_path_buf();
        handles.push(std::thread::spawn(
            move || -> Result<Vec<Numstat>, SourceError> {
                let mut args: Vec<OsString> = vec![
                    "diff".into(),
                    NO_ORDER_FILE.into(),
                    "--numstat".into(),
                    "-z".into(),
                    "-M".into(),
                    "HEAD".into(),
                    "--".into(),
                ];
                // グロブ文字を含むパスが別のシャードのパスに一致しないよう literal で渡す。
                args.extend(shard.iter().map(|path| literal_pathspec(path)));
                let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
                Ok(parse_numstat(&git_raw_os(&repo, &args)?))
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

/// `-- <pathspec>` に渡す literal な pathspec。生のバイトのまま組み立てる。
fn literal_pathspec(path: &Path) -> OsString {
    let mut spec = OsString::from(":(literal)");
    spec.push(path.as_os_str());
    spec
}

/// `prefix` に生のパスをつないで 1 つの git 引数（例 `HEAD:src/a.rs`）にする。
fn git_spec(prefix: &str, path: &Path) -> OsString {
    let mut spec = OsString::from(prefix);
    spec.push(path.as_os_str());
    spec
}

/// git が返した生のバイト列をパスとして運ぶ。表示のときだけ lossy に変換する。
#[cfg(unix)]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

fn diff_entries(repo: &Path, range_args: &[&str]) -> Result<Vec<DiffEntry>, SourceError> {
    let mut args = vec!["diff", NO_ORDER_FILE, "--numstat", "-z", "-M"];
    args.extend_from_slice(range_args);
    let numstat = parse_numstat(&git_raw(repo, &args)?);

    let mut args = vec!["diff", NO_ORDER_FILE, "--name-status", "-z", "-M"];
    args.extend_from_slice(range_args);
    let name_status = parse_name_status(&git_raw(repo, &args)?);
    Ok(combine_entries(name_status, numstat))
}

/// 状態（name-status / raw）と増減数（numstat）を、新側のパスで突き合わせる。
fn combine_entries(name_status: Vec<NameStatus>, numstat: Vec<Numstat>) -> Vec<DiffEntry> {
    let statistics: HashMap<PathBuf, Numstat> = numstat
        .into_iter()
        .map(|stat| (stat.new_path.clone(), stat))
        .collect();
    name_status
        .into_iter()
        .map(|name| {
            let stat = statistics.get(&name.path);
            DiffEntry {
                add: stat.map_or(0, |stat| stat.add),
                del: stat.map_or(0, |stat| stat.del),
                binary: stat.is_some_and(|stat| stat.binary),
                path: name.path,
                old_path: name.old_path,
                status: name.status,
            }
        })
        .collect()
}

/// 範囲のコミット（マージを除く、古い順）の件名と本文。
struct CommitMessage {
    sha: String,
    subject: String,
    body: String,
}

fn range_messages(repo: &Path, range: &str) -> Result<Vec<CommitMessage>, SourceError> {
    let text = git_log(
        repo,
        &[
            "--no-merges",
            "--reverse",
            "-z",
            "--format=%H%x1f%s%x1f%b",
            range,
        ],
    )?;
    Ok(text
        .split('\0')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut fields = record.splitn(3, '\x1f');
            Some(CommitMessage {
                sha: fields.next()?.trim().to_string(),
                subject: fields.next().unwrap_or_default().trim_end().to_string(),
                body: fields.next().unwrap_or_default().trim().to_string(),
            })
        })
        .collect())
}

/// 並列に git を呼ぶ本数（D8）。範囲の標準フィクスチャで、コミットごとの単位を
/// 1 秒未満で作りつつ、表示中の api/file を遅らせない値として選んだ。
fn commit_diff_workers() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .clamp(2, 8)
}

/// 各コミットの差分（状態と増減数）。コミットを分けて `git diff-tree --stdin` を並列に呼ぶ。
fn commit_diff_entries(
    repo: &Path,
    shas: &[String],
) -> Result<HashMap<String, Vec<DiffEntry>>, SourceError> {
    if shas.is_empty() {
        return Ok(HashMap::new());
    }
    let chunk = shas.len().div_ceil(commit_diff_workers());
    let handles: Vec<_> = shas
        .chunks(chunk)
        .map(|chunk| {
            let repo = repo.to_path_buf();
            let input = format!("{}\n", chunk.join("\n"));
            std::thread::spawn(move || -> Result<Vec<u8>, SourceError> {
                git_with_input(
                    &repo,
                    &[
                        "diff-tree",
                        "--stdin",
                        "-r",
                        "-M",
                        "--root",
                        "--raw",
                        "--numstat",
                        "-z",
                    ],
                    input.as_bytes(),
                )
            })
        })
        .collect();
    let mut entries = HashMap::new();
    for handle in handles {
        let output = handle
            .join()
            .map_err(|_| SourceError::Git("diff-tree の並列実行に失敗しました".to_string()))??;
        for (sha, name_status, numstat) in parse_diff_tree(&output) {
            entries.insert(sha, combine_entries(name_status, numstat));
        }
    }
    Ok(entries)
}

/// `git diff-tree --stdin --raw --numstat -z` の出力を、コミットごとの状態と増減数に分ける。
/// 各コミットは「sha\0」に続いて raw の記録、その後に numstat の記録が並ぶ。
fn parse_diff_tree(bytes: &[u8]) -> Vec<(String, Vec<NameStatus>, Vec<Numstat>)> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut commits: Vec<(String, Vec<NameStatus>, Vec<Numstat>)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.is_empty() {
            index += 1;
            continue;
        }
        if token[0] == b':' {
            let letter = token
                .rsplit(|byte| *byte == b' ')
                .next()
                .and_then(|field| field.first().copied())
                .unwrap_or(b'M');
            // 知らない状態の記録もパスを 1 つ持つので、変更として読んでパスを取り込む。
            let letter = match letter {
                b'R' | b'C' | b'A' | b'D' | b'M' | b'T' => letter,
                _ => b'M',
            };
            let (record, consumed) = name_status_record(letter, &tokens, index);
            if let (Some(record), Some((_, names, _))) = (record, commits.last_mut()) {
                names.push(record);
            }
            index += consumed;
            continue;
        }
        if token.contains(&b'\t') {
            let (record, consumed) = numstat_record(&tokens, index);
            if let (Some(record), Some((_, _, stats))) = (record, commits.last_mut()) {
                stats.push(record);
            }
            index += consumed;
            continue;
        }
        commits.push((
            String::from_utf8_lossy(token).into_owned(),
            Vec::new(),
            Vec::new(),
        ));
        index += 1;
    }
    commits
}

/// 標準入力を渡して git を呼ぶ。
fn git_with_input(repo: &Path, args: &[&str], input: &[u8]) -> Result<Vec<u8>, SourceError> {
    use std::io::Write;
    use std::process::Stdio;

    let io_error = |source| SourceError::Io {
        path: repo.to_path_buf(),
        source,
    };
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
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
    if !output.status.success() {
        return Err(SourceError::Git(format!(
            "git {} が失敗しました: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

struct Numstat {
    old_path: Option<PathBuf>,
    new_path: PathBuf,
    add: u64,
    del: u64,
    binary: bool,
}

fn parse_numstat(bytes: &[u8]) -> Vec<Numstat> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is_empty() {
            index += 1;
            continue;
        }
        let (record, consumed) = numstat_record(&tokens, index);
        entries.extend(record);
        index += consumed;
    }
    entries
}

/// `-z` の numstat の 1 記録を読む。`add\tdel\tpath`、改名なら `add\tdel\t` に続く
/// 旧・新のパス。読んだ記録と、使ったトークン数を返す。
fn numstat_record(tokens: &[&[u8]], index: usize) -> (Option<Numstat>, usize) {
    let fields: Vec<&[u8]> = tokens[index].splitn(3, |byte| *byte == b'\t').collect();
    if fields.len() < 3 {
        return (None, 1);
    }
    let (add, del, binary) = (
        parse_count(fields[0]),
        parse_count(fields[1]),
        fields[0] == b"-",
    );
    if !fields[2].is_empty() {
        let record = Numstat {
            old_path: None,
            new_path: path_from_bytes(fields[2]),
            add,
            del,
            binary,
        };
        return (Some(record), 1);
    }
    if index + 2 < tokens.len() {
        let record = Numstat {
            old_path: Some(path_from_bytes(tokens[index + 1])),
            new_path: path_from_bytes(tokens[index + 2]),
            add,
            del,
            binary,
        };
        return (Some(record), 3);
    }
    (None, 1)
}

struct NameStatus {
    path: PathBuf,
    old_path: Option<PathBuf>,
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
        let (record, consumed) = name_status_record(token[0], &tokens, index);
        entries.extend(record);
        index += consumed;
    }
    entries
}

/// 状態の文字 `letter` を持つ記録（`tokens[index]`）に続くパスを読む。改名とコピーは
/// 旧・新の 2 つ、ほかは 1 つ。読んだ記録と、使ったトークン数を返す。
fn name_status_record(letter: u8, tokens: &[&[u8]], index: usize) -> (Option<NameStatus>, usize) {
    match letter {
        b'R' | b'C' if index + 2 < tokens.len() => (
            Some(NameStatus {
                path: path_from_bytes(tokens[index + 2]),
                old_path: Some(path_from_bytes(tokens[index + 1])),
                status: Status::Rename,
            }),
            3,
        ),
        b'A' | b'D' | b'M' | b'T' if index + 1 < tokens.len() => (
            Some(NameStatus {
                path: path_from_bytes(tokens[index + 1]),
                old_path: None,
                status: match letter {
                    b'A' => Status::Add,
                    b'D' => Status::Delete,
                    _ => Status::Modify,
                },
            }),
            2,
        ),
        _ => (None, 1),
    }
}

/// `--name-status -z --no-renames` の「状態\0パス\0」を読む。
fn parse_name_status_z(bytes: &[u8]) -> Vec<(u8, PathBuf)> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        if tokens[index].is_empty() {
            index += 1;
            continue;
        }
        entries.push((tokens[index][0], path_from_bytes(tokens[index + 1])));
        index += 2;
    }
    entries
}

fn parse_count(field: &[u8]) -> u64 {
    if field == b"-" {
        return 0;
    }
    String::from_utf8_lossy(field).parse().unwrap_or(0)
}

fn split_z(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect()
}

fn side_size(side: &SideRef) -> u64 {
    match side {
        SideRef::Absent => 0,
        SideRef::Inline(bytes) => bytes.len() as u64,
        SideRef::Disk(path) => std::fs::metadata(path).map_or(0, |meta| meta.len()),
        SideRef::Git { repo, spec } => git_raw_os(
            repo,
            &[OsStr::new("cat-file"), OsStr::new("-s"), spec.as_os_str()],
        )
        .ok()
        .and_then(|text| {
            std::str::from_utf8(&text)
                .ok()
                .and_then(|text| text.trim().parse().ok())
        })
        .unwrap_or(0),
    }
}

fn read_attributes(repo: &Path) -> String {
    std::fs::read_to_string(repo.join(".gitattributes")).unwrap_or_default()
}

fn resolve_ref(repo: &Path, revision: &str) -> Result<String, SourceError> {
    let spec = format!("{revision}^{{commit}}");
    Ok(git_text(repo, &["rev-parse", "--verify", "--quiet", &spec])
        .map_err(|_| SourceError::Git(format!("ref が見つかりません: {revision}")))?
        .trim()
        .to_string())
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

/// `--to` の ref 更新を検知するための監視パス。返すのは ref のファイルそのもので、
/// 親ディレクトリは含めない。親を渡すとその下の別ブランチの更新まで拾うため。
/// packed refs で loose ref が無い場合は、存在しないパスを返して watch 側に
/// 親ディレクトリの監視とファイル名の照合を任せる。
pub fn ref_watch_paths(repo: &Path, reference: &str) -> Result<Vec<PathBuf>, SourceError> {
    let full = git_text(repo, &["rev-parse", "--symbolic-full-name", reference])?
        .trim()
        .to_string();
    let git_dir = PathBuf::from(git_text(repo, &["rev-parse", "--absolute-git-dir"])?.trim());
    let mut paths = Vec::new();
    // HEAD がシンボリック ref のとき、コミットで書き換わるのは参照先の
    // loose ref で、HEAD 自身は書き換わらない。両方を監視して、checkout で
    // 見先が変わる場合と、参照先の ref が進む場合の両方を拾う。
    if reference == "HEAD" || full == "HEAD" {
        paths.push(git_dir.join("HEAD"));
    }
    if full.starts_with("refs/") {
        paths.push(git_dir.join(&full));
    }
    Ok(paths)
}

pub fn repo_root(path: &Path) -> Result<PathBuf, SourceError> {
    let text = git_text(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(text.trim()))
}

pub(crate) fn show(repo: &Path, spec: &OsStr) -> Result<Vec<u8>, SourceError> {
    git_raw_os(repo, &[OsStr::new("show"), spec])
}

pub(crate) fn git_raw(repo: &Path, args: &[&str]) -> Result<Vec<u8>, SourceError> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git_raw_os(repo, &args)
}

/// git の引数に生のバイト列（非 UTF-8 のパスを含む pathspec）を渡せる版。
pub(crate) fn git_raw_os(repo: &Path, args: &[&OsStr]) -> Result<Vec<u8>, SourceError> {
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
            display_args(args),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn display_args(args: &[&OsStr]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

/// コミットを読む `git log`。利用者の設定で、読み取る出力の形が変わらないようにする。
/// log.showSignature は署名の検証結果を sha の前に混ぜ、i18n.logOutputEncoding は
/// 件名と本文を UTF-8 以外で出すため、どちらも引数で打ち消す。
pub(crate) fn git_log(repo: &Path, args: &[&str]) -> Result<String, SourceError> {
    let mut full = vec!["log", "--no-show-signature", "--encoding=UTF-8"];
    full.extend_from_slice(args);
    git_text(repo, &full)
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
    fn worktree_rename_is_detected_among_many_changes() {
        let repo = TempRepo::new();
        let body: String = (1..=20).map(|n| format!("line{n}\n")).collect();
        repo.write("a000.txt", &body);
        for index in 0..300 {
            repo.write(&format!("m{index:03}.txt"), "one\n");
        }
        repo.add_and_commit("base");

        repo.git(&["mv", "a000.txt", "zzz.txt"]);
        for index in 0..300 {
            repo.write(&format!("m{index:03}.txt"), "one\ntwo\n");
        }

        let source = source(&repo, GitMode::Worktree);
        let review = source.review().unwrap();

        assert_eq!(review.groups[0].files.len(), 301);
        let entry = find_file(&review, "zzz.txt");
        assert_eq!(entry.status, Status::Rename);
        assert_eq!(entry.old_path.as_deref(), Some("a000.txt"));
        assert_eq!((entry.add, entry.del), (0, 0));
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

    /// a.txt / b.txt / c.txt を、範囲のコミット・index・作業ツリーのそれぞれで変え、
    /// 利用者の設定として diff.orderFile を置いたリポジトリ。git commit も
    /// diff.orderFile を読むので、設定は最後に置く。
    fn repo_with_diff_order_file(order_file: &str) -> (TempRepo, String) {
        let repo = TempRepo::new();
        let names = ["a.txt", "b.txt", "c.txt"];
        names.iter().for_each(|name| repo.write(name, "base\n"));
        let base = repo.add_and_commit("base");
        names.iter().for_each(|name| repo.write(name, "range\n"));
        repo.add_and_commit("change");
        names.iter().for_each(|name| repo.write(name, "staged\n"));
        repo.git(&["add", "-A"]);
        names.iter().for_each(|name| repo.write(name, "worktree\n"));
        let order_file = repo.path.join(".git").join(order_file);
        repo.git(&["config", "diff.orderFile", order_file.to_str().unwrap()]);
        (repo, base)
    }

    /// 最終形・worktree・staged のそれぞれで、一覧に並ぶパス。
    fn listed_paths_in_each_mode(repo: &TempRepo, base: &str) -> Vec<Vec<String>> {
        let range = GitMode::Range {
            from: base.to_string(),
            to: "HEAD".to_string(),
            group_by: GroupBy::File,
        };
        [range, GitMode::Worktree, GitMode::Staged]
            .into_iter()
            .map(|mode| {
                let review = source(repo, mode.clone())
                    .review()
                    .unwrap_or_else(|error| panic!("{mode:?}: {error}"));
                review.groups[0]
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn missing_diff_order_file_does_not_stop_the_review() {
        let (repo, base) = repo_with_diff_order_file("missing-order");

        for paths in listed_paths_in_each_mode(&repo, &base) {
            assert_eq!(paths, ["a.txt", "b.txt", "c.txt"]);
        }
    }

    #[test]
    fn diff_order_file_does_not_reorder_the_file_list() {
        let (repo, base) = repo_with_diff_order_file("order");
        std::fs::write(repo.path.join(".git").join("order"), "c.txt\nb.txt\n").unwrap();

        for paths in listed_paths_in_each_mode(&repo, &base) {
            assert_eq!(paths, ["a.txt", "b.txt", "c.txt"]);
        }
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
    fn range_commit_group_ids_are_shas_even_when_log_shows_signatures() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        let base = repo.add_and_commit("base");
        repo.write("a.txt", "two\n");
        if repo.add_and_commit_signed("signed").is_none() {
            return;
        }
        repo.write("a.txt", "three\n");
        repo.add_and_commit("unsigned");
        repo.git(&["config", "log.showSignature", "true"]);

        let review = source(
            &repo,
            GitMode::Range {
                from: base.clone(),
                to: "HEAD".to_string(),
                group_by: GroupBy::Commit,
            },
        )
        .review()
        .unwrap();

        let ids: Vec<&str> = review
            .groups
            .iter()
            .map(|group| group.id.as_str())
            .collect();
        let expected = repo.git(&[
            "rev-list",
            "--reverse",
            "--no-merges",
            &format!("{base}..HEAD"),
        ]);
        assert_eq!(ids, expected.lines().collect::<Vec<_>>());
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

    #[cfg(unix)]
    fn non_utf8_name() -> &'static std::ffi::OsStr {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(b"b\xffad.txt")
    }

    #[cfg(unix)]
    fn find_non_utf8(review: &ReviewMeta) -> &FileEntry {
        review
            .groups
            .iter()
            .flat_map(|group| group.files.iter())
            .find(|file| file.path.ends_with("ad.txt"))
            .expect("non-UTF-8 path must be listed")
    }

    #[cfg(unix)]
    #[test]
    fn worktree_includes_non_utf8_untracked_file() {
        let repo = TempRepo::new();
        repo.write("a.txt", "a\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        std::fs::write(repo.path.join(non_utf8_name()), "x\ny\n").unwrap();

        let review = source.review().unwrap();
        let entry = find_non_utf8(&review);

        assert_eq!(entry.status, Status::Add);
        assert_eq!((entry.add, entry.del), (2, 0));
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old, None);
        assert_eq!(content.new.unwrap(), b"x\ny\n");
    }

    #[cfg(unix)]
    #[test]
    fn worktree_reads_non_utf8_tracked_file() {
        let repo = TempRepo::new();
        std::fs::write(repo.path.join(non_utf8_name()), "one\n").unwrap();
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Worktree);

        std::fs::write(repo.path.join(non_utf8_name()), "two\n").unwrap();

        let review = source.review().unwrap();
        let entry = find_non_utf8(&review);

        assert_eq!(entry.status, Status::Modify);
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old.unwrap(), b"one\n");
        assert_eq!(content.new.unwrap(), b"two\n");
    }

    #[cfg(unix)]
    #[test]
    fn staged_reads_non_utf8_path() {
        let repo = TempRepo::new();
        repo.write("a.txt", "a\n");
        repo.add_and_commit("base");
        let source = source(&repo, GitMode::Staged);

        std::fs::write(repo.path.join(non_utf8_name()), "x\ny\n").unwrap();
        repo.git(&["add", "-A"]);

        let review = source.review().unwrap();
        let entry = find_non_utf8(&review);

        assert_eq!(entry.status, Status::Add);
        let content = source.content(&entry.id).unwrap();
        assert_eq!(content.old, None);
        assert_eq!(content.new.unwrap(), b"x\ny\n");
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

    #[test]
    fn ref_watch_paths_point_at_ref_files_not_their_directories() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        repo.add_and_commit("base");
        let branch = repo.git(&["symbolic-ref", "--short", "HEAD"]);

        let paths = ref_watch_paths(&repo.path, "HEAD").unwrap();

        assert!(
            paths.iter().all(|path| !path.is_dir()),
            "watch paths must be files only: {paths:?}"
        );
        assert!(paths.iter().any(|path| path.ends_with("HEAD")));
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(format!("refs/heads/{branch}"))),
            "the resolved branch ref must be watched: {paths:?}"
        );
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::source::testutil::TempRepo;
    use crate::source::{FileContent, FocusSource};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 3 コミットの範囲。`b.txt` は範囲の途中で足して消すので、最終形には出ない。
    fn range_repo() -> (TempRepo, String, Vec<String>) {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        let base = repo.add_and_commit("base");
        repo.write("a.txt", "two\n");
        repo.write("b.txt", "temporary\n");
        let first = repo.add_and_commit("feat: a and b");
        repo.remove("b.txt");
        let second = repo.add_and_commit("fix: drop b");
        repo.write("a.txt", "three\n");
        let third = repo.add_and_commit("feat: a again");
        (repo, base, vec![first, second, third])
    }

    fn range_source(repo: &TempRepo, from: &str, group_by: GroupBy) -> GitSource {
        GitSource::new(
            repo.path.clone(),
            GitMode::Range {
                from: from.to_string(),
                to: "HEAD".to_string(),
                group_by,
            },
        )
    }

    fn ids_by_group_and_path(review: &ReviewMeta) -> Vec<(String, String, String)> {
        review
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .files
                    .iter()
                    .map(|file| (group.id.clone(), file.path.clone(), file.id.clone()))
            })
            .collect()
    }

    #[test]
    fn unit_both_units_serve_content_from_one_source() {
        let (repo, base, commits) = range_repo();
        let source = range_source(&repo, &base, GroupBy::File);

        let final_form = source.review_unit(GroupBy::File).unwrap();
        let per_commit = source.review_unit(GroupBy::Commit).unwrap();

        assert_eq!(final_form.groups.len(), 1);
        assert_eq!(final_form.groups[0].id, "all");
        let group_ids: Vec<&str> = per_commit
            .groups
            .iter()
            .map(|group| group.id.as_str())
            .collect();
        assert_eq!(
            group_ids,
            commits.iter().map(String::as_str).collect::<Vec<_>>()
        );
        let final_a = &final_form.groups[0].files[0];
        let first_a = &per_commit.groups[0].files[0];
        assert_ne!(final_a.id, first_a.id);
        assert_eq!(
            source.content(&final_a.id).unwrap().new.unwrap(),
            b"three\n"
        );
        assert_eq!(source.content(&first_a.id).unwrap().new.unwrap(), b"two\n");
    }

    #[test]
    fn unit_refetch_keeps_other_unit_content_reachable() {
        let (repo, base, _) = range_repo();
        let source = range_source(&repo, &base, GroupBy::Commit);
        let per_commit = source.review_unit(GroupBy::Commit).unwrap();
        source.review_unit(GroupBy::File).unwrap();

        source.review_unit(GroupBy::File).unwrap();

        for (_, _, id) in ids_by_group_and_path(&per_commit) {
            assert!(source.content(&id).is_ok(), "{id} became unreachable");
        }
    }

    #[test]
    fn unit_ids_stable_across_refetch_of_both_units() {
        let (repo, base, _) = range_repo();
        let source = range_source(&repo, &base, GroupBy::File);
        let first = (
            source.review_unit(GroupBy::File).unwrap(),
            source.review_unit(GroupBy::Commit).unwrap(),
        );

        let second = (
            source.review_unit(GroupBy::Commit).unwrap(),
            source.review_unit(GroupBy::File).unwrap(),
        );

        assert_eq!(
            ids_by_group_and_path(&first.0),
            ids_by_group_and_path(&second.1)
        );
        assert_eq!(
            ids_by_group_and_path(&first.1),
            ids_by_group_and_path(&second.0)
        );
    }

    #[test]
    fn unit_commit_groups_keep_subject_and_body_in_range_order() {
        let repo = TempRepo::new();
        repo.write("a.txt", "one\n");
        let base = repo.add_and_commit("base");
        repo.write("a.txt", "two\n");
        repo.add_and_commit("feat: two\n\nwhy it changed\nsecond line");
        repo.write("a.txt", "three\n");
        repo.add_and_commit("fix: three");
        let source = range_source(&repo, &base, GroupBy::Commit);

        let review = source.review().unwrap();

        let titles: Vec<(&str, &str)> = review
            .groups
            .iter()
            .map(|group| (group.title.as_str(), group.why.as_str()))
            .collect();
        assert_eq!(
            titles,
            vec![
                ("feat: two", "why it changed\nsecond line"),
                ("fix: three", "")
            ]
        );
    }

    #[test]
    fn unit_units_list_startup_unit_first_only_for_ranges() {
        let (repo, base, _) = range_repo();

        assert_eq!(
            range_source(&repo, &base, GroupBy::File).units(),
            vec![GroupBy::File, GroupBy::Commit]
        );
        assert_eq!(
            range_source(&repo, &base, GroupBy::Commit).units(),
            vec![GroupBy::Commit, GroupBy::File]
        );
        assert!(GitSource::new(repo.path.clone(), GitMode::Worktree)
            .units()
            .is_empty());
    }

    fn focus_source(inner: Box<dyn ReviewSource>, repo: &TempRepo, json: &str) -> FocusSource {
        repo.write("focus.json", json);
        FocusSource::from_path(inner, Path::new("focus.json"), &repo.path).unwrap()
    }

    #[test]
    fn focus_accepts_commit_group_id_at_final_startup() {
        let (repo, base, commits) = range_repo();
        let json = format!(r#"{{"groups":{{"{}":{{"watch":"ここ"}}}}}}"#, commits[1]);
        let source = focus_source(
            Box::new(range_source(&repo, &base, GroupBy::File)),
            &repo,
            &json,
        );

        source.review().unwrap();
        let per_commit = source.review_unit(GroupBy::Commit).unwrap();

        let group = per_commit
            .groups
            .iter()
            .find(|group| group.id == commits[1])
            .unwrap();
        assert_eq!(group.watch, "ここ");
    }

    #[test]
    fn focus_accepts_path_removed_within_range_and_marks_both_units() {
        let (repo, base, _) = range_repo();
        let source = focus_source(
            Box::new(range_source(&repo, &base, GroupBy::File)),
            &repo,
            r#"{"files":[{"path":"b.txt","note":"途中で消えた"},{"path":"a.txt"}]}"#,
        );

        let final_form = source.review().unwrap();
        let per_commit = source.review_unit(GroupBy::Commit).unwrap();

        assert!(final_form.groups[0].files[0].focus);
        let b_entries: Vec<&FileEntry> = per_commit
            .groups
            .iter()
            .flat_map(|group| group.files.iter())
            .filter(|file| file.path == "b.txt")
            .collect();
        assert_eq!(b_entries.len(), 2);
        assert!(b_entries
            .iter()
            .all(|file| file.focus && file.note == "途中で消えた"));
    }

    #[test]
    fn focus_rejects_unknown_path_in_range() {
        let (repo, base, _) = range_repo();
        let source = focus_source(
            Box::new(range_source(&repo, &base, GroupBy::File)),
            &repo,
            r#"{"files":[{"path":"missing.txt"}]}"#,
        );

        let message = source.review().unwrap_err().to_string();

        assert!(message.contains("missing.txt"), "{message}");
    }

    #[test]
    fn focus_rejects_unknown_group_id_in_range() {
        let (repo, base, _) = range_repo();
        let source = focus_source(
            Box::new(range_source(&repo, &base, GroupBy::Commit)),
            &repo,
            r#"{"groups":{"0000000":{"watch":"x"}}}"#,
        );

        assert!(source.review().is_err());
    }

    /// 内容の取得（`content`）の呼び出しを数える。
    struct CountingSource {
        inner: GitSource,
        reads: Arc<AtomicUsize>,
    }

    impl ReviewSource for CountingSource {
        fn review(&self) -> Result<ReviewMeta, SourceError> {
            self.inner.review()
        }

        fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.content(file_id)
        }

        fn units(&self) -> Vec<GroupBy> {
            self.inner.units()
        }

        fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
            self.inner.review_unit(unit)
        }

        fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
            self.inner.extra_focus_targets()
        }
    }

    #[test]
    fn unit_startup_reads_no_content_even_with_focus() {
        let (repo, base, commits) = range_repo();
        for group_by in [GroupBy::File, GroupBy::Commit] {
            let reads = Arc::new(AtomicUsize::new(0));
            let counting = CountingSource {
                inner: range_source(&repo, &base, group_by),
                reads: reads.clone(),
            };
            let json = format!(
                r#"{{"groups":{{"{}":{{"watch":"w"}}}},"files":[{{"path":"b.txt"}}]}}"#,
                commits[0]
            );
            let source = focus_source(Box::new(counting), &repo, &json);

            source.review().unwrap();

            assert_eq!(reads.load(Ordering::SeqCst), 0, "{group_by:?}");
        }
    }
}
