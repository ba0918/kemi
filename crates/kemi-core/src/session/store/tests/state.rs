//! 状態の保存と往復（R-SESSION）。書く順序と最終更新の進め方、停止の後に届いた保存、削除。

use super::super::*;
use super::fixtures::*;
use crate::domain::agent::{
    AgentEvent, Channel, Handed, HandedComment, HandedReply, ReplyRef, Unhanded,
};
use crate::domain::review::{Author, Message};

#[test]
fn saved_parts_capacity_counts_the_whole_hand_event_and_keeps_rejected_state_unchanged() {
    use crate::domain::conversation_content::ConversationContent;
    use crate::domain::parts::{Part, Role};
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let mut state = state_with_comment();
    state.comments[0].file_mut().unwrap().quote = vec![String::new()];
    state.messages = (1..=16)
        .map(|n| Message {
            id: format!("m{n}"),
            seq: n,
            author: Author::Reviewer,
            body: ConversationContent::Parts(
                (0..16)
                    .map(|_| Part::Code {
                        body: "x".repeat(32_700),
                        language: "x".into(),
                        role: Role::Example,
                        title: None,
                    })
                    .collect(),
            ),
        })
        .collect();
    let hand = |state: &mut SessionState| {
        state.channel = Channel::default();
        state.channel.note_comment("c1");
        for message in &state.messages {
            state.channel.note_message(&message.id);
        }
        state.channel.hand(&state.comments, &state.messages);
    };
    hand(&mut state);
    let measured = |state: &SessionState| {
        let dto = serde_json::to_value(MetaDto::from_parts(
            &info(id, 100),
            state,
            &CopyStatus::Pending,
        ))
        .unwrap();
        dto["state"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(crate::session::canonical_size)
            .sum::<usize>()
            + crate::session::canonical_size(&dto["state"]["channel"]["events"][0])
    };
    let extra = 16_777_216 - measured(&state);
    state.comments[0].file_mut().unwrap().quote = crate::domain::comment::quote_for(
        &["x".repeat(extra)],
        crate::domain::review::LineRange { start: 1, end: 1 },
    )
    .unwrap();
    hand(&mut state);
    assert_eq!(measured(&state), 16_777_216);
    assert_eq!(crate::session::additional_saved_size(&state), 16_777_216);
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state.clone(), 200).unwrap();
    let before = std::fs::read(session_path(&scratch.dir(), id)).unwrap();
    state.comments[0].file_mut().unwrap().quote[0].push('x');
    hand(&mut state);
    assert!(open.save_state_at(state, 300).is_err());
    assert_eq!(open.info().updated, 200);
    assert_eq!(
        std::fs::read(session_path(&scratch.dir(), id)).unwrap(),
        before
    );
    assert_eq!(
        crate::session::additional_saved_size(open.state()),
        16_777_216
    );
}

#[test]
fn issued_generation_is_persisted_without_changing_timestamp_or_payload_and_io_failure_rolls_back()
{
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let mut open = store.create(info(id, 100)).unwrap();
    let first = SessionState {
        display: Some(crate::domain::display_version::DisplayVersions::initial(id).unwrap()),
        ..SessionState::default()
    };
    open.save_issued_state(first.clone()).unwrap();
    assert_eq!(open.info().updated, 100);
    assert!(!payload_path(&scratch.dir(), id).exists());
    let bytes = std::fs::read(session_path(&scratch.dir(), id)).unwrap();
    let mut second = first.clone();
    second.display = Some(first.display.as_ref().unwrap().next(id).unwrap());
    std::fs::create_dir(scratch.dir().join(format!(".{id}.session.tmp"))).unwrap();
    assert!(open.save_issued_state(second).is_err());
    assert_eq!(open.state(), &first);
    assert_eq!(open.info().updated, 100);
    assert_eq!(
        std::fs::read(session_path(&scratch.dir(), id)).unwrap(),
        bytes
    );
}

/// 説明の版 1 つと、最初の表示版の世代を持つ状態。
fn explanation_state(id: &str) -> SessionState {
    let document:crate::domain::explanation::ExplanationDocument=serde_json::from_value(serde_json::json!({"sections":[{"id":"s","title":"Title","blocks":[{"id":"b","part":{"type":"text","body":"Body"},"references":[]}]}]})).unwrap();
    SessionState {
        display: Some(crate::domain::display_version::DisplayVersions::initial(id).unwrap()),
        explanation: Some(crate::domain::explanation::ExplanationHistory {
            current_revision: 1,
            versions: vec![crate::domain::explanation::ExplanationVersion {
                revision: 1,
                base_display_version: format!("{id}:1"),
                sections: document.sections,
            }],
        }),
        ..SessionState::default()
    }
}

