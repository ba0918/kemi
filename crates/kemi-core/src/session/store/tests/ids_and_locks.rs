//! id の検査、ロック、つなぎ先（R-SESSION、R-AGENT-LINK）。

use super::super::*;
use super::fixtures::*;

#[test]
fn session_lock_is_exclusive_until_dropped() {
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
    let scratch = Scratch::new("session");
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
fn a_running_session_is_told_apart_from_a_dead_one() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let open = store.create(info(id, 100)).unwrap();

    assert!(store.is_running(id));
    drop(open);
    assert!(!store.is_running(id));
    assert!(!store.is_running("not-an-id"));
}

#[test]
fn deleting_a_session_removes_its_endpoint_and_running_record() {
    use crate::session::{
        Endpoint, endpoint_path, running_path, write_endpoint, write_running_record,
    };
    let scratch = Scratch::new("session");
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
    write_running_record(&scratch.dir(), &id, &running_record()).unwrap();

    open.delete().unwrap();

    assert!(!endpoint_path(&scratch.dir(), &id).exists());
    assert!(!running_path(&scratch.dir(), &id).exists());
}

/// 別のスレッドが持つ、復元できるセッションのロック。起動中かを確かめる側（ps、`--resume` の
/// 一覧、掃除）が一瞬ロックを取るのの代わり。
struct HeldLock {
    release: std::sync::mpsc::Sender<()>,
    holder: std::thread::JoinHandle<()>,
}

impl HeldLock {
    /// 復元できるセッションを置き、別のスレッドにそのロックを持たせる。持ち始めてから戻る。
    /// スレッドは [`HeldLock::release`] で合図を受けるか、`at_most` が過ぎるまで持つ。
    fn of_a_resumable_session(
        store: &SessionStore,
        id: &str,
        at_most: Option<std::time::Duration>,
    ) -> Self {
        let mut open = store.create(info(id, 100)).unwrap();
        open.save_frozen_copy(prepared(copy()), state_with_comment())
            .unwrap();
        drop(open);
        let (held, holding) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let holder_store = store.clone();
        let holder_id = id.to_string();
        let holder = std::thread::spawn(move || {
            let lock = SessionLock::acquire(holder_store.dir(), &holder_id).unwrap();
            held.send(()).unwrap();
            // テストが合図を送る前に落ちても、送り手が消えて受け取りが終わるので、ロックは残らない。
            match at_most {
                Some(hold) => {
                    let _ = released.recv_timeout(hold);
                }
                None => {
                    let _ = released.recv();
                }
            }
            drop(lock);
        });
        holding.recv().unwrap();
        HeldLock { release, holder }
    }

    /// 手放す合図を送り、手放し終えるまで待つ。
    fn release(self) {
        let _ = self.release.send(());
        self.holder.join().unwrap();
    }
}

#[test]
fn resume_lock_waits_out_a_brief_hold() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let held =
        HeldLock::of_a_resumable_session(&store, id, Some(std::time::Duration::from_millis(200)));

    let opened = store.open(id);

    assert!(opened.is_ok(), "{:?}", opened.err());
    held.release();
}

#[test]
fn resume_lock_gives_up_after_a_second() {
    use crate::session::lock::ACQUIRE_PATIENCE;
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let held = HeldLock::of_a_resumable_session(&store, id, None);
    let started = std::time::Instant::now();

    let opened = store.open(id);

    // 持ち主は合図を受けるまで手放さないので、取れずに断った。
    assert!(
        matches!(opened, Err(SessionError::Locked { .. })),
        "{:?}",
        opened.err()
    );
    // 一度で断ったのではなく、待つ長さの間は取り直した。
    assert!(started.elapsed() >= ACQUIRE_PATIENCE);
    held.release();
}

#[test]
fn new_session_lock_waits_out_a_brief_hold() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let held =
        HeldLock::of_a_resumable_session(&store, id, Some(std::time::Duration::from_millis(200)));

    let created = store.create(info(id, 100));

    assert!(created.is_ok(), "{:?}", created.err());
    held.release();
}

#[test]
fn running_ids_are_the_sessions_whose_lock_is_held() {
    let scratch = Scratch::new("session");
    let store = SessionStore::new(scratch.dir());
    assert!(store.running_ids().unwrap().is_empty());
    let running = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let suspended = "01HF7YAT00BBBBBBBBBBBBBBBB";
    let killed = "01HF7YAT00CCCCCCCCCCCCCCCC";
    let held = store.create(info(running, 100)).unwrap();
    let mut open = store.create(info(suspended, 100)).unwrap();
    open.save_state_at(state_with_comment(), 100).unwrap();
    drop(open);
    // 強制終了で残ったロックのファイルと、id でない名前のロック。
    std::fs::write(scratch.dir().join(format!("{killed}.lock")), b"").unwrap();
    std::fs::write(scratch.dir().join("not-an-id.lock"), b"").unwrap();

    assert_eq!(store.running_ids().unwrap(), vec![running.to_string()]);
    drop(held);
}
