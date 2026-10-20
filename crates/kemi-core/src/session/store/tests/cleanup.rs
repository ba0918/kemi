//! 掃除と一覧の並び（R-SESSION）。上限、ロック中の数え方、孤児の片付け。

use std::time::{Duration, SystemTime};

use super::super::*;
use super::fixtures::*;
use crate::session::cleanup::{ORPHAN_LOCK_MIN_AGE, cleanup_at};
use crate::session::files::SnapshotKind;

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
    old.save_frozen_copy(prepared(large), SessionState::default())
        .unwrap();
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
    for (id, updated) in [
        ("01HF7YAT00AAAAAAAAAAAAAAAA", 100),
        ("01HF7YAT00BBBBBBBBBBBBBBBB", 200),
    ] {
        let mut held = store.create(info(id, updated)).unwrap();
        held.save_frozen_copy(prepared(copy()), SessionState::default())
            .unwrap();
    }
    let mut locked = store
        .create(info("01HF7YAT00CCCCCCCCCCCCCCCC", 300))
        .unwrap();
    locked
        .save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();

    // 最新を開いたまま 2 件だけ残す。ロック中は消えず、件数にも数えないので、
    // 保留した古い 2 件も残る。
    cleanup(&scratch.dir(), 2, u64::MAX);

    assert!(store.read("01HF7YAT00AAAAAAAAAAAAAAAA").is_ok());
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
    assert!(store.read("01HF7YAT00CCCCCCCCCCCCCCCC").is_ok());
}

#[test]
fn session_cleanup_keeps_a_locked_session_out_of_the_total_size() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    for (id, updated) in [
        ("01HF7YAT00AAAAAAAAAAAAAAAA", 100),
        ("01HF7YAT00BBBBBBBBBBBBBBBB", 200),
        ("01HF7YAT00CCCCCCCCCCCCCCCC", 250),
    ] {
        let mut held = store.create(info(id, updated)).unwrap();
        held.save_state_at(state_with_comment(), updated).unwrap();
    }
    // 最新のレビューは動いていて、写しが大きい。
    let mut locked = store
        .create(info("01HF7YAT00DDDDDDDDDDDDDDDD", 300))
        .unwrap();
    let mut large = copy();
    large.contents.insert(
        "big".to_string(),
        content(None, Some(&incompressible(20_000))),
    );
    locked
        .save_frozen_copy(prepared(large), SessionState::default())
        .unwrap();
    let size_of = |name: &str| std::fs::metadata(scratch.dir().join(name)).unwrap().len();
    // 保留した新しい 2 件だけが収まる上限。ロック中の大きさを数えると、その 2 件も
    // 収まらなくなる。
    let limit = size_of("01HF7YAT00BBBBBBBBBBBBBBBB.session")
        + size_of("01HF7YAT00CCCCCCCCCCCCCCCC.session")
        + 10;

    cleanup(&scratch.dir(), KEEP_SESSIONS, limit);

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
    assert!(store.read("01HF7YAT00CCCCCCCCCCCCCCCC").is_ok());
    assert!(store.read("01HF7YAT00DDDDDDDDDDDDDDDD").is_ok());
}

#[test]
fn sessions_updated_at_the_same_time_are_ordered_by_the_larger_id() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    for id in ["01HF7YAT00AAAAAAAAAAAAAAAA", "01HF7YAT00BBBBBBBBBBBBBBBB"] {
        let mut held = store.create(info(id, 100)).unwrap();
        held.save_frozen_copy(prepared(copy()), state_with_comment())
            .unwrap();
    }

    assert_eq!(
        store
            .list()
            .unwrap()
            .iter()
            .map(|summary| summary.id.as_str())
            .collect::<Vec<_>>(),
        vec!["01HF7YAT00BBBBBBBBBBBBBBBB", "01HF7YAT00AAAAAAAAAAAAAAAA"]
    );

    cleanup(&scratch.dir(), 1, u64::MAX);

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
    assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
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
    stale
        .save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();
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
fn session_list_orders_by_updated_and_skips_unusable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    // 新しい方の id を小さくして、id の順に並べても通らないようにする。
    let mut older = store
        .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 100))
        .unwrap();
    older.save_state_at(state_with_comment(), 100).unwrap();
    older
        .save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();
    let mut newer = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 200))
        .unwrap();
    newer.save_state_at(state_with_comment(), 200).unwrap();
    newer
        .save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();
    let mut unusable = SessionStore::new(scratch.dir())
        .with_copy_limit(1)
        .create(info("01HF7YAT00CCCCCCCCCCCCCCCC", 300))
        .unwrap();
    unusable.save_state_at(state_with_comment(), 300).unwrap();
    unusable
        .save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();

    let listed = store.list().unwrap();

    assert_eq!(
        listed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["01HF7YAT00AAAAAAAAAAAAAAAA", "01HF7YAT00BBBBBBBBBBBBBBBB"]
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

    let clean_at = |now| cleanup_at(&scratch.dir(), KEEP_SESSIONS, KEEP_BYTES, now);

    // 新しすぎるロックは、始まったばかりのセッションと競合しないよう残す。
    clean_at(written + ORPHAN_LOCK_MIN_AGE - Duration::from_secs(1));
    assert!(lock_path.exists());

    // ロック中のものは他のプロセスのものなので残す。
    let held = SessionLock::acquire(&scratch.dir(), "01HF7YAT00AAAAAAAAAAAAAAAA").unwrap();
    clean_at(written + ORPHAN_LOCK_MIN_AGE);
    assert!(lock_path.exists());
    drop(held);

    // 使われていない古いロックは、猶予ちょうどで消える。
    std::fs::write(&lock_path, b"").unwrap();
    let written = modified(&lock_path);
    clean_at(written + ORPHAN_LOCK_MIN_AGE);
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
