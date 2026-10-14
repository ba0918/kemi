//! 保存形式の DTO と旧版の読み替え、写しの符号化（R-SESSION）。ドメイン型に serde を付けず、保存形式の変更をここに閉じ込める。

mod legacy;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    CopyState, FrozenUnit, SessionCopy, SessionInfo, SessionMode, SessionState, encoding,
    validate_additional,
};
use crate::domain::agent::{
    AgentEvent, Channel, Handed, HandedComment, HandedReply, ReplyRef, Unhanded,
};
use crate::domain::conversation_content::ConversationContent;
use crate::domain::display_version::{DisplayVersions, known_generation};
use crate::domain::explanation::{ExplanationHistory, canonical_size, validate_version};
use crate::domain::live::LivePage;
use crate::domain::parts::Part;
use crate::domain::parts_validation::validate_parts;
use crate::domain::review::{
    Approval, Author, Comment, CommentTarget, ExplanationTarget, FileEntry, FileTarget, Group,
    GroupBy, Message, PageTarget, Place, PlaceElement, PlaceKind, Point, Rect, Reply, ReviewMeta,
    Side, Status, Suggestion,
};

pub fn additional_saved_size(state: &SessionState) -> usize {
    fn saved_bytes(value: impl Serialize) -> usize {
        canonical_size(&serde_json::to_value(value).expect("session DTO contains JSON values"))
    }
    fn new_comment(comment: &Comment) -> bool {
        matches!(comment.target, CommentTarget::Explanation(_))
            || comment
                .replies
                .iter()
                .any(|reply| reply.body.parts().is_some())
    }
    fn comment_bytes(comment: &Comment) -> usize {
        if matches!(comment.target, CommentTarget::Explanation(_)) {
            saved_bytes(CommentDto::from(comment))
        } else {
            comment
                .replies
                .iter()
                .filter(|reply| reply.body.parts().is_some())
                .map(|reply| saved_bytes(ReplyDto::from(reply)))
                .sum()
        }
    }
    // 容量と保存形式がずれないよう、null と省略項目を保存 DTO に任せる。
    let mut bytes = state.explanation.as_ref().map_or(0, saved_bytes);
    for comment in &state.comments {
        bytes = bytes.saturating_add(comment_bytes(comment));
    }
    for message in state
        .messages
        .iter()
        .filter(|message| message.body.parts().is_some())
    {
        bytes = bytes.saturating_add(saved_bytes(MessageDto::from(message)));
    }
    for event in &state.channel.events {
        let AgentEvent::Handed(handed) = event;
        let new = handed.comments.iter().any(|change| match change {
            HandedComment::Added(comment) | HandedComment::Edited(comment) => new_comment(comment),
            HandedComment::Deleted(_) => false,
        }) || handed
            .replies
            .iter()
            .any(|reply| reply.reply.body.parts().is_some())
            || handed
                .messages
                .iter()
                .any(|message| message.body.parts().is_some());
        if new {
            bytes = bytes.saturating_add(saved_bytes(EventDto::from(event)));
        }
    }
    bytes
}

// ---- 保存形式の DTO。ドメイン型に serde を付けず、形式の変更をここに閉じる。 ----

/// `<id>.session` の中身。セッションの情報と状態だけを持ち、凍結したレビューの
/// メタデータは `<id>.payload` にある（R-SESSION）。
#[derive(Serialize, Deserialize)]
pub(super) struct MetaDto<'a> {
    info: InfoDto,
    state: StateDto<'a>,
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
    Live {
        page: PageDto,
        root: String,
    },
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
enum PageDto {
    Url(String),
    File(String),
}

/// 版 2 に無い項目は `default` で空として読む。
#[derive(Serialize, Deserialize)]
struct StateDto<'a> {
    #[serde(default)]
    diff_display_version: Option<String>,
    #[serde(default)]
    frozen_display_version: Option<String>,
    #[serde(default)]
    issued_display_generation: Option<String>,
    #[serde(default)]
    explanation: Option<Cow<'a, ExplanationHistory>>,
    comments: Vec<CommentDto>,
    seen: Cow<'a, BTreeSet<String>>,
    collapsed: Cow<'a, BTreeMap<String, bool>>,
    #[serde(default)]
    last_comment: u32,
    #[serde(default)]
    messages: Vec<MessageDto>,
    #[serde(default)]
    last_reply: u32,
    #[serde(default)]
    last_message: u32,
    #[serde(default)]
    last_seq: u32,
    #[serde(default)]
    channel: ChannelDto,
    /// 割り当てが無ければ書かない（`--live` の前からある形のまま）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    mocks: Cow<'a, BTreeMap<String, String>>,
    /// 立っていなければ書かない（印を足す前の形のまま）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    page_comment_saved: bool,
}

