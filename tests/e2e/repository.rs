use super::*;

#[tokio::test]
async fn refresh_keeps_comment_on_the_same_file() {
    let dir = TempDir::new();
    git(&dir.path, &["init", "-q"]);
    git(&dir.path, &["config", "user.email", "kemi@example.com"]);
    git(&dir.path, &["config", "user.name", "kemi"]);
    dir.write("a.txt", "a1\n");
    dir.write("b.txt", "b1\n");
    git(&dir.path, &["add", "a.txt", "b.txt"]);
    git(&dir.path, &["commit", "-q", "-m", "base"]);
    dir.write("a.txt", "a2\n");
    dir.write("b.txt", "b2\n");

    let kemi = Kemi::spawn(&dir.path, &["--worktree", "--no-open", "--port", "0"]);
    let review = kemi.review_json().await;
    let file_id = |review: &serde_json::Value, path: &str| {
        review["groups"][0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let b_id = file_id(&review, "b.txt");
    let a_id = file_id(&review, "a.txt");

    for (id, body) in [(&b_id, "b へのコメント"), (&a_id, "a へのコメント")] {
        let response = kemi
            .post(
                "api/comment",
                serde_json::json!({
                    "op": "add", "file_id": id, "side": "new",
                    "start_line": 1, "end_line": 1, "body": body
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
    }

    // a を戻して再取得しても、残る b の id とコメントの対応は変わらない。
    dir.write("a.txt", "a1\n");
    let refreshed = kemi
        .get("api/review?refresh=1")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(file_id(&refreshed, "b.txt"), b_id);

    let file = kemi
        .get(&format!("api/file/{b_id}"))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(file["comments"][0]["body"], "b へのコメント");

    let response = kemi
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let comments = document["comments"].as_array().unwrap();
    let b_comment = comments
        .iter()
        .find(|comment| comment["path"] == "b.txt")
        .unwrap();
    assert_eq!(b_comment["outdated"], false);
    // 一覧から消えたファイルへのコメントは古い扱いで残る。
    let a_comment = comments
        .iter()
        .find(|comment| comment["path"] == "a.txt")
        .unwrap();
    assert_eq!(a_comment["outdated"], true);
}

#[tokio::test]
async fn cli_default_group_by_file_starts_in_final_form() {
    let dir = TempDir::new();
    let base = two_commit_repo(&dir);
    let kemi = Kemi::spawn(&dir.path, &["--from", &base, "--no-open", "--port", "0"]);

    let review = kemi.review_json().await;

    assert_eq!(review["unit"], "file");
    assert_eq!(review["groups"].as_array().unwrap().len(), 1);
    assert_eq!(review["groups"][0]["id"], "all");
    kemi.kill();
}

#[tokio::test]
async fn origin_is_found_even_if_the_global_blame_ignore_revs_file_is_missing() {
    let dir = TempDir::new();
    let base = two_commit_repo(&dir);
    let changed_a = String::from_utf8(run_git(&dir.path, &["rev-parse", "HEAD~1"])).unwrap();
    let state = TempDir::new();
    // 利用者の本物の設定の代わりに、一時ファイルを git の全体設定として読ませる。
    let global = state.path.join("gitconfig");
    std::fs::write(
        &global,
        format!(
            "[blame]\n\tignoreRevsFile = {}\n",
            git_path(&state.path.join("missing-ignore-revs"))
        ),
    )
    .unwrap();
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "a.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changed_a.trim());
    kemi.kill();
}

#[tokio::test]
async fn origin_is_found_in_a_partial_clone_whose_remote_needs_the_global_config() {
    let upstream = TempDir::new();
    let clone = TempDir::new();
    let state = TempDir::new();
    // 取り寄せ先の URL は、全体の設定の url.<base>.insteadOf を通したときだけ upstream に届く。
    let unreachable = file_url(&state.path.join("nowhere"));
    let global = state.path.join("gitconfig");
    std::fs::write(
        &global,
        format!(
            "[user]\n\tname = kemi\n\temail = kemi@example.com\n\
             [core]\n\thooksPath = /dev/null\n\
             [url \"{}\"]\n\tinsteadOf = {unreachable}\n",
            file_url(&upstream.path)
        ),
    )
    .unwrap();
    let git_up = |args: &[&str]| git_with_global(&upstream.path, &global, args);
    git_up(&["init", "-q"]);
    git_up(&["config", "uploadpack.allowFilter", "true"]);
    upstream.write("a.txt", "one\n");
    git_up(&["add", "-A"]);
    git_up(&["commit", "-q", "-m", "base"]);
    let base = git_up(&["rev-parse", "HEAD"]);
    // 途中の版の blob は、差分には要らず blame だけが取り寄せる。
    upstream.write("a.txt", "two\n");
    git_up(&["commit", "-q", "-am", "feat: two"]);
    upstream.write("a.txt", "three\n");
    git_up(&["commit", "-q", "-am", "feat: three"]);
    let changed = git_up(&["rev-parse", "HEAD"]);
    git_with_global(
        &clone.path,
        &global,
        &["clone", "-q", "--filter=blob:none", &unreachable, "."],
    );
    let missing = git_with_global(
        &clone.path,
        &global,
        &["rev-list", "--objects", "--missing=print", "--all"],
    );
    assert!(
        missing.lines().any(|line| line.starts_with('?')),
        "the clone must lack some blobs"
    );
    let mut kemi = start_with_global(
        &clone.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );
    let review = kemi.review_json().await;
    let id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = kemi.get(&format!("api/origin/{id}")).await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changed);
    assert!(kemi.child.try_wait().unwrap().is_none());
    kemi.kill();
}

#[tokio::test]
async fn origin_names_each_block_even_if_a_textconv_driver_rewrites_the_file() {
    let dir = TempDir::new();
    let state = TempDir::new();
    // 先頭の 2 行を落とす変換。変換後の行で数えると、行番号が 2 行ずれる。
    let global = global_config(&state, "[diff \"cut\"]\n\ttextconv = tail -n +3\n");
    let (base, changing, deleting) = textconv_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "f.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changing);
    assert_eq!(origin["blocks"][1]["entries"][0]["sha"], deleting);
    kemi.kill();
}

#[tokio::test]
async fn origin_is_found_even_if_the_textconv_driver_fails() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let global = global_config(&state, "[diff \"cut\"]\n\ttextconv = false\n");
    let (base, changing, deleting) = textconv_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "f.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changing);
    assert_eq!(origin["blocks"][1]["entries"][0]["sha"], deleting);
    kemi.kill();
}

#[tokio::test]
async fn commit_unit_reads_utf8_messages_even_if_log_output_encoding_is_legacy() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let global = global_config(&state, "[i18n]\n\tlogOutputEncoding = Shift_JIS\n");
    let (base, changed) = japanese_message_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &[
            "--from",
            &base,
            "--group-by",
            "commit",
            "--no-open",
            "--port",
            "0",
        ],
    );

    let review = kemi.review_json().await;

    assert_eq!(review["groups"][0]["id"], changed);
    assert_eq!(review["groups"][0]["title"], "feat: 二行目に変える");
    assert_eq!(review["groups"][0]["why"], "理由の本文");
    kemi.kill();
}

#[tokio::test]
async fn origin_reads_utf8_messages_even_if_log_output_encoding_is_legacy() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let global = global_config(&state, "[i18n]\n\tlogOutputEncoding = Shift_JIS\n");
    let (base, changed) = japanese_message_repo(&dir, &global);
    let kemi = start_with_global(
        &dir.path,
        &state,
        &global,
        &["--from", &base, "--no-open", "--port", "0"],
    );

    let response = fetch_origin(&kemi, "a.txt").await;

    assert_eq!(response.status(), 200);
    let origin: serde_json::Value = response.json().await.unwrap();
    assert_eq!(origin["blocks"][0]["entries"][0]["sha"], changed);
    assert_eq!(
        origin["commits"][changed.as_str()]["subject"],
        "feat: 二行目に変える"
    );
    kemi.kill();
}
