use super::*;

#[cfg(unix)]
#[tokio::test]
async fn interrupt_exits_130_without_json_or_a_result_file() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let kemi = Kemi::spawn_with_state(
        &dir.path,
        &["manifest.json", "--no-open", "--port", "0"],
        &state.path,
    );
    kemi.wait_serving().await;

    signal(&kemi.child, "-INT");
    let (exit, stdout) = kemi.wait();

    assert_eq!(exit.code(), Some(130));
    assert!(stdout.is_empty(), "stdout: {stdout}");
    assert!(result_files(&state.path).is_empty());
}

/// 組めないフラグの判定は cli.rs の単体テストが確かめる。ここは拒否が終了コードと stderr に
/// 届く配線だけを見る。
#[test]
fn resume_refuses_other_flags_in_english() {
    let dir = TempDir::new();
    let state = TempDir::new();
    // 復元できるセッションを置く。拒否が無ければ、一覧を出して 0 で終わる。
    craft_session(
        &state.path,
        "01HF7YAT00AAAAAAAAAAAAAAAA",
        SessionMode::Worktree,
        "Working tree changes",
    );

    let output = run_with_state(&dir.path, &["--resume", "--digest"], &state.path);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
    assert!(!contains_japanese(&stderr), "{stderr}");
}

#[cfg(unix)]
#[tokio::test]
async fn interrupt_leaves_a_session_and_prints_the_resume_line() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let state = TempDir::new();
    let kemi = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);
    kemi.wait_serving().await;
    let session = wait_for_frozen_session(&state.path).await;
    let id = session_id(&session);

    signal(&kemi.child, "-INT");
    let (status, stdout, stderr) = kemi.wait_with_stderr();

    assert_eq!(status.code(), Some(130));
    assert!(stdout.is_empty(), "stdout must stay empty: {stdout}");
    assert!(
        stderr.contains(&format!("kemi: resume with: kemi --resume {id}")),
        "stderr: {stderr}"
    );
    assert!(session.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_leaves_a_session_and_prints_the_resume_line() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let state = TempDir::new();
    let kemi = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);
    kemi.wait_serving().await;
    let session = wait_for_frozen_session(&state.path).await;
    let id = session_id(&session);

    signal(&kemi.child, "-TERM");
    let (status, stdout, stderr) = kemi.wait_with_stderr();

    assert_eq!(status.code(), Some(130));
    assert!(stdout.is_empty());
    assert!(
        stderr.contains(&format!("kemi: resume with: kemi --resume {id}")),
        "stderr: {stderr}"
    );
    assert!(session.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn interrupt_without_state_or_copy_leaves_no_session() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let state = TempDir::new();
    let kemi = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);

    // api/review を取らないので凍結は始まらず、状態も空のまま。
    signal(&kemi.child, "-INT");
    let (status, stdout, stderr) = kemi.wait_with_stderr();

    assert_eq!(status.code(), Some(130));
    assert!(stdout.is_empty());
    assert!(
        !stderr.contains("resume with"),
        "no resumable session yet: {stderr}"
    );
    assert!(session_dir_files(&state.path).is_empty());
}

#[test]
fn resume_with_an_unknown_id_exits_2() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let output = run_with_state(
        &dir.path,
        &["--resume", "01HF7YAT00AAAAAAAAAAAAAAAA"],
        &state.path,
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).is_empty());
}

#[test]
fn resume_with_an_id_outside_the_sessions_dir_leaves_the_target_alone() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let sentinel = state.path.join("sentinel.lock");
    std::fs::write(&sentinel, b"keep").unwrap();
    let absolute = state.path.join("sentinel");

    for id in [
        absolute.to_string_lossy().into_owned(),
        "../../sentinel".to_string(),
    ] {
        let output = run_with_state(&dir.path, &["--resume", &id], &state.path);
        assert_eq!(output.status.code(), Some(2), "--resume {id}");
        assert!(output.stdout.is_empty(), "--resume {id}");
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"keep",
            "--resume {id} touched a file outside the sessions directory"
        );
    }
}

