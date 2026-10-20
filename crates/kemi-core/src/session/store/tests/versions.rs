//! 版ごとの `<id>.session` の読み取り（R-SESSION）。古い版の補いと、壊れた値・読み手の無い版の断り。

use super::super::*;
use super::fixtures::*;
use crate::domain::agent::Channel;
use crate::domain::review::{Author, Reply};

#[test]
fn malformed_legacy_containers_are_reported_as_corrupt_instead_of_panicking() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    std::fs::create_dir_all(scratch.dir()).unwrap();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let state = SessionState::default();
    let meta = MetaDto::from_parts(&info(id, 100), &state, &CopyStatus::Pending);
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
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    drop(store.create(info(id, 100)).unwrap());
    let mut meta = serde_json::to_value(MetaDto::from_parts(
        &info(id, 100),
        &state_with_comment(),
        &CopyStatus::Pending,
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
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    drop(store.create(info(id, 222)).unwrap());
    let mut oldcopy = copy();
    oldcopy.startup_unit = None;
    oldcopy.units[0].unit = None;
    let payload = encoding::gzip(&crate::session::encode_copy(&oldcopy).unwrap());
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
fn malformed_parts_are_rejected_from_version_four_instead_of_becoming_plain() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    drop(store.create(info(id, 100)).unwrap());
    let mut value = serde_json::to_value(MetaDto::from_parts(
        &info(id, 100),
        &state_with_comment(),
        &CopyStatus::Pending,
    ))
    .unwrap();
    let write = |value: &serde_json::Value| {
        std::fs::write(
            session_path(&scratch.dir(), id),
            encoding::encode_session(4, &serde_json::to_vec(value).unwrap()),
        )
        .unwrap();
    };
    value["state"]["comments"][0]["replies"][0]["body"] = serde_json::Value::Null;
    value["state"]["comments"][0]["replies"][0]["parts"] =
        serde_json::json!([{"type":"text","body":"valid"}]);
    write(&value);
    assert!(store.read(id).is_ok(), "the same reply with valid parts");
    for parts in [
        serde_json::json!([]),
        serde_json::json!([{"type":"text","body":null}]),
        serde_json::json!([{"type":"code","body":"x","language":"x","role":"example","title":null}]),
    ] {
        value["state"]["comments"][0]["replies"][0]["parts"] = parts;
        write(&value);
        let result = store.read(id);
        assert!(
            matches!(result, Err(SessionError::Corrupt { .. })),
            "{result:?}"
        );
    }
    value["state"]["comments"][0]["replies"][0]["parts"] =
        serde_json::json!([{"type":"text","body":"valid"}]);
    value["state"]["comments"][0]["replies"][0]
        .as_object_mut()
        .unwrap()
        .remove("body");
    write(&value);
    let result = store.read(id);
    assert!(
        matches!(result, Err(SessionError::Corrupt { .. })),
        "{result:?}"
    );
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
        &CopyStatus::Pending,
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

/// 読み戻した写しの状態（中身を除く）。読んだものをそのまま書き直すときに使う。
fn status_of(copy: &CopyState) -> CopyStatus {
    match copy {
        CopyState::Pending => CopyStatus::Pending,
        CopyState::Ready(_) => CopyStatus::Ready,
        CopyState::Unusable(reason) => CopyStatus::Unusable(reason.clone()),
    }
}

fn code_block() -> serde_json::Value {
    serde_json::json!({"id":"b","part":{"type":"code","body":"literal\r\n","language":"pseudocode","role":"example"},"references":[]})
}

#[test]
fn version_four_restores_parts_full_explanation_quotes_and_generation_without_rewriting() {
    let scratch = Scratch::new("session");
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
        &status_of(&restored.copy),
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

/// 版 4 の保存値の 1 か所を壊す関数。
type Corruption = fn(&mut serde_json::Value, &str);

#[test]
fn version_four_with_a_broken_generation_or_explanation_history_is_corrupt() {
    use serde_json::{Value, json};
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00CCCCCCCCCCCCCCCC";
    drop(store.create(info(id, 100)).unwrap());
    let block = code_block();
    let valid = version_four_with_explanation_comment(
        id,
        &block,
        json!({"revision":1,"block_id":"b","quote":{"section_title":"Section","block":block}}),
    );
    let write = |value: &Value| {
        std::fs::write(
            session_path(&scratch.dir(), id),
            encoding::encode_session(4, &serde_json::to_vec(value).unwrap()),
        )
        .unwrap();
    };
    write(&valid);
    assert!(
        store.read(id).is_ok(),
        "the unbroken value must be readable"
    );
    let cases: Vec<(&str, Corruption, &str)> = vec![
        (
            "issued with a leading zero",
            |value, _| value["state"]["issued_display_generation"] = json!("02"),
            "invalid decimal generation",
        ),
        (
            "issued that is not a decimal",
            |value, _| value["state"]["issued_display_generation"] = json!("x"),
            "invalid decimal generation",
        ),
        (
            "issued beyond u128",
            |value, _| {
                value["state"]["issued_display_generation"] =
                    json!("340282366920938463463374607431768211456");
            },
            "outside u128",
        ),
        (
            "only the current generation",
            |value, _| value["state"]["issued_display_generation"] = Value::Null,
            "incomplete generation state",
        ),
        (
            "only the issued generation",
            |value, _| value["state"]["diff_display_version"] = Value::Null,
            "incomplete generation state",
        ),
        (
            "current beyond the issued generation",
            |value, id| value["state"]["diff_display_version"] = json!(format!("{id}:3")),
            "has not been issued",
        ),
        (
            "frozen beyond the issued generation",
            |value, id| value["state"]["frozen_display_version"] = json!(format!("{id}:3")),
            "has not been issued",
        ),
        (
            "current of another review",
            |value, _| {
                value["state"]["diff_display_version"] = json!("01HF7YAT00DDDDDDDDDDDDDDDD:1");
            },
            "another review",
        ),
        (
            "current revision not retained",
            |value, _| value["state"]["explanation"]["current_revision"] = json!(2),
            "current version not retained",
        ),
        (
            "duplicate revision",
            |value, _| {
                let versions = &mut value["state"]["explanation"]["versions"];
                let first = versions[0].clone();
                versions.as_array_mut().unwrap().push(first);
            },
            "duplicate revision",
        ),
        (
            "explanation based on an unissued generation",
            |value, id| {
                value["state"]["explanation"]["versions"][0]["base_display_version"] =
                    json!(format!("{id}:3"));
            },
            "has not been issued",
        ),
    ];
    for (name, corrupt, expected) in cases {
        let mut value = valid.clone();
        corrupt(&mut value, id);
        write(&value);
        match store.read(id) {
            Err(SessionError::Corrupt { reason, .. }) => {
                assert!(reason.contains(expected), "{name}: {reason}");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn version_four_keeps_the_saved_selection_of_an_explanation_comment() {
    let scratch = Scratch::new("session");
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
        &status_of(&restored.copy),
    ))
    .unwrap();
    assert_eq!(encoded["state"]["comments"][0]["explanation"], target);
}

#[test]
fn version_four_refuses_a_selection_the_creation_rules_would_refuse() {
    let scratch = Scratch::new("session");
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

#[test]
fn only_sessions_of_an_unreadable_version_are_reported() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());

    // セッションを 1 つも保留していない（ディレクトリが無い）。
    assert!(!store.has_unreadable_version());

    // 今の版のセッションは読めるので、報告の対象にしない。
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();
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
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    place_version_2_session(&scratch);
    // 写しのファイルは版を持たないので、今の書き方で置いてよい。
    let payload = encoding::gzip(&crate::session::encode_copy(&copy()).unwrap());
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
    let mut opened = store.open(VERSION_2_ID).unwrap();
    assert_eq!(opened.copy(), &CopyStatus::Ready);
    assert_eq!(opened.take_copy(), Some(copy()));
    assert_eq!(opened.state().comments.len(), 2);
}

#[test]
fn session_of_a_version_without_a_reader_is_unsupported_and_reported() {
    let scratch = Scratch::new("session");
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
fn version_3_session_written_before_the_page_comment_mark_reads_as_never_saved() {
    let scratch = Scratch::new("session");
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
