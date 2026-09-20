//! HTTP / SSE サーバの契約テスト（R-SERVE, R-SUBMIT, R-COMMENT）。

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use kemi_core::domain::content::AUTO_MAX_BYTES;
use kemi_core::domain::review::{Approval, FileEntry, Group, ReviewMeta, Side, Status};
use kemi_core::source::{FileContent, ReviewSource, SourceError};
use kemi_server::{serve, session_host, session_url, Asset, Assets, ServeOutcome, ServeParams};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct FakeSource {
    meta: ReviewMeta,
    contents: HashMap<String, FileContent>,
    reads: AtomicUsize,
    content_delay_ms: AtomicUsize,
    content_started: tokio::sync::Notify,
    /// リポジトリを読める入力（git のモード）を装う。相対パス画像はこの表から返す。
    repository: Option<HashMap<String, Vec<u8>>>,
}

impl FakeSource {
    fn new() -> Self {
        let (old, new) = text_pair();
        let mut contents = HashMap::new();
        contents.insert(
            "f1".to_string(),
            FileContent {
                old: Some(old.into_bytes()),
                new: Some(new.into_bytes()),
            },
        );
        contents.insert(
            "f2".to_string(),
            FileContent {
                old: Some(vec![0xff, 0x00, 0x01]),
                new: Some(vec![0xff, 0x00, 0x02]),
            },
        );
        let large = "line\n".repeat(10_001);
        contents.insert(
            "f3".to_string(),
            FileContent {
                old: Some(large.clone().into_bytes()),
                new: Some(large.into_bytes()),
            },
        );
        contents.insert(
            "f4".to_string(),
            FileContent {
                old: None,
                new: Some("para\n".repeat(10_001).into_bytes()),
            },
        );
        contents.insert(
            "f5".to_string(),
            FileContent {
                old: Some(b"# Guide\n\nold words here\n".to_vec()),
                new: Some(
                    b"# Guide\n\nnew words here\n\n```rust\nlet a = 1;\n```\n\n![shot](img/shot.png)\n"
                        .to_vec(),
                ),
            },
        );
        contents.insert(
            "f6".to_string(),
            FileContent {
                old: None,
                new: Some(vec![0x89, b'P', b'N', b'G', 0x00]),
            },
        );
        contents.insert(
            "f7".to_string(),
            FileContent {
                old: None,
                new: Some(format!("{}x\n", "> ".repeat(101)).into_bytes()),
            },
        );
        contents.insert(
            "f8".to_string(),
            FileContent {
                old: Some(b"name,note\nx,\"fine\"\n".to_vec()),
                new: Some(b"name,note\nx,\"fine\"\ny,\"broken\n".to_vec()),
            },
        );
        contents.insert(
            "f9".to_string(),
            FileContent {
                old: Some(b"name\tnote\nx\tone\n".to_vec()),
                new: Some(b"name\tnote\nx\ttwo\n".to_vec()),
            },
        );
        contents.insert(
            "f11".to_string(),
            FileContent {
                old: Some(
                    b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"1\"/></svg>\n".to_vec(),
                ),
                new: Some(
                    b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"2\"/></svg>\n".to_vec(),
                ),
            },
        );
        contents.insert(
            "f10".to_string(),
            FileContent {
                old: None,
                new: Some(vec![0x89; 6_000_000]),
            },
        );
        contents.insert(
            "f12".to_string(),
            FileContent {
                old: Some(vec![0x89, b'P', b'N', b'G', 0x00, 0x01]),
                new: Some(vec![0x89, b'P', b'N', b'G', 0x00, 0x01]),
            },
        );
        FakeSource {
            meta: ReviewMeta {
                title: "テストのレビュー".to_string(),
                subtitle: String::new(),
                meta: Value::Null,
                groups: vec![Group {
                    id: "g1".to_string(),
                    title: "最初の変更".to_string(),
                    why: String::new(),
                    watch: "ここを見て".to_string(),
                    files: vec![
                        file_entry("f1", "src/a.rs"),
                        file_entry("f3", "src/large.rs"),
                        FileEntry {
                            id: "f2".to_string(),
                            group_id: "g1".to_string(),
                            path: "assets/logo.png".to_string(),
                            old_path: None,
                            status: Status::Modify,
                            add: 0,
                            del: 0,
                            binary: true,
                            old_size: 10,
                            new_size: 20,
                            focus: true,
                            note: "重点".to_string(),
                            noise: true,
                            content_skipped: false,
                        },
                        FileEntry {
                            status: Status::Add,
                            ..file_entry("f4", "docs/big.md")
                        },
                        file_entry("f5", "docs/guide.md"),
                        FileEntry {
                            status: Status::Add,
                            binary: true,
                            new_size: 5,
                            ..file_entry("f6", "docs/img/shot.png")
                        },
                        FileEntry {
                            status: Status::Add,
                            ..file_entry("f7", "docs/deep.md")
                        },
                        file_entry("f8", "data/rows.csv"),
                        file_entry("f9", "data/rows.tsv"),
                        FileEntry {
                            status: Status::Add,
                            binary: true,
                            new_size: 6_000_000,
                            ..file_entry("f10", "assets/huge.png")
                        },
                        file_entry("f11", "art/icon.svg"),
                        FileEntry {
                            status: Status::Rename,
                            old_path: Some("art/old-name.png".to_string()),
                            binary: true,
                            old_size: 6,
                            new_size: 6,
                            ..file_entry("f12", "art/new-name.png")
                        },
                    ],
                }],
                approval: vec![Approval {
                    path: "src/a.rs".to_string(),
                    identity: "sha256:abc".to_string(),
                }],
            },
            contents,
            reads: AtomicUsize::new(0),
            content_delay_ms: AtomicUsize::new(0),
            content_started: tokio::sync::Notify::new(),
            repository: None,
        }
    }

    fn with_repository(mut self, files: &[(&str, &[u8])]) -> Self {
        self.repository = Some(
            files
                .iter()
                .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
                .collect(),
        );
        self
    }

