use super::*;

fn explanation_sections() -> Value {
    json!([{"id":"overview","title":"Overview","blocks":[{"id":"intent","part":{"type":"text","body":"**Intent**"},"references":[]}]}])
}

#[tokio::test]
async fn new_result_without_explanation_has_version_four_and_explicit_content_keys() {
    let server = TestServer::start().await;
    let review: Value = server.get("api/review").await.json().await.unwrap();
    assert!(review["diff_display_version"].as_str().is_some());
    let response = server
        .post("api/message", json!({"body":"**plain**"}))
        .await;
    assert!(response.status().is_success());
    let response = server
        .post("api/submit", json!({"verdict":"approved"}))
        .await;
    assert!(response.status().is_success());
    let result = server.finish().await;
    assert_eq!(result["kemi"], 4);
    assert_eq!(
        result["diff_display_version"],
        review["diff_display_version"]
    );
    assert!(result.get("explanation").unwrap().is_null());
    assert_eq!(result["messages"][0]["body"], "**plain**");
    assert!(result["messages"][0].get("parts").unwrap().is_null());
}

#[tokio::test]
async fn restored_explanation_target_keeps_its_complete_quote_and_parts_in_wait_and_result() {
    use kemi_core::domain::conversation_content::ConversationContent;
    use kemi_core::domain::explanation::{
        ExplanationDocument, ExplanationHistory, ExplanationSelection, ExplanationVersion,
        quote_block,
    };
    use kemi_core::domain::review::{
        Author, Comment, CommentTarget, ExplanationTarget, Message, Reply,
    };
    let document:ExplanationDocument=serde_json::from_value(json!({"sections":[{"id":"s","title":"Original","blocks":[{"id":"b","part":{"type":"code","body":"literal\r\n","language":"pseudocode","role":"example"},"references":[]}]}]})).unwrap();
    let quote = quote_block(&document, "b").unwrap();
    let mut changed = document.clone();
    changed.sections[0].blocks[0].part =
        serde_json::from_value(json!({"type":"text","body":"changed"})).unwrap();
    let sink = Arc::new(RecordingSink::default());
    sink.copy_already_saved.store(true, Ordering::SeqCst);
    let mut state = SessionState {
        explanation: Some(ExplanationHistory {
            current_revision: 2,
            versions: vec![
                ExplanationVersion {
                    revision: 1,
                    base_display_version: "01TESTREVIEW:1".into(),
                    sections: document.sections,
                },
                ExplanationVersion {
                    revision: 2,
                    base_display_version: "01TESTREVIEW:1".into(),
                    sections: changed.sections,
                },
            ],
        }),
        ..SessionState::default()
    };
    state.comments.push(Comment {
        id: "c1".into(),
        seq: 1,
        group_id: String::new(),
        group_title: String::new(),
        body: "question".into(),
        replies: vec![Reply {
            id: "r1".into(),
            seq: 2,
            author: Author::Agent,
            body: ConversationContent::Parts(
                serde_json::from_value(json!([{"type":"text","body":"**parts**"}])).unwrap(),
            ),
        }],
        resolved: false,
        outdated: false,
        target: CommentTarget::Explanation(ExplanationTarget {
            revision: 1,
            block_id: "b".into(),
            quote: quote.clone(),
            selection: Some(ExplanationSelection {
                text: "literal".into(),
                occurrence: 0,
            }),
        }),
    });
    state.messages.push(Message {
        id: "m1".into(),
        seq: 3,
        author: Author::Reviewer,
        body: "**plain**".into(),
    });
    state.channel.note_comment("c1");
    state.channel.note_message("m1");
    state.explanation.as_mut().unwrap().versions.reverse();
    *sink.initial.lock().unwrap() = Some(state);
    let server = AgentServer::start_with_sink(Arc::new(FakeSource::new()), sink).await;
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["comments"][0]["outdated"], true);
    assert!(
        server
            .page
            .post("api/hand", json!({}))
            .await
            .status()
            .is_success()
    );
    let (status, answer) = server.wait(Some(1000)).await;
    assert_eq!(status, 200);
    let handed = &handed_events(&answer)[0]["comments"][0]["comment"];
    assert_eq!(handed["explanation"]["quote"], json!(quote));
    assert_eq!(
        handed["explanation"]["selection"],
        json!({"text":"literal","occurrence":0})
    );
    assert_eq!(handed["replies"][0]["body"], Value::Null);
    assert_eq!(handed["replies"][0]["parts"][0]["body"], "**parts**");
    assert!(
        server
            .page
            .post("api/submit", json!({"verdict":"approved"}))
            .await
            .status()
            .is_success()
    );
    let result = server.page.finish().await;
    assert_eq!(result["kemi"], 4);
    assert_eq!(
        result["explanation"]["versions"].as_array().unwrap().len(),
        2
    );
    assert_eq!(result["explanation"]["versions"][0]["revision"], 1);
    assert_eq!(result["explanation"]["versions"][1]["revision"], 2);
    let comment = &result["comments"][0];
    assert_eq!(comment["target_type"], "explanation");
    for key in [
        "group_id",
        "group_title",
        "path",
        "side",
        "start_line",
        "end_line",
        "page",
        "suggestion",
    ] {
        assert!(comment.get(key).unwrap().is_null(), "{key}");
    }
    assert_eq!(comment["quote"], json!([]));
    assert_eq!(comment["explanation"]["quote"], json!(quote));
    assert_eq!(
        comment["explanation"]["selection"],
        json!({"text":"literal","occurrence":0})
    );
    assert_eq!(result["messages"][0]["body"], "**plain**");
    assert!(result["messages"][0].get("parts").unwrap().is_null());
}

