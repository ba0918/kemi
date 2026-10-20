//! HTTP / SSE サーバの契約テスト（R-SERVE, R-SUBMIT, R-COMMENT）。

#![expect(
    clippy::unwrap_used,
    reason = "#[test] の外の補助関数も、失敗をそのままテストの失敗として見せる"
)]

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use kemi_core::domain::content::AUTO_MAX_BYTES;
use kemi_core::domain::review::{Approval, FileEntry, Group, ReviewMeta, Side, Status};
use kemi_core::source::{FileContent, ReviewSource, SourceError};
use kemi_server::{
    Asset, Assets, Notice, NoticeSink, ResultSaveError, ResultSink, ServeOutcome, ServeParams,
    serve, session_host, session_url,
};
use serde_json::{Value, json};
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
    notices: Arc<RecordingNotices>,
    task: JoinHandle<Result<ServeOutcome, kemi_server::ServerError>>,
}

/// サーバが起動側に知らせたことを記録する。
#[derive(Default)]
struct RecordingNotices {
    notices: std::sync::Mutex<Vec<Notice>>,
}

impl NoticeSink for RecordingNotices {
    fn notify(&self, notice: Notice) {
        self.notices.lock().unwrap().push(notice);
    }
}

impl RecordingNotices {
    fn any(&self, predicate: impl Fn(&Notice) -> bool) -> bool {
        self.notices.lock().unwrap().iter().any(predicate)
    }
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
        let notices = Arc::new(RecordingNotices::default());
        let params = ServeParams {
            source: source.clone(),
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
            results: None,
            session: None,
            notices: notices.clone(),
            share_address,
            agent: None,
            live: None,
            initial_explanation: None,
        };
        let task = tokio::spawn(serve(listener, params));
        TestServer {
            url,
            origin,
            port,
            source,
            notices,
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
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status line")
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
    notices: Arc<RecordingNotices>,
    task: JoinHandle<Result<ServeOutcome, kemi_server::ServerError>>,
}

impl LiveServer {
    async fn start(source: Arc<dyn ReviewSource>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let notices = Arc::new(RecordingNotices::default());
        let params = ServeParams {
            source,
            assets: Arc::new(FakeAssets),
            token: "live-token".to_string(),
            results: None,
            session: None,
            notices: notices.clone(),
            share_address: None,
            agent: None,
            live: None,
            initial_explanation: None,
        };
        let task = tokio::spawn(serve(listener, params));
        // 監視スレッドがパスを登録するのを待つ。
        tokio::time::sleep(Duration::from_millis(400)).await;
        LiveServer {
            port,
            notices,
            task,
        }
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

struct GenerationGate {
    entered: tokio::sync::Notify,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
struct GenerationSource {
    inner: Arc<dyn ReviewSource>,
    gates: Arc<Vec<GenerationGate>>,
    next: Arc<AtomicUsize>,
    generation: Option<usize>,
}
impl ReviewSource for GenerationSource {
    fn fork(&self) -> Result<Option<Arc<dyn ReviewSource>>, SourceError> {
        Ok(Some(Arc::new(Self {
            inner: self.inner.fork()?.unwrap_or_else(|| self.inner.clone()),
            gates: self.gates.clone(),
            next: self.next.clone(),
            generation: Some(self.next.fetch_add(1, Ordering::SeqCst)),
        })))
    }
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }
    fn content(&self, id: &str) -> Result<FileContent, SourceError> {
        self.inner.content(id)
    }
    fn units(&self) -> Vec<GroupBy> {
        self.inner.units()
    }
    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        if unit == GroupBy::Commit
            && let Some(index) = self.generation
            && let Some(gate) = self.gates.get(index)
        {
            gate.entered.notify_one();
            let _ = gate.release.lock().unwrap().recv();
        }
        self.inner.review_unit(unit)
    }
}

struct UnitFailureSource {
    inner: Arc<dyn ReviewSource>,
    fail: Arc<AtomicBool>,
}
impl ReviewSource for UnitFailureSource {
    fn fork(&self) -> Result<Option<Arc<dyn ReviewSource>>, SourceError> {
        Ok(Some(Arc::new(Self {
            inner: self.inner.fork()?.unwrap_or_else(|| self.inner.clone()),
            fail: self.fail.clone(),
        })))
    }
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }
    fn content(&self, id: &str) -> Result<FileContent, SourceError> {
        self.inner.content(id)
    }
    fn units(&self) -> Vec<GroupBy> {
        self.inner.units()
    }
    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        if unit == GroupBy::Commit && self.fail.load(Ordering::SeqCst) {
            Err(SourceError::Git("cannot compute opposite unit".into()))
        } else {
            self.inner.review_unit(unit)
        }
    }
}

