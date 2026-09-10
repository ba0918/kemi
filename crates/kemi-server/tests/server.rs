//! HTTP / SSE サーバの契約テスト（R-SERVE, R-SUBMIT, R-COMMENT）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use kemi_core::domain::review::{Approval, FileEntry, Group, ReviewMeta, Status};
use kemi_core::source::{FileContent, ReviewSource, SourceError};
use kemi_server::{serve, session_url, Asset, Assets, ServeOutcome, ServeParams};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct FakeSource {
    meta: ReviewMeta,
    contents: HashMap<String, FileContent>,
    reads: AtomicUsize,
    content_delay_ms: AtomicUsize,
    content_started: tokio::sync::Notify,
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
        let large = "line\n".repeat(10_001);
        contents.insert(
            "f3".to_string(),
            FileContent {
                old: Some(large.clone().into_bytes()),
                new: Some(large.into_bytes()),
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
        }
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
        let source = Arc::new(FakeSource::new());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = session_url(&listener, "test-token").unwrap();
        let origin = url.split("/s/").next().unwrap().to_string();
        let params = ServeParams {
            source: source.clone(),
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
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
            "http://evil.example",
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

#[tokio::test]
async fn host_mismatch_rejected() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let server = TestServer::start().await;
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
        .await
        .unwrap();
    let body = "{}";
    let request = format!(
        "POST /s/test-token/api/state HTTP/1.1\r\nHost: evil.example\r\nOrigin: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        server.origin,
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
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

fn raw_post(address: (&str, u16), path: &str, origin: &str, body: &str) -> u16 {
    use std::io::{Read, Write};

    let mut stream = std::net::TcpStream::connect(address).unwrap();
    let host = format!("{}:{}", address.0, address.1);
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

use kemi_core::source::git::{GitMode, GitSource, GroupBy};
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

    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
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
async fn live_debounce_coalesces_rapid_writes() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.commit("base");
    repo.write("a.txt", "initial change\n");
    let source = Arc::new(GitSource::new(repo.path.clone(), GitMode::Worktree));
    let server = LiveServer::start(source).await;

    let mut sse = SseStream::connect(server.port).await;
    for index in 0..5 {
        repo.write("a.txt", &format!("write {index}\n"));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let count = sse.count_updates(Duration::from_millis(1400)).await;
    // 5 回の書き込みが debounce でまとまる。tick 境界の関係で 1〜2 通に
    // なることはあるが、書き込みごとに通知はしない（バッジは冪等に出す）。
    assert!(
        (1..=2).contains(&count),
        "rapid writes should coalesce, got {count} updates"
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