#[test]
fn explanation_state_saving_advances_updated_time_only_after_atomic_success_without_payload_io() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let mut open = store.create(info(id, 100)).unwrap();
    let state = explanation_state(id);
    open.save_explanation_state_at(state.clone(), 200).unwrap();
    assert_eq!(open.info().updated, 200);
    assert_eq!(store.read(id).unwrap().info.updated, 200);
    assert!(!payload_path(&scratch.dir(), id).exists());
    let bytes = std::fs::read(session_path(&scratch.dir(), id)).unwrap();
    std::fs::create_dir(scratch.dir().join(format!(".{id}.session.tmp"))).unwrap();
    assert!(open.save_explanation_state_at(state.clone(), 300).is_err());
    assert_eq!(open.state(), &state);
    assert_eq!(open.info().updated, 200);
    assert_eq!(
        std::fs::read(session_path(&scratch.dir(), id)).unwrap(),
        bytes
    );
}

#[test]
fn explanation_state_arriving_after_delete_writes_nothing() {
    // submit を確定してセッションを消した後に、処理中だった説明の保存が届く。
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.delete().unwrap();

    assert!(matches!(
        open.save_explanation_state_at(explanation_state(id), 300),
        Err(SessionError::Unusable { .. })
    ));

    assert!(!session_path(&scratch.dir(), id).exists());
    assert!(matches!(store.read(id), Err(SessionError::NotFound { .. })));
}

#[test]
fn issued_state_arriving_after_a_live_review_was_suspended_writes_nothing() {
    // 会話の無い `--live` のレビューは、保留で `<id>.session` を消す。その後に届いた世代の
    // 保存が書くと、写しを見ずに復元できるとされる空の `--live` のセッションが一覧に出る。
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let mut open = store.create(live_info(id, 100)).unwrap();
    let first = SessionState {
        display: Some(crate::domain::display_version::DisplayVersions::initial(id).unwrap()),
        ..SessionState::default()
    };
    open.save_issued_state(first.clone()).unwrap();
    open.close_suspended().unwrap();
    assert!(!session_path(&scratch.dir(), id).exists());
    let next = SessionState {
        display: Some(first.display.as_ref().unwrap().next(id).unwrap()),
        ..first
    };

    assert!(matches!(
        open.save_issued_state(next),
        Err(SessionError::Unusable { .. })
    ));

    assert!(!session_path(&scratch.dir(), id).exists());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn session_roundtrips_info_state_and_copy() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();
    let id = open.id().to_string();
    drop(open);

    let mut opened = store.open(&id).unwrap();

    assert_eq!(opened.info().id, id);
    assert_eq!(opened.info().updated, 200);
    assert_eq!(opened.state(), &state_with_comment());
    assert_eq!(opened.copy(), &CopyStatus::Ready);
    assert_eq!(opened.take_copy(), Some(copy()));
    assert!(opened.is_resumable());
    assert!(scratch.dir().join(format!("{id}.session")).exists());
    assert!(scratch.dir().join(format!("{id}.payload")).exists());
}

#[test]
fn session_file_holds_no_frozen_review_paths() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    // 状態のどのコメントも指していないパスだけを写しに入れる。
    let mut only_in_the_copy = copy();
    only_in_the_copy.units[0].review.groups[0].files[0].path = "src/needle_marker.rs".to_string();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_frozen_copy(prepared(only_in_the_copy), state_with_comment())
        .unwrap();
    let id = open.id().to_string();
    drop(open);

    let bytes = std::fs::read(scratch.dir().join(format!("{id}.session"))).unwrap();

    assert!(
        !bytes
            .windows(b"needle_marker".len())
            .any(|window| window == b"needle_marker"),
        "the frozen review paths must not be in the session file"
    );
}

#[test]
fn general_state_has_no_file_or_generation_arrays_and_needs_no_payload_io() {
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut states = Vec::new();
    for (files, generation) in [(2, 1), (10_000, 4098)] {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut metadata = info(id, 100);
        metadata.total_files = files;
        let mut open = store.create(metadata).unwrap();
        let mut frozen = copy();
        let template = frozen.units[0].review.groups[0].files[0].clone();
        for index in 2..files {
            let mut file = template.clone();
            file.id = format!("extra-{index}");
            file.path = format!("extra-{index}.txt");
            frozen
                .contents
                .insert(file.id.clone(), content(None, Some(b"one\n")));
            frozen.units[0].review.groups[0].files.push(file);
        }
        open.save_frozen_copy(prepared(frozen), SessionState::default())
            .unwrap();
        let payload = payload_path(&scratch.dir(), id);
        std::fs::remove_file(&payload).unwrap();
        std::fs::create_dir(&payload).unwrap();
        let mut state = state_with_comment();
        state.display = Some(crate::domain::display_version::DisplayVersions {
            current: format!("{id}:{generation}"),
            frozen: Some(format!("{id}:1")),
            issued: generation,
        });
        for updated in [300, 400, 500] {
            open.save_state_at(state.clone(), updated).unwrap();
        }
        assert!(payload.is_dir());
        let bytes = std::fs::read(scratch.dir().join(format!("{id}.session"))).unwrap();
        let envelope: serde_json::Value =
            serde_json::from_slice(encoding::decode_session(&bytes).unwrap().1).unwrap();
        let mut saved = envelope["state"].clone();
        for key in [
            "diff_display_version",
            "frozen_display_version",
            "issued_display_generation",
        ] {
            assert!(
                saved
                    .as_object_mut()
                    .unwrap()
                    .remove(key)
                    .unwrap()
                    .is_string()
            );
        }
        states.push(saved);
    }
    assert_eq!(states[0], states[1]);
}

