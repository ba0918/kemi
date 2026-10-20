use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_copy_stays_at_d1_while_d2_is_published_and_its_opposite_unit_is_pending() {
    check_initial_copy_order(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_d2_opposite_unit_can_finish_before_the_initial_d1_copy_without_rewinding() {
    check_initial_copy_order(true).await;
}

#[tokio::test]
async fn a_failed_opposite_unit_candidate_keeps_both_published_lists_and_the_generation() {
    let repo = TempRepo::new();
    repo.write("a.txt", "base\n");
    let base = repo.commit("base");
    repo.write("a.txt", "D1\n");
    repo.commit("D1");
    let fail = Arc::new(AtomicBool::new(false));
    let sink = Arc::new(RecordingSink::default());
    sink.copy_already_saved.store(true, Ordering::SeqCst);
    let server = AgentServer::start_with_sink(
        Arc::new(UnitFailureSource {
            inner: Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
            fail: fail.clone(),
        }),
        Ipv4Addr::LOCALHOST,
        sink,
    )
    .await;
    let first: Value = server.page.get("api/review").await.json().await.unwrap();
    let other: Value = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let response = server.page.get("api/review?unit=commit").await;
            if response.status() == 200 {
                break response.json().await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    repo.write("b.txt", "D2\n");
    repo.commit("D2");
    fail.store(true, Ordering::SeqCst);
    assert_eq!(server.page.get("api/review?refresh=1").await.status(), 500);
    let unchanged: Value = server.page.get("api/review").await.json().await.unwrap();
    let unchanged_other: Value = server
        .page
        .get("api/review?unit=commit")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(unchanged["groups"], first["groups"]);
    assert_eq!(unchanged_other["groups"], other["groups"]);
    assert_eq!(
        unchanged["diff_display_version"],
        first["diff_display_version"]
    );
    fail.store(false, Ordering::SeqCst);
    let current: Value = server
        .page
        .get("api/review?refresh=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(current["diff_display_version"], "01TESTREVIEW:2");
    server.page.task.abort();
}

#[tokio::test]
async fn unit_background_first_review_does_not_wait_for_the_other_unit() {
    let (repo, base) = range_repo();
    let (release, gate) = std::sync::mpsc::channel();
    let source = Arc::new(GatedSource {
        inner: Box::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
        gated: GroupBy::Commit,
        gate: std::sync::Mutex::new(gate),
    });
    let server = LiveServer::start(source).await;

    let review = tokio::time::timeout(Duration::from_secs(2), server.get_json("api/review"))
        .await
        .expect("api/review must not wait for the other unit");

    assert_eq!(review["unit"], "file");
    assert_eq!(review["units"][0]["unit"], "file");
    assert_eq!(review["units"][1]["unit"], "commit");
    assert_eq!(review["units"][1]["state"], "building");
    release.send(()).unwrap();
    server.wait_unit("ready").await;
    let per_commit = server.get_json("api/review?unit=commit").await;
    assert_eq!(per_commit["unit"], "commit");
    assert_eq!(per_commit["groups"].as_array().unwrap().len(), 2);
    server.stop();
}

#[tokio::test]
async fn the_unit_event_carries_the_units_of_the_review() {
    let (repo, base) = range_repo();
    let (release, gate) = std::sync::mpsc::channel();
    let source = Arc::new(GatedSource {
        inner: Box::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
        gated: GroupBy::Commit,
        gate: std::sync::Mutex::new(gate),
    });
    let server = LiveServer::start(source).await;
    let mut events = http()
        .get(format!("{}api/events", server.url()))
        .send()
        .await
        .unwrap();
    let first = server.get_json("api/review").await;
    assert_eq!(first["units"][1]["state"], "building");

    release.send(()).unwrap();
    let notified = next_sse_event(&mut events, "unit").await;

    assert_eq!(notified["units"][1]["unit"], "commit");
    assert_eq!(notified["units"][1]["state"], "ready");
    assert_eq!(
        notified["units"],
        server.get_json("api/review").await["units"]
    );
    server.stop();
}

#[tokio::test]
async fn unit_failure_keeps_review_and_retries() {
    let (repo, base) = range_repo();
    let fail = Arc::new(AtomicBool::new(true));
    let server = LiveServer::start(Arc::new(UnitFailureSource {
        inner: Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
        fail: fail.clone(),
    }))
    .await;

    let review = server.get_json("api/review").await;
    let failed = server.wait_unit("failed").await;

    let reason = failed["units"][1]["error"].as_str().unwrap();
    assert!(!reason.is_empty());
    let file_id = review["groups"][0]["files"][0]["id"].as_str().unwrap();
    let file = http()
        .get(format!("{}api/file/{file_id}", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(file.status(), 200, "the review must go on");

    fail.store(false, Ordering::SeqCst);
    let retry = server
        .post("api/unit", json!({"op": "retry", "unit": "commit"}))
        .await;
    assert_eq!(retry.status(), 200);
    server.wait_unit("ready").await;
    server.stop();
}

#[tokio::test]
async fn unit_comments_both_in_submit_with_their_group_ids() {
    let (repo, base) = range_repo();
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File))).await;
    let final_form = server.get_json("api/review").await;
    server.wait_unit("ready").await;
    let per_commit = server.get_json("api/review?unit=commit").await;
    let first_sha = per_commit["groups"][0]["id"].as_str().unwrap().to_string();

    for (file, body) in [
        (file_in(&final_form, 0, "a.txt"), "最終形へ"),
        (file_in(&per_commit, 0, "a.txt"), "コミットへ"),
    ] {
        let response = server
            .post(
                "api/comment",
                json!({
                    "op": "add", "file_id": file["id"], "side": "new",
                    "start_line": 1, "end_line": 1, "body": body
                }),
            )
            .await;
        assert_eq!(response.status(), 200);
    }
    // 単位を行き来して表示しても、前の単位のコメントは古くならない。
    let _ = server.get_json("api/review?unit=file").await;
    let response = server
        .post("api/submit", json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;

    let comments = document["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["group_id"], "all");
    assert_eq!(comments[0]["body"], "最終形へ");
    assert_eq!(comments[1]["group_id"], first_sha.as_str());
    assert_eq!(comments[1]["group_title"], "feat: two");
    assert!(comments.iter().all(|comment| comment["outdated"] == false));
}

#[tokio::test]
async fn unit_seen_separate_per_unit() {
    let (repo, base) = range_repo();
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File))).await;
    let final_form = server.get_json("api/review").await;
    server.wait_unit("ready").await;
    let final_a = file_in(&final_form, 0, "a.txt")["id"].clone();

    let response = server
        .post("api/state", json!({"file_id": final_a, "seen": true}))
        .await;
    assert_eq!(response.status(), 200);

    let final_form = server.get_json("api/review?unit=file").await;
    let per_commit = server.get_json("api/review?unit=commit").await;
    assert_eq!(file_in(&final_form, 0, "a.txt")["seen"], true);
    assert!(
        files_of(&per_commit)
            .iter()
            .filter(|file| file["path"] == "a.txt")
            .all(|file| file["seen"] == false)
    );
    server.stop();
}

#[tokio::test]
async fn unit_focus_applies_to_both_units() {
    let (repo, base) = range_repo();
    repo.write(
        "focus.json",
        r#"{"files":[{"path":"a.txt","note":"ここ"}]}"#,
    );
    let source = FocusSource::from_path(
        Box::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
        std::path::Path::new("focus.json"),
        &repo.path,
    )
    .unwrap();
    let server = LiveServer::start(Arc::new(source)).await;

    let final_form = server.get_json("api/review").await;
    server.wait_unit("ready").await;
    let per_commit = server.get_json("api/review?unit=commit").await;

    assert_eq!(file_in(&final_form, 0, "a.txt")["focus"], true);
    let marked: Vec<Value> = files_of(&per_commit)
        .into_iter()
        .filter(|file| file["path"] == "a.txt")
        .collect();
    assert_eq!(marked.len(), 2);
    assert!(
        marked
            .iter()
            .all(|file| file["focus"] == true && file["note"] == "ここ")
    );
    server.stop();
}

#[tokio::test]
async fn origin_api_returns_blocks_for_final_files_only() {
    let (repo, base) = range_repo();
    let last = repo.git(&["rev-parse", "HEAD"]);
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File))).await;
    let final_form = server.get_json("api/review").await;
    server.wait_unit("ready").await;
    let per_commit = server.get_json("api/review?unit=commit").await;

    let final_a = file_in(&final_form, 0, "a.txt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let origin = server.get_json(&format!("api/origin/{final_a}")).await;
    assert_eq!(origin["available"], true);
    assert_eq!(origin["enabled"], true);
    let block = &origin["blocks"][0];
    assert_eq!(block["entries"][0]["sha"], last.as_str());
    assert_eq!(block["entries"][0]["target"]["side"], "new");
    assert_eq!(block["unknown"], "none");
    assert_eq!(origin["commits"][last.as_str()]["subject"], "fix: three");

    let commit_a = file_in(&per_commit, 0, "a.txt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let none = server.get_json(&format!("api/origin/{commit_a}")).await;
    assert_eq!(none["available"], false);
    server.stop();
}

#[tokio::test]
async fn origin_api_large_file_opt_in() {
    let repo = TempRepo::new();
    let lines: String = (1..=10_001).map(|n| format!("line {n}\n")).collect();
    repo.write("big.txt", &lines);
    let base = repo.commit("base");
    repo.write("big.txt", &lines.replacen("line 5000\n", "changed\n", 1));
    repo.commit("change");
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File))).await;
    let review = server.get_json("api/review").await;
    let id = review["groups"][0]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let default = server.get_json(&format!("api/origin/{id}")).await;
    assert_eq!(default["available"], true);
    assert_eq!(default["enabled"], false);
    assert_eq!(default["blocks"], json!([]));

    let forced = server.get_json(&format!("api/origin/{id}?force=1")).await;
    assert_eq!(forced["enabled"], true);
    assert_eq!(forced["blocks"].as_array().unwrap().len(), 1);
    server.stop();
}

