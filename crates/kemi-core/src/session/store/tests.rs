use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::domain::agent::{
    AgentEvent, Channel, Handed, HandedComment, HandedReply, ReplyRef, Unhanded,
};
use crate::domain::review::{Author, GroupBy, Message, Reply, ReviewMeta, Side, Suggestion};
use crate::session::files::{PageSnapshot, SnapshotKind};
use crate::session::{FrozenUnit, SessionMode};
use crate::source::FileContent;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kemi-session-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        Scratch(path)
    }

    fn dir(&self) -> PathBuf {
        self.0.join("sessions")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn info(id: &str, updated: u128) -> SessionInfo {
    SessionInfo {
        id: id.to_string(),
        created: 1,
        updated,
        workspace: PathBuf::from("/tmp/workspace"),
        workspace_key: "00000000000000aa".to_string(),
        mode: SessionMode::Worktree,
        title: "Working tree changes".to_string(),
        total_files: 2,
    }
}

fn comment() -> crate::domain::review::Comment {
    crate::domain::review::Comment {
        id: "c1".to_string(),
        seq: 0,
        group_id: "all".to_string(),
        group_title: "final".to_string(),
        body: "please change".to_string(),
        replies: vec![Reply {
            id: "r1".to_string(),
            seq: 0,
            author: Author::Reviewer,
            body: "done".into(),
        }],
        resolved: true,
        outdated: false,
        target: crate::domain::review::CommentTarget::File(crate::domain::review::FileTarget {
            file_id: "f1".to_string(),
            path: "src/a.rs".to_string(),
            side: Side::New,
            start_line: Some(1),
            end_line: Some(2),
            quote: vec!["one".to_string(), "two".to_string()],
            content_hash: "hash".to_string(),
            suggestion: Some(Suggestion {
                replacement: "replaced".to_string(),
            }),
        }),
    }
}

fn state_with_comment() -> SessionState {
    SessionState {
        comments: vec![comment()],
        seen: ["f1".to_string()].into_iter().collect(),
        collapsed: [("f1".to_string(), true)].into_iter().collect(),
        last_comment: 1,
        last_reply: 1,
        ..SessionState::default()
    }
}

#[test]
fn malformed_legacy_containers_are_reported_as_corrupt_instead_of_panicking() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    std::fs::create_dir_all(scratch.dir()).unwrap();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let state = SessionState::default();
    let meta = MetaDto::from_parts(&info(id, 100), &state, &CopyState::Pending);
    let base = serde_json::to_value(meta).unwrap();
    let mut invalid = vec![serde_json::json!(false), serde_json::json!([])];
    for state in [
        serde_json::json!(false),
        serde_json::json!([]),
        serde_json::json!(42),
    ] {
        let mut value = base.clone();
        value["state"] = state;
        invalid.push(value);
    }
    let mut value = base.clone();
    value["state"]["channel"]["events"] = serde_json::json!([false]);
    invalid.push(value);
    for version in [2, 3] {
        for value in &invalid {
            let bytes = encoding::encode_session(version, &serde_json::to_vec(value).unwrap());
            std::fs::write(session_path(&scratch.dir(), id), bytes).unwrap();
            assert!(
                matches!(store.read(id), Err(SessionError::Corrupt { .. })),
                "version {version}: {value}"
            );
        }
    }
}

#[test]
fn old_format_ignores_new_field_names_and_opaque_extensions_under_its_original_contract() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    drop(store.create(info(id, 100)).unwrap());
    let mut meta = serde_json::to_value(MetaDto::from_parts(
        &info(id, 100),
        &state_with_comment(),
        &CopyState::Pending,
    ))
    .unwrap();
    meta["state"]["opaque_extension"] =
        serde_json::json!({"author":"opaque","body":{"not":"a conversation"}});
    meta["state"]["comments"][0]["explanation"] =
        serde_json::json!({"unknown":"ignored in version 3"});
    meta["state"]["comments"][0]["replies"][0]["parts"] =
        serde_json::json!({"unknown":"ignored in version 3"});
    std::fs::write(
        session_path(&scratch.dir(), id),
        encoding::encode_session(3, &serde_json::to_vec(&meta).unwrap()),
    )
    .unwrap();
    let restored = store.read(id).unwrap();
    assert!(restored.state.comments[0].file().is_some());
    assert_eq!(restored.state.comments[0].replies[0].body, "done".into());
    assert!(restored.state.explanation.is_none());
}

#[test]
fn fixed_version_three_plain_session_is_supplemented_and_first_written_as_four_without_timestamp_or_payload_changes()
 {
    const META: &str = r#"{"info":{"id":"01HF7YAT00AAAAAAAAAAAAAAAA","created":111,"updated":222,"workspace":"/tmp/fixture","workspace_key":"00000000000000aa","mode":{"kind":"worktree"},"title":"Working tree changes","total_files":1},"state":{"comments":[],"seen":[],"collapsed":{},"messages":[{"id":"m1","author":"agent","body":"**plain**"}]},"copy":{"state":"ready"}}"#;
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    drop(store.create(info(id, 222)).unwrap());
    let mut oldcopy = copy();
    oldcopy.startup_unit = None;
    oldcopy.units[0].unit = None;
    let payload = encoding::gzip(&super::super::encode_copy(&oldcopy).unwrap());
    std::fs::write(payload_path(&scratch.dir(), id), &payload).unwrap();
    let original = encoding::encode_session(3, META.as_bytes());
    std::fs::write(session_path(&scratch.dir(), id), &original).unwrap();
    let mut open = store.open(id).unwrap();
    assert_eq!(open.info().created, 111);
    assert_eq!(open.info().updated, 222);
    assert_eq!(open.state().messages[0].body, "**plain**".into());
    let display = open.state().display.as_ref().unwrap();
    assert_eq!(display.current, format!("{id}:1"));
    assert_eq!(display.frozen.as_deref(), Some(format!("{id}:1").as_str()));
    assert_eq!(
        std::fs::read(session_path(&scratch.dir(), id)).unwrap(),
        original
    );
    open.save_issued_state(open.state().clone()).unwrap();
    assert_eq!(open.info().updated, 222);
    assert_eq!(
        encoding::decode_version(&std::fs::read(session_path(&scratch.dir(), id)).unwrap())
            .unwrap(),
        4
    );
    assert_eq!(
        std::fs::read(payload_path(&scratch.dir(), id)).unwrap(),
        payload
    );
}

#[test]
fn old_plain_file_conversations_are_not_retroactively_counted_as_new_saved_content() {
    let mut state = state_with_comment();
    state.comments = (0..257)
        .map(|i| {
            let mut c = comment();
            c.id = format!("c{i}");
            c.body = "x".repeat(65_536);
            c
        })
        .collect();
    assert_eq!(super::super::additional_saved_size(&state), 0);
    assert!(super::super::validate_additional(&state).is_ok());
}

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
            &CopyState::Pending,
        ))
        .unwrap();
        dto["state"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(super::super::canonical_size)
            .sum::<usize>()
            + super::super::canonical_size(&dto["state"]["channel"]["events"][0])
    };
    let extra = 16_777_216 - measured(&state);
    state.comments[0].file_mut().unwrap().quote = crate::domain::comment::quote_for(
        &["x".repeat(extra)],
        crate::domain::review::LineRange { start: 1, end: 1 },
    )
    .unwrap();
    hand(&mut state);
    assert_eq!(measured(&state), 16_777_216);
    assert_eq!(super::super::additional_saved_size(&state), 16_777_216);
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
        super::super::additional_saved_size(open.state()),
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

