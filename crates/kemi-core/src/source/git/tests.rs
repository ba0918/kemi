use super::*;
use crate::domain::review::Side;
use crate::source::testutil::TempRepo;

#[test]
fn binary_metadata_keeps_old_and_new_sizes_across_worktree_index_and_range() {
    let repo = TempRepo::new();
    for i in 0..4 {
        std::fs::write(repo.path.join(format!("file-{i}.bin")), vec![0; 1024]).unwrap();
    }
    let base = repo.add_and_commit("base");
    for i in 0..4 {
        std::fs::write(repo.path.join(format!("file-{i}.bin")), vec![0; 2048]).unwrap();
    }
    for mode in [
        GitMode::Worktree,
        GitMode::Staged,
        GitMode::Range {
            from: base,
            to: "HEAD".into(),
            group_by: GroupBy::File,
        },
    ] {
        match mode {
            GitMode::Worktree => {}
            GitMode::Staged => {
                repo.git(&["add", "."]);
            }
            GitMode::Range { .. } => {
                repo.git(&["commit", "-q", "-m", "larger binaries"]);
            }
        }
        let review = source(&repo, mode).review().unwrap();
        assert_eq!(review.groups[0].files.len(), 4);
        assert!(
            review.groups[0]
                .files
                .iter()
                .all(|f| f.old_size == 1024 && f.new_size == 2048)
        );
    }
}

fn source(repo: &TempRepo, mode: GitMode) -> GitSource {
    GitSource::new(repo.path.clone(), mode)
}

fn find_file<'a>(review: &'a ReviewMeta, path: &str) -> &'a FileEntry {
    review
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("file not found: {path}"))
}

// ---- 相対パス画像の読み取り（R-RENDER, R-SERVE） ----

fn png(tag: u8) -> Vec<u8> {
    vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, tag]
}

fn write_bytes(repo: &TempRepo, relative: &str, bytes: &[u8]) {
    let full = repo.path.join(relative);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, bytes).unwrap();
}

fn image_repo() -> TempRepo {
    let repo = TempRepo::new();
    repo.write("docs/a.md", "![x](img/x.png)\n");
    write_bytes(&repo, "docs/img/x.png", &png(1));
    repo.add_and_commit("base");
    repo
}

fn markdown_id(source: &GitSource) -> String {
    let review = source.review().unwrap();
    find_file(&review, "docs/a.md").id.clone()
}

#[test]
fn worktree_reads_relative_images_from_the_working_tree_and_head() {
    let repo = image_repo();
    repo.write("docs/a.md", "![x](img/x.png) changed\n");
    write_bytes(&repo, "docs/img/x.png", &png(2));
    let source = source(&repo, GitMode::Worktree);
    let id = markdown_id(&source);

    assert!(source.reads_repository());
    let new = source
        .repository_file(&id, Side::New, "docs/img/x.png", 1_000)
        .unwrap();
    let old = source
        .repository_file(&id, Side::Old, "docs/img/x.png", 1_000)
        .unwrap();
    assert_eq!(new, Some(png(2)));
    assert_eq!(old, Some(png(1)));
}

#[test]
fn staged_reads_relative_images_from_the_index_and_head() {
    let repo = image_repo();
    repo.write("docs/a.md", "![x](img/x.png) staged\n");
    write_bytes(&repo, "docs/img/x.png", &png(2));
    repo.git(&["add", "-A"]);
    write_bytes(&repo, "docs/img/x.png", &png(3));
    let source = source(&repo, GitMode::Staged);
    let id = markdown_id(&source);

    let new = source
        .repository_file(&id, Side::New, "docs/img/x.png", 1_000)
        .unwrap();
    let old = source
        .repository_file(&id, Side::Old, "docs/img/x.png", 1_000)
        .unwrap();
    assert_eq!(new, Some(png(2)));
    assert_eq!(old, Some(png(1)));
}