#[tokio::test]
async fn agent_context_waits_for_the_request_body_before_answering() {
    let server = AgentServer::start().await;
    assert_eq!(server.page.get("api/review").await.status(), 200);

    let (status, context) =
        agent_response_after_delayed_body(&server, &server.agent_token, "context", "").await;

    assert_eq!(status, 200);
    assert_eq!(context["kemi"], 3);
    assert_eq!(context["review"], "01TESTREVIEW");
    assert_eq!(context["explanation_revision"], 0);
    assert!(context["explanation"].is_null());
    assert!(context["references"]["code"].is_array());
}

#[tokio::test]
async fn initial_explanation_is_saved_before_the_first_response_without_a_conversation_event() {
    let sink = Arc::new(RecordingSink::default());
    let server = AgentServer::launch(
        Launch {
            initial_explanation: Some(
                serde_json::from_value(json!({"sections":explanation_sections()})).unwrap(),
            ),
            ..Launch::new(Arc::new(FakeSource::new()))
        },
        sink.clone(),
    )
    .await;
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["explanation"]["current_revision"], 1);
    let saved = sink.accepted.lock().unwrap().first().unwrap().clone();
    assert_eq!(saved.explanation.unwrap().current_revision, 1);
    assert!(saved.channel.events.is_empty());
}

#[tokio::test]
async fn explanation_context_and_updates_are_separate_from_conversation_and_reject_conflicts() {
    let server = AgentServer::start().await;
    let context = server.agent_post("context", json!({})).await;
    assert_eq!(context.status(), 200);
    let context: Value = context.json().await.unwrap();
    assert_eq!(context["kemi"], 3);
    assert_eq!(context["explanation_revision"], 0);
    assert!(context["explanation"].is_null());
    let update = json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()});
    let first = server.agent_post("explain", update.clone()).await;
    assert_eq!(first.status(), 200);
    assert_eq!(first.json::<Value>().await.unwrap()["revision"], 1);
    assert_eq!(server.agent_post("explain", update).await.status(), 409);
    let after: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(after["explanation_revision"], 1);
    assert_eq!(after["explanation"]["sections"], explanation_sections());
    assert_eq!(server.agent_status().await, "not-connected");
    let saved = server.sink.states.lock().unwrap().last().unwrap().clone();
    assert!(
        saved.comments.is_empty() && saved.messages.is_empty() && saved.channel.events.is_empty()
    );
}