    fn with_file(mut self, entry: FileEntry, content: FileContent) -> Self {
        self.contents.insert(entry.id.clone(), content);
        self.meta.groups[0].files.push(entry);
        self
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    fn set_content_delay(&self, delay: Duration) {
        self.content_delay_ms
            .store(delay.as_millis() as usize, Ordering::SeqCst);
    }
}

fn file_entry(id: &str, path: &str) -> FileEntry {
    FileEntry {
        id: id.to_string(),
        group_id: "g1".to_string(),
        path: path.to_string(),
        old_path: None,
        status: Status::Modify,
        add: 1,
        del: 1,
        binary: false,
        old_size: 0,
        new_size: 0,
        focus: false,
        note: String::new(),
        noise: false,
        content_skipped: false,
    }
}

fn text_pair() -> (String, String) {
    let mut old = String::new();
    let mut new = String::new();
    for index in 0..10 {
        old.push_str(&format!("top{index}\n"));
        new.push_str(&format!("top{index}\n"));
    }
    old.push_str("old\n");
    new.push_str("new\n");
    for index in 0..10 {
        old.push_str(&format!("bottom{index}\n"));
        new.push_str(&format!("bottom{index}\n"));
    }
    (old, new)
}

impl ReviewSource for FakeSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        Ok(self.meta.clone())
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let delay = self.content_delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            self.content_started.notify_one();
            std::thread::sleep(Duration::from_millis(delay as u64));
        }
        self.contents
            .get(file_id)
            .cloned()
            .ok_or_else(|| SourceError::UnknownFileId(file_id.to_string()))
    }

    fn reads_repository(&self) -> bool {
        self.repository.is_some()
    }

    fn repository_file(
        &self,
        _file_id: &str,
        _side: Side,
        path: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        Ok(self
            .repository
            .as_ref()
            .and_then(|files| files.get(path))
            .filter(|bytes| bytes.len() as u64 <= limit)
            .cloned())
    }
}

struct FakeAssets;

impl Assets for FakeAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        match path {
            "index.html" => Some(Asset {
                bytes: std::borrow::Cow::Borrowed(b"<html>kemi</html>"),
                mime: "text/html; charset=utf-8",
            }),
            "app.js" => Some(Asset {
                bytes: std::borrow::Cow::Borrowed(b"console.log('kemi')"),
                mime: "text/javascript; charset=utf-8",
            }),
            _ => None,
        }
    }
}

struct TestServer {
    url: String,
    origin: String,
    port: u16,
    source: Arc<FakeSource>,
    task: JoinHandle<Result<ServeOutcome, kemi_server::ServerError>>,
}

impl TestServer {
    async fn start() -> Self {
        TestServer::start_bound(Ipv4Addr::LOCALHOST, None).await
    }

    /// wildcard バインドや共有アドレスを再現する。url はテストが繋げる loopback のまま
    /// （表示 URL の組み立ては `session_url` / `session_host` を直接検証する）。
    async fn start_bound(bind: Ipv4Addr, share_address: Option<Ipv4Addr>) -> Self {
        TestServer::start_source(Arc::new(FakeSource::new()), bind, share_address).await
    }

