//! git を読む入力ソース（R-INPUT-2〜4）。

mod command;
mod parse;
mod repository_file;
mod sizes;

pub(crate) use command::{git_log, git_raw, git_raw_os, git_text, git_with_stdin, show};
pub(crate) use parse::{path_bytes, path_from_bytes, split_z};

use command::git_with_input;
use parse::{
    NameStatus, Numstat, parse_diff_tree, parse_name_status, parse_name_status_z, parse_numstat,
};
use repository_file::{is_plain_repository_path, read_tree_file, read_worktree_file};
use sizes::side_sizes;

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::domain::content;
use crate::domain::focus::FocusTargets;
use crate::domain::noise::{GeneratedAttributes, NoiseInput, classify};
use crate::domain::review::{FileEntry, Group, ReviewMeta, Side, Status};
use crate::session::SessionError;
use crate::source::origin::{OriginPaths, OriginRange, file_origin};
use crate::source::work_tree::WorkTree;
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
    new_lines: Option<u32>,
    path: PathBuf,
    old_path: Option<PathBuf>,
    status: Status,
    add: u64,
    del: u64,
    binary: bool,
    content_skipped: bool,
}

/// 変更の状態から、旧側と新側の内容をどこから読むかを決める。旧側は改名前のパスを優先する。
fn select_sides(
    entry: &DiffEntry,
    old_ref: impl FnOnce(&Path) -> SideRef,
    new_ref: impl FnOnce(&Path) -> SideRef,
) -> (SideRef, SideRef) {
    let old = match entry.status {
        Status::Add => SideRef::Absent,
        Status::Delete | Status::Rename | Status::Modify => {
            old_ref(entry.old_path.as_deref().unwrap_or(&entry.path))
        }
    };
    let new = match entry.status {
        Status::Delete => SideRef::Absent,
        Status::Add | Status::Rename | Status::Modify => new_ref(&entry.path),
    };
    (old, new)
}