#[test]
fn resume_of_an_unfinished_session_exits_2_with_the_reason() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let store = SessionStore::new(sessions_dir(&state.path));
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut open = store
        .create(session_info(
            id,
            SessionMode::Worktree,
            "Working tree changes",
            &state.path,
        ))
        .unwrap();
    open.save_state(SessionState {
        comments: vec![session_comment()],
        ..SessionState::default()
    })
    .unwrap();
    drop(open);

    let output = run_with_state(
        &dir.path,
        &["--resume", id, "--no-open", "--port", "0"],
        &state.path,
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "the reason must reach stderr");
}

#[test]
fn resume_of_a_session_without_its_copy_file_exits_2_and_is_not_listed() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    craft_session(
        &state.path,
        id,
        SessionMode::Worktree,
        "Working tree changes",
    );
    std::fs::remove_file(sessions_dir(&state.path).join(format!("{id}.payload"))).unwrap();

    let output = run_with_state(
        &dir.path,
        &["--resume", id, "--no-open", "--port", "0"],
        &state.path,
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).is_empty());

    // 端末でない `--resume` の一覧にも出ない。
    let listed = run_with_state(&dir.path, &["--resume"], &state.path);
    assert_eq!(listed.status.code(), Some(2));
    assert!(listed.stdout.is_empty());
}

#[test]
fn resume_of_an_unreadable_session_exits_2_with_the_reason() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let sessions = sessions_dir(&state.path);
    std::fs::create_dir_all(&sessions).unwrap();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    std::fs::write(sessions.join(format!("{id}.session")), b"not a session").unwrap();

    let output = run_with_state(
        &dir.path,
        &["--resume", id, "--no-open", "--port", "0"],
        &state.path,
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "the reason must reach stderr");
}

#[tokio::test]
async fn resume_of_a_running_session_exits_2() {
    let dir = TempDir::new();
    worktree_fixture(&dir);
    let state = TempDir::new();
    let kemi = Kemi::spawn_with_state(&dir.path, &["--worktree", "--no-open"], &state.path);
    kemi.wait_serving().await;
    let id = session_id(&wait_for_session(&state.path).await);

    let output = run_with_state(
        &dir.path,
        &["--resume", &id, "--no-open", "--port", "0"],
        &state.path,
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "the reason must reach stderr");
    kemi.kill();
}

/// 列の中身は session.rs の単体テストが、並び順は kemi-core のセッションの一覧のテストが
/// 確かめる。ここは保留したセッションごとに 5 列の 1 行が出ることだけを見る。
#[test]
fn resume_without_an_id_lists_each_session_in_five_columns() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let first = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let second = "01HF7YAT00BBBBBBBBBBBBBBBB";
    // タブと改行を含むタイトルでも、行と列は崩れない。
    craft_session(&state.path, first, SessionMode::Manifest, "a\tb\nc");
    craft_session(
        &state.path,
        second,
        SessionMode::Worktree,
        "Working tree changes",
    );

    let output = run_with_state(&dir.path, &["--resume"], &state.path);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut ids: Vec<&str> = stdout
        .lines()
        .map(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            assert_eq!(columns.len(), 5, "{stdout}");
            columns[0]
        })
        .collect();
    ids.sort();
    assert_eq!(ids, vec![first, second], "{stdout}");
}