#[test]
fn final_state_reads_relative_images_from_to_and_from() {
    let repo = image_repo();
    let base = repo.head();
    repo.write("docs/a.md", "![x](img/x.png) second\n");
    write_bytes(&repo, "docs/img/x.png", &png(2));
    let head = repo.add_and_commit("second");
    let source = source(
        &repo,
        GitMode::Range {
            from: base,
            to: head,
            group_by: GroupBy::File,
        },
    );
    let id = markdown_id(&source);

    let new = source
        .repository_file(&id, Side::New, "docs/img/x.png", 1_000)
        .unwrap();
    let old = source
        .repository_file(&id, Side::Old, "docs/img/x.png", 1_000)
        .unwrap();
    assert_eq!(new, Some(png(2)));
    assert_eq!(old, Some(png(1)));
}

#[test]
fn per_commit_reads_relative_images_from_the_commit_and_its_parent() {
    let repo = image_repo();
    let base = repo.head();
    repo.write("docs/a.md", "![x](img/x.png) second\n");
    write_bytes(&repo, "docs/img/x.png", &png(2));
    repo.add_and_commit("second");
    write_bytes(&repo, "docs/img/x.png", &png(3));
    let head = repo.add_and_commit("third");
    let source = source(
        &repo,
        GitMode::Range {
            from: base,
            to: head,
            group_by: GroupBy::Commit,
        },
    );
    let id = markdown_id(&source);

    let new = source
        .repository_file(&id, Side::New, "docs/img/x.png", 1_000)
        .unwrap();
    let old = source
        .repository_file(&id, Side::Old, "docs/img/x.png", 1_000)
        .unwrap();
    assert_eq!(new, Some(png(2)));
    assert_eq!(old, Some(png(1)));
}

// symlink を作るので unix だけ。Windows では作成に権限が要り、CI のランナーでは
// そもそもコンパイルできない（`std::os::unix`）。
#[cfg(unix)]
#[test]
fn relative_images_that_are_symlinks_too_large_or_missing_are_not_served() {
    let repo = image_repo();
    repo.write("docs/a.md", "![x](img/x.png) changed\n");
    write_bytes(&repo, "docs/img/big.png", &[0x89; 64]);
    std::os::unix::fs::symlink(
        repo.path.join("docs/img/x.png"),
        repo.path.join("docs/img/link.png"),
    )
    .unwrap();
    std::os::unix::fs::symlink(repo.path.join("docs/img"), repo.path.join("docs/alias")).unwrap();
    repo.add_and_commit("links");
    repo.write("docs/a.md", "![x](img/x.png) changed again\n");
    let source = source(&repo, GitMode::Worktree);
    let id = markdown_id(&source);

    for side in [Side::New, Side::Old] {
        assert_eq!(
            source
                .repository_file(&id, side, "docs/img/link.png", 1_000)
                .unwrap(),
            None,
            "{side:?} symlink"
        );
        assert_eq!(
            source
                .repository_file(&id, side, "docs/alias/x.png", 1_000)
                .unwrap(),
            None,
            "{side:?} symlinked directory"
        );
        assert_eq!(
            source
                .repository_file(&id, side, "docs/img/big.png", 32)
                .unwrap(),
            None,
            "{side:?} over the limit"
        );
        assert_eq!(
            source
                .repository_file(&id, side, "docs/img/missing.png", 1_000)
                .unwrap(),
            None,
            "{side:?} missing"
        );
        assert_eq!(
            source
                .repository_file(&id, side, "docs/img/x.png", 1_000)
                .unwrap(),
            Some(png(1)),
            "{side:?} regular"
        );
    }
}

#[test]
fn relative_image_paths_that_are_not_plain_repository_paths_are_refused_without_an_error() {
    let repo = image_repo();
    repo.write("docs/a.md", "![x](img/x.png) changed\n");
    let source = source(&repo, GitMode::Worktree);
    let id = markdown_id(&source);

    for side in [Side::New, Side::Old] {
        for path in [
            "../x.png",
            "/etc/passwd.png",
            "./docs/img/x.png",
            "docs//img/x.png",
            "docs/img/../img/x.png",
        ] {
            assert_eq!(
                source.repository_file(&id, side, path, 1_000).unwrap(),
                None,
                "{side:?} {path}"
            );
        }
    }
}

