use super::*;

#[tokio::test]
async fn submit_schema_follows_contract() {
    let server = TestServer::start().await;
    let _ = server
        .post(
            "api/comment",
            json!({
                "op": "add",
                "file_id": "f1",
                "side": "new",
                "start_line": 11,
                "end_line": 11,
                "body": "新側の本文",
                "suggestion": "NEW\n"
            }),
        )
        .await
        .json::<Value>()
        .await
        .unwrap();
    let _ = server
        .post(
            "api/comment",
            json!({
                "op": "add",
                "file_id": "f1",
                "side": "old",
                "start_line": 11,
                "end_line": 11,
                "body": "旧側の本文"
            }),
        )
        .await
        .json::<Value>()
        .await
        .unwrap();
    let file_wide: Value = server
        .post(
            "api/comment",
            json!({
                "op": "add",
                "file_id": "f1",
                "side": "new",
                "body": "全体の本文\n\"引用符\""
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(file_wide["start_line"], Value::Null);
    assert_eq!(file_wide["end_line"], Value::Null);
    assert_eq!(file_wide["quote"], json!([]));
    assert_eq!(file_wide["suggestion"], Value::Null);

    let _ = server
        .post(
            "api/comment",
            json!({"op": "reply", "id": "c1", "body": "返信1"}),
        )
        .await;
    let resolved: Value = server
        .post(
            "api/comment",
            json!({"op": "resolve", "id": "c1", "resolved": true}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(resolved["resolved"], true);

    let response = server
        .post("api/submit", json!({"verdict": "approved"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;

    assert_eq!(document["kemi"], 3);
    assert_eq!(document["title"], "テストのレビュー");
    assert_eq!(document["messages"], json!([]));
    assert_eq!(document["verdict"], "approved");
    assert_eq!(document["approval"][0]["identity"], "sha256:abc");
    let comments = document["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 3, "旧側の 400 は記録されない");
    assert_eq!(comments[0]["id"], "c1");
    assert_eq!(comments[0]["group_id"], "g1");
    assert_eq!(comments[0]["group_title"], "最初の変更");
    assert_eq!(comments[0]["path"], "src/a.rs");
    assert_eq!(comments[0]["side"], "new");
    assert_eq!(comments[0]["start_line"], 11);
    assert_eq!(comments[0]["end_line"], 11);
    assert_eq!(comments[0]["quote"], json!(["new"]));
    assert_eq!(comments[0]["page"], Value::Null);
    assert_eq!(
        comments[0]["replies"],
        json!([{
            "id": "r1",
            "author": "reviewer",
            "body": "返信1",
            "parts": null,
            "variants": [],
            "chosen": null,
            "applied": null,
        }])
    );
    assert_eq!(comments[0]["resolved"], true);
    assert_eq!(comments[0]["outdated"], false);
    assert_eq!(
        comments[0]["suggestion"]["replacement"],
        Value::String("NEW\n".to_string())
    );
    assert_eq!(comments[1]["side"], "old");
    assert_eq!(comments[1]["quote"], json!(["old"]));
    assert_eq!(comments[1]["suggestion"], Value::Null);
    assert_eq!(comments[2]["side"], "new");
    assert_eq!(comments[2]["quote"], json!([]));
    assert_eq!(comments[2]["body"], "全体の本文\n\"引用符\"");
}

#[tokio::test]
async fn approval_passthrough_on_submit() {
    let server = TestServer::start().await;
    let response = server
        .post("api/submit", json!({"verdict": "changes_requested"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;

    assert_eq!(document["verdict"], "changes_requested");
    assert_eq!(document["approval"][0]["path"], "src/a.rs");
    assert_eq!(document["approval"][0]["identity"], "sha256:abc");
    assert_eq!(document["comments"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn submit_concurrent_409() {
    let server = TestServer::start().await;
    let _ = server
        .post(
            "api/comment",
            json!({
                "op": "add", "file_id": "f1", "side": "new",
                "start_line": 11, "end_line": 11, "body": "本文"
            }),
        )
        .await;

    // 受理側が本文を読んでいる間に 2 つ目を届かせ、409 を確実に観測する。
    // 2 つ目はサーバと同じランタイムを使わない素の接続で送る。
    server.source.set_content_delay(Duration::from_millis(300));
    let (signal, wait) = std::sync::mpsc::channel::<()>();
    let address = ("127.0.0.1", server.port);
    let origin = server.origin.clone();
    let raw = std::thread::spawn(move || {
        wait.recv().unwrap();
        raw_post(
            address,
            "/s/test-token/api/submit",
            &origin,
            r#"{"verdict":"changes_requested"}"#,
        )
    });

    let url = server.url.clone();
    let origin = server.origin.clone();
    let mut first_task = tokio::spawn(async move {
        post_json(&url, &origin, "api/submit", json!({"verdict": "approved"})).await
    });

    until_reached(
        "the first submit reading content",
        server.source.content_started.notified(),
        &mut first_task,
    )
    .await;
    signal.send(()).unwrap();
    let second_status = raw.join().unwrap();

    let mut statuses = vec![first_task.await.unwrap().status().as_u16(), second_status];
    statuses.sort();
    assert_eq!(statuses, vec![200, 409]);
    let _ = server.finish().await;
}

#[tokio::test]
async fn submit_lists_reviewer_and_agent_replies_in_creation_order_with_messages() {
    use kemi_core::domain::review::{Author, Message, Reply};
    let sink = Arc::new(RecordingSink::default());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
    server.add_new_side_comment().await;
    // エージェントの書き込みはまだ経路が無いので、保存された状態に直接置いて復元する。
    let mut saved = sink.last_state();
    saved.comments[0].replies.push(Reply {
        id: "r1".to_string(),
        seq: 2,
        author: Author::Agent,
        body: "renamed it".into(),
    });
    saved.last_reply = 1;
    saved.messages.push(Message {
        id: "m1".to_string(),
        seq: 3,
        author: Author::Agent,
        body: "all comments are addressed".into(),
    });
    saved.last_message = 1;
    let restored = Arc::new(RecordingSink::default());
    *restored.initial.lock().unwrap() = Some(saved);
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), restored).await;
    server
        .comment(json!({"op": "reply", "id": "c1", "body": "thanks"}))
        .await;

    let document = server.submit_document().await;

    let replies = &document["comments"][0]["replies"];
    assert_eq!(replies[0]["id"], "r1");
    assert_eq!(replies[0]["author"], "agent");
    assert_ne!(replies[1]["id"], replies[0]["id"]);
    assert_eq!(replies[1]["author"], "reviewer");
    assert_eq!(replies[1]["body"], "thanks");
    for reply in replies.as_array().unwrap() {
        assert_eq!(reply["variants"], json!([]));
        assert_eq!(reply["chosen"], Value::Null);
        assert_eq!(reply["applied"], Value::Null);
    }
    assert_eq!(
        document["messages"],
        json!([{"id": "m1", "author": "agent", "body": "all comments are addressed", "parts":null}])
    );
}

#[tokio::test]
async fn submit_hands_the_result_to_the_waiting_request_and_then_stops() {
    let server = AgentServer::start().await;
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

    let response = server
        .page
        .post("api/submit", json!({"verdict": "changes_requested"}))
        .await;
    assert_eq!(response.status(), 200);

    let answer = tokio::time::timeout(WAIT_LIMIT, waiting)
        .await
        .unwrap()
        .unwrap();
    let events = answer["events"].as_array().unwrap();
    let submitted = events.last().unwrap();
    assert_eq!(submitted["type"], "submitted");
    assert_eq!(submitted["result"]["verdict"], "changes_requested");
    assert_eq!(submitted["result"]["kemi"], 3);
    let document = server.page.finish().await;
    assert_eq!(document["verdict"], "changes_requested");
}

#[tokio::test]
async fn submitted_comments_carry_only_the_keys_of_the_contract() {
    let server = TestServer::start().await;
    server.add_new_side_comment().await;

    let comments = server.submit_comments().await;

    let mut keys: Vec<&str> = comments[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "body",
            "end_line",
            "explanation",
            "group_id",
            "group_title",
            "id",
            "outdated",
            "page",
            "path",
            "quote",
            "replies",
            "resolved",
            "side",
            "start_line",
            "suggestion",
            "target_type",
        ]
    );
}

#[tokio::test]
async fn submit_reads_each_commented_file_once_for_both_sides() {
    let server = TestServer::start().await;
    for side in ["old", "new", "new"] {
        let response = server.post("api/comment", json!({"op":"add","file_id":"f1","side":side,"start_line":1,"end_line":1,"body":side})).await;
        assert_eq!(response.status(), 200);
    }
    server.source.reads.store(0, Ordering::SeqCst);
    assert_eq!(
        server
            .post("api/submit", json!({"verdict":"approved"}))
            .await
            .status(),
        200
    );
    assert_eq!(server.source.reads.load(Ordering::SeqCst), 1);
    let result = server.finish().await;
    let comments = result["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 3);
    assert!(comments.iter().all(|comment| comment["outdated"] == false));
    assert_eq!(comments[0]["side"], "old");
    assert_eq!(comments[1]["side"], "new");
}

#[tokio::test]
async fn reviewer_writes_arriving_after_submit_are_rejected_instead_of_lost() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (action, body) in [
        ("message", json!({"body":"late"})),
        (
            "comment",
            json!({"op":"add","file_id":"f1","side":"new","body":"late"}),
        ),
        ("comment", json!({"op":"edit","id":"c1","body":"late"})),
        ("comment", json!({"op":"reply","id":"c1","body":"late"})),
        ("comment", json!({"op":"resolve","id":"c1","resolved":true})),
        ("comment", json!({"op":"delete","id":"c1"})),
    ] {
        let server = TestServer::start().await;
        let accepted = server
            .post(
                "api/comment",
                json!({"op":"add","file_id":"f1","side":"new","body":"accepted"}),
            )
            .await;
        assert_eq!(accepted.status(), 200);
        let body = body.to_string();
        let mut late = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
            .await
            .unwrap();
        let head = format!(
            "POST /s/test-token/api/{action} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            server.port,
            server.origin,
            body.len()
        );
        late.write_all(head.as_bytes()).await.unwrap();
        let mut first = [0];
        assert!(
            tokio::time::timeout(Duration::from_millis(100), late.read(&mut first))
                .await
                .is_err()
        );
        assert_eq!(
            server
                .post("api/submit", json!({"verdict":"approved"}))
                .await
                .status(),
            200
        );
        late.write_all(body.as_bytes()).await.unwrap();
        assert_eq!(response_status(&mut late).await, 409, "{action}: {body}");
        let result = server.finish().await;
        assert_eq!(result["comments"][0]["body"], "accepted");
        assert!(
            result["comments"][0]["replies"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(result["comments"][0]["resolved"], false);
        assert!(result["messages"].as_array().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_reply_arriving_after_the_submit_is_not_reported_as_written_when_missing_from_the_result()
{
    use tokio::io::AsyncWriteExt;
    let server = AgentServer::start().await;
    let body = json!({ "writes": [{ "type": "message", "body": "late" }] }).to_string();
    let mut late = agent_request_head(&server, "reply", body.len()).await;

    let response = tokio::time::timeout(
        WAIT_LIMIT,
        server
            .page
            .post("api/submit", json!({"verdict": "approved"})),
    )
    .await
    .expect("the submit never answered");
    assert_eq!(response.status(), 200);
    late.write_all(body.as_bytes()).await.unwrap();
    let status = response_status(&mut late).await;

    let document = server.page.finish().await;
    let in_result = document["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["body"] == "late");
    assert!(
        status != 200 || in_result,
        "the reply succeeded ({status}) but is missing from the result: {document}"
    );
}