#[test]
fn session_file_is_not_left_empty() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(SessionState::default(), 200).unwrap();
    let id = open.id().to_string();

    assert!(matches!(
        store.read(&id),
        Err(SessionError::NotFound { .. })
    ));
}

#[test]
fn session_atomic_replace_ignores_a_leftover_temporary() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();
    let id = open.id().to_string();
    let temporary = scratch.dir().join(format!(".{id}.session.tmp"));
    std::fs::write(&temporary, b"part of the next write").unwrap();

    let stored = store.read(&id).unwrap();

    assert_eq!(stored.copy, CopyState::Ready(copy()));
    assert!(temporary.exists());
    open.save_state_at(state_with_comment(), 300).unwrap();
    assert!(!temporary.exists());
    assert_eq!(store.read(&id).unwrap().state, state_with_comment());
}

#[test]
fn session_delete_removes_the_file() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();
    let id = open.id().to_string();

    open.delete().unwrap();

    assert!(matches!(
        store.read(&id),
        Err(SessionError::NotFound { .. })
    ));
    assert!(!scratch.dir().join(format!("{id}.payload")).exists());
}

#[test]
fn suspending_after_submit_does_not_allow_late_writes_to_recreate_the_session() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.delete().unwrap();
    open.close_suspended().unwrap();

    open.save_state_at(state_with_comment(), 300).unwrap();
    assert!(
        open.save_frozen_copy(prepared(copy()), state_with_comment())
            .is_err()
    );
    let write = open.save_image("c1", b"image", &BTreeSet::new());

    assert!(matches!(write.written, FileWritten::Failed(_)));
    assert!(!session_path(&scratch.dir(), id).exists());
    assert!(!payload_path(&scratch.dir(), id).exists());
    assert!(!files_path(&scratch.dir(), id).exists());
}

#[cfg(unix)]
#[test]
fn session_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();
    let id = open.id().to_string();

    let dir_mode = std::fs::metadata(scratch.dir())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let mode_of = |name: String| {
        std::fs::metadata(scratch.dir().join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };

    assert_eq!(dir_mode, 0o700);
    assert_eq!(mode_of(format!("{id}.session")), 0o600);
    assert_eq!(mode_of(format!("{id}.payload")), 0o600);
}

/// エージェントとの往復の続きまで持った状態。
fn state_with_agent_channel() -> SessionState {
    let mut state = state_with_comment();
    state.comments[0].replies = vec![
        reply("r1", Author::Reviewer, "why this name?"),
        reply("r2", Author::Agent, "it follows the module"),
    ];
    state.messages = vec![Message {
        id: "m1".to_string(),
        seq: 0,
        author: Author::Agent,
        body: "started on the review".into(),
    }];
    state.last_reply = 2;
    state.last_message = 1;
    let mut edited = comment();
    edited.body = "edited body".to_string();
    state.channel = Channel {
        called: true,
        handed_comments: vec!["c1".to_string()],
        unhanded: Unhanded {
            comments: vec!["c1".to_string()],
            replies: vec![ReplyRef {
                comment_id: "c1".to_string(),
                reply_id: "r1".to_string(),
            }],
            messages: vec!["m1".to_string()],
        },
        events: vec![AgentEvent::Handed(Handed {
            comments: vec![
                HandedComment::Added(comment()),
                HandedComment::Edited(edited),
                HandedComment::Deleted("c7".to_string()),
            ],
            replies: vec![HandedReply {
                comment_id: "c1".to_string(),
                reply: reply("r1", Author::Reviewer, "why this name?"),
            }],
            messages: vec![Message {
                id: "m2".to_string(),
                seq: 0,
                author: Author::Reviewer,
                body: "look at the tests first".into(),
            }],
        })],
    };
    state
}

#[test]
fn session_roundtrips_replies_messages_and_the_agent_channel() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_agent_channel(), 200).unwrap();
    let id = open.id().to_string();
    drop(open);

    let bytes = std::fs::read(scratch.dir().join(format!("{id}.session"))).unwrap();
    assert_eq!(encoding::decode_version(&bytes).unwrap(), 4);
    assert_eq!(store.read(&id).unwrap().state, state_with_agent_channel());
}