#[test]
fn resume_listing_ignores_port_bind_and_no_open() {
    let dir = TempDir::new();
    let state = TempDir::new();
    craft_session(
        &state.path,
        "01HF7YAT00AAAAAAAAAAAAAAAA",
        SessionMode::Worktree,
        "Working tree changes",
    );

    let output = run_with_state(
        &dir.path,
        &[
            "--resume",
            "--port",
            "0",
            "--bind",
            "127.0.0.1",
            "--no-open",
        ],
        &state.path,
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
}

#[test]
fn resume_without_an_id_tells_unreadable_version_sessions_apart_from_having_none() {
    let dir = TempDir::new();
    // 2 回の起動で同じ保存領域を使う。案内には保存領域のパスが入りうるので、別々の
    // 保存領域にすると、パスが違うというだけで 2 つの案内が食い違ってしまう。
    let state = TempDir::new();

    // セッションを 1 つも保留していない場合。
    let none = run_with_state(&dir.path, &["--resume"], &state.path);

    // 読めない版（版 1）の `<id>.session` だけが残っている場合。版 1 の読み手はもう
    // 無いので、バイト列を直に書く。
    let sessions = sessions_dir(&state.path);
    std::fs::create_dir_all(&sessions).unwrap();
    let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
    let mut old = b"kemi-session\n".to_vec();
    old.push(1);
    old.extend_from_slice(&2u32.to_le_bytes());
    old.extend_from_slice(b"{}");
    std::fs::write(sessions.join(format!("{id}.session")), &old).unwrap();

    let output = run_with_state(&dir.path, &["--resume"], &state.path);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    // 文言は契約にしないが、1 つも保留していない場合と同じ案内で終わってはいけない。
    assert_ne!(
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&none.stderr),
        "unreadable-version sessions must not end like having no session at all"
    );
}

