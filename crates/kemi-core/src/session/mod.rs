//! セッションの保存と復元（R-SESSION）。
//!
//! 未 submit のレビューを、当時の差分の写しと状態（コメント・見た・折りたたみ・解決・
//! 返信・発言・エージェントとの往復の続き）と一緒にディスクへ残し、`kemi --resume` で
//! 続きから開けるようにする。
//!
//! - 1 セッションは 2 つのファイル。`<id>.session` がセッションの情報と状態、
//!   `<id>.payload` が写し（凍結したレビューのメタデータと変更ファイルの内容）
//!   （`crates/kemi-core/src/session/encoding.rs`）。
//! - 状態は変更のたびに一時ファイルから rename で差し替える。書くのは `<id>.session`
//!   だけで、写しのファイルには触らない（`store.rs`）。
//! - 写しは丸ごと gzip で圧縮し、`<id>.payload` が 20 MB を超えたら捨てて復元不可にする。
//! - 起動中のレビューはロックを取り、`--resume` の二重起動を防ぐ。

mod encoding;
mod store;
mod ulid;

pub use store::{
    COPY_LIMIT, KEEP_BYTES, KEEP_SESSIONS, OpenSession, SessionError, SessionLock, SessionStore,
    StoredSession,
};
pub use ulid::{generate_ulid, is_valid_id, new_ulid, now_millis};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::agent::{
    AgentEvent, Channel, Handed, HandedComment, HandedReply, ReplyRef, Unhanded,
};
use crate::domain::review::{
    Approval, Author, Comment, FileEntry, Group, GroupBy, Message, Reply, ReviewMeta, Side, Status,
    Suggestion,
};
use crate::source::FileContent;

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
    /// 最後に付けたコメントの番号。削除では戻さず、復元しても次の番号から採番する
    /// （R-COMMENT は id を再利用しないと定める）。
    pub last_comment: u32,
    /// レビュー全体への発言。作成順。
    pub messages: Vec<Message>,
    /// 最後に付けた返信と発言の番号。コメントと同じく再利用しない。
    pub last_reply: u32,
    pub last_message: u32,
    /// エージェントとの往復の続き。
    pub channel: Channel,
}