#[test]
fn explanation_state_saving_advances_updated_time_only_after_atomic_success_without_payload_io() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let mut open = store.create(info(id, 100)).unwrap();
    let document:crate::domain::explanation::ExplanationDocument=serde_json::from_value(serde_json::json!({"sections":[{"id":"s","title":"Title","blocks":[{"id":"b","part":{"type":"text","body":"Body"},"references":[]}]}]})).unwrap();
    let state = SessionState {
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
    };
    open.save_explanation_state(state.clone()).unwrap();
    assert!(open.info().updated > 100);
    let updated = open.info().updated;
    assert!(!payload_path(&scratch.dir(), id).exists());
    let bytes = std::fs::read(session_path(&scratch.dir(), id)).unwrap();
    std::fs::create_dir(scratch.dir().join(format!(".{id}.session.tmp"))).unwrap();
    assert!(open.save_explanation_state(state.clone()).is_err());
    assert_eq!(open.state(), &state);
    assert_eq!(open.info().updated, updated);
    assert_eq!(
        std::fs::read(session_path(&scratch.dir(), id)).unwrap(),
        bytes
    );
}

#[test]
fn malformed_parts_are_rejected_from_version_four_instead_of_becoming_plain() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    drop(store.create(info(id, 100)).unwrap());
    let mut value = serde_json::to_value(MetaDto::from_parts(
        &info(id, 100),
        &state_with_comment(),
        &CopyState::Pending,
    ))
    .unwrap();
    value["state"]["comments"][0]["replies"][0]["body"] = serde_json::Value::Null;
    for parts in [
        serde_json::json!([]),
        serde_json::json!([{"type":"text","body":null}]),
        serde_json::json!([{"type":"code","body":"x","language":"x","role":"example","title":null}]),
    ] {
        value["state"]["comments"][0]["replies"][0]["parts"] = parts;
        std::fs::write(
            session_path(&scratch.dir(), id),
            encoding::encode_session(4, &serde_json::to_vec(&value).unwrap()),
        )
        .unwrap();
        assert!(store.read(id).is_err());
    }
    value["state"]["comments"][0]["replies"][0]["parts"] =
        serde_json::json!([{"type":"text","body":"valid"}]);
    value["state"]["comments"][0]["replies"][0]
        .as_object_mut()
        .unwrap()
        .remove("body");
    std::fs::write(
        session_path(&scratch.dir(), id),
        encoding::encode_session(4, &serde_json::to_vec(&value).unwrap()),
    )
    .unwrap();
    assert!(store.read(id).is_err());
}

/// 説明コメント一つを持つ版 4 の保存値。`target` はコメントの `explanation` の値。
fn version_four_with_explanation_comment(
    id: &str,
    block: &serde_json::Value,
    target: serde_json::Value,
) -> serde_json::Value {
    let mut value = serde_json::to_value(MetaDto::from_parts(
        &info(id, 100),
        &state_with_comment(),
        &CopyState::Pending,
    ))
    .unwrap();
    value["state"]["explanation"] = serde_json::json!({"current_revision":1,"versions":[{"revision":1,"base_display_version":format!("{id}:2"),"sections":[{"id":"s","title":"Section","blocks":[block.clone()]}]}]});
    value["state"]["diff_display_version"] = serde_json::json!(format!("{id}:2"));
    value["state"]["frozen_display_version"] = serde_json::Value::Null;
    value["state"]["issued_display_generation"] = serde_json::json!("2");
    let comment = &mut value["state"]["comments"][0];
    comment["explanation"] = target;
    comment["file_id"] = serde_json::Value::Null;
    comment["side"] = serde_json::Value::Null;
    comment["path"] = serde_json::Value::Null;
    comment["replies"][0]["body"] = serde_json::Value::Null;
    comment["replies"][0]["parts"] = serde_json::json!([{"type":"text","body":"**parts**"}]);
    value
}

fn code_block() -> serde_json::Value {
    serde_json::json!({"id":"b","part":{"type":"code","body":"literal\r\n","language":"pseudocode","role":"example"},"references":[]})
}

#[test]
fn version_four_restores_parts_full_explanation_quotes_and_generation_without_rewriting() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let open = store.create(info(id, 100)).unwrap();
    drop(open);
    let block = code_block();
    let value = version_four_with_explanation_comment(
        id,
        &block,
        serde_json::json!({"revision":1,"block_id":"b","quote":{"section_title":"Section","block":block}}),
    );
    let bytes = encoding::encode_session(4, &serde_json::to_vec(&value).unwrap());
    std::fs::write(session_path(&scratch.dir(), id), &bytes).unwrap();
    let restored = store.read(id).unwrap();
    let encoded = serde_json::to_value(MetaDto::from_parts(
        &restored.info,
        &restored.state,
        &restored.copy,
    ))
    .unwrap();
    assert_eq!(
        encoded["state"]["explanation"],
        value["state"]["explanation"]
    );
    assert_eq!(
        encoded["state"]["comments"][0]["explanation"],
        serde_json::json!({"revision":1,"block_id":"b","quote":{"section_title":"Section","block":block},"selection":null})
    );
    assert_eq!(
        encoded["state"]["comments"][0]["replies"][0]["parts"],
        value["state"]["comments"][0]["replies"][0]["parts"]
    );
    assert_eq!(restored.info.updated, 100);
    assert_eq!(
        std::fs::read(session_path(&scratch.dir(), id)).unwrap(),
        bytes
    );
}

#[test]
fn version_four_keeps_the_saved_selection_of_an_explanation_comment() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    drop(store.create(info(id, 100)).unwrap());
    let block = code_block();
    let target = serde_json::json!({"revision":1,"block_id":"b","quote":{"section_title":"Section","block":block},"selection":{"text":"liter","occurrence":3}});
    let value = version_four_with_explanation_comment(id, &block, target.clone());
    std::fs::write(
        session_path(&scratch.dir(), id),
        encoding::encode_session(4, &serde_json::to_vec(&value).unwrap()),
    )
    .unwrap();
    let restored = store.read(id).unwrap();
    let encoded = serde_json::to_value(MetaDto::from_parts(
        &restored.info,
        &restored.state,
        &restored.copy,
    ))
    .unwrap();
    assert_eq!(encoded["state"]["comments"][0]["explanation"], target);
}

#[test]
fn version_four_refuses_a_selection_the_creation_rules_would_refuse() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    drop(store.create(info(id, 100)).unwrap());
    let diagram = serde_json::json!({"id":"b","part":{"type":"diagram","source":"a = A\n| a |","title":"Grid","role":"example"},"references":[]});
    for (block, selection) in [
        (code_block(), serde_json::json!({"text":"","occurrence":0})),
        (
            code_block(),
            serde_json::json!({"text":"x","occurrence":9_007_199_254_740_992_u64}),
        ),
        (diagram, serde_json::json!({"text":"A","occurrence":0})),
    ] {
        let write = |selection: &serde_json::Value| {
            let target = serde_json::json!({"revision":1,"block_id":"b","quote":{"section_title":"Section","block":block},"selection":selection});
            let value = version_four_with_explanation_comment(id, &block, target);
            std::fs::write(
                session_path(&scratch.dir(), id),
                encoding::encode_session(4, &serde_json::to_vec(&value).unwrap()),
            )
            .unwrap();
        };
        write(&serde_json::Value::Null);
        assert!(store.read(id).is_ok(), "the same block without a selection");
        write(&selection);
        assert!(store.read(id).is_err(), "{selection}");
    }
}

