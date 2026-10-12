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
mod endpoint;
pub(crate) mod files;
mod format;
mod store;
mod ulid;

pub use endpoint::{Endpoint, endpoint_path, read_endpoint, remove_endpoint, write_endpoint};
pub use files::{FILES_LIMIT, PageSnapshot, SnapshotKind};
pub use format::additional_saved_size;

pub use store::{
    COPY_LIMIT, FileWritten, FilesWrite, KEEP_BYTES, KEEP_SESSIONS, OpenSession, SessionError,
    SessionLock, SessionStore, StoredSession,
};
pub use ulid::{generate_ulid, is_valid_id, new_ulid, now_millis};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use format::{decode_copy, encode_copy};

use crate::domain::agent::{AgentEvent, Channel, HandedComment};
use crate::domain::conversation_content::ConversationContent;
use crate::domain::display_version::DisplayVersions;
use crate::domain::explanation::{ExplanationHistory, canonical_size};
use crate::domain::live::LivePage;
use crate::domain::parts_validation::validate_parts;
use crate::domain::review::{Comment, CommentTarget, GroupBy, Message, ReviewMeta};
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

impl SessionState {
    pub fn refresh_explanation_comments(&mut self) {
        if let Some(history) = self.explanation.as_mut() {
            history.versions.sort_by_key(|version| version.revision);
        }
        let document = self
            .explanation
            .as_ref()
            .and_then(ExplanationHistory::current)
            .map(|v| crate::domain::explanation::ExplanationDocument {
                sections: v.sections.clone(),
            });
        for comment in &mut self.comments {
            if let CommentTarget::Explanation(target) = &comment.target {
                comment.outdated = document.as_ref().is_none_or(|document| {
                    crate::domain::explanation::block_outdated(&target.quote.block, document)
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
            128,
        )
        .map_err(|e| SessionError::Encode {
            reason: e.to_string(),
        })?;
        let bytes = history
            .versions
            .iter()
            .map(|v| canonical_size(&serde_json::to_value(v).expect("explanation is JSON")))
            .sum::<usize>();
        crate::domain::parts_validation::bound(
            "explanation history canonical JSON bytes",
            bytes,
            0,
            8_388_608,
        )
        .map_err(|e| SessionError::Encode {
            reason: e.to_string(),
        })?;
    }
    crate::domain::parts_validation::bound(
        "additional saved content canonical JSON bytes",
        additional_saved_size(state),
        0,
        16_777_216,
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

pub struct PreparedCopy {
    pub(crate) copy: SessionCopy,
    pub(crate) compressed: Vec<u8>,
}
impl PreparedCopy {
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