    async fn start_source(
        source: Arc<FakeSource>,
        bind: Ipv4Addr,
        share_address: Option<Ipv4Addr>,
    ) -> Self {
        let listener = TcpListener::bind((bind, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = session_url(&listener, "test-token", Ipv4Addr::LOCALHOST).unwrap();
        let origin = url.split("/s/").next().unwrap().to_string();
        let params = ServeParams {
            source: source.clone(),
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
            results: None,
            session: None,
            share_address,
        };
        let task = tokio::spawn(serve(listener, params));
        TestServer {
            url,
            origin,
            port,
            source,
            task,
        }
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        reqwest::Client::new()
            .get(format!("{}{path}", self.url))
            .send()
            .await
            .unwrap()
    }

    async fn post(&self, path: &str, body: Value) -> reqwest::Response {
        self.post_with_origin(path, body, &self.origin).await
    }

    async fn post_with_origin(&self, path: &str, body: Value, origin: &str) -> reqwest::Response {
        post_json(&self.url, origin, path, body).await
    }

    async fn finish(self) -> Value {
        match tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .expect("server did not stop")
            .expect("server task panicked")
            .expect("server returned error")
        {
            ServeOutcome::Submitted(document) => document,
        }
    }
}

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
    let server = TestServer::start().await;
    let response = server.get("api/review").await;
    assert_eq!(response.status(), 200);
    let _ = response.text().await.unwrap();

    assert_eq!(server.source.reads(), 0);
}

#[tokio::test]
async fn content_reads_happen_on_file_request() {
    let server = TestServer::start().await;
    let response = server.get("api/file/f1").await;
    assert_eq!(response.status(), 200);
    assert_eq!(server.source.reads(), 1);
}

#[tokio::test]
async fn token_required_rejects_unknown_token() {
    let server = TestServer::start().await;
    let response = reqwest::Client::new()
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
    let response = reqwest::Client::new()
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
async fn two_clients_share_comments_and_seen_without_an_author_field() {
    let server = TestServer::start().await;
    let first = reqwest::Client::new();
    let second = reqwest::Client::new();

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
    assert_eq!(comment["replies"], json!(["2台目から"]));
    assert!(comment.get("author").is_none(), "{comment}");
}

#[tokio::test]
async fn file_rows_collapse_and_expand() {
    let server = TestServer::start().await;
    let body: Value = server.get("api/file/f1").await.json().await.unwrap();

    assert_eq!(body["binary"], false);
    assert_eq!(body["rows"][0]["kind"], "skip");
    assert_eq!(body["rows"][0]["count"], 7);
    assert_eq!(body["rows"][0]["from"], 0);
    assert_eq!(body["rows"][0]["to"], 7);
    assert_eq!(body["old_total"], 21);
    assert_eq!(body["new_total"], 21);

    let expanded: Value = server
        .get("api/file/f1?from=0&to=7")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(expanded["rows"].as_array().unwrap().len(), 7);
    assert_eq!(expanded["rows"][0]["kind"], "equal");
    assert_eq!(expanded["rows"][0]["old"]["text"], "top0");
    assert_eq!(expanded["next"], Value::Null);
}

#[tokio::test]
async fn binary_file_has_no_rows() {
    let server = TestServer::start().await;
    let body: Value = server.get("api/file/f2").await.json().await.unwrap();

    assert_eq!(body["binary"], true);
    assert_eq!(body["old_size"], 10);
    assert_eq!(body["new_size"], 20);
    assert_eq!(body["rows"].as_array().unwrap().len(), 0);
    assert_eq!(server.source.reads(), 0);
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

    assert_eq!(document["kemi"], 1);
    assert_eq!(document["title"], "テストのレビュー");
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
    assert_eq!(comments[0]["replies"], json!(["返信1"]));
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
    let first_task = tokio::spawn(async move {
        post_json(&url, &origin, "api/submit", json!({"verdict": "approved"})).await
    });

    server.source.content_started.notified().await;
    signal.send(()).unwrap();
    let second_status = raw.join().unwrap();

    let mut statuses = vec![first_task.await.unwrap().status().as_u16(), second_status];
    statuses.sort();
    assert_eq!(statuses, vec![200, 409]);
    let _ = server.finish().await;
}

/// テスト中のサーバへ、Host と Origin を指定して state の POST を送る。
fn state_post(server: &TestServer, host: &str, origin: &str, body: &str) -> u16 {
    raw_post_with_host(
        ("127.0.0.1", server.port),
        host,
        "/s/test-token/api/state",
        origin,
        body,
    )
}

fn raw_post(address: (&str, u16), path: &str, origin: &str, body: &str) -> u16 {
    let host = format!("{}:{}", address.0, address.1);
    raw_post_with_host(address, &host, path, origin, body)
}

fn raw_post_with_host(
    address: (&str, u16),
    host: &str,
    path: &str,
    origin: &str,
    body: &str,
) -> u16 {
    use std::io::{Read, Write};

    let mut stream = std::net::TcpStream::connect(address).unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status line");
    status
}

async fn post_json(url: &str, origin: &str, path: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{url}{path}"))
        .header("Origin", origin)
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn highlight_enabled_by_default_and_rows_carry_html() {
    let server = TestServer::start().await;
    let body: Value = server
        .get("api/file/f1?from=0&to=3")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["highlight"]["capable"], true);
    assert_eq!(body["highlight"]["enabled"], true);
    let html = body["rows"][0]["old"]["html"].as_str().unwrap();
    assert!(html.contains("<span"), "{html}");
}

#[tokio::test]
async fn highlight_can_be_turned_off() {
    let server = TestServer::start().await;
    let body: Value = server
        .get("api/file/f1?from=0&to=3&highlight=off")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["highlight"]["enabled"], false);
    assert!(body["rows"][0]["old"].get("html").is_none());
}

// ---- R-LIVE（監視）の結合テスト ----

use kemi_core::source::git::{GitMode, GitSource, GroupBy, UNTRACKED_LIMIT};
use kemi_core::source::manifest::ManifestSource;
use std::path::PathBuf;
use std::time::Instant;

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "kemi-live-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let repo = TempRepo { path };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.email", "kemi@example.com"]);
        repo.git(&["config", "user.name", "kemi"]);
        repo.git(&["config", "core.hooksPath", "/dev/null"]);
        repo
    }

    /// フィクスチャを作る git は、開発者の全体・システムの設定（署名の program、
    /// commit.gpgsign、diff.orderFile など）を読まない。読むと、同じテストが環境によって
    /// 失敗する。全体の設定の置き場は一時リポジトリの中の無いファイルにし、書かれても
    /// 外へ漏れない。
    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .env(
                "GIT_CONFIG_GLOBAL",
                self.path.join(".git").join("test-global-config"),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write(&self, relative: &str, content: &str) {
        std::fs::write(self.path.join(relative), content).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct LiveServer {
    port: u16,
    task: JoinHandle<Result<ServeOutcome, kemi_server::ServerError>>,
}

impl LiveServer {
    async fn start(source: Arc<dyn ReviewSource>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let params = ServeParams {
            source,
            assets: Arc::new(FakeAssets),
            token: "live-token".to_string(),
            results: None,
            session: None,
            share_address: None,
        };
        let task = tokio::spawn(serve(listener, params));
        // 監視スレッドがパスを登録するのを待つ。
        tokio::time::sleep(Duration::from_millis(400)).await;
        LiveServer { port, task }
    }

    fn stop(self) {
        self.task.abort();
    }
}

struct SseStream {
    stream: tokio::net::TcpStream,
    buffer: String,
}

impl SseStream {
    async fn connect(port: u16) -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = format!(
            "GET /s/live-token/api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut sse = SseStream {
            stream,
            buffer: String::new(),
        };
        let mut chunk = [0u8; 2048];
        let read = tokio::time::timeout(Duration::from_secs(2), sse.stream.read(&mut chunk))
            .await
            .unwrap()
            .unwrap();
        sse.buffer
            .push_str(&String::from_utf8_lossy(&chunk[..read]));
        sse
    }

    async fn next_update(&mut self, timeout: Duration) -> bool {
        use tokio::io::AsyncReadExt;

        let deadline = Instant::now() + timeout;
        loop {
            if self.drain_updates() > 0 {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let mut chunk = [0u8; 2048];
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0)) | Err(_) | Ok(Err(_)) => return false,
                Ok(Ok(read)) => self
                    .buffer
                    .push_str(&String::from_utf8_lossy(&chunk[..read])),
            }
        }
    }

    async fn count_updates(&mut self, duration: Duration) -> usize {
        use tokio::io::AsyncReadExt;

        let deadline = Instant::now() + duration;
        let mut count = self.drain_updates();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return count;
            }
            let mut chunk = [0u8; 2048];
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0)) | Err(_) | Ok(Err(_)) => return count,
                Ok(Ok(read)) => {
                    self.buffer
                        .push_str(&String::from_utf8_lossy(&chunk[..read]));
                    count += self.drain_updates();
                }
            }
        }
    }

    fn drain_updates(&mut self) -> usize {
        let needle = "event: update";
        let mut count = 0;
        while let Some(index) = self.buffer.find(needle) {
            count += 1;
            self.buffer.drain(..index + needle.len());
        }
        count
    }
}

#[tokio::test]
async fn live_worktree_change_sends_update() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    repo.write("a.txt", "initial change\n");
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = LiveServer::start(source).await;

    let mut sse = SseStream::connect(server.port).await;
    repo.write("a.txt", "two\n");
    assert!(
        sse.next_update(Duration::from_secs(2)).await,
        "no update event"
    );

    server.stop();
}

#[tokio::test]
async fn live_manifest_path_change_sends_update() {
    let dir = TempRepo::new();
    dir.write("old.txt", "one\n");
    dir.write("new.txt", "one\n");
    let json =
        r#"{"groups":[{"diffs":[{"path":"a.txt","old_path":"old.txt","new_path":"new.txt"}]}]}"#;
    let source = Arc::new(ManifestSource::from_json(json, &dir.path).unwrap());
    let server = LiveServer::start(source).await;

    let mut sse = SseStream::connect(server.port).await;
    dir.write("new.txt", "two\n");
    assert!(
        sse.next_update(Duration::from_secs(2)).await,
        "no update event"
    );

    server.stop();
}