impl SessionState {
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
            && self.seen.is_empty()
            && self.collapsed.is_empty()
            && self.messages.is_empty()
            && self.channel == Channel::default()
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

/// `<id>.session` の中身。セッションの情報と状態だけを持ち、凍結したレビューの
/// メタデータは `<id>.payload` にある（R-SESSION）。
#[derive(Serialize, Deserialize)]
struct MetaDto {
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

/// 版 2 に無い項目は `default` で空として読む。
#[derive(Serialize, Deserialize)]
struct StateDto {
    comments: Vec<CommentDto>,
    seen: BTreeSet<String>,
    collapsed: BTreeMap<String, bool>,
    #[serde(default)]
    last_comment: u32,
    #[serde(default)]
    messages: Vec<MessageDto>,
    #[serde(default)]
    last_reply: u32,
    #[serde(default)]
    last_message: u32,
    #[serde(default)]
    channel: ChannelDto,
}

#[derive(Serialize, Deserialize)]
struct ReplyDto {
    id: String,
    author: String,
    body: String,
}

#[derive(Serialize, Deserialize)]
struct MessageDto {
    id: String,
    author: String,
    body: String,
}

#[derive(Serialize, Deserialize, Default)]
struct ChannelDto {
    called: bool,
    handed_comments: Vec<String>,
    unhanded: UnhandedDto,
    events: Vec<EventDto>,
}

#[derive(Serialize, Deserialize, Default)]
struct UnhandedDto {
    comments: Vec<String>,
    replies: Vec<ReplyRefDto>,
    messages: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ReplyRefDto {
    comment_id: String,
    reply_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EventDto {
    Handed {
        comments: Vec<HandedCommentDto>,
        replies: Vec<HandedReplyDto>,
        messages: Vec<MessageDto>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
enum HandedCommentDto {
    Added { comment: CommentDto },
    Edited { comment: CommentDto },
    Deleted { id: String },
}

#[derive(Serialize, Deserialize)]
struct HandedReplyDto {
    comment_id: String,
    reply: ReplyDto,
}

/// 写しの状態だけ。「使える」写しの中身は `<id>.payload` にある。
#[derive(Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum CopyMetaDto {
    Pending,
    Unusable { reason: String },
    Ready,
}

/// `<id>.payload` の前半（JSON）。凍結したレビューのメタデータ。
#[derive(Serialize, Deserialize)]
struct CopyDto {
    startup_unit: Option<String>,
    units: Vec<UnitDto>,
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
    content_skipped: bool,
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
    replies: Vec<ReplyDto>,
    resolved: bool,
    outdated: bool,
    content_hash: String,
    suggestion: Option<String>,
}

/// `<id>.session` が書いている写しの状態。中身は store が `<id>.payload` から読む。
pub(crate) enum CopyMeta {
    Pending,
    Unusable(String),
    Ready,
}

impl MetaDto {
    pub(crate) fn from_parts(info: &SessionInfo, state: &SessionState, copy: &CopyState) -> Self {
        let copy = match copy {
            CopyState::Pending => CopyMetaDto::Pending,
            CopyState::Unusable(reason) => CopyMetaDto::Unusable {
                reason: reason.clone(),
            },
            CopyState::Ready(_) => CopyMetaDto::Ready,
        };
        MetaDto {
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
            CopyMetaDto::Ready => CopyMeta::Ready,
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
            last_comment: state.last_comment,
            messages: state.messages.iter().map(MessageDto::from).collect(),
            last_reply: state.last_reply,
            last_message: state.last_message,
            channel: ChannelDto::from(&state.channel),
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
            last_comment: self.last_comment,
            messages: self
                .messages
                .into_iter()
                .map(MessageDto::into_message)
                .collect::<Result<Vec<_>, String>>()?,
            last_reply: self.last_reply,
            last_message: self.last_message,
            channel: self.channel.into_channel()?,
        })
    }
}

fn author_dto(author: Author) -> String {
    author.as_str().to_string()
}

fn parse_author(author: &str) -> Result<Author, String> {
    match author {
        "reviewer" => Ok(Author::Reviewer),
        "agent" => Ok(Author::Agent),
        other => Err(format!("unknown author: {other}")),
    }
}

impl From<&Reply> for ReplyDto {
    fn from(reply: &Reply) -> Self {
        ReplyDto {
            id: reply.id.clone(),
            author: author_dto(reply.author),
            body: reply.body.clone(),
        }
    }
}

impl ReplyDto {
    fn into_reply(self) -> Result<Reply, String> {
        Ok(Reply {
            id: self.id,
            author: parse_author(&self.author)?,
            body: self.body,
        })
    }
}

impl From<&Message> for MessageDto {
    fn from(message: &Message) -> Self {
        MessageDto {
            id: message.id.clone(),
            author: author_dto(message.author),
            body: message.body.clone(),
        }
    }
}

impl MessageDto {
    fn into_message(self) -> Result<Message, String> {
        Ok(Message {
            id: self.id,
            author: parse_author(&self.author)?,
            body: self.body,
        })
    }
}

impl From<&Channel> for ChannelDto {
    fn from(channel: &Channel) -> Self {
        ChannelDto {
            called: channel.called,
            handed_comments: channel.handed_comments.clone(),
            unhanded: UnhandedDto {
                comments: channel.unhanded.comments.clone(),
                replies: channel
                    .unhanded
                    .replies
                    .iter()
                    .map(|reference| ReplyRefDto {
                        comment_id: reference.comment_id.clone(),
                        reply_id: reference.reply_id.clone(),
                    })
                    .collect(),
                messages: channel.unhanded.messages.clone(),
            },
            events: channel.events.iter().map(EventDto::from).collect(),
        }
    }
}

impl ChannelDto {
    fn into_channel(self) -> Result<Channel, String> {
        Ok(Channel {
            called: self.called,
            handed_comments: self.handed_comments,
            unhanded: Unhanded {
                comments: self.unhanded.comments,
                replies: self
                    .unhanded
                    .replies
                    .into_iter()
                    .map(|reference| ReplyRef {
                        comment_id: reference.comment_id,
                        reply_id: reference.reply_id,
                    })
                    .collect(),
                messages: self.unhanded.messages,
            },
            events: self
                .events
                .into_iter()
                .map(EventDto::into_event)
                .collect::<Result<Vec<_>, String>>()?,
        })
    }
}

impl From<&AgentEvent> for EventDto {
    fn from(event: &AgentEvent) -> Self {
        match event {
            AgentEvent::Handed(handed) => EventDto::Handed {
                comments: handed
                    .comments
                    .iter()
                    .map(|change| match change {
                        HandedComment::Added(comment) => HandedCommentDto::Added {
                            comment: CommentDto::from(comment),
                        },
                        HandedComment::Edited(comment) => HandedCommentDto::Edited {
                            comment: CommentDto::from(comment),
                        },
                        HandedComment::Deleted(id) => HandedCommentDto::Deleted { id: id.clone() },
                    })
                    .collect(),
                replies: handed
                    .replies
                    .iter()
                    .map(|handed| HandedReplyDto {
                        comment_id: handed.comment_id.clone(),
                        reply: ReplyDto::from(&handed.reply),
                    })
                    .collect(),
                messages: handed.messages.iter().map(MessageDto::from).collect(),
            },
        }
    }
}

impl EventDto {
    fn into_event(self) -> Result<AgentEvent, String> {
        match self {
            EventDto::Handed {
                comments,
                replies,
                messages,
            } => Ok(AgentEvent::Handed(Handed {
                comments: comments
                    .into_iter()
                    .map(|change| {
                        Ok(match change {
                            HandedCommentDto::Added { comment } => {
                                HandedComment::Added(comment.into_comment()?)
                            }
                            HandedCommentDto::Edited { comment } => {
                                HandedComment::Edited(comment.into_comment()?)
                            }
                            HandedCommentDto::Deleted { id } => HandedComment::Deleted(id),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                replies: replies
                    .into_iter()
                    .map(|handed| {
                        Ok(HandedReply {
                            comment_id: handed.comment_id,
                            reply: handed.reply.into_reply()?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                messages: messages
                    .into_iter()
                    .map(MessageDto::into_message)
                    .collect::<Result<Vec<_>, String>>()?,
            })),
        }
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
            replies: comment.replies.iter().map(ReplyDto::from).collect(),
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
            replies: self
                .replies
                .into_iter()
                .map(ReplyDto::into_reply)
                .collect::<Result<Vec<_>, String>>()?,
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
                            content_skipped: file.content_skipped,
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
            content_skipped: self.content_skipped,
        })
    }
}

/// 写しを `<id>.payload` の中身（圧縮前）にする。
pub(crate) fn encode_copy(copy: &SessionCopy) -> Result<Vec<u8>, String> {
    let meta = CopyDto {
        startup_unit: copy.startup_unit.map(GroupBy::as_str).map(str::to_string),
        units: copy
            .units
            .iter()
            .map(|frozen| UnitDto {
                unit: frozen.unit.map(GroupBy::as_str).map(str::to_string),
                review: ReviewDto::from(&frozen.review),
            })
            .collect(),
    };
    let meta = serde_json::to_vec(&meta).map_err(|error| error.to_string())?;
    let mut body = Vec::new();
    encoding::push_copy_meta(&mut body, &meta);
    encoding::encode_contents_into(&mut body, &copy.contents);
    Ok(body)
}

/// `encode_copy` の逆。
pub(crate) fn decode_copy(bytes: &[u8]) -> Result<SessionCopy, String> {
    let (meta, contents) = encoding::decode_copy_body(bytes).map_err(|error| error.to_string())?;
    let meta: CopyDto = serde_json::from_slice(meta).map_err(|error| error.to_string())?;
    let contents = encoding::decode_contents(contents).map_err(|error| error.to_string())?;
    Ok(SessionCopy {
        startup_unit: parse_unit(meta.startup_unit)?,
        units: meta
            .units
            .into_iter()
            .map(|unit| {
                Ok(FrozenUnit {
                    unit: parse_unit(unit.unit)?,
                    review: unit.review.into_parts()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        contents,
    })
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
    /// `<id>.session` の meta を、セッション形式の版に合わせて読む。読み手のある版は
    /// `encoding::is_readable` が決め、ここに来るのはその版だけ。
    pub(crate) fn decode(version: u8, meta: &[u8]) -> Result<Self, String> {
        if version == 2 {
            let mut value: Value =
                serde_json::from_slice(meta).map_err(|error| error.to_string())?;
            upgrade_version_2(&mut value)?;
            return serde_json::from_value(value).map_err(|error| error.to_string());
        }
        serde_json::from_slice(meta).map_err(|error| error.to_string())
    }

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
        matches!(self.copy, CopyMetaDto::Ready)
    }
}

/// 版 2 の状態を版 3 の形に直す。版 2 の返信は本文の文字列の並びで、版 2 の画面からは
/// 返信を書けず API で書けたのも画面の側（人間）だけなので、人間の返信として読む。
/// id はコメントの順・返信の順に振り、採番をその続きから始める。2 に無い項目は
/// `StateDto` の `default` が空にする。
fn upgrade_version_2(meta: &mut Value) -> Result<(), String> {
    let state = meta
        .get_mut("state")
        .ok_or_else(|| "no state".to_string())?;
    let mut last_reply = 0u32;
    if let Some(comments) = state.get_mut("comments").and_then(Value::as_array_mut) {
        for comment in comments {
            let Some(replies) = comment.get_mut("replies").and_then(Value::as_array_mut) else {
                continue;
            };
            for reply in replies.iter_mut() {
                let body = reply
                    .as_str()
                    .ok_or_else(|| "a version 2 reply is not a string".to_string())?
                    .to_string();
                last_reply += 1;
                *reply = serde_json::json!({
                    "id": format!("r{last_reply}"),
                    "author": Author::Reviewer.as_str(),
                    "body": body,
                });
            }
        }
    }
    state["last_reply"] = Value::from(last_reply);
    Ok(())
}