async fn check_initial_copy_order(reverse: bool) {
    let repo = TempRepo::new();
    repo.write("a.txt", "base\n");
    let base = repo.commit("base");
    repo.write("a.txt", "D1\n");
    repo.commit("D1");
    let (release1, receiver1) = std::sync::mpsc::channel();
    let (release2, receiver2) = std::sync::mpsc::channel();
    let gates = Arc::new(vec![
        GenerationGate {
            entered: tokio::sync::Notify::new(),
            release: Mutex::new(receiver1),
        },
        GenerationGate {
            entered: tokio::sync::Notify::new(),
            release: Mutex::new(receiver2),
        },
    ]);
    let source = Arc::new(GenerationSource {
        inner: Arc::new(range_source(&repo, &base, "HEAD", GroupBy::File)),
        gates: gates.clone(),
        next: Arc::new(AtomicUsize::new(0)),
        generation: None,
    });
    let sink = Arc::new(RecordingSink::default());
    let server = AgentServer::start_with_sink(source, Ipv4Addr::LOCALHOST, sink.clone()).await;
    let d1: Value = server.page.get("api/review").await.json().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), gates[0].entered.notified())
        .await
        .unwrap();
    repo.write("b.txt", "D2\n");
    repo.commit("D2");
    let d2: Value = tokio::time::timeout(Duration::from_secs(3), async {
        server
            .page
            .get("api/review?refresh=1")
            .await
            .json()
            .await
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(d2["diff_display_version"], "01TESTREVIEW:2");
    tokio::time::timeout(Duration::from_secs(3), gates[1].entered.notified())
        .await
        .unwrap();
    if reverse {
        release2.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if server.page.get("api/review?unit=commit").await.status() == 200 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(sink.copies.lock().unwrap().is_empty());
    }
    release1.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if sink
                .states
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.display.as_ref().is_some_and(|d| d.frozen.is_some()))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("D1 copy must finish without waiting for D2 opposite unit");
    let copies = sink.copies.lock().unwrap();
    let copy = copies.last().unwrap();
    assert_eq!(copy.units.len(), 2);
    assert_eq!(copy.units[0].review.groups[0].files.len(), 1);
    assert_eq!(copy.units[1].review.groups.len(), 1);
    drop(copies);
    let saved = sink.last_state();
    let display = saved.display.unwrap();
    assert_eq!(
        display.current,
        d2["diff_display_version"].as_str().unwrap()
    );
    assert_eq!(
        display.frozen.as_deref(),
        d1["diff_display_version"].as_str()
    );
    assert_eq!(display.issued, 2);
    if !reverse {
        release2.send(()).unwrap();
    }
    server.page.task.abort();
}

impl ReviewSource for GatedSource {
    fn select_references(
        &self,
        requested: &[(
            kemi_core::domain::explanation::ReferenceUnit,
            String,
            String,
        )],
    ) -> Result<Vec<kemi_core::domain::explanation::CodeReferenceEntry>, SourceError> {
        self.inner.select_references(requested)
    }
    fn reference_metadata(&self, id: &str) -> Option<kemi_core::source::ReferenceMetadata> {
        self.inner.reference_metadata(id)
    }
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
    fn select_references(
        &self,
        requested: &[(
            kemi_core::domain::explanation::ReferenceUnit,
            String,
            String,
        )],
    ) -> Result<Vec<kemi_core::domain::explanation::CodeReferenceEntry>, SourceError> {
        self.inner.select_references(requested)
    }
    fn reference_metadata(&self, id: &str) -> Option<kemi_core::source::ReferenceMetadata> {
        self.inner.reference_metadata(id)
    }
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