#[tokio::test]
async fn live_ref_change_sends_update_and_other_git_writes_are_ignored() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.commit("base");
    let source = Arc::new(GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base,
            to: "HEAD".to_string(),
            group_by: GroupBy::Commit,
        },
    ));
    let server = LiveServer::start(source).await;

    let mut sse = SseStream::connect(server.port).await;

    // `--to`（HEAD）以外の ref、たとえば別ブランチの更新は監視対象ではない。
    repo.git(&["branch", "other"]);
    assert_eq!(
        sse.count_updates(Duration::from_millis(1000)).await,
        0,
        "another branch ref sent an event"
    );

    // `.git` の他の書き込みも監視対象ではない。
    std::fs::write(repo.path.join(".git/not-a-ref.txt"), "x").unwrap();
    assert_eq!(
        sse.count_updates(Duration::from_millis(1000)).await,
        0,
        "unrelated .git write sent an event"
    );

    // 対象の ref が進めば通知する。
    repo.write("b.txt", "two\n");
    repo.commit("second");
    assert!(
        sse.next_update(Duration::from_secs(2)).await,
        "no update event for the new commit"
    );

    server.stop();
}

#[tokio::test]
async fn live_worktree_read_does_not_send_update() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    repo.write("a.txt", "initial change\n");
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = LiveServer::start(source).await;

    let mut sse = SseStream::connect(server.port).await;
    // api/review はファイルを読む。読み取り（Access）は更新ではない。
    let response = reqwest::get(format!(
        "http://127.0.0.1:{}/s/live-token/api/review",
        server.port
    ))
    .await
    .unwrap();
    assert_eq!(response.status(), 200);
    let _ = response.text().await.unwrap();

    assert!(
        !sse.next_update(Duration::from_millis(700)).await,
        "read-only access must not send an update"
    );
    server.stop();
}

#[tokio::test]
async fn highlight_cap_can_be_overridden_for_one_file() {
    let server = TestServer::start().await;

    let capped: Value = server.get("api/file/f3").await.json().await.unwrap();
    assert_eq!(capped["highlight"]["capable"], false);
    assert_eq!(capped["highlight"]["enabled"], false);

    let forced: Value = server
        .get("api/file/f3?highlight=on&from=0&to=2")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(forced["highlight"]["capable"], false);
    assert_eq!(forced["highlight"]["enabled"], true);
    assert!(forced["rows"][0]["old"]["html"].is_string());
}

// ---- R-UNIT（2 つのグループ単位）と R-ORIGIN の取得 ----

use kemi_core::domain::focus::FocusTargets;
use kemi_core::source::{FileOrigin, FocusSource};

impl LiveServer {
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/s/live-token/", self.port)
    }

    async fn get_json(&self, path: &str) -> Value {
        let response = reqwest::get(format!("{}{path}", self.url())).await.unwrap();
        assert_eq!(response.status(), 200, "GET {path}");
        response.json().await.unwrap()
    }

    async fn post(&self, path: &str, body: Value) -> reqwest::Response {
        post_json(
            &self.url(),
            &format!("http://127.0.0.1:{}", self.port),
            path,
            body,
        )
        .await
    }

    /// もう片方の単位が `state` になるまで api/review を読み直す。
    async fn wait_unit(&self, state: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let review = self.get_json("api/review").await;
            if review["units"][1]["state"] == state {
                return review;
            }
            assert!(
                Instant::now() < deadline,
                "unit never became {state}: {}",
                review["units"]
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn finish(self) -> Value {
        match tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .expect("server did not stop")
            .expect("server task panicked")
            .expect("server returned error")
        {
            ServeOutcome::Submitted(document) => document,
        }
    }
}

/// 3 コミットの範囲: base → 「feat: two」（a.txt と b.txt）→ 「fix: three」（a.txt）。
fn range_repo() -> (TempRepo, String) {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.commit("base");
    repo.write("a.txt", "two\n");
    repo.write("b.txt", "b\n");
    repo.commit("feat: two");
    repo.write("a.txt", "three\n");
    repo.commit("fix: three");
    (repo, base)
}

fn range_source(repo: &TempRepo, from: &str, to: &str, group_by: GroupBy) -> GitSource {
    GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: from.to_string(),
            to: to.to_string(),
            group_by,
        },
    )
}

fn files_of(review: &Value) -> Vec<Value> {
    review["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["files"].as_array().unwrap().clone())
        .collect()
}

fn file_in<'a>(review: &'a Value, group: usize, path: &str) -> &'a Value {
    review["groups"][group]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("{path} not in group {group}"))
}

/// もう片方の単位を作る処理を、合図があるまで止めておく。
struct GatedSource {
    inner: Box<dyn ReviewSource>,
    gated: GroupBy,
    gate: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl ReviewSource for GatedSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.inner.content(file_id)
    }

    fn units(&self) -> Vec<GroupBy> {
        self.inner.units()
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        if unit == self.gated {
            let _ = self.gate.lock().unwrap().recv();
        }
        self.inner.review_unit(unit)
    }
}

/// 内容の取得（`content`）の呼び出しを数える。
struct CountingSource {
    inner: Box<dyn ReviewSource>,
    reads: Arc<AtomicUsize>,
}

