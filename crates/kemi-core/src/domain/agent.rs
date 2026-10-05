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

impl Channel {
    /// 人間がコメントを追加・編集・削除した。
    pub fn note_comment(&mut self, id: &str) {
        if !self.unhanded.comments.iter().any(|known| known == id) {
            self.unhanded.comments.push(id.to_string());
        }
    }

    /// 人間が返信を書いた。
    pub fn note_reply(&mut self, comment_id: &str, reply_id: &str) {
        self.unhanded.replies.push(ReplyRef {
            comment_id: comment_id.to_string(),
            reply_id: reply_id.to_string(),
        });
    }

    /// 人間が発言を書いた。
    pub fn note_message(&mut self, id: &str) {
        self.unhanded.messages.push(id.to_string());
    }

    /// 今渡したら届く 1 回分。中身は今のコメントと発言から読む。渡す前に書いて消した
    /// コメントは、エージェントが一度も見ていないので届けない。消したコメントへの返信も
    /// 届けない。
    pub fn preview(&self, comments: &[Comment], messages: &[Message]) -> Handed {
        let find = |id: &str| comments.iter().find(|comment| comment.id == id);
        let handed_before = |id: &str| self.handed_comments.iter().any(|known| known == id);
        let changed = self
            .unhanded
            .comments
            .iter()
            .filter_map(|id| match (find(id), handed_before(id)) {
                (Some(comment), false) => Some(HandedComment::Added(comment.clone())),
                (Some(comment), true) => Some(HandedComment::Edited(comment.clone())),
                (None, true) => Some(HandedComment::Deleted(id.clone())),
                (None, false) => None,
            })
            .collect();
        let replies = self
            .unhanded
            .replies
            .iter()
            .filter_map(|reference| {
                let reply = find(&reference.comment_id)?
                    .replies
                    .iter()
                    .find(|reply| reply.id == reference.reply_id)?;
                Some(HandedReply {
                    comment_id: reference.comment_id.clone(),
                    reply: reply.clone(),
                })
            })
            .collect();
        let messages = self
            .unhanded
            .messages
            .iter()
            .filter_map(|id| messages.iter().find(|message| &message.id == id).cloned())
            .collect();
        Handed {
            comments: changed,
            replies,
            messages,
        }
    }

    /// まだ渡していない件数（R-SUBMIT の確認と、画面の表示）。
    pub fn unhanded_count(&self, comments: &[Comment], messages: &[Message]) -> usize {
        let preview = self.preview(comments, messages);
        preview.comments.len() + preview.replies.len() + preview.messages.len()
    }

    /// 前に渡した後の変化を 1 回分にまとめて、受け取られるのを待つ起きたことに足す。
    /// 届けるものが無ければ何も足さず false。
    pub fn hand(&mut self, comments: &[Comment], messages: &[Message]) -> bool {
        let handed = self.preview(comments, messages);
        self.unhanded = Unhanded::default();
        if handed.is_empty() {
            return false;
        }
        for change in &handed.comments {
            match change {
                HandedComment::Added(comment) => self.handed_comments.push(comment.id.clone()),
                HandedComment::Edited(_) => {}
                HandedComment::Deleted(id) => self.handed_comments.retain(|known| known != id),
            }
        }
        self.events.push(AgentEvent::Handed(handed));
        true
    }
}

impl Handed {
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty() && self.replies.is_empty() && self.messages.is_empty()
    }
}

/// エージェントの状態（R-AGENT-STATE）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    /// このレビューで `kemi wait` が一度も呼ばれていない。
    Unconnected,
    /// `kemi wait` が待っている。
    Waiting,
    /// `kemi wait` が返った後か、待っていた接続が切れた後で、次がまだ来ていない。
    Working,
    /// 作業中のまま 10 分、`kemi wait` も `kemi reply` も来ていない。表示だけで、
    /// レビューは止めない。
    Unresponsive,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Unconnected => "unconnected",
            AgentStatus::Waiting => "waiting",
            AgentStatus::Working => "working",
            AgentStatus::Unresponsive => "unresponsive",
        }
    }
}

/// 作業中から応答なしになるまで。
pub const UNRESPONSIVE_AFTER_MILLIS: u128 = 10 * 60 * 1000;