fn review() -> ReviewMeta {
    ReviewMeta {
        title: "Working tree changes".to_string(),
        subtitle: "sub".to_string(),
        meta: serde_json::json!({ "branch": "main" }),
        groups: vec![crate::domain::review::Group {
            id: "all".to_string(),
            title: "final".to_string(),
            why: "why".to_string(),
            watch: "watch".to_string(),
            files: vec![crate::domain::review::FileEntry {
                id: "f1".to_string(),
                group_id: "all".to_string(),
                path: "src/a.rs".to_string(),
                old_path: None,
                status: crate::domain::review::Status::Modify,
                add: 1,
                del: 1,
                binary: false,
                old_size: 0,
                new_size: 0,
                focus: false,
                note: "note".to_string(),
                noise: false,
                // 復元でも「内容を読まなかった」判定が同じになるよう、写しに残る。
                content_skipped: true,
            }],
        }],
        approval: vec![crate::domain::review::Approval {
            path: "src/a.rs".to_string(),
            identity: "sha256:abc".to_string(),
        }],
    }
}

fn content(old: Option<&[u8]>, new: Option<&[u8]>) -> FileContent {
    FileContent {
        old: old.map(<[u8]>::to_vec),
        new: new.map(<[u8]>::to_vec),
    }
}

fn copy() -> SessionCopy {
    SessionCopy {
        startup_unit: Some(GroupBy::File),
        units: vec![FrozenUnit {
            unit: Some(GroupBy::File),
            review: review(),
        }],
        contents: BTreeMap::from([
            (
                "f1".to_string(),
                content(Some(&[0xff, 0xfe]), Some(b"hello\n")),
            ),
            ("f2".to_string(), content(None, Some(b"added"))),
        ]),
    }
}

#[test]
fn session_roundtrips_info_state_and_copy() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();
    let id = open.id().to_string();
    drop(open);

    let opened = store.open(&id).unwrap();

    assert_eq!(opened.info().id, id);
    assert_eq!(opened.info().updated, 300);
    assert_eq!(opened.state(), &state_with_comment());
    assert_eq!(opened.copy(), &CopyState::Ready(copy()));
    assert!(opened.is_resumable());
    assert!(scratch.dir().join(format!("{id}.session")).exists());
    assert!(scratch.dir().join(format!("{id}.payload")).exists());
}

fn page_comment() -> crate::domain::review::Comment {
    use crate::domain::review::{
        CommentTarget, PageTarget, Place, PlaceElement, PlaceKind, Point, Rect,
    };
    crate::domain::review::Comment {
        id: "c2".to_string(),
        seq: 3,
        group_id: "page".to_string(),
        group_title: "Page".to_string(),
        body: "1 is too large".to_string(),
        replies: Vec::new(),
        resolved: false,
        outdated: false,
        target: CommentTarget::Page(PageTarget {
            url: "/products?x=1".to_string(),
            width: 390,
            places: vec![
                Place {
                    n: 1,
                    kind: PlaceKind::Element,
                    points: Vec::new(),
                    elements: vec![PlaceElement {
                        selector: "#buy".to_string(),
                        text: "Buy".to_string(),
                        rect: Rect {
                            x: 0.0,
                            y: 120.5,
                            w: 200.0,
                            h: 60.0,
                        },
                    }],
                },
                Place {
                    n: 3,
                    kind: PlaceKind::Pen,
                    points: vec![Point { x: 1.0, y: 2.0 }, Point { x: 3.5, y: 4.0 }],
                    elements: Vec::new(),
                },
            ],
            image: Some("/state/kemi/sessions/X.files/c2.png".to_string()),
        }),
    }
}

#[test]
fn session_roundtrips_a_page_comment_with_its_places_and_image() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let state = SessionState {
        comments: vec![comment(), page_comment()],
        last_comment: 2,
        ..SessionState::default()
    };
    open.save_state_at(state.clone(), 200).unwrap();
    let id = open.id().to_string();
    drop(open);

    let opened = store.open(&id).unwrap();

    assert_eq!(opened.state(), &state);
}

fn files_dir(scratch: &Scratch, id: &str) -> PathBuf {
    scratch.dir().join(format!("{id}.files"))
}

#[test]
fn a_session_file_is_written_into_its_files_directory_for_the_owner_only() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();

    let path = saved_path(&open.save_image("c1", b"png", &no_comments()));

    assert_eq!(path, files_dir(&scratch, open.id()).join("c1.png"));
    assert!(path.is_absolute());
    assert_eq!(std::fs::read(&path).unwrap(), b"png");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&files_dir(&scratch, open.id())), 0o700);
    }
}

#[test]
fn deleting_a_session_removes_its_files_directory() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    saved_path(&open.save_image("c1", b"png", &no_comments()));
    let id = open.id().to_string();

    open.delete().unwrap();

    assert!(!files_dir(&scratch, &id).exists());
}

#[test]
fn a_live_session_whose_conversation_is_gone_removes_its_files_directory_when_suspended() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    saved_path(&open.save_image("c1", b"png", &no_comments()));
    let id = open.id().to_string();
    open.save_state_at(SessionState::default(), 300).unwrap();

    open.close_suspended().unwrap();

    assert!(!scratch.dir().join(format!("{id}.session")).exists());
    assert!(!files_dir(&scratch, &id).exists());
}

fn page_snapshot(number: u32, kind: SnapshotKind, html: &str) -> PageSnapshot {
    PageSnapshot {
        number,
        kind,
        page: "/products?x=1".to_string(),
        width: 390,
        html: html.to_string(),
        description: Some(serde_json::json!("H4sIAAAA")),
    }
}

fn no_comments() -> BTreeSet<String> {
    BTreeSet::new()
}

fn saved_path(write: &FilesWrite) -> PathBuf {
    match &write.written {
        FileWritten::Saved(path) => path.clone(),
        other @ (FileWritten::NoRoom | FileWritten::Failed(_)) => {
            panic!("not saved: {other:?}")
        }
    }
}

#[test]
fn a_snapshot_reads_back_the_same_from_another_open_session() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    let start = page_snapshot(1, SnapshotKind::Start, "<p>start</p>");
    let manual = PageSnapshot {
        width: 1280,
        description: None,
        ..page_snapshot(2, SnapshotKind::Manual, "<p>manual</p>")
    };
    saved_path(&open.save_snapshot(&start, &no_comments()));
    saved_path(&open.save_snapshot(&manual, &no_comments()));
    let id = open.id().to_string();
    drop(open);

    let opened = store.open(&id).unwrap();
    let read: Vec<PageSnapshot> = opened
        .read_snapshots()
        .into_iter()
        .map(Result::unwrap)
        .collect();

    assert_eq!(read, vec![start, manual]);
}