#[test]
fn plain_repository_paths_reject_segments_with_a_colon() {
    assert!(is_plain_repository_path("docs/img/x.png"));
    assert!(!is_plain_repository_path("C:/Windows/x.png"));
    assert!(!is_plain_repository_path("C:x.png"));
}

// ファイルの側を作れるのは unix だけ。Windows では `C:x.png` が「C ドライブの
// 現在位置からの相対」を意味し、この名前のファイルをリポジトリの中に置けない。
// 受け口が拒むこと自体は、上の `is_plain_repository_path` の判定と
// `relative_image_paths_that_are_not_plain_repository_paths_are_refused_without_an_error`
// が両方の OS で確かめる。
#[cfg(unix)]
#[test]
fn relative_image_paths_with_a_drive_prefix_are_refused_even_when_such_files_exist() {
    let repo = image_repo();
    // Linux では `C:` は普通のディレクトリ名。ファイルが有っても受け口で拒む。
    write_bytes(&repo, "C:/Windows/x.png", &png(3));
    write_bytes(&repo, "C:x.png", &png(4));
    repo.add_and_commit("drive-like paths");
    repo.write("docs/a.md", "![x](img/x.png) changed\n");
    let source = source(&repo, GitMode::Worktree);
    let id = markdown_id(&source);

    for side in [Side::New, Side::Old] {
        for path in ["C:/Windows/x.png", "C:x.png"] {
            assert_eq!(
                source.repository_file(&id, side, path, 1_000).unwrap(),
                None,
                "{side:?} {path}"
            );
        }
    }
}

#[test]
fn worktree_files_outside_the_repository_are_not_read_even_if_the_path_resolves() {
    let repo = TempRepo::new();
    let outside = TempRepo::new();
    write_bytes(&outside, "x.png", &png(5));
    // 絶対パスの push は基準を置き換える（Windows のドライブ接頭辞と同じ）。
    let escaped = outside.path.join("x.png");

    assert_eq!(
        read_worktree_file(&repo.path, &escaped, 1_000).unwrap(),
        None
    );
}

#[test]
fn worktree_change_has_uncommitted_content() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\ntwo\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    repo.write("a.txt", "one\nTWO\n");
    let review = source.review().unwrap();
    let entry = find_file(&review, "a.txt");

    assert_eq!(entry.status, Status::Modify);
    assert_eq!(entry.group_id, "worktree");
    assert_eq!((entry.add, entry.del), (1, 1));
    assert_eq!(review.title, "Working tree changes");
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old.unwrap(), b"one\ntwo\n");
    assert_eq!(content.new.unwrap(), b"one\nTWO\n");
}

#[test]
fn worktree_includes_untracked_as_add() {
    let repo = TempRepo::new();
    repo.write("a.txt", "a\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    repo.write("new/b.txt", "x\ny\n");
    let review = source.review().unwrap();
    let entry = find_file(&review, "new/b.txt");

    assert_eq!(entry.status, Status::Add);
    assert_eq!((entry.add, entry.del), (2, 0));
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old, None);
    assert_eq!(content.new.unwrap(), b"x\ny\n");
}

#[test]
fn worktree_file_ids_survive_re_review() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.write("b.txt", "one\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    repo.write("a.txt", "two\n");
    repo.write("b.txt", "two\n");
    let first = source.review().unwrap();
    let first_a = find_file(&first, "a.txt").id.clone();
    let first_b = find_file(&first, "b.txt").id.clone();
    assert_ne!(first_a, first_b);

    // a を戻すと一覧から消えるが、残る b の id は変えない。
    repo.write("a.txt", "one\n");
    let second = source.review().unwrap();
    assert_eq!(find_file(&second, "b.txt").id, first_b);

    // 一度消えた a が再び現れても、最初の id を使う。
    repo.write("a.txt", "three\n");
    let third = source.review().unwrap();
    assert_eq!(find_file(&third, "a.txt").id, first_a);
}

