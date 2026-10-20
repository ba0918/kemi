use super::*;
use crate::testutil::file_comment;

fn comment(id: &str, body: &str) -> Comment {
    Comment {
        body: body.into(),
        ..file_comment(id)
    }
}

fn message(id: &str, body: &str) -> Message {
    Message {
        id: id.to_string(),
        seq: 0,
        author: Author::Reviewer,
        body: body.into(),
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
        body: "also the caller".into(),
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
        body: "more".into(),
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
                    body: "more".into(),
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

fn lines<'a>(link: &'a AgentLink, comments: &[Comment]) -> Vec<(Option<&'a str>, HandLineState)> {
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
fn deleting_the_last_awaited_comment_after_a_write_turns_working_into_replied() {
    let events = [handed_event(&["c1", "c2"], &[], 0)];
    let mut link = working_on(&events);
    link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
    assert_eq!(link.status(true, T0), AgentStatus::Working);

    let without_c2 = [comment("c1", "body"), comment("c3", "body")];
    link.comment_deleted(true, T0, &without_c2);

    assert_eq!(link.status(true, T0), AgentStatus::Replied);
}

#[test]
fn deleting_the_last_awaited_comment_after_a_write_turns_no_response_into_replied() {
    let events = [handed_event(&["c1", "c2"], &[], 0)];
    let mut link = working_on(&events);
    link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
    let later = T0 + UNRESPONSIVE_AFTER_MILLIS;
    assert_eq!(link.status(true, later), AgentStatus::NoResponse);

    link.comment_deleted(true, later, &[comment("c1", "body")]);

    assert_eq!(link.status(true, later), AgentStatus::Replied);
}

#[test]
fn deleting_the_awaited_comments_before_any_write_changes_nothing() {
    let events = [handed_event(&["c1"], &[], 0)];
    let mut link = working_on(&events);

    link.comment_deleted(true, T0, &[]);

    assert_eq!(link.status(true, T0), AgentStatus::Working);
}

#[test]
fn deleting_a_comment_while_other_awaited_lines_remain_keeps_working() {
    let events = [handed_event(&["c1", "c2", "c3"], &[], 0)];
    let mut link = working_on(&events);
    link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());

    link.comment_deleted(true, T0, &[comment("c1", "body"), comment("c3", "body")]);

    assert_eq!(link.status(true, T0), AgentStatus::Working);
}

#[test]
fn a_write_before_a_wait_that_timed_out_does_not_count_for_a_deletion() {
    let events = [handed_event(&["c1", "c2"], &[], 0)];
    let mut link = working_on(&events);
    link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
    link.wait_started();
    link.wait_ended_empty(T0);

    link.comment_deleted(true, T0, &[comment("c1", "body")]);

    assert_eq!(link.status(true, T0), AgentStatus::Working);
}

#[test]
fn a_write_before_the_last_return_does_not_count_for_a_deletion() {
    let events = [handed_event(&["c1"], &[], 0)];
    let mut link = working_on(&events);
    link.agent_wrote(true, T0, &["c1".to_string()], false, &existing());
    let next = [handed_event(&["c2"], &[], 0)];
    hand(&mut link, &next[0]);
    link.wait_started();
    link.wait_returned(T0, &next);

    link.comment_deleted(true, T0, &[comment("c1", "body")]);

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
            body: "x".into(),
        })
        .collect()
}

fn reply_write(comment_id: &str, body: &str) -> ContentWrite {
    ContentWrite::Reply {
        comment_id: comment_id.to_string(),
        content: body.into(),
    }
}

#[test]
fn writes_within_the_limits_are_accepted() {
    let comments = vec![comment("c1", "one")];

    assert_eq!(
        validate_writes(
            &[
                reply_write("c1", "ok"),
                ContentWrite::Message {
                    content: "done".into()
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
fn agent_replies_on_one_comment_stop_at_fifty_counting_this_write() {
    let mut comments = vec![comment("c1", "one")];
    comments[0].replies = agent_replies(49);
    // 人間の返信は数えない。
    comments[0].replies.push(Reply {
        id: "r99".to_string(),
        seq: 0,
        author: Author::Reviewer,
        body: "x".into(),
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
            body: "x".into(),
        })
        .collect();
    messages.push(message("m200", "a reviewer message is not counted"));
    let one = || ContentWrite::Message {
        content: "x".into(),
    };

    assert_eq!(validate_writes(&[one()], &[], &messages), Ok(()));
    assert_eq!(
        validate_writes(&[one(), one()], &[], &messages),
        Err(WriteError::TooManyMessages)
    );
}