#[tokio::test]
async fn the_review_carries_only_the_current_explanation_version_and_old_ones_are_fetched_apart() {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let mut update = json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()});
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        200
    );
    update["expected_revision"] = json!(1);
    update["sections"][0]["blocks"][0]["part"]["body"] = json!("changed");
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        200
    );

    let review: Value = server.page.get("api/review").await.json().await.unwrap();

    assert_eq!(review["explanation"]["current_revision"], 2);
    let versions = review["explanation"]["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0]["revision"], 2);
    assert_eq!(versions[0]["sections"], update["sections"]);
    let old: Value = server
        .page
        .get("api/explanation?revision=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(old["sections"], explanation_sections());
}

#[tokio::test]
async fn parts_reply_accepts_typed_content_and_refuses_an_entire_batch_with_a_bad_later_write() {
    let server = AgentServer::start().await;
    let comment = server.page.add_comment_with_body(11, "question").await;
    let parts = json!([{"type":"text","body":"**reply**"},{"type":"code","body":"literal","language":"pseudocode","role":"example"}]);
    let (status,_) = server.reply(json!([{"type":"reply","comment_id":comment["id"],"parts":parts},{"type":"message","parts":parts}])).await;
    assert_eq!(status, 200);
    let before: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(before["comments"][0]["replies"][0]["parts"], parts);
    for invalid in [
        json!({"body":null,"parts":parts}),
        json!({"body":"plain","parts":parts}),
        json!({"parts":null}),
        json!({"parts":[]}),
        json!({"parts":[{"type":"code","body":"x","language":"rust","role":"wrong"}]}),
    ] {
        let mut bad = invalid;
        bad["type"] = json!("message");
        assert_eq!(
            server
                .reply(json!([{"type":"message","parts":parts},bad]))
                .await
                .0,
            400
        );
    }
    let after: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(before["messages"], after["messages"]);
    assert_eq!(before["comments"], after["comments"]);
}

#[tokio::test]
async fn context_uses_known_source_lines_and_real_sides_without_reading_content_again() {
    let source = kemi_core::source::manifest::ManifestSource::from_json(&json!({"title":"Manifest","groups":[{"id":"g","title":"G","diffs":[{"path":"a.txt","status":"add","old":"one\ntwo\n","new":"ONE\ntwo\nthree\n"},{"path":"b.txt","status":"modify","new":"present\n"}]}]}).to_string(),std::path::Path::new(".")).unwrap();
    let server = AgentServer::start_with(Arc::new(source)).await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(context["references"]["code"][0]["old_present"], true);
    assert_eq!(context["references"]["code"][0]["old_lines"], 2);
    assert_eq!(context["references"]["code"][0]["new_lines"], 3);
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["groups"][0]["files"][0]["old_present"], true);
    assert_eq!(review["groups"][0]["files"][0]["old_lines"], 2);
    assert_eq!(review["groups"][0]["files"][1]["old_present"], false);
    assert_eq!(review["groups"][0]["files"][1]["new_present"], true);
    assert!(review["groups"][0]["files"][1]["old_lines"].is_null());
    let mut sections = explanation_sections();
    sections[0]["blocks"][0]["references"] = json!([{"type":"code","unit":null,"group_id":"g","path":"a.txt","side":"old","start_line":1,"end_line":2}]);
    let update = json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":sections});
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        200
    );
    let mut bad = update;
    bad["expected_revision"] = json!(1);
    bad["sections"][0]["blocks"][0]["references"][0]["end_line"] = json!(3);
    assert_eq!(server.agent_post("explain", bad).await.status(), 400);
}

async fn explanation_body_boundary(op: &str) {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(server.agent_post("explain", json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()})).await.status(), 200);
    let comment: Value = server.page.post("api/comment", json!({"op":"add_explanation","explanation_revision":1,"block_id":"intent","body":"question"})).await.json().await.unwrap();
    let boundary = "あ".repeat(21_845) + "x";
    assert_eq!(boundary.len(), 65_536);
    assert_eq!(
        server
            .page
            .post(
                "api/comment",
                json!({"op":op,"id":comment["id"],"body":boundary})
            )
            .await
            .status(),
        200
    );
    let before: Value = server.page.get("api/review").await.json().await.unwrap();
    let saved = server.sink.last_state();
    let rejected = server
        .page
        .post(
            "api/comment",
            json!({"op":op,"id":comment["id"],"body":boundary.clone()+"x"}),
        )
        .await;
    assert_eq!(rejected.status(), 413);
    let reason: Value = rejected.json().await.unwrap();
    assert!(reason.to_string().contains("body"));
    let after: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(before["comments"], after["comments"]);
    let after_saved = server.sink.last_state();
    assert_eq!(saved.comments, after_saved.comments);
    assert_eq!(saved.channel, after_saved.channel);
    assert_eq!(saved.last_comment, after_saved.last_comment);
    assert_eq!(saved.last_reply, after_saved.last_reply);
    assert_eq!(saved.last_seq, after_saved.last_seq);
    let _next: Value = server
        .page
        .post(
            "api/comment",
            json!({"op":"reply","id":comment["id"],"body":"next reply"}),
        )
        .await
        .json()
        .await
        .unwrap();
    let next_saved = server.sink.last_state();
    assert_eq!(next_saved.last_seq, saved.last_seq + 1);
    assert_eq!(
        next_saved.channel.unhanded.comments,
        saved.channel.unhanded.comments
    );
    assert_eq!(
        next_saved.channel.unhanded.replies.len(),
        saved.channel.unhanded.replies.len() + 1
    );
    assert_eq!(next_saved.channel.events, saved.channel.events);
}

