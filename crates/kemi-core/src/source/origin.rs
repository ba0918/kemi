//! 由来（R-ORIGIN）の行ごとのコミットを git から求める。
//!
//! - 新側の行: `git blame <from>..<to>` が、その行を最後に変えたコミットを示す。
//!   範囲の外のコミットは boundary になるので、特定できない行として扱う。
//! - 消えた行: `git blame --reverse` が「その行が最後にあったコミット」を示し、その子が
//!   行を消したコミットになる。子がマージのときは、もう片方の親の側で消えたのか、
//!   マージ自体が消したのかを確かめる。`git blame --reverse` は同じ内容の子へ blame を
//!   丸ごと渡すので、横のブランチで消した行が、何もしていないマージのせいに見えるため。

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::domain::content;
use crate::domain::diff;
use crate::domain::origin::{self as origin_domain, LineCommit, RangeCommit};
use crate::source::git::{git_raw_os, git_text, path_from_bytes};
use crate::source::{FileContent, FileOrigin, SourceError};

/// マージのもう片方の親をたどる深さの上限。これを超えた行は特定できないとする。
const MAX_MERGE_DEPTH: u32 = 8;

/// blame の 1 行。`line` と `path` は `sha` の版での位置。
#[derive(Clone, Debug)]
struct BlameLine {
    sha: String,
    line: u32,
    path: PathBuf,
    boundary: bool,
}

/// blame を呼ぶ git。
///
/// Why not 利用者の全体やシステムの設定を外して呼ぶ: 部分クローンで足りない blob を
/// 取り寄せるには、その設定（url.<base>.insteadOf、認証の helper、proxy など）が要り、
/// 他人が所有するリポジトリを信頼する safe.directory もそこにある。
struct BlameGit<'a> {
    repo: &'a Path,
}

impl<'a> BlameGit<'a> {
    fn new(repo: &'a Path) -> Self {
        BlameGit { repo }
    }

    /// `git blame --line-porcelain` を、対象の版の行番号順（`final` の順）に読む。
    /// `--reverse` では対象の版は範囲の始点になる。
    fn blame(
        &self,
        range: &str,
        path: &Path,
        reverse: bool,
    ) -> Result<Vec<Option<BlameLine>>, SourceError> {
        let mut args: Vec<OsString> =
            vec!["-c".into(), "core.quotePath=false".into(), "blame".into()];
        if reverse {
            args.push("--reverse".into());
        }
        args.extend([
            // 設定の blame.ignoreRevsFile で飛ばされたコミットも由来に出すため、無視する
            // 一覧を読む前に取り消す。無いファイルを指す設定も、これで読まずに済む。
            // Why not `--ignore-revs-file=`: 空の指定は設定のファイルを読んだ後で一覧を
            // 空にするので、ファイルが無いと先に失敗する（git 2.43 で確認）。
            OsString::from("--no-ignore-revs-file"),
            // 設定の diff.<driver>.textconv で変換した行で数えると、行番号が表示している
            // 生の内容とずれ、変換が失敗すると blame ごと失敗するため、変換しない。
            OsString::from("--no-textconv"),
            OsString::from("--line-porcelain"),
            OsString::from(range),
            OsString::from("--"),
            path.as_os_str().to_os_string(),
        ]);
        let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
        let output = git_raw_os(self.repo, &args)?;
        Ok(parse_line_porcelain(&output))
    }
}