#[test]
fn a_snapshot_is_written_into_its_files_directory_for_the_owner_only() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();

    let path = saved_path(&open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, "<p></p>"),
        &no_comments(),
    ));

    assert!(path.starts_with(std::path::absolute(files_dir(&scratch, open.id())).unwrap()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&files_dir(&scratch, open.id())), 0o700);
    }
}

#[test]
fn an_unreadable_snapshot_file_does_not_hide_the_others() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let start = page_snapshot(1, SnapshotKind::Start, "<p>start</p>");
    open.save_snapshot(&start, &no_comments());
    let broken = saved_path(&open.save_snapshot(
        &page_snapshot(2, SnapshotKind::Manual, "<p>manual</p>"),
        &no_comments(),
    ));
    std::fs::write(&broken, b"not gzip").unwrap();

    let read = open.read_snapshots();

    assert_eq!(read.len(), 2);
    assert_eq!(read[0].as_ref().unwrap(), &start);
    assert!(read[1].is_err());
}

#[test]
fn the_last_snapshot_number_counts_snapshot_files_that_cannot_be_read() {
    // 強制終了で途中まで書いたファイルも番号を持つ。復元後の新しいスナップショットが
    // その番号を使うと、20 MB の規則がそのファイルを消したときに取り違える（R-PAGE-SESSION）。
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, "<p>start</p>"),
        &no_comments(),
    );
    let broken = saved_path(&open.save_snapshot(
        &page_snapshot(2, SnapshotKind::Manual, "<p>manual</p>"),
        &no_comments(),
    ));
    std::fs::write(&broken, b"not gzip").unwrap();

    assert_eq!(open.last_snapshot_number(), 2);
}

#[test]
fn writing_into_a_full_files_directory_removes_files_by_the_rule_first() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir()).with_files_limit(3000);
    let open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let html = |seed: u8| String::from_utf8(incompressible_text(1000, seed)).unwrap();
    open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, &html(1)),
        &no_comments(),
    );
    let handed = saved_path(&open.save_snapshot(
        &page_snapshot(2, SnapshotKind::Handed, &html(2)),
        &no_comments(),
    ));
    let manual = saved_path(&open.save_snapshot(
        &page_snapshot(3, SnapshotKind::Manual, &html(3)),
        &no_comments(),
    ));

    let write = open.save_snapshot(
        &page_snapshot(4, SnapshotKind::Manual, &html(4)),
        &no_comments(),
    );

    assert_eq!(write.removed_snapshots, vec![2]);
    assert!(!handed.exists());
    assert!(manual.exists());
    assert!(saved_path(&write).exists());
}

#[test]
fn a_snapshot_that_does_not_fit_beside_the_start_snapshot_and_current_images_is_not_written() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir()).with_files_limit(3000);
    let open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let html = |seed: u8| String::from_utf8(incompressible_text(1200, seed)).unwrap();
    open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, &html(1)),
        &no_comments(),
    );
    let comments: BTreeSet<String> = ["c1".to_string()].into_iter().collect();
    saved_path(&open.save_image("c1", &incompressible(1200), &comments));

    let write = open.save_snapshot(&page_snapshot(2, SnapshotKind::Manual, &html(2)), &comments);

    assert!(matches!(write.written, FileWritten::NoRoom), "{write:?}");
    assert!(open.read_snapshots().len() == 1);
}

#[test]
fn a_comment_image_makes_room_by_removing_images_of_deleted_comments() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir()).with_files_limit(2500);
    let open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let first = saved_path(&open.save_image("c1", &incompressible(1200), &no_comments()));
    let comments: BTreeSet<String> = ["c2".to_string()].into_iter().collect();
    let second = saved_path(&open.save_image("c2", &incompressible(1200), &comments));

    let third = open.save_image("c3", &incompressible(1200), &comments);

    assert!(!first.exists());
    assert!(second.exists());
    assert_eq!(
        std::fs::read(saved_path(&third)).unwrap(),
        incompressible(1200)
    );
}

#[test]
fn snapshots_of_a_live_session_without_a_conversation_stay_until_it_is_suspended() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let path = saved_path(&open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, "<p></p>"),
        &no_comments(),
    ));
    let seen_only = SessionState {
        seen: ["f1".to_string()].into_iter().collect(),
        ..SessionState::default()
    };

    open.save_state_at(seen_only, 200).unwrap();
    assert!(path.exists());

    open.close_suspended().unwrap();
    assert!(!files_dir(&scratch, open.id()).exists());
}

#[test]
fn a_suspended_live_session_writes_nothing_more_into_its_files_directory() {
    // 保留で片付けた後に、まだ処理中だった要求のスナップショットや画像が届いても、
    // 消した `<id>.files/` を作り直さない（R-PAGE-SESSION）。
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    saved_path(&open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, "<p></p>"),
        &no_comments(),
    ));
    open.close_suspended().unwrap();

    let snapshot = open.save_snapshot(
        &page_snapshot(2, SnapshotKind::Manual, "<p></p>"),
        &no_comments(),
    );
    let image = open.save_image("c1", b"png", &no_comments());

    assert!(matches!(snapshot.written, FileWritten::Failed(_)));
    assert!(matches!(image.written, FileWritten::Failed(_)));
    assert!(!files_dir(&scratch, open.id()).exists());
}

#[test]
fn snapshots_of_a_live_session_with_a_conversation_stay_after_it_is_suspended() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let path = saved_path(&open.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, "<p></p>"),
        &no_comments(),
    ));
    open.save_state_at(state_with_comment(), 200).unwrap();

    open.close_suspended().unwrap();

    assert!(path.exists());
}

#[test]
fn a_session_removed_by_the_cleanup_loses_its_files_directory() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    for (id, updated) in [
        ("01HF7YAT00AAAAAAAAAAAAAAAA", 100),
        ("01HF7YAT00BBBBBBBBBBBBBBBB", 200),
    ] {
        let mut open = store.create(live_info(id, updated)).unwrap();
        open.save_state_at(state_with_comment(), updated).unwrap();
        saved_path(&open.save_image("c1", b"png", &no_comments()));
        drop(open);
    }

    cleanup(&scratch.dir(), 1, u64::MAX);

    assert!(!files_dir(&scratch, "01HF7YAT00AAAAAAAAAAAAAAAA").exists());
    assert!(files_dir(&scratch, "01HF7YAT00BBBBBBBBBBBBBBBB").exists());
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
    open.save_copy_with_limit(only_in_the_copy, 300, COPY_LIMIT)
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
fn session_without_a_copy_is_not_resumable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    let id = open.id().to_string();
    drop(open);

    let stored = store.read(&id).unwrap();
    assert_eq!(stored.copy, CopyState::Pending);
    assert!(matches!(
        store.open(&id),
        Err(SessionError::NotReady { .. })
    ));
}

#[test]
fn session_without_its_copy_file_is_not_listed_and_cannot_be_opened() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();
    let id = open.id().to_string();
    drop(open);
    std::fs::remove_file(scratch.dir().join(format!("{id}.payload"))).unwrap();

    assert!(store.list().unwrap().is_empty());
    assert!(matches!(
        store.open(&id),
        Err(SessionError::Unusable { .. })
    ));
}

