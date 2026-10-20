//! `--live` のセッション（R-PAGE-SESSION）。写しを持たず、会話があるときだけ残す。

use super::super::*;
use super::fixtures::*;
use crate::domain::review::{Author, Message};
use crate::session::SessionMode;

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
    let scratch = Scratch::new("session");
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

#[test]
fn live_session_is_listed_and_opened_without_a_copy() {
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
fn live_session_roundtrips_a_file_page_and_its_root() {
    let scratch = Scratch::new("session");
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