fn parse_line_porcelain(bytes: &[u8]) -> Vec<Option<BlameLine>> {
    let mut lines: Vec<Option<BlameLine>> = Vec::new();
    let mut header: Option<(String, u32, usize)> = None;
    let mut boundary = false;
    let mut path = PathBuf::new();
    for raw in bytes.split(|byte| *byte == b'\n') {
        if raw.first() == Some(&b'\t') {
            if let Some((sha, line, final_line)) = header.take() {
                if lines.len() < final_line {
                    lines.resize(final_line, None);
                }
                lines[final_line - 1] = Some(BlameLine {
                    sha,
                    line,
                    path: path.clone(),
                    boundary,
                });
            }
            boundary = false;
            continue;
        }
        if header.is_none() {
            let text = String::from_utf8_lossy(raw);
            let mut fields = text.split(' ');
            let (Some(sha), Some(line), Some(final_line)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            if let (Ok(line), Ok(final_line)) = (line.parse(), final_line.parse::<usize>()) {
                if final_line > 0 {
                    header = Some((sha.to_string(), line, final_line));
                }
            }
        } else if raw == b"boundary" {
            boundary = true;
        } else if let Some(name) = raw.strip_prefix(b"filename ") {
            path = path_from_bytes(&unquote(name));
        }
    }
    lines
}

/// git の C 形式の引用（`"a\tb"` や `"\343\201\202"`）を元のバイト列に戻す。
fn unquote(name: &[u8]) -> Vec<u8> {
    let Some(inner) = name
        .strip_prefix(b"\"")
        .and_then(|rest| rest.strip_suffix(b"\""))
    else {
        return name.to_vec();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < inner.len() {
        if inner[index] != b'\\' || index + 1 >= inner.len() {
            bytes.push(inner[index]);
            index += 1;
            continue;
        }
        let escaped = inner[index + 1];
        let octal = inner
            .get(index + 1..index + 4)
            .filter(|digits| digits.iter().all(|digit| (b'0'..=b'7').contains(digit)));
        if let Some(digits) = octal {
            bytes.push(digits.iter().fold(0u8, |value, digit| {
                value.wrapping_mul(8).wrapping_add(digit - b'0')
            }));
            index += 4;
            continue;
        }
        bytes.push(match escaped {
            b'a' => 0x07,
            b'b' => 0x08,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'r' => b'\r',
            other => other,
        });
        index += 2;
    }
    bytes
}

/// 最終形の由来を求めるための範囲。どれも解決済みの sha。
#[derive(Clone, Debug)]
pub(crate) struct OriginRange {
    pub from: String,
    pub to: String,
    pub merge_base: String,
}

/// 最終形の 1 ファイルの左右のパス。旧側はマージ基点、新側は `to` の版のもの。
#[derive(Clone, Debug)]
pub(crate) struct OriginPaths {
    pub old: Option<PathBuf>,
    pub new: Option<PathBuf>,
}

/// 最終形の 1 ファイルの由来を求める。バイナリには由来が無い（None）。
pub(crate) fn file_origin(
    repo: &Path,
    range: &OriginRange,
    paths: &OriginPaths,
    file: &FileContent,
    force: bool,
) -> Result<Option<FileOrigin>, SourceError> {
    let is_binary = |side: &Option<Vec<u8>>| side.as_deref().is_some_and(content::is_binary);
    if is_binary(&file.old) || is_binary(&file.new) {
        return Ok(None);
    }
    let text = |side: &Option<Vec<u8>>| {
        side.as_deref()
            .map(|bytes| content::normalize(&String::from_utf8_lossy(bytes)))
    };
    let (old_text, new_text) = (text(&file.old), text(&file.new));
    if !force && !content::within_auto_limit(old_text.as_deref(), new_text.as_deref()) {
        return Ok(Some(FileOrigin {
            enabled: false,
            blocks: Vec::new(),
            commits: Vec::new(),
        }));
    }
    let old_lines = content::lines(old_text.as_deref().unwrap_or_default());
    let new_lines = content::lines(new_text.as_deref().unwrap_or_default());
    let rows = diff::align(&old_lines, &new_lines);
    let blocks = origin_domain::change_blocks(&rows);

    let needs_new_side = blocks
        .iter()
        .any(|block| rows[block.clone()].iter().any(|row| row.new.is_some()));
    let deleted_lines: Vec<u32> = blocks
        .iter()
        .filter(|block| rows[(*block).clone()].iter().all(|row| row.new.is_none()))
        .flat_map(|block| rows[block.clone()].iter())
        .filter_map(|row| row.old.as_ref().map(|line| line.number))
        .collect();

    let commits = range_commits(repo, &range.from, &range.to)?;
    let git = BlameGit::new(repo);
    let new_side = match (&paths.new, needs_new_side) {
        (Some(path), true) => new_side_commits(&git, &range.from, &range.to, path)?,
        _ => Vec::new(),
    };
    let old_side = match (&paths.old, deleted_lines.is_empty()) {
        (Some(path), false) => {
            deleted_line_commits(&git, &range.merge_base, &range.to, path, &deleted_lines)?
        }
        _ => Vec::new(),
    };
    let blocks = origin_domain::assign_origins(&rows, &new_side, &old_side, &commits);
    let referenced: Vec<RangeCommit> = commits
        .into_iter()
        .filter(|commit| {
            blocks
                .iter()
                .any(|block| block.entries.iter().any(|entry| entry.sha == commit.sha))
        })
        .collect();
    Ok(Some(FileOrigin {
        enabled: true,
        blocks,
        commits: referenced,
    }))
}

/// 範囲内のコミットを新しい順（子が親より先）に返す。
pub(crate) fn range_commits(
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<RangeCommit>, SourceError> {
    let range = format!("{from}..{to}");
    let text = git_text(
        repo,
        &[
            "log",
            // 利用者の log.showSignature で署名の検証結果が sha の前に混ざらないようにする。
            "--no-show-signature",
            "-z",
            "--topo-order",
            "--format=%H%x1f%P%x1f%s%x1f%b",
            &range,
        ],
    )?;
    Ok(text
        .split('\0')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut fields = record.splitn(4, '\x1f');
            let sha = fields.next()?.trim().to_string();
            let parents = fields.next()?.split_whitespace().count();
            Some(RangeCommit {
                sha,
                merge: parents > 1,
                subject: fields.next().unwrap_or_default().trim().to_string(),
                body: fields.next().unwrap_or_default().trim().to_string(),
            })
        })
        .collect())
}

/// 新側の各行を最後に変えたコミット。範囲の外（boundary）の行は空。
fn new_side_commits(
    git: &BlameGit,
    from: &str,
    to: &str,
    path: &Path,
) -> Result<Vec<Vec<LineCommit>>, SourceError> {
    Ok(git
        .blame(&format!("{from}..{to}"), path, false)?
        .into_iter()
        .map(|line| match line {
            Some(line) if !line.boundary => vec![line_commit(&line.sha, &line)],
            _ => Vec::new(),
        })
        .collect())
}

/// 旧側（`start` の版の `path`）の、指定した行を消したコミット。`lines` 以外の行は空。
fn deleted_line_commits(
    git: &BlameGit,
    start: &str,
    tip: &str,
    path: &Path,
    lines: &[u32],
) -> Result<Vec<Vec<LineCommit>>, SourceError> {
    let mut tracer = Tracer::new(git);
    // 最上位の reverse blame の失敗は、レビュー中の git の失敗としてそのまま返す。
    tracer.reverse_blame(start, tip, path)?;
    let mut result: Vec<Vec<LineCommit>> = Vec::new();
    for &line in lines {
        let index = line as usize - 1;
        if result.len() <= index {
            result.resize(index + 1, Vec::new());
        }
        if let Trace::Removed { by, .. } = tracer.trace(start, path, line, tip, 0) {
            result[index] = by;
        }
    }
    Ok(result)
}

fn line_commit(sha: &str, line: &BlameLine) -> LineCommit {
    LineCommit {
        sha: sha.to_string(),
        path: line.path.to_string_lossy().into_owned(),
        line: line.line,
    }
}

enum Trace {
    /// 行が `tip` まで残っている。
    Survives,
    /// 行を消したコミット（`by` が空なら特定できない）。
    Removed { by: Vec<LineCommit> },
}

#[derive(Default)]
struct Graph {
    parents: HashMap<String, Vec<String>>,
    children: HashMap<String, Vec<String>>,
}

struct Tracer<'a> {
    git: &'a BlameGit<'a>,
    reverse: HashMap<(String, String, PathBuf), Vec<Option<BlameLine>>>,
    forward: HashMap<(String, String, PathBuf), Vec<Option<BlameLine>>>,
    graphs: HashMap<(String, String), Graph>,
    merge_bases: HashMap<(String, String), Option<String>>,
}