impl ReviewSource for CountingSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.content(file_id)
    }

    fn units(&self) -> Vec<GroupBy> {
        self.inner.units()
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        self.inner.review_unit(unit)
    }

    fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
        self.inner.extra_focus_targets()
    }

    fn origin(&self, file_id: &str, force: bool) -> Result<Option<FileOrigin>, SourceError> {
        self.inner.origin(file_id, force)
    }
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
async fn unit_failure_keeps_review_and_retries() {
    let (repo, base) = range_repo();
    repo.git(&["branch", "topic"]);
    let server =
        LiveServer::start(Arc::new(range_source(&repo, &base, "topic", GroupBy::File))).await;
    repo.git(&["branch", "-D", "topic"]);

    let review = server.get_json("api/review").await;
    let failed = server.wait_unit("failed").await;

    let reason = failed["units"][1]["error"].as_str().unwrap();
    assert!(!reason.is_empty());
    let file_id = review["groups"][0]["files"][0]["id"].as_str().unwrap();
    let file = reqwest::get(format!("{}api/file/{file_id}", server.url()))
        .await
        .unwrap();
    assert_eq!(file.status(), 200, "the review must go on");

    repo.git(&["branch", "topic"]);
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
    assert!(files_of(&per_commit)
        .iter()
        .filter(|file| file["path"] == "a.txt")
        .all(|file| file["seen"] == false));
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
    assert!(marked
        .iter()
        .all(|file| file["focus"] == true && file["note"] == "ここ"));
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
    assert!(groups[2]["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|file| file["seen"] == false));
    assert_eq!(file_in(&refreshed, 0, "a.txt")["id"], first_a);
    assert_eq!(file_in(&refreshed, 0, "a.txt")["seen"], true);
    assert_eq!(refreshed["comments"][0]["body"], "残る");
    let final_form = server.get_json("api/review?unit=file").await;
    assert!(files_of(&final_form)
        .iter()
        .any(|file| file["path"] == "c.txt"));
    server.stop();
}

#[tokio::test]
async fn live_rewritten_history_comment_outdated_with_original_group() {
    let (repo, base) = range_repo();
    let server = LiveServer::start(Arc::new(range_source(
        &repo,
        &base,
        "HEAD",
        GroupBy::Commit,
    )))
    .await;
    let per_commit = server.get_json("api/review").await;
    let old_sha = per_commit["groups"][1]["id"].as_str().unwrap().to_string();
    let file = file_in(&per_commit, 1, "a.txt")["id"].clone();
    server
        .post(
            "api/comment",
            json!({"op": "add", "file_id": file, "side": "new",
                   "start_line": 1, "end_line": 1, "body": "書き換え前"}),
        )
        .await;

    repo.git(&["commit", "--amend", "-q", "-m", "fix: three (amended)"]);
    let refreshed = server.get_json("api/review?refresh=1").await;
    assert_ne!(refreshed["groups"][1]["id"], old_sha.as_str());
    let response = server
        .post("api/submit", json!({"verdict": "changes_requested"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;

    let comment = &document["comments"][0];
    assert_eq!(comment["group_id"], old_sha.as_str());
    assert_eq!(comment["group_title"], "fix: three");
    assert_eq!(comment["outdated"], true);
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

// ---- R-COMMENT（編集と削除）----

impl TestServer {
    async fn comment(&self, body: Value) -> reqwest::Response {
        self.post("api/comment", body).await
    }

    async fn add_new_side_comment(&self) -> Value {
        self.comment(json!({
            "op": "add", "file_id": "f1", "side": "new",
            "start_line": 11, "end_line": 11, "body": "元の本文", "suggestion": "NEW\n"
        }))
        .await
        .json()
        .await
        .unwrap()
    }

    async fn submit_comments(self) -> Vec<Value> {
        let response = self
            .post("api/submit", json!({"verdict": "approved"}))
            .await;
        assert_eq!(response.status(), 200);
        self.finish().await["comments"].as_array().unwrap().clone()
    }
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
    assert!(comments
        .iter()
        .all(|comment| comment["suggestion"] == Value::Null && comment["body"] != "x"));
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

// ---- R-SESSION（セッションの保存と凍結、submit での削除） ----

use std::sync::atomic::AtomicBool;
use std::sync::{Condvar, Mutex};

use kemi_core::session::{
    OpenSession, SessionCopy, SessionInfo, SessionMode, SessionState, SessionStore,
};
use kemi_server::SessionSink;

/// 保存先の呼び出しを記録する sink。`fail` で保存の失敗を再現する。
#[derive(Default)]
struct RecordingSink {
    title: Mutex<String>,
    total_files: Mutex<usize>,
    states: Mutex<Vec<SessionState>>,
    copies: Mutex<Vec<SessionCopy>>,
    unusable: Mutex<Vec<String>>,
    initial: Mutex<Option<SessionState>>,
    deleted: AtomicBool,
    fail: AtomicBool,
    /// 復元のように、写しが既に保存されている状態を装う。
    copy_already_saved: AtomicBool,
}

impl RecordingSink {
    fn last_state(&self) -> SessionState {
        self.states
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("no state was saved")
    }
}

impl SessionSink for RecordingSink {
    fn initial_state(&self) -> SessionState {
        self.initial.lock().unwrap().clone().unwrap_or_default()
    }

    fn needs_copy(&self) -> bool {
        !self.copy_already_saved.load(Ordering::SeqCst)
    }

    fn describe_review(&self, title: &str, total_files: usize) {
        *self.title.lock().unwrap() = title.to_string();
        *self.total_files.lock().unwrap() = total_files;
    }

    fn save_state(&self, state: SessionState) -> Result<(), String> {
        self.states.lock().unwrap().push(state);
        if self.fail.load(Ordering::SeqCst) {
            Err("disk is full".to_string())
        } else {
            Ok(())
        }
    }

    fn save_copy(&self, copy: SessionCopy) -> Result<(), String> {
        self.copies.lock().unwrap().push(copy);
        Ok(())
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), String> {
        self.unusable.lock().unwrap().push(reason.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), String> {
        self.deleted.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// 最初の状態保存を止める sink。保存が完了した順に状態を記録する。
struct SlowSink {
    entered: tokio::sync::Notify,
    calls: AtomicUsize,
    gate: Mutex<bool>,
    gate_opened: Condvar,
    states: Mutex<Vec<SessionState>>,
}

impl SlowSink {
    fn new() -> Self {
        SlowSink {
            entered: tokio::sync::Notify::new(),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(false),
            gate_opened: Condvar::new(),
            states: Mutex::new(Vec::new()),
        }
    }

    fn open_first_save(&self) {
        *self.gate.lock().unwrap() = true;
        self.gate_opened.notify_all();
    }

    fn last_saved_bodies(&self) -> Vec<String> {
        self.states
            .lock()
            .unwrap()
            .last()
            .map(|state| {
                state
                    .comments
                    .iter()
                    .map(|comment| comment.body.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl SessionSink for SlowSink {
    fn describe_review(&self, _title: &str, _total_files: usize) {}

    fn save_state(&self, state: SessionState) -> Result<(), String> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.entered.notify_one();
            let opened = self.gate.lock().unwrap();
            let _opened = self
                .gate_opened
                .wait_while(opened, |opened| !*opened)
                .unwrap();
        }
        self.states.lock().unwrap().push(state);
        Ok(())
    }

    fn save_copy(&self, _copy: SessionCopy) -> Result<(), String> {
        Ok(())
    }

    fn mark_unresumable(&self, _reason: &str) -> Result<(), String> {
        Ok(())
    }

    fn delete(&self) -> Result<(), String> {
        Ok(())
    }
}

impl TestServer {
    async fn start_with_session(source: Arc<FakeSource>, sink: Arc<dyn SessionSink>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = session_url(&listener, "test-token", Ipv4Addr::LOCALHOST).unwrap();
        let origin = url.split("/s/").next().unwrap().to_string();
        let params = ServeParams {
            source: source.clone(),
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
            results: None,
            session: Some(sink),
            share_address: None,
        };
        let task = tokio::spawn(serve(listener, params));
        TestServer {
            url,
            origin,
            port,
            source,
            task,
        }
    }

    async fn wait_for_copy(&self, sink: &RecordingSink) -> SessionCopy {
        for _ in 0..200 {
            if let Some(copy) = sink.copies.lock().unwrap().first().cloned() {
                return copy;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the session copy was never saved");
    }
}

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
    assert_eq!(state.comments[0].replies, vec!["reply".to_string()]);
    assert!(state.comments[0].resolved);
    assert!(state.seen.contains("f1"));
    assert_eq!(state.collapsed.get("f1"), Some(&true));
    assert_eq!(*sink.title.lock().unwrap(), "テストのレビュー");
    assert_eq!(*sink.total_files.lock().unwrap(), 12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_saves_keep_changes_made_while_a_save_is_slow() {
    let sink = Arc::new(SlowSink::new());
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;

    let url = server.url.clone();
    let origin = server.origin.clone();
    let first = tokio::spawn({
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
    sink.entered.notified().await;
    let mut second = tokio::spawn(async move {
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

    // 2 つ目の変更が保存に入るのを待つ。保存が直列化されていれば入らないので、
    // 打ち切ってから最初の保存を解放する。
    let second_done = tokio::time::timeout(Duration::from_millis(300), &mut second).await;
    sink.open_first_save();
    let second_response = match second_done {
        Ok(result) => result.unwrap(),
        Err(_) => second.await.unwrap(),
    };

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

/// 2 つ目のグループ単位の作成が遅いソース。凍結は両方がそろうまで待つ。
struct TwoUnitSource {
    first: ReviewMeta,
    second: ReviewMeta,
    contents: HashMap<String, FileContent>,
}

impl ReviewSource for TwoUnitSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        Ok(self.first.clone())
    }

    fn units(&self) -> Vec<GroupBy> {
        vec![GroupBy::File, GroupBy::Commit]
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        if unit == GroupBy::Commit {
            std::thread::sleep(Duration::from_millis(200));
            Ok(self.second.clone())
        } else {
            Ok(self.first.clone())
        }
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.contents
            .get(file_id)
            .cloned()
            .ok_or_else(|| SourceError::UnknownFileId(file_id.to_string()))
    }
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
        share_address: None,
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
    sink.fail.store(true, Ordering::SeqCst);
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;

    let added = server
        .comment(json!({"op": "add", "file_id": "f1", "side": "new", "start_line": 1, "end_line": 1, "body": "x"}))
        .await;
    assert_eq!(added.status(), 200);
    let review = server.get("api/review").await;
    assert_eq!(review.status(), 200);

    let response = server
        .post("api/submit", json!({"verdict": "changes_requested"}))
        .await;
    assert_eq!(response.status(), 200);
    let document = server.finish().await;
    assert_eq!(document["comments"].as_array().unwrap().len(), 1);
}

/// 一時ディレクトリのセッションを実際に書く sink。上限の扱いを core に任せる。
struct StoreSink {
    open: Mutex<OpenSession>,
}

impl SessionSink for StoreSink {
    fn describe_review(&self, _title: &str, _total_files: usize) {}

    fn save_state(&self, state: SessionState) -> Result<(), String> {
        self.open
            .lock()
            .unwrap()
            .save_state(state)
            .map_err(|error| error.to_string())
    }

    fn save_copy(&self, copy: SessionCopy) -> Result<(), String> {
        self.open
            .lock()
            .unwrap()
            .save_copy(copy)
            .map_err(|error| error.to_string())
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), String> {
        self.open
            .lock()
            .unwrap()
            .mark_unresumable(reason)
            .map_err(|error| error.to_string())
    }

    fn delete(&self) -> Result<(), String> {
        self.open
            .lock()
            .unwrap()
            .delete()
            .map_err(|error| error.to_string())
    }
}

fn incompressible(size: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xff) as u8
        })
        .collect()
}

#[tokio::test]
async fn session_copy_over_the_limit_is_unresumable() {
    let dir = std::env::temp_dir().join(format!(
        "kemi-server-session-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SessionStore::new(dir.clone());
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
    let open = store.create(info).unwrap();
    let sink = Arc::new(StoreSink {
        open: Mutex::new(open),
    });

    let mut source = FakeSource::new();
    source.meta.groups[0].files = vec![file_entry("f1", "src/a.rs"), file_entry("big", "big.bin")];
    source.contents.insert(
        "big".to_string(),
        FileContent {
            old: None,
            new: Some(incompressible(21 * 1024 * 1024)),
        },
    );
    let server = TestServer::start_with_session(Arc::new(source), sink).await;
    // 状態を持つセッションだけが上限超過の印を残せる。状態も写しも無いセッションは
    // 残さない（R-SESSION）。
    server.add_new_side_comment().await;
    server.get("api/review").await;

    let id = "01HF7YAT00SERVER0000000000";
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(stored) = store.read(id) {
            if matches!(stored.copy, kemi_core::session::CopyState::Unusable(_)) {
                break;
            }
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
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn session_initial_state_is_restored() {
    use kemi_core::domain::review::Side;
    let comment = kemi_core::domain::review::Comment {
        id: "c7".to_string(),
        file_id: "f1".to_string(),
        group_id: "g1".to_string(),
        group_title: "最初の変更".to_string(),
        path: "src/a.rs".to_string(),
        side: Side::New,
        start_line: Some(1),
        end_line: Some(1),
        quote: vec!["x".to_string()],
        body: "復元されたコメント".to_string(),
        replies: vec!["返信".to_string()],
        resolved: true,
        outdated: false,
        content_hash: "hash".to_string(),
        suggestion: None,
    };
    let sink = Arc::new(RecordingSink::default());
    *sink.initial.lock().unwrap() = Some(SessionState {
        comments: vec![comment],
        seen: ["f1".to_string()].into_iter().collect(),
        collapsed: [("f1".to_string(), true)].into_iter().collect(),
        last_comment: 7,
    });
    let server = TestServer::start_with_session(Arc::new(FakeSource::new()), sink).await;

    let review: Value = server.get("api/review").await.json().await.unwrap();
    let file = &review["groups"][0]["files"][0];

    assert_eq!(review["comments"][0]["body"], "復元されたコメント");
    assert_eq!(review["comments"][0]["resolved"], true);
    assert_eq!(file["seen"], true);
    assert_eq!(file["collapsed"], true);
}

// ---- R-RENDER（描画のエンドポイントと api/file の描画表示の情報） ----

#[tokio::test]
async fn render_requires_the_token() {
    let server = TestServer::start().await;
    let response = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/api/render/f5",
            server.port
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn render_reads_content_only_when_the_rendered_view_is_requested() {
    let server = TestServer::start().await;
    let _ = server.get("api/review").await.text().await.unwrap();
    assert_eq!(server.source.reads(), 0);

    let response = server.get("api/render/f5").await;

    assert_eq!(response.status(), 200);
    assert_eq!(server.source.reads(), 1);
}

#[tokio::test]
async fn file_of_a_markdown_offers_the_rendered_toggle_and_other_files_do_not() {
    let server = TestServer::start().await;

    let markdown: Value = server.get("api/file/f5").await.json().await.unwrap();
    assert_eq!(markdown["render"]["target"], "markdown");
    assert_eq!(markdown["render"]["toggle"], true);
    assert_eq!(markdown["render"]["initial"], "source");
    assert_eq!(markdown["render"]["reason"], Value::Null);

    let source: Value = server.get("api/file/f1").await.json().await.unwrap();
    assert_eq!(source["render"]["target"], Value::Null);
    assert_eq!(source["render"]["toggle"], false);
}

#[tokio::test]
async fn file_of_a_markdown_over_the_line_limit_reports_why_it_cannot_be_rendered() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f4").await.json().await.unwrap();

    assert_eq!(body["render"]["target"], "markdown");
    assert_eq!(body["render"]["toggle"], true);
    assert!(body["render"]["reason"].is_string(), "{body}");
}

#[tokio::test]
async fn file_of_an_image_carries_no_line_or_byte_reason() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f2").await.json().await.unwrap();

    assert_eq!(body["render"]["reason"], Value::Null, "{body}");
}

#[tokio::test]
async fn render_follows_the_highlight_setting_and_the_theme() {
    let server = TestServer::start().await;

    let light: Value = server.get("api/render/f5").await.json().await.unwrap();
    let off: Value = server
        .get("api/render/f5?highlight=off")
        .await
        .json()
        .await
        .unwrap();
    let dark: Value = server
        .get("api/render/f5?dark=1")
        .await
        .json()
        .await
        .unwrap();

    let light_html = light["html"].as_str().unwrap();
    let off_html = off["html"].as_str().unwrap();
    let dark_html = dark["html"].as_str().unwrap();
    assert!(light_html.contains("style=\"color:#"), "{light_html}");
    assert!(!off_html.contains("style=\"color:#"), "{off_html}");
    assert!(dark_html.contains("style=\"color:#"), "{dark_html}");
    assert_ne!(light_html, dark_html);
    assert!(off_html.contains("let a = 1;"), "{off_html}");
}

#[tokio::test]
async fn render_returns_blocks_and_turns_review_image_references_into_urls() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();

    assert_eq!(body["kind"], "markdown");
    let blocks = body["blocks"].as_array().unwrap();
    assert_eq!(blocks[0]["side"], "new");
    assert_eq!(blocks[0]["start"], 1);
    assert_eq!(blocks[0]["end"], 1);
    assert_eq!(blocks[0]["mark"], "unchanged");
    assert_eq!(blocks[1]["mark"], "modified");
    let html = body["html"].as_str().unwrap();
    assert!(html.contains("<img src=\"api/image/"), "{html}");
    assert!(html.contains("data-kemi-path=\"img/shot.png\""), "{html}");
    assert_eq!(body["new_lines"].as_array().unwrap().len(), 9);
    assert_eq!(body["old_lines"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn render_of_a_markdown_the_parser_rejects_is_a_failure_response() {
    let server = TestServer::start().await;

    let response = server.get("api/render/f7").await;

    assert_eq!(response.status(), 422);
    let body: Value = response.json().await.unwrap();
    assert!(body["error"].is_string(), "{body}");
}

#[tokio::test]
async fn crlf_markdown_is_measured_the_same_way_before_and_at_rendering() {
    // 生バイトは 1 MB を超え、改行を正規化すると下回る。行数（10,000）は上限の中。
    let text = format!("{}\r\n\r\n", "x".repeat(206)).repeat(5_000);
    assert!(text.len() > AUTO_MAX_BYTES);
    assert!(text.len() - 10_000 <= AUTO_MAX_BYTES);
    let source = FakeSource::new().with_file(
        FileEntry {
            status: Status::Add,
            ..file_entry("f13", "docs/crlf.md")
        },
        FileContent {
            old: None,
            new: Some(text.into_bytes()),
        },
    );
    let server = TestServer::start_source(Arc::new(source), Ipv4Addr::LOCALHOST, None).await;

    let file: Value = server
        .get("api/file/f13?highlight=off")
        .await
        .json()
        .await
        .unwrap();
    let render = server.get("api/render/f13").await;

    assert_eq!(file["render"]["reason"], Value::Null, "{}", file["render"]);
    assert_eq!(render.status(), 200);
}

#[tokio::test]
async fn file_of_a_csv_with_an_unbalanced_quote_reports_why_it_cannot_be_rendered() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f8").await.json().await.unwrap();

    assert_eq!(body["render"]["target"], "table");
    assert_eq!(body["render"]["toggle"], true);
    assert!(body["render"]["reason"].is_string(), "{body}");
    let response = server.get("api/render/f8").await;
    assert_eq!(response.status(), 422);
}

#[tokio::test]
async fn render_of_a_tsv_returns_table_rows_as_blocks() {
    let server = TestServer::start().await;

    let file: Value = server.get("api/file/f9").await.json().await.unwrap();
    assert_eq!(file["render"]["target"], "table");
    assert_eq!(file["render"]["reason"], Value::Null);

    let body: Value = server.get("api/render/f9").await.json().await.unwrap();
    assert_eq!(body["kind"], "table");
    let html = body["html"].as_str().unwrap();
    assert!(html.contains("<th>note</th>"), "{html}");
    assert!(html.contains("data-kemi-block=\"old:2-2\""), "{html}");
    assert!(html.contains("data-kemi-block=\"new:2-2\""), "{html}");
    assert_eq!(body["blocks"].as_array().unwrap().len(), 3);
}

// ---- R-RENDER（画像） ----

#[tokio::test]
async fn review_image_bytes_carry_the_content_type_nosniff_and_sandbox_headers() {
    let server = TestServer::start().await;

    let response = server.get("api/image/review/f2/new").await;

    assert_eq!(response.status(), 200);
    let headers = response.headers().clone();
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("content-security-policy").unwrap(), "sandbox");
    assert_eq!(
        response.bytes().await.unwrap().to_vec(),
        vec![0xff, 0x00, 0x02]
    );

    let old = server.get("api/image/review/f2/old").await;
    assert_eq!(old.bytes().await.unwrap().to_vec(), vec![0xff, 0x00, 0x01]);
    let svg = server.get("api/image/review/f11/new").await;
    assert_eq!(svg.headers().get("content-type").unwrap(), "image/svg+xml");
}

#[tokio::test]
async fn review_image_is_refused_without_the_token_and_for_unknown_or_non_image_ids() {
    let server = TestServer::start().await;
    let without_token = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/api/image/review/f2/new",
            server.port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(without_token.status(), 404);

    assert_eq!(server.get("api/image/review/nope/new").await.status(), 404);
    assert_eq!(server.get("api/image/review/f1/new").await.status(), 404);
    assert_eq!(server.get("api/image/review/f6/old").await.status(), 404);
}

#[tokio::test]
async fn file_of_an_image_over_five_megabytes_only_reports_the_byte_counts() {
    let server = TestServer::start().await;

    let body: Value = server.get("api/file/f10").await.json().await.unwrap();

    assert_eq!(body["binary"], true);
    assert_eq!(body["new_size"], 6_000_000);
    assert_eq!(body["render"]["target"], Value::Null);
    assert_eq!(server.get("api/render/f10").await.status(), 422);
}

#[tokio::test]
async fn untracked_files_over_the_limit_are_not_rendered_because_their_content_was_not_read() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    // 1 MB 超 5 MB 未満の untracked。画像は 5 MB の規則の中だが、内容を読んでいない。
    let big = vec![b'x'; UNTRACKED_LIMIT as usize + 1];
    std::fs::write(repo.path.join("shot.png"), &big).unwrap();
    std::fs::write(repo.path.join("notes.md"), &big).unwrap();
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = LiveServer::start(source).await;

    let review = server.get_json("api/review").await;
    let id_of = |path: &str| {
        file_in(&review, 0, path)["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let image = server
        .get_json(&format!("api/file/{}", id_of("shot.png")))
        .await;
    let markdown = server
        .get_json(&format!("api/file/{}", id_of("notes.md")))
        .await;

    assert_eq!(
        image["render"]["target"],
        Value::Null,
        "{}",
        image["render"]
    );
    assert!(
        markdown["render"]["reason"].is_string(),
        "{}",
        markdown["render"]
    );
    server.stop();
}

#[tokio::test]
async fn file_of_an_svg_has_the_toggle_with_rendered_as_default_and_other_images_have_no_toggle() {
    let server = TestServer::start().await;

    let svg: Value = server.get("api/file/f11").await.json().await.unwrap();
    assert_eq!(svg["render"]["target"], "image");
    assert_eq!(svg["render"]["toggle"], true);
    assert_eq!(svg["render"]["initial"], "rendered");
    assert_eq!(svg["binary"], false);

    let png: Value = server.get("api/file/f2").await.json().await.unwrap();
    assert_eq!(png["render"]["target"], "image");
    assert_eq!(png["render"]["toggle"], false);
    assert_eq!(png["render"]["initial"], "rendered");
}

#[tokio::test]
async fn render_of_an_image_gives_both_sides_and_marks_a_rename_with_identical_bytes_as_unchanged()
{
    let server = TestServer::start().await;

    let changed: Value = server.get("api/render/f2").await.json().await.unwrap();
    assert_eq!(changed["kind"], "image");
    assert_eq!(changed["same"], false);
    assert_eq!(changed["old"]["size"], 3);
    assert_eq!(changed["new"]["size"], 3);
    assert!(changed["old"]["url"]
        .as_str()
        .unwrap()
        .contains("api/image/review/f2/old"));

    let renamed: Value = server.get("api/render/f12").await.json().await.unwrap();
    assert_eq!(renamed["same"], true);

    let added: Value = server.get("api/render/f6").await.json().await.unwrap();
    assert_eq!(added["old"], Value::Null);
    assert_eq!(added["new"]["size"], 5);
}

// ---- R-RENDER（Markdown の相対パス画像） ----

/// f5（docs/guide.md）の相対パス画像: img/shot.png はレビュー対象の f6 に一致し、
/// img/other.png はレビュー対象に無い。
fn guide_with_two_images() -> FakeSource {
    let mut source = FakeSource::new();
    source.contents.insert(
        "f5".to_string(),
        FileContent {
            old: None,
            new: Some(b"![a](img/shot.png)\n\n![b](img/other.png)\n".to_vec()),
        },
    );
    source
}

fn image_sources(html: &str) -> Vec<String> {
    html.match_indices("<img src=\"")
        .map(|(at, prefix)| {
            let rest = &html[at + prefix.len()..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

#[tokio::test]
async fn relative_images_in_the_review_use_the_review_url_and_others_use_the_repository_url() {
    let source =
        Arc::new(guide_with_two_images().with_repository(&[("docs/img/other.png", &[0x89, 1])]));
    let server = TestServer::start_source(source, Ipv4Addr::LOCALHOST, None).await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();
    let sources = image_sources(body["html"].as_str().unwrap());

    assert_eq!(sources.len(), 2, "{sources:?}");
    assert!(
        sources[0].contains("api/image/review/f6/new"),
        "{sources:?}"
    );
    assert!(!sources[1].contains("review"), "{sources:?}");
    let response = server.get(&sources[1]).await;
    let status = response.status();
    assert_eq!(
        status,
        200,
        "{} -> {}",
        sources[1],
        response.text().await.unwrap()
    );
    let response = server.get(&sources[1]).await;
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        response.headers().get("content-security-policy").unwrap(),
        "sandbox"
    );
    assert_eq!(response.bytes().await.unwrap().to_vec(), vec![0x89, 1]);
}

#[tokio::test]
async fn relative_images_the_source_cannot_serve_are_404_from_the_repository_url() {
    let source = Arc::new(guide_with_two_images().with_repository(&[]));
    let server = TestServer::start_source(source, Ipv4Addr::LOCALHOST, None).await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();
    let sources = image_sources(body["html"].as_str().unwrap());

    assert_eq!(sources.len(), 2, "{sources:?}");
    assert_eq!(server.get(&sources[1]).await.status(), 404);
    let without_token = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{}/s/wrong-token/{}",
            server.port, sources[1]
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(without_token.status(), 404);
}

#[tokio::test]
async fn relative_images_are_frames_when_the_source_cannot_read_the_repository() {
    let server =
        TestServer::start_source(Arc::new(guide_with_two_images()), Ipv4Addr::LOCALHOST, None)
            .await;

    let body: Value = server.get("api/render/f5").await.json().await.unwrap();
    let html = body["html"].as_str().unwrap();
    let sources = image_sources(html);

    assert_eq!(sources.len(), 1, "{sources:?}");
    assert!(
        sources[0].contains("api/image/review/f6/new"),
        "{sources:?}"
    );
    assert!(html.contains("kb-img-frame"), "{html}");
    assert!(html.contains("img/other.png"), "{html}");
}
