//! エージェントとの往復の続き（agent-channel.md）。人間が書いてまだ渡していない変化と、
//! 渡したがまだ `kemi wait` が受け取っていない起きたこと。

use super::conversation_content::ConversationContent;
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

/// 渡す 1 件のコメントの変化を、本文を写さずに指す。`preview` と `unhanded_count` が同じ選び方を使う。
enum HandedCommentRef<'a> {
    Added(&'a Comment),
    Edited(&'a Comment),
    Deleted(&'a str),
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
        let changed = self
            .selected_comments(comments)
            .map(|change| match change {
                HandedCommentRef::Added(comment) => HandedComment::Added(comment.clone()),
                HandedCommentRef::Edited(comment) => HandedComment::Edited(comment.clone()),
                HandedCommentRef::Deleted(id) => HandedComment::Deleted(id.to_string()),
            })
            .collect();
        let replies = self
            .selected_replies(comments)
            .map(|(comment_id, reply)| HandedReply {
                comment_id: comment_id.to_string(),
                reply: reply.clone(),
            })
            .collect();
        let messages = self.selected_messages(messages).cloned().collect();
        Handed {
            comments: changed,
            replies,
            messages,
        }
    }

    /// まだ渡していない件数（R-SUBMIT の確認と、画面の表示）。
    pub fn unhanded_count(&self, comments: &[Comment], messages: &[Message]) -> usize {
        self.selected_comments(comments).count()
            + self.selected_replies(comments).count()
            + self.selected_messages(messages).count()
    }

    fn selected_comments<'a>(
        &'a self,
        comments: &'a [Comment],
    ) -> impl Iterator<Item = HandedCommentRef<'a>> + 'a {
        let find = |id: &str| comments.iter().find(|comment| comment.id == id);
        let handed_before = |id: &str| self.handed_comments.iter().any(|known| known == id);
        self.unhanded
            .comments
            .iter()
            .filter_map(move |id| match (find(id), handed_before(id)) {
                (Some(comment), false) => Some(HandedCommentRef::Added(comment)),
                (Some(comment), true) => Some(HandedCommentRef::Edited(comment)),
                (None, true) => Some(HandedCommentRef::Deleted(id)),
                (None, false) => None,
            })
    }

    fn selected_replies<'a>(
        &'a self,
        comments: &'a [Comment],
    ) -> impl Iterator<Item = (&'a str, &'a Reply)> + 'a {
        self.unhanded.replies.iter().filter_map(|reference| {
            let reply = comments
                .iter()
                .find(|comment| comment.id == reference.comment_id)?
                .replies
                .iter()
                .find(|reply| reply.id == reference.reply_id)?;
            Some((reference.comment_id.as_str(), reply))
        })
    }

    fn selected_messages<'a>(
        &'a self,
        messages: &'a [Message],
    ) -> impl Iterator<Item = &'a Message> + 'a {
        self.unhanded
            .messages
            .iter()
            .filter_map(|id| messages.iter().find(|message| &message.id == id))
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
    /// 最後に `kemi wait` が返った（または返さずに終わった）後に、書き込みが 1 件以上届いたか。
    /// コメントを消して残りの行が無くなったときに、返事済みにするかを決める。
    wrote_since_return: bool,
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
            wrote_since_return: false,
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
        self.wrote_since_return = false;
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
        self.wrote_since_return = true;
        self.last_activity = now;
    }

    /// 人間がコメントを消した。`comments` は消した後に今あるコメント。消したスレッドの行は読むときに
    /// 絞られるので、ここでは状態だけを表のとおりに移す: 作業中か応答なしで残りの行が無くなり、最後に
    /// 返った後に書き込みが届いていれば返事済み。まだ届いていなければ（返事を 1 つも書いていないので）
    /// 変えない。応答なしまでの時計は数え直さない（エージェントが動いたわけではない）。
    pub fn comment_deleted(&mut self, called: bool, now: u128, comments: &[Comment]) {
        let before = self.status(called, now);
        if self.wrote_since_return && !self.lines_left(comments) {
            self.replied = replied_after_write(before, false);
        }
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

/// 検証を通った、エージェントの書き込み 1 件（`kemi reply`、R-AGENT-WRITE）。
pub enum ContentWrite {
    Reply {
        comment_id: String,
        content: ConversationContent,
    },
    Message {
        content: ConversationContent,
    },
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
    TooManyReplies(String),
    TooManyMessages,
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NoSuchComment(id) => write!(formatter, "no comment {id} in this review"),
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
/// 本文の大きさと形は、書き込みを作る前に `validate_content` が全件を確かめている。
pub fn validate_writes(
    writes: &[ContentWrite],
    comments: &[Comment],
    messages: &[Message],
) -> Result<(), WriteError> {
    let mut new_replies: Vec<(&str, usize)> = Vec::new();
    let mut new_messages = 0usize;
    for write in writes {
        match write {
            ContentWrite::Reply { comment_id, .. } => {
                if !comments.iter().any(|comment| &comment.id == comment_id) {
                    return Err(WriteError::NoSuchComment(comment_id.clone()));
                }
                match new_replies.iter_mut().find(|(id, _)| id == comment_id) {
                    Some((_, count)) => *count += 1,
                    None => new_replies.push((comment_id, 1)),
                }
            }
            ContentWrite::Message { .. } => new_messages += 1,
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
#[path = "agent/tests.rs"]
mod tests;