#[tokio::test]
async fn explanation_comment_edits_accept_the_utf8_boundary_and_reject_overflow_without_changes() {
    explanation_body_boundary("edit").await;
}

#[tokio::test]
async fn explanation_comment_replies_accept_the_utf8_boundary_and_reject_overflow_without_changes()
{
    explanation_body_boundary("reply").await;
}

#[tokio::test]
async fn explanation_comments_keep_the_original_block_and_old_versions_can_be_held_and_released() {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let mut update = json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()});
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        200
    );
    assert_eq!(
        server
            .page
            .post(
                "api/explanation",
                json!({"op":"hold","screen":"reader","revision":1})
            )
            .await
            .status(),
        200
    );
    let response = server.page.post("api/comment",json!({"op":"add_explanation","explanation_revision":1,"block_id":"intent","body":"question"})).await;
    assert_eq!(response.status(), 200);
    let comment: Value = response.json().await.unwrap();
    assert_eq!(comment["target_type"], "explanation");
    assert!(comment["path"].is_null());
    update["expected_revision"] = json!(1);
    update["sections"][0]["blocks"][0]["part"]["body"] = json!("changed");
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        200
    );
    let old: Value = server
        .page
        .get("api/explanation?revision=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(old["sections"], explanation_sections());
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["comments"][0]["explanation"], comment["explanation"]);
    assert_eq!(review["comments"][0]["outdated"], true);
    assert_eq!(
        server
            .page
            .post(
                "api/explanation",
                json!({"op":"release","screen":"reader","revision":1})
            )
            .await
            .status(),
        200
    );
    assert_eq!(server.page.post("api/comment",json!({"op":"add_explanation","explanation_revision":1,"block_id":"missing","body":"question"})).await.status(),404);
}

#[tokio::test]
async fn pages_cannot_be_registered_outside_a_live_review() {
    let server = TestServer::start().await;

    let response = server
        .post(
            "api/explanation",
            json!({"op":"register_page","url":"/next?state=1"}),
        )
        .await;

    assert_eq!(response.status(), 400);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["error"],
        "pages can be registered only in a live review"
    );
}

