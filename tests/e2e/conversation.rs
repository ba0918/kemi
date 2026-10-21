use super::*;

/// 期限に u64 の上限を渡しても、ミリ秒への換算で桁あふれせずに、渡されるまで待つ。
#[tokio::test]
async fn wait_with_the_largest_timeout_returns_the_comment_handed_while_it_waited() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let largest = u64::MAX.to_string();
    let waiting = spawn_wait(
        &dir.path,
        &state.path,
        &["wait", &id, "--timeout", &largest],
    );
    kemi.wait_agent_status("waiting").await;

    kemi.add_comment(1, "rename this").await;
    kemi.hand().await;

    let (code, stdout, stderr) = finish_agent(waiting).await;
    assert_eq!(code, Some(0), "{stderr}");
    let answer: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(answer["kemi"], 3);
    assert_eq!(answer["review"], id.as_str());
    assert_eq!(answer["events"][0]["type"], "handed");
    assert_eq!(
        answer["events"][0]["comments"][0]["comment"]["body"],
        "rename this"
    );
    kemi.kill();
}

#[tokio::test]
async fn what_one_wait_returned_is_not_returned_by_the_next_wait() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.add_comment(1, "rename this").await;
    kemi.hand().await;
    let first = run_agent(&dir.path, &state.path, &["wait", &id], "");
    assert_eq!(first.status.code(), Some(0), "{first:?}");

    let second = run_agent(&dir.path, &state.path, &["wait", &id, "--timeout", "1"], "");

    assert_eq!(second.status.code(), Some(3), "{second:?}");
    assert!(second.stdout.is_empty());
    kemi.kill();
}

#[tokio::test]
async fn reply_writes_a_reply_and_a_message_that_reach_the_submit() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.add_comment(1, "rename this").await;

    let output = run_agent(
        &dir.path,
        &state.path,
        &["reply", &id],
        r#"{"writes":[{"type":"reply","comment_id":"c1","body":"renamed"},{"type":"message","body":"all done"}]}"#,
    );

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let ids: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(ids["ids"].as_array().map(Vec::len), Some(2), "{ids}");
    kemi.submit("approved").await;
    let (status, stdout) = kemi.wait();
    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    // 書いた順の id が、書いた返信と発言のものになる。
    assert_eq!(document["comments"][0]["replies"][0]["id"], ids["ids"][0]);
    assert_eq!(document["messages"][0]["id"], ids["ids"][1]);
    assert_eq!(document["comments"][0]["replies"][0]["author"], "agent");
    assert_eq!(document["comments"][0]["replies"][0]["body"], "renamed");
    assert_eq!(document["messages"][0]["body"], "all done");
}

#[tokio::test]
async fn approving_ends_the_waiting_wait_with_the_result_and_code_0() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let waiting = spawn_wait(&dir.path, &state.path, &["wait", &id]);
    kemi.wait_agent_status("waiting").await;

    kemi.submit("approved").await;

    let (code, stdout, _) = finish_agent(waiting).await;
    assert_eq!(code, Some(0));
    let answer: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let submitted = answer["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(submitted["type"], "submitted");
    assert_eq!(submitted["result"]["verdict"], "approved");
    let (status, review_stdout) = kemi.wait();
    assert_eq!(status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_str(review_stdout.trim()).unwrap();
    assert_eq!(submitted["result"], document);
}

#[tokio::test]
async fn requesting_changes_ends_the_waiting_wait_with_code_1() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let waiting = spawn_wait(&dir.path, &state.path, &["wait", &id]);
    kemi.wait_agent_status("waiting").await;

    kemi.submit("changes_requested").await;

    let (code, stdout, _) = finish_agent(waiting).await;
    assert_eq!(code, Some(1));
    assert!(stdout.contains("changes_requested"));
    let (status, _) = kemi.wait();
    assert_eq!(status.code(), Some(1));
}

#[tokio::test]
async fn wait_with_a_timeout_and_nothing_happening_exits_3_with_empty_stdout() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);

    let started = std::time::Instant::now();
    let output = run_agent(&dir.path, &state.path, &["wait", &id, "--timeout", "1"], "");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(started.elapsed() >= std::time::Duration::from_millis(900));
    kemi.kill();
}

#[test]
fn wait_and_reply_for_an_unknown_id_exit_2() {
    let dir = TempDir::new();
    let state = TempDir::new();

    for args in [
        vec!["wait", "01HF7YAT00ZZZZZZZZZZZZZZZZ"],
        vec!["reply", "01HF7YAT00ZZZZZZZZZZZZZZZZ"],
        vec!["wait", "../../etc/passwd"],
    ] {
        // kemi wait は stdin を読まない（reply は読むが、ここでは id を断ることだけを見る）。
        let output = run_command(
            kemi_command(&dir.path, &state.path).args(&args),
            Some(Input::MayBeUnread("{\"writes\":[]}")),
        );
        assert_eq!(output.status.code(), Some(2), "kemi {args:?}");
        assert!(output.stdout.is_empty(), "kemi {args:?}");
        assert!(!output.stderr.is_empty(), "kemi {args:?}");
    }
}