pub struct GitSource {
    repo: PathBuf,
    mode: GitMode,
    store: PlanStore,
    ids: Arc<Mutex<FileIds>>,
    labels: Option<(String, String)>,
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
            ids: Arc::new(Mutex::new(FileIds::default())),
            labels: None,
            final_origin: Mutex::new(None),
        }
    }

    /// これまでに振った id から、その id のファイルの生のパス（git が返したバイト列）への対応。
    /// 表示のパスは lossy なので、別のファイルを見分けるにはこちらを使う。
    pub fn raw_paths(&self) -> HashMap<String, Vec<u8>> {
        self.ids
            .lock()
            .expect("file id lock poisoned")
            .map
            .iter()
            .map(|((_, path), id)| (id.clone(), path_bytes(path)))
            .collect()
    }

    pub fn open(mode: GitMode) -> Result<Self, SourceError> {
        let repo = repo_root(Path::new("."))?;
        Ok(GitSource::new(repo, mode))
    }

    pub(crate) fn detached(&self) -> Result<Self, SourceError> {
        let (mode, labels) = match &self.mode {
            GitMode::Range { from, to, group_by } => (
                GitMode::Range {
                    from: resolve_ref(&self.repo, from)?,
                    to: resolve_ref(&self.repo, to)?,
                    group_by: *group_by,
                },
                Some(
                    self.labels
                        .clone()
                        .unwrap_or_else(|| (from.clone(), to.clone())),
                ),
            ),
            GitMode::Worktree => (GitMode::Worktree, None),
            GitMode::Staged => (GitMode::Staged, None),
        };
        let mut candidate = Self::new(self.repo.clone(), mode);
        candidate.ids = self.ids.clone();
        candidate.labels = labels;
        Ok(candidate)
    }

    fn review_worktree(&self) -> Result<(ReviewMeta, Plan), SourceError> {
        let attributes = read_attributes(&self.repo);
        let generated = GeneratedAttributes::parse(&attributes);
        let mut entries = worktree_diff_entries(&self.repo)?;

        let untracked = git_raw(
            &self.repo,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )?;
        for path in split_z(&untracked) {
            let full = self.repo.join(path_from_bytes(path));
            let size = std::fs::metadata(&full).map_or(0, |meta| meta.len());
            let content_skipped = size > UNTRACKED_LIMIT;
            let (add, binary, new_lines) = if content_skipped {
                (0, true, None)
            } else {
                let bytes = std::fs::read(&full).map_err(|source| SourceError::Io {
                    path: full.clone(),
                    source,
                })?;
                if content::is_binary(&bytes) {
                    (0, true, None)
                } else {
                    let lines = content::line_count(&bytes);
                    (lines as u64, false, u32::try_from(lines).ok())
                }
            };
            entries.push(DiffEntry {
                new_lines,
                path: path_from_bytes(path),
                old_path: None,
                status: Status::Add,
                add,
                del: 0,
                binary,
                content_skipped,
            });
        }

        let group_id = "worktree";
        let files = self.build_entries(&entries, &generated, group_id, |entry| {
            select_sides(
                entry,
                |path| SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec("HEAD:", path),
                },
                |path| SideRef::Disk(self.repo.join(path)),
            )
        });

        let title = "Working tree changes";
        Ok((
            single_group(title.to_string(), group_id, title.to_string(), files.0),
            Plan { files: files.1 },
        ))
    }

    fn review_staged(&self) -> Result<(ReviewMeta, Plan), SourceError> {
        let attributes = read_attributes(&self.repo);
        let generated = GeneratedAttributes::parse(&attributes);
        let entries = diff_entries(&self.repo, &["--cached"])?;
        let group_id = "staged";
        let files = self.build_entries(&entries, &generated, group_id, |entry| {
            select_sides(
                entry,
                |path| SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec("HEAD:", path),
                },
                |path| SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec(":", path),
                },
            )
        });

        let title = "Staged changes";
        Ok((
            single_group(title.to_string(), group_id, title.to_string(), files.0),
            Plan { files: files.1 },
        ))
    }

    /// コミットごとの単位。マージ以外の各コミットが 1 グループ。コミットの差分は
    /// 並列に求める（R-UNIT）。
    fn review_per_commit(&self, from: &str, to: &str) -> Result<(ReviewMeta, Plan), SourceError> {
        commit_sha(&self.repo, from)?;
        commit_sha(&self.repo, to)?;
        let attributes = read_attributes(&self.repo);
        let generated = GeneratedAttributes::parse(&attributes);
        let commits = range_messages(&self.repo, &format!("{from}..{to}"))?;
        let shas: Vec<String> = commits.iter().map(|commit| commit.sha.clone()).collect();
        let mut diffs = commit_diff_entries(&self.repo, &shas)?;

        let mut groups = Vec::new();
        let mut plan_files = Vec::new();
        for commit in commits {
            let entries = diffs.remove(&commit.sha).unwrap_or_default();
            let sha = commit.sha.as_str();
            let files = self.build_entries(&entries, &generated, sha, |entry| {
                select_sides(
                    entry,
                    |path| SideRef::Git {
                        repo: self.repo.clone(),
                        spec: git_spec(&format!("{sha}^:"), path),
                    },
                    |path| SideRef::Git {
                        repo: self.repo.clone(),
                        spec: git_spec(&format!("{sha}:"), path),
                    },
                )
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
        // 由来と内容が同じ版を指すよう、範囲の端を sha に解決して使う。
        let from_sha = commit_sha(&self.repo, from)?;
        let to_sha = commit_sha(&self.repo, to)?;
        let attributes = read_attributes(&self.repo);
        let generated = GeneratedAttributes::parse(&attributes);
        let merge_base = git_text(&self.repo, &["merge-base", &from_sha, &to_sha])?
            .trim()
            .to_string();
        let range = format!("{from_sha}...{to_sha}");
        let entries = diff_entries(&self.repo, &[&range])?;
        let group_id = "all";
        let files = self.build_entries(&entries, &generated, group_id, |entry| {
            select_sides(
                entry,
                |path| SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec(&format!("{merge_base}:"), path),
                },
                |path| SideRef::Git {
                    repo: self.repo.clone(),
                    spec: git_spec(&format!("{to_sha}:"), path),
                },
            )
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
            single_group(
                format!("{from}..{to}"),
                group_id,
                format!("{from}...{to}"),
                files.0,
            ),
            Plan { files: files.1 },
        ))
    }

    fn build_entries(
        &self,
        entries: &[DiffEntry],
        generated: &GeneratedAttributes<'_>,
        group_id: &str,
        sides: impl Fn(&DiffEntry) -> (SideRef, SideRef),
    ) -> (Vec<FileEntry>, Vec<PlannedFile>) {
        let mut files = Vec::new();
        let mut planned = Vec::new();
        let prepared: Vec<_> = entries
            .iter()
            .filter(|entry| entry.binary)
            .map(&sides)
            .collect();
        let binary_sides: Vec<_> = prepared.iter().flat_map(|(old, new)| [old, new]).collect();
        let sizes = side_sizes(&binary_sides);
        let mut sizes = sizes.into_iter();
        let mut prepared = prepared.into_iter();
        for entry in entries {
            let (old, new) = if entry.binary {
                prepared
                    .next()
                    .expect("references prepared for each binary entry")
            } else {
                sides(entry)
            };
            let id = self
                .ids
                .lock()
                .expect("file id lock poisoned")
                .get(group_id, &entry.path);
            let (old_size, new_size) = if entry.binary {
                (
                    sizes.next().expect("two sizes per binary entry"),
                    sizes.next().expect("two sizes per binary entry"),
                )
            } else {
                (0, 0)
            };
            // 表示は UTF-8 へ置換してよいが、id と読み取りは元のバイトのまま扱う。
            let display_path = entry.path.to_string_lossy().into_owned();
            let noise = classify(&NoiseInput {
                path: &display_path,
                linguist_generated: generated.matches(&display_path),
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
                content_skipped: entry.content_skipped,
            });
            planned.push(PlannedFile {
                id,
                old,
                new,
                old_lines: None,
                new_lines: entry.new_lines,
            });
        }
        (files, planned)
    }
}

impl ReviewSource for GitSource {
    fn reference_metadata(&self, file_id: &str) -> Option<super::ReferenceMetadata> {
        self.store.reference_metadata(file_id)
    }
    fn select_references(
        &self,
        requested: &[(crate::domain::explanation::ReferenceUnit, String, String)],
    ) -> Result<Vec<crate::domain::explanation::CodeReferenceEntry>, SourceError> {
        use crate::domain::explanation::{CodeReferenceEntry, ReferenceUnit};
        let GitMode::Range { from, to, .. } = &self.mode else {
            return Ok(vec![]);
        };
        let range = format!("{from}..{to}");
        let commits = git_raw(&self.repo, &["rev-list", "--no-merges", &range])?;
        let commits = String::from_utf8_lossy(&commits);
        let groups: std::collections::HashSet<_> = requested
            .iter()
            .map(|(unit, group, _)| (GroupBy::from(*unit), group.clone()))
            .collect();
        let mut result = Vec::new();
        for (unit, group) in groups {
            let entries = match unit {
                GroupBy::File if group == "all" => {
                    let range = format!("{from}...{to}");
                    parse_name_status(&git_raw(
                        &self.repo,
                        &[
                            "diff",
                            NO_ORDER_FILE,
                            "--name-status",
                            "-z",
                            "-M",
                            &range,
                            "--",
                        ],
                    )?)
                }
                GroupBy::Commit if commits.lines().any(|sha| sha == group) => {
                    parse_name_status(&git_raw(
                        &self.repo,
                        &[
                            "diff-tree",
                            "--root",
                            "--no-commit-id",
                            "-r",
                            "--name-status",
                            "-z",
                            "-M",
                            &group,
                            "--",
                        ],
                    )?)
                }
                GroupBy::File | GroupBy::Commit => continue,
            };
            let reference_unit = ReferenceUnit::from(unit);
            for entry in entries {
                let path = entry.path.to_string_lossy().into_owned();
                if requested
                    .iter()
                    .any(|(unit, g, p)| *unit == reference_unit && *g == group && *p == path)
                {
                    result.push(CodeReferenceEntry {
                        unit: Some(reference_unit),
                        group_id: group.clone(),
                        path,
                        old_present: entry.status != Status::Add,
                        new_present: entry.status != Status::Delete,
                        old_lines: None,
                        new_lines: None,
                    });
                }
            }
        }
        Ok(result)
    }
    fn fork(&self) -> Result<Option<Arc<dyn ReviewSource>>, SourceError> {
        Ok(Some(Arc::new(self.detached()?)))
    }
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
        let (mut review, plan) = match unit {
            GroupBy::File => self.review_final(from, to)?,
            GroupBy::Commit => self.review_per_commit(from, to)?,
        };
        self.store.update_unit(Some(unit), plan);
        if let Some((from, to)) = &self.labels {
            review.title = format!("{from}..{to}");
            if unit == GroupBy::File
                && let Some(group) = review.groups.first_mut()
            {
                group.title = format!("{from}...{to}");
            }
        }
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

    fn work_tree(&self) -> Option<WorkTree> {
        match &self.mode {
            GitMode::Worktree => Some(WorkTree::new(self.repo.clone())),
            GitMode::Staged | GitMode::Range { .. } => None,
        }
    }

    fn reads_repository(&self) -> bool {
        true
    }

    /// 版は、そのファイルのその側の内容参照から決める。作業ツリーならディスク、git の
    /// 参照なら同じ接頭辞（`HEAD:`、`:`（インデックス）、`<sha>:`、`<sha>^:`）で読む。
    fn repository_file(
        &self,
        file_id: &str,
        side: Side,
        path: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        // URL は core が正規化したパスから作るが、経路は token を知る誰でも叩けるので、
        // 受け口でも「リポジトリの中の素朴なパス」だけを通す。`..`・絶対パス・`.`・空の
        // 区切り・ドライブ接頭辞は、git を呼ぶ前に無いものとして扱う（git の失敗で
        // レビューを止めない）。
        if !is_plain_repository_path(path) {
            return Ok(None);
        }
        let Some(planned) = self.store.planned(file_id) else {
            return Ok(None);
        };
        let reference = match side {
            Side::Old => planned.old,
            Side::New => planned.new,
        };
        match reference {
            SideRef::Disk(_) => read_worktree_file(&self.repo, Path::new(path), limit),
            SideRef::Git { spec, .. } => {
                let spec = spec.to_string_lossy();
                let Some((revision, _)) = spec.split_once(':') else {
                    return Ok(None);
                };
                read_tree_file(&self.repo, revision, Path::new(path), limit)
            }
            SideRef::Absent | SideRef::Inline(_) => Ok(None),
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
                new_lines: None,
                path: stat.new_path,
                old_path: stat.old_path,
                status,
                add: stat.add,
                del: stat.del,
                binary: stat.binary,
                content_skipped: false,
            }
        })
        .collect())
}