#[test]
fn resume_without_sessions_exits_2_with_empty_stdout() {
    let dir = TempDir::new();
    let state = TempDir::new();

    let output = run_with_state(&dir.path, &["--resume"], &state.path);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).is_empty());

    // --port / --bind / --no-open を付けても、使い方の誤りではない。
    let output = run_with_state(
        &dir.path,
        &[
            "--resume",
            "--port",
            "0",
            "--bind",
            "127.0.0.1",
            "--no-open",
        ],
        &state.path,
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn digest_and_result_do_not_create_sessions() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let state = TempDir::new();

    let digest = run_with_state(&dir.path, &["--digest", "manifest.json"], &state.path);
    assert_eq!(digest.status.code(), Some(0));
    assert!(!sessions_dir(&state.path).exists());

    let result = run_with_state(&dir.path, &["--result"], &state.path);
    assert_eq!(result.status.code(), Some(2));
    assert!(!sessions_dir(&state.path).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn resume_keeps_the_frozen_contents_and_state() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id, file_id) = frozen_worktree_review(&dir, &state).await;
    kemi.post(
        "api/comment",
        serde_json::json!({
            "op": "add", "file_id": file_id, "side": "new",
            "start_line": 1, "end_line": 1, "body": "note"
        }),
    )
    .await;
    kemi.post(
        "api/comment",
        serde_json::json!({"op": "resolve", "id": "c1", "resolved": true}),
    )
    .await;
    kemi.post(
        "api/state",
        serde_json::json!({"file_id": file_id, "seen": true, "collapsed": true}),
    )
    .await;
    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));

    // 保留の間にワークツリーを変える。復元はこの変更を読まない。
    std::fs::write(dir.path.join("a.txt"), "totally\nchanged\n").unwrap();
    std::fs::write(dir.path.join("b.txt"), "added\n").unwrap();

    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    resumed.wait_serving().await;
    let review = resumed.get_json("api/review").await;
    let files = review["groups"][0]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{review}");
    assert_eq!(files[0]["path"], "a.txt");
    assert_eq!(files[0]["seen"], true);
    assert_eq!(files[0]["collapsed"], true);
    assert_eq!(review["comments"][0]["body"], "note");
    assert_eq!(review["comments"][0]["resolved"], true);

    let file = resumed.get_json(&format!("api/file/{file_id}")).await;
    let texts: Vec<&str> = file["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["new"]["text"].as_str())
        .collect();
    assert!(texts.contains(&"TWO"), "{texts:?}");
    assert!(
        !texts.iter().any(|text| text.contains("changed")),
        "{texts:?}"
    );

    // 復元は同じ id の続きで、セッションは増えない。
    assert_eq!(session_files(&state.path).len(), 1);
    assert_eq!(session_id(&session_files(&state.path)[0]), id);
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn resume_writes_state_back_to_the_same_session() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id, file_id) = frozen_worktree_review(&dir, &state).await;
    kemi.post(
        "api/comment",
        serde_json::json!({
            "op": "add", "file_id": file_id, "side": "new",
            "start_line": 1, "end_line": 1, "body": "first"
        }),
    )
    .await;
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();

    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    resumed.wait_serving().await;
    let added = resumed
        .post(
            "api/comment",
            serde_json::json!({
                "op": "add", "file_id": file_id, "side": "new",
                "start_line": 1, "end_line": 1, "body": "second"
            }),
        )
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(added["id"], "c2", "comment ids must not be reused");
    signal(&resumed.child, "-INT");
    resumed.wait_with_stderr();

    let store = SessionStore::new(sessions_dir(&state.path));
    let stored = store.read(&id).unwrap();
    let ids: Vec<&str> = stored
        .state
        .comments
        .iter()
        .map(|comment| comment.id.as_str())
        .collect();
    assert_eq!(ids, vec!["c1", "c2"]);
    assert_eq!(session_files(&state.path).len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn resume_without_the_repository_keeps_origin_unknown() {
    let dir = TempDir::new();
    let base = two_commit_repo(&dir);
    let state = TempDir::new();
    let kemi = Kemi::spawn_with_state(
        &dir.path,
        &["--from", &base, "--no-open", "--port", "0"],
        &state.path,
    );
    kemi.wait_serving().await;
    let id = session_id(&wait_for_frozen_session(&state.path).await);
    let review = kemi.get_json("api/review").await;
    let file_id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();

    // 元のワークスペースを消しても復元でき、由来だけ「特定できない」になる。
    std::fs::remove_dir_all(&dir.path).unwrap();
    let elsewhere = TempDir::new();
    let resumed = Kemi::spawn_with_state(
        &elsewhere.path,
        &["--resume", &id, "--no-open"],
        &state.path,
    );
    resumed.wait_serving().await;

    let review = resumed.get_json("api/review").await;
    assert_eq!(review["groups"][0]["files"][0]["path"], "a.txt");
    let origin = resumed.get_json(&format!("api/origin/{file_id}")).await;
    assert_eq!(origin["available"], false, "{origin}");
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn resume_submit_uses_the_original_workspace_for_the_result() {
    let dir = TempDir::new();
    dir.write("manifest.json", MANIFEST);
    let state = TempDir::new();
    let kemi = Kemi::spawn_with_state(&dir.path, &["manifest.json", "--no-open"], &state.path);
    kemi.wait_serving().await;
    let id = session_id(&wait_for_frozen_session(&state.path).await);
    let original_url = kemi.url.clone();
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();

    // 別のディレクトリから復元し、submit する。
    let elsewhere = TempDir::new();
    let resumed = Kemi::spawn_with_state(
        &elsewhere.path,
        &["--resume", &id, "--no-open"],
        &state.path,
    );
    resumed.wait_serving().await;
    assert_ne!(resumed.url, original_url, "token must be new");
    let response = resumed
        .post("api/submit", serde_json::json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let (status, stdout, _) = resumed.wait_with_stderr();

    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        document["approval"],
        serde_json::json!([{ "path": "a.txt", "identity": "sha256:e2e" }])
    );
    // 結果ファイルは、記録した元のワークスペースの識別で置かれる。
    let result = run_with_state(
        &elsewhere.path,
        &["--result", "--workspace", dir.path.to_str().unwrap()],
        &state.path,
    );
    assert_eq!(result.status.code(), Some(0));
    let from_file: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(from_file["approval"], document["approval"]);
    assert!(session_dir_files(&state.path).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn suspending_removes_the_endpoint_and_ends_the_waiting_wait_with_2() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let waiting = spawn_wait(&dir.path, &state.path, &["wait", &id]);
    kemi.wait_agent_status("waiting").await;

    signal(&kemi.child, "-INT");
    let (status, stdout, _) = kemi.wait_with_stderr();

    assert_eq!(status.code(), Some(130));
    assert!(stdout.is_empty());
    let (code, wait_stdout, wait_stderr) = finish_agent(waiting).await;
    assert_eq!(code, Some(2));
    assert!(wait_stdout.is_empty());
    assert!(!wait_stderr.trim().is_empty());
    assert!(
        !endpoint_dir(&state.path)
            .join(format!("{id}.endpoint"))
            .exists()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_hand_made_before_suspending_is_returned_after_resuming() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.wait_serving().await;
    wait_for_frozen_session(&state.path).await;
    kemi.add_comment(1, "rename this").await;
    kemi.hand().await;
    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));

    let mut resumed =
        Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    assert_eq!(resumed.review_id(), id);
    let output = run_agent(&dir.path, &state.path, &["wait", &id], "");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("rename this"));
    let review = resumed.review_json().await;
    assert_eq!(review["agent"]["called"], true);
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn live_suspend_and_resume_bring_back_the_comment_and_the_message() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    let mut kemi =
        Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);
    let id = kemi.review_id();
    kemi.wait_serving().await;
    kemi.add_live_comment("fix the wording").await;
    let response = kemi
        .post("api/message", serde_json::json!({ "body": "looks close" }))
        .await;
    assert_eq!(response.status(), 200);
    signal(&kemi.child, "-INT");
    let (status, _, stderr) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));
    assert!(
        stderr.contains(&format!("kemi: resume with: kemi --resume {id}")),
        "{stderr}"
    );
    assert!(
        !sessions_dir(&state.path)
            .join(format!("{id}.payload"))
            .exists()
    );

    // 保留の間に作業ツリーを変えると、復元したコードの見方は今の作業ツリーを読む。
    std::fs::write(dir.path.join("b.txt"), "new\n").unwrap();
    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    let review = resumed.review_json().await;

    assert_eq!(review["title"], format!("Live review of {LIVE_URL}"));
    assert_eq!(review["comments"][0]["body"], "fix the wording");
    assert_eq!(review["messages"][0]["body"], "looks close");
    let paths: Vec<&str> = review["groups"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert!(paths.contains(&"b.txt"), "{review}");
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn live_session_with_only_seen_and_collapsed_is_not_kept() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    let kemi = Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);
    kemi.wait_serving().await;
    let response = kemi
        .post(
            "api/state",
            serde_json::json!({ "file_id": kemi.first_file_id().await, "seen": true, "collapsed": true }),
        )
        .await;
    assert_eq!(response.status(), 200);

    signal(&kemi.child, "-INT");
    let (status, _, stderr) = kemi.wait_with_stderr();

    assert_eq!(status.code(), Some(130));
    assert!(!stderr.contains("resume with"), "{stderr}");
    assert!(session_dir_files(&state.path).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn live_session_is_listed_with_live_and_the_page_url() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    let mut kemi =
        Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);
    let id = kemi.review_id();
    kemi.wait_serving().await;
    kemi.add_live_comment("keep").await;
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();

    let output = run_with_state(&dir.path, &["--resume"], &state.path);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let columns: Vec<&str> = stdout.lines().next().unwrap().split('\t').collect();
    assert_eq!(columns[0], id);
    assert_eq!(columns[3], format!("live {LIVE_URL}"));
}

