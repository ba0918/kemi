//! `--live` の中継の契約テスト（R-PAGE-PROXY）。開発サーバの代わりに、受け取った要求を
//! 返す最小のサーバを相手にする。

#![expect(
    clippy::unwrap_used,
    reason = "#[test] の外の補助関数も、失敗をそのままテストの失敗として見せる"
)]

use std::borrow::Cow;
use std::net::Ipv4Addr;
use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use kemi_core::domain::review::ReviewMeta;
use kemi_core::source::{FileContent, ReviewSource, SourceError};
use kemi_server::{Asset, Assets, LiveParams, LiveTarget, Notice, NoticeSink, ServeParams, serve};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const RELAY: &str = "relay-secret";

struct EmptySource;

impl ReviewSource for EmptySource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        Ok(ReviewMeta {
            title: "Live review of the test page".to_string(),
            subtitle: String::new(),
            meta: serde_json::Value::Null,
            groups: Vec::new(),
            approval: Vec::new(),
        })
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        Err(SourceError::UnknownFileId(file_id.to_string()))
    }
}

struct PageAssets;

impl Assets for PageAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        match path {
            "index.html" => Some(Asset {
                bytes: Cow::Borrowed(b"<!doctype html><title>kemi</title>"),
                mime: "text/html; charset=utf-8",
            }),
            "live/page.js" => Some(Asset {
                bytes: Cow::Borrowed(b"/* page script */"),
                mime: "text/javascript; charset=utf-8",
            }),
            _ => None,
        }
    }
}

struct Quiet;

impl NoticeSink for Quiet {
    fn notify(&self, _notice: Notice) {}
}

/// 開発サーバの代わり。`/` は枠を拒むヘッダを付けた HTML、`/cookies` は受け取った
/// Cookie ヘッダを返す。
async fn start_dev_server() -> (String, JoinHandle<()>) {
    async fn page() -> Response {
        (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::X_FRAME_OPTIONS, "DENY"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "frame-ancestors 'none'; script-src 'self'",
                ),
            ],
            "<!doctype html><html><head><title>dev</title></head><body>dev page</body></html>",
        )
            .into_response()
    }
    async fn cookies(headers: HeaderMap) -> String {
        headers
            .get(header::COOKIE)
            .map(|value| value.to_str().unwrap().to_string())
            .unwrap_or_else(|| "(none)".to_string())
    }
    let app = Router::new()
        .route("/", get(page))
        .route("/cookies", get(cookies));
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let authority = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (authority, task)
}

struct Running {
    review: String,
    live: String,
    live_port: u16,
    task: JoinHandle<Result<kemi_server::ServeOutcome, kemi_server::ServerError>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn start_review(authority: &str) -> Running {
    start_review_of(LiveTarget::Url {
        authority: authority.to_string(),
        start: "/".to_string(),
        display: format!("http://{authority}/"),
    })
    .await
}

async fn start_review_of(target: LiveTarget) -> Running {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let review_port = listener.local_addr().unwrap().port();
    let live_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let live_port = live_listener.local_addr().unwrap().port();
    let params = ServeParams {
        source: Arc::new(EmptySource),
        assets: Arc::new(PageAssets),
        token: "test-token".to_string(),
        results: None,
        session: None,
        notices: Arc::new(Quiet),
        share_address: None,
        agent: None,
        live: Some(LiveParams {
            listener: live_listener,
            target,
            cookie: RELAY.to_string(),
            code_view: false,
        }),
    };
    let task = tokio::spawn(serve(listener, params));
    Running {
        review: format!("http://127.0.0.1:{review_port}/s/test-token/"),
        live: format!("http://127.0.0.1:{live_port}"),
        live_port,
        task,
    }
}

fn relay_cookie(running: &Running) -> String {
    format!("kemi_live_{}={RELAY}", running.live_port)
}

#[tokio::test]
async fn opening_the_review_url_sets_an_http_only_strict_relay_cookie() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let response = reqwest::get(&running.review).await.unwrap();

    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with(&relay_cookie(&running)), "{cookie}");
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=Strict"), "{cookie}");
}