#[test]
fn session_copy_is_stored_as_gzip() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
    let id = open.id().to_string();
    drop(open);

    let bytes = std::fs::read(scratch.dir().join(format!("{id}.payload"))).unwrap();
    // 写しのファイルは全体が 1 つの gzip で、目印も版も持たない。
    assert_eq!(&bytes[..3], &[0x1f, 0x8b, 0x08]);
    let inflated = encoding::gunzip(&bytes, encoding::DECOMPRESS_LIMIT).unwrap();

    assert_eq!(super::super::decode_copy(&inflated).unwrap(), copy());
}

#[test]
fn session_state_write_leaves_the_copy_file_untouched() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
    let id = open.id().to_string();
    let payload = scratch.dir().join(format!("{id}.payload"));
    // gzip として読めないバイト列に置き換える。状態の保存がここを書き直すなら
    // 中身が変わり、読むなら失敗する。
    std::fs::write(&payload, b"not a gzip").unwrap();

    for updated in [300, 400, 500] {
        open.save_state_at(state_with_comment(), updated).unwrap();
    }

    assert_eq!(std::fs::read(&payload).unwrap(), b"not a gzip");
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
        open.save_copy_with_limit(frozen, 200, COPY_LIMIT).unwrap();
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
        let envelope: serde_json::Value = serde_json::from_slice(&bytes[18..]).unwrap();
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

#[cfg(unix)]
#[test]
fn session_copy_that_cannot_be_written_does_not_become_resumable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    // 写しのファイルを置けなくする（同じ名前のディレクトリがあると rename が
    // 失敗する）。ディスクが一杯のときと同じ経路になる。
    std::fs::create_dir_all(scratch.dir().join(format!("{id}.payload"))).unwrap();

    assert!(open.save_copy_with_limit(copy(), 300, COPY_LIMIT).is_err());

    // 写しを置けていないので「使える」にしない。この後に状態を書いても、
    // 「使える」と書いた `<id>.session` は残らない（R-SESSION の不変条件）。
    assert!(!open.is_resumable());
    open.save_state_at(state_with_comment(), 400).unwrap();
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn session_copy_over_the_limit_is_unusable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 110).unwrap();
    let mut oversized = copy();
    oversized.contents.insert(
        "big".to_string(),
        content(None, Some(&incompressible(1024))),
    );
    // 上限は `<id>.payload` 全体で判定する（R-SESSION）。変更ファイルの内容だけで
    // ちょうど収まる上限を渡し、凍結したレビューのメタデータの分で超えさせる。
    let contents_only = encoding::gzip(&encoding::encode_contents(&oversized.contents)).len();
    open.save_copy_with_limit(oversized, 200, contents_only as u64)
        .unwrap();
    let id = open.id().to_string();
    assert!(matches!(open.copy(), CopyState::Unusable(_)));
    drop(open);

    let stored = store.read(&id).unwrap();
    assert!(matches!(stored.copy, CopyState::Unusable(_)));
    assert!(matches!(
        store.open(&id),
        Err(SessionError::Unusable { .. })
    ));
}

/// gzip でほとんど縮まない ASCII の文字列（`seed` ごとに違う）。
fn incompressible_text(size: usize, seed: u8) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = incompressible(size + usize::from(seed));
    bytes.drain(..usize::from(seed));
    bytes
        .into_iter()
        .map(|byte| ALPHABET[usize::from(byte) % ALPHABET.len()])
        .collect()
}

fn incompressible(size: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_9abc_def0u64;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xff) as u8
        })
        .collect()
}

#[test]
fn session_rejects_an_unknown_format_version() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    std::fs::create_dir_all(scratch.dir()).unwrap();
    let id = "01HF7YAT00BBBBBBBBBBBBBBBB";
    write_atomic(
        &scratch.dir(),
        &scratch.dir().join(format!("{id}.session")),
        &encoding::encode_session(99, b"{}"),
    )
    .unwrap();

    assert!(matches!(
        store.read(id),
        Err(SessionError::UnsupportedVersion { version: 99, .. })
    ));
}

#[test]
fn only_sessions_of_an_unreadable_version_are_reported() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());

    // セッションを 1 つも保留していない（ディレクトリが無い）。
    assert!(!store.has_unreadable_version());

    // 今の版のセッションは読めるので、報告の対象にしない。
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();
    drop(open);
    assert!(!store.has_unreadable_version());

    // 版 1 は読み手が無い。一覧には出ないが、あることは分かる。
    let old = "01HF7YAT00BBBBBBBBBBBBBBBB";
    write_atomic(
        &scratch.dir(),
        &scratch.dir().join(format!("{old}.session")),
        &encoding::encode_session(1, b"{}"),
    )
    .unwrap();

    assert!(store.has_unreadable_version());
    assert!(
        store
            .list()
            .unwrap()
            .iter()
            .all(|summary| summary.id != old)
    );
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
fn session_without_state_and_an_unusable_copy_is_not_kept() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_copy_with_limit(copy(), 200, 1).unwrap();

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
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
    open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
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
fn session_cleanup_keeps_the_newest_sessions() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    for (id, updated) in [
        ("01HF7YAT00AAAAAAAAAAAAAAAA", 100),
        ("01HF7YAT00BBBBBBBBBBBBBBBB", 200),
        ("01HF7YAT00CCCCCCCCCCCCCCCC", 300),
    ] {
        let mut open = store.create(info(id, updated)).unwrap();
        open.save_state_at(state_with_comment(), updated).unwrap();
        drop(open);
    }

    cleanup(&scratch.dir(), 2, u64::MAX);

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
    assert!(store.read("01HF7YAT00CCCCCCCCCCCCCCCC").is_ok());
}

#[test]
fn session_cleanup_keeps_the_total_size() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    // 古い方が大きく、新しい方が小さい。合計の上限で古い方だけが消える。
    let mut old = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let mut large = copy();
    large.contents.insert(
        "big".to_string(),
        content(None, Some(&incompressible(20_000))),
    );
    old.save_copy_with_limit(large, 100, COPY_LIMIT).unwrap();
    drop(old);
    let mut newest = store
        .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
        .unwrap();
    newest.save_state_at(state_with_comment(), 200).unwrap();
    drop(newest);
    let size_of = |name: &str| std::fs::metadata(scratch.dir().join(name)).unwrap().len();
    // `<id>.session` だけを数えるなら 2 件とも収まる上限。古い方の
    // `<id>.payload` を足して初めて超える（R-SESSION: 合計は 2 ファイルの合算）。
    let limit = size_of("01HF7YAT00AAAAAAAAAAAAAAAA.session")
        + size_of("01HF7YAT00BBBBBBBBBBBBBBBB.session")
        + 10;

    cleanup(&scratch.dir(), KEEP_SESSIONS, limit);

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
}

#[test]
fn session_cleanup_keeps_a_locked_session_out_of_the_count() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut locked = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    locked
        .save_copy_with_limit(copy(), 100, COPY_LIMIT)
        .unwrap();
    let mut middle = store
        .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
        .unwrap();
    middle
        .save_copy_with_limit(copy(), 200, COPY_LIMIT)
        .unwrap();
    let mut newest = store
        .create(info("01HF7YAT00CCCCCCCCCCCCCCCC", 300))
        .unwrap();
    newest
        .save_copy_with_limit(copy(), 300, COPY_LIMIT)
        .unwrap();

    // 最古を開いたまま 2 件だけ残す。ロック中は消えず、件数にも数えないので、
    // 2 番目に古いものも残る。
    cleanup(&scratch.dir(), 2, u64::MAX);

    assert!(store.read("01HF7YAT00AAAAAAAAAAAAAAAA").is_ok());
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
    assert!(store.read("01HF7YAT00CCCCCCCCCCCCCCCC").is_ok());
}

