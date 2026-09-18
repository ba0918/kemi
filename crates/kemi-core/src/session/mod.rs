//! セッションの保存と復元（R-SESSION）。
//!
//! 未 submit のレビューを、当時の差分の写しと状態（コメント・見た・折りたたみ・解決）
//! と一緒にディスクへ残し、`kemi --resume` で続きから開けるようにする。
//!
//! - 形式は版つきの 1 ファイル（`crates/kemi-core/src/session/encoding.rs`）。
//! - 状態は変更のたびに一時ファイルから rename で差し替える（`store.rs`）。
//! - 写しの内容は gzip で圧縮し、1 セッション 20 MB を超えたら捨てて復元不可にする。
//! - 起動中のレビューはロックを取り、`--resume` の二重起動を防ぐ。

mod encoding;
mod store;
mod ulid;

pub use store::{
    OpenSession, SessionError, SessionLock, SessionStore, StoredSession, COPY_LIMIT, KEEP_BYTES,
    KEEP_SESSIONS,
};
pub use ulid::{generate_ulid, is_valid_id, new_ulid, now_millis};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::review::{
    Approval, Comment, FileEntry, Group, GroupBy, ReviewMeta, Side, Status, Suggestion,
};
use crate::source::FileContent;

/// 保存形式の版。読めない版は復元せず、理由を出して終了コード 2 にする。
pub const FORMAT: u32 = 1;

/// コミット範囲の端を完全な sha に解決する（R-SESSION）。起動時に 1 度だけ呼ぶ。
pub fn resolve_range(repo: &Path, from: &str, to: &str) -> Result<(String, String), SessionError> {
    fn resolve(repo: &Path, revision: &str) -> Result<String, SessionError> {
        let spec = format!("{revision}^{{commit}}");
        crate::source::git::git_text(repo, &["rev-parse", "--verify", "--quiet", &spec])
            .map(|sha| sha.trim().to_string())
            .map_err(|error| SessionError::Range {
                revision: revision.to_string(),
                reason: error.to_string(),
            })
    }
    Ok((resolve(repo, from)?, resolve(repo, to)?))
}

/// セッション 1 つの情報。一覧の列と、復元の手がかりになる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
    /// 作成と最終更新の時刻（UNIX エポックからのミリ秒）。
    pub created: u128,
    pub updated: u128,
    /// 元のワークスペース（git のトップ、git の外なら起動ディレクトリ）。
    pub workspace: PathBuf,
    /// 元のワークスペースの識別（R-RESULT と同じ）。
    pub workspace_key: String,
    pub mode: SessionMode,
    /// ページの題。manifest では選択画面のモードの列にも使う。
    pub title: String,
    /// 起動時に開くグループ単位の全ファイル数。
    pub total_files: usize,
}

/// 元の入力モードと範囲。コミット範囲は解決済みの完全な sha を持つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionMode {
    Worktree,
    Staged,
    Manifest,
    Range {
        from: String,
        to: String,
        from_sha: String,
        to_sha: String,
    },
}

impl SessionMode {
    /// 一覧と選択画面に出すモードの表示（R-SESSION）。
    pub fn label(&self, title: &str) -> String {
        match self {
            SessionMode::Worktree => "worktree".to_string(),
            SessionMode::Staged => "staged".to_string(),
            SessionMode::Manifest => title.to_string(),
            SessionMode::Range { from, to, .. } => format!("{from}..{to}"),
        }
    }
}

/// セッション状態。折りたたみはファイルごとの畳みの上書き（R-VIEW）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionState {
    pub comments: Vec<Comment>,
    pub seen: BTreeSet<String>,
    pub collapsed: BTreeMap<String, bool>,
}

impl SessionState {
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty() && self.seen.is_empty() && self.collapsed.is_empty()
    }
}

/// 凍結した 1 グループ単位分のレビュー。
#[derive(Clone, Debug, PartialEq)]
pub struct FrozenUnit {
    /// 起動時の単位は `None`（コミット範囲以外は単位が 1 つだけ）。
    pub unit: Option<GroupBy>,
    pub review: ReviewMeta,
}

/// 復元に使うレビューの写し。両グループ単位のメタデータと、各ファイルの実バイト。
#[derive(Clone, Debug, PartialEq)]
pub struct SessionCopy {
    pub startup_unit: Option<GroupBy>,
    pub units: Vec<FrozenUnit>,
    pub contents: BTreeMap<String, FileContent>,
}