#[test]
fn worktree_deleted_file_shows_delete() {
    let repo = TempRepo::new();
    repo.write("a.txt", "a\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    repo.remove("a.txt");
    let review = source.review().unwrap();
    let entry = find_file(&review, "a.txt");

    assert_eq!(entry.status, Status::Delete);
    assert_eq!((entry.add, entry.del), (0, 1));
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old.unwrap(), b"a\n");
    assert_eq!(content.new, None);
}

#[test]
fn worktree_rename_is_detected_among_many_changes() {
    let repo = TempRepo::new();
    let body: String = (1..=20).map(|n| format!("line{n}\n")).collect();
    repo.write("a000.txt", &body);
    for index in 0..300 {
        repo.write(&format!("m{index:03}.txt"), "one\n");
    }
    repo.add_and_commit("base");

    repo.git(&["mv", "a000.txt", "zzz.txt"]);
    for index in 0..300 {
        repo.write(&format!("m{index:03}.txt"), "one\ntwo\n");
    }

    let source = source(&repo, GitMode::Worktree);
    let review = source.review().unwrap();

    assert_eq!(review.groups[0].files.len(), 301);
    let entry = find_file(&review, "zzz.txt");
    assert_eq!(entry.status, Status::Rename);
    assert_eq!(entry.old_path.as_deref(), Some("a000.txt"));
    assert_eq!((entry.add, entry.del), (0, 0));
}

