//! 写し（`<id>.payload`）の保存と上限、復元できるかの判定（R-SESSION）。

use super::super::*;
use super::fixtures::*;

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
    open.save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();
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
    open.save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();
    let id = open.id().to_string();
    drop(open);

    let bytes = std::fs::read(scratch.dir().join(format!("{id}.payload"))).unwrap();
    // 写しのファイルは全体が 1 つの gzip で、目印も版も持たない。
    assert_eq!(&bytes[..3], &[0x1f, 0x8b, 0x08]);
    let inflated = encoding::gunzip(&bytes, encoding::DECOMPRESS_LIMIT).unwrap();

    assert_eq!(crate::session::decode_copy(&inflated).unwrap(), copy());
}

#[test]
fn session_state_write_leaves_the_copy_file_untouched() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    open.save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();
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

/// サーバが写しと一緒に渡す状態。表示版の現在の世代に凍結の印が付いている。
fn frozen_state(id: &str) -> SessionState {
    let mut display = crate::domain::display_version::DisplayVersions::initial(id).unwrap();
    display.frozen = Some(display.current.clone());
    SessionState {
        display: Some(display),
        ..state_with_comment()
    }
}

#[test]
fn frozen_copy_is_saved_with_its_state_without_advancing_the_updated_time() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();

    assert!(
        open.save_frozen_copy(prepared(copy()), frozen_state(id))
            .unwrap()
    );

    assert!(open.is_resumable());
    assert_eq!(open.info().updated, 100);
    drop(open);
    let saved = store.read(id).unwrap();
    assert_eq!(saved.copy, CopyState::Ready(copy()));
    assert_eq!(saved.state, frozen_state(id));
    assert_eq!(saved.info.updated, 100);
    assert_eq!(
        store
            .list()
            .unwrap()
            .iter()
            .map(|summary| summary.id.as_str())
            .collect::<Vec<_>>(),
        vec![id]
    );
}

#[cfg(unix)]
#[test]
fn frozen_copy_that_cannot_be_written_does_not_become_resumable() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();
    // 写しのファイルを置けなくする（同じ名前のディレクトリがあると rename が
    // 失敗する）。ディスクが一杯のときと同じ経路になる。
    std::fs::create_dir_all(payload_path(&scratch.dir(), id)).unwrap();

    assert!(
        open.save_frozen_copy(prepared(copy()), frozen_state(id))
            .is_err()
    );

    // 写しを置けていないので「使える」にしない。この後に状態を書いても、
    // 「使える」と書いた `<id>.session` は残らない（R-SESSION の不変条件）。
    assert!(!open.is_resumable());
    assert_eq!(open.copy(), &CopyStatus::Pending);
    open.save_issued_state(frozen_state(id)).unwrap();
    assert_eq!(store.read(id).unwrap().copy, CopyState::Pending);
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn frozen_copy_arriving_after_the_review_stopped_writes_nothing() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    // submit と競争した凍結が、セッションを消した後に届く。
    let mut deleted = store.create(info(id, 100)).unwrap();
    deleted.save_state_at(state_with_comment(), 200).unwrap();
    deleted.delete().unwrap();

    assert!(matches!(
        deleted.save_frozen_copy(prepared(copy()), frozen_state(id)),
        Err(SessionError::Unusable { .. })
    ));
    assert!(!session_path(&scratch.dir(), id).exists());
    assert!(!payload_path(&scratch.dir(), id).exists());
    drop(deleted);

    // 保留と競争した凍結が、片付けた後に届く。
    let mut suspended = store.create(info(id, 100)).unwrap();
    suspended.close_suspended().unwrap();

    assert!(matches!(
        suspended.save_frozen_copy(prepared(copy()), frozen_state(id)),
        Err(SessionError::Unusable { .. })
    ));
    assert!(!session_path(&scratch.dir(), id).exists());
    assert!(!payload_path(&scratch.dir(), id).exists());
}

#[test]
fn frozen_copy_over_the_limit_is_unusable_and_never_written() {
    // 上限は `<id>.payload` 全体のバイト数で判定する（R-SESSION）。ちょうどは収まり、
    // 1 バイト超えると捨てる。
    let limit = prepared(copy()).compressed.len() as u64;
    let scratch = Scratch::new();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let fits = "01HF7YAT00BBBBBBBBBBBBBBBB";
    let mut open = SessionStore::new(scratch.dir())
        .with_copy_limit(limit)
        .create(info(fits, 100))
        .unwrap();
    assert!(
        open.save_frozen_copy(prepared(copy()), frozen_state(fits))
            .unwrap()
    );
    drop(open);
    let store = SessionStore::new(scratch.dir()).with_copy_limit(limit - 1);
    let mut open = store.create(info(id, 100)).unwrap();

    assert!(
        !open
            .save_frozen_copy(prepared(copy()), frozen_state(id))
            .unwrap()
    );

    assert!(matches!(open.copy(), CopyStatus::Unusable(_)));
    assert!(!payload_path(&scratch.dir(), id).exists());
    drop(open);
    let saved = store.read(id).unwrap();
    assert!(matches!(saved.copy, CopyState::Unusable(_)));
    // 写しを捨てたので、凍結の印も残さない。
    let display = saved.state.display.unwrap();
    assert_eq!(display.frozen, None);
    assert_eq!(display.current, frozen_state(id).display.unwrap().current);
    assert!(matches!(store.open(id), Err(SessionError::Unusable { .. })));
    assert_eq!(
        store
            .list()
            .unwrap()
            .iter()
            .map(|summary| summary.id.as_str())
            .collect::<Vec<_>>(),
        vec![fits]
    );
}

#[test]
fn session_without_state_and_an_unusable_copy_is_not_kept() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir()).with_copy_limit(1);
    let mut open = store
        .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
        .unwrap();
    assert!(
        !open
            .save_frozen_copy(prepared(copy()), SessionState::default())
            .unwrap()
    );

    open.close_suspended().unwrap();

    assert!(matches!(
        store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
        Err(SessionError::NotFound { .. })
    ));
}

#[test]
fn session_dropping_the_copy_removes_the_copy_file() {
    let scratch = Scratch::new();
    let store = SessionStore::new(scratch.dir());
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store.create(info(id, 100)).unwrap();
    open.save_state_at(state_with_comment(), 200).unwrap();
    open.save_frozen_copy(prepared(copy()), state_with_comment())
        .unwrap();

    open.mark_unresumable_at("cannot read the contents", 400)
        .unwrap();

    // 状態は残るので `<id>.session` は残り、写しのファイルだけが消える。
    assert!(scratch.dir().join(format!("{id}.session")).exists());
    assert!(!scratch.dir().join(format!("{id}.payload")).exists());

    // 状態が空のまま写しが使えなくなる枝では、セッションごと消えて写しも残らない。
    let other = "01HF7YAT00BBBBBBBBBBBBBBBB";
    let mut empty = store.create(info(other, 100)).unwrap();
    empty
        .save_frozen_copy(prepared(copy()), SessionState::default())
        .unwrap();

    empty
        .mark_unresumable_at("cannot read the contents", 300)
        .unwrap();

    assert!(!scratch.dir().join(format!("{other}.session")).exists());
    assert!(!scratch.dir().join(format!("{other}.payload")).exists());
}