/// 写しの状態。未完成・失敗・上限超過のセッションは一覧と復元の対象にしない。
#[derive(Clone, Debug, PartialEq)]
pub enum CopyState {
    /// まだ凍結が終わっていない。
    Pending,
    Ready(SessionCopy),
    /// 凍結できなかった理由（内容の読み取り失敗、20 MB 超過など）。
    Unusable(String),
}

/// 一覧と選択画面の 1 行。
#[derive(Clone, Debug, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub updated: u128,
    pub workspace: PathBuf,
    pub mode: SessionMode,
    pub title: String,
    pub seen: usize,
    pub total_files: usize,
}

impl SessionSummary {
    pub fn mode_label(&self) -> String {
        self.mode.label(&self.title)
    }
}

// ---- 保存形式の DTO。ドメイン型に serde を付けず、形式の変更をここに閉じる。 ----

#[derive(Serialize, Deserialize)]
struct MetaDto {
    format: u32,
    info: InfoDto,
    state: StateDto,
    copy: CopyMetaDto,
}

#[derive(Serialize, Deserialize)]
struct InfoDto {
    id: String,
    created: u128,
    updated: u128,
    workspace: String,
    workspace_key: String,
    mode: ModeDto,
    title: String,
    total_files: u64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ModeDto {
    Worktree,
    Staged,
    Manifest,
    Range {
        from: String,
        to: String,
        from_sha: String,
        to_sha: String,
    },
}

#[derive(Serialize, Deserialize)]
struct StateDto {
    comments: Vec<CommentDto>,
    seen: BTreeSet<String>,
    collapsed: BTreeMap<String, bool>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum CopyMetaDto {
    Pending,
    Unusable {
        reason: String,
    },
    Ready {
        startup_unit: Option<String>,
        units: Vec<UnitDto>,
        payload_len: u64,
    },
}

#[derive(Serialize, Deserialize)]
struct UnitDto {
    unit: Option<String>,
    review: ReviewDto,
}

#[derive(Serialize, Deserialize)]
struct ReviewDto {
    title: String,
    subtitle: String,
    meta: Value,
    groups: Vec<GroupDto>,
    approval: Vec<ApprovalDto>,
}

#[derive(Serialize, Deserialize)]
struct GroupDto {
    id: String,
    title: String,
    why: String,
    watch: String,
    files: Vec<FileDto>,
}

#[derive(Serialize, Deserialize)]
struct FileDto {
    id: String,
    group_id: String,
    path: String,
    old_path: Option<String>,
    status: String,
    add: u32,
    del: u32,
    binary: bool,
    old_size: u64,
    new_size: u64,
    focus: bool,
    note: String,
    noise: bool,
}

#[derive(Serialize, Deserialize)]
struct ApprovalDto {
    path: String,
    identity: String,
}

#[derive(Serialize, Deserialize)]
struct CommentDto {
    id: String,
    file_id: String,
    group_id: String,
    group_title: String,
    path: String,
    side: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    quote: Vec<String>,
    body: String,
    replies: Vec<String>,
    resolved: bool,
    outdated: bool,
    content_hash: String,
    suggestion: Option<String>,
}

/// payload を読む前の写しの情報。内容は store が payload から組み立てる。
pub(crate) enum CopyMeta {
    Pending,
    Unusable(String),
    Ready {
        startup_unit: Option<GroupBy>,
        units: Vec<FrozenUnit>,
    },
}

impl MetaDto {
    pub(crate) fn from_parts(
        info: &SessionInfo,
        state: &SessionState,
        copy: &CopyState,
        payload_len: u64,
    ) -> Self {
        let copy = match copy {
            CopyState::Pending => CopyMetaDto::Pending,
            CopyState::Unusable(reason) => CopyMetaDto::Unusable {
                reason: reason.clone(),
            },
            CopyState::Ready(copy) => CopyMetaDto::Ready {
                startup_unit: copy.startup_unit.map(GroupBy::as_str).map(str::to_string),
                units: copy
                    .units
                    .iter()
                    .map(|frozen| UnitDto {
                        unit: frozen.unit.map(GroupBy::as_str).map(str::to_string),
                        review: ReviewDto::from(&frozen.review),
                    })
                    .collect(),
                payload_len,
            },
        };
        MetaDto {
            format: FORMAT,
            info: InfoDto {
                id: info.id.clone(),
                created: info.created,
                updated: info.updated,
                workspace: info.workspace.to_string_lossy().into_owned(),
                workspace_key: info.workspace_key.clone(),
                mode: ModeDto::from(&info.mode),
                title: info.title.clone(),
                total_files: info.total_files as u64,
            },
            state: StateDto::from(state),
            copy,
        }
    }

