//! エージェントとの往復の続き（agent-channel.md）。人間が書いてまだ渡していない変化と、
//! 渡したがまだ `kemi wait` が受け取っていない起きたこと。

use super::review::{Author, Comment, Message, Reply};

/// 往復の続き。セッション状態の一部として保存し、保留と復元をまたぐ（R-SESSION）。
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    Handed(Handed),
}

/// 「エージェントに渡す」1 回分。中身は渡した時点のもの。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handed {
    pub comments: Vec<HandedComment>,
    pub replies: Vec<HandedReply>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, PartialEq)]
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
    NotConnected,
    /// `kemi wait` が待っている。
    Waiting,
    /// `kemi wait` が返った後で、エージェントの返事を待っている。
    Working,
    /// 最後に返った `kemi wait` が返した 1 回分に、エージェントが返事を書き終えた。
    Replied,
    /// 作業中のまま 10 分、`kemi wait` も `kemi reply` も来ていない。表示だけで、
    /// レビューは止めない。
    NoResponse,
}

impl AgentStatus {
    /// 画面の `data-kemi-agent-state` の値（R-AGENT-STATE の契約）。
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::NotConnected => "not-connected",
            AgentStatus::Waiting => "waiting",
            AgentStatus::Working => "working",
            AgentStatus::Replied => "replied",
            AgentStatus::NoResponse => "no-response",
        }
    }
}

/// 作業中から応答なしになるまで。
pub const UNRESPONSIVE_AFTER_MILLIS: u128 = 10 * 60 * 1000;

/// 今の状態。`replied` は返事済みになってから次の `kemi wait` まで立つ。`last_activity` は最後に
/// `kemi wait` が返った（または切れた）か `kemi reply` が来た時刻、`now` は今。どちらも UNIX
/// エポックからのミリ秒。返事済みは何分たっても応答なしにしない。
pub fn agent_status(
    called: bool,
    waiting: bool,
    replied: bool,
    last_activity: u128,
    now: u128,
) -> AgentStatus {
    if !called {
        AgentStatus::NotConnected
    } else if waiting {
        AgentStatus::Waiting
    } else if replied {
        AgentStatus::Replied
    } else if now.saturating_sub(last_activity) >= UNRESPONSIVE_AFTER_MILLIS {
        AgentStatus::NoResponse
    } else {
        AgentStatus::Working
    }
}

/// エージェントの書き込みが届いた後に返事済みか（R-AGENT-STATE の表）。作業中と応答なしは、最後に
/// 返った応答の行が残っていなければ返事済みになる。ほかの状態は変えない。
pub fn replied_after_write(before: AgentStatus, lines_left: bool) -> bool {
    match before {
        AgentStatus::Working | AgentStatus::NoResponse => !lines_left,
        AgentStatus::Replied => true,
        AgentStatus::NotConnected | AgentStatus::Waiting => false,
    }
}

/// 渡した 1 回分の行のある場所（R-AGENT-HAND）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandLineAt {
    /// そのスレッドの末尾。
    Thread(String),
    /// 発言だけの 1 回分の、会話パネルの並びの末尾。
    Messages,
}

/// 行の状態。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandLineState {
    /// エージェントの受け取りを待っている。
    Pending,
    /// `kemi wait` が返し、エージェントが作業している。
    Working,
}