#[derive(Serialize, Deserialize)]
struct ReplyDto {
    id: String,
    /// 版 2 と、通し番号を足す前の版 3 には無い。
    #[serde(default)]
    seq: Option<u32>,
    author: String,
    #[serde(deserialize_with = "saved_nullable")]
    body: Option<String>,
    #[serde(deserialize_with = "saved_nullable")]
    parts: Option<Vec<Part>>,
}

#[derive(Serialize, Deserialize)]
struct MessageDto {
    id: String,
    #[serde(default)]
    seq: Option<u32>,
    author: String,
    #[serde(deserialize_with = "saved_nullable")]
    body: Option<String>,
    #[serde(deserialize_with = "saved_nullable")]
    parts: Option<Vec<Part>>,
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

/// ファイルへのコメントはファイルの位置の項目を持ち、ページへのコメントはそれらを持たずに
/// `page` を持つ（ファイルへのコメントの形は今までと同じ）。
#[derive(Serialize, Deserialize)]
struct CommentDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    explanation: Option<ExplanationTarget>,
    id: String,
    #[serde(default)]
    seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_id: Option<String>,
    group_id: String,
    group_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    side: Option<String>,
    #[serde(default)]
    start_line: Option<u32>,
    #[serde(default)]
    end_line: Option<u32>,
    #[serde(default)]
    quote: Vec<String>,
    body: String,
    replies: Vec<ReplyDto>,
    resolved: bool,
    outdated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(default)]
    suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page: Option<CommentPageDto>,
}