#[tokio::test]
async fn wait_after_the_submit_exits_2_and_points_to_the_result() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.submit("approved").await;
    let (status, _) = kemi.wait();
    assert_eq!(status.code(), Some(0));

    let output = run_agent(&dir.path, &state.path, &["wait", &id], "");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("kemi --result"), "{stderr}");
}

/// wait / reply / explain が受けない引数の判定は agent.rs の単体テストが確かめる。ここは
/// 拒否が終了コードと stderr に届く配線だけを見る。
#[tokio::test]
async fn an_agent_command_refuses_a_flag_it_does_not_take_in_english() {
    let dir = TempDir::new();
    let state = TempDir::new();
    // 動いているレビューの id を使う。拒否が無ければ、書き込みを渡して 0 で終わる。
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);

    // 引数を断るときは stdin を読む前に終わる。
    let output = run_command(
        kemi_command(&dir.path, &state.path).args(["reply", &id, "--no-open"]),
        Some(Input::MayBeUnread(r#"{"writes":[]}"#)),
    );

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
    assert!(!contains_japanese(&stderr), "{stderr}");
    kemi.kill();
}

#[tokio::test]
async fn a_review_where_kemi_wait_is_never_called_gives_the_page_its_id_and_still_submits() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let agent = kemi.review_json().await["agent"].clone();
    assert_eq!(agent["called"], false, "{agent}");
    assert_eq!(agent["status"], "not-connected", "{agent}");
    // 画面は「Hand to agent」を押せないまま出し、この id で kemi wait <id> を写させる（R-AGENT-STATE）。
    assert_eq!(agent["review"], id.as_str(), "{agent}");

    kemi.submit("approved").await;
    let (status, stdout) = kemi.wait();

    assert_eq!(status.code(), Some(0));
    assert!(stdout.contains("\"verdict\":\"approved\""), "{stdout}");
}

#[cfg(unix)]
#[tokio::test]
async fn the_endpoint_is_owner_only_and_gone_after_the_submit() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    let endpoint = endpoint_dir(&state.path).join(format!("{id}.endpoint"));
    let mode = std::fs::metadata(&endpoint).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    kemi.submit("approved").await;
    let (status, _) = kemi.wait();

    assert_eq!(status.code(), Some(0));
    assert!(!endpoint.exists());
}

#[test]
fn a_stale_endpoint_without_a_running_review_is_treated_as_missing() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let id = "01HF7YAT00YYYYYYYYYYYYYYYY";
    // 強制終了で残ったつなぎ先。ロックは無い。待ち受けの無いポートを指す。
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    std::fs::create_dir_all(endpoint_dir(&state.path)).unwrap();
    std::fs::write(
        endpoint_dir(&state.path).join(format!("{id}.endpoint")),
        format!("{{\"port\":{port},\"token\":\"stale\"}}"),
    )
    .unwrap();

    let output = run_agent(&dir.path, &state.path, &["wait", id], "");

    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn wait_and_reply_work_when_the_page_listens_on_another_loopback_address() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &["--bind", "127.0.0.2"]);
    assert!(kemi.url.starts_with("http://127.0.0.2:"), "{}", kemi.url);
    kemi.add_comment(1, "rename this").await;
    kemi.hand().await;

    let waited = run_agent(&dir.path, &state.path, &["wait", &id], "");
    let replied = run_agent(
        &dir.path,
        &state.path,
        &["reply", &id],
        r#"{"writes":[{"type":"message","body":"ok"}]}"#,
    );

    assert_eq!(waited.status.code(), Some(0), "{waited:?}");
    assert!(String::from_utf8_lossy(&waited.stdout).contains("rename this"));
    assert_eq!(replied.status.code(), Some(0), "{replied:?}");
    kemi.kill();
}

#[tokio::test]
async fn refused_replies_exit_2_and_add_nothing_to_the_submit() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.add_comment(1, "rename this").await;
    let too_long = "x".repeat(64 * 1024 + 1);
    let over_limit = serde_json::json!({ "writes": [
        { "type": "message", "body": "fine" },
        { "type": "reply", "comment_id": "c1", "body": too_long },
    ]})
    .to_string();
    let with_variants = r#"{"writes":[{"type":"reply","comment_id":"c1","body":"two ideas","variants":[{"label":"A","replace":[],"css":""}]}]}"#;

    for input in [over_limit.as_str(), with_variants, "not json"] {
        let output = run_agent(&dir.path, &state.path, &["reply", &id], input);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }

    kemi.submit("approved").await;
    let (_, stdout) = kemi.wait();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(document["comments"][0]["replies"], serde_json::json!([]));
    assert_eq!(document["messages"], serde_json::json!([]));
}