#[test]
fn session_cleanup_removes_a_payload_without_its_session() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let held = store
        .create(info("01HF7YAT00DDDDDDDDDDDDDDDD", 100))
        .unwrap();
    std::fs::write(
        scratch.dir().join("01HF7YAT00CCCCCCCCCCCCCCCC.payload"),
        b"orphan",
    )
    .unwrap();
    // 起動中のレビューは `<id>.payload` を書いてから `<id>.session` を書くので、
    // その間だけ同じ形になる。ロックを持っているものは消さない（R-SESSION）。
    let held_payload = scratch.dir().join("01HF7YAT00DDDDDDDDDDDDDDDD.payload");
    std::fs::write(&held_payload, b"being written").unwrap();

    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 200))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();

    assert!(
        !scratch
            .dir()
            .join("01HF7YAT00CCCCCCCCCCCCCCCC.payload")
            .exists()
    );
    assert!(held_payload.exists());
    drop(held);
}

#[test]
fn session_cleanup_counts_the_files_directory_in_the_total_size() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut old = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    old.save_state_at(state_with_comment(), 100).unwrap();
    saved_path(&old.save_image("c1", &incompressible(20_000), &no_comments()));
    drop(old);
    let mut newest = store
        .create(live_info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
        .unwrap();
    newest.save_state_at(state_with_comment(), 200).unwrap();
    drop(newest);
    let size_of = |name: &str| std::fs::metadata(scratch.dir().join(name)).unwrap().len();
    // `<id>.session` だけなら 2 件とも収まる上限。古い方の `<id>.files/` を足して
    // 初めて超える（R-SESSION: 合計には `<id>.files/` も数える）。
    let limit = size_of("01HF7YAT00AAAAAAAAAAAAAAAA.session")
        + size_of("01HF7YAT00BBBBBBBBBBBBBBBB.session")
        + 10;

    cleanup(&scratch.dir(), KEEP_SESSIONS, limit);

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
    assert!(!files_dir(&scratch, "01HF7YAT00AAAAAAAAAAAAAAAA").exists());
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
}

#[test]
fn session_cleanup_removes_a_files_directory_without_its_session() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let held = store
        .create(live_info("01HF7YAT00DDDDDDDDDDDDDDDD", 100))
        .unwrap();
    // 起動中のレビューは会話が無い間も `<id>.files/` にスナップショットを書く。
    // ロックを持っているものは消さない。
    let held_snapshot = saved_path(&held.save_snapshot(
        &page_snapshot(1, SnapshotKind::Start, "<p></p>"),
        &no_comments(),
    ));
    let left = files_dir(&scratch, "01HF7YAT00CCCCCCCCCCCCCCCC");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("c1.png"), b"left by a killed review").unwrap();

    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 200))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();

    assert!(!left.exists());
    assert!(held_snapshot.exists());
    drop(held);
}

#[test]
fn session_cleanup_evicts_a_stale_pair_by_the_limits() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut stale = store.create(info(id, 100)).unwrap();
    stale.save_state_at(state_with_comment(), 100).unwrap();
    stale.save_copy_with_limit(copy(), 100, COPY_LIMIT).unwrap();
    let session = scratch.dir().join(format!("{id}.session"));
    let payload = scratch.dir().join(format!("{id}.payload"));
    // 強制終了で残りうる形。「使える」でない `<id>.session` と `<id>.payload` が
    // 揃っている（公開 API だけでは作れないので、写しを置き直して作る）。
    let written = std::fs::read(&payload).unwrap();
    stale.mark_unresumable_at("interrupted", 100).unwrap();
    std::fs::write(&payload, &written).unwrap();
    drop(stale);
    let mut newest = store
        .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
        .unwrap();
    newest.save_state_at(state_with_comment(), 200).unwrap();
    drop(newest);

    cleanup(&scratch.dir(), 1, u64::MAX);

    assert!(!session.exists());
    assert!(!payload.exists());
}

#[test]
fn session_dropping_the_copy_removes_the_copy_file() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();

    open.mark_unresumable_at("cannot read the contents", 400)
        .unwrap();

    // 状態は残るので `<id>.session` は残り、写しのファイルだけが消える。
    assert!(scratch.dir().join(format!("{id}.session")).exists());
    assert!(!scratch.dir().join(format!("{id}.payload")).exists());

    // 状態が空のまま写しが使えなくなる枝では、セッションごと消えて写しも残らない。
    let other = "01HF7YAT00BBBBBBBBBBBBBBBB";
    let mut empty = store.create(info(other, 100)).unwrap();
    empty.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();

    empty
        .mark_unresumable_at("cannot read the contents", 300)
        .unwrap();

    assert!(!scratch.dir().join(format!("{other}.session")).exists());
    assert!(!scratch.dir().join(format!("{other}.payload")).exists());
}

#[test]
fn session_lock_is_exclusive_until_dropped() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let first = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();

    let second = store.create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100));
    assert!(matches!(second, Err(SessionError::Locked { .. })));

    drop(first);
    assert!(
        store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .is_ok()
    );
}

#[test]
fn session_list_orders_by_updated_and_skips_unusable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut older = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    older.save_state_at(state_with_comment(), 100).unwrap();
    older.save_copy_with_limit(copy(), 100, COPY_LIMIT).unwrap();
    let mut newer = store
        .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
        .unwrap();
    newer.save_state_at(state_with_comment(), 200).unwrap();
    newer.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
    let mut unusable = store
        .create(info("01HF7YAT00CCCCCCCCCCCCCCCC", 300))
        .unwrap();
    unusable.save_state_at(state_with_comment(), 300).unwrap();
    unusable.save_copy_with_limit(copy(), 300, 1).unwrap();

    let listed = store.list().unwrap();

    assert_eq!(
        listed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["01HF7YAT00BBBBBBBBBBBBBBBB", "01HF7YAT00AAAAAAAAAAAAAAAA"]
    );
    assert_eq!(listed[0].seen, 1);
    assert_eq!(listed[0].total_files, 2);
}

#[test]
fn session_cleanup_removes_orphan_lock_files() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(scratch.dir()).unwrap();
    let lock_path = scratch.dir().join("01HF7YAT00AAAAAAAAAAAAAAAA.lock");
    std::fs::write(&lock_path, b"").unwrap();
    let written = modified(&lock_path);

    // 新しすぎるロックは、始まったばかりのセッションと競合しないよう残す。
    prune_orphan_locks(
        &scratch.dir(),
        written + ORPHAN_LOCK_MIN_AGE - Duration::from_secs(1),
        ORPHAN_LOCK_MIN_AGE,
    );
    assert!(lock_path.exists());

    // ロック中のものは他のプロセスのものなので残す。
    let held = SessionLock::acquire(&scratch.dir(), "01HF7YAT00AAAAAAAAAAAAAAAA").unwrap();
    prune_orphan_locks(
        &scratch.dir(),
        written + ORPHAN_LOCK_MIN_AGE,
        ORPHAN_LOCK_MIN_AGE,
    );
    assert!(lock_path.exists());
    drop(held);

    // 使われていない古いロックは、猶予ちょうどで消える。
    std::fs::write(&lock_path, b"").unwrap();
    let written = modified(&lock_path);
    prune_orphan_locks(
        &scratch.dir(),
        written + ORPHAN_LOCK_MIN_AGE,
        ORPHAN_LOCK_MIN_AGE,
    );
    assert!(!lock_path.exists());
}

