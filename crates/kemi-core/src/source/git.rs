//! git を読む入力ソース（R-INPUT-2〜4）。

mod sizes;
#[cfg(test)]
use sizes::side_size;
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

#[cfg(test)]
#[path = "git/candidate_tests.rs"]
mod candidate_tests;

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

/// `a/b/c.png` の形か。空・`.`・`..`・`\` か `:` を含む区切り（Windows のドライブ接頭辞
/// `C:` はパスの基準を置き換える）と、先頭や末尾の `/` は認めない。
fn is_plain_repository_path(path: &str) -> bool {
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
fn read_worktree_file(
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
fn read_tree_file(
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
            .map_err(|_| SourceError::Git("parallel numstat failed".to_string()))??;
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

/// `path_from_bytes` の逆。運んだパスを git が返したときのバイト列に戻す。
#[cfg(unix)]
pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
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
    let (output, _) = git_with_stdin(repo, args, input).map_err(|source| SourceError::Io {
        path: repo.to_path_buf(),
        source,
    })?;
    if !output.status.success() {
        return Err(SourceError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

/// 標準入力に `input` を書きながら git の終わりを待つ。終了コードの解釈は呼び出し側に
/// 任せ、出力と、入力を書き終えられたかを返す。
pub(crate) fn git_with_stdin(
    repo: &Path,
    args: &[&str],
    input: &[u8],
) -> std::io::Result<(std::process::Output, std::io::Result<()>)> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    std::thread::scope(|scope| {
        // 要求と応答がどちらもパイプ容量を超えうるので、書き込み中も応答を回収する。
        let writer = scope.spawn(move || stdin.write_all(input));
        let output = child.wait_with_output()?;
        let written = writer
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("writing to git stdin panicked")));
        Ok((output, written))
    })
}

/// `git cat-file -s` でオブジェクトの大きさを読む。git が失敗したら Err、出力を数として
/// 読めなければ Ok(None)。
fn object_size(repo: &Path, spec: &OsStr) -> Result<Option<u64>, SourceError> {
    let output = git_raw_os(repo, &[OsStr::new("cat-file"), OsStr::new("-s"), spec])?;
    Ok(std::str::from_utf8(&output)
        .ok()
        .and_then(|text| text.trim().parse().ok()))
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

pub(crate) fn split_z(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect()
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
            "git {} failed: {}",
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
    String::from_utf8(bytes)
        .map_err(|_| SourceError::Git(format!("output of git {} is not UTF-8", args.join(" "))))
}

#[cfg(test)]
#[path = "git/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "git/watch_tests.rs"]
mod watch_tests;

#[cfg(test)]
#[path = "git/explanation_reference_tests.rs"]
mod explanation_reference_tests;
#[cfg(test)]
#[path = "git/unit_tests.rs"]
mod unit_tests;
