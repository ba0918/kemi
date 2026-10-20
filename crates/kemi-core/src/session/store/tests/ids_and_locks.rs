//! id の検査、ロック、つなぎ先（R-SESSION、R-AGENT-LINK）。

use super::super::*;
use super::fixtures::*;

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
fn endpoint_is_removed_with_the_session() {
    use crate::session::{Endpoint, endpoint_path, write_endpoint};
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();
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