impl<'a> Tracer<'a> {
    fn new(git: &'a BlameGit<'a>) -> Self {
        Tracer {
            git,
            reverse: HashMap::new(),
            forward: HashMap::new(),
            graphs: HashMap::new(),
            merge_bases: HashMap::new(),
        }
    }

    fn reverse_blame(
        &mut self,
        start: &str,
        tip: &str,
        path: &Path,
    ) -> Result<&[Option<BlameLine>], SourceError> {
        let key = (start.to_string(), tip.to_string(), path.to_path_buf());
        if !self.reverse.contains_key(&key) {
            let lines = self.git.blame(&format!("{start}..{tip}"), path, true)?;
            self.reverse.insert(key.clone(), lines);
        }
        Ok(&self.reverse[&key])
    }

    /// `tip` の版の `line` 行目を、`base` の側から blame した結果。求められなければ None。
    fn forward_line(&mut self, base: &str, tip: &str, path: &Path, line: u32) -> Option<BlameLine> {
        let key = (base.to_string(), tip.to_string(), path.to_path_buf());
        if !self.forward.contains_key(&key) {
            let lines = self
                .git
                .blame(&format!("{base}..{tip}"), path, false)
                .ok()?;
            self.forward.insert(key.clone(), lines);
        }
        self.forward[&key].get(line as usize - 1).cloned().flatten()
    }