#[cfg(unix)]
#[tokio::test]
async fn resume_of_a_live_review_accepts_a_live_port_and_listens_there() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    let mut kemi =
        Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);
    let id = kemi.review_id();
    kemi.wait_serving().await;
    kemi.add_live_comment("keep").await;
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();
    // ポートを解放してから子が bind するまでに、別のテストが取得しうるため。中継の待ち受けは
    // URL の行より前なので、取られた kemi は URL を出さずに終わる。
    for attempt in 0..5 {
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        let port_text = port.to_string();
        let exited = match Kemi::try_start(kemi_command(&dir.path, &state.path).args([
            "--resume",
            &id,
            "--live-port",
            &port_text,
            "--no-open",
        ])) {
            Ok(mut resumed) => {
                assert_eq!(resumed.live_url(), format!("http://127.0.0.1:{port}/app"));
                resumed.kill();
                return;
            }
            Err(exited) => exited,
        };
        let code = exited.status.as_ref().ok().and_then(|status| status.code());
        assert!(
            code == Some(2)
                && exited.stderr.lines().any(|line| {
                    line.starts_with(&format!("kemi: cannot listen on 127.0.0.1:{port}: "))
                }),
            "中継ポートの待ち受け失敗以外で終了した: {:?}\n{}",
            exited.status,
            exited.stderr
        );
        assert!(
            attempt < 4,
            "中継ポートの待ち受けを5回試しても失敗した: {}",
            exited.stderr
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn live_file_review_resumed_after_the_file_is_gone_says_so_and_waits() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    dir.write("page.html", "<p>page</p>");
    let mut kemi = Kemi::spawn_with_state(
        &dir.path,
        &["--live", "page.html", "--no-open"],
        &state.path,
    );
    let id = kemi.review_id();
    kemi.wait_serving().await;
    kemi.add_live_comment("keep").await;
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();
    std::fs::remove_file(dir.path.join("page.html")).unwrap();

    let mut resumed =
        Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    let live = resumed.live_url();
    let (status, body) = read_live_page(&resumed, &live).await;

    assert_eq!(status, 404, "{body}");
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn live_resume_keeps_comments_and_seen_marks_on_the_same_files() {
    let dir = TempDir::new();
    let state = TempDir::new();
    worktree_fixture(&dir);
    std::fs::write(dir.path.join("0.txt"), "zero\n").unwrap();
    git(&dir.path, &["add", "0.txt"]);
    git(&dir.path, &["commit", "-q", "-m", "zero"]);
    let mut kemi =
        Kemi::spawn_with_state(&dir.path, &["--live", LIVE_URL, "--no-open"], &state.path);
    let id = kemi.review_id();
    let a = file_id(&kemi.review_json().await, "a.txt");
    kemi.post(
        "api/comment",
        serde_json::json!({
            "op": "add", "file_id": a, "side": "new",
            "start_line": 2, "end_line": 2, "body": "on a.txt"
        }),
    )
    .await;
    kemi.post(
        "api/state",
        serde_json::json!({ "file_id": a, "seen": true }),
    )
    .await;
    signal(&kemi.child, "-INT");
    kemi.wait_with_stderr();
    // 保留の間に、名前の順で前に来るファイルが変わる。
    std::fs::write(dir.path.join("0.txt"), "ZERO\n").unwrap();

    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    let review = resumed.review_json().await;
    let a = file_id(&review, "a.txt");
    let zero = file_id(&review, "0.txt");
    let file = resumed.get_json(&format!("api/file/{a}")).await;
    let other = resumed.get_json(&format!("api/file/{zero}")).await;

    assert_eq!(file["comments"][0]["body"], "on a.txt", "{file}");
    assert_eq!(other["comments"].as_array().unwrap().len(), 0, "{other}");
    assert_eq!(file_entry(&review, "a.txt")["seen"], true);
    assert_eq!(file_entry(&review, "0.txt")["seen"], false);
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn a_saved_page_comment_stays_marked_after_deleting_it_and_resuming() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    kemi.wait_serving().await;
    assert_eq!(
        kemi.review_json().await["live"]["page_comment_saved"],
        false
    );

    let first = kemi.add_page_comment("first").await;
    kemi.add_page_comment("second").await;
    let deleted = kemi
        .post(
            "api/comment",
            serde_json::json!({ "op": "delete", "id": first["id"] }),
        )
        .await;
    assert_eq!(deleted.status(), 200);
    assert_eq!(kemi.review_json().await["live"]["page_comment_saved"], true);

    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));
    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    let review = resumed.review_json().await;

    assert_eq!(review["live"]["page_comment_saved"], true, "{review}");
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn a_page_comment_handed_before_suspending_comes_back_with_its_image_after_resuming() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    kemi.add_page_comment("1 is too large").await;
    kemi.hand().await;
    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));

    let mut resumed =
        Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    assert_eq!(resumed.review_id(), id);
    let review = resumed.review_json().await;
    let comment = handed_comment(&run_agent(&dir.path, &state.path, &["wait", &id], ""));

    assert_eq!(
        review["comments"][0]["page"]["places"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(review["comments"][0]["page"]["url"], "/products?x=1");
    let image = Path::new(comment["page"]["image"].as_str().unwrap());
    assert_eq!(std::fs::read(image).unwrap(), page_image().0);
    resumed.kill();
}

/// ページへのコメントを、場所の点を `points` から (x, y) の並びとして読む。
fn place_points(comment: &serde_json::Value) -> Vec<Vec<(f64, f64)>> {
    comment["page"]["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|place| {
            place["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|point| (point["x"].as_f64().unwrap(), point["y"].as_f64().unwrap()))
                .collect()
        })
        .collect()
}

/// 版 4 より前の kemi で保存したページへのコメントの点は、復元してから渡しても submit しても、
/// 点の数も値も保存したときのまま出る。版 4 の kemi で置いた点の座標は、`kemi wait` と submit の
/// stdout で `12` と書き、`12.0` と書かない（live.md の R-PAGE-COMMENT）。サーバは受け取った点を
/// 変えずに保存し、セッションの形式も版 4 の前と同じなので、版 4 の前の画面が送った点（小数を
/// 含み、版 4 の手順なら間引かれる点を残した矢印とペン）を API で置いて保留すれば、版 4 の前の
/// kemi が保存したセッションと同じになる。
#[cfg(unix)]
#[tokio::test]
async fn page_comment_points_keep_their_saved_values_after_resuming_and_new_ones_are_whole_numbers()
{
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    let saved = serde_json::json!([
        { "n": 1, "kind": "arrow", "points": [{ "x": 0, "y": 0 }, { "x": 5, "y": 0.5 }, { "x": 10, "y": 0 }] },
        { "n": 2, "kind": "pen", "points": [{ "x": 10, "y": 20.5 }, { "x": 15.25, "y": 20.5 }, { "x": 20, "y": 20.5 }] },
    ]);
    let saved_points = vec![
        vec![(0.0, 0.0), (5.0, 0.5), (10.0, 0.0)],
        vec![(10.0, 20.5), (15.25, 20.5), (20.0, 20.5)],
    ];
    let response = kemi
        .post(
            "api/comment",
            serde_json::json!({
                "op": "add_page", "body": "saved before version 4",
                "page": { "url": "/app", "width": 390, "places": saved },
            }),
        )
        .await;
    assert_eq!(response.status(), 200);
    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));
    let mut resumed =
        Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    assert_eq!(resumed.review_id(), id);
    let response = resumed
        .post(
            "api/comment",
            serde_json::json!({
                "op": "add_page", "body": "placed by version 4",
                "page": { "url": "/app", "width": 390, "places": [
                    { "n": 1, "kind": "pen", "points": [{ "x": 12, "y": 340 }, { "x": 40, "y": 340 }, { "x": 40, "y": 400 }] },
                ] },
            }),
        )
        .await;
    assert_eq!(response.status(), 200);
    resumed.hand().await;
    let whole = r#""points":[{"x":12,"y":340},{"x":40,"y":340},{"x":40,"y":400}]"#;

    let waited = run_agent(&dir.path, &state.path, &["wait", &id], "");
    resumed.submit("changes_requested").await;
    let (status, submitted) = resumed.wait();

    assert_eq!(waited.status.code(), Some(0), "{waited:?}");
    assert_eq!(status.code(), Some(1));
    let waited = String::from_utf8(waited.stdout).unwrap();
    let answer: serde_json::Value = serde_json::from_str(&waited).unwrap();
    let handed = &answer["events"][0]["comments"];
    assert_eq!(handed[0]["comment"]["body"], "saved before version 4");
    assert_eq!(place_points(&handed[0]["comment"]), saved_points);
    let result: serde_json::Value = serde_json::from_str(&submitted).unwrap();
    assert_eq!(result["comments"][0]["body"], "saved before version 4");
    assert_eq!(place_points(&result["comments"][0]), saved_points);
    for written in [&waited, &submitted] {
        assert!(written.contains(whole), "{written}");
        assert!(!written.contains(r#""x":12.0"#), "{written}");
    }
}

#[tokio::test]
async fn submitting_removes_the_files_of_the_session() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    kemi.add_page_comment("1 is too large").await;
    let files = sessions_dir(&state.path).join(format!("{id}.files"));
    assert!(files.is_dir(), "the image is kept with the session");

    kemi.submit("changes_requested").await;
    let (status, _) = kemi.wait();

    assert_eq!(status.code(), Some(1));
    assert!(!files.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn a_start_snapshot_taken_before_the_first_comment_comes_back_after_resuming() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    let taken = kemi.take_start_snapshot().await;
    // コメント無しで見たの印を付けても、開始時のスナップショットは残る。
    let response = kemi
        .post(
            "api/state",
            serde_json::json!({ "file_id": kemi.first_file_id().await, "seen": true }),
        )
        .await;
    assert_eq!(response.status(), 200);
    kemi.add_live_comment("keep").await;
    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));

    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    let listed = resumed.get_json("api/snapshots").await;
    let snapshot = resumed
        .get_json(&format!("api/snapshot/{}", taken["id"].as_str().unwrap()))
        .await;

    assert_eq!(listed["snapshots"].as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed["snapshots"][0]["id"], taken["id"]);
    assert_eq!(listed["snapshots"][0]["kind"], "start");
    assert_eq!(snapshot["html"], "<p>at the start</p>");
    assert_eq!(snapshot["description"], "H4sIdescribed");
    resumed.kill();
}

