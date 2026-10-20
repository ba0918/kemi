use super::*;

#[tokio::test]
async fn two_clients_share_comments_and_seen_without_an_author_field() {
    let server = TestServer::start().await;
    let first = http();
    let second = http();

    let response = first
        .post(format!("{}api/comment", server.url))
        .header("Origin", &server.origin)
        .json(&json!({
            "op": "add", "file_id": "f1", "side": "new",
            "start_line": 11, "end_line": 11, "body": "1台目から"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = first
        .post(format!("{}api/state", server.url))
        .header("Origin", &server.origin)
        .json(&json!({"file_id": "f1", "seen": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let review: Value = second
        .get(format!("{}api/review", server.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(review["groups"][0]["files"][0]["seen"], true);
    assert_eq!(review["comments"][0]["body"], "1台目から");

    let response = second
        .post(format!("{}api/comment", server.url))
        .header("Origin", &server.origin)
        .json(&json!({"op": "reply", "id": "c1", "body": "2台目から"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = second
        .post(format!("{}api/submit", server.url))
        .header("Origin", &server.origin)
        .json(&json!({"verdict": "approved"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let document = server.finish().await;
    let comment = &document["comments"][0];
    assert_eq!(comment["replies"][0]["body"], "2台目から");
    // 返信の書いた人は「人間」だけで、どの端末かは区別しない（P5）。
    assert_eq!(comment["replies"][0]["author"], "reviewer");
    assert!(comment.get("author").is_none(), "{comment}");
}

#[tokio::test]
async fn comment_api_validates_line_range() {
    let server = TestServer::start().await;
    let response = server
        .post(
            "api/comment",
            json!({
                "op": "add",
                "file_id": "f1",
                "side": "new",
                "start_line": 100,
                "end_line": 101,
                "body": "範囲外"
            }),
        )
        .await;

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn page_comments_are_refused_outside_a_review_of_a_running_page() {
    let server = TestServer::start().await;
    let response = server
        .post(
            "api/comment",
            json!({
                "op": "add_page",
                "page": { "url": "/", "width": 390, "places": [{ "n": 1, "kind": "element", "points": [], "elements": [] }] },
                "body": "page"
            }),
        )
        .await;

    assert_eq!(response.status(), 400);
    let review: Value = server.get("api/review").await.json().await.unwrap();
    assert_eq!(review["comments"], json!([]));
}

#[tokio::test]
async fn comment_edit_changes_only_body_and_suggestion() {
    let server = TestServer::start().await;
    let created = server.add_new_side_comment().await;

    let response = server
        .comment(json!({"op": "edit", "id": "c1", "body": "直した本文", "suggestion": "NEWER\n"}))
        .await;
    assert_eq!(response.status(), 200);
    let comments = server.submit_comments().await;

    let edited = &comments[0];
    assert_eq!(edited["body"], "直した本文");
    assert_eq!(edited["suggestion"]["replacement"], "NEWER\n");
    for field in [
        "id",
        "side",
        "start_line",
        "end_line",
        "quote",
        "group_id",
        "path",
    ] {
        assert_eq!(edited[field], created[field], "{field} must not change");
    }
    assert_eq!(edited["outdated"], false);
}

#[tokio::test]
async fn comment_edit_can_drop_the_suggestion() {
    let server = TestServer::start().await;
    server.add_new_side_comment().await;

    let edited: Value = server
        .comment(json!({"op": "edit", "id": "c1", "body": "提案なし", "suggestion": null}))
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(edited["suggestion"], Value::Null);
}

#[tokio::test]
async fn comment_edit_rejects_suggestion_on_old_side_and_file_wide() {
    let server = TestServer::start().await;
    server
        .comment(json!({
            "op": "add", "file_id": "f1", "side": "old",
            "start_line": 11, "end_line": 11, "body": "旧側"
        }))
        .await;
    server
        .comment(json!({"op": "add", "file_id": "f1", "side": "new", "body": "全体"}))
        .await;

    for id in ["c1", "c2"] {
        let response = server
            .comment(json!({"op": "edit", "id": id, "body": "x", "suggestion": "y"}))
            .await;
        assert_eq!(response.status(), 400, "{id}");
    }
    let comments = server.submit_comments().await;
    assert!(
        comments
            .iter()
            .all(|comment| comment["suggestion"] == Value::Null && comment["body"] != "x")
    );
}

#[tokio::test]
async fn comment_delete_removes_it_from_submit() {
    let server = TestServer::start().await;
    server.add_new_side_comment().await;
    server.add_new_side_comment().await;

    let response = server.comment(json!({"op": "delete", "id": "c1"})).await;
    assert_eq!(response.status(), 200);
    let comments = server.submit_comments().await;

    let ids: Vec<&str> = comments
        .iter()
        .map(|comment| comment["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["c2"]);
}

#[tokio::test]
async fn comment_delete_never_reuses_the_id() {
    let server = TestServer::start().await;
    server.add_new_side_comment().await;
    server.add_new_side_comment().await;
    server.comment(json!({"op": "delete", "id": "c2"})).await;

    let added = server.add_new_side_comment().await;

    assert_eq!(added["id"], "c3");
}

#[tokio::test]
async fn adding_a_comment_reports_its_path_side_and_lines() {
    let server = TestServer::start().await;

    let line = server
        .comment(json!({"op": "add", "file_id": "f1", "side": "old", "start_line": 11, "end_line": 11, "body": "x"}))
        .await;
    assert_eq!(line.status(), 200);
    let whole = server
        .comment(json!({"op": "add", "file_id": "f1", "side": "new", "body": "y"}))
        .await;
    assert_eq!(whole.status(), 200);

    let notices = server.notices.notices.lock().unwrap();
    let added: Vec<_> = notices
        .iter()
        .filter_map(|notice| {
            if let Notice::CommentAdded { path, side, lines } = notice {
                Some((path.clone(), *side, *lines))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        added,
        vec![
            ("src/a.rs".to_string(), Side::Old, Some((11, 11))),
            ("src/a.rs".to_string(), Side::New, None),
        ]
    );
}

#[tokio::test]
async fn hand_puts_two_written_comments_into_one_handed() {
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;
    server.add_comment_with_body(12, "second").await;

    server.hand().await;

    let handed = saved_handed(&sink);
    assert_eq!(handed.len(), 1);
    assert_eq!(
        changes(&handed[0]),
        vec![
            ("added", "c1".to_string(), Some("first".to_string())),
            ("added", "c2".to_string(), Some("second".to_string())),
        ]
    );
}

#[tokio::test]
async fn writing_without_handing_stores_nothing_for_the_agent() {
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;
    server
        .post("api/message", json!({"body": "look at the tests"}))
        .await;

    assert!(sink.last_state().channel.events.is_empty());
}

#[tokio::test]
async fn a_comment_edited_after_handing_is_handed_again_as_edited() {
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first wording").await;
    server.hand().await;
    server
        .comment(json!({"op": "edit", "id": "c1", "body": "second wording"}))
        .await;

    server.hand().await;

    let handed = saved_handed(&sink);
    assert_eq!(
        changes(&handed[1]),
        vec![(
            "edited",
            "c1".to_string(),
            Some("second wording".to_string())
        )]
    );
}

#[tokio::test]
async fn a_deleted_comment_is_handed_by_its_id_only() {
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;
    server.hand().await;
    server.comment(json!({"op": "delete", "id": "c1"})).await;

    server.hand().await;

    assert_eq!(
        changes(&saved_handed(&sink)[1]),
        vec![("deleted", "c1".to_string(), None)]
    );
}

#[tokio::test]
async fn a_reviewer_reply_and_message_are_handed_with_their_author() {
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;
    server
        .comment(json!({"op": "reply", "id": "c1", "body": "and the caller"}))
        .await;
    let message: Value = server
        .post("api/message", json!({"body": "start with the tests"}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(message["author"], "reviewer");

    server.hand().await;

    let handed = &saved_handed(&sink)[0];
    assert_eq!(handed.replies[0].comment_id, "c1");
    assert_eq!(handed.replies[0].reply.body, "and the caller".into());
    assert_eq!(handed.messages[0].body, "start with the tests".into());
}

#[tokio::test]
async fn resolving_changes_only_through_the_page_and_is_kept() {
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;

    let resolved: Value = server
        .comment(json!({"op": "resolve", "id": "c1", "resolved": true}))
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(resolved["resolved"], true);
    assert!(sink.last_state().comments[0].resolved);
}

#[tokio::test]
async fn writing_a_message_is_notified_over_sse() {
    let (server, _sink) = TestServer::with_recording_session().await;
    let mut events = server.get("api/events").await;

    server
        .post("api/message", json!({"body": "start with the tests"}))
        .await;

    let notified = next_sse_event(&mut events, "message").await;
    assert_eq!(notified["body"], "start with the tests");
    assert_eq!(notified["author"], "reviewer");
}

#[tokio::test]
async fn writing_a_reply_is_notified_over_sse_with_its_thread() {
    let (server, _sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;
    let mut events = server.get("api/events").await;

    server
        .comment(json!({"op": "reply", "id": "c1", "body": "and the caller"}))
        .await;

    let notified = next_sse_event(&mut events, "thread").await;
    assert_eq!(notified["id"], "c1");
    assert_eq!(notified["replies"][0]["body"], "and the caller");
}

#[tokio::test]
async fn the_first_load_carries_replies_messages_and_the_agent_state() {
    use kemi_core::domain::review::{Author, Message, Reply};
    let (server, sink) = TestServer::with_recording_session().await;
    server.add_comment_with_body(11, "first").await;
    let mut saved = sink.last_state();
    saved.comments[0].replies.push(Reply {
        id: "r1".to_string(),
        seq: 2,
        author: Author::Agent,
        body: "renamed it".into(),
    });
    saved.messages.push(Message {
        id: "m1".to_string(),
        seq: 3,
        author: Author::Agent,
        body: "done for now".into(),
    });
    saved.channel.called = true;
    // 復元した画面でも、kemi wait が呼ばれたことと未渡しが分かる。
    let restored = Arc::new(RecordingSink::default());
    *restored.initial.lock().unwrap() = Some(saved);
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), restored).await;

    let review: Value = server.get("api/review").await.json().await.unwrap();

    assert_eq!(review["comments"][0]["replies"][0]["author"], "agent");
    assert_eq!(review["messages"][0]["body"], "done for now");
    assert_eq!(review["agent"]["called"], true);
    assert_eq!(review["agent"]["status"], "working");
    assert_eq!(review["agent"]["unhanded"], 1);
}

#[tokio::test]
async fn the_page_gets_the_review_id_to_copy_only_when_an_agent_can_connect() {
    let server = AgentServer::start().await;
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["agent"]["review"], "01TESTREVIEW");

    let (alone, _sink) = TestServer::with_recording_session().await;
    let review: Value = alone.get("api/review").await.json().await.unwrap();
    assert!(review["agent"]["review"].is_null(), "{}", review["agent"]);
}

#[tokio::test]
async fn a_review_where_kemi_wait_was_never_called_is_not_connected() {
    let (server, _sink) = TestServer::with_recording_session().await;

    let review: Value = server.get("api/review").await.json().await.unwrap();

    assert_eq!(review["agent"]["called"], false);
    assert_eq!(review["agent"]["status"], "not-connected");
    assert_eq!(review["messages"], json!([]));
}

#[tokio::test]
async fn a_waiting_request_returns_when_the_reviewer_hands() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    let waiting = {
        let url = server.agent_url(&server.agent_token, "wait");
        tokio::spawn(async move {
            http()
                .post(url)
                .json(&json!({ "timeout_ms": null }))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    server.wait_until_status("waiting").await;

    server.page.hand().await;

    let answer = tokio::time::timeout(WAIT_LIMIT, waiting)
        .await
        .unwrap()
        .unwrap();
    let handed = handed_events(&answer);
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0]["comments"][0]["change"], "added");
    assert_eq!(handed[0]["comments"][0]["comment"]["body"], "first");
}

#[tokio::test]
async fn a_request_returns_at_once_when_something_already_happened() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    server.page.hand().await;
    server
        .page
        .comment(json!({"op": "delete", "id": "c1"}))
        .await;
    server.page.hand().await;

    let (status, answer) = server.wait(Some(5_000)).await;

    assert_eq!(status, 200);
    let handed = handed_events(&answer);
    assert_eq!(handed.len(), 2);
    assert_eq!(
        handed[1]["comments"][0],
        json!({ "change": "deleted", "comment": { "id": "c1" } })
    );
}

#[tokio::test]
async fn received_events_are_not_returned_again() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    server.page.hand().await;
    let (_, first) = server.wait(Some(5_000)).await;
    assert_eq!(handed_events(&first).len(), 1);

    let (status, second) = server.wait(Some(100)).await;

    assert_eq!(status, 200);
    assert_eq!(second["timeout"], true);
    eventually(
        "the received events leaving the stored state",
        async || server.sink.last_state().channel.events,
        Vec::is_empty,
    )
    .await;
}

#[tokio::test]
async fn a_second_concurrent_wait_is_refused() {
    let server = AgentServer::start().await;
    let _first = {
        let url = server.agent_url(&server.agent_token, "wait");
        tokio::spawn(async move {
            http()
                .post(url)
                .json(&json!({ "timeout_ms": null }))
                .send()
                .await
        })
    };
    server.wait_until_status("waiting").await;

    let (status, _) = server.wait(Some(100)).await;

    assert_eq!(status, 409);
}

#[tokio::test]
async fn a_runtime_error_ends_the_waiting_request_with_its_reason_and_serve_returns() {
    let source = Arc::new(FailingRefreshSource {
        inner: FakeSource::new(),
        fail: AtomicBool::new(false),
    });
    let server = AgentServer::start_with(source.clone()).await;
    let waiting = {
        let url = server.agent_url(&server.agent_token, "wait");
        tokio::spawn(async move {
            let response = http()
                .post(url)
                .json(&json!({ "timeout_ms": null }))
                .send()
                .await
                .unwrap();
            (
                response.status().as_u16(),
                response.json::<Value>().await.unwrap(),
            )
        })
    };
    server.wait_until_status("waiting").await;
    source.fail.store(true, Ordering::SeqCst);

    let _ = server.page.get("api/file/f1").await;

    let (status, answer) = tokio::time::timeout(WAIT_LIMIT, waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status, 503);
    assert!(
        answer["error"].as_str().unwrap().contains("disk went away"),
        "{answer}"
    );
    let outcome = tokio::time::timeout(WAIT_LIMIT, server.page.task)
        .await
        .expect("serve did not return")
        .unwrap();
    assert!(outcome.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hand_racing_the_timeout_is_returned_by_the_next_wait() {
    let server = AgentServer::start().await;
    for round in 0..8u32 {
        server
            .page
            .add_comment_with_body(11, &format!("round {round}"))
            .await;
        let waiting = tokio::spawn(wait_like_the_cli(
            server.agent_port,
            server.agent_token.clone(),
            Some(40),
        ));
        // 時間切れの前後に散らして渡す。
        tokio::time::sleep(Duration::from_millis(30 + u64::from(round) * 3)).await;
        server.page.hand().await;
        let (_, first) = waiting.await.unwrap();

        let mut seen = handed_events(&first).len();
        if seen == 0 {
            assert_eq!(first["timeout"], true);
            let (_, next) = server.wait(Some(5_000)).await;
            seen = handed_events(&next).len();
        }

        assert_eq!(seen, 1, "round {round}");
    }
}

#[tokio::test]
async fn reply_writes_replies_and_messages_as_the_agent_and_returns_their_ids() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;

    let (status, answer) = server
        .reply(json!([
            agent_reply("c1", "renamed it"),
            { "type": "message", "body": "done for now" },
        ]))
        .await;

    assert_eq!(status, 200);
    assert_eq!(answer["ids"].as_array().map(Vec::len), Some(2), "{answer}");
    let state = server.sink.last_state();
    // 書いた順の id が、書いた返信と発言のものになる。
    assert_eq!(answer["ids"][0], state.comments[0].replies[0].id.as_str());
    assert_eq!(answer["ids"][1], state.messages[0].id.as_str());
    assert_eq!(state.comments[0].replies[0].body, "renamed it".into());
    assert_eq!(
        state.comments[0].replies[0].author,
        kemi_core::domain::review::Author::Agent
    );
    assert_eq!(
        state.messages[0].author,
        kemi_core::domain::review::Author::Agent
    );
    // エージェントの書き込みは渡す対象ではない。
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["agent"]["unhanded"], 1);
}

#[tokio::test]
async fn writes_from_the_page_and_the_agent_share_one_sequence_in_writing_order() {
    let server = AgentServer::start().await;
    let comment = server.page.add_comment_with_body(11, "first").await;
    server.reply(json!([agent_reply("c1", "renamed it")])).await;
    let message: Value = server
        .page
        .post("api/message", json!({ "body": "overall" }))
        .await
        .json()
        .await
        .unwrap();
    let thread: Value = server
        .page
        .comment(json!({"op": "reply", "id": "c1", "body": "thanks"}))
        .await
        .json()
        .await
        .unwrap();

    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    let file: Value = server.page.get("api/file/f1").await.json().await.unwrap();

    let in_review = &review["comments"][0];
    let sequence = [
        in_review["seq"].as_u64().unwrap(),
        in_review["replies"][0]["seq"].as_u64().unwrap(),
        review["messages"][0]["seq"].as_u64().unwrap(),
        in_review["replies"][1]["seq"].as_u64().unwrap(),
    ];
    assert!(
        sequence.windows(2).all(|pair| pair[0] < pair[1]),
        "{sequence:?}"
    );
    assert_eq!(comment["seq"], in_review["seq"]);
    assert_eq!(message["seq"], review["messages"][0]["seq"]);
    assert_eq!(thread["replies"], in_review["replies"]);
    assert_eq!(file["comments"][0], *in_review);
}

#[tokio::test]
async fn a_write_after_restoring_comes_after_every_earlier_write() {
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
    server.add_new_side_comment().await;
    server
        .comment(json!({"op": "reply", "id": "c1", "body": "first"}))
        .await;
    server
        .post("api/message", json!({ "body": "overall" }))
        .await;
    let saved = sink.last_state();
    let restored = Arc::new(RecordingSink::default());
    *restored.initial.lock().unwrap() = Some(saved);
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), restored).await;
    let before: Value = server.get("api/review").await.json().await.unwrap();

    let thread: Value = server
        .comment(json!({"op": "reply", "id": "c1", "body": "second"}))
        .await
        .json()
        .await
        .unwrap();

    let earlier = [
        before["comments"][0]["seq"].as_u64().unwrap(),
        before["comments"][0]["replies"][0]["seq"].as_u64().unwrap(),
        before["messages"][0]["seq"].as_u64().unwrap(),
    ];
    assert_eq!(
        thread["replies"][0]["seq"],
        before["comments"][0]["replies"][0]["seq"]
    );
    let latest = thread["replies"][1]["seq"].as_u64().unwrap();
    assert!(
        earlier.iter().all(|seq| *seq < latest),
        "{earlier:?} {latest}"
    );
}

#[tokio::test]
async fn a_write_to_a_missing_comment_writes_nothing() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;

    let (status, answer) = server
        .reply(json!([agent_reply("c1", "ok"), agent_reply("c9", "lost")]))
        .await;

    assert_eq!(status, 400);
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "{answer}"
    );
    assert_nothing_written(&server).await;
}

#[tokio::test]
async fn a_body_over_64_kb_writes_nothing() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;

    let (status, _) = server
        .reply(json!([
            { "type": "message", "body": "fine" },
            agent_reply("c1", &"x".repeat(64 * 1024 + 1)),
        ]))
        .await;

    assert_eq!(status, 400);
    assert_nothing_written(&server).await;
}

#[tokio::test]
async fn the_fifty_first_agent_reply_on_a_comment_writes_nothing() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    let fifty: Vec<Value> = (0..50)
        .map(|n| agent_reply("c1", &format!("{n}")))
        .collect();
    let (status, _) = server.reply(Value::Array(fifty)).await;
    assert_eq!(status, 200);

    let (status, _) = server
        .reply(json!([{ "type": "message", "body": "fine" }, agent_reply("c1", "one more")]))
        .await;

    assert_eq!(status, 400);
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(
        review["comments"][0]["replies"].as_array().unwrap().len(),
        50
    );
    assert_eq!(review["messages"], json!([]));
}

#[tokio::test]
async fn the_two_hundred_first_agent_message_writes_nothing() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    let many: Vec<Value> = (0..200)
        .map(|n| json!({ "type": "message", "body": format!("{n}") }))
        .collect();
    let (status, _) = server.reply(Value::Array(many)).await;
    assert_eq!(status, 200);

    let (status, _) = server
        .reply(json!([agent_reply("c1", "fine"), { "type": "message", "body": "one more" }]))
        .await;

    assert_eq!(status, 400);
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["comments"][0]["replies"], json!([]));
    assert_eq!(review["messages"].as_array().unwrap().len(), 200);
}

#[tokio::test]
async fn a_write_with_variants_is_a_shape_error_outside_a_live_review() {
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    let mut with_variants = agent_reply("c1", "two ideas");
    with_variants["variants"] = json!([{ "label": "A", "replace": [], "css": "" }]);

    let (status, _) = server.reply(json!([with_variants])).await;

    assert_eq!(status, 400);
    assert_nothing_written(&server).await;
}

#[tokio::test]
async fn the_status_goes_from_not_connected_to_waiting_to_working() {
    let server = AgentServer::start().await;
    let mut events = server.page.get("api/events").await;
    assert_eq!(server.agent_status().await, "not-connected");

    let waiting = {
        let url = server.agent_url(&server.agent_token, "wait");
        tokio::spawn(async move {
            http()
                .post(url)
                .json(&json!({ "timeout_ms": 300 }))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    let notified = next_sse_event(&mut events, "agent").await;
    assert_eq!(notified["status"], "waiting");
    assert_eq!(notified["called"], true);

    waiting.await.unwrap();

    let notified = next_sse_event(&mut events, "agent").await;
    assert_eq!(notified["status"], "working");
    assert_eq!(server.agent_status().await, "working");
    assert!(server.sink.last_state().channel.called);
}

#[tokio::test]
async fn working_for_ten_minutes_without_the_agent_turns_into_no_response_and_reaches_the_page() {
    use kemi_core::domain::agent::UNRESPONSIVE_AFTER_MILLIS;
    use std::sync::atomic::AtomicU64;
    let now = Arc::new(AtomicU64::new(1_000_000));
    let clock: kemi_server::Clock = {
        let now = now.clone();
        Arc::new(move || u128::from(now.load(Ordering::SeqCst)))
    };
    let server = AgentServer::start_with_clock(clock).await;
    server.wait(Some(0)).await;
    assert_eq!(server.agent_status().await, "working");
    let mut events = server.page.get("api/events").await;

    let ten_minutes = u64::try_from(UNRESPONSIVE_AFTER_MILLIS).unwrap();
    now.fetch_add(ten_minutes - 1, Ordering::SeqCst);
    assert_eq!(server.agent_status().await, "working");
    now.fetch_add(1, Ordering::SeqCst);
    assert_eq!(server.agent_status().await, "no-response");

    // 状態は時刻だけで変わるので、要求が無くても見回りが知らせる。見回りの間隔（15 秒）を
    // 越えるまでサーバの時間を進める。進めた後は止めておかない（止めたままだと、通知を
    // 読む前に読む期限まで時間が飛ぶことがある）。
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(60)).await;
    tokio::time::resume();
    let notified = next_sse_event(&mut events, "agent").await;
    assert_eq!(notified["status"], "no-response");
}

#[tokio::test]
async fn an_empty_reply_writes_nothing_returns_no_ids_and_keeps_the_state_and_its_ten_minutes() {
    use kemi_core::domain::agent::UNRESPONSIVE_AFTER_MILLIS;
    use std::sync::atomic::AtomicU64;
    let now = Arc::new(AtomicU64::new(1_000_000));
    let clock: kemi_server::Clock = {
        let now = now.clone();
        Arc::new(move || u128::from(now.load(Ordering::SeqCst)))
    };
    let server = AgentServer::start_with_clock(clock).await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    server.page.hand().await;
    let (status, _) = server.wait(Some(0)).await;
    assert_eq!(status, 200);
    let ten_minutes = u64::try_from(UNRESPONSIVE_AFTER_MILLIS).unwrap();
    now.fetch_add(ten_minutes - 1, Ordering::SeqCst);
    let lines = vec![(c1, "working".to_string())];
    assert_eq!(server.hand_lines().await, lines);

    let (status, answer) = server.reply(json!([])).await;

    assert_eq!(status, 200);
    assert_eq!(answer, json!({ "ids": [] }));
    assert_nothing_written(&server).await;
    assert_eq!(server.agent_status().await, "working");
    assert_eq!(server.hand_lines().await, lines);
    // 届いた書き込みとして 10 分を数え直していれば、ここではまだ作業中。
    now.fetch_add(1, Ordering::SeqCst);
    assert_eq!(server.agent_status().await, "no-response");
}

#[tokio::test]
async fn replying_to_one_of_two_handed_threads_keeps_it_working_and_to_both_makes_it_replied() {
    let server = AgentServer::start().await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    let c2 = server.page.add_comment_with_body(12, "second").await["id"].clone();
    server.page.hand().await;
    assert_eq!(
        server.hand_lines().await,
        vec![
            (c1.clone(), "pending".to_string()),
            (c2.clone(), "pending".to_string())
        ]
    );
    let (status, _) = server.wait(Some(1000)).await;
    assert_eq!(status, 200);
    server.wait_until_status("working").await;
    assert_eq!(
        server.hand_lines().await,
        vec![
            (c1.clone(), "working".to_string()),
            (c2.clone(), "working".to_string())
        ]
    );

    server
        .reply(json!([{ "type": "reply", "comment_id": c1, "body": "done" }]))
        .await;

    assert_eq!(server.agent_status().await, "working");
    assert_eq!(
        server.hand_lines().await,
        vec![(c2.clone(), "working".to_string())]
    );

    server
        .reply(json!([{ "type": "reply", "comment_id": c2, "body": "done too" }]))
        .await;

    assert_eq!(server.agent_status().await, "replied");
    assert_eq!(server.hand_lines().await, vec![]);
}

#[tokio::test]
async fn a_deleted_handed_comment_leaves_no_line_and_is_not_waited_for() {
    let server = AgentServer::start().await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    let c2 = server.page.add_comment_with_body(12, "second").await["id"].clone();
    server.page.hand().await;
    server.wait(Some(1000)).await;
    server.wait_until_status("working").await;

    server.page.comment(json!({"op": "delete", "id": c2})).await;

    assert_eq!(
        server.hand_lines().await,
        vec![(c1.clone(), "working".to_string())]
    );
    server
        .reply(json!([{ "type": "reply", "comment_id": c1, "body": "done" }]))
        .await;
    assert_eq!(server.agent_status().await, "replied");
}

#[tokio::test]
async fn deleting_the_last_awaited_comment_after_a_reply_turns_the_status_to_replied() {
    let server = AgentServer::start().await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    let c2 = server.page.add_comment_with_body(12, "second").await["id"].clone();
    server.page.hand().await;
    server.wait(Some(1000)).await;
    server.wait_until_status("working").await;
    server
        .reply(json!([{ "type": "reply", "comment_id": c1, "body": "done" }]))
        .await;
    assert_eq!(server.agent_status().await, "working");

    server.page.comment(json!({"op": "delete", "id": c2})).await;

    assert!(server.hand_lines().await.is_empty());
    assert_eq!(server.agent_status().await, "replied");
}

#[tokio::test]
async fn deleting_the_awaited_comments_before_any_write_keeps_the_status_working() {
    let server = AgentServer::start().await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    server.page.hand().await;
    server.wait(Some(1000)).await;
    server.wait_until_status("working").await;

    server.page.comment(json!({"op": "delete", "id": c1})).await;

    assert_eq!(server.agent_status().await, "working");
}

#[tokio::test]
async fn a_comment_handed_then_deleted_before_suspending_leaves_no_line_after_resuming() {
    let server = AgentServer::start().await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    let c2 = server.page.add_comment_with_body(12, "second").await["id"].clone();
    server.page.hand().await;
    server.page.comment(json!({"op": "delete", "id": c1})).await;
    let saved = server.sink.last_state();
    let resumed = AgentServer::resume(saved).await;

    let (status, _) = resumed.wait(Some(1000)).await;
    assert_eq!(status, 200);
    resumed.wait_until_status("working").await;
    assert_eq!(
        resumed.hand_lines().await,
        vec![(c2.clone(), "working".to_string())]
    );
    resumed
        .reply(json!([{ "type": "reply", "comment_id": c2, "body": "done" }]))
        .await;

    assert_eq!(resumed.agent_status().await, "replied");
}

#[tokio::test]
async fn handing_while_replied_keeps_it_replied_with_a_pending_line() {
    let server = AgentServer::start().await;
    let c1 = server.page.add_comment_with_body(11, "first").await["id"].clone();
    server.page.hand().await;
    server.wait(Some(1000)).await;
    server.wait_until_status("working").await;
    server
        .reply(json!([{ "type": "reply", "comment_id": c1, "body": "done" }]))
        .await;
    assert_eq!(server.agent_status().await, "replied");
    let c2 = server.page.add_comment_with_body(12, "second").await["id"].clone();

    server.page.hand().await;

    assert_eq!(server.agent_status().await, "replied");
    assert_eq!(server.hand_lines().await, vec![(c2, "pending".to_string())]);
}

#[tokio::test]
async fn a_dropped_wait_connection_turns_the_status_to_working() {
    use tokio::io::AsyncWriteExt;
    let server = AgentServer::start().await;
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.agent_port))
        .await
        .unwrap();
    let body = r#"{"timeout_ms":null}"#;
    let request = format!(
        "POST /a/{}/wait HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        server.agent_token,
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    server.wait_until_status("waiting").await;

    drop(stream);

    server.wait_until_status("working").await;
}

#[tokio::test]
async fn a_large_batch_within_the_limits_is_written_whatever_its_size() {
    let server = AgentServer::start().await;
    // 1 件ずつは上限の中でも、まとめると 2 MB を超える書き込み。
    let many: Vec<Value> = (0..40)
        .map(|_| json!({ "type": "message", "body": "x".repeat(60 * 1024) }))
        .collect();

    let (status, answer) = server.reply(Value::Array(many)).await;

    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["ids"].as_array().unwrap().len(), 40);
}

#[tokio::test]
async fn events_answered_to_a_wait_that_never_finished_are_returned_by_the_next_wait() {
    use tokio::io::AsyncWriteExt;
    let server = AgentServer::start().await;
    server.page.add_comment_with_body(11, "first").await;
    server.page.hand().await;
    // 応答を受け取りきる前に終わった kemi wait（強制終了など）。
    let body = r#"{"timeout_ms":null}"#;
    let mut interrupted = agent_request_head(&server, "wait", body.len()).await;
    interrupted.write_all(body.as_bytes()).await.unwrap();
    assert_eq!(response_status(&mut interrupted).await, 200);
    drop(interrupted);
    assert_eq!(saved_handed(&server.sink).len(), 1);

    let (status, answer) = server.wait(Some(2_000)).await;

    assert_eq!(status, 200);
    assert_eq!(handed_events(&answer).len(), 1, "{answer}");
}

#[tokio::test]
async fn file_comment_additions_accept_the_utf8_boundary_and_reject_overflow_without_changes() {
    file_body_boundary("add").await;
}

#[tokio::test]
async fn file_comment_edits_accept_the_utf8_boundary_and_reject_overflow_without_changes() {
    file_body_boundary("edit").await;
}

#[tokio::test]
async fn file_comment_replies_accept_the_utf8_boundary_and_reject_overflow_without_changes() {
    file_body_boundary("reply").await;
}

#[tokio::test]
async fn reviewer_messages_accept_the_utf8_boundary_and_reject_overflow_without_changes() {
    file_body_boundary("message").await;
}

#[tokio::test]
async fn a_hand_over_capacity_keeps_all_typed_messages_unhanded_and_stores_no_partial_event() {
    use kemi_core::domain::conversation_content::ConversationContent;
    use kemi_core::domain::review::{Author, Message};
    let parts:Vec<kemi_core::domain::parts::Part>=serde_json::from_value(json!((0..16).map(|_|json!({"type":"code","body":"x".repeat(32000),"language":"pseudocode","role":"example"})).collect::<Vec<_>>())).unwrap();
    let mut saved = SessionState::default();
    saved.channel.called = true;
    for n in 1..=20 {
        let id = format!("m{n}");
        saved.messages.push(Message {
            id: id.clone(),
            seq: n,
            author: Author::Reviewer,
            body: ConversationContent::Parts(parts.clone()),
        });
        saved.channel.note_message(&id);
    }
    kemi_core::session::validate_additional(&saved).unwrap();
    let server = AgentServer::resume(saved.clone()).await;
    assert_eq!(server.page.post("api/hand", json!({})).await.status(), 413);
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(context["explanation_revision"], 0);
    let after = server.sink.last_state();
    assert_eq!(after.messages, saved.messages);
    assert_eq!(after.channel, saved.channel);
    let (_, waiting) = server.wait(Some(0)).await;
    assert_eq!(waiting["timeout"], true);
}

#[tokio::test]
async fn plain_writes_keep_legacy_container_extensions_without_relaxing_part_fields_or_mixed_content()
 {
    let server = AgentServer::start().await;
    assert_eq!(
        server
            .reply(json!([{"type":"message","body":"**plain**","extension":{"opaque":true}}]))
            .await
            .0,
        200
    );
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["messages"][0]["body"], "**plain**");
    assert!(review["messages"][0]["parts"].is_null());
    assert_eq!(
        server
            .reply(json!([{"type":"message","body":"plain","parts":null,"extension":true}]))
            .await
            .0,
        400
    );
    assert_eq!(
        server
            .reply(
                json!([{"type":"message","parts":[{"type":"text","body":"part","extension":true}]}])
            )
            .await
            .0,
        400
    );
}