    fn graph(&mut self, start: &str, tip: &str) -> &Graph {
        let key = (start.to_string(), tip.to_string());
        let repo = self.git.repo;
        self.graphs.entry(key).or_insert_with(|| {
            let range = format!("{start}..{tip}");
            let mut graph = Graph::default();
            let text = git_text(repo, &["rev-list", "--parents", &range]).unwrap_or_default();
            for line in text.lines() {
                let mut shas = line.split_whitespace().map(str::to_string);
                let Some(commit) = shas.next() else {
                    continue;
                };
                let parents: Vec<String> = shas.collect();
                for parent in &parents {
                    graph
                        .children
                        .entry(parent.clone())
                        .or_default()
                        .push(commit.clone());
                }
                graph.parents.insert(commit, parents);
            }
            graph
        })
    }

    fn merge_base(&mut self, left: &str, right: &str) -> Option<String> {
        let key = (left.to_string(), right.to_string());
        let repo = self.git.repo;
        self.merge_bases
            .entry(key)
            .or_insert_with(|| {
                git_text(repo, &["merge-base", left, right])
                    .ok()
                    .map(|text| text.trim().to_string())
                    .filter(|sha| !sha.is_empty())
            })
            .clone()
    }

    /// `start` の版の `path` の `line` 行目が、`tip` までの間にどうなったか。
    fn trace(&mut self, start: &str, path: &Path, line: u32, tip: &str, depth: u32) -> Trace {
        let unknown = Trace::Removed { by: Vec::new() };
        let last = match self.reverse_blame(start, tip, path) {
            Ok(lines) => match lines.get(line as usize - 1) {
                Some(Some(last)) => last.clone(),
                _ => return unknown,
            },
            Err(_) => return unknown,
        };
        if last.sha == tip {
            return Trace::Survives;
        }
        let graph = self.graph(start, tip);
        let children = graph.children.get(&last.sha).cloned().unwrap_or_default();
        let parents_of: Vec<Vec<String>> = children
            .iter()
            .map(|child| graph.parents.get(child).cloned().unwrap_or_default())
            .collect();

        let mut by = Vec::new();
        for (child, parents) in children.iter().zip(parents_of) {
            if parents.len() <= 1 {
                by.push(line_commit(child, &last));
                continue;
            }
            if depth >= MAX_MERGE_DEPTH {
                continue;
            }
            let mut removed_on_other_side = false;
            for other in parents.iter().filter(|parent| **parent != last.sha) {
                match self.trace_into(&last, other, depth + 1) {
                    None | Some(Trace::Survives) => {}
                    Some(Trace::Removed { by: more }) => {
                        removed_on_other_side = true;
                        by.extend(more);
                    }
                }
            }
            if !removed_on_other_side {
                by.push(line_commit(child, &last));
            }
        }
        Trace::Removed { by }
    }

