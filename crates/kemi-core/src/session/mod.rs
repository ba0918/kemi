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

mod cleanup;
mod encoding;
mod endpoint;
pub(crate) mod files;
mod format;
mod lock;
mod storage_io;
mod store;
mod ulid;

pub(crate) use endpoint::endpoint_path;
pub use endpoint::{Endpoint, read_endpoint, remove_endpoint, write_endpoint};
pub use files::{FILES_LIMIT, PageSnapshot, SnapshotKind};
pub use format::additional_saved_size;

pub use store::{FileWritten, FilesWrite, OpenSession, SavedSession, SessionError, SessionStore};
pub use ulid::{is_valid_id, new_ulid, now_millis};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use format::{decode_copy, encode_copy};

use crate::domain::agent::{AgentEvent, Channel, ContentWrite, HandedComment};
use crate::domain::content::FileContent;
use crate::domain::conversation_content::ConversationContent;
use crate::domain::display_version::DisplayVersions;
use crate::domain::explanation::{
    ExplanationHistory, MAX_ADDITIONAL_SAVED_JSON_BYTES, MAX_HISTORY_JSON_BYTES,
    MAX_RETAINED_VERSIONS, canonical_size,
};
use crate::domain::live::LivePage;
use crate::domain::parts_validation::validate_parts;
use crate::domain::review::{Author, Comment, CommentTarget, GroupBy, Message, Reply, ReviewMeta};

/// ULID でない id は存在しない id として拒否し、保存領域の外へパスを組み立てない。
fn checked_id(id: &str) -> Result<&str, SessionError> {
    if is_valid_id(id) {
        Ok(id)
    } else {
        Err(SessionError::NotFound { id: id.to_string() })
    }
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
    /// `--live`。写しを持たず、復元でページにつなぎ直す（R-PAGE-SESSION）。`root` は
    /// 配れる範囲の根で、git の外では起動ディレクトリなので復元時の場所からは決められない。
    Live {
        page: LivePage,
        root: PathBuf,
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
            SessionMode::Live { page, .. } => format!("live {}", page.display_url()),
        }
    }

    /// 写しを持たないモードか（R-PAGE-SESSION）。
    pub fn is_live(&self) -> bool {
        matches!(self, SessionMode::Live { .. })
    }
}

/// セッション状態。折りたたみはファイルごとの畳みの上書き（R-VIEW）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionState {
    pub display: Option<DisplayVersions>,
    pub explanation: Option<ExplanationHistory>,
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
    /// 最後に振った通し番号。コメント・返信・発言で 1 本の列を使う（R-SESSION）。
    pub last_seq: u32,
    /// エージェントとの往復の続き。
    pub channel: Channel,
    /// `--live` のモックの割り当て。ページ（パスとクエリ）→ 配れる範囲の根からの相対パス
    /// （live-compare.md の R-PAGE-MOCK）。
    pub mocks: BTreeMap<String, String>,
    /// このレビューでページへのコメントを一度でも保存したか。コメントを全部消しても
    /// 戻さない（始め方の案内を出し直さないため。live.md の R-PAGE-COMMENT）。
    pub page_comment_saved: bool,
}

/// `apply_agent_writes` の結果。
pub struct AppliedAgentWrites {
    /// 振った id。書き込みの順。
    pub ids: Vec<String>,
    /// 返信したコメントの id。初めて現れた順で、重複しない。
    pub touched: Vec<String>,
    /// 足した発言の `messages` の中の位置。
    pub message_indices: Vec<usize>,
}

impl SessionState {
    /// 次の書き込みの通し番号。画面とエージェントのどちらが書いても同じ列から振る。
    pub fn next_seq(&mut self) -> u32 {
        self.last_seq += 1;
        self.last_seq
    }

