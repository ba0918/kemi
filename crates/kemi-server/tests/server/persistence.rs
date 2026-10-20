use super::*;

#[tokio::test]
async fn session_state_is_saved_on_every_change() {
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;

    server
        .comment(json!({"op": "add", "file_id": "f1", "side": "new", "start_line": 1, "end_line": 1, "body": "x"}))
        .await;
    server
        .post(
            "api/state",
            json!({"file_id": "f1", "seen": true, "collapsed": true}),
        )
        .await;
    server
        .comment(json!({"op": "reply", "id": "c1", "body": "reply"}))
        .await;
    server
        .comment(json!({"op": "resolve", "id": "c1", "resolved": true}))
        .await;

    let state = sink.last_state();
    assert_eq!(state.comments.len(), 1);
    assert_eq!(
        state.comments[0].replies,
        vec![kemi_core::domain::review::Reply {
            id: "r1".to_string(),
            seq: 2,
            author: kemi_core::domain::review::Author::Reviewer,
            body: "reply".into(),
        }]
    );
    assert!(state.comments[0].resolved);
    assert!(state.seen.contains("f1"));
    assert_eq!(state.collapsed.get("f1"), Some(&true));
    assert_eq!(*sink.title.lock().unwrap(), "テストのレビュー");
    assert_eq!(*sink.total_files.lock().unwrap(), 12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_saves_keep_changes_made_while_a_save_is_slow() {
    let sink = Arc::new(SlowSink::new());
    let _release_on_drop = FirstSaveGuard(sink.clone());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;

    let url = server.url.clone();
    let origin = server.origin.clone();
    let mut first = tokio::spawn({
        let (url, origin) = (url.clone(), origin.clone());
        async move {
            post_json(
                &url,
                &origin,
                "api/comment",
                json!({
                    "op": "add", "file_id": "f1", "side": "new",
                    "start_line": 1, "end_line": 1, "body": "first"
                }),
            )
            .await
        }
    });
    // HTTP タスクが保存に届く前に失敗しても、通知を永久に待たないため。
    tokio::select! {
        () = sink.entered.notified() => {}
        result = &mut first => panic!("最初の要求が保存に入る前に終了した: {result:?}"),
        () = tokio::time::sleep(Duration::from_secs(10)) => {
            panic!("最初の要求が保存に入らなかった");
        }
    }
    let second = tokio::spawn(async move {
        post_json(
            &url,
            &origin,
            "api/comment",
            json!({
                "op": "add", "file_id": "f1", "side": "new",
                "start_line": 1, "end_line": 1, "body": "second"
            }),
        )
        .await
    });

    // 保存がワーカーを同期的にふさいでも、tokio のタイマーに頼らず解放するため。
    // 直列化されていれば、最初の保存を解放するまで次の保存は始まらない。
    let release = std::thread::spawn({
        let sink = sink.clone();
        move || {
            let opened = sink.gate.lock().unwrap();
            let (mut opened, _) = sink
                .gate_opened
                .wait_timeout_while(opened, Duration::from_millis(300), |opened| !*opened)
                .unwrap();
            let calls_before_release = sink.calls.load(Ordering::SeqCst);
            *opened = true;
            sink.gate_opened.notify_all();
            calls_before_release
        }
    });
    let second_response = second.await.unwrap();
    assert_eq!(release.join().unwrap(), 1);

    assert_eq!(first.await.unwrap().status(), 200);
    assert_eq!(second_response.status(), 200);
    assert_eq!(sink.last_saved_bodies(), vec!["first", "second"]);
}

#[tokio::test]
async fn session_copy_is_saved_after_the_first_review() {
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;

    server.get("api/review").await;

    let copy = server.wait_for_copy(&sink).await;
    assert_eq!(copy.startup_unit, None);
    assert_eq!(copy.units.len(), 1);
    assert_eq!(copy.units[0].review.groups[0].id, "g1");
    assert!(copy.contents.contains_key("f1"));
    assert!(copy.contents.contains_key("f2"));
    assert!(copy.contents.contains_key("f3"));
}

#[tokio::test]
async fn session_copy_waits_for_both_units() {
    let base = FakeSource::new();
    let mut second = base.meta.clone();
    second.groups[0].id = "deadbeef".to_string();
    let mut contents = base.contents.clone();
    contents.insert(
        "f4".to_string(),
        FileContent {
            old: None,
            new: Some(b"second\n".to_vec()),
        },
    );
    second.groups[0]
        .files
        .push(file_entry("f4", "src/second.rs"));
    let source = Arc::new(TwoUnitSource {
        first: base.meta.clone(),
        second,
        contents,
    });
    let sink = Arc::new(RecordingSink::default());
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let url = session_url(&listener, "test-token", Ipv4Addr::LOCALHOST).unwrap();
    let params = ServeParams {
        source,
        assets: Arc::new(FakeAssets),
        token: "test-token".to_string(),
        results: None,
        session: Some(sink.clone()),
        notices: Arc::new(RecordingNotices::default()),
        share_address: None,
        agent: None,
        live: None,
        initial_explanation: None,
    };
    tokio::spawn(serve(listener, params));

    reqwest::get(format!("{url}api/review")).await.unwrap();

    let copy = {
        let mut found = None;
        for _ in 0..200 {
            if let Some(copy) = sink.copies.lock().unwrap().first().cloned() {
                found = Some(copy);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        found.expect("the session copy was never saved")
    };
    assert_eq!(copy.startup_unit, Some(GroupBy::File));
    assert_eq!(copy.units.len(), 2);
    assert_eq!(copy.units[1].unit, Some(GroupBy::Commit));
    assert!(copy.contents.contains_key("f4"));
}

#[tokio::test]
async fn session_with_a_saved_copy_is_not_frozen_again() {
    let sink = Arc::new(RecordingSink::default());
    sink.copy_already_saved.store(true, Ordering::SeqCst);
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;

    server.get("api/review").await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(server.source.reads.load(Ordering::SeqCst), 0);
    assert!(sink.copies.lock().unwrap().is_empty());
    assert!(sink.unusable.lock().unwrap().is_empty());
}

#[tokio::test]
async fn session_copy_failure_marks_the_session_unresumable() {
    let mut source = FakeSource::new();
    source.contents.clear();
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(source), sink.clone()).await;

    server.get("api/review").await;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if !sink.unusable.lock().unwrap().is_empty() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "never marked unusable"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert!(sink.copies.lock().unwrap().is_empty());
    assert!(!sink.unusable.lock().unwrap()[0].is_empty());
}

#[tokio::test]
async fn session_review_response_does_not_wait_for_the_freeze() {
    let source = FakeSource::new();
    source.set_content_delay(Duration::from_millis(600));
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(source), sink.clone()).await;

    let started = std::time::Instant::now();
    let response = server.get("api/review").await;
    let elapsed = started.elapsed();

    assert_eq!(response.status(), 200);
    assert!(
        elapsed < Duration::from_millis(400),
        "review waited {elapsed:?}"
    );
}

#[tokio::test]
async fn session_is_deleted_on_submit() {
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
    server.get("api/review").await;

    let response = server
        .post("api/submit", json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    server.finish().await;

    assert!(sink.deleted.load(Ordering::SeqCst));
}

#[tokio::test]
async fn session_sink_failure_does_not_stop_the_review() {
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
    assert_eq!(server.get("api/review").await.status(), 200);
    sink.fail.store(true, Ordering::SeqCst);

    let added = server
        .comment(json!({"op": "add", "file_id": "f1", "side": "new", "start_line": 1, "end_line": 1, "body": "x"}))
        .await;
    assert_eq!(added.status(), 200);
    let review = server.get("api/review").await;
    assert_eq!(review.status(), 200);

    assert!(
        server
            .notices
            .any(|notice| matches!(notice, Notice::SessionNotSaved(_))),
        "a failed save must be reported to the caller"
    );

    let response = server
        .post("api/submit", json!({"verdict": "changes_requested"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;
    assert_eq!(document["comments"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn result_save_failure_is_reported_and_shown_without_changing_the_submit() {
    let server = TestServer::start_with_sinks(
        Arc::new(FakeSource::new()),
        None,
        Some(Arc::new(FailingResults)),
    )
    .await;

    let response = server
        .post("api/submit", json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["saved"]["error"],
        "cannot determine where to store results (no home directory)"
    );
    assert!(
        server
            .notices
            .any(|notice| matches!(notice, Notice::ResultNotSaved(_)))
    );
    let document = server.finish().await;
    assert_eq!(document["verdict"], "approved");
}

#[tokio::test]
async fn session_copy_over_the_limit_is_unresumable() {
    let temp = TempRepo::new();
    let dir = temp.path.join("sessions");
    // 上限を小さくしたストアで、20 MB を超える内容を作らずに同じ道（凍結 → 上限超過 →
    // 復元不可）を通す。
    let store = SessionStore::new(dir.clone()).with_copy_limit(64 * 1024);
    let info = SessionInfo {
        id: "01HF7YAT00SERVER0000000000".to_string(),
        created: 1,
        updated: 1,
        workspace: PathBuf::from("/tmp/workspace"),
        workspace_key: "00000000000000aa".to_string(),
        mode: SessionMode::Worktree,
        title: "big".to_string(),
        total_files: 1,
    };
    let sink = Arc::new(kemi_server::StoredSession::start(&store, info).unwrap());

    let mut source = FakeSource::new();
    source.meta.groups[0].files = vec![file_entry("f1", "src/a.rs"), file_entry("big", "big.bin")];
    source.contents.insert(
        "big".to_string(),
        FileContent {
            old: None,
            new: Some(incompressible(128 * 1024)),
        },
    );
    let server = TestServer::start_with_session(Arc::new(source), sink).await;
    // 状態を持つセッションだけが上限超過の印を残せる。状態も写しも無いセッションは
    // 残さない（R-SESSION）。
    server.add_new_side_comment().await;
    server.get("api/review").await;

    let id = "01HF7YAT00SERVER0000000000";
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(stored) = store.read(id)
            && matches!(stored.copy, kemi_core::session::CopyState::Unusable(_))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "never became unusable"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(store.list().unwrap().is_empty());
    // 上限の判定は `<id>.payload` 全体で行い、超える写しはディスクに書かない
    // （R-SESSION）。
    assert!(!dir.join(format!("{id}.payload")).exists());
}

#[tokio::test]
async fn session_initial_state_is_restored() {
    use kemi_core::domain::review::Side;
    let comment = kemi_core::domain::review::Comment {
        id: "c7".to_string(),
        seq: 1,
        group_id: "g1".to_string(),
        group_title: "最初の変更".to_string(),
        body: "復元されたコメント".to_string(),
        replies: vec![kemi_core::domain::review::Reply {
            id: "r1".to_string(),
            seq: 2,
            author: kemi_core::domain::review::Author::Reviewer,
            body: "返信".into(),
        }],
        resolved: true,
        outdated: false,
        target: kemi_core::domain::review::CommentTarget::File(
            kemi_core::domain::review::FileTarget {
                file_id: "f1".to_string(),
                path: "src/a.rs".to_string(),
                side: Side::New,
                start_line: Some(1),
                end_line: Some(1),
                quote: vec!["x".to_string()],
                content_hash: "hash".to_string(),
                suggestion: None,
            },
        ),
    };
    let sink = Arc::new(RecordingSink::default());
    *sink.initial.lock().unwrap() = Some(SessionState {
        comments: vec![comment],
        seen: ["f1".to_string()].into_iter().collect(),
        collapsed: [("f1".to_string(), true)].into_iter().collect(),
        last_comment: 7,
        last_reply: 1,
        ..SessionState::default()
    });
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink).await;

    let review: Value = server.get("api/review").await.json().await.unwrap();
    let file = &review["groups"][0]["files"][0];

    assert_eq!(review["comments"][0]["body"], "復元されたコメント");
    assert_eq!(review["comments"][0]["resolved"], true);
    assert_eq!(file["seen"], true);
    assert_eq!(file["collapsed"], true);
}

#[tokio::test]
async fn session_restored_replies_continue_their_numbering() {
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
    server.add_new_side_comment().await;
    server
        .comment(json!({"op": "reply", "id": "c1", "body": "first"}))
        .await;
    let mut saved = sink.last_state();
    // 返信を消しても、復元した後の返信は消した番号を使わない。
    saved.comments[0].replies.clear();
    let restored = Arc::new(RecordingSink::default());
    *restored.initial.lock().unwrap() = Some(saved);
    let server =
        TestServer::start_with_session(Arc::new(FakeSource::new()), restored.clone()).await;

    server
        .comment(json!({"op": "reply", "id": "c1", "body": "second"}))
        .await;

    assert_eq!(restored.last_state().comments[0].replies[0].id, "r2");
}

#[tokio::test]
async fn suspend_after_submit_returns_the_committed_document() {
    for verdict in ["approved", "changes_requested"] {
        let (release, receiver) = std::sync::mpsc::channel();
        let gate = Arc::new(GenerationGate {
            entered: tokio::sync::Notify::new(),
            release: Mutex::new(receiver),
        });
        let source = Arc::new(SuspendGatedSource {
            inner: FakeSource::new(),
            gate: gate.clone(),
        });
        let sink = Arc::new(RecordingSink::default());
        sink.copy_already_saved.store(true, Ordering::SeqCst);
        let server = AgentServer::start_with_sink(source, Ipv4Addr::LOCALHOST, sink).await;
        server.control.ready().await;
        // 別のファイル取得が処理中なら、submit の応答後も serve は接続の終了を待つ。
        let url = format!("{}api/file/f1", server.page.url);
        let reading = tokio::spawn(async move { reqwest::get(url).await.unwrap() });
        tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
            .await
            .expect("file request did not reach the source");

        let response = server
            .page
            .post("api/submit", json!({"verdict": verdict}))
            .await;
        assert_eq!(response.status(), 200);
        let submitted: Value = response.json().await.unwrap();
        assert_eq!(submitted["result"]["verdict"], verdict);
        assert!(server.sink.deleted.load(Ordering::SeqCst));
        assert!(!server.page.task.is_finished());

        let outcome = server.control.suspend();
        release.send(()).unwrap();
        let kemi_server::SuspendOutcome::Submitted { document, .. } = outcome else {
            panic!("suspend discarded the committed submit");
        };
        assert_eq!(document, submitted["result"]);
        assert!(server.sink.deleted.load(Ordering::SeqCst));

        assert!(
            tokio::time::timeout(Duration::from_secs(5), reading)
                .await
                .unwrap()
                .unwrap()
                .status()
                .is_success()
        );
        assert_eq!(server.page.finish().await, document);
    }
}

#[tokio::test]
async fn suspend_before_submit_returns_suspended_without_a_waiter() {
    let server = AgentServer::start().await;
    server.control.ready().await;

    assert!(matches!(
        server.control.suspend(),
        kemi_server::SuspendOutcome::Suspended { waiting: false }
    ));
    assert!(!server.sink.deleted.load(Ordering::SeqCst));
    let error = tokio::time::timeout(Duration::from_secs(5), server.page.task)
        .await
        .expect("server did not stop")
        .expect("server task panicked")
        .expect_err("suspend must not produce a submit");
    assert_eq!(error.to_string(), "the review was suspended");
}