    /// `last` の行が、マージのもう片方の親 `other` の側でどうなったか。
    /// その側に元から無かった行（マージ基点より後に `last` の側で入った行）は None。
    fn trace_into(&mut self, last: &BlameLine, other: &str, depth: u32) -> Option<Trace> {
        let base = self.merge_base(&last.sha, other)?;
        let (start, path, line) = if base == last.sha {
            (base, last.path.clone(), last.line)
        } else {
            let Some(origin) = self.forward_line(&base, &last.sha, &last.path, last.line) else {
                // たどれないときは、マージが消したと断定せず、特定できない行にする。
                return Some(Trace::Removed { by: Vec::new() });
            };
            if !origin.boundary {
                return None;
            }
            (origin.sha, origin.path, origin.line)
        };
        Some(self.trace(&start, &path, line, other, depth))
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::origin::{BlockOrigin, OriginTarget, Unknown};
    use crate::domain::review::{ReviewMeta, Side};
    use crate::source::git::{GitMode, GitSource, GroupBy};
    use crate::source::testutil::TempRepo;
    use crate::source::{FileOrigin, ReviewSource};

    fn numbered(count: usize) -> Vec<String> {
        (1..=count).map(|n| format!("line {n}")).collect()
    }

    fn text(lines: &[String]) -> String {
        lines.iter().map(|line| format!("{line}\n")).collect()
    }

    fn final_source(repo: &TempRepo, from: &str, to: &str) -> GitSource {
        GitSource::new(
            repo.path.clone(),
            GitMode::Range {
                from: from.to_string(),
                to: to.to_string(),
                group_by: GroupBy::File,
            },
        )
    }

    fn file_id(review: &ReviewMeta, path: &str) -> String {
        review
            .groups
            .iter()
            .flat_map(|group| group.files.iter())
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("file not found: {path}"))
            .id
            .clone()
    }

    fn origin_of(source: &GitSource, path: &str) -> FileOrigin {
        let review = source.review().unwrap();
        source
            .origin(&file_id(&review, path), false)
            .unwrap()
            .expect("final-form file has an origin")
    }

    fn shas(block: &BlockOrigin) -> Vec<&str> {
        block
            .entries
            .iter()
            .map(|entry| entry.sha.as_str())
            .collect()
    }

    #[test]
    fn origin_two_commits_changing_separate_places_each_own_their_block() {
        let repo = TempRepo::new();
        let mut lines = numbered(20);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[2] = "three".to_string();
        repo.write("f.txt", &text(&lines));
        let first = repo.add_and_commit("first");
        lines[14] = "fifteen".to_string();
        repo.write("f.txt", &text(&lines));
        let second = repo.add_and_commit("second");

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        assert!(origin.enabled);
        assert_eq!(origin.blocks.len(), 2);
        assert_eq!(shas(&origin.blocks[0]), vec![first.as_str()]);
        assert_eq!(shas(&origin.blocks[1]), vec![second.as_str()]);
        assert_eq!(origin.blocks[0].unknown, Unknown::None);
        assert_eq!(
            origin.blocks[1].entries[0].target,
            Some(OriginTarget {
                path: "f.txt".to_string(),
                side: Side::New,
                line: 15,
            })
        );
        let subjects: Vec<&str> = origin
            .commits
            .iter()
            .map(|commit| commit.subject.as_str())
            .collect();
        assert!(subjects.contains(&"first") && subjects.contains(&"second"));
    }

    #[test]
    fn origin_block_changed_by_several_commits_lists_all_newest_first() {
        let repo = TempRepo::new();
        let mut lines = numbered(10);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[3] = "four".to_string();
        repo.write("f.txt", &text(&lines));
        let older = repo.add_and_commit("older");
        lines[4] = "five".to_string();
        repo.write("f.txt", &text(&lines));
        let newer = repo.add_and_commit("newer");

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        assert_eq!(origin.blocks.len(), 1);
        assert_eq!(
            shas(&origin.blocks[0]),
            vec![newer.as_str(), older.as_str()]
        );
    }

    #[test]
    fn origin_names_signed_commits_even_when_log_shows_signatures() {
        let repo = TempRepo::new();
        let mut lines = numbered(10);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[3] = "four".to_string();
        repo.write("f.txt", &text(&lines));
        let signed = repo.add_and_commit_signed("signed");
        repo.git(&["config", "log.showSignature", "true"]);

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        assert_eq!(shas(&origin.blocks[0]), vec![signed.as_str()]);
        assert_eq!(origin.blocks[0].unknown, Unknown::None);
        assert_eq!(origin.commits[0].sha, signed);
    }