    /// エージェントの書き込みを会話に足す。途中まで書かれた会話を残さないよう、呼び出し側が
    /// `validate_writes` で全件を確かめてから呼ぶ（R-AGENT-WRITE）。
    pub fn apply_agent_writes(&mut self, writes: Vec<ContentWrite>) -> AppliedAgentWrites {
        let mut ids = Vec::new();
        let mut touched: Vec<String> = Vec::new();
        let mut message_indices = Vec::new();
        for write in writes {
            match write {
                ContentWrite::Reply {
                    comment_id,
                    content,
                } => {
                    self.last_reply += 1;
                    let reply = Reply {
                        id: format!("r{}", self.last_reply),
                        seq: self.next_seq(),
                        author: Author::Agent,
                        body: content,
                    };
                    ids.push(reply.id.clone());
                    if let Some(comment) = self
                        .comments
                        .iter_mut()
                        .find(|comment| comment.id == comment_id)
                    {
                        comment.replies.push(reply);
                    }
                    if !touched.contains(&comment_id) {
                        touched.push(comment_id);
                    }
                }
                ContentWrite::Message { content } => {
                    self.last_message += 1;
                    let message = Message {
                        id: format!("m{}", self.last_message),
                        seq: self.next_seq(),
                        author: Author::Agent,
                        body: content,
                    };
                    ids.push(message.id.clone());
                    message_indices.push(self.messages.len());
                    self.messages.push(message);
                }
            }
        }
        AppliedAgentWrites {
            ids,
            touched,
            message_indices,
        }
    }

    /// コメント・返信・発言の id は再利用しないので、採番は
    /// 保存した最終番号と、残っているものの番号の大きい方から続ける。
    pub fn prepare_for_restore(mut self) -> Self {
        self.refresh_explanation_comments();
        let from_comments =
            highest_number(self.comments.iter().map(|comment| comment.id.as_str()), 'c');
        let from_replies = highest_number(
            self.comments
                .iter()
                .flat_map(|comment| comment.replies.iter().map(|reply| reply.id.as_str())),
            'r',
        );
        let from_messages =
            highest_number(self.messages.iter().map(|message| message.id.as_str()), 'm');
        let from_writes = self
            .comments
            .iter()
            .flat_map(|comment| {
                std::iter::once(comment.seq).chain(comment.replies.iter().map(|reply| reply.seq))
            })
            .chain(self.messages.iter().map(|message| message.seq))
            .max()
            .unwrap_or(0);
        self.last_comment = self.last_comment.max(from_comments);
        self.last_reply = self.last_reply.max(from_replies);
        self.last_message = self.last_message.max(from_messages);
        self.last_seq = self.last_seq.max(from_writes);
        self
    }