impl HandLineState {
    /// 画面の `data-kemi-hand-line` の値（R-AGENT-HAND の契約）。
    pub fn as_str(self) -> &'static str {
        match self {
            HandLineState::Pending => "pending",
            HandLineState::Working => "working",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandLine {
    pub at: HandLineAt,
    pub state: HandLineState,
}

impl Handed {
    /// この 1 回分が行を付ける場所。足した・編集したコメントと返信のスレッドで、スレッドを 1 つも
    /// 含まず発言を含むなら並びの末尾。消したコメントは行を持たない。
    pub fn line_places(&self) -> Vec<HandLineAt> {
        let mut threads: Vec<HandLineAt> = Vec::new();
        let comments = self.comments.iter().filter_map(|change| match change {
            HandedComment::Added(comment) | HandedComment::Edited(comment) => Some(&comment.id),
            HandedComment::Deleted(_) => None,
        });
        for id in comments.chain(self.replies.iter().map(|reply| &reply.comment_id)) {
            let at = HandLineAt::Thread(id.clone());
            if !threads.contains(&at) {
                threads.push(at);
            }
        }
        if threads.is_empty() && !self.messages.is_empty() {
            threads.push(HandLineAt::Messages);
        }
        threads
    }
}

/// 返した起きたことが行を付けた場所。
fn returned_places(events: &[AgentEvent]) -> Vec<HandLineAt> {
    let mut places: Vec<HandLineAt> = Vec::new();
    for AgentEvent::Handed(handed) in events {
        for at in handed.line_places() {
            if !places.contains(&at) {
                places.push(at);
            }
        }
    }
    places
}

/// エージェントとのつながりのうち、セッションに保存しない部分（R-AGENT-STATE, R-AGENT-HAND）。
/// `kemi wait` が呼ばれたかどうかはセッション状態（[`Channel::called`]）にあるので、状態を読む
/// ときに渡す。
///
/// 行の表は起きたことの記録で、消したコメントのスレッドの行も残りうる（削除・復元・同じ 1 回分の
/// 返し直しのどれからでも）。行を読むとき（画面に出す、残りの行を数える）に、今あるコメントで
/// 絞る。エージェントは消えたコメントに返信できないので、その行は出さず、残りの行にも数えない。
#[derive(Clone, Debug, PartialEq)]
pub struct AgentLink {
    waiting: bool,
    replied: bool,
    last_activity: u128,
    lines: Vec<HandLine>,
    /// 最後に 1 回分を返した `kemi wait` の応答の行の場所。返さずに終わったら空。
    last_returned: Vec<HandLineAt>,
}

impl AgentLink {
    /// 起動したとき。復元したレビューでは、まだ受け取られていない起きたことから受け取り待ちの
    /// 行を作り直す（作業中の行と返事済みは、返したかどうかを覚えていないので戻さない）。
    pub fn new(now: u128, unreceived: &[AgentEvent]) -> Self {
        let mut link = AgentLink {
            waiting: false,
            replied: false,
            last_activity: now,
            lines: Vec::new(),
            last_returned: Vec::new(),
        };
        for AgentEvent::Handed(handed) in unreceived {
            link.handed(handed);
        }
        link
    }

    pub fn is_waiting(&self) -> bool {
        self.waiting
    }

    /// 今出す行。今あるコメントのスレッドの行と、並びの末尾の行。
    pub fn lines(&self, comments: &[Comment]) -> Vec<&HandLine> {
        self.lines
            .iter()
            .filter(|line| thread_exists(&line.at, comments))
            .collect()
    }

    pub fn status(&self, called: bool, now: u128) -> AgentStatus {
        agent_status(called, self.waiting, self.replied, self.last_activity, now)
    }

    /// `kemi wait` が始まった。
    pub fn wait_started(&mut self) {
        self.waiting = true;
        self.replied = false;
    }

    /// `kemi wait` が起きたことを返した。その応答の行のうち、受け取り待ちのものを作業中にする
    /// （行を新しく作ったり、消えた行を戻したりはしない）。
    pub fn wait_returned(&mut self, now: u128, returned: &[AgentEvent]) {
        self.wait_ended(now);
        self.last_returned = returned_places(returned);
        for line in &mut self.lines {
            if line.state == HandLineState::Pending && self.last_returned.contains(&line.at) {
                line.state = HandLineState::Working;
            }
        }
    }

    /// `kemi wait` が 1 回分を返さずに終わった（時間切れ、接続が切れた）。
    pub fn wait_ended_empty(&mut self, now: u128) {
        self.wait_ended(now);
        self.last_returned.clear();
    }

    fn wait_ended(&mut self, now: u128) {
        self.waiting = false;
        self.replied = false;
        self.last_activity = now;
    }

    /// 人間が渡した。その 1 回分の行を受け取り待ちにする（行のあるスレッドは戻す）。
    pub fn handed(&mut self, handed: &Handed) {
        for at in handed.line_places() {
            match self.lines.iter_mut().find(|line| line.at == at) {
                Some(line) => line.state = HandLineState::Pending,
                None => self.lines.push(HandLine {
                    at,
                    state: HandLineState::Pending,
                }),
            }
        }
    }

    /// `kemi reply` の書き込みが届いた。返信のスレッドの行と、発言があれば並びの末尾の行を消し、
    /// 状態を表のとおりに移す。1 件も書いていなければ、何も届いていないので何も変えない。
    /// `comments` は書いた後に今あるコメント。
    pub fn agent_wrote(
        &mut self,
        called: bool,
        now: u128,
        threads: &[String],
        message: bool,
        comments: &[Comment],
    ) {
        if threads.is_empty() && !message {
            return;
        }
        let before = self.status(called, now);
        self.lines.retain(|line| match &line.at {
            HandLineAt::Thread(id) => !threads.contains(id),
            HandLineAt::Messages => !message,
        });
        self.replied = replied_after_write(before, self.lines_left(comments));
        self.last_activity = now;
    }

    /// 最後に返った応答の行のうち、まだ作業中で残っているものがあるか。
    fn lines_left(&self, comments: &[Comment]) -> bool {
        self.lines(comments).into_iter().any(|line| {
            line.state == HandLineState::Working && self.last_returned.contains(&line.at)
        })
    }
}

/// 行の場所が今もあるか。並びの末尾はいつもある。
fn thread_exists(at: &HandLineAt, comments: &[Comment]) -> bool {
    match at {
        HandLineAt::Thread(id) => comments.iter().any(|comment| &comment.id == id),
        HandLineAt::Messages => true,
    }
}

/// `kemi reply` の書き込み 1 件（R-AGENT-WRITE）。案は動いているページのレビューでだけ
/// 受け付けるので、ここには無い。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentWrite {
    Reply { comment_id: String, body: String },
    Message { body: String },
}

/// 1 つのコメントへのエージェントの返信の上限。
pub const MAX_AGENT_REPLIES_PER_COMMENT: usize = 50;
/// エージェントのレビュー全体への発言の上限。
pub const MAX_AGENT_MESSAGES: usize = 200;
/// 本文 1 つの上限（UTF-8 のバイト数）。
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// 書き込みを 1 件も書かない理由。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    NoSuchComment(String),
    BodyTooLarge,
    TooManyReplies(String),
    TooManyMessages,
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NoSuchComment(id) => write!(formatter, "no comment {id} in this review"),
            WriteError::BodyTooLarge => write!(
                formatter,
                "a body is larger than {} KB",
                MAX_BODY_BYTES / 1024
            ),
            WriteError::TooManyReplies(id) => write!(
                formatter,
                "comment {id} would have more than {MAX_AGENT_REPLIES_PER_COMMENT} agent replies"
            ),
            WriteError::TooManyMessages => write!(
                formatter,
                "the review would have more than {MAX_AGENT_MESSAGES} agent messages"
            ),
        }
    }
}