    #[test]
    fn origin_names_the_last_commit_even_if_blame_is_told_to_ignore_it() {
        let repo = TempRepo::new();
        let mut lines = numbered(10);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[3] = "four".to_string();
        repo.write("f.txt", &text(&lines));
        repo.add_and_commit("first");
        lines[3] = "four again".to_string();
        repo.write("f.txt", &text(&lines));
        let last = repo.add_and_commit("reformat");
        let ignore = repo.path.join(".git").join("ignore-revs");
        std::fs::write(&ignore, format!("{last}\n")).unwrap();
        repo.git(&["config", "blame.ignoreRevsFile", &ignore.to_string_lossy()]);

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        assert_eq!(shas(&origin.blocks[0]), vec![last.as_str()]);
    }

    #[test]
    fn origin_deletion_block_is_the_commit_that_removed_the_lines() {
        let repo = TempRepo::new();
        let mut lines = numbered(12);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[0] = "one".to_string();
        repo.write("f.txt", &text(&lines));
        repo.add_and_commit("unrelated");
        lines.drain(5..7);
        repo.write("f.txt", &text(&lines));
        let deleting = repo.add_and_commit("delete six and seven");

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        let deletion = &origin.blocks[1];
        assert_eq!(shas(deletion), vec![deleting.as_str()]);
        assert_eq!(deletion.unknown, Unknown::None);
        assert_eq!(
            deletion.entries[0].target,
            Some(OriginTarget {
                path: "f.txt".to_string(),
                side: Side::Old,
                line: 6,
            })
        );
    }

    /// 2 本の枝が互いのコミットを取り込んだ（criss-cross）履歴。マージ基点が 2 つあり、
    /// git は新しい方（`q1`）を選ぶ。`p1` は `--from`（`p2`）から辿れるので範囲の外。
    struct CrissCross {
        repo: TempRepo,
        from: String,
        p1: String,
    }

    fn criss_cross(p1_lines: &[String]) -> CrissCross {
        let repo = TempRepo::new();
        let lines = numbered(12);
        repo.write("f.txt", &text(&lines));
        repo.add_and_commit_at("2026-01-01T00:00:00+00:00", "base");
        repo.git(&["branch", "-M", "q"]);
        repo.git(&["checkout", "-q", "-b", "p"]);
        repo.write("f.txt", &text(p1_lines));
        let p1 = repo.add_and_commit_at("2026-01-02T00:00:00+00:00", "p1");
        repo.git(&["checkout", "-q", "q"]);
        repo.write("g.txt", "q\n");
        let q1 = repo.add_and_commit_at("2026-01-03T00:00:00+00:00", "q1");
        repo.git(&["checkout", "-q", "p"]);
        repo.git_at(
            "2026-01-04T00:00:00+00:00",
            &["merge", "-q", "--no-ff", "q", "-m", "p2"],
        );
        let from = repo.head();
        repo.git(&["checkout", "-q", "q"]);
        repo.git_at(
            "2026-01-05T00:00:00+00:00",
            &["merge", "-q", "--no-ff", &p1, "-m", "q2"],
        );
        let chosen = repo.git(&["merge-base", &from, "q"]);
        assert_eq!(chosen, q1, "git must pick the newer merge base");
        CrissCross { repo, from, p1 }
    }

    #[test]
    fn origin_partially_traced_deletion_lists_found_commits_and_partial_unknown() {
        let mut p1_lines = numbered(12);
        p1_lines.remove(5);
        let history = criss_cross(&p1_lines);
        let mut q3_lines = p1_lines.clone();
        q3_lines.remove(5);
        history.repo.write("f.txt", &text(&q3_lines));
        let q3 = history
            .repo
            .add_and_commit_at("2026-01-06T00:00:00+00:00", "q3");

        let origin = origin_of(&final_source(&history.repo, &history.from, "q"), "f.txt");

        assert_eq!(origin.blocks.len(), 1);
        assert_eq!(shas(&origin.blocks[0]), vec![q3.as_str()]);
        assert_eq!(origin.blocks[0].unknown, Unknown::Some);
    }

