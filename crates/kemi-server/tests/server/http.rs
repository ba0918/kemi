use super::*;

#[test]
fn session_host_uses_the_share_address_only_for_a_wildcard_bind() {
    let concrete = Ipv4Addr::new(192, 168, 1, 5);
    let shared = Ipv4Addr::new(192, 0, 2, 10);

    assert_eq!(session_host(concrete, None), concrete);
    assert_eq!(session_host(concrete, Some(shared)), concrete);
    assert_eq!(
        session_host(Ipv4Addr::UNSPECIFIED, None),
        Ipv4Addr::LOCALHOST
    );
    assert_eq!(session_host(Ipv4Addr::UNSPECIFIED, Some(shared)), shared);
}

#[tokio::test]
async fn session_url_uses_the_given_host_and_the_listener_port() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let url = session_url(&listener, "tok", Ipv4Addr::new(192, 0, 2, 10)).unwrap();

    assert_eq!(url, format!("http://192.0.2.10:{port}/s/tok/"));
}

#[tokio::test]
async fn review_api_returns_groups_and_files() {
    let server = TestServer::start().await;
    let response = server.get("api/review").await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();

    assert_eq!(body["kemi"], 1);
    assert_eq!(body["title"], "テストのレビュー");
    assert_eq!(body["groups"][0]["id"], "g1");
    assert_eq!(body["groups"][0]["watch"], "ここを見て");
    assert_eq!(body["groups"][0]["files"][0]["path"], "src/a.rs");
    assert_eq!(body["groups"][0]["files"][0]["seen"], false);
    assert_eq!(body["groups"][0]["files"][2]["path"], "assets/logo.png");
    assert_eq!(body["groups"][0]["files"][2]["binary"], true);
    assert_eq!(body["approval"][0]["identity"], "sha256:abc");
}

#[tokio::test]
async fn content_reads_zero_during_startup_and_review() {
    let source = Arc::new(FakeSource::new());
    let server = TestServer::start_source(source.clone()).await;
    let response = server.get("api/review").await;
    assert_eq!(response.status(), 200);
    let _ = response.text().await.unwrap();

    assert_eq!(source.reads(), 0);
}

#[tokio::test]
async fn content_reads_happen_on_file_request() {
    let source = Arc::new(FakeSource::new());
    let server = TestServer::start_source(source.clone()).await;
    let response = server.get("api/file/f1").await;
    assert_eq!(response.status(), 200);
    assert_eq!(source.reads(), 1);
}