#[test]
fn worktree_untracked_over_limit_has_stats_without_content() {
    let repo = TempRepo::new();
    repo.write("a.txt", "a\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    let big = "x".repeat(UNTRACKED_LIMIT as usize + 1);
    repo.write("big.bin", &big);
    let review = source.review().unwrap();
    let entry = find_file(&review, "big.bin");

    assert_eq!(entry.status, Status::Add);
    assert!(entry.binary);
    assert_eq!((entry.add, entry.del), (0, 0));
    assert_eq!(entry.new_size, UNTRACKED_LIMIT + 1);
    assert!(entry.content_skipped);
}

#[test]
fn staged_shows_index_side_not_worktree() {
    let repo = TempRepo::new();
    repo.write("a.txt", "base\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Staged);

    repo.write("a.txt", "index\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "worktree\n");

    let review = source.review().unwrap();
    let entry = find_file(&review, "a.txt");
    assert_eq!(entry.group_id, "staged");
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old.unwrap(), b"base\n");
    assert_eq!(content.new.unwrap(), b"index\n");
}

#[test]
fn staged_ignores_worktree_only_change() {
    let repo = TempRepo::new();
    repo.write("a.txt", "base\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Staged);

    repo.write("a.txt", "worktree\n");
    let review = source.review().unwrap();

    assert!(review.groups[0].files.is_empty());
}

/// a.txt / b.txt / c.txt を、範囲のコミット・index・作業ツリーのそれぞれで変え、
/// 利用者の設定として diff.orderFile を置いたリポジトリ。git commit も
/// diff.orderFile を読むので、設定は最後に置く。
fn repo_with_diff_order_file(order_file: &str) -> (TempRepo, String) {
    let repo = TempRepo::new();
    let names = ["a.txt", "b.txt", "c.txt"];
    names.iter().for_each(|name| repo.write(name, "base\n"));
    let base = repo.add_and_commit("base");
    names.iter().for_each(|name| repo.write(name, "range\n"));
    repo.add_and_commit("change");
    names.iter().for_each(|name| repo.write(name, "staged\n"));
    repo.git(&["add", "-A"]);
    names.iter().for_each(|name| repo.write(name, "worktree\n"));
    let order_file = repo.path.join(".git").join(order_file);
    repo.git(&["config", "diff.orderFile", order_file.to_str().unwrap()]);
    (repo, base)
}

/// 最終形・worktree・staged のそれぞれで、一覧に並ぶパス。
fn listed_paths_in_each_mode(repo: &TempRepo, base: &str) -> Vec<Vec<String>> {
    let range = GitMode::Range {
        from: base.to_string(),
        to: "HEAD".to_string(),
        group_by: GroupBy::File,
    };
    [range, GitMode::Worktree, GitMode::Staged]
        .into_iter()
        .map(|mode| {
            let review = source(repo, mode.clone())
                .review()
                .unwrap_or_else(|error| panic!("{mode:?}: {error}"));
            review.groups[0]
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect()
        })
        .collect()
}

#[test]
fn missing_diff_order_file_does_not_stop_the_review() {
    let (repo, base) = repo_with_diff_order_file("missing-order");

    for paths in listed_paths_in_each_mode(&repo, &base) {
        assert_eq!(paths, ["a.txt", "b.txt", "c.txt"]);
    }
}

#[test]
fn diff_order_file_does_not_reorder_the_file_list() {
    let (repo, base) = repo_with_diff_order_file("order");
    std::fs::write(repo.path.join(".git").join("order"), "c.txt\nb.txt\n").unwrap();

    for paths in listed_paths_in_each_mode(&repo, &base) {
        assert_eq!(paths, ["a.txt", "b.txt", "c.txt"]);
    }
}

#[test]
fn range_commit_groups_exclude_merges_and_keep_subject_body() {
    let repo = TempRepo::new();
    repo.write("a.txt", "a\n");
    let base = repo.add_and_commit("base");

    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("b.txt", "b\n");
    let feature = repo.add_and_commit("feat: b\n\nbody line");
    repo.git(&["checkout", "-q", "-"]);
    repo.write("c.txt", "c\n");
    repo.add_and_commit("feat: c");
    repo.git(&["merge", "--no-ff", "-q", "feature", "-m", "merge feature"]);
    let head = repo.head();

    let source = source(
        &repo,
        GitMode::Range {
            from: base.clone(),
            to: head.clone(),
            group_by: GroupBy::Commit,
        },
    );
    let review = source.review().unwrap();

    let ids: Vec<&str> = review
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&feature.as_str()));
    assert!(!ids.iter().any(|id| id.starts_with(&head[..7])));
    assert_eq!(review.title, format!("{base}..{head}"));

    let group = review
        .groups
        .iter()
        .find(|group| group.id == feature)
        .unwrap();
    assert_eq!(group.title, "feat: b");
    assert_eq!(group.why, "body line");
    assert_eq!(group.files.len(), 1);
    assert_eq!(group.files[0].status, Status::Add);
    let content = source.content(&group.files[0].id).unwrap();
    assert_eq!(content.old, None);
    assert_eq!(content.new.unwrap(), b"b\n");
}

#[test]
fn range_commit_group_ids_are_shas_even_when_log_shows_signatures() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.add_and_commit("base");
    repo.write("a.txt", "two\n");
    if repo.add_and_commit_signed("signed").is_none() {
        return;
    }
    repo.write("a.txt", "three\n");
    repo.add_and_commit("unsigned");
    repo.git(&["config", "log.showSignature", "true"]);

    let review = source(
        &repo,
        GitMode::Range {
            from: base.clone(),
            to: "HEAD".to_string(),
            group_by: GroupBy::Commit,
        },
    )
    .review()
    .unwrap();

    let ids: Vec<&str> = review
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    let expected = repo.git(&[
        "rev-list",
        "--reverse",
        "--no-merges",
        &format!("{base}..HEAD"),
    ]);
    assert_eq!(ids, expected.lines().collect::<Vec<_>>());
}