    #[test]
    fn origin_untraced_deletion_is_unknown_only() {
        let mut p1_lines = numbered(12);
        p1_lines.remove(5);
        let history = criss_cross(&p1_lines);

        let origin = origin_of(&final_source(&history.repo, &history.from, "q"), "f.txt");

        assert_eq!(origin.blocks.len(), 1);
        assert!(origin.blocks[0].entries.is_empty());
        assert_eq!(origin.blocks[0].unknown, Unknown::All);
    }

    #[test]
    fn origin_line_hit_by_no_range_commit_is_unknown_and_outside_commit_not_listed() {
        let mut p1_lines = numbered(12);
        p1_lines[5] = "six from p1".to_string();
        let history = criss_cross(&p1_lines);
        let mut q3_lines = p1_lines.clone();
        q3_lines[4] = "five from q3".to_string();
        history.repo.write("f.txt", &text(&q3_lines));
        let q3 = history
            .repo
            .add_and_commit_at("2026-01-06T00:00:00+00:00", "q3");

        let origin = origin_of(&final_source(&history.repo, &history.from, "q"), "f.txt");

        assert_eq!(origin.blocks.len(), 1);
        assert_eq!(shas(&origin.blocks[0]), vec![q3.as_str()]);
        assert_eq!(origin.blocks[0].unknown, Unknown::Some);
        assert!(!origin.commits.iter().any(|commit| commit.sha == history.p1));
    }

    #[test]
    fn origin_commit_before_range_is_not_listed() {
        let repo = TempRepo::new();
        let mut lines = numbered(12);
        repo.write("f.txt", &text(&lines));
        repo.add_and_commit("base");
        lines[4] = "five before range".to_string();
        repo.write("f.txt", &text(&lines));
        let before = repo.add_and_commit("before range");
        lines[4] = "five in range".to_string();
        lines[5] = "six in range".to_string();
        repo.write("f.txt", &text(&lines));
        let inside = repo.add_and_commit("in range");

        let origin = origin_of(&final_source(&repo, &before, "HEAD"), "f.txt");

        assert_eq!(origin.blocks.len(), 1);
        assert_eq!(shas(&origin.blocks[0]), vec![inside.as_str()]);
        assert_eq!(origin.blocks[0].unknown, Unknown::None);
    }

    /// `main` と `feature` がそれぞれ別の行を変え、マージする。`edit` でマージの
    /// コミットに手を入れる（衝突の解決に相当する）。
    fn merged_history(edit_in_merge: impl Fn(&mut Vec<String>)) -> (TempRepo, String, String) {
        let repo = TempRepo::new();
        let mut lines = numbered(15);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        repo.git(&["branch", "-M", "main"]);
        repo.git(&["checkout", "-q", "-b", "feature"]);
        let mut feature_lines = lines.clone();
        feature_lines[1] = "two on feature".to_string();
        repo.write("f.txt", &text(&feature_lines));
        repo.add_and_commit("feature");
        repo.git(&["checkout", "-q", "main"]);
        lines[13] = "fourteen on main".to_string();
        repo.write("f.txt", &text(&lines));
        repo.add_and_commit("main");
        repo.git(&["merge", "-q", "--no-ff", "--no-commit", "feature"]);
        let mut merged = feature_lines.clone();
        merged[13] = "fourteen on main".to_string();
        edit_in_merge(&mut merged);
        repo.write("f.txt", &text(&merged));
        let merge = repo.add_and_commit("merge feature");
        (repo, base, merge)
    }

    #[test]
    fn origin_lines_changed_by_the_merge_itself_show_the_merge() {
        let (repo, base, merge) = merged_history(|lines| lines[7] = "eight in merge".to_string());

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        assert_eq!(origin.blocks.len(), 3);
        let middle = &origin.blocks[1];
        assert_eq!(shas(middle), vec![merge.as_str()]);
        assert!(middle.entries[0].merge);
        assert_eq!(middle.entries[0].target, None);
        assert!(!origin.blocks[0].entries[0].merge);
    }