#[test]
fn session_cleanup_removes_an_orphan_lock_even_within_the_limits() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    drop(open);
    // 強制終了したレビューが残したロック。セッションは上限の内側なので、
    // 消すセッションが無くても毎回片付ける。
    let orphan = scratch.dir().join("01HF7YAT00BBBBBBBBBBBBBBBB.lock");
    std::fs::write(&orphan, b"").unwrap();

    cleanup_at(
        &scratch.dir(),
        KEEP_SESSIONS,
        KEEP_BYTES,
        modified(&orphan) + ORPHAN_LOCK_MIN_AGE,
    );

    assert!(!orphan.exists());
    assert!(store.read(id).is_ok());
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn session_delete_removes_the_file() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
    let id = open.id().to_string();

    open.delete().unwrap();

    assert!(matches!(
        store.read(&id),
        Err(SessionError::NotFound { .. })
    ));
    assert!(!scratch.dir().join(format!("{id}.payload")).exists());
}

#[test]
fn session_open_rejects_an_id_that_is_not_a_ulid_and_keeps_the_lock_alone() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    std::fs::create_dir_all(&scratch.0).unwrap();
    let sentinel = scratch.0.join("sentinel.lock");
    std::fs::write(&sentinel, b"keep").unwrap();

    let result = store.open("../sentinel");

    assert!(matches!(result, Err(SessionError::NotFound { .. })));
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
}

#[test]
fn session_read_rejects_an_id_that_is_not_a_ulid() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 100).unwrap();
    let id = open.id().to_string();
    drop(open);
    let outside = scratch.0.join("sentinel.session");
    std::fs::copy(scratch.dir().join(format!("{id}.session")), &outside).unwrap();

    let result = store.read("../sentinel");

    assert!(matches!(result, Err(SessionError::NotFound { .. })));
    assert!(outside.exists());
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
    open.save_copy_with_limit(copy(), 400, COPY_LIMIT).unwrap();
    let write = open.save_image("c1", b"image", &BTreeSet::new());

    assert!(matches!(write.written, FileWritten::Failed(_)));
    assert!(!session_path(&scratch.dir(), id).exists());
    assert!(!payload_path(&scratch.dir(), id).exists());
    assert!(!files_path(&scratch.dir(), id).exists());
}

#[test]
fn session_is_not_recreated_by_a_copy_saved_after_delete() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.delete().unwrap();

    // submit と競争した凍結が、消えたあとに写しを保存する。
    open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
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
    open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
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
fn reply(id: &str, author: Author, body: &str) -> Reply {
    Reply {
        id: id.to_string(),
        seq: 0,
        author,
        body: body.into(),
    }
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

/// セッション形式の版 2 の kemi が書いた `<id>.session`（返信は文字列の並び、
/// 写しは「使える」）。版 3 の読み手を足す前の `encode_session(2, …)` で作った。
const VERSION_2_SESSION: &[u8] = b"kemi-session\n\x02\x6b\x03\x00\x00\
        {\"info\":{\"id\":\"01HF7YAT00CCCCCCCCCCCCCCCC\",\"created\":1700000000000,\
        \"updated\":1700000002000,\"workspace\":\"/tmp/workspace\",\"workspace_key\":\"00000000000000aa\",\
        \"mode\":{\"kind\":\"worktree\"},\"title\":\"Working tree changes\",\"total_files\":2},\
        \"state\":{\"comments\":[{\"id\":\"c1\",\"file_id\":\"f1\",\"group_id\":\"all\",\
        \"group_title\":\"final\",\"path\":\"src/a.rs\",\"side\":\"new\",\"start_line\":1,\
        \"end_line\":2,\"quote\":[\"one\",\"two\"],\"body\":\"please change\",\"replies\":[\"first\",\
        \"second\"],\"resolved\":true,\"outdated\":false,\"content_hash\":\"hash\",\
        \"suggestion\":\"replaced\"},{\"id\":\"c2\",\"file_id\":\"f1\",\"group_id\":\"all\",\
        \"group_title\":\"final\",\"path\":\"src/a.rs\",\"side\":\"new\",\"start_line\":1,\
        \"end_line\":2,\"quote\":[\"one\",\"two\"],\"body\":\"please change\",\"replies\":[\"third\"],\
        \"resolved\":false,\"outdated\":false,\"content_hash\":\"hash\",\"suggestion\":null}],\
        \"seen\":[\"f1\"],\"collapsed\":{\"f1\":true},\"last_comment\":2},\"copy\":{\"state\":\"ready\"}}";

const VERSION_2_ID: &str = "01HF7YAT00CCCCCCCCCCCCCCCC";

fn place_version_2_session(scratch: &Scratch) {
    std::fs::create_dir_all(scratch.dir()).unwrap();
    std::fs::write(
        scratch.dir().join(format!("{VERSION_2_ID}.session")),
        VERSION_2_SESSION,
    )
    .unwrap();
}

#[test]
fn version_2_session_reads_its_string_replies_as_reviewer_replies() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    place_version_2_session(&scratch);

    let state = store.read(VERSION_2_ID).unwrap().state;

    // 版 2 の画面からは返信を書けず、API で書けたのも画面の側（人間）だけだった。
    // id は読むときに、コメントの順・返信の順に振る。
    assert_eq!(
        state.comments[0].replies,
        vec![
            Reply {
                seq: 2,
                ..reply("r1", Author::Reviewer, "first")
            },
            Reply {
                seq: 3,
                ..reply("r2", Author::Reviewer, "second")
            },
        ]
    );
    assert_eq!(
        state.comments[1].replies,
        vec![Reply {
            seq: 5,
            ..reply("r3", Author::Reviewer, "third")
        }]
    );
    assert_eq!(state.last_reply, 3);
    assert!(state.comments[0].resolved);
    assert_eq!(state.last_comment, 2);
    // 版 2 に無い項目は空として読む。
    assert!(state.messages.is_empty());
    assert_eq!(state.last_message, 0);
    assert_eq!(state.channel, Channel::default());
    assert!(!state.page_comment_saved);
}

#[test]
fn version_2_session_numbers_each_comment_then_its_replies_in_creation_order() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    place_version_2_session(&scratch);

    let state = store.read(VERSION_2_ID).unwrap().state;

    let numbers: Vec<u32> = state
        .comments
        .iter()
        .flat_map(|comment| {
            std::iter::once(comment.seq).chain(comment.replies.iter().map(|reply| reply.seq))
        })
        .collect();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);
    assert_eq!(state.last_seq, 5);
}

#[test]
fn version_2_session_is_listed_and_resumable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    place_version_2_session(&scratch);
    // 写しのファイルは版を持たないので、今の書き方で置いてよい。
    let payload = encoding::gzip(&super::super::encode_copy(&copy()).unwrap());
    std::fs::write(
        scratch.dir().join(format!("{VERSION_2_ID}.payload")),
        payload,
    )
    .unwrap();

    let listed: Vec<String> = store
        .list()
        .unwrap()
        .into_iter()
        .map(|summary| summary.id)
        .collect();

    assert_eq!(listed, vec![VERSION_2_ID.to_string()]);
    assert!(!store.has_unreadable_version());
    let opened = store.open(VERSION_2_ID).unwrap();
    assert_eq!(opened.copy(), &CopyState::Ready(copy()));
    assert_eq!(opened.state().comments.len(), 2);
}

