//! HTTP / SSE サーバの契約テスト（R-SERVE, R-SUBMIT, R-COMMENT）。

#![expect(
    clippy::unwrap_used,
    reason = "#[test] の外の補助関数も、失敗をそのままテストの失敗として見せる"
)]

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use kemi_core::domain::agent::{AgentEvent, Handed, HandedComment};
use kemi_core::domain::content::AUTO_MAX_BYTES;
use kemi_core::domain::focus::FocusTargets;
use kemi_core::domain::review::{Approval, FileEntry, Group, ReviewMeta, Side, Status};
use kemi_core::session::{
    SessionCopy, SessionError, SessionInfo, SessionMode, SessionState, SessionStore,
};
use kemi_core::source::git::{GitMode, GitSource, GroupBy, UNTRACKED_LIMIT};
use kemi_core::source::manifest::ManifestSource;
use kemi_core::source::{FileContent, FileOrigin, FocusSource, ReviewSource, SourceError};
use kemi_server::{
    AgentParams, Asset, Assets, Notice, NoticeSink, ResultSaveError, ResultSink, ServeOutcome,
    ServeParams, SessionSink, bind_agent_listener, serve, session_host, session_url,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

mod common;
use common::http;

struct FakeSource {
    meta: ReviewMeta,
    contents: HashMap<String, FileContent>,
    reads: AtomicUsize,
    content_delay_ms: AtomicUsize,
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
    notices: Arc<RecordingNotices>,
    task: JoinHandle<Result<ServeOutcome, kemi_server::ServerError>>,
}

/// テストのサーバの起動の仕方。指定しないものは既定（ループバックで待ち受け、セッション・
/// 結果ファイル・エージェント・起動時の説明は無く、壁時計を読む）。
struct Launch {
    source: Arc<dyn ReviewSource>,
    bind: Ipv4Addr,
    share_address: Option<Ipv4Addr>,
    session: Option<Arc<dyn SessionSink>>,
    results: Option<Arc<dyn ResultSink>>,
    agent: Option<AgentParams>,
    initial_explanation: Option<kemi_core::domain::explanation::ExplanationDocument>,
    clock: kemi_server::Clock,
}

impl Launch {
    fn new(source: Arc<dyn ReviewSource>) -> Self {
        Launch {
            source,
            bind: Ipv4Addr::LOCALHOST,
            share_address: None,
            session: None,
            results: None,
            agent: None,
            initial_explanation: None,
            clock: kemi_server::system_clock(),
        }
    }
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
        TestServer::start_source(Arc::new(FakeSource::new())).await
    }

    /// wildcard バインドや共有アドレスを再現する。url はテストが繋げる loopback のまま
    /// （表示 URL の組み立ては `session_url` / `session_host` を直接検証する）。
    async fn start_bound(bind: Ipv4Addr, share_address: Option<Ipv4Addr>) -> Self {
        TestServer::launch(Launch {
            bind,
            share_address,
            ..Launch::new(Arc::new(FakeSource::new()))
        })
        .await
    }

    async fn start_source(source: Arc<dyn ReviewSource>) -> Self {
        TestServer::launch(Launch::new(source)).await
    }

    async fn launch(launch: Launch) -> Self {
        let listener = TcpListener::bind((launch.bind, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = session_url(&listener, "test-token", Ipv4Addr::LOCALHOST).unwrap();
        let origin = url.split("/s/").next().unwrap().to_string();
        let notices = Arc::new(RecordingNotices::default());
        let params = ServeParams {
            source: launch.source,
            assets: Arc::new(FakeAssets),
            token: "test-token".to_string(),
            results: launch.results,
            session: launch.session,
            notices: notices.clone(),
            share_address: launch.share_address,
            agent: launch.agent,
            live: None,
            initial_explanation: launch.initial_explanation,
            clock: launch.clock,
        };
        let task = tokio::spawn(serve(listener, params));
        TestServer {
            url,
            origin,
            port,
            notices,
            task,
        }
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        http()
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
        match tokio::time::timeout(WAIT_LIMIT, self.task)
            .await
            .expect("server did not stop")
            .expect("server task panicked")
            .expect("server returned error")
        {
            ServeOutcome::Submitted(document) => document,
        }
    }
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
    // サーバが応答しない退行でも止まり続けない。
    stream.set_read_timeout(Some(WAIT_LIMIT)).unwrap();
    stream.set_write_timeout(Some(WAIT_LIMIT)).unwrap();
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
    http()
        .post(format!("{url}{path}"))
        .header("Origin", origin)
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// 待ちの上限。止まらないためだけのもので、順序は決めない。通る実行の時間は変わらないので、
/// 負荷で相手が遅れても偽に落ちないよう長めに取る。
const WAIT_LIMIT: Duration = Duration::from_secs(10);

/// `look` で読んだ値が `done` を満たすまで、間を置いて読み直す。`WAIT_LIMIT` を過ぎたら、
/// 最後に読んだ値を出して失敗させる。
async fn eventually<T: std::fmt::Debug>(
    what: &str,
    mut look: impl AsyncFnMut() -> T,
    done: impl Fn(&T) -> bool,
) -> T {
    let deadline = std::time::Instant::now() + WAIT_LIMIT;
    loop {
        let seen = look().await;
        if done(&seen) {
            return seen;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what} did not happen within {WAIT_LIMIT:?}; last seen: {seen:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// 相手の要求（`peer`）が `reached` の合図を出すまで待つ。合図の前に相手が終わったら
/// （届く前に誤りで返る退行など）、その結果を出して失敗させ、合図を永久に待たない。
async fn until_reached<T: std::fmt::Debug>(
    what: &str,
    reached: impl Future<Output = ()>,
    peer: &mut JoinHandle<T>,
) {
    tokio::select! {
        () = reached => {}
        result = peer => panic!("the request ended before {what}: {result:?}"),
        () = tokio::time::sleep(WAIT_LIMIT) => {
            panic!("{what} did not happen within {WAIT_LIMIT:?}");
        }
    }
}

// ---- R-LIVE（監視）の結合テスト ----

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

impl TestServer {
    async fn start_watched(source: Arc<dyn ReviewSource>) -> Self {
        let server = TestServer::start_source(source).await;
        // 監視スレッドがパスを登録するのを待つ。
        tokio::time::sleep(Duration::from_millis(400)).await;
        server
    }

    fn stop(self) {
        self.task.abort();
    }
}

// ---- R-UNIT（2 つのグループ単位）と R-ORIGIN の取得 ----

impl TestServer {
    async fn get_json(&self, path: &str) -> Value {
        let response = self.get(path).await;
        assert_eq!(response.status(), 200, "GET {path}");
        response.json().await.unwrap()
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

    async fn submit_comments(self) -> Vec<Value> {
        let response = self
            .post("api/submit", json!({"verdict": "approved"}))
            .await;
        assert_eq!(response.status(), 200);
        self.finish().await["comments"].as_array().unwrap().clone()
    }
}

// ---- R-SESSION（セッションの保存と凍結、submit での削除） ----

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
    async fn start_with_session(source: Arc<dyn ReviewSource>, sink: Arc<dyn SessionSink>) -> Self {
        TestServer::launch(Launch {
            session: Some(sink),
            ..Launch::new(source)
        })
        .await
    }
}

// ---- R-AGENT-HAND（ページ側の API: 渡す・返信・解決・発言と、その通知） ----

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

/// SSE を開き、`event: <name>` の行の後の data を 1 つ読む。
async fn next_sse_event(response: &mut reqwest::Response, name: &str) -> Value {
    let needle = format!("event: {name}\ndata: ");
    let mut buffer = String::new();
    let deadline = tokio::time::Instant::now() + WAIT_LIMIT;
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
        AgentServer::start_with(Arc::new(FakeSource::new())).await
    }

    async fn start_with(source: Arc<dyn ReviewSource>) -> Self {
        AgentServer::start_with_sink(source, Arc::new(RecordingSink::default())).await
    }

    /// 保留したレビューを `--resume` で開き直したように、保存した状態から始める。
    async fn resume(saved: SessionState) -> Self {
        let sink = Arc::new(RecordingSink::default());
        *sink.initial.lock().unwrap() = Some(saved);
        AgentServer::start_with_sink(Arc::new(FakeSource::new()), sink).await
    }

    async fn start_with_sink(source: Arc<dyn ReviewSource>, sink: Arc<RecordingSink>) -> Self {
        AgentServer::launch(Launch::new(source), sink).await
    }

    /// エージェントの状態の時刻を `clock` から読むサーバ。写しは既にある扱いにして、凍結の
    /// 裏の読み取りを動かさない。
    async fn start_with_clock(clock: kemi_server::Clock) -> Self {
        let sink = Arc::new(RecordingSink::default());
        sink.copy_already_saved.store(true, Ordering::SeqCst);
        AgentServer::launch(
            Launch {
                clock,
                ..Launch::new(Arc::new(FakeSource::new()))
            },
            sink,
        )
        .await
    }

    /// `launch` に、`sink` を保存先にしてエージェント用のリスナーを足したサーバ。
    async fn launch(launch: Launch, sink: Arc<RecordingSink>) -> Self {
        let agent_listener = bind_agent_listener().await.unwrap();
        let agent_port = agent_listener.local_addr().unwrap().port();
        let control = kemi_server::ServeControl::new();
        let page = TestServer::launch(Launch {
            session: Some(sink.clone()),
            agent: Some(AgentParams {
                listener: agent_listener,
                token: "agent-secret".to_string(),
                review: "01TESTREVIEW".to_string(),
                control: control.clone(),
            }),
            ..launch
        })
        .await;
        AgentServer {
            page,
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
        http()
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
        eventually(
            &format!("the agent status becoming {expected}"),
            async || self.agent_status().await,
            |status| status == expected,
        )
        .await;
    }
}

/// `kemi wait` と同じに、待って、起きたことを受け取りきったら受け取りを知らせる。
/// 応答の (状態コード, JSON)。
async fn wait_like_the_cli(port: u16, token: String, timeout_ms: Option<u64>) -> (u16, Value) {
    let client = http();
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
    tokio::time::timeout(WAIT_LIMIT, stream.read_to_end(&mut response))
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
    tokio::time::timeout(WAIT_LIMIT, stream.read_to_end(&mut response))
        .await
        .expect("the agent API never closed the connection")
        .unwrap();
    let text = String::from_utf8_lossy(&response);
    text.split(' ').nth(1).unwrap().parse().unwrap()
}

/// 内容の取得（`content`）を、`blocked` の間は合図があるまで止める。
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