    pub fn refresh_explanation_comments(&mut self) {
        if let Some(history) = self.explanation.as_mut() {
            history.versions.sort_by_key(|version| version.revision);
        }
        let sections = self
            .explanation
            .as_ref()
            .and_then(ExplanationHistory::current)
            .map(|v| v.sections.as_slice());
        for comment in &mut self.comments {
            if let CommentTarget::Explanation(target) = &comment.target {
                comment.outdated = sections.is_none_or(|sections| {
                    crate::domain::explanation::block_outdated(&target.quote.block, sections)
                });
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
            && self.seen.is_empty()
            && self.collapsed.is_empty()
            && self.messages.is_empty()
            && self.channel == Channel::default()
            && self.explanation.is_none()
    }

    /// コメント（返信を含む）・発言・説明が 1 つでもあるか。`--live` のセッションはこれが
    /// 無ければ保留しても残さない（R-PAGE-SESSION）。
    pub fn has_conversation(&self) -> bool {
        !self.comments.is_empty() || !self.messages.is_empty() || self.explanation.is_some()
    }
}

/// `c12` のような id の番号の最大。
fn highest_number<'a>(ids: impl Iterator<Item = &'a str>, prefix: char) -> u32 {
    ids.filter_map(|id| id.strip_prefix(prefix))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
}

pub fn validate_additional(state: &SessionState) -> Result<(), SessionError> {
    let check = |content: &ConversationContent| match content {
        ConversationContent::Plain(_) => Ok(()),
        ConversationContent::Parts(parts) => {
            validate_parts(parts).map_err(|e| SessionError::Encode {
                reason: e.to_string(),
            })
        }
    };
    for reply in state.comments.iter().flat_map(|c| &c.replies) {
        check(&reply.body)?;
    }
    for message in &state.messages {
        check(&message.body)?;
    }
    for AgentEvent::Handed(event) in &state.channel.events {
        for change in &event.comments {
            match change {
                HandedComment::Added(comment) | HandedComment::Edited(comment) => {
                    for reply in &comment.replies {
                        check(&reply.body)?;
                    }
                }
                HandedComment::Deleted(_) => {}
            }
        }
        for reply in &event.replies {
            check(&reply.reply.body)?;
        }
        for message in &event.messages {
            check(&message.body)?;
        }
    }
    if let Some(history) = &state.explanation {
        crate::domain::parts_validation::bound(
            "explanation.versions",
            history.versions.len(),
            1,
            MAX_RETAINED_VERSIONS,
        )
        .map_err(|e| SessionError::Encode {
            reason: e.to_string(),
        })?;
        let bytes = history
            .versions
            .iter()
            .map(canonical_size)
            .fold(0usize, usize::saturating_add);
        crate::domain::parts_validation::bound(
            "explanation history canonical JSON bytes",
            bytes,
            0,
            MAX_HISTORY_JSON_BYTES,
        )
        .map_err(|e| SessionError::Encode {
            reason: e.to_string(),
        })?;
    }
    crate::domain::parts_validation::bound(
        "additional saved content canonical JSON bytes",
        additional_saved_size(state),
        0,
        MAX_ADDITIONAL_SAVED_JSON_BYTES,
    )
    .map_err(|e| SessionError::Encode {
        reason: e.to_string(),
    })
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

/// 保存する前に圧縮まで済ませた写し。圧縮は重いので、保存の門の外で作る。
pub struct PreparedCopy {
    pub(crate) copy: SessionCopy,
    pub(crate) compressed: Vec<u8>,
}
impl PreparedCopy {
    /// 写しの中身。core のストアを使わない保存先（`SessionSink` の実装）が中身を持つときに使う。
    pub fn into_copy(self) -> SessionCopy {
        self.copy
    }
}
pub fn prepare_copy(copy: SessionCopy) -> Result<PreparedCopy, SessionError> {
    let body = encode_copy(&copy).map_err(|reason| SessionError::Encode { reason })?;
    Ok(PreparedCopy {
        copy,
        compressed: encoding::gzip(&body),
    })
}

/// 写しの状態と、「使える」写しの中身。未完成・失敗・上限超過のセッションは一覧と
/// 復元の対象にしない。
#[derive(Clone, Debug, PartialEq)]
pub enum CopyState {
    /// まだ凍結が終わっていない。
    Pending,
    Ready(SessionCopy),
    /// 凍結できなかった理由（内容の読み取り失敗、20 MB 超過など）。
    Unusable(String),
}

/// 写しの状態だけ。`<id>.session` に書くのはこれで、「使える」写しの中身は `<id>.payload`
/// にある。開いているセッションも中身を持たず、これだけを持つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyStatus {
    Pending,
    Ready,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::ContentWrite;
    use crate::domain::review::{Author, FileTarget, Reply};
    use crate::testutil::{file_comment, file_target};

    fn comment(id: &str) -> Comment {
        Comment {
            seq: 1,
            target: CommentTarget::File(FileTarget {
                path: "a.rs".to_string(),
                quote: vec!["line".to_string()],
                ..file_target()
            }),
            ..file_comment(id)
        }
    }

    /// c3（返信 r5）と発言 m2 を持つ状態。通し番号は書いた順に 1, 2, 3 で、
    /// `largest_seq_at` の位置のものにだけ 9 を振る。
    fn restored_contents(largest_seq_at: usize) -> SessionState {
        let mut seqs = [1, 2, 3];
        seqs[largest_seq_at] = 9;
        let mut c3 = comment("c3");
        c3.seq = seqs[0];
        c3.replies.push(Reply {
            id: "r5".to_string(),
            seq: seqs[1],
            author: Author::Reviewer,
            body: "reply".into(),
        });
        SessionState {
            comments: vec![c3],
            messages: vec![Message {
                id: "m2".to_string(),
                seq: seqs[2],
                author: Author::Agent,
                body: "message".into(),
            }],
            ..SessionState::default()
        }
    }

    #[test]
    fn restoring_continues_numbering_from_the_contents_above_the_saved_counters() {
        // コメント・返信・発言のどれが最後の通し番号を持っていても、その番号から続ける。
        for largest_seq_at in 0..3 {
            let restored = restored_contents(largest_seq_at).prepare_for_restore();

            assert_eq!(
                (
                    restored.last_comment,
                    restored.last_reply,
                    restored.last_message,
                    restored.last_seq
                ),
                (3, 5, 2, 9),
                "the largest sequence number is at {largest_seq_at}"
            );
        }
    }

    #[test]
    fn restoring_keeps_saved_counters_above_the_contents() {
        // 消したコメント・返信・発言の番号は、中身から消えても再利用しない（R-COMMENT）。
        let state = SessionState {
            last_comment: 10,
            last_reply: 20,
            last_message: 30,
            last_seq: 40,
            ..restored_contents(0)
        };

        let restored = state.prepare_for_restore();

        assert_eq!(
            (
                restored.last_comment,
                restored.last_reply,
                restored.last_message,
                restored.last_seq
            ),
            (10, 20, 30, 40)
        );
    }

    #[test]
    fn agent_writes_continue_ids_and_share_the_sequence_in_input_order() {
        let existing_message = Message {
            id: "m3".to_string(),
            seq: 9,
            author: Author::Reviewer,
            body: "existing".into(),
        };
        let mut state = SessionState {
            comments: vec![comment("c1"), comment("c2")],
            messages: vec![existing_message.clone()],
            last_reply: 4,
            last_message: 3,
            last_seq: 9,
            ..SessionState::default()
        };
        let applied = state.apply_agent_writes(vec![
            ContentWrite::Reply {
                comment_id: "c2".to_string(),
                content: "first".into(),
            },
            ContentWrite::Message {
                content: "second".into(),
            },
            ContentWrite::Reply {
                comment_id: "c1".to_string(),
                content: "third".into(),
            },
            ContentWrite::Reply {
                comment_id: "c2".to_string(),
                content: "fourth".into(),
            },
            ContentWrite::Message {
                content: "fifth".into(),
            },
        ]);

        assert_eq!(applied.ids, ["r5", "m4", "r6", "r7", "m5"]);
        assert_eq!(applied.touched, ["c2", "c1"]);
        assert_eq!(applied.message_indices, [1, 2]);
        assert_eq!(state.messages[0], existing_message);
        for (comment_index, reply_index, id, seq, body) in [
            (1, 0, "r5", 10, "first"),
            (0, 0, "r6", 12, "third"),
            (1, 1, "r7", 13, "fourth"),
        ] {
            assert_eq!(
                state.comments[comment_index].replies[reply_index],
                Reply {
                    id: id.to_string(),
                    seq,
                    author: Author::Agent,
                    body: body.into(),
                }
            );
        }
        for (index, id, seq, body) in [(1, "m4", 11, "second"), (2, "m5", 14, "fifth")] {
            assert_eq!(
                state.messages[index],
                Message {
                    id: id.to_string(),
                    seq,
                    author: Author::Agent,
                    body: body.into(),
                }
            );
        }
        assert_eq!(
            (state.last_reply, state.last_message, state.last_seq),
            (7, 5, 14)
        );
    }
}