#[tokio::test]
async fn token_required_rejects_unknown_token() {
    let server = TestServer::start().await;
    let response = http()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/api/review",
            server.port
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn cross_origin_rejected_on_comment_post() {
    let server = TestServer::start().await;
    let response = server
        .post_with_origin(
            "api/state",
            json!({"file_id": "f1", "seen": true}),
            &format!("http://localhost:{}", server.port),
        )
        .await;

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn post_without_origin_rejected() {
    let server = TestServer::start().await;
    let response = http()
        .post(format!("{}api/state", server.url))
        .json(&json!({"file_id": "f1"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_mismatch_rejected() {
    let server = TestServer::start().await;
    let host = format!("192.0.2.10:{}", server.port);

    let status = state_post(&server, &host, &format!("http://{host}"), "{}");

    assert_eq!(status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_from_the_shared_address_accepted() {
    let shared = Ipv4Addr::new(192, 0, 2, 10);
    let server = TestServer::start_bound(Ipv4Addr::UNSPECIFIED, Some(shared)).await;
    let host = format!("{shared}:{}", server.port);

    let status = state_post(
        &server,
        &host,
        &format!("http://{host}"),
        r#"{"file_id":"f1","seen":true}"#,
    );

    assert_eq!(status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_from_an_unlisted_address_rejected() {
    let shared = Ipv4Addr::new(192, 0, 2, 10);
    let other = Ipv4Addr::new(198, 51, 100, 9);
    let server = TestServer::start_bound(Ipv4Addr::UNSPECIFIED, Some(shared)).await;
    let host = format!("{other}:{}", server.port);

    let status = state_post(&server, &host, &format!("http://{host}"), "{}");

    assert_eq!(status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_with_wildcard_host_rejected() {
    let shared = Ipv4Addr::new(192, 0, 2, 10);
    let server = TestServer::start_bound(Ipv4Addr::UNSPECIFIED, Some(shared)).await;
    let host = format!("0.0.0.0:{}", server.port);

    let status = state_post(&server, &host, &format!("http://{host}"), "{}");

    assert_eq!(status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_from_a_lan_address_rejected_without_a_share_address() {
    let server = TestServer::start_bound(Ipv4Addr::UNSPECIFIED, None).await;
    let host = format!("192.0.2.10:{}", server.port);

    let status = state_post(&server, &host, &format!("http://{host}"), "{}");

    assert_eq!(status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_with_a_wrong_port_rejected() {
    let shared = Ipv4Addr::new(192, 0, 2, 10);
    let server = TestServer::start_bound(Ipv4Addr::UNSPECIFIED, Some(shared)).await;
    let host = format!("{shared}:{}", server.port.wrapping_add(1));

    let status = state_post(&server, &host, &format!("http://{host}"), "{}");

    assert_eq!(status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_with_a_mismatched_origin_rejected() {
    let shared = Ipv4Addr::new(192, 0, 2, 10);
    let server = TestServer::start_bound(Ipv4Addr::UNSPECIFIED, Some(shared)).await;

    let status = state_post(
        &server,
        &format!("{shared}:{}", server.port),
        &server.origin,
        "{}",
    );

    assert_eq!(status, 403);
}

#[tokio::test]
async fn seen_state_kept_on_server() {
    let server = TestServer::start().await;
    let response = server
        .post("api/state", json!({"file_id": "f1", "seen": true}))
        .await;
    assert_eq!(response.status(), 200);

    let review: Value = server.get("api/review").await.json().await.unwrap();
    assert_eq!(review["groups"][0]["files"][0]["seen"], true);
    assert_eq!(review["groups"][0]["files"][1]["seen"], false);
}

#[tokio::test]
async fn an_agent_request_with_an_origin_is_forbidden() {
    let server = AgentServer::start().await;

    let response = http()
        .post(server.agent_url(&server.agent_token, "reply"))
        .header("Origin", "http://127.0.0.1:3000")
        .json(&json!({ "writes": [{ "type": "message", "body": "x" }] }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    assert_nothing_written_without_comments(&server).await;
}

#[tokio::test]
async fn an_agent_request_with_a_wrong_token_is_refused() {
    let server = AgentServer::start().await;

    let response = http()
        .post(server.agent_url("test-token", "reply"))
        .json(&json!({ "writes": [{ "type": "message", "body": "x" }] }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    assert_nothing_written_without_comments(&server).await;
}

#[tokio::test]
async fn the_agent_listener_stays_on_loopback_when_the_page_listens_on_all_interfaces() {
    let listener = bind_agent_listener().await.unwrap();

    assert_eq!(
        listener.local_addr().unwrap().ip(),
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
    );
    let server = AgentServer::launch(
        Launch {
            bind: Ipv4Addr::UNSPECIFIED,
            ..Launch::new(Arc::new(FakeSource::new()))
        },
        Arc::new(RecordingSink::default()),
    )
    .await;
    let (status, _) = server.wait(Some(10)).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn agent_token_rejection_waits_for_the_request_body_before_answering() {
    let server = AgentServer::start().await;

    for action in ["wait", "reply", "received", "context", "explain"] {
        let (status, answer) =
            agent_response_after_delayed_body(&server, "wrong-token", action, "").await;

        assert_eq!(status, 403, "{action}: {answer}");
        assert!(answer["error"].is_string(), "{action}: {answer}");
    }
}