/// `last_activity` は最後に `kemi wait` が返った（または切れた）か `kemi reply` が来た
/// 時刻、`now` は今。どちらも UNIX エポックからのミリ秒。
pub fn agent_status(called: bool, waiting: bool, last_activity: u128, now: u128) -> AgentStatus {
    if !called {
        AgentStatus::Unconnected
    } else if waiting {
        AgentStatus::Waiting
    } else if now.saturating_sub(last_activity) >= UNRESPONSIVE_AFTER_MILLIS {
        AgentStatus::Unresponsive
    } else {
        AgentStatus::Working
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{Author, Side};

    fn comment(id: &str, body: &str) -> Comment {
        Comment {
            id: id.to_string(),
            file_id: "f1".to_string(),
            group_id: "g1".to_string(),
            group_title: "group".to_string(),
            path: "src/a.rs".to_string(),
            side: Side::New,
            start_line: Some(1),
            end_line: Some(1),
            quote: vec!["one".to_string()],
            body: body.to_string(),
            replies: Vec::new(),
            resolved: false,
            outdated: false,
            content_hash: "hash".to_string(),
            suggestion: None,
        }
    }

    fn message(id: &str, body: &str) -> Message {
        Message {
            id: id.to_string(),
            author: Author::Reviewer,
            body: body.to_string(),
        }
    }

    fn handed(channel: &Channel) -> Vec<&Handed> {
        channel
            .events
            .iter()
            .map(|event| match event {
                AgentEvent::Handed(handed) => handed,
            })
            .collect()
    }

    #[test]
    fn writing_alone_hands_nothing() {
        let mut channel = Channel::default();

        channel.note_comment("c1");
        channel.note_message("m1");

        assert!(channel.events.is_empty());
    }

    #[test]
    fn hand_puts_every_change_since_the_last_hand_into_one_event() {
        let mut channel = Channel::default();
        let comments = vec![comment("c1", "one"), comment("c2", "two")];
        let messages = vec![message("m1", "start with the tests")];
        channel.note_comment("c1");
        channel.note_comment("c2");
        channel.note_message("m1");

        assert!(channel.hand(&comments, &messages));

        let events = handed(&channel);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].comments,
            vec![
                HandedComment::Added(comments[0].clone()),
                HandedComment::Added(comments[1].clone()),
            ]
        );
        assert_eq!(events[0].messages, messages);
        assert_eq!(channel.unhanded_count(&comments, &messages), 0);
    }

    #[test]
    fn a_comment_edited_after_it_was_handed_is_handed_again_as_edited() {
        let mut channel = Channel::default();
        let mut comments = vec![comment("c1", "first wording")];
        channel.note_comment("c1");
        channel.hand(&comments, &[]);
        comments[0].body = "second wording".to_string();
        channel.note_comment("c1");

        channel.hand(&comments, &[]);

        assert_eq!(
            handed(&channel)[1].comments,
            vec![HandedComment::Edited(comments[0].clone())]
        );
    }

    #[test]
    fn a_comment_edited_before_it_was_ever_handed_is_handed_as_added() {
        let mut channel = Channel::default();
        let comments = vec![comment("c1", "second wording")];
        channel.note_comment("c1");
        channel.note_comment("c1");

        channel.hand(&comments, &[]);

        assert_eq!(
            handed(&channel)[0].comments,
            vec![HandedComment::Added(comments[0].clone())]
        );
    }

    #[test]
    fn a_comment_deleted_after_it_was_handed_is_handed_by_its_id() {
        let mut channel = Channel::default();
        channel.note_comment("c1");
        channel.hand(&[comment("c1", "one")], &[]);
        channel.note_comment("c1");

        channel.hand(&[], &[]);

        assert_eq!(
            handed(&channel)[1].comments,
            vec![HandedComment::Deleted("c1".to_string())]
        );
    }

    #[test]
    fn a_comment_written_and_deleted_between_hands_is_not_handed() {
        let mut channel = Channel::default();
        channel.note_comment("c1");

        assert!(!channel.hand(&[], &[]));
        assert!(channel.events.is_empty());
    }

    #[test]
    fn a_reply_is_handed_with_its_comment_id() {
        let mut channel = Channel::default();
        let mut comments = vec![comment("c1", "one")];
        channel.note_comment("c1");
        channel.hand(&comments, &[]);
        let reply = Reply {
            id: "r1".to_string(),
            author: Author::Reviewer,
            body: "also the caller".to_string(),
        };
        comments[0].replies.push(reply.clone());
        channel.note_reply("c1", "r1");

        channel.hand(&comments, &[]);

        let second = handed(&channel)[1];
        assert!(second.comments.is_empty());
        assert_eq!(
            second.replies,
            vec![HandedReply {
                comment_id: "c1".to_string(),
                reply,
            }]
        );
    }

    #[test]
    fn replies_on_a_comment_deleted_before_the_hand_are_not_handed() {
        let mut channel = Channel::default();
        channel.note_comment("c1");
        channel.hand(&[comment("c1", "one")], &[]);
        channel.note_reply("c1", "r1");
        channel.note_comment("c1");

        channel.hand(&[], &[]);

        assert!(handed(&channel)[1].replies.is_empty());
    }

    #[test]
    fn unhanded_count_counts_comments_replies_and_messages_that_would_be_handed() {
        let mut channel = Channel::default();
        let mut comments = vec![comment("c1", "one")];
        comments[0].replies.push(Reply {
            id: "r1".to_string(),
            author: Author::Reviewer,
            body: "more".to_string(),
        });
        let messages = vec![message("m1", "hello")];
        channel.note_comment("c1");
        channel.note_reply("c1", "r1");
        channel.note_message("m1");
        // 書いて消したコメントは渡らないので、数えない。
        channel.note_comment("c2");

        assert_eq!(channel.unhanded_count(&comments, &messages), 3);
    }

    #[test]
    fn status_is_unconnected_until_kemi_wait_is_called() {
        assert_eq!(agent_status(false, false, 0, 0), AgentStatus::Unconnected);
    }

    #[test]
    fn status_is_waiting_while_kemi_wait_waits() {
        assert_eq!(
            agent_status(true, true, 0, UNRESPONSIVE_AFTER_MILLIS * 3),
            AgentStatus::Waiting
        );
    }

    #[test]
    fn status_is_working_after_kemi_wait_returned() {
        let returned = 1_700_000_000_000;

        assert_eq!(
            agent_status(true, false, returned, returned + 1_000),
            AgentStatus::Working
        );
        assert_eq!(
            agent_status(
                true,
                false,
                returned,
                returned + UNRESPONSIVE_AFTER_MILLIS - 1
            ),
            AgentStatus::Working
        );
    }

    #[test]
    fn status_is_unresponsive_after_ten_minutes_of_working() {
        let returned = 1_700_000_000_000;

        assert_eq!(UNRESPONSIVE_AFTER_MILLIS, 10 * 60 * 1000);
        assert_eq!(
            agent_status(true, false, returned, returned + UNRESPONSIVE_AFTER_MILLIS),
            AgentStatus::Unresponsive
        );
    }
}