/// worktree の numstat。ファイル数が多いときはパスを分割して並列に引く。
/// パス一覧は内容差分を伴わない plumbing から取る（`diff --name-only HEAD` は
/// 全ファイルの差分判定をやり直すため、1 万ファイルでは約 0.5 秒かかる）。
fn worktree_numstat(repo: &Path) -> Result<Vec<Numstat>, SourceError> {
    let mut paths: Vec<PathBuf> = Vec::new();
    // 未追跡のファイルは `git diff HEAD` に出ないので、ここでは数えない（一覧には
    // review_worktree が足す）。数えると、変更の少ない作業ツリーまで並列の経路に入る。
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
            .map_err(|_| SourceError::Git("parallel numstat failed".to_string()))??;
        entries.append(&mut shard);
    }
    // 1 回の git diff と同じ、パスのバイト列の順に戻す。Path の比較は構成要素ごとなので、
    // `a.rs` と `a/b.rs` の前後が git と逆になる。
    entries.sort_by(|left, right| {
        left.new_path
            .as_os_str()
            .as_encoded_bytes()
            .cmp(right.new_path.as_os_str().as_encoded_bytes())
    });
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
                new_lines: None,
                del: stat.map_or(0, |stat| stat.del),
                binary: stat.is_some_and(|stat| stat.binary),
                content_skipped: false,
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
        .map(|count| count.get().saturating_mul(2))
        .unwrap_or(4)
        .clamp(2, 16)
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
            .map_err(|_| SourceError::Git("parallel diff-tree failed".to_string()))??;
        for (sha, name_status, numstat) in parse_diff_tree(&output) {
            entries.insert(sha, combine_entries(name_status, numstat));
        }
    }
    Ok(entries)
}