/// text・code・図のブロックを一つずつ持つ説明を入れたサーバ。
async fn server_with_selectable_explanation() -> AgentServer {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let sections = json!([{"id":"overview","title":"Overview","blocks":[
        {"id":"intent","part":{"type":"text","body":"Read the intent, then the intent again."},"references":[]},
        {"id":"snippet","part":{"type":"code","body":"let a = 1;","language":"rust","role":"example"},"references":[]},
        {"id":"grid","part":{"type":"diagram","source":"a = A\n| a |","title":"Grid","role":"example"},"references":[]}
    ]}]);
    assert_eq!(server.agent_post("explain", json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":sections})).await.status(), 200);
    server
}

fn explanation_comment_request(block_id: &str, selection: Option<Value>) -> Value {
    let mut request = json!({"op":"add_explanation","explanation_revision":1,"block_id":block_id,"body":"question"});
    if let Some(selection) = selection {
        request["selection"] = selection;
    }
    request
}

#[tokio::test]
async fn explanation_selections_reach_the_review_the_handed_wait_and_the_result() {
    let server = server_with_selectable_explanation().await;
    let text_selection = json!({"text":"intent","occurrence":1});
    let code_selection = json!({"text":"a = 1","occurrence":0});
    for (block, selection) in [
        ("intent", Some(text_selection.clone())),
        ("snippet", Some(code_selection.clone())),
        ("grid", None),
        ("intent", Some(Value::Null)),
    ] {
        let response = server
            .page
            .post("api/comment", explanation_comment_request(block, selection))
            .await;
        assert_eq!(response.status(), 200);
    }
    let expected = [text_selection, code_selection, Value::Null, Value::Null];
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    for (comment, selection) in review["comments"].as_array().unwrap().iter().zip(&expected) {
        assert_eq!(comment["explanation"]["selection"], *selection);
    }
    assert!(
        server
            .page
            .post("api/hand", json!({}))
            .await
            .status()
            .is_success()
    );
    let (status, answer) = server.wait(Some(1000)).await;
    assert_eq!(status, 200);
    let handed = handed_events(&answer)[0]["comments"].as_array().unwrap();
    for (entry, selection) in handed.iter().zip(&expected) {
        let explanation = entry["comment"]["explanation"].as_object().unwrap();
        assert_eq!(explanation["selection"], *selection);
    }
    assert!(
        server
            .page
            .post("api/submit", json!({"verdict":"changes_requested"}))
            .await
            .status()
            .is_success()
    );
    let result = server.page.finish().await;
    for (comment, selection) in result["comments"].as_array().unwrap().iter().zip(&expected) {
        let explanation = comment["explanation"].as_object().unwrap();
        assert_eq!(explanation["selection"], *selection);
    }
}

#[tokio::test]
async fn explanation_selections_accept_the_byte_and_occurrence_boundaries() {
    let server = server_with_selectable_explanation().await;
    let boundary = "あ".repeat(21_845) + "x";
    assert_eq!(boundary.len(), 65_536);
    for selection in [
        json!({"text":"x","occurrence":0}),
        json!({"text":boundary,"occurrence":9_007_199_254_740_991_u64}),
    ] {
        let response = server
            .page
            .post(
                "api/comment",
                explanation_comment_request("intent", Some(selection.clone())),
            )
            .await;
        assert_eq!(response.status(), 200, "{selection}");
        let comment: Value = response.json().await.unwrap();
        assert_eq!(comment["explanation"]["selection"], selection);
    }
}

#[tokio::test]
async fn invalid_explanation_selections_are_refused_with_a_reason_and_change_nothing() {
    let server = server_with_selectable_explanation().await;
    assert_eq!(
        server
            .page
            .post(
                "api/comment",
                explanation_comment_request(
                    "intent",
                    Some(json!({"text":"intent","occurrence":0}))
                ),
            )
            .await
            .status(),
        200
    );
    let cases = [
        ("grid", json!({"text":"A","occurrence":0})),
        ("intent", json!({"text":"","occurrence":0})),
        ("intent", json!({"text":"あ".repeat(21_846),"occurrence":0})),
        ("intent", json!({"text":"x","occurrence":-1})),
        (
            "intent",
            serde_json::from_str(r#"{"text":"x","occurrence":1.0}"#).unwrap(),
        ),
        (
            "intent",
            json!({"text":"x","occurrence":9_007_199_254_740_992_u64}),
        ),
        ("intent", json!({"text":"x","occurrence":"0"})),
        ("intent", json!({"text":1,"occurrence":0})),
        ("intent", json!({"text":"x"})),
        ("intent", json!({"occurrence":0})),
        ("intent", json!({"text":"x","occurrence":0,"extra":true})),
        ("intent", json!("intent")),
        ("intent", json!([])),
    ];
    for (block, selection) in cases {
        let before: Value = server.page.get("api/review").await.json().await.unwrap();
        let saved = server.sink.last_state();
        let response = server
            .page
            .post(
                "api/comment",
                explanation_comment_request(block, Some(selection.clone())),
            )
            .await;
        assert_eq!(response.status(), 400, "{selection}");
        let reason: Value = response.json().await.unwrap();
        assert!(reason.to_string().contains("selection"), "{reason}");
        let after: Value = server.page.get("api/review").await.json().await.unwrap();
        assert_eq!(before["comments"], after["comments"], "{selection}");
        let after_saved = server.sink.last_state();
        assert_eq!(saved.comments, after_saved.comments);
        assert_eq!(saved.channel, after_saved.channel);
        assert_eq!(saved.last_comment, after_saved.last_comment);
        assert_eq!(saved.last_seq, after_saved.last_seq);
    }
}

#[tokio::test]
async fn editing_an_explanation_comment_keeps_its_selection() {
    let server = server_with_selectable_explanation().await;
    let selection = json!({"text":"intent","occurrence":1});
    let comment: Value = server
        .page
        .post(
            "api/comment",
            explanation_comment_request("intent", Some(selection.clone())),
        )
        .await
        .json()
        .await
        .unwrap();
    let edited = server
        .page
        .post(
            "api/comment",
            json!({"op":"edit","id":comment["id"],"body":"changed"}),
        )
        .await;
    assert_eq!(edited.status(), 200);
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["comments"][0]["body"], "changed");
    assert_eq!(review["comments"][0]["explanation"]["selection"], selection);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn initial_explanation_accepts_opposite_units_while_the_full_build_is_gated() {
    for startup in [GroupBy::File, GroupBy::Commit] {
        let (repo, base) = range_repo();
        let group = if startup == GroupBy::File {
            repo.git(&["rev-parse", "HEAD~1"]).trim().to_string()
        } else {
            "all".into()
        };
        let unit = if startup == GroupBy::File {
            "commit"
        } else {
            "file"
        };
        let mut sections = explanation_sections();
        sections[0]["blocks"][0]["references"] = json!([{"type":"code","unit":unit,"group_id":group,"path":"b.txt","side":"new","start_line":null,"end_line":null}]);
        let sink = Arc::new(RecordingSink::default());
        let explanation = serde_json::from_value(json!({"sections":sections})).unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let (release, gate) = std::sync::mpsc::channel();
        let source = Arc::new(GatedSource {
            inner: Box::new(CountingSource {
                inner: Box::new(range_source(&repo, &base, "HEAD", startup)),
                reads: reads.clone(),
            }),
            gated: startup.other(),
            gate: Mutex::new(gate),
        });
        let server = AgentServer::launch(
            Launch {
                initial_explanation: Some(explanation),
                ..Launch::new(source)
            },
            sink,
        )
        .await;
        let first = tokio::time::timeout(WAIT_LIMIT, server.page.get("api/review"))
            .await
            .expect("initial response waited for all opposite metadata");
        assert_eq!(first.status(), 200);
        let first: Value = first.json().await.unwrap();
        assert_eq!(first["explanation"]["current_revision"], 1);
        assert_eq!(first["units"][1]["state"], "building");
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(server.agent_post("context", json!({})).await.status(), 409);
        let update = json!({"expected_revision":1,"base_display_version":first["diff_display_version"],"sections":sections});
        assert_eq!(server.agent_post("explain", update).await.status(), 200);
        release.send(()).unwrap();
        server.page.task.abort();
    }
}

#[tokio::test]
async fn explanation_update_keeps_hand_receipts_agent_lines_and_unreceived_events() {
    let server = AgentServer::start().await;
    server.wait(Some(0)).await;
    server.page.add_comment_with_body(11, "first").await;
    assert_eq!(server.page.post("api/hand", json!({})).await.status(), 200);
    let (_, first) = server.wait(Some(0)).await;
    let through = first["through"].clone();
    let before: Value = server.page.get("api/review").await.json().await.unwrap();
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(server.agent_post("explain",json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()})).await.status(),200);
    let after: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(before["agent"], after["agent"]);
    server.page.add_comment_with_body(12, "second").await;
    assert_eq!(server.page.post("api/hand", json!({})).await.status(), 200);
    assert_eq!(
        server
            .agent_post("received", json!({"through":through}))
            .await
            .status(),
        200
    );
    let (_, pending) = server.wait(Some(0)).await;
    assert_eq!(
        handed_events(&pending)[0]["comments"][0]["comment"]["body"],
        "second"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explanation_updates_are_atomic_under_concurrency_and_storage_failure() {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let update = json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()});
    let (a, b) = tokio::join!(
        server.agent_post("explain", update.clone()),
        server.agent_post("explain", update)
    );
    let mut status = [a.status().as_u16(), b.status().as_u16()];
    status.sort();
    assert_eq!(status, [200, 409]);
    let before: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let accepted_before = server.sink.accepted.lock().unwrap().last().unwrap().clone();
    server.sink.fail.store(true, Ordering::SeqCst);
    assert_eq!(server.agent_post("explain",json!({"expected_revision":1,"base_display_version":context["diff_display_version"],"sections":explanation_sections()})).await.status(),500);
    let after: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        server.sink.accepted.lock().unwrap().last().unwrap(),
        &accepted_before
    );
}