#[cfg(unix)]
#[tokio::test]
async fn a_live_review_suspended_without_a_comment_leaves_neither_its_session_nor_its_files() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    kemi.take_start_snapshot().await;
    assert!(
        sessions_dir(&state.path)
            .join(format!("{id}.files"))
            .is_dir()
    );

    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();

    assert_eq!(status.code(), Some(130));
    assert!(session_dir_files(&state.path).is_empty());
    assert!(
        !sessions_dir(&state.path)
            .join(format!("{id}.files"))
            .exists()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_mock_assignment_comes_back_after_resuming() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    dir.write("mocks/products.html", "<p>mock</p>\n");
    let assigned = kemi
        .post(
            "api/mock",
            serde_json::json!({ "page": "/products?x=1", "path": "mocks/products.html" }),
        )
        .await;
    assert_eq!(assigned.status(), 200);
    kemi.add_live_comment("keep").await;
    signal(&kemi.child, "-INT");
    let (status, _, _) = kemi.wait_with_stderr();
    assert_eq!(status.code(), Some(130));

    let resumed = Kemi::spawn_with_state(&dir.path, &["--resume", &id, "--no-open"], &state.path);
    let mocks = resumed.get_json("api/mocks").await;

    assert_eq!(mocks["mocks"].as_array().unwrap().len(), 1, "{mocks}");
    assert_eq!(mocks["mocks"][0]["page"], "/products?x=1");
    assert_eq!(mocks["mocks"][0]["path"], "mocks/products.html");
    resumed.kill();
}