#[derive(Serialize, Deserialize)]
struct CommentPageDto {
    url: String,
    width: u32,
    places: Vec<PlaceDto>,
    image: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PlaceDto {
    n: u32,
    kind: String,
    points: Vec<PointDto>,
    elements: Vec<PlaceElementDto>,
}

#[derive(Serialize, Deserialize)]
struct PointDto {
    x: f64,
    y: f64,
}

#[derive(Serialize, Deserialize)]
struct PlaceElementDto {
    selector: String,
    text: String,
    rect: RectDto,
}

#[derive(Serialize, Deserialize)]
struct RectDto {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// `<id>.session` が書いている写しの状態。中身は store が `<id>.payload` から読む。
pub(super) enum CopyMeta {
    Pending,
    Unusable(String),
    Ready,
}

impl<'a> MetaDto<'a> {
    pub(super) fn from_parts(
        info: &SessionInfo,
        state: &'a SessionState,
        copy: &CopyState,
    ) -> Self {
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
    pub(super) fn into_parts(self) -> Result<(SessionInfo, SessionState, CopyMeta), String> {
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
        let mut state = self.state.into_parts()?;
        if let Some(display) = &state.display {
            display.validate(&info.id).map_err(|e| e.to_string())?;
        }
        if let Some(history) = &state.explanation {
            if history.current().is_none() {
                return Err("explanation.current_revision: current version not retained".into());
            }
            let mut revisions = BTreeSet::new();
            for version in &history.versions {
                validate_version(version).map_err(|e| e.to_string())?;
                if !revisions.insert(version.revision) {
                    return Err("explanation.versions: duplicate revision".into());
                }
                if let Some(display) = &state.display {
                    known_generation(&version.base_display_version, &info.id, display.issued)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        validate_additional(&state).map_err(|e| e.to_string())?;
        state.refresh_explanation_comments();
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
            SessionMode::Live { page, root } => ModeDto::Live {
                page: match page {
                    LivePage::Url(url) => PageDto::Url(url.clone()),
                    LivePage::File(path) => PageDto::File(path.clone()),
                },
                root: root.to_string_lossy().into_owned(),
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
            ModeDto::Live { page, root } => SessionMode::Live {
                page: match page {
                    PageDto::Url(url) => LivePage::Url(url),
                    PageDto::File(path) => LivePage::File(path),
                },
                root: PathBuf::from(root),
            },
        }
    }
}

impl<'a> From<&'a SessionState> for StateDto<'a> {
    fn from(state: &'a SessionState) -> Self {
        StateDto {
            diff_display_version: state.display.as_ref().map(|d| d.current.clone()),
            frozen_display_version: state.display.as_ref().and_then(|d| d.frozen.clone()),
            issued_display_generation: state.display.as_ref().map(|d| d.issued.to_string()),
            explanation: state.explanation.as_ref().map(Cow::Borrowed),
            comments: state.comments.iter().map(CommentDto::from).collect(),
            seen: Cow::Borrowed(&state.seen),
            collapsed: Cow::Borrowed(&state.collapsed),
            last_comment: state.last_comment,
            messages: state.messages.iter().map(MessageDto::from).collect(),
            last_reply: state.last_reply,
            last_message: state.last_message,
            last_seq: state.last_seq,
            channel: ChannelDto::from(&state.channel),
            mocks: Cow::Borrowed(&state.mocks),
            page_comment_saved: state.page_comment_saved,
        }
    }
}

impl StateDto<'_> {
    fn into_parts(self) -> Result<SessionState, String> {
        let unnumbered = self.comments.iter().any(|comment| {
            comment.seq.is_none() || comment.replies.iter().any(|reply| reply.seq.is_none())
        }) || self.messages.iter().any(|message| message.seq.is_none());
        let mut state = SessionState {
            display: match (self.diff_display_version, self.issued_display_generation) {
                (Some(current), Some(issued)) => {
                    if issued.starts_with('0') || !issued.bytes().all(|b| b.is_ascii_digit()) {
                        return Err("issued_display_generation: invalid decimal generation".into());
                    }
                    Some(DisplayVersions {
                        current,
                        frozen: self.frozen_display_version,
                        issued: issued
                            .parse()
                            .map_err(|_| "issued_display_generation: outside u128")?,
                    })
                }
                (None, None) => None,
                _ => return Err("diff_display_version: incomplete generation state".into()),
            },
            explanation: self.explanation.map(Cow::into_owned),
            comments: self
                .comments
                .into_iter()
                .map(CommentDto::into_comment)
                .collect::<Result<Vec<_>, String>>()?,
            seen: self.seen.into_owned(),
            collapsed: self.collapsed.into_owned(),
            last_comment: self.last_comment,
            messages: self
                .messages
                .into_iter()
                .map(MessageDto::into_message)
                .collect::<Result<Vec<_>, String>>()?,
            last_reply: self.last_reply,
            last_message: self.last_message,
            last_seq: self.last_seq,
            channel: self.channel.into_channel()?,
            mocks: self.mocks.into_owned(),
            page_comment_saved: self.page_comment_saved,
        };
        if unnumbered {
            number_in_creation_order(&mut state);
        }
        Ok(state)
    }
}

/// 通し番号の無い状態に、コメントを作成順に並べ、各コメントのすぐ後にその返信を作成順に
/// 置いた順で番号を振る（R-SESSION の版 2 の読み方）。版 2 には発言が無い。通し番号を
/// 足す前の版 3 の発言は、どこに挟まったかが分からないので、コメントと返信の後ろに置く。
fn number_in_creation_order(state: &mut SessionState) {
    let mut seq = 0;
    let mut next = || {
        seq += 1;
        seq
    };
    for comment in &mut state.comments {
        comment.seq = next();
        for reply in &mut comment.replies {
            reply.seq = next();
        }
    }
    for message in &mut state.messages {
        message.seq = next();
    }
    state.last_seq = seq;
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

fn stored_content(
    body: Option<String>,
    parts: Option<Vec<Part>>,
) -> Result<ConversationContent, String> {
    match (body, parts) {
        (Some(body), None) => Ok(ConversationContent::Plain(body)),
        (None, Some(parts)) => {
            validate_parts(&parts).map_err(|e| e.to_string())?;
            Ok(ConversationContent::Parts(parts))
        }
        _ => Err("body/parts: stored content must contain exactly one non-null value".into()),
    }
}

fn saved_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

impl From<&Reply> for ReplyDto {
    fn from(reply: &Reply) -> Self {
        ReplyDto {
            id: reply.id.clone(),
            seq: Some(reply.seq),
            author: author_dto(reply.author),
            body: reply.body.plain().map(str::to_owned),
            parts: reply.body.parts().map(<[Part]>::to_vec),
        }
    }
}

impl ReplyDto {
    fn into_reply(self) -> Result<Reply, String> {
        Ok(Reply {
            id: self.id,
            seq: self.seq.unwrap_or_default(),
            author: parse_author(&self.author)?,
            body: stored_content(self.body, self.parts)?,
        })
    }
}

impl From<&Message> for MessageDto {
    fn from(message: &Message) -> Self {
        MessageDto {
            id: message.id.clone(),
            seq: Some(message.seq),
            author: author_dto(message.author),
            body: message.body.plain().map(str::to_owned),
            parts: message.body.parts().map(<[Part]>::to_vec),
        }
    }
}

impl MessageDto {
    fn into_message(self) -> Result<Message, String> {
        Ok(Message {
            id: self.id,
            seq: self.seq.unwrap_or_default(),
            author: parse_author(&self.author)?,
            body: stored_content(self.body, self.parts)?,
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
        let mut dto = CommentDto {
            explanation: None,
            id: comment.id.clone(),
            seq: Some(comment.seq),
            file_id: None,
            group_id: comment.group_id.clone(),
            group_title: comment.group_title.clone(),
            path: None,
            side: None,
            start_line: None,
            end_line: None,
            quote: Vec::new(),
            body: comment.body.clone(),
            replies: comment.replies.iter().map(ReplyDto::from).collect(),
            resolved: comment.resolved,
            outdated: comment.outdated,
            content_hash: None,
            suggestion: None,
            page: None,
        };
        match &comment.target {
            CommentTarget::File(file) => {
                dto.file_id = Some(file.file_id.clone());
                dto.path = Some(file.path.clone());
                dto.side = Some(file.side.as_str().to_string());
                dto.start_line = file.start_line;
                dto.end_line = file.end_line;
                dto.quote = file.quote.clone();
                dto.content_hash = Some(file.content_hash.clone());
                dto.suggestion = file
                    .suggestion
                    .as_ref()
                    .map(|suggestion| suggestion.replacement.clone());
            }
            CommentTarget::Page(page) => dto.page = Some(CommentPageDto::from(page)),
            CommentTarget::Explanation(target) => dto.explanation = Some(target.clone()),
        }
        dto
    }
}

impl CommentDto {
    fn into_comment(self) -> Result<Comment, String> {
        let target = if let Some(target) = self.explanation {
            crate::domain::parts_validation::id("explanation.block_id", &target.block_id)
                .map_err(|e| e.to_string())?;
            if target.revision == 0
                || target.revision > crate::domain::explanation::MAX_REVISION
                || target.quote.block.id != target.block_id
            {
                return Err("explanation: invalid original target".into());
            }
            crate::domain::explanation::validate_document(
                &crate::domain::explanation::ExplanationDocument {
                    sections: vec![crate::domain::explanation::ExplanationSection {
                        id: "quote".into(),
                        title: target.quote.section_title.clone(),
                        blocks: vec![target.quote.block.clone()],
                    }],
                },
            )
            .map_err(|e| e.to_string())?;
            if let Some(selection) = &target.selection {
                crate::domain::explanation::validate_selection(&target.quote.block.part, selection)
                    .map_err(|e| e.to_string())?;
            }
            CommentTarget::Explanation(target)
        } else {
            match self.page {
                Some(page) => CommentTarget::Page(page.into_target()?),
                None => {
                    let side = match self.side.as_deref() {
                        Some("new") => Side::New,
                        Some("old") => Side::Old,
                        Some(other) => return Err(format!("unknown side: {other}")),
                        None => return Err("a comment has neither a file nor a page".to_string()),
                    };
                    CommentTarget::File(FileTarget {
                        file_id: self.file_id.unwrap_or_default(),
                        path: self.path.unwrap_or_default(),
                        side,
                        start_line: self.start_line,
                        end_line: self.end_line,
                        quote: self.quote,
                        content_hash: self.content_hash.unwrap_or_default(),
                        suggestion: self
                            .suggestion
                            .map(|replacement| Suggestion { replacement }),
                    })
                }
            }
        };
        Ok(Comment {
            id: self.id,
            seq: self.seq.unwrap_or_default(),
            group_id: self.group_id,
            group_title: self.group_title,
            body: self.body,
            replies: self
                .replies
                .into_iter()
                .map(ReplyDto::into_reply)
                .collect::<Result<Vec<_>, String>>()?,
            resolved: self.resolved,
            outdated: self.outdated,
            target,
        })
    }
}

impl From<&PageTarget> for CommentPageDto {
    fn from(page: &PageTarget) -> Self {
        CommentPageDto {
            url: page.url.clone(),
            width: page.width,
            places: page
                .places
                .iter()
                .map(|place| PlaceDto {
                    n: place.n,
                    kind: place.kind.as_str().to_string(),
                    points: place
                        .points
                        .iter()
                        .map(|point| PointDto {
                            x: point.x,
                            y: point.y,
                        })
                        .collect(),
                    elements: place
                        .elements
                        .iter()
                        .map(|element| PlaceElementDto {
                            selector: element.selector.clone(),
                            text: element.text.clone(),
                            rect: RectDto {
                                x: element.rect.x,
                                y: element.rect.y,
                                w: element.rect.w,
                                h: element.rect.h,
                            },
                        })
                        .collect(),
                })
                .collect(),
            image: page.image.clone(),
        }
    }
}

impl CommentPageDto {
    fn into_target(self) -> Result<PageTarget, String> {
        let places = self
            .places
            .into_iter()
            .map(|place| {
                let kind = PlaceKind::parse(&place.kind)
                    .ok_or_else(|| format!("unknown place kind: {}", place.kind))?;
                Ok(Place {
                    n: place.n,
                    kind,
                    points: place
                        .points
                        .into_iter()
                        .map(|point| Point {
                            x: point.x,
                            y: point.y,
                        })
                        .collect(),
                    elements: place
                        .elements
                        .into_iter()
                        .map(|element| PlaceElement {
                            selector: element.selector,
                            text: element.text,
                            rect: Rect {
                                x: element.rect.x,
                                y: element.rect.y,
                                w: element.rect.w,
                                h: element.rect.h,
                            },
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(PageTarget {
            url: self.url,
            width: self.width,
            places,
            image: self.image,
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
pub(super) fn encode_copy(copy: &SessionCopy) -> Result<Vec<u8>, String> {
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
pub(super) fn decode_copy(bytes: &[u8]) -> Result<SessionCopy, String> {
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

impl MetaDto<'_> {
    /// `<id>.session` の meta を、セッション形式の版に合わせて読む。読み手のある版は
    /// `encoding::is_readable` が決め、ここに来るのはその版だけ。
    pub(super) fn decode(version: u8, meta: &[u8]) -> Result<Self, String> {
        if version == 2 || version == 3 {
            let mut value: Value =
                serde_json::from_slice(meta).map_err(|error| error.to_string())?;
            legacy::upgrade(version, &mut value)?;
            return serde_json::from_value(value).map_err(|error| error.to_string());
        }
        serde_json::from_slice(meta).map_err(|error| error.to_string())
    }

    /// info と状態だけを取り出す（一覧のための軽い読み）。
    pub(super) fn summary(&self) -> (SessionInfo, usize) {
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

    pub(super) fn is_resumable(&self) -> bool {
        matches!(self.info.mode, ModeDto::Live { .. }) || matches!(self.copy, CopyMetaDto::Ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_history_and_view_state_outlive_the_json_input_after_restoration() {
        let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
        let info = SessionInfo {
            id: id.into(),
            created: 1,
            updated: 1,
            workspace: PathBuf::from("."),
            workspace_key: "workspace".into(),
            mode: SessionMode::Worktree,
            title: "review".into(),
            total_files: 2,
        };
        let state = SessionState {
            explanation: Some(ExplanationHistory {
                current_revision: 1,
                versions: vec![crate::domain::explanation::ExplanationVersion {
                    revision: 1,
                    base_display_version: format!("{id}:1"),
                    sections: vec![crate::domain::explanation::ExplanationSection {
                        id: "section".into(),
                        title: "説明".into(),
                        blocks: vec![crate::domain::explanation::ExplanationBlock {
                            id: "block".into(),
                            part: Part::Text {
                                body: "日本語\n\"quoted\"".into(),
                            },
                            references: vec![],
                        }],
                    }],
                }],
            }),
            seen: BTreeSet::from(["f1".into(), "f2".into()]),
            collapsed: BTreeMap::from([("f1".into(), true)]),
            mocks: BTreeMap::from([("/preview".into(), "mock.html".into())]),
            ..SessionState::default()
        };
        let bytes = serde_json::to_vec(&MetaDto::from_parts(&info, &state, &CopyState::Pending))
            .expect("serializable session");
        let saved: MetaDto<'static> =
            MetaDto::decode(encoding::VERSION, &bytes).expect("read saved session");
        drop(bytes);
        let (restored_info, restored_state, _) = saved.into_parts().expect("restore saved session");
        assert_eq!(restored_info, info);
        assert_eq!(restored_state, state);
    }
}