#[tokio::test]
async fn the_page_explanation_api_needs_the_page_token() {
    let server = AgentServer::start().await;
    let wrong_page = http()
        .post(format!(
            "{}api/explanation",
            server.page.url.replace("test-token", "wrong")
        ))
        .header("Origin", &server.page.origin)
        .json(&json!({"op":"hold","screen":"reader","revision":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_page.status(), 404);
}

#[tokio::test]
async fn explanation_accepts_known_old_code_bases_but_not_unknown_tokens_or_current_unknown_targets()
 {
    let server = AgentServer::start().await;
    let before: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let current: Value = server
        .page
        .get("api/review?refresh=1")
        .await
        .json()
        .await
        .unwrap();
    let mut sections = explanation_sections();
    sections[0]["blocks"][0]["references"] = json!([{"type":"code","unit":null,"group_id":"missing","path":"not-present.txt","side":"new","start_line":null,"end_line":null}]);
    let mut update = json!({"expected_revision":0,"base_display_version":before["diff_display_version"],"sections":sections});
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        200
    );
    update["expected_revision"] = json!(1);
    update["base_display_version"] = current["diff_display_version"].clone();
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        400
    );
    for token in ["another-review:1", "01TESTREVIEW:999", "01TESTREVIEW:0"] {
        update["base_display_version"] = json!(token);
        assert_eq!(
            server.agent_post("explain", update.clone()).await.status(),
            400
        );
    }
    assert_eq!(
        server
            .agent_post("context", json!({}))
            .await
            .json::<Value>()
            .await
            .unwrap()["explanation_revision"],
        1
    );
}