#[tokio::test]
async fn a_handed_comment_has_the_same_keys_as_in_the_submit() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_worktree_review(&dir, &state, &[]);
    kemi.add_comment(1, "rename this").await;
    kemi.hand().await;

    let output = run_agent(&dir.path, &state.path, &["wait", &id], "");
    kemi.submit("approved").await;
    let (_, stdout) = kemi.wait();

    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer["kemi"], 3);
    assert_eq!(answer["review"], id.as_str());
    let handed = &answer["events"][0]["comments"][0]["comment"];
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let submitted = &document["comments"][0];
    assert_eq!(handed["id"], submitted["id"]);
    assert_eq!(sorted_keys(handed), sorted_keys(submitted));
}

#[test]
fn a_manifest_named_wait_opens_with_a_path() {
    let dir = TempDir::new();
    dir.write("wait", MANIFEST);

    let output = run(&dir.path, &["--digest", "./wait"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let digest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(digest["title"], "e2e のレビュー");
}

#[tokio::test]
async fn a_handed_page_comment_reaches_wait_with_its_places_and_a_readable_image() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    let added = kemi.add_page_comment("1 is too large").await;
    kemi.hand().await;

    let comment = handed_comment(&run_agent(&dir.path, &state.path, &["wait", &id], ""));

    assert_eq!(comment["id"], added["id"]);
    let page = &comment["page"];
    assert_eq!(page["url"], "/products?x=1");
    assert_eq!(page["width"], 390);
    let places = page["places"].as_array().unwrap();
    let numbers: Vec<u64> = places
        .iter()
        .map(|place| place["n"].as_u64().unwrap())
        .collect();
    let kinds: Vec<&str> = places
        .iter()
        .map(|place| place["kind"].as_str().unwrap())
        .collect();
    assert_eq!(numbers, vec![1, 2, 3]);
    assert_eq!(kinds, vec!["element", "arrow", "pen"]);
    for place in places {
        assert_eq!(place["elements"][0]["selector"], "#buy");
        assert_eq!(place["elements"][0]["text"], "Buy");
        assert_eq!(place["elements"][0]["rect"]["w"], 200.0);
    }
    assert_eq!(
        places[1]["points"][1],
        serde_json::json!({ "x": 100.0, "y": 150.0 })
    );
    let image = Path::new(page["image"].as_str().unwrap());
    assert!(image.is_absolute(), "{image:?}");
    assert_eq!(std::fs::read(image).unwrap(), page_image().0);
    kemi.kill();
}

#[tokio::test]
async fn a_submitted_page_comment_has_no_image_and_the_keys_it_had_when_handed() {
    let dir = TempDir::new();
    let state = TempDir::new();
    let (kemi, id) = start_live_review(&dir, &state);
    kemi.add_page_comment("1 is too large").await;
    kemi.hand().await;
    let handed = handed_comment(&run_agent(&dir.path, &state.path, &["wait", &id], ""));

    kemi.submit("approved").await;
    let (_, stdout) = kemi.wait();

    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let submitted = &document["comments"][0];
    assert!(submitted["page"]["image"].is_null(), "{submitted}");
    assert_eq!(submitted["page"]["places"], handed["page"]["places"]);
    for key in ["path", "side", "start_line", "end_line", "suggestion"] {
        assert!(submitted[key].is_null(), "{key}: {submitted}");
    }
    assert_eq!(submitted["quote"], serde_json::json!([]));
    assert_eq!(submitted["outdated"], false);
    assert_eq!(submitted["group_id"], "page");
    assert_eq!(submitted["group_title"], "Page");
    assert_eq!(sorted_keys(&handed), sorted_keys(submitted));
    assert_eq!(
        sorted_keys(&handed["page"]),
        sorted_keys(&submitted["page"])
    );
}

#[tokio::test]
async fn a_page_comment_is_added_and_handed_in_a_live_review_outside_git() {
    let dir = TempDir::new();
    let state = TempDir::new();
    dir.write("page.html", "<p>hello</p>\n");
    let mut kemi = Kemi::spawn_with_state(
        &dir.path,
        &["--live", "page.html", "--no-open"],
        &state.path,
    );
    let id = kemi.review_id();
    kemi.add_page_comment("outside git").await;
    kemi.hand().await;

    let comment = handed_comment(&run_agent(&dir.path, &state.path, &["wait", &id], ""));

    assert_eq!(comment["body"], "outside git");
    assert_eq!(comment["group_id"], "page");
    assert!(comment["page"]["image"].is_string(), "{comment}");
    kemi.kill();
}
