//! セッションに添えるファイル（`<id>.files/`）の書き込みと 20 MB の規則（R-PAGE-SESSION）。

use super::super::*;
use super::fixtures::*;
use crate::session::files::{PageSnapshot, SnapshotKind};

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