#[tokio::test]
async fn origin_git_failure_keeps_review_and_comments_submittable() {
    let (repo, base) = range_repo();
    let middle = repo.git(&["rev-parse", "HEAD~1"]);
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File))).await;
    let final_form = server.get_json("api/review").await;
    server.wait_unit("ready").await;
    let final_a = file_in(&final_form, 0, "a.txt")["id"].clone();
    // 範囲の途中のコミットが読めないと、由来の git log と blame が失敗する。
    // 表示する内容（両端の版）はまだ読める。
    std::fs::remove_file(
        repo.path
            .join(".git/objects")
            .join(&middle[..2])
            .join(&middle[2..]),
    )
    .unwrap();
    let file = server
        .get_json(&format!("api/file/{}", final_a.as_str().unwrap()))
        .await;
    assert_eq!(file["binary"], false);

    let origin = server
        .get_json(&format!("api/origin/{}", final_a.as_str().unwrap()))
        .await;
    assert_eq!(
        origin["commits"],
        json!({}),
        "no commit may be named: {origin}"
    );
    assert!(
        server
            .notices
            .any(|notice| matches!(notice, Notice::OriginUnknown { path, .. } if path == "a.txt"))
    );
    let response = server
        .post(
            "api/comment",
            json!({
                "op": "add", "file_id": final_a, "side": "new",
                "start_line": 1, "end_line": 1, "body": "残る"
            }),
        )
        .await;
    assert_eq!(response.status(), 200);
    let response = server
        .post("api/submit", json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;
    assert_eq!(document["comments"][0]["body"], "残る");
}