    #[test]
    fn origin_lines_deleted_by_the_merge_itself_show_the_merge() {
        let (repo, base, merge) = merged_history(|lines| {
            lines.remove(7);
        });

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        let middle = &origin.blocks[1];
        assert_eq!(shas(middle), vec![merge.as_str()]);
        assert!(middle.entries[0].merge);
        assert_eq!(middle.unknown, Unknown::None);
    }

    #[test]
    fn origin_deletion_on_a_merged_side_branch_is_the_branch_commit() {
        let repo = TempRepo::new();
        let lines = numbered(15);
        repo.write("f.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        repo.git(&["branch", "-M", "main"]);
        repo.git(&["checkout", "-q", "-b", "feature"]);
        let mut feature_lines = lines.clone();
        feature_lines.remove(7);
        repo.write("f.txt", &text(&feature_lines));
        let deleting = repo.add_and_commit("delete eight on feature");
        repo.git(&["checkout", "-q", "main"]);
        repo.write("g.txt", "other\n");
        repo.add_and_commit("main touches another file");
        repo.git(&["merge", "-q", "--no-ff", "feature", "-m", "merge feature"]);

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "f.txt");

        assert_eq!(origin.blocks.len(), 1);
        assert_eq!(shas(&origin.blocks[0]), vec![deleting.as_str()]);
        assert!(!origin.blocks[0].entries[0].merge);
    }

    #[test]
    fn origin_renamed_file_targets_path_and_line_at_that_commit() {
        let repo = TempRepo::new();
        let mut lines = numbered(20);
        repo.write("old.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[9] = "ten".to_string();
        lines.remove(14);
        repo.write("old.txt", &text(&lines));
        let edit = repo.add_and_commit("edit before rename");
        repo.git(&["mv", "old.txt", "new.txt"]);
        let mut renamed = vec!["h1".to_string(), "h2".to_string(), "h3".to_string()];
        renamed.extend(lines.iter().cloned());
        repo.write("new.txt", &text(&renamed));
        let rename = repo.add_and_commit("rename with header");

        let origin = origin_of(&final_source(&repo, &base, "HEAD"), "new.txt");

        assert_eq!(origin.blocks.len(), 3);
        assert_eq!(shas(&origin.blocks[0]), vec![rename.as_str()]);
        assert_eq!(shas(&origin.blocks[1]), vec![edit.as_str()]);
        assert_eq!(
            origin.blocks[1].entries[0].target,
            Some(OriginTarget {
                path: "old.txt".to_string(),
                side: Side::New,
                line: 10,
            })
        );
        assert_eq!(shas(&origin.blocks[2]), vec![edit.as_str()]);
        assert_eq!(
            origin.blocks[2].entries[0].target,
            Some(OriginTarget {
                path: "old.txt".to_string(),
                side: Side::Old,
                line: 15,
            })
        );
    }

    #[test]
    fn origin_large_file_is_off_by_default_and_computed_when_forced() {
        let repo = TempRepo::new();
        let mut lines = numbered(10_001);
        repo.write("big.txt", &text(&lines));
        let base = repo.add_and_commit("base");
        lines[5_000] = "changed".to_string();
        repo.write("big.txt", &text(&lines));
        let change = repo.add_and_commit("change");
        let source = final_source(&repo, &base, "HEAD");
        let review = source.review().unwrap();
        let id = file_id(&review, "big.txt");

        let default = source.origin(&id, false).unwrap().unwrap();
        assert!(!default.enabled);
        assert!(default.blocks.is_empty());

        let forced = source.origin(&id, true).unwrap().unwrap();
        assert!(forced.enabled);
        assert_eq!(shas(&forced.blocks[0]), vec![change.as_str()]);
    }

    #[test]
    fn origin_is_absent_for_commit_groups() {
        let repo = TempRepo::new();
        repo.write("f.txt", "one\n");
        let base = repo.add_and_commit("base");
        repo.write("f.txt", "two\n");
        repo.add_and_commit("change");
        let source = GitSource::new(
            repo.path.clone(),
            GitMode::Range {
                from: base,
                to: "HEAD".to_string(),
                group_by: GroupBy::Commit,
            },
        );
        let review = source.review().unwrap();

        assert_eq!(
            source.origin(&file_id(&review, "f.txt"), false).unwrap(),
            None
        );
    }
}