#[test]
fn range_commit_detects_rename_and_reads_old_path() {
    let repo = TempRepo::new();
    let body: String = (1..=20).map(|n| format!("line{n}\n")).collect();
    repo.write("old.txt", &body);
    let base = repo.add_and_commit("base");

    repo.git(&["mv", "old.txt", "new.txt"]);
    repo.write(
        "new.txt",
        &(1..=19).map(|n| format!("line{n}\n")).collect::<String>(),
    );
    let head = repo.add_and_commit("rename");

    let source = source(
        &repo,
        GitMode::Range {
            from: base,
            to: head,
            group_by: GroupBy::Commit,
        },
    );
    let review = source.review().unwrap();
    let entry = find_file(&review, "new.txt");

    assert_eq!(entry.status, Status::Rename);
    assert_eq!(entry.old_path.as_deref(), Some("old.txt"));
    let content = source.content(&entry.id).unwrap();
    assert!(content.old.unwrap().starts_with(b"line1\n"));
    assert!(content.new.unwrap().ends_with(b"line19\n"));
}

#[test]
fn range_file_keeps_one_entry_per_path_across_commits() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.add_and_commit("base");
    repo.write("a.txt", "two\n");
    repo.add_and_commit("second");
    repo.write("a.txt", "three\n");
    let head = repo.add_and_commit("third");

    let source = source(
        &repo,
        GitMode::Range {
            from: base.clone(),
            to: head.clone(),
            group_by: GroupBy::File,
        },
    );
    let review = source.review().unwrap();

    assert_eq!(review.groups.len(), 1);
    assert_eq!(review.groups[0].id, "all");
    assert_eq!(review.groups[0].title, format!("{base}...{head}"));
    assert_eq!(review.groups[0].files.len(), 1);
    let content = source.content(&review.groups[0].files[0].id).unwrap();
    assert_eq!(content.old.unwrap(), b"one\n");
    assert_eq!(content.new.unwrap(), b"three\n");
}

#[cfg(unix)]
fn non_utf8_name() -> &'static std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b"b\xffad.txt")
}

#[cfg(unix)]
fn find_non_utf8(review: &ReviewMeta) -> &FileEntry {
    review
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .find(|file| file.path.ends_with("ad.txt"))
        .expect("non-UTF-8 path must be listed")
}

#[cfg(unix)]
#[test]
fn worktree_includes_non_utf8_untracked_file() {
    let repo = TempRepo::new();
    repo.write("a.txt", "a\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    std::fs::write(repo.path.join(non_utf8_name()), "x\ny\n").unwrap();

    let review = source.review().unwrap();
    let entry = find_non_utf8(&review);

    assert_eq!(entry.status, Status::Add);
    assert_eq!((entry.add, entry.del), (2, 0));
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old, None);
    assert_eq!(content.new.unwrap(), b"x\ny\n");
}

#[cfg(unix)]
#[test]
fn worktree_reads_non_utf8_tracked_file() {
    let repo = TempRepo::new();
    std::fs::write(repo.path.join(non_utf8_name()), "one\n").unwrap();
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Worktree);

    std::fs::write(repo.path.join(non_utf8_name()), "two\n").unwrap();

    let review = source.review().unwrap();
    let entry = find_non_utf8(&review);

    assert_eq!(entry.status, Status::Modify);
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old.unwrap(), b"one\n");
    assert_eq!(content.new.unwrap(), b"two\n");
}

#[cfg(unix)]
#[test]
fn staged_reads_non_utf8_path() {
    let repo = TempRepo::new();
    repo.write("a.txt", "a\n");
    repo.add_and_commit("base");
    let source = source(&repo, GitMode::Staged);

    std::fs::write(repo.path.join(non_utf8_name()), "x\ny\n").unwrap();
    repo.git(&["add", "-A"]);

    let review = source.review().unwrap();
    let entry = find_non_utf8(&review);

    assert_eq!(entry.status, Status::Add);
    let content = source.content(&entry.id).unwrap();
    assert_eq!(content.old, None);
    assert_eq!(content.new.unwrap(), b"x\ny\n");
}