#[tokio::test]
async fn live_refresh_both_units_adds_new_commit_unseen() {
    let (repo, base) = range_repo();
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File))).await;
    server.get_json("api/review").await;
    server.wait_unit("ready").await;
    let per_commit = server.get_json("api/review?unit=commit").await;
    let first_a = file_in(&per_commit, 0, "a.txt")["id"].clone();
    server
        .post("api/state", json!({"file_id": first_a, "seen": true}))
        .await;
    server
        .post(
            "api/comment",
            json!({"op": "add", "file_id": first_a, "side": "new",
                   "start_line": 1, "end_line": 1, "body": "残る"}),
        )
        .await;

    repo.write("c.txt", "c\n");
    let new_sha = repo.commit("feat: c");
    let refreshed = server.get_json("api/review?refresh=1&unit=commit").await;

    let groups = refreshed["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[2]["id"], new_sha.as_str());
    assert!(
        groups[2]["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["seen"] == false)
    );
    assert_eq!(file_in(&refreshed, 0, "a.txt")["id"], first_a);
    assert_eq!(file_in(&refreshed, 0, "a.txt")["seen"], true);
    assert_eq!(refreshed["comments"][0]["body"], "残る");
    let final_form = server.get_json("api/review?unit=file").await;
    assert!(
        files_of(&final_form)
            .iter()
            .any(|file| file["path"] == "c.txt")
    );
    server.stop();
}

#[tokio::test]
async fn range_startup_reads_no_content_including_background_build() {
    let (repo, base) = range_repo();
    let reads = Arc::new(AtomicUsize::new(0));
    let source = CountingSource {
        inner: Box::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
        reads: reads.clone(),
    };
    let server = LiveServer::start(Arc::new(source)).await;

    server.get_json("api/review").await;
    server.wait_unit("ready").await;
    server.get_json("api/review?unit=commit").await;

    assert_eq!(reads.load(Ordering::SeqCst), 0);
    server.stop();
}

#[tokio::test]
async fn refresh_save_failure_keeps_published_generation_metadata_and_content_references() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    repo.write("a.txt", "two\n");
    let sink = Arc::new(RecordingSink::default());
    sink.copy_already_saved.store(true, Ordering::SeqCst);
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server =
        AgentServer::start_with_sink(source.clone(), Ipv4Addr::LOCALHOST, sink.clone()).await;
    let first: Value = server.page.get("api/review").await.json().await.unwrap();
    repo.write("b.txt", "new\n");
    sink.fail.store(true, Ordering::SeqCst);
    let response = server.page.get("api/review?refresh=1").await;
    assert_eq!(response.status(), 500);
    let unchanged: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(unchanged["groups"], first["groups"]);
    assert_eq!(
        unchanged["diff_display_version"],
        first["diff_display_version"]
    );
    assert_eq!(
        sink.accepted
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .display
            .as_ref()
            .unwrap()
            .current,
        first["diff_display_version"].as_str().unwrap()
    );
    let candidate = source.fork().unwrap().unwrap().review().unwrap();
    let b = candidate.groups[0]
        .files
        .iter()
        .find(|f| f.path == "b.txt")
        .unwrap();
    assert_eq!(
        server
            .page
            .get(&format!("api/file/{}", b.id))
            .await
            .status(),
        404
    );
    sink.fail.store(false, Ordering::SeqCst);
    let accepted: Value = server
        .page
        .get("api/review?refresh=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(accepted["diff_display_version"], "01TESTREVIEW:2");
    assert_eq!(accepted["groups"][0]["files"].as_array().unwrap().len(), 2);
    server.page.task.abort();
}

#[tokio::test]
async fn manifest_refresh_save_failure_keeps_published_token_sides_lines_and_statistics() {
    let dir = TempRepo::new();
    dir.write("new.txt", "one\n");
    let manifest =
        json!({"groups":[{"id":"g","title":"G","diffs":[{"path":"a.txt","new_path":"new.txt"}]}]});
    let source = Arc::new(ManifestSource::from_json(&manifest.to_string(), &dir.path).unwrap());
    let sink = Arc::new(RecordingSink::default());
    sink.copy_already_saved.store(true, Ordering::SeqCst);
    let server = AgentServer::start_with_sink(source, Ipv4Addr::LOCALHOST, sink.clone()).await;
    let first: Value = server.page.get("api/review").await.json().await.unwrap();
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(context["references"]["code"][0]["new_lines"], 1);
    assert_eq!(context["references"]["code"][0]["new_present"], true);
    assert_eq!(context["references"]["code"][0]["old_present"], false);
    dir.write("new.txt", "one\ntwo\nthree\n");
    sink.fail.store(true, Ordering::SeqCst);
    assert_eq!(server.page.get("api/review?refresh=1").await.status(), 500);
    let unchanged: Value = server.page.get("api/review").await.json().await.unwrap();
    let unchanged_context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(unchanged_context, context);
    assert_eq!(unchanged["groups"], first["groups"]);
    assert_eq!(
        unchanged["diff_display_version"],
        first["diff_display_version"]
    );
    sink.fail.store(false, Ordering::SeqCst);
    let accepted = server.page.get("api/review?refresh=1").await;
    assert_eq!(accepted.status(), 200);
    let accepted: Value = accepted.json().await.unwrap();
    let updated: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_ne!(
        updated["diff_display_version"],
        context["diff_display_version"]
    );
    assert_eq!(
        updated["diff_display_version"],
        accepted["diff_display_version"]
    );
    assert_eq!(updated["references"]["code"][0]["new_lines"], 3);
    assert_eq!(accepted["groups"][0]["files"][0]["add"], 3);
    server.page.task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_general_save_cannot_overwrite_a_later_published_display_generation() {
    let sink = Arc::new(SlowSink::new());
    let _release_on_drop = FirstSaveGuard(sink.clone());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
    let initial: Value = server.get("api/review").await.json().await.unwrap();
    let message_url = format!("{}api/message", server.url);
    let origin = server.origin.clone();
    let message = tokio::spawn(async move {
        http()
            .post(message_url)
            .header("Origin", origin)
            .json(&json!({"body":"new"}))
            .send()
            .await
            .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(3), sink.entered.notified())
        .await
        .unwrap();
    let refresh_url = format!("{}api/review?refresh=1", server.url);
    let refreshed = tokio::spawn(async move {
        http()
            .get(refresh_url)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    });
    let before: Value = server.get("api/review").await.json().await.unwrap();
    assert_eq!(
        before["diff_display_version"],
        initial["diff_display_version"]
    );
    sink.open_first_save();
    assert_eq!(message.await.unwrap().status(), 200);
    let current = tokio::time::timeout(Duration::from_secs(3), refreshed)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        current["diff_display_version"],
        initial["diff_display_version"]
    );
    let saved = sink.states.lock().unwrap().clone();
    let generations: Vec<_> = saved
        .iter()
        .filter_map(|s| s.display.as_ref())
        .map(|d| {
            d.current
                .rsplit_once(':')
                .unwrap()
                .1
                .parse::<u128>()
                .unwrap()
        })
        .collect();
    assert!(
        generations.windows(2).all(|pair| pair[0] <= pair[1]),
        "{generations:?}"
    );
    assert_eq!(
        saved.last().unwrap().display.as_ref().unwrap().current,
        current["diff_display_version"].as_str().unwrap()
    );
    server.task.abort();
}