/// グループ 1 つのレビュー（worktree・staged・最終形）。
fn single_group(
    title: String,
    group_id: &str,
    group_title: String,
    files: Vec<FileEntry>,
) -> ReviewMeta {
    ReviewMeta {
        title,
        subtitle: String::new(),
        meta: serde_json::Value::Null,
        groups: vec![Group {
            id: group_id.to_string(),
            title: group_title,
            why: String::new(),
            watch: String::new(),
            files,
        }],
        approval: Vec::new(),
    }
}

fn read_attributes(repo: &Path) -> String {
    std::fs::read_to_string(repo.join(".gitattributes")).unwrap_or_default()
}

/// fork で範囲の端を sha に固定する。誤りは git を起動できないときも含めて ref not found。
fn resolve_ref(repo: &Path, revision: &str) -> Result<String, SourceError> {
    commit_sha(repo, revision).map_err(|_| ref_not_found(revision))
}

/// `<revision>^{commit}` を完全な sha に解決する。git を起動できなければ Io、
/// 解決できなければ ref not found。
fn commit_sha(repo: &Path, revision: &str) -> Result<String, SourceError> {
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
        return Err(ref_not_found(revision));
    }
    let sha = String::from_utf8(output.stdout).map_err(|_| ref_not_found(revision))?;
    Ok(sha.trim().to_string())
}

fn ref_not_found(revision: &str) -> SourceError {
    SourceError::Git(format!("ref not found: {revision}"))
}

/// コミット範囲の端を完全な sha に解決する（R-SESSION）。起動時に 1 度だけ呼ぶ。
pub fn resolve_range(repo: &Path, from: &str, to: &str) -> Result<(String, String), SessionError> {
    fn resolve(repo: &Path, revision: &str) -> Result<String, SessionError> {
        let spec = format!("{revision}^{{commit}}");
        git_text(repo, &["rev-parse", "--verify", "--quiet", &spec])
            .map(|sha| sha.trim().to_string())
            .map_err(|error| SessionError::Range {
                revision: revision.to_string(),
                reason: error.to_string(),
            })
    }
    Ok((resolve(repo, from)?, resolve(repo, to)?))
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

#[cfg(test)]
#[path = "git/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "git/watch_tests.rs"]
mod watch_tests;

#[cfg(test)]
#[path = "git/candidate_tests.rs"]
mod candidate_tests;

#[cfg(test)]
#[path = "git/explanation_reference_tests.rs"]
mod explanation_reference_tests;
#[cfg(test)]
#[path = "git/unit_tests.rs"]
mod unit_tests;