#[tokio::test]
async fn held_explanation_versions_are_not_discarded_to_accept_an_over_capacity_update() {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    for expected in 0..128 {
        assert_eq!(server.agent_post("explain",json!({"expected_revision":expected,"base_display_version":context["diff_display_version"],"sections":explanation_sections()})).await.status(),200);
        assert_eq!(
            server
                .page
                .post(
                    "api/explanation",
                    json!({"op":"hold","screen":"reader","revision":expected+1})
                )
                .await
                .status(),
            200
        );
    }
    let before = server.sink.last_state();
    let update = json!({"expected_revision":128,"base_display_version":context["diff_display_version"],"sections":explanation_sections()});
    assert_eq!(
        server.agent_post("explain", update.clone()).await.status(),
        413
    );
    assert_eq!(server.sink.last_state(), before);
    let refreshed: Value = server
        .page
        .get("api/review?refresh=1")
        .await
        .json()
        .await
        .unwrap();
    assert_ne!(
        refreshed["diff_display_version"],
        context["diff_display_version"]
    );
    assert_eq!(server.sink.last_state().explanation, before.explanation);
    assert_eq!(
        server.page.get("api/explanation?revision=1").await.status(),
        200
    );
    assert_eq!(
        server
            .page
            .post(
                "api/explanation",
                json!({"op":"release","screen":"reader","revision":1})
            )
            .await
            .status(),
        200
    );
    assert_eq!(server.agent_post("explain", update).await.status(), 200);
    assert_eq!(
        server.page.get("api/explanation?revision=1").await.status(),
        404
    );
}

#[tokio::test]
async fn code_imports_without_explanation_continue_past_4096_generations() {
    let server = AgentServer::start().await;
    for generation in 2..=4098 {
        let response = server.page.get("api/review?refresh=1").await;
        assert_eq!(response.status(), 200);
        let review: Value = response.json().await.unwrap();
        assert_eq!(
            review["diff_display_version"],
            format!("01TESTREVIEW:{generation}")
        );
        assert!(review["explanation"].is_null());
    }
    let saved = server.sink.last_state();
    assert_eq!(saved.display.unwrap().issued, 4098);
    assert!(saved.explanation.is_none());
}