/// 書き込みをまとめて確かめる。1 件でも上限を超えるか宛先が無ければ、全体を断る
/// （途中まで書いて半分だけ画面に出ることを防ぐ。R-AGENT-CLI）。上限には今ある
/// エージェントの書き込みと、この書き込みの両方を数える。人間の書き込みは数えない。
pub fn validate_writes(
    writes: &[AgentWrite],
    comments: &[Comment],
    messages: &[Message],
) -> Result<(), WriteError> {
    let mut new_replies: Vec<(&str, usize)> = Vec::new();
    let mut new_messages = 0usize;
    for write in writes {
        let body = match write {
            AgentWrite::Reply { comment_id, body } => {
                if !comments.iter().any(|comment| &comment.id == comment_id) {
                    return Err(WriteError::NoSuchComment(comment_id.clone()));
                }
                match new_replies.iter_mut().find(|(id, _)| id == comment_id) {
                    Some((_, count)) => *count += 1,
                    None => new_replies.push((comment_id, 1)),
                }
                body
            }
            AgentWrite::Message { body } => {
                new_messages += 1;
                body
            }
        };
        if body.len() > MAX_BODY_BYTES {
            return Err(WriteError::BodyTooLarge);
        }
    }
    for (comment_id, added) in new_replies {
        let existing = comments
            .iter()
            .filter(|comment| comment.id == comment_id)
            .flat_map(|comment| &comment.replies)
            .filter(|reply| reply.author == Author::Agent)
            .count();
        if existing + added > MAX_AGENT_REPLIES_PER_COMMENT {
            return Err(WriteError::TooManyReplies(comment_id.to_string()));
        }
    }
    let existing = messages
        .iter()
        .filter(|message| message.author == Author::Agent)
        .count();
    if existing + new_messages > MAX_AGENT_MESSAGES {
        return Err(WriteError::TooManyMessages);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{CommentTarget, FileTarget, Side};

    fn comment(id: &str, body: &str) -> Comment {
        Comment {
            id: id.to_string(),
            seq: 0,
            group_id: "g1".to_string(),
            group_title: "group".to_string(),
            body: body.to_string(),
            replies: Vec::new(),
            resolved: false,
            outdated: false,
            target: CommentTarget::File(FileTarget {
                file_id: "f1".to_string(),
                path: "src/a.rs".to_string(),
                side: Side::New,
                start_line: Some(1),
                end_line: Some(1),
                quote: vec!["one".to_string()],
                content_hash: "hash".to_string(),
                suggestion: None,
            }),
        }
    }

    fn message(id: &str, body: &str) -> Message {
        Message {
            id: id.to_string(),
            seq: 0,
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
            seq: 0,
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
            seq: 0,
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

    const T0: u128 = 1_700_000_000_000;

    fn handed_event(comments: &[&str], replies: &[&str], messages: usize) -> AgentEvent {
        AgentEvent::Handed(Handed {
            comments: comments
                .iter()
                .map(|id| HandedComment::Added(comment(id, "body")))
                .collect(),
            replies: replies
                .iter()
                .map(|id| HandedReply {
                    comment_id: id.to_string(),
                    reply: Reply {
                        id: format!("r-{id}"),
                        seq: 0,
                        author: Author::Reviewer,
                        body: "more".to_string(),
                    },
                })
                .collect(),
            messages: (0..messages)
                .map(|n| message(&format!("m{n}"), "note"))
                .collect(),
        })
    }

    fn hand(link: &mut AgentLink, event: &AgentEvent) {
        let AgentEvent::Handed(handed) = event;
        link.handed(handed);
    }

    /// 行の付くスレッドのコメントが、どれも残っているレビュー。
    fn existing() -> Vec<Comment> {
        ["c1", "c2", "c3"]
            .iter()
            .map(|id| comment(id, "body"))
            .collect()
    }

    fn lines<'a>(
        link: &'a AgentLink,
        comments: &[Comment],
    ) -> Vec<(Option<&'a str>, HandLineState)> {
        link.lines(comments)
            .into_iter()
            .map(|line| {
                let at = match &line.at {
                    HandLineAt::Thread(id) => Some(id.as_str()),
                    HandLineAt::Messages => None,
                };
                (at, line.state)
            })
            .collect()
    }

    /// `kemi wait` が呼ばれ、渡したものを返して作業中になったところ。
    fn working_on(events: &[AgentEvent]) -> AgentLink {
        let mut link = AgentLink::new(T0, &[]);
        for event in events {
            hand(&mut link, event);
        }
        link.wait_started();
        link.wait_returned(T0, events);
        link
    }

    #[test]
    fn status_is_not_connected_until_kemi_wait_is_called() {
        assert_eq!(
            AgentLink::new(T0, &[]).status(false, T0),
            AgentStatus::NotConnected
        );
    }

    #[test]
    fn status_is_waiting_while_kemi_wait_waits_whatever_it_was() {
        let mut replied = working_on(&[]);
        replied.agent_wrote(true, T0, &[], true, &existing());
        assert_eq!(replied.status(true, T0), AgentStatus::Replied);

        replied.wait_started();

        assert_eq!(
            replied.status(true, T0 + UNRESPONSIVE_AFTER_MILLIS * 3),
            AgentStatus::Waiting
        );
    }

    #[test]
    fn status_is_working_after_kemi_wait_returned() {
        let link = working_on(&[handed_event(&["c1"], &[], 0)]);

        assert_eq!(link.status(true, T0 + 1_000), AgentStatus::Working);
        assert_eq!(
            link.status(true, T0 + UNRESPONSIVE_AFTER_MILLIS - 1),
            AgentStatus::Working
        );
    }

    #[test]
    fn status_is_working_after_kemi_wait_timed_out() {
        let mut link = AgentLink::new(T0, &[]);
        link.wait_started();

        link.wait_ended_empty(T0);

        assert_eq!(link.status(true, T0), AgentStatus::Working);
    }

    #[test]
    fn status_is_no_response_after_ten_minutes_of_working() {
        let link = working_on(&[handed_event(&["c1"], &[], 0)]);

        assert_eq!(UNRESPONSIVE_AFTER_MILLIS, 10 * 60 * 1000);
        assert_eq!(
            link.status(true, T0 + UNRESPONSIVE_AFTER_MILLIS),
            AgentStatus::NoResponse
        );
    }

    #[test]
    fn a_reply_that_leaves_no_line_of_the_last_return_makes_it_replied() {
        let mut link = working_on(&[handed_event(&["c1"], &[], 0)]);

        link.agent_wrote(true, T0 + 1_000, &["c1".to_string()], false, &existing());

        assert_eq!(link.status(true, T0 + 1_000), AgentStatus::Replied);
    }

    #[test]
    fn a_write_after_a_return_without_lines_makes_it_replied() {
        let mut link = working_on(&[]);

        link.agent_wrote(true, T0 + 1_000, &[], true, &existing());

        assert_eq!(link.status(true, T0 + 1_000), AgentStatus::Replied);
    }

    #[test]
    fn a_reply_that_leaves_lines_keeps_it_working_and_counts_ten_minutes_again() {
        let mut link = working_on(&[handed_event(&["c1", "c2"], &[], 0)]);
        let wrote = T0 + UNRESPONSIVE_AFTER_MILLIS - 1;

        link.agent_wrote(true, wrote, &["c1".to_string()], false, &existing());

        assert_eq!(
            link.status(true, wrote + UNRESPONSIVE_AFTER_MILLIS - 1),
            AgentStatus::Working
        );
    }

    #[test]
    fn a_write_while_not_responding_makes_it_replied_or_working_by_the_lines_left() {
        let late = T0 + UNRESPONSIVE_AFTER_MILLIS;
        let mut all = working_on(&[handed_event(&["c1"], &[], 0)]);
        assert_eq!(all.status(true, late), AgentStatus::NoResponse);
        all.agent_wrote(true, late, &["c1".to_string()], false, &existing());
        assert_eq!(all.status(true, late), AgentStatus::Replied);

        let mut part = working_on(&[handed_event(&["c1", "c2"], &[], 0)]);
        part.agent_wrote(true, late, &["c1".to_string()], false, &existing());
        assert_eq!(part.status(true, late), AgentStatus::Working);
    }

    #[test]
    fn a_write_while_not_connected_or_waiting_does_not_change_the_status() {
        let mut never = AgentLink::new(T0, &[]);
        never.agent_wrote(false, T0, &[], true, &existing());
        assert_eq!(never.status(false, T0), AgentStatus::NotConnected);

        let mut waiting = AgentLink::new(T0, &[]);
        waiting.wait_started();
        waiting.agent_wrote(true, T0, &[], true, &existing());
        assert_eq!(waiting.status(true, T0), AgentStatus::Waiting);
        waiting.wait_ended_empty(T0);
        assert_eq!(waiting.status(true, T0), AgentStatus::Working);
    }

    #[test]
    fn a_write_while_replied_keeps_it_replied() {
        let mut link = working_on(&[handed_event(&["c1"], &[], 0)]);
        link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
        hand(&mut link, &handed_event(&["c2"], &[], 0));

        link.agent_wrote(true, T0, &[], true, &existing());

        assert_eq!(link.status(true, T0), AgentStatus::Replied);
    }

    #[test]
    fn replied_stays_replied_after_ten_minutes() {
        let mut link = working_on(&[handed_event(&["c1"], &[], 0)]);
        link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());

        assert_eq!(
            link.status(true, T0 + UNRESPONSIVE_AFTER_MILLIS * 6),
            AgentStatus::Replied
        );
    }

    #[test]
    fn handing_does_not_change_the_status_and_puts_a_pending_line() {
        let mut replied = working_on(&[handed_event(&["c1"], &[], 0)]);
        replied.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
        hand(&mut replied, &handed_event(&["c2"], &[], 0));
        assert_eq!(replied.status(true, T0), AgentStatus::Replied);
        assert_eq!(
            lines(&replied, &existing()),
            vec![(Some("c2"), HandLineState::Pending)]
        );

        let mut working = working_on(&[handed_event(&["c1"], &[], 0)]);
        hand(&mut working, &handed_event(&["c2"], &[], 0));
        assert_eq!(working.status(true, T0), AgentStatus::Working);
    }

    #[test]
    fn working_lines_of_an_earlier_return_do_not_keep_it_working() {
        let first = handed_event(&["c1"], &[], 0);
        let second = handed_event(&["c2"], &[], 0);
        let mut link = working_on(&[first]);
        hand(&mut link, &second);
        link.wait_started();
        link.wait_returned(T0, std::slice::from_ref(&second));

        link.agent_wrote(true, T0, &["c2".to_string()], false, &existing());

        assert_eq!(
            lines(&link, &existing()),
            vec![(Some("c1"), HandLineState::Working)]
        );
        assert_eq!(link.status(true, T0), AgentStatus::Replied);
    }

    #[test]
    fn a_reply_after_a_timed_out_wait_makes_it_replied() {
        let mut link = working_on(&[handed_event(&["c1", "c2"], &[], 0)]);
        link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
        link.wait_started();
        link.wait_ended_empty(T0);

        link.agent_wrote(true, T0, &["c2".to_string()], false, &existing());

        assert_eq!(link.status(true, T0), AgentStatus::Replied);
    }

    #[test]
    fn handing_puts_a_pending_line_on_each_thread_it_carries() {
        let mut link = AgentLink::new(T0, &[]);
        let mut event = handed_event(&["c1"], &["c2"], 1);
        let AgentEvent::Handed(handed) = &mut event;
        handed
            .comments
            .push(HandedComment::Deleted("c3".to_string()));

        hand(&mut link, &event);

        assert_eq!(
            lines(&link, &existing()),
            vec![
                (Some("c1"), HandLineState::Pending),
                (Some("c2"), HandLineState::Pending),
            ]
        );
    }

    #[test]
    fn handing_only_messages_puts_one_line_at_the_end() {
        let mut link = AgentLink::new(T0, &[]);

        hand(&mut link, &handed_event(&[], &[], 2));

        assert_eq!(
            lines(&link, &existing()),
            vec![(None, HandLineState::Pending)]
        );
    }

    #[test]
    fn a_return_turns_the_lines_it_carries_to_working() {
        let event = handed_event(&["c1"], &[], 0);
        let mut link = AgentLink::new(T0, &[]);
        hand(&mut link, &event);
        hand(&mut link, &handed_event(&["c2"], &[], 0));
        link.wait_started();

        link.wait_returned(T0, std::slice::from_ref(&event));

        assert_eq!(
            lines(&link, &existing()),
            vec![
                (Some("c1"), HandLineState::Working),
                (Some("c2"), HandLineState::Pending),
            ]
        );
    }

    #[test]
    fn a_return_of_several_hand_overs_turns_all_their_lines_to_working() {
        let link = working_on(&[handed_event(&["c1"], &[], 0), handed_event(&["c2"], &[], 0)]);

        assert_eq!(
            lines(&link, &existing()),
            vec![
                (Some("c1"), HandLineState::Working),
                (Some("c2"), HandLineState::Working),
            ]
        );
    }

    #[test]
    fn an_agent_reply_takes_away_only_the_line_of_its_thread() {
        let mut link = working_on(&[handed_event(&["c1", "c2"], &[], 0)]);

        link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());

        assert_eq!(
            lines(&link, &existing()),
            vec![(Some("c2"), HandLineState::Working)]
        );
    }

    #[test]
    fn a_deleted_comment_has_no_line_so_it_is_no_line_left() {
        let link_events = [handed_event(&["c1", "c2"], &[], 0)];
        let mut link = working_on(&link_events);
        let after_deleting_c2 = [comment("c1", "body")];

        assert_eq!(
            lines(&link, &after_deleting_c2),
            vec![(Some("c1"), HandLineState::Working)]
        );
        link.agent_wrote(
            true,
            T0 + 1_000,
            &["c1".to_string()],
            false,
            &after_deleting_c2,
        );

        assert_eq!(link.status(true, T0 + 1_000), AgentStatus::Replied);
    }

    #[test]
    fn an_agent_message_takes_away_the_line_at_the_end() {
        let mut link = working_on(&[handed_event(&[], &[], 1)]);

        link.agent_wrote(true, T0, &[], true, &existing());

        assert!(lines(&link, &existing()).is_empty());
    }

    #[test]
    fn handing_a_thread_again_puts_its_line_back_to_pending() {
        let mut link = working_on(&[handed_event(&["c1"], &[], 0)]);

        hand(&mut link, &handed_event(&[], &["c1"], 0));

        assert_eq!(
            lines(&link, &existing()),
            vec![(Some("c1"), HandLineState::Pending)]
        );
    }

    #[test]
    fn the_same_hand_over_returned_twice_does_not_bring_back_a_replied_line() {
        let event = handed_event(&["c1", "c2"], &[], 0);
        let mut link = working_on(std::slice::from_ref(&event));
        link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
        link.wait_started();

        link.wait_returned(T0, std::slice::from_ref(&event));

        assert_eq!(
            lines(&link, &existing()),
            vec![(Some("c2"), HandLineState::Working)]
        );
    }

    #[test]
    fn a_resumed_review_has_pending_lines_for_what_kemi_wait_has_not_received() {
        let link = AgentLink::new(T0, &[handed_event(&["c1"], &[], 0)]);

        assert_eq!(
            lines(&link, &existing()),
            vec![(Some("c1"), HandLineState::Pending)]
        );
        assert_eq!(link.status(true, T0), AgentStatus::Working);
    }

    #[test]
    fn a_resumed_review_has_no_line_for_a_handed_comment_that_no_longer_exists() {
        let unreceived = [handed_event(&["c1"], &[], 0), handed_event(&["c2"], &[], 0)];
        let existing = [comment("c2", "body")];
        let mut link = AgentLink::new(T0, &unreceived);

        assert_eq!(
            lines(&link, &existing),
            vec![(Some("c2"), HandLineState::Pending)]
        );
        link.wait_started();
        link.wait_returned(T0, &unreceived);
        link.agent_wrote(true, T0, &["c2".to_string()], false, &existing);

        assert_eq!(link.status(true, T0), AgentStatus::Replied);
    }

    #[test]
    fn a_write_with_nothing_in_it_does_not_change_the_status() {
        let mut link = AgentLink::new(T0, &[]);
        link.wait_started();
        link.wait_ended_empty(T0);

        link.agent_wrote(true, T0, &[], false, &[]);

        assert_eq!(link.status(true, T0), AgentStatus::Working);
    }

    fn agent_replies(count: usize) -> Vec<Reply> {
        (0..count)
            .map(|n| Reply {
                id: format!("r{n}"),
                seq: 0,
                author: Author::Agent,
                body: "x".to_string(),
            })
            .collect()
    }

    fn reply_write(comment_id: &str, body: &str) -> AgentWrite {
        AgentWrite::Reply {
            comment_id: comment_id.to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn writes_within_the_limits_are_accepted() {
        let comments = vec![comment("c1", "one")];

        assert_eq!(
            validate_writes(
                &[
                    reply_write("c1", "ok"),
                    AgentWrite::Message {
                        body: "done".to_string()
                    },
                ],
                &comments,
                &[],
            ),
            Ok(())
        );
    }

    #[test]
    fn a_reply_to_a_missing_comment_is_refused() {
        assert_eq!(
            validate_writes(&[reply_write("c9", "lost")], &[comment("c1", "one")], &[]),
            Err(WriteError::NoSuchComment("c9".to_string()))
        );
    }

    #[test]
    fn a_body_over_64_kb_of_utf8_is_refused() {
        let comments = vec![comment("c1", "one")];
        // 3 バイトの文字で、文字数ではなくバイト数で数えることを確かめる。
        let at_limit = "あ".repeat(MAX_BODY_BYTES / 3) + "x";
        assert_eq!(at_limit.len(), MAX_BODY_BYTES);

        assert_eq!(
            validate_writes(&[reply_write("c1", &at_limit)], &comments, &[]),
            Ok(())
        );
        assert_eq!(
            validate_writes(
                &[AgentWrite::Message {
                    body: at_limit + "x"
                }],
                &comments,
                &[]
            ),
            Err(WriteError::BodyTooLarge)
        );
    }

    #[test]
    fn agent_replies_on_one_comment_stop_at_fifty_counting_this_write() {
        let mut comments = vec![comment("c1", "one")];
        comments[0].replies = agent_replies(49);
        // 人間の返信は数えない。
        comments[0].replies.push(Reply {
            id: "r99".to_string(),
            seq: 0,
            author: Author::Reviewer,
            body: "x".to_string(),
        });

        assert_eq!(
            validate_writes(&[reply_write("c1", "fiftieth")], &comments, &[]),
            Ok(())
        );
        assert_eq!(
            validate_writes(
                &[
                    reply_write("c1", "fiftieth"),
                    reply_write("c1", "fifty-first")
                ],
                &comments,
                &[]
            ),
            Err(WriteError::TooManyReplies("c1".to_string()))
        );
    }

    #[test]
    fn agent_messages_stop_at_two_hundred_counting_this_write() {
        let mut messages: Vec<Message> = (0..199)
            .map(|n| Message {
                id: format!("m{n}"),
                seq: 0,
                author: Author::Agent,
                body: "x".to_string(),
            })
            .collect();
        messages.push(message("m200", "a reviewer message is not counted"));
        let one = || AgentWrite::Message {
            body: "x".to_string(),
        };

        assert_eq!(validate_writes(&[one()], &[], &messages), Ok(()));
        assert_eq!(
            validate_writes(&[one(), one()], &[], &messages),
            Err(WriteError::TooManyMessages)
        );
    }
}