    /// 情報・状態・写しのメタデータに分ける。
    pub(crate) fn into_parts(self) -> Result<(SessionInfo, SessionState, CopyMeta), String> {
        let info = SessionInfo {
            id: self.info.id,
            created: self.info.created,
            updated: self.info.updated,
            workspace: PathBuf::from(self.info.workspace),
            workspace_key: self.info.workspace_key,
            mode: self.info.mode.into(),
            title: self.info.title,
            total_files: self.info.total_files as usize,
        };
        let state = self.state.into_parts()?;
        let copy = match self.copy {
            CopyMetaDto::Pending => CopyMeta::Pending,
            CopyMetaDto::Unusable { reason } => CopyMeta::Unusable(reason),
            CopyMetaDto::Ready {
                startup_unit,
                units,
                ..
            } => CopyMeta::Ready {
                startup_unit: parse_unit(startup_unit)?,
                units: units
                    .into_iter()
                    .map(|unit| {
                        Ok(FrozenUnit {
                            unit: parse_unit(unit.unit)?,
                            review: unit.review.into_parts()?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            },
        };
        Ok((info, state, copy))
    }
}

impl From<&SessionMode> for ModeDto {
    fn from(mode: &SessionMode) -> Self {
        match mode {
            SessionMode::Worktree => ModeDto::Worktree,
            SessionMode::Staged => ModeDto::Staged,
            SessionMode::Manifest => ModeDto::Manifest,
            SessionMode::Range {
                from,
                to,
                from_sha,
                to_sha,
            } => ModeDto::Range {
                from: from.clone(),
                to: to.clone(),
                from_sha: from_sha.clone(),
                to_sha: to_sha.clone(),
            },
        }
    }
}

impl From<ModeDto> for SessionMode {
    fn from(mode: ModeDto) -> Self {
        match mode {
            ModeDto::Worktree => SessionMode::Worktree,
            ModeDto::Staged => SessionMode::Staged,
            ModeDto::Manifest => SessionMode::Manifest,
            ModeDto::Range {
                from,
                to,
                from_sha,
                to_sha,
            } => SessionMode::Range {
                from,
                to,
                from_sha,
                to_sha,
            },
        }
    }
}

impl From<&SessionState> for StateDto {
    fn from(state: &SessionState) -> Self {
        StateDto {
            comments: state.comments.iter().map(CommentDto::from).collect(),
            seen: state.seen.clone(),
            collapsed: state.collapsed.clone(),
        }
    }
}

impl StateDto {
    fn into_parts(self) -> Result<SessionState, String> {
        Ok(SessionState {
            comments: self
                .comments
                .into_iter()
                .map(CommentDto::into_comment)
                .collect::<Result<Vec<_>, String>>()?,
            seen: self.seen,
            collapsed: self.collapsed,
        })
    }
}

impl From<&Comment> for CommentDto {
    fn from(comment: &Comment) -> Self {
        CommentDto {
            id: comment.id.clone(),
            file_id: comment.file_id.clone(),
            group_id: comment.group_id.clone(),
            group_title: comment.group_title.clone(),
            path: comment.path.clone(),
            side: comment.side.as_str().to_string(),
            start_line: comment.start_line,
            end_line: comment.end_line,
            quote: comment.quote.clone(),
            body: comment.body.clone(),
            replies: comment.replies.clone(),
            resolved: comment.resolved,
            outdated: comment.outdated,
            content_hash: comment.content_hash.clone(),
            suggestion: comment
                .suggestion
                .as_ref()
                .map(|suggestion| suggestion.replacement.clone()),
        }
    }
}

impl CommentDto {
    fn into_comment(self) -> Result<Comment, String> {
        let side = match self.side.as_str() {
            "new" => Side::New,
            "old" => Side::Old,
            other => return Err(format!("unknown side: {other}")),
        };
        Ok(Comment {
            id: self.id,
            file_id: self.file_id,
            group_id: self.group_id,
            group_title: self.group_title,
            path: self.path,
            side,
            start_line: self.start_line,
            end_line: self.end_line,
            quote: self.quote,
            body: self.body,
            replies: self.replies,
            resolved: self.resolved,
            outdated: self.outdated,
            content_hash: self.content_hash,
            suggestion: self
                .suggestion
                .map(|replacement| Suggestion { replacement }),
        })
    }
}

impl From<&ReviewMeta> for ReviewDto {
    fn from(review: &ReviewMeta) -> Self {
        ReviewDto {
            title: review.title.clone(),
            subtitle: review.subtitle.clone(),
            meta: review.meta.clone(),
            groups: review
                .groups
                .iter()
                .map(|group| GroupDto {
                    id: group.id.clone(),
                    title: group.title.clone(),
                    why: group.why.clone(),
                    watch: group.watch.clone(),
                    files: group
                        .files
                        .iter()
                        .map(|file| FileDto {
                            id: file.id.clone(),
                            group_id: file.group_id.clone(),
                            path: file.path.clone(),
                            old_path: file.old_path.clone(),
                            status: file.status.as_str().to_string(),
                            add: file.add,
                            del: file.del,
                            binary: file.binary,
                            old_size: file.old_size,
                            new_size: file.new_size,
                            focus: file.focus,
                            note: file.note.clone(),
                            noise: file.noise,
                        })
                        .collect(),
                })
                .collect(),
            approval: review
                .approval
                .iter()
                .map(|approval| ApprovalDto {
                    path: approval.path.clone(),
                    identity: approval.identity.clone(),
                })
                .collect(),
        }
    }
}

impl ReviewDto {
    fn into_parts(self) -> Result<ReviewMeta, String> {
        Ok(ReviewMeta {
            title: self.title,
            subtitle: self.subtitle,
            meta: self.meta,
            groups: self
                .groups
                .into_iter()
                .map(|group| {
                    Ok(Group {
                        id: group.id,
                        title: group.title,
                        why: group.why,
                        watch: group.watch,
                        files: group
                            .files
                            .into_iter()
                            .map(FileDto::into_entry)
                            .collect::<Result<Vec<_>, String>>()?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            approval: self
                .approval
                .into_iter()
                .map(|approval| Approval {
                    path: approval.path,
                    identity: approval.identity,
                })
                .collect(),
        })
    }
}

impl FileDto {
    fn into_entry(self) -> Result<FileEntry, String> {
        let status = match self.status.as_str() {
            "add" => Status::Add,
            "delete" => Status::Delete,
            "rename" => Status::Rename,
            "modify" => Status::Modify,
            other => return Err(format!("unknown status: {other}")),
        };
        Ok(FileEntry {
            id: self.id,
            group_id: self.group_id,
            path: self.path,
            old_path: self.old_path,
            status,
            add: self.add,
            del: self.del,
            binary: self.binary,
            old_size: self.old_size,
            new_size: self.new_size,
            focus: self.focus,
            note: self.note,
            noise: self.noise,
        })
    }
}

fn parse_unit(unit: Option<String>) -> Result<Option<GroupBy>, String> {
    match unit.as_deref() {
        None => Ok(None),
        Some("file") => Ok(Some(GroupBy::File)),
        Some("commit") => Ok(Some(GroupBy::Commit)),
        Some(other) => Err(format!("unknown group unit: {other}")),
    }
}

impl MetaDto {
    /// info と状態だけを取り出す（一覧のための軽い読み）。
    pub(crate) fn summary(&self) -> (SessionInfo, usize) {
        let info = SessionInfo {
            id: self.info.id.clone(),
            created: self.info.created,
            updated: self.info.updated,
            workspace: PathBuf::from(&self.info.workspace),
            workspace_key: self.info.workspace_key.clone(),
            mode: self.info.mode.clone().into(),
            title: self.info.title.clone(),
            total_files: self.info.total_files as usize,
        };
        (info, self.state.seen.len())
    }

    pub(crate) fn is_resumable(&self) -> bool {
        matches!(self.copy, CopyMetaDto::Ready { .. })
    }
}