#[tokio::test]
async fn explanation_and_parts_inputs_refuse_duplicate_keys_before_mutating_state() {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let explanation = format!(
        "{{\"expected_revision\":0,\"expected_revision\":0,\"base_display_version\":{},\"sections\":{}}}",
        context["diff_display_version"],
        explanation_sections()
    );
    for (action, body) in [
        ("explain", explanation.as_str()),
        (
            "reply",
            r#"{"writes":[{"type":"message","body":"first","body":"second"}]}"#,
        ),
        (
            "reply",
            r#"{"writes":[{"type":"message","parts":[{"type":"text","body":"one","body":"two"}]}]}"#,
        ),
    ] {
        let response = http()
            .post(server.agent_url(&server.agent_token, action))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert!(review["explanation"].is_null());
    assert!(review["messages"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn submit_claiming_first_refuses_the_explanation_while_result_content_is_gated() {
    let (release, wait) = std::sync::mpsc::channel();
    let gate = Arc::new(GenerationGate {
        entered: tokio::sync::Notify::new(),
        release: Mutex::new(wait),
    });
    let source = Arc::new(GatedContentSource {
        inner: Arc::new(FakeSource::new()),
        gate: gate.clone(),
        blocked: AtomicBool::new(false),
    });
    let server = AgentServer::start_with(source.clone()).await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    server.page.add_comment_with_body(11, "question").await;
    source.blocked.store(true, Ordering::SeqCst);
    let url = server.page.url.clone();
    let origin = server.page.origin.clone();
    let mut submit = tokio::spawn(async move {
        post_json(&url, &origin, "api/submit", json!({"verdict":"approved"})).await
    });
    until_reached(
        "the submit reading the gated content",
        gate.entered.notified(),
        &mut submit,
    )
    .await;
    assert_eq!(server.agent_post("explain",json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()})).await.status(),409);
    release.send(()).unwrap();
    let response = submit.await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.json::<Value>().await.unwrap()["result"]["explanation"].is_null());
    server.page.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn submit_started_after_the_explanation_save_began_includes_the_explanation() {
    let sink = Arc::new(RecordingSink::default());
    let server = AgentServer::start_with_sink(Arc::new(FakeSource::new()), sink.clone()).await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let (release, wait) = std::sync::mpsc::channel();
    let gate = Arc::new(GenerationGate {
        entered: tokio::sync::Notify::new(),
        release: Mutex::new(wait),
    });
    *sink.explanation_gate.lock().unwrap() = Some(gate.clone());
    let url = server.agent_url(&server.agent_token, "explain");
    let update = json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()});
    let mut explain =
        tokio::spawn(async move { http().post(url).json(&update).send().await.unwrap() });
    until_reached(
        "the explanation save starting",
        gate.entered.notified(),
        &mut explain,
    )
    .await;
    // 説明の保存が始まってから submit を出す。submit が保存の門に着いて待っていることは外から
    // 観測できず、門はすぐ開けるので、ほとんどの実行で submit は保存が終わった後に届く。保存と
    // submit の競合の検査ではない。逆の順（submit が先に請求する）は
    // submit_claiming_first_refuses_the_explanation_while_result_content_is_gated が確かめる。
    let url = server.page.url.clone();
    let origin = server.page.origin.clone();
    let submit = tokio::spawn(async move {
        post_json(&url, &origin, "api/submit", json!({"verdict":"approved"})).await
    });
    release.send(()).unwrap();
    assert_eq!(explain.await.unwrap().status(), 200);
    let result: Value = submit.await.unwrap().json().await.unwrap();
    assert_eq!(result["result"]["explanation"]["current_revision"], 1);
    server.page.finish().await;
}

#[tokio::test]
async fn explanation_notifications_do_not_emit_code_updates_or_conversation_events() {
    let server = AgentServer::start().await;
    let context: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    let mut sse = server.page.get("api/events").await;
    assert_eq!(server.agent_post("explain",json!({"expected_revision":0,"base_display_version":context["diff_display_version"],"sections":explanation_sections()})).await.status(),200);
    let notification = next_sse_event(&mut sse, "explanation").await;
    assert_eq!(notification["revision"], 1);
    assert_eq!(
        notification["base_display_version"],
        context["diff_display_version"]
    );
    server.agent_post("context", json!({})).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), sse.chunk())
            .await
            .is_err()
    );
    assert!(server.sink.last_state().channel.events.is_empty());
}

#[tokio::test]
async fn old_code_bases_still_require_the_reference_unit_shape_of_this_review() {
    let server = AgentServer::start().await;
    let old: Value = server
        .agent_post("context", json!({}))
        .await
        .json()
        .await
        .unwrap();
    server.page.get("api/review?refresh=1").await;
    let mut sections = explanation_sections();
    sections[0]["blocks"][0]["references"] = json!([{"type":"code","unit":"file","group_id":"all","path":"old.txt","side":"new","start_line":null,"end_line":null}]);
    assert_eq!(server.agent_post("explain",json!({"expected_revision":0,"base_display_version":old["diff_display_version"],"sections":sections})).await.status(),400);
}