#[tokio::test]
async fn a_request_without_the_relay_cookie_is_refused() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let client = reqwest::Client::new();

    let without = client.get(&running.live).send().await.unwrap();
    let wrong = client
        .get(&running.live)
        .header(
            header::COOKIE,
            format!("kemi_live_{}=guess", running.live_port),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(without.status(), StatusCode::FORBIDDEN);
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_relay_cookie_is_removed_before_the_dev_server_sees_the_request() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let body = reqwest::Client::new()
        .get(format!("{}/cookies", running.live))
        .header(
            header::COOKIE,
            format!("theme=dark; {}", relay_cookie(&running)),
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(body, "theme=dark");
}

#[tokio::test]
async fn a_page_that_refuses_frames_is_rewritten_and_gets_the_page_script() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let response = reqwest::Client::new()
        .get(&running.live)
        .header(header::COOKIE, relay_cookie(&running))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::X_FRAME_OPTIONS).is_none());
    let policy = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let review_origin = running.review.trim_end_matches("/s/test-token/");
    assert!(
        policy.contains(&format!("frame-ancestors {review_origin}")),
        "{policy}"
    );
    assert!(
        policy.contains(&format!(
            "script-src 'self' {}/__kemi/page.js",
            running.live
        )),
        "{policy}"
    );
    let body = response.text().await.unwrap();
    assert!(body.contains(r#"<script src="/__kemi/page.js""#), "{body}");
    assert!(body.contains("dev page"), "{body}");
}

#[tokio::test]
async fn the_page_script_is_served_on_the_relay_port_only_with_the_cookie() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let client = reqwest::Client::new();

    let with = client
        .get(format!("{}/__kemi/page.js", running.live))
        .header(header::COOKIE, relay_cookie(&running))
        .send()
        .await
        .unwrap();
    let without = client
        .get(format!("{}/__kemi/page.js", running.live))
        .send()
        .await
        .unwrap();

    assert_eq!(with.status(), StatusCode::OK);
    assert_eq!(with.text().await.unwrap(), "/* page script */");
    assert_eq!(without.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unreachable_dev_server_shows_a_waiting_page() {
    // 立ててすぐ閉じたポートには何も居ない。
    let closed = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let authority = format!("127.0.0.1:{}", closed.local_addr().unwrap().port());
    drop(closed);
    let running = start_review(&authority).await;

    let response = reqwest::Client::new()
        .get(&running.live)
        .header(header::COOKIE, relay_cookie(&running))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = response.text().await.unwrap();
    assert!(body.contains("kemi cannot reach"), "{body}");
    assert!(body.contains(r#"<script src="/__kemi/page.js""#), "{body}");
}

#[tokio::test]
async fn the_review_tells_the_page_where_the_relay_listens() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let review: serde_json::Value = reqwest::get(format!("{}api/review", running.review))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(review["live"]["port"], running.live_port);
    assert_eq!(review["live"]["start"], "/");
    assert_eq!(review["live"]["page"], format!("http://{authority}/"));
    assert_eq!(review["live"]["code"], false);
}

// ---- 手元の HTML ファイル（R-PAGE-MODE の <ファイル>） ----

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("kemi-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Scratch(std::fs::canonicalize(path).unwrap())
    }

    fn write(&self, relative: &str, content: &str) {
        let full = self.0.join(relative);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn start_file_review(root: &Scratch, path: &str) -> Running {
    start_review_of(LiveTarget::File {
        root: root.0.clone(),
        path: path.to_string(),
    })
    .await
}

async fn get_with_cookie(running: &Running, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}{path}", running.live))
        .header(header::COOKIE, relay_cookie(running))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_file_page_is_served_with_the_page_script_and_its_css() {
    let root = Scratch::new("served");
    root.write(
        "docs/page.html",
        "<html><head><link rel=stylesheet href=style.css></head><body>file page</body></html>",
    );
    root.write("docs/style.css", "p { color: red; }");
    let running = start_file_review(&root, "docs/page.html").await;

    let page = get_with_cookie(&running, "/docs/page.html").await;
    let css = get_with_cookie(&running, "/docs/style.css").await;

    assert_eq!(page.status(), StatusCode::OK);
    let body = page.text().await.unwrap();
    assert!(body.contains("file page"), "{body}");
    assert!(body.contains(r#"<script src="/__kemi/page.js""#), "{body}");
    assert_eq!(css.status(), StatusCode::OK);
    assert!(
        css.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/css")
    );
    assert_eq!(css.text().await.unwrap(), "p { color: red; }");
}

#[tokio::test]
async fn a_file_review_needs_the_relay_cookie_too() {
    let root = Scratch::new("cookie");
    root.write("page.html", "<p>file page</p>");
    let running = start_file_review(&root, "page.html").await;

    let response = reqwest::get(format!("{}/page.html", running.live))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn paths_outside_the_range_and_inside_dot_git_are_not_served() {
    let root = Scratch::new("range");
    root.write("site/page.html", "<p>inside</p>");
    root.write("site/.git/config", "[core]");
    root.write("secret.html", "<p>secret</p>");
    let range = Scratch(root.0.join("site"));
    let running = start_file_review(&range, "page.html").await;

    for path in [
        "/%2e%2e/secret.html",
        "/..%2fsecret.html",
        "/.git/config",
        "/missing.css",
    ] {
        let response = get_with_cookie(&running, path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        let body = response.text().await.unwrap();
        assert!(
            !body.contains("<p>secret</p>") && !body.contains("[core]"),
            "{path}: {body}"
        );
    }
    std::mem::forget(range);
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_whose_target_is_outside_the_range_is_not_served() {
    let root = Scratch::new("link");
    root.write("site/page.html", "<p>inside</p>");
    root.write("secret.html", "<p>secret</p>");
    std::os::unix::fs::symlink(root.0.join("secret.html"), root.0.join("site/link.html")).unwrap();
    let range = Scratch(root.0.join("site"));
    let running = start_file_review(&range, "page.html").await;

    let response = get_with_cookie(&running, "/link.html").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    std::mem::forget(range);
}

#[tokio::test]
async fn a_missing_file_page_says_so_and_waits() {
    let root = Scratch::new("missing");
    let running = start_file_review(&root, "gone.html").await;

    let response = get_with_cookie(&running, "/gone.html").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response.text().await.unwrap();
    assert!(body.contains("kemi cannot read /gone.html"), "{body}");
    assert!(body.contains("/__kemi/alive"), "{body}");
}

// ---- スナップショット（R-PAGE-SNAPSHOT） ----

async fn post_review(running: &Running, path: &str, body: serde_json::Value) -> reqwest::Response {
    let origin = running
        .review
        .trim_end_matches("/s/test-token/")
        .to_string();
    reqwest::Client::new()
        .post(format!("{}{path}", running.review))
        .header(header::ORIGIN, origin)
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn get_review_json(running: &Running, path: &str) -> serde_json::Value {
    reqwest::get(format!("{}{path}", running.review))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_snapshot_is_kept_listed_and_returned() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let taken = post_review(
        &running,
        "api/snapshot",
        serde_json::json!({ "page": "/", "width": 1280, "kind": "start", "html": "<p>then</p>" }),
    )
    .await;
    assert_eq!(taken.status(), StatusCode::OK);
    let taken: serde_json::Value = taken.json().await.unwrap();
    let list = get_review_json(&running, "api/snapshots").await;
    let one = get_review_json(
        &running,
        &format!("api/snapshot/{}", taken["id"].as_str().unwrap()),
    )
    .await;

    assert_eq!(taken["page"], "/");
    assert_eq!(taken["width"], 1280);
    assert_eq!(taken["kind"], "start");
    assert_eq!(list["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(list["snapshots"][0]["id"], taken["id"]);
    assert!(
        list["snapshots"][0].get("html").is_none(),
        "the list carries no bodies"
    );
    assert_eq!(one["html"], "<p>then</p>");
}

#[tokio::test]
async fn a_snapshot_over_two_megabytes_is_refused_and_not_kept() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let refused = post_review(
        &running,
        "api/snapshot",
        serde_json::json!({ "page": "/", "width": 1280, "kind": "manual", "html": "x".repeat(2 * 1024 * 1024 + 1) }),
    )
    .await;
    let list = get_review_json(&running, "api/snapshots").await;

    assert_eq!(refused.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(list["snapshots"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_snapshot_of_an_unknown_kind_is_refused() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let refused = post_review(
        &running,
        "api/snapshot",
        serde_json::json!({ "page": "/", "width": 1280, "kind": "later", "html": "<p></p>" }),
    )
    .await;

    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn handing_to_the_agent_asks_the_page_for_a_snapshot() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let mut events = reqwest::get(format!("{}api/events", running.review))
        .await
        .unwrap();
    post_review(
        &running,
        "api/message",
        serde_json::json!({ "body": "look" }),
    )
    .await;

    let handed = post_review(&running, "api/hand", serde_json::json!({})).await;
    assert_eq!(handed.status(), StatusCode::OK);

    let mut buffer = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !buffer.contains("event: handed") {
        let chunk = tokio::time::timeout_at(deadline, events.chunk())
            .await
            .expect("no handed event")
            .unwrap()
            .expect("the event stream ended");
        buffer.push_str(&String::from_utf8_lossy(&chunk));
    }
}