    async fn submit_document(self) -> Value {
        let response = self
            .post("api/submit", json!({"verdict": "approved"}))
            .await;
        assert_eq!(response.status(), 200);
        self.finish().await
    }

    async fn submit_comments(self) -> Vec<Value> {
        let response = self
            .post("api/submit", json!({"verdict": "approved"}))
            .await;
        assert_eq!(response.status(), 200);
        self.finish().await["comments"].as_array().unwrap().clone()
    }
}

// ---- R-SESSION（セッションの保存と凍結、submit での削除） ----

use std::sync::atomic::AtomicBool;
use std::sync::{Condvar, Mutex};

use kemi_core::session::{
    SessionCopy, SessionError, SessionInfo, SessionMode, SessionState, SessionStore,
};
use kemi_server::SessionSink;

/// 保存先の呼び出しを記録する sink。`fail` で保存の失敗を再現する。
#[derive(Default)]
struct RecordingSink {
    explanation_gate: Mutex<Option<Arc<GenerationGate>>>,
    accepted: Mutex<Vec<SessionState>>,
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

/// ファイルを持たない sink の書き込みの結果。
fn keeps_no_files() -> kemi_core::session::FilesWrite {
    kemi_core::session::FilesWrite {
        written: kemi_core::session::FileWritten::Failed(SessionError::Io {
            path: std::path::PathBuf::from("files"),
            source: std::io::Error::other("this sink keeps no files"),
        }),
        removed_snapshots: Vec::new(),
    }
}

impl SessionSink for RecordingSink {
    fn save_explanation_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.save_state(state)
    }
    fn save_issued_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.save_state(state)
    }
    fn is_resumed(&self) -> bool {
        self.initial.lock().unwrap().is_some()
    }
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

    fn save_state(&self, state: SessionState) -> Result<(), SessionError> {
        if state.explanation.is_some()
            && let Some(gate) = self.explanation_gate.lock().unwrap().take()
        {
            gate.entered.notify_one();
            let _ = gate.release.lock().unwrap().recv();
        }
        self.states.lock().unwrap().push(state);
        if self.fail.load(Ordering::SeqCst) {
            Err(SessionError::Io {
                path: std::path::PathBuf::from("sessions/x.session"),
                source: std::io::Error::other("disk is full"),
            })
        } else {
            self.accepted
                .lock()
                .unwrap()
                .push(self.states.lock().unwrap().last().unwrap().clone());
            Ok(())
        }
    }

    fn save_frozen_copy(
        &self,
        copy: kemi_core::session::PreparedCopy,
        state: SessionState,
    ) -> Result<bool, SessionError> {
        self.copies.lock().unwrap().push(copy.into_copy());
        self.save_issued_state(state)?;
        Ok(true)
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), SessionError> {
        self.unusable.lock().unwrap().push(reason.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), SessionError> {
        self.deleted.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn save_image(
        &self,
        _comment_id: &str,
        _bytes: &[u8],
        _comments: &std::collections::BTreeSet<String>,
    ) -> kemi_core::session::FilesWrite {
        keeps_no_files()
    }

    fn save_snapshot(
        &self,
        _snapshot: &kemi_core::session::PageSnapshot,
        _comments: &std::collections::BTreeSet<String>,
    ) -> kemi_core::session::FilesWrite {
        keeps_no_files()
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

// 途中のアサーションで失敗しても、保存を止めたワーカーを残さないため。
struct FirstSaveGuard(Arc<SlowSink>);

impl Drop for FirstSaveGuard {
    fn drop(&mut self) {
        self.0.open_first_save();
    }
}

impl SessionSink for SlowSink {
    fn save_explanation_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.save_issued_state(state)
    }
    fn save_issued_state(&self, state: SessionState) -> Result<(), SessionError> {
        self.states.lock().unwrap().push(state);
        Ok(())
    }
    fn describe_review(&self, _title: &str, _total_files: usize) {}

    fn save_state(&self, state: SessionState) -> Result<(), SessionError> {
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

    fn save_frozen_copy(
        &self,
        _copy: kemi_core::session::PreparedCopy,
        state: SessionState,
    ) -> Result<bool, SessionError> {
        self.save_issued_state(state)?;
        Ok(true)
    }

    fn mark_unresumable(&self, _reason: &str) -> Result<(), SessionError> {
        Ok(())
    }

    fn delete(&self) -> Result<(), SessionError> {
        Ok(())
    }

    fn save_image(
        &self,
        _comment_id: &str,
        _bytes: &[u8],
        _comments: &std::collections::BTreeSet<String>,
    ) -> kemi_core::session::FilesWrite {
        keeps_no_files()
    }

    fn save_snapshot(
        &self,
        _snapshot: &kemi_core::session::PageSnapshot,
        _comments: &std::collections::BTreeSet<String>,
    ) -> kemi_core::session::FilesWrite {
        keeps_no_files()
    }
}

impl TestServer {
    async fn start_with_session(source: Arc<FakeSource>, sink: Arc<dyn SessionSink>) -> Self {
        TestServer::start_with_sinks(source, Some(sink), None).await
    }

    async fn start_with_sinks(
        source: Arc<FakeSource>,
        session: Option<Arc<dyn SessionSink>>,
        results: Option<Arc<dyn ResultSink>>,
    ) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = session_url(&listener, "test-token", Ipv4Addr::LOCALHOST).unwrap();
        let origin = url.split("/s/").next().unwrap().to_string();
        let notices = Arc::new(RecordingNotices::default());
        let params = ServeParams {
            source: source.clone(),
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
            results,
            session,
            notices: notices.clone(),
            share_address: None,
            agent: None,
            live: None,
            initial_explanation: None,
        };
        let task = tokio::spawn(serve(listener, params));
        TestServer {
            url,
            origin,
            port,
            source,
            notices,
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

/// 書けない結果ファイルを装う。
struct FailingResults;

impl ResultSink for FailingResults {
    fn save(&self, _text: &str) -> Result<std::path::PathBuf, ResultSaveError> {
        Err(ResultSaveError::Unlocated("no home directory".to_string()))
    }

    fn location(&self) -> Option<String> {
        None
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

// ---- R-AGENT-HAND（ページ側の API: 渡す・返信・解決・発言と、その通知） ----

use kemi_core::domain::agent::{AgentEvent, Handed, HandedComment};

impl TestServer {
    async fn with_recording_session() -> (Self, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let server =
            TestServer::start_with_session(Arc::new(FakeSource::new()), sink.clone()).await;
        (server, sink)
    }

    async fn add_comment_with_body(&self, line: u32, body: &str) -> Value {
        let response = self
            .comment(json!({
                "op": "add", "file_id": "f1", "side": "new",
                "start_line": line, "end_line": line, "body": body
            }))
            .await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }

    async fn hand(&self) -> Value {
        let response = self.post("api/hand", json!({})).await;
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }
}

/// 保存された状態にたまっている、まだ受け取られていない「渡した」の 1 回分ずつ。
fn saved_handed(sink: &RecordingSink) -> Vec<Handed> {
    sink.last_state()
        .channel
        .events
        .into_iter()
        .map(|event| match event {
            AgentEvent::Handed(handed) => handed,
        })
        .collect()
}

fn changes(handed: &Handed) -> Vec<(&'static str, String, Option<String>)> {
    handed
        .comments
        .iter()
        .map(|change| match change {
            HandedComment::Added(comment) => {
                ("added", comment.id.clone(), Some(comment.body.clone()))
            }
            HandedComment::Edited(comment) => {
                ("edited", comment.id.clone(), Some(comment.body.clone()))
            }
            HandedComment::Deleted(id) => ("deleted", id.clone(), None),
        })
        .collect()
}

/// SSE を開き、`event: <name>` の行の後の data を 1 つ読む。
async fn next_sse_event(response: &mut reqwest::Response, name: &str) -> Value {
    let needle = format!("event: {name}\ndata: ");
    let mut buffer = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(start) = buffer.find(&needle) {
            let rest = &buffer[start + needle.len()..];
            if let Some(end) = rest.find('\n') {
                return serde_json::from_str(&rest[..end]).unwrap();
            }
        }
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .unwrap_or_else(|_| panic!("no {name} event in {buffer:?}"))
            .unwrap()
            .expect("the event stream ended");
        buffer.push_str(&String::from_utf8_lossy(&chunk));
    }
}

// ---- R-AGENT-CLI / R-AGENT-WRITE / R-AGENT-STATE / R-AGENT-LINK（エージェント用の API） ----

use kemi_server::{AgentParams, bind_agent_listener};

/// エージェント用のリスナーも立てたサーバ。
struct AgentServer {
    page: TestServer,
    agent_port: u16,
    agent_token: String,
    sink: Arc<RecordingSink>,
    control: kemi_server::ServeControl,
}

impl AgentServer {
    async fn start() -> Self {
        AgentServer::start_with(Arc::new(FakeSource::new()), Ipv4Addr::LOCALHOST).await
    }

    async fn start_with(source: Arc<dyn ReviewSource>, bind: Ipv4Addr) -> Self {
        AgentServer::start_with_sink(source, bind, Arc::new(RecordingSink::default())).await
    }

    /// 保留したレビューを `--resume` で開き直したように、保存した状態から始める。
    async fn resume(saved: SessionState) -> Self {
        let sink = Arc::new(RecordingSink::default());
        *sink.initial.lock().unwrap() = Some(saved);
        AgentServer::start_with_sink(Arc::new(FakeSource::new()), Ipv4Addr::LOCALHOST, sink).await
    }

    async fn start_with_sink(
        source: Arc<dyn ReviewSource>,
        bind: Ipv4Addr,
        sink: Arc<RecordingSink>,
    ) -> Self {
        AgentServer::start_with_explanation(source, bind, sink, None).await
    }

    /// 起動時の説明（`--explanation`）を入れて始める。
    async fn start_with_explanation(
        source: Arc<dyn ReviewSource>,
        bind: Ipv4Addr,
        sink: Arc<RecordingSink>,
        initial_explanation: Option<kemi_core::domain::explanation::ExplanationDocument>,
    ) -> Self {
        let listener = TcpListener::bind((bind, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = session_url(&listener, "test-token", Ipv4Addr::LOCALHOST).unwrap();
        let origin = url.split("/s/").next().unwrap().to_string();
        let agent_listener = bind_agent_listener().await.unwrap();
        let agent_port = agent_listener.local_addr().unwrap().port();
        let notices = Arc::new(RecordingNotices::default());
        let control = kemi_server::ServeControl::new();
        let params = ServeParams {
            source,
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
            results: None,
            session: Some(sink.clone()),
            notices: notices.clone(),
            share_address: None,
            agent: Some(AgentParams {
                listener: agent_listener,
                token: "agent-secret".to_string(),
                review: "01TESTREVIEW".to_string(),
                control: control.clone(),
            }),
            live: None,
            initial_explanation,
        };
        let task = tokio::spawn(serve(listener, params));
        AgentServer {
            page: TestServer {
                url,
                origin,
                port,
                source: Arc::new(FakeSource::new()),
                notices,
                task,
            },
            agent_port,
            agent_token: "agent-secret".to_string(),
            sink,
            control,
        }
    }

    fn agent_url(&self, token: &str, action: &str) -> String {
        format!("http://127.0.0.1:{}/a/{token}/{action}", self.agent_port)
    }

    async fn agent_post(&self, action: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(self.agent_url(&self.agent_token, action))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    /// `kemi wait` と同じ要求を出す。応答の (状態コード, JSON)。
    async fn wait(&self, timeout_ms: Option<u64>) -> (u16, Value) {
        wait_like_the_cli(self.agent_port, self.agent_token.clone(), timeout_ms).await
    }

    async fn reply(&self, writes: Value) -> (u16, Value) {
        let response = self.agent_post("reply", json!({ "writes": writes })).await;
        let status = response.status().as_u16();
        (status, response.json().await.unwrap())
    }

    async fn agent_status(&self) -> String {
        let review: Value = self.page.get("api/review").await.json().await.unwrap();
        review["agent"]["status"].as_str().unwrap().to_string()
    }

    async fn wait_until_status(&self, expected: &str) {
        for _ in 0..200 {
            if self.agent_status().await == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the agent status never became {expected}");
    }
}

/// `kemi wait` と同じに、待って、起きたことを受け取りきったら受け取りを知らせる。
/// 応答の (状態コード, JSON)。
async fn wait_like_the_cli(port: u16, token: String, timeout_ms: Option<u64>) -> (u16, Value) {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{port}/a/{token}/wait"))
        .json(&json!({ "timeout_ms": timeout_ms }))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let answer: Value = response.json().await.unwrap();
    if let Some(through) = answer.get("through") {
        let received = client
            .post(format!("http://127.0.0.1:{port}/a/{token}/received"))
            .json(&json!({ "through": through }))
            .send()
            .await
            .unwrap();
        assert_eq!(received.status(), 200);
    }
    (status, answer)
}

/// 応答の「渡した」の 1 回分ずつ。時間切れの応答では空。
fn handed_events(answer: &Value) -> Vec<&Value> {
    answer["events"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|event| event["type"] == "handed")
        .collect()
}

struct SuspendGatedSource {
    inner: FakeSource,
    gate: Arc<GenerationGate>,
}

impl ReviewSource for SuspendGatedSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }

    fn content(&self, id: &str) -> Result<FileContent, SourceError> {
        self.gate.entered.notify_one();
        let _ = self.gate.release.lock().unwrap().recv();
        self.inner.content(id)
    }
}

/// 起動時は読めるが、取り直しの読み取りで失敗する入力。
struct FailingRefreshSource {
    inner: FakeSource,
    fail: AtomicBool,
}

impl ReviewSource for FailingRefreshSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(SourceError::Git("disk went away".to_string()));
        }
        self.inner.review()
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(SourceError::Git("disk went away".into()));
        }
        self.inner.content(file_id)
    }
}

fn agent_reply(comment_id: &str, body: &str) -> Value {
    json!({ "type": "reply", "comment_id": comment_id, "body": body })
}

/// 書き込みが 1 件も残っていないこと。
async fn assert_nothing_written(server: &AgentServer) {
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["comments"][0]["replies"], json!([]));
    assert_eq!(review["messages"], json!([]));
}

async fn assert_nothing_written_without_comments(server: &AgentServer) {
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(review["messages"], json!([]));
}

impl AgentServer {
    /// 渡した 1 回分の行（スレッドか、発言だけなら null, 状態）。
    async fn hand_lines(&self) -> Vec<(Value, String)> {
        let review: Value = self.page.get("api/review").await.json().await.unwrap();
        review["agent"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| {
                (
                    line["thread"].clone(),
                    line["state"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }
}

/// エージェント用の API に要求の頭だけを送り、本文はまだ送らない接続。
async fn agent_request_head(
    server: &AgentServer,
    action: &str,
    body_len: usize,
) -> tokio::net::TcpStream {
    use tokio::io::AsyncWriteExt;
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.agent_port))
        .await
        .unwrap();
    let head = format!(
        "POST /a/{}/{action} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n",
        server.agent_token,
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    stream
}

async fn agent_response_after_delayed_body(
    server: &AgentServer,
    token: &str,
    action: &str,
    extra_headers: &str,
) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.agent_port))
        .await
        .unwrap();
    let body = b"{}";
    let head = format!(
        "POST /a/{token}/{action} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();

    // RST の有無は OS によるため、本文を送る前に応答・切断しないことを確かめる。
    let mut first_byte = [0u8; 1];
    let early =
        tokio::time::timeout(Duration::from_millis(100), stream.read(&mut first_byte)).await;
    assert!(
        early.is_err(),
        "{action} answered or closed before the request body arrived: {early:?}"
    );

    stream.write_all(body).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("the agent API never finished the response after receiving the body")
        .unwrap();
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("the agent API returned incomplete response headers");
    let head = std::str::from_utf8(&response[..split]).unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let length: usize = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .expect("the JSON response has no content length")
        .1
        .trim()
        .parse()
        .unwrap();
    let body = &response[split + 4..];
    assert_eq!(body.len(), length, "{action} returned an incomplete body");
    (status, serde_json::from_slice(body).unwrap())
}

/// 接続が閉じるまで読んだ応答の状態コード。閉じないまま止まったら、待ち続けずに失敗させる。
async fn response_status(stream: &mut tokio::net::TcpStream) -> u16 {
    use tokio::io::AsyncReadExt;
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("the agent API never closed the connection")
        .unwrap();
    let text = String::from_utf8_lossy(&response);
    text.split(' ').nth(1).unwrap().parse().unwrap()
}

fn explanation_sections() -> Value {
    json!([{"id":"overview","title":"Overview","blocks":[{"id":"intent","part":{"type":"text","body":"**Intent**"},"references":[]}]}])
}

async fn file_body_boundary(op: &str) {
    let server = AgentServer::start().await;
    let comment = server.page.add_comment_with_body(11, "question").await;
    let review: Value = server.page.get("api/review").await.json().await.unwrap();
    let request = |body: &str| match op {
        "add" => {
            json!({"op":"add","file_id":review["groups"][0]["files"][0]["id"],"side":"new","body":body})
        }
        "message" => json!({"body":body}),
        _ => json!({"op":op,"id":comment["id"],"body":body}),
    };
    let endpoint = if op == "message" {
        "api/message"
    } else {
        "api/comment"
    };
    let boundary = "あ".repeat(21_845) + "x";
    assert_eq!(
        server
            .page
            .post(endpoint, request(&boundary))
            .await
            .status(),
        200
    );
    let before: Value = server.page.get("api/review").await.json().await.unwrap();
    let saved = server.sink.last_state();
    let rejected = server.page.post(endpoint, request(&(boundary + "x"))).await;
    assert_eq!(rejected.status(), 413);
    assert!(rejected.text().await.unwrap().contains("body"));
    let after: Value = server.page.get("api/review").await.json().await.unwrap();
    assert_eq!(before["comments"], after["comments"]);
    assert_eq!(before["messages"], after["messages"]);
    assert_eq!(before["agent"], after["agent"]);
    assert_eq!(server.sink.last_state(), saved);
    assert_eq!(
        server.page.post(endpoint, request("next")).await.status(),
        200
    );
    let next = server.sink.last_state();
    assert_eq!(next.last_seq, saved.last_seq + u32::from(op != "edit"));
    assert_eq!(
        next.last_comment,
        saved.last_comment + u32::from(op == "add")
    );
    assert_eq!(next.last_reply, saved.last_reply + u32::from(op == "reply"));
    assert_eq!(
        next.last_message,
        saved.last_message + u32::from(op == "message")
    );
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

struct GatedContentSource {
    inner: Arc<FakeSource>,
    gate: Arc<GenerationGate>,
    blocked: AtomicBool,
}
impl ReviewSource for GatedContentSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }
    fn content(&self, id: &str) -> Result<FileContent, SourceError> {
        if self.blocked.load(Ordering::SeqCst) {
            self.gate.entered.notify_one();
            let _ = self.gate.release.lock().unwrap().recv();
        }
        self.inner.content(id)
    }
}

#[path = "server/conversation.rs"]
mod conversation_cases;

#[path = "server/explanation.rs"]
mod explanation_cases;

#[path = "server/files.rs"]
mod files_cases;

#[path = "server/http.rs"]
mod http_cases;

#[path = "server/persistence.rs"]
mod persistence_cases;

#[path = "server/submission.rs"]
mod submission_cases;

#[path = "server/units.rs"]
mod units_cases;

#[path = "server/watch.rs"]
mod watch_cases;
