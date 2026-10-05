//! エージェントとの往復の続き（agent-channel.md）。人間が書いてまだ渡していない変化と、
//! 渡したがまだ `kemi wait` が受け取っていない起きたこと。

use super::review::{Comment, Message, Reply};

/// 往復の続き。セッション状態の一部として保存し、保留と復元をまたぐ（R-SESSION）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Channel {
    /// このレビューで `kemi wait` が一度でも呼ばれたか（R-AGENT-STATE）。
    pub called: bool,
    /// 一度でも渡したコメントの id。次に渡すとき、追加か編集かを分け、削除を知らせるか
    /// を決める。
    pub handed_comments: Vec<String>,
    /// 前に渡した後に人間が起こした変化（R-AGENT-HAND）。
    pub unhanded: Unhanded,
    /// 渡したが、まだ `kemi wait` が受け取っていない起きたこと。起きた順。
    pub events: Vec<AgentEvent>,
}

/// まだ渡していない変化。中身は渡すときの最新を読むので、id だけを持つ。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unhanded {
    /// 追加・編集・削除のあったコメント。最初に変わった順。
    pub comments: Vec<String>,
    /// 人間が書いた返信。書いた順。
    pub replies: Vec<ReplyRef>,
    /// 人間が書いた発言。書いた順。
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyRef {
    pub comment_id: String,
    pub reply_id: String,
}

/// `kemi wait` に返す起きたこと（R-AGENT-EVENTS）。submit は保存しない（submit で
/// セッションごと消える）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    Handed(Handed),
}

/// 「エージェントに渡す」1 回分。中身は渡した時点のもの。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Handed {
    pub comments: Vec<HandedComment>,
    pub replies: Vec<HandedReply>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandedComment {
    Added(Comment),
    Edited(Comment),
    /// 削除したコメントは id だけを持つ。
    Deleted(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandedReply {
    pub comment_id: String,
    pub reply: Reply,
}
