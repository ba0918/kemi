use super::*;

#[tokio::test]
async fn worktree_change_to_a_file_outside_the_startup_diff_sends_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;

    let updated = updates
        .after_repeating(|attempt| dir.write("b.txt", &format!("b2 {attempt}\n")))
        .await;

    assert!(
        updated,
        "changing a tracked file outside the diff sent no update"
    );
    assert!(review_paths(&kemi).await.contains(&"b.txt".to_string()));
    kemi.kill();
}

#[tokio::test]
async fn worktree_new_file_sends_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;

    let updated = updates
        .after_repeating(|attempt| dir.write("new.txt", &format!("new {attempt}\n")))
        .await;

    assert!(updated, "creating a file sent no update");
    assert!(review_paths(&kemi).await.contains(&"new.txt".to_string()));
    kemi.kill();
}

#[tokio::test]
async fn worktree_change_in_a_directory_created_while_watching_sends_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;
    assert!(
        updates
            .after_repeating(|attempt| dir.write("b.txt", &format!("b2 {attempt}\n")))
            .await
    );
    std::fs::create_dir_all(dir.path.join("fresh/inner")).unwrap();
    while updates.count(std::time::Duration::from_millis(1500)).await > 0 {}

    dir.write("fresh/inner/c.txt", "c\n");

    assert!(
        updates.count(std::time::Duration::from_secs(3)).await > 0,
        "a change in a new directory sent no update"
    );
    kemi.kill();
}

#[tokio::test]
async fn worktree_change_under_a_directory_renamed_away_and_back_sends_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    std::fs::create_dir_all(dir.path.join("moved/inner")).unwrap();
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;
    assert!(
        updates
            .after_repeating(|attempt| dir.write("b.txt", &format!("b2 {attempt}\n")))
            .await
    );
    std::fs::rename(dir.path.join("moved"), dir.path.join("away")).unwrap();
    std::fs::rename(dir.path.join("away"), dir.path.join("moved")).unwrap();
    while updates.count(std::time::Duration::from_millis(1500)).await > 0 {}

    dir.write("moved/inner/c.txt", "c\n");

    assert!(
        updates.count(std::time::Duration::from_secs(3)).await > 0,
        "a change under a directory renamed back sent no update"
    );
    kemi.kill();
}

#[tokio::test]
async fn worktree_change_in_a_directory_no_longer_ignored_by_gitignore_sends_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;
    assert!(
        updates
            .after_repeating(|attempt| dir.write("b.txt", &format!("b2 {attempt}\n")))
            .await
    );
    dir.write(".gitignore", "*.log\n");
    while updates.count(std::time::Duration::from_millis(1500)).await > 0 {}

    dir.write("build/new.txt", "n\n");

    assert!(
        updates.count(std::time::Duration::from_secs(3)).await > 0,
        "a change in a directory .gitignore stopped ignoring sent no update"
    );
    kemi.kill();
}

#[tokio::test]
async fn worktree_change_in_a_directory_no_longer_ignored_by_info_exclude_sends_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    dir.write(".git/info/exclude", "scratch/\n");
    dir.write("scratch/old.txt", "o\n");
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;
    assert!(
        updates
            .after_repeating(|attempt| dir.write("b.txt", &format!("b2 {attempt}\n")))
            .await
    );
    dir.write(".git/info/exclude", "");
    while updates.count(std::time::Duration::from_millis(1500)).await > 0 {}

    dir.write("scratch/new.txt", "n\n");

    assert!(
        updates.count(std::time::Duration::from_secs(3)).await > 0,
        "a change in a directory info/exclude stopped ignoring sent no update"
    );
    kemi.kill();
}

#[tokio::test]
async fn worktree_writes_to_ignored_paths_and_dot_git_send_no_update() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;
    // 見張りが登録し終わったことを、見張る側への変更で確かめてから書く。
    assert!(
        updates
            .after_repeating(|attempt| dir.write("b.txt", &format!("b2 {attempt}\n")))
            .await
    );

    for round in 0..5 {
        dir.write("build/out.o", &format!("o {round}\n"));
        dir.write("build/deep/x.o", &format!("x {round}\n"));
        dir.write("debug.log", &format!("log {round}\n"));
        dir.write(".git/kemi-e2e.txt", &format!("git {round}\n"));
    }

    assert_eq!(
        updates.count(std::time::Duration::from_millis(1500)).await,
        0,
        "writes to ignored paths or .git sent an update"
    );
    kemi.kill();
}

#[tokio::test]
async fn worktree_watch_lets_a_directory_in_the_work_tree_be_renamed_and_removed() {
    let dir = TempDir::new();
    worktree_with_an_unchanged_file(&dir);
    // 差分のファイルと、変わっていない追跡済みのファイルを両方持つディレクトリ。
    dir.write("sub/inner/changed.txt", "c1\n");
    dir.write("sub/inner/same.txt", "s1\n");
    git(&dir.path, &["add", "sub"]);
    git(&dir.path, &["commit", "-q", "-m", "sub"]);
    dir.write("sub/inner/changed.txt", "c2\n");
    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let mut updates = Updates::open(&kemi).await;
    // 見張りが登録し終わったことを、そのディレクトリの中への変更で確かめてから動かす。
    assert!(
        updates
            .after_repeating(|attempt| dir.write("sub/inner/same.txt", &format!("s2 {attempt}\n")))
            .await
    );

    std::fs::rename(dir.path.join("sub"), dir.path.join("moved"))
        .expect("renaming a directory in the work tree while watching");
    std::fs::remove_dir_all(dir.path.join("moved"))
        .expect("removing a directory in the work tree while watching");

    assert!(!review_paths(&kemi).await.is_empty());
    kemi.kill();
}

// 上限は Linux だけ（macOS と Windows は根の再帰の見張り 1 つで、ディレクトリを数えない）。
#[cfg(target_os = "linux")]
#[tokio::test]
async fn worktree_over_the_directory_limit_watches_only_the_diff_and_says_so_once() {
    let within = TempDir::new();
    worktree_with_an_unchanged_file(&within);
    let over = TempDir::new();
    worktree_with_an_unchanged_file(&over);
    for index in 0..10_001 {
        std::fs::create_dir_all(over.path.join(format!("many/d{index}"))).unwrap();
    }

    let usual = stderr_of_a_review_that_sees_an_update(&within).await;
    let fallen_back = stderr_of_a_review_that_sees_an_update(&over).await;

    // 文言は契約ではないので、上限の内側のレビューより 1 行だけ多いことで確かめる。
    assert_eq!(
        fallen_back.len(),
        usual.len() + 1,
        "within the limit: {usual:?}, over the limit: {fallen_back:?}"
    );
}