#[test]
fn session_of_a_version_without_a_reader_is_unsupported_and_reported() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    std::fs::create_dir_all(scratch.dir()).unwrap();
    let id = "01HF7YAT00BBBBBBBBBBBBBBBB";
    std::fs::write(
        scratch.dir().join(format!("{id}.session")),
        encoding::encode_session(99, b"{}"),
    )
    .unwrap();

    assert!(matches!(
        store.read(id),
        Err(SessionError::UnsupportedVersion { version: 99, .. })
    ));
    assert!(store.has_unreadable_version());
}

#[test]
fn endpoint_roundtrips_and_is_owner_only() {
    use crate::session::{Endpoint, read_endpoint, write_endpoint};
    let scratch = Scratch::new();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let endpoint = Endpoint {
        port: 41234,
        token: "secret".to_string(),
    };

    write_endpoint(&scratch.dir(), id, &endpoint).unwrap();

    assert_eq!(read_endpoint(&scratch.dir(), id).unwrap(), Some(endpoint));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(crate::session::endpoint_path(&scratch.dir(), id))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    assert_eq!(
        read_endpoint(&scratch.dir(), "01HF7YAT00BBBBBBBBBBBBBBBB").unwrap(),
        None
    );
}

#[test]
fn endpoint_is_removed_with_the_session() {
    use crate::session::{Endpoint, endpoint_path, write_endpoint};
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
    let id = open.id().to_string();
    let endpoint = Endpoint {
        port: 1,
        token: "t".to_string(),
    };
    write_endpoint(&scratch.dir(), &id, &endpoint).unwrap();

    open.delete().unwrap();

    assert!(!endpoint_path(&scratch.dir(), &id).exists());
}

#[test]
fn a_running_session_is_told_apart_from_a_dead_one() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let open = store.create(info(id, 100)).unwrap();

    assert!(store.is_running(id));
    drop(open);
    assert!(!store.is_running(id));
    assert!(!store.is_running("not-an-id"));
}

#[test]
fn cleanup_removes_an_endpoint_left_by_a_review_that_is_gone() {
    use crate::session::{Endpoint, endpoint_path, write_endpoint};
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let endpoint = Endpoint {
        port: 1,
        token: "t".to_string(),
    };
    let stale = "01HF7YAT00BBBBBBBBBBBBBBBB";
    write_endpoint(&scratch.dir(), stale, &endpoint).unwrap();
    let mut running = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    write_endpoint(&scratch.dir(), running.id(), &endpoint).unwrap();

    running.save_state_at(state_with_comment(), 200).unwrap();

    assert!(!endpoint_path(&scratch.dir(), stale).exists());
    assert!(endpoint_path(&scratch.dir(), running.id()).exists());
}

fn live_info(id: &str, updated: u128) -> SessionInfo {
    SessionInfo {
        mode: SessionMode::Live {
            page: crate::domain::live::LivePage::Url("http://127.0.0.1:5173/".to_string()),
            root: PathBuf::from("/tmp/workspace"),
        },
        title: "Live review of http://127.0.0.1:5173/".to_string(),
        ..info(id, updated)
    }
}

#[test]
fn live_session_is_listed_and_opened_without_a_copy() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    assert!(open.is_resumable());
    let id = open.id().to_string();
    drop(open);

    let listed = store.list().unwrap();
    let opened = store.open(&id).unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].mode_label(), "live http://127.0.0.1:5173/");
    assert_eq!(opened.info().mode, live_info(&id, 0).mode);
    assert_eq!(opened.state(), &state_with_comment());
    assert!(!scratch.dir().join(format!("{id}.payload")).exists());
}

#[test]
fn live_session_without_a_comment_or_a_message_is_not_kept() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let seen_only = SessionState {
        comments: Vec::new(),
        last_comment: 0,
        last_reply: 0,
        ..state_with_comment()
    };

    open.save_state_at(seen_only, 200).unwrap();

    assert!(!open.is_resumable());
    assert!(
        !scratch
            .dir()
            .join(format!("{}.session", open.id()))
            .exists()
    );
}

#[test]
fn live_session_with_only_a_message_is_kept() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let message_only = SessionState {
        messages: vec![Message {
            id: "m1".to_string(),
            seq: 1,
            author: Author::Reviewer,
            body: "hello".into(),
        }],
        last_message: 1,
        last_seq: 1,
        ..SessionState::default()
    };

    open.save_state_at(message_only, 200).unwrap();

    assert!(open.is_resumable());
    assert!(
        scratch
            .dir()
            .join(format!("{}.session", open.id()))
            .exists()
    );
}

#[test]
fn live_session_roundtrips_its_mock_assignments() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let state = SessionState {
        mocks: [(
            "/products?x=1".to_string(),
            "mocks/products.html".to_string(),
        )]
        .into_iter()
        .collect(),
        ..state_with_comment()
    };
    open.save_state_at(state.clone(), 200).unwrap();
    let id = open.id().to_string();
    drop(open);

    let opened = store.open(&id).unwrap();

    assert_eq!(opened.state(), &state);
}

#[test]
fn live_session_roundtrips_that_a_page_comment_was_once_saved() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    let state = SessionState {
        page_comment_saved: true,
        ..state_with_comment()
    };
    open.save_state_at(state.clone(), 200).unwrap();
    let id = open.id().to_string();
    drop(open);

    let opened = store.open(&id).unwrap();

    assert_eq!(opened.state(), &state);
}

#[test]
fn version_3_session_written_before_the_page_comment_mark_reads_as_never_saved() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    let id = open.id().to_string();
    drop(open);
    // 印を足す前の版 3 の kemi は、この項目を書かなかった。
    let path = scratch.dir().join(format!("{id}.session"));
    let bytes = std::fs::read(&path).unwrap();
    let (version, meta) = encoding::decode_session(&bytes).unwrap();
    let mut meta: serde_json::Value = serde_json::from_slice(meta).unwrap();
    meta["state"]
        .as_object_mut()
        .unwrap()
        .remove("page_comment_saved");
    assert_eq!(version, 4);
    std::fs::write(
        &path,
        encoding::encode_session(3, &serde_json::to_vec(&meta).unwrap()),
    )
    .unwrap();

    let state = store.read(&id).unwrap().state;

    assert!(!state.page_comment_saved);
    assert_eq!(state.comments, state_with_comment().comments);
}

#[test]
fn live_session_roundtrips_a_file_page_and_its_root() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut info = live_info("01HF7YAT00AAAAAAAAAAAAAAAA", 100);
    info.mode = SessionMode::Live {
        page: crate::domain::live::LivePage::File("docs/mock.html".to_string()),
        root: PathBuf::from("/tmp/outside-git"),
    };
    let mut open = store.create(info.clone()).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    drop(open);

    let opened = store.open(&info.id).unwrap();

    assert_eq!(opened.info().mode, info.mode);
    assert_eq!(opened.info().mode.label(""), "live /docs/mock.html");
}
