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
use kemi_core::session::{FilesWrite, OpenSession, PageSnapshot, SessionError, SnapshotKind};
use kemi_core::source::{FileContent, ReviewSource, SourceError};
use kemi_server::{
    Asset, Assets, LiveParams, LiveTarget, Notice, NoticeSink, ServeParams, SessionSink, serve,
};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const RELAY: &str = "relay-secret";
const MOCK_SECRET: &str = "mock-secret";

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

/// 開発サーバの代わり。`/` は枠を拒むヘッダを付けた HTML、`/nonce` は nonce と
/// 'strict-dynamic' でスクリプトを縛る HTML、`/cookies` は受け取った Cookie ヘッダを返す。
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
    async fn cookies(headers: HeaderMap) -> Vec<u8> {
        headers
            .get(header::COOKIE)
            .map(|value| value.as_bytes().to_vec())
            .unwrap_or_else(|| b"(none)".to_vec())
    }
    async fn nonce_page() -> Response {
        (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "script-src 'nonce-dev' 'strict-dynamic'",
                ),
            ],
            "<!doctype html><html><head></head><body>nonce page</body></html>",
        )
            .into_response()
    }
    let app = Router::new()
        .route("/", get(page))
        .route("/nonce", get(nonce_page))
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
    start_review_of(
        LiveTarget::Url {
            authority: authority.to_string(),
            start: "/".to_string(),
            display: format!("http://{authority}/"),
        },
        std::env::temp_dir(),
    )
    .await
}

async fn start_review_of(target: LiveTarget, root: std::path::PathBuf) -> Running {
    start_review_with_session(target, root, None).await
}

async fn start_review_with_session(
    target: LiveTarget,
    root: std::path::PathBuf,
    session: Option<Arc<dyn SessionSink>>,
) -> Running {
    start_restored_review(target, root, session, Vec::new(), 0).await
}

/// 復元したレビューのように、保存してあったスナップショットを持って始める。
async fn start_restored_review(
    target: LiveTarget,
    root: std::path::PathBuf,
    session: Option<Arc<dyn SessionSink>>,
    snapshots: Vec<PageSnapshot>,
    last_snapshot_number: u32,
) -> Running {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let review_port = listener.local_addr().unwrap().port();
    let live_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let live_port = live_listener.local_addr().unwrap().port();
    let params = ServeParams {
        source: Arc::new(EmptySource),
        assets: Arc::new(PageAssets),
        token: "test-token".to_string(),
        results: None,
        session,
        notices: Arc::new(Quiet),
        share_address: None,
        agent: None,
        live: Some(LiveParams {
            listener: live_listener,
            target,
            root,
            cookie: RELAY.to_string(),
            mock_secret: MOCK_SECRET.to_string(),
            code_view: false,
            snapshots,
            last_snapshot_number,
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
async fn other_cookies_with_non_ascii_bytes_keep_the_relay_open_and_reach_the_dev_server_unchanged()
{
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let mut cookie = "name=caf\u{e9}; ".as_bytes().to_vec();
    cookie.extend_from_slice(relay_cookie(&running).as_bytes());

    let response = reqwest::Client::new()
        .get(format!("{}/cookies", running.live))
        .header(
            header::COOKIE,
            reqwest::header::HeaderValue::from_bytes(&cookie).unwrap(),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        "name=caf\u{e9}".as_bytes()
    );
}

#[tokio::test]
async fn a_page_that_refuses_frames_is_rewritten_and_gets_the_page_script() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let response = open_as_page(&running, "/").await;

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
    // 開発サーバの script-src 'self' に、差し込むスクリプトの 1 つだけが足される。
    let script_src: Vec<&str> = policy
        .split(';')
        .map(str::trim)
        .find_map(|directive| directive.strip_prefix("script-src "))
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(script_src.len(), 2, "{policy}");
    assert_eq!(script_src[0], "'self'", "{policy}");
    let body = response.text().await.unwrap();
    assert_eq!(body.matches("<script").count(), 1, "{body}");
    assert!(body.contains("dev page"), "{body}");
}

/// `headers` を付けて中継の `path` を取る。
async fn get_with(running: &Running, path: &str, headers: &[(&str, &str)]) -> String {
    let mut request = reqwest::Client::new()
        .get(format!("{}{path}", running.live))
        .header(header::COOKIE, relay_cookie(running));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send().await.unwrap().text().await.unwrap()
}

/// `Sec-Fetch-Dest` を付けて中継の `path` を取る（ブラウザが要求の行き先に付ける見出し）。
async fn get_as(running: &Running, path: &str, dest: &str) -> String {
    get_with(running, path, &[("sec-fetch-dest", dest)]).await
}

#[tokio::test]
async fn html_fetched_by_the_page_scripts_is_relayed_without_the_page_script() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let fetched = get_as(&running, "/", "empty").await;
    let document = get_as(&running, "/", "document").await;
    let framed = get_as(&running, "/", "iframe").await;

    assert_eq!(
        fetched,
        "<!doctype html><html><head><title>dev</title></head><body>dev page</body></html>"
    );
    assert!(
        document.contains(r#"<script src="/__kemi/page.js""#),
        "{document}"
    );
    assert!(
        framed.contains(r#"<script src="/__kemi/page.js""#),
        "{framed}"
    );
}

/// ブラウザは `Sec-Fetch-Dest` を安全なオリジン（localhost など）にだけ送るので、`--bind` で
/// LAN のアドレスから平文の HTTP で開くと、移動にも fetch にも付かない。
#[tokio::test]
async fn without_sec_fetch_dest_only_a_navigation_gets_the_page_script() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let fetched = get_with(&running, "/", &[("accept", "*/*")]).await;
    let navigated = get_with(
        &running,
        "/",
        &[
            ("upgrade-insecure-requests", "1"),
            (
                "accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        ],
    )
    .await;

    assert_eq!(
        fetched,
        "<!doctype html><html><head><title>dev</title></head><body>dev page</body></html>"
    );
    assert!(
        navigated.contains(r#"<script src="/__kemi/page.js""#),
        "{navigated}"
    );
}

#[tokio::test]
async fn a_page_that_allows_scripts_by_nonce_lets_the_page_script_run_by_its_own_nonce() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let response = open_as_page(&running, "/nonce").await;

    let policy = response.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_string();
    let body = response.text().await.unwrap();
    let nonce = policy
        .split_whitespace()
        .filter_map(|source| source.strip_prefix("'nonce-")?.strip_suffix('\''))
        .find(|nonce| *nonce != "dev")
        .unwrap_or_else(|| panic!("no nonce for the page script: {policy}"));
    assert!(body.contains(&format!(r#"nonce="{nonce}""#)), "{body}");
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
    start_review_of(
        LiveTarget::File {
            path: path.to_string(),
        },
        root.0.clone(),
    )
    .await
}

/// ブラウザが文書として開くときと同じく `Sec-Fetch-Dest: document` を付けて取る。
async fn open_as_page(running: &Running, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}{path}", running.live))
        .header(header::COOKIE, relay_cookie(running))
        .header("sec-fetch-dest", "document")
        .send()
        .await
        .unwrap()
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

    let page = open_as_page(&running, "/docs/page.html").await;
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
async fn an_html_file_fetched_by_the_page_scripts_is_served_without_the_page_script() {
    let root = Scratch::new("fetched");
    root.write("page.html", "<html><body>page</body></html>");
    root.write("part.html", "<p>part</p>");
    let running = start_file_review(&root, "page.html").await;

    let fetched = get_as(&running, "/part.html", "empty").await;
    let framed = get_as(&running, "/page.html", "iframe").await;

    assert_eq!(fetched, "<p>part</p>");
    assert!(
        framed.contains(r#"<script src="/__kemi/page.js""#),
        "{framed}"
    );
}

#[tokio::test]
async fn a_file_page_whose_name_needs_percent_encoding_opens_at_its_start_url() {
    let root = Scratch::new("encoded-page");
    root.write("50% #1 ページ.html", "<p>encoded page</p>");
    let running = start_file_review(&root, "50% #1 ページ.html").await;
    let review = get_review_json(&running, "api/review").await;

    let page = get_with_cookie(&running, review["live"]["start"].as_str().unwrap()).await;

    assert_eq!(page.status(), StatusCode::OK);
    assert!(page.text().await.unwrap().contains("encoded page"));
}

#[tokio::test]
async fn a_file_page_in_another_encoding_is_served_with_its_own_bytes_and_declaration() {
    let root = Scratch::new("shift-jis");
    // Shift_JIS の「日本」。UTF-8 としては読めない。
    let mut html = br#"<html><head><meta charset="shift_jis"></head><body>"#.to_vec();
    html.extend_from_slice(&[0x93, 0xfa, 0x96, 0x7b]);
    html.extend_from_slice(b"</body></html>");
    std::fs::write(root.0.join("page.html"), &html).unwrap();
    let running = start_file_review(&root, "page.html").await;

    let page = get_with_cookie(&running, "/page.html").await;

    let content_type = page.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(!content_type.contains("utf-8"), "{content_type}");
    let body = page.bytes().await.unwrap();
    assert!(
        body.windows(4)
            .any(|window| window == [0x93, 0xfa, 0x96, 0x7b]),
        "{body:?}"
    );
}

#[tokio::test]
async fn an_html_file_over_32_mb_opened_as_a_page_is_not_served_but_can_still_be_fetched() {
    let root = Scratch::new("large");
    let mut html = b"<html><body><p>large page</p>".to_vec();
    html.resize(32 * 1024 * 1024 + 1, b' ');
    std::fs::write(root.0.join("page.html"), &html).unwrap();
    let running = start_file_review(&root, "page.html").await;

    let page = open_as_page(&running, "/page.html").await;
    let fetched = get_as(&running, "/page.html", "empty").await;

    let body = page.text().await.unwrap();
    assert!(
        !body.contains("large page"),
        "{}",
        &body[..body.len().min(200)]
    );
    assert_eq!(fetched.len(), html.len());
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
async fn a_missing_file_page_is_answered_as_not_found() {
    let root = Scratch::new("missing");
    let running = start_file_review(&root, "gone.html").await;

    let response = get_with_cookie(&running, "/gone.html").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
async fn a_snapshot_keeps_the_element_description_and_returns_it_with_the_body() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let description = serde_json::json!({
        "width": 1280,
        "height": 800,
        "styles": [{ "color": "rgb(0, 0, 0)" }],
        "elements": [{ "parent": -1, "tag": "html", "id": "", "cls": "", "text": "", "box": [0, 0, 1280, 800], "style": 0 }],
    });

    let taken = post_review(
        &running,
        "api/snapshot",
        serde_json::json!({ "page": "/", "width": 1280, "kind": "manual", "html": "<p>then</p>", "description": description }),
    )
    .await;
    assert_eq!(taken.status(), StatusCode::OK);
    let taken: serde_json::Value = taken.json().await.unwrap();
    let one = get_review_json(
        &running,
        &format!("api/snapshot/{}", taken["id"].as_str().unwrap()),
    )
    .await;

    assert_eq!(one["description"], description);
}

#[tokio::test]
async fn the_two_megabyte_limit_counts_the_html_and_not_the_description() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let element = serde_json::json!({ "parent": 0, "tag": "div", "id": "", "cls": "", "text": "x".repeat(100), "box": [0, 0, 10, 10], "style": 0 });
    let description = serde_json::json!({
        "width": 1280,
        "height": 800,
        "styles": [{}],
        "elements": vec![element; 10_000],
    });

    let taken = post_review(
        &running,
        "api/snapshot",
        serde_json::json!({ "page": "/", "width": 1280, "kind": "manual", "html": "x".repeat(2 * 1024 * 1024 - 1024), "description": description }),
    )
    .await;

    assert_eq!(taken.status(), StatusCode::OK);
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

// ---- モック（live-compare.md の R-PAGE-MOCK） ----

#[tokio::test]
async fn a_mock_inside_the_range_is_assigned_to_a_page_and_listed() {
    let root = Scratch::new("mock-assign");
    root.write("page.html", "<p>page</p>");
    root.write("mocks/next.html", "<p>mock</p>");
    let running = start_file_review(&root, "page.html").await;

    let assigned = post_review(
        &running,
        "api/mock",
        serde_json::json!({ "page": "/page.html", "path": "mocks/next.html" }),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::OK);
    let assigned: serde_json::Value = assigned.json().await.unwrap();
    let list = get_review_json(&running, "api/mocks").await;

    assert_eq!(assigned["page"], "/page.html");
    assert_eq!(assigned["path"], "mocks/next.html");
    assert_eq!(list["mocks"][0]["page"], "/page.html");
}

#[tokio::test]
async fn a_mock_outside_the_range_or_not_html_is_refused_with_the_reason() {
    let root = Scratch::new("mock-refuse");
    root.write("site/page.html", "<p>page</p>");
    root.write("site/notes.txt", "notes");
    root.write("secret.html", "<p>secret</p>");
    let range = Scratch(root.0.join("site"));
    let running = start_file_review(&range, "page.html").await;

    for path in ["../secret.html", "notes.txt", "missing.html"] {
        let refused = post_review(
            &running,
            "api/mock",
            serde_json::json!({ "page": "/page.html", "path": path }),
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        let body: serde_json::Value = refused.json().await.unwrap();
        assert!(
            !body["error"].as_str().unwrap().trim().is_empty(),
            "{path}: {body}"
        );
    }
    let list = get_review_json(&running, "api/mocks").await;
    assert_eq!(list["mocks"].as_array().unwrap().len(), 0);
    std::mem::forget(range);
}

#[tokio::test]
async fn a_removed_mock_is_no_longer_listed() {
    let root = Scratch::new("mock-remove");
    root.write("page.html", "<p>page</p>");
    root.write("mock.html", "<p>mock</p>");
    let running = start_file_review(&root, "page.html").await;
    post_review(
        &running,
        "api/mock",
        serde_json::json!({ "page": "/page.html", "path": "mock.html" }),
    )
    .await;

    let removed = post_review(
        &running,
        "api/mock",
        serde_json::json!({ "page": "/page.html", "path": null }),
    )
    .await;

    assert_eq!(removed.status(), StatusCode::OK);
    let list = get_review_json(&running, "api/mocks").await;
    assert_eq!(list["mocks"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_mock_whose_name_needs_percent_encoding_is_served_at_its_url() {
    let root = Scratch::new("encoded-mock");
    root.write("page.html", "<p>page</p>");
    root.write("mocks/50% #1.html", "<p>encoded mock</p>");
    root.write("mocks/50%25.html", "<p>other mock</p>");
    let running = start_file_review(&root, "page.html").await;
    let base = running
        .review
        .trim_end_matches("/s/test-token/")
        .to_string();

    for (path, content) in [
        ("mocks/50% #1.html", "encoded mock"),
        ("mocks/50%25.html", "other mock"),
    ] {
        let assigned: serde_json::Value = post_review(
            &running,
            "api/mock",
            serde_json::json!({ "page": "/page.html", "path": path }),
        )
        .await
        .json()
        .await
        .unwrap();
        let mock = reqwest::get(format!("{base}{}", assigned["url"].as_str().unwrap()))
            .await
            .unwrap();

        assert_eq!(mock.status(), StatusCode::OK, "{path}");
        assert!(mock.text().await.unwrap().contains(content), "{path}");
    }
}

#[tokio::test]
async fn the_mock_file_list_shows_200_files_with_the_total_and_a_search_finds_the_others() {
    let root = Scratch::new("mock-files-limit");
    root.write("page.html", "<p>page</p>");
    for index in 0..200 {
        root.write(&format!("mocks/m{index:03}.html"), "<p>mock</p>");
    }
    root.write("mocks/notes.txt", "not html");
    let running = start_file_review(&root, "page.html").await;

    let all = get_review_json(&running, "api/mock-files?q=").await;
    let found = get_review_json(&running, "api/mock-files?q=PAGE").await;

    let files = all["files"].as_array().unwrap();
    assert_eq!(files.len(), 200);
    assert_eq!(all["total"], 201);
    assert_eq!(files[0], "mocks/m000.html");
    assert!(!files.contains(&serde_json::json!("page.html")));
    assert_eq!(found["files"], serde_json::json!(["page.html"]));
    assert_eq!(found["total"], 1);
}

#[tokio::test]
async fn the_mock_file_list_needs_the_page_token() {
    let root = Scratch::new("mock-files-token");
    root.write("page.html", "<p>page</p>");
    let running = start_file_review(&root, "page.html").await;
    let without = running.review.replace("/s/test-token/", "/s/wrong-token/");

    let refused = reqwest::get(format!("{without}api/mock-files?q="))
        .await
        .unwrap();

    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mock_files_are_served_without_the_token_or_the_cookie_inside_the_range_only() {
    let root = Scratch::new("mock-serve");
    root.write("site/page.html", "<p>page</p>");
    root.write(
        "site/mock.html",
        "<link rel=stylesheet href=mock.css><p>mock</p>",
    );
    root.write("site/mock.css", "p { color: green; }");
    root.write("site/.git/config", "[core]");
    root.write("secret.html", "<p>secret</p>");
    let range = Scratch(root.0.join("site"));
    let running = start_file_review(&range, "page.html").await;
    let base = running
        .review
        .trim_end_matches("/s/test-token/")
        .to_string();

    let page = reqwest::get(format!("{base}/m/{MOCK_SECRET}/mock.html"))
        .await
        .unwrap();
    let css = reqwest::get(format!("{base}/m/{MOCK_SECRET}/mock.css"))
        .await
        .unwrap();

    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        page.headers()[header::CONTENT_SECURITY_POLICY],
        "sandbox allow-scripts"
    );
    assert!(page.text().await.unwrap().contains("<p>mock</p>"));
    assert_eq!(css.status(), StatusCode::OK);
    for path in [
        format!("/m/{MOCK_SECRET}/..%2fsecret.html"),
        format!("/m/{MOCK_SECRET}/.git/config"),
        "/m/wrong-secret/mock.html".to_string(),
    ] {
        let refused = reqwest::get(format!("{base}{path}")).await.unwrap();
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "{path}");
    }
    std::mem::forget(range);
}

// ---- ページへのコメント（live.md の R-PAGE-COMMENT、kemi.md の R-SUBMIT の `page`） ----

fn page_place(n: u32, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "n": n,
        "kind": kind,
        "points": if kind == "element" { serde_json::json!([]) } else { serde_json::json!([{ "x": 10, "y": 20.5 }, { "x": 30, "y": 40 }]) },
        "elements": [{ "selector": "#buy", "text": "Buy", "rect": { "x": 0, "y": 120, "w": 200, "h": 60 } }],
    })
}

async fn add_page_comment(running: &Running, places: serde_json::Value) -> reqwest::Response {
    post_review(
        running,
        "api/comment",
        serde_json::json!({
            "op": "add_page",
            "page": { "url": "/products?x=1", "width": 390, "places": places },
            "body": "1 is too large",
        }),
    )
    .await
}

#[tokio::test]
async fn a_page_comment_is_added_in_the_page_group_with_its_places_in_number_order() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let added = add_page_comment(
        &running,
        serde_json::json!([page_place(2, "pen"), page_place(1, "element")]),
    )
    .await;

    assert_eq!(added.status(), StatusCode::OK);
    let comment: serde_json::Value = added.json().await.unwrap();
    assert_eq!(comment["group_id"], "page");
    assert_eq!(comment["group_title"], "Page");
    for key in ["path", "side", "start_line", "end_line", "suggestion"] {
        assert!(comment[key].is_null(), "{key}: {comment}");
    }
    assert_eq!(comment["quote"], serde_json::json!([]));
    assert_eq!(comment["outdated"], false);
    assert_eq!(comment["page"]["url"], "/products?x=1");
    assert_eq!(comment["page"]["width"], 390);
    assert_eq!(comment["page"]["places"][0]["n"], 1);
    assert_eq!(comment["page"]["places"][0]["kind"], "element");
    assert_eq!(
        comment["page"]["places"][0]["points"],
        serde_json::json!([])
    );
    assert_eq!(
        comment["page"]["places"][0]["elements"][0],
        serde_json::json!({ "selector": "#buy", "text": "Buy", "rect": { "x": 0.0, "y": 120.0, "w": 200.0, "h": 60.0 } })
    );
    assert_eq!(comment["page"]["places"][1]["n"], 2);
    assert_eq!(comment["page"]["places"][1]["kind"], "pen");
    assert_eq!(
        comment["page"]["places"][1]["points"],
        serde_json::json!([{ "x": 10.0, "y": 20.5 }, { "x": 30.0, "y": 40.0 }])
    );
    assert!(comment["page"]["image"].is_null());
    let review = get_review_json(&running, "api/review").await;
    assert_eq!(review["comments"][0]["page"], comment["page"]);
}

#[tokio::test]
async fn a_page_comment_whose_place_numbers_skip_one_is_refused() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    let refused = add_page_comment(
        &running,
        serde_json::json!([page_place(1, "element"), page_place(3, "pen")]),
    )
    .await;

    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let review = get_review_json(&running, "api/review").await;
    assert_eq!(review["comments"], serde_json::json!([]));
}

#[tokio::test]
async fn a_page_comment_without_a_place_with_a_repeated_number_or_an_unknown_kind_is_refused() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;

    for places in [
        serde_json::json!([]),
        serde_json::json!([page_place(1, "element"), page_place(1, "arrow")]),
        serde_json::json!([page_place(1, "circle")]),
    ] {
        let refused = add_page_comment(&running, places.clone()).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{places}");
        let message = refused.text().await.unwrap();
        assert!(message.is_ascii(), "{message}");
    }
    let review = get_review_json(&running, "api/review").await;
    assert_eq!(review["comments"], serde_json::json!([]));
}

#[tokio::test]
async fn editing_a_page_comment_changes_only_its_body() {
    let (authority, _dev) = start_dev_server().await;
    let running = start_review(&authority).await;
    let added: serde_json::Value =
        add_page_comment(&running, serde_json::json!([page_place(1, "arrow")]))
            .await
            .json()
            .await
            .unwrap();

    let edited = post_review(
        &running,
        "api/comment",
        serde_json::json!({
            "op": "edit", "id": added["id"], "body": "make it smaller",
            "page": { "url": "/other", "width": 1280, "places": [page_place(2, "pen")] },
        }),
    )
    .await;

    assert_eq!(edited.status(), StatusCode::OK);
    let edited: serde_json::Value = edited.json().await.unwrap();
    assert_eq!(edited["body"], "make it smaller");
    assert_eq!(edited["page"], added["page"]);
}

/// 割り込む要求。画像を書いた直後に別のスレッドで送り、応答の状態を返す。
type Interruption = Box<dyn FnOnce() -> std::sync::mpsc::Receiver<StatusCode> + Send>;

/// 実際のセッションに書く sink。画像を書いた直後に、割り込む要求を送って少し待つ。
struct InterruptedImageSink {
    open: std::sync::Mutex<kemi_core::session::OpenSession>,
    interruption: std::sync::Mutex<Option<Interruption>>,
    answer: std::sync::Mutex<Option<std::sync::mpsc::Receiver<StatusCode>>>,
    runtime: tokio::runtime::Handle,
}

impl SessionSink for InterruptedImageSink {
    fn describe_review(&self, _title: &str, _total_files: usize) {}

    fn save_state(
        &self,
        state: kemi_core::session::SessionState,
    ) -> Result<(), kemi_core::session::SessionError> {
        self.open.lock().unwrap().save_state(state)
    }

    fn save_copy(
        &self,
        copy: kemi_core::session::SessionCopy,
    ) -> Result<(), kemi_core::session::SessionError> {
        self.open.lock().unwrap().save_copy(copy)
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), kemi_core::session::SessionError> {
        self.open.lock().unwrap().mark_unresumable(reason)
    }

    fn delete(&self) -> Result<(), kemi_core::session::SessionError> {
        self.open.lock().unwrap().delete()
    }

    fn save_snapshot(
        &self,
        snapshot: &PageSnapshot,
        comments: &std::collections::BTreeSet<String>,
    ) -> FilesWrite {
        self.open.lock().unwrap().save_snapshot(snapshot, comments)
    }

    fn save_image(
        &self,
        comment_id: &str,
        bytes: &[u8],
        comments: &std::collections::BTreeSet<String>,
    ) -> FilesWrite {
        let written = self
            .open
            .lock()
            .unwrap()
            .save_image(comment_id, bytes, comments);
        let interruption = self.interruption.lock().unwrap().take();
        if let Some(interrupt) = interruption {
            let answer = interrupt();
            // この作業スレッドが止まっている間も、ほかの作業スレッドに入出力を見させる。
            // 何かを起こさないと、寝ている作業スレッドは割り込んだ要求に気づかない。この
            // スレッドから spawn すると止まっているこのスレッドに積まれるので、外から積む。
            let runtime = self.runtime.clone();
            std::thread::spawn(move || drop(runtime.spawn(async {})))
                .join()
                .unwrap();
            // 保存が画像の書き込みを待つなら、割り込んだ要求はこの間には終わらない。
            let answer = match answer.recv_timeout(std::time::Duration::from_millis(300)) {
                Ok(status) => {
                    let (send, again) = std::sync::mpsc::channel();
                    send.send(status).unwrap();
                    again
                }
                Err(_) => answer,
            };
            *self.answer.lock().unwrap() = Some(answer);
        }
        written
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_snapshot_taken_while_a_comment_image_is_written_does_not_take_it_for_a_deleted_comments_image()
 {
    use kemi_core::session::{SessionInfo, SessionMode, SessionStore};

    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("interrupted-image-sessions");
    let root = Scratch::new("interrupted-image");
    // 画像とスナップショットが並んでは入らない上限。画像を削除したコメントのものと
    // 見誤れば、スナップショットのために画像が消える。
    let open = SessionStore::new(sessions.0.clone())
        .with_files_limit(2000)
        .create(SessionInfo {
            id: "01HF7YAT00PAGE000000000000".to_string(),
            created: 1,
            updated: 1,
            workspace: root.0.clone(),
            workspace_key: "00000000000000aa".to_string(),
            mode: SessionMode::Live {
                page: kemi_core::domain::live::LivePage::Url(format!("http://{authority}/")),
                root: root.0.clone(),
            },
            title: "live".to_string(),
            total_files: 0,
        })
        .unwrap();
    let sink = Arc::new(InterruptedImageSink {
        open: std::sync::Mutex::new(open),
        interruption: std::sync::Mutex::new(None),
        answer: std::sync::Mutex::new(None),
        runtime: tokio::runtime::Handle::current(),
    });
    let running = start_review_with_session(
        LiveTarget::Url {
            authority: authority.clone(),
            start: "/".to_string(),
            display: format!("http://{authority}/"),
        },
        root.0.clone(),
        Some(sink.clone()),
    )
    .await;
    // 画像を書いた後、コメントが状態に入る前に、スナップショットを取る。
    let review = running.review.clone();
    let origin = review.trim_end_matches("/s/test-token/").to_string();
    *sink.interruption.lock().unwrap() = Some(Box::new(move || {
        let (send, answer) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let status = runtime.block_on(async {
                reqwest::Client::new()
                    .post(format!("{review}api/snapshot"))
                    .header(header::ORIGIN, origin)
                    .json(&serde_json::json!({
                        "page": "/", "width": 390, "kind": "manual",
                        "html": incompressible_text(1200, 1),
                    }))
                    .send()
                    .await
                    .unwrap()
                    .status()
            });
            send.send(status).unwrap();
        });
        answer
    }));

    let added = add_page_comment_with_image(&running, incompressible_image(1200)).await;

    let answer = sink.answer.lock().unwrap().take().unwrap();
    assert_eq!(answer.recv().unwrap(), StatusCode::OK);
    assert!(added.get("image_unsaved").is_none(), "{added}");
    let open = sink.open.lock().unwrap();
    let saved = &open.state().comments;
    let kemi_core::domain::review::CommentTarget::Page(page) = &saved[0].target else {
        panic!("{saved:?}");
    };
    let image = page.image.as_deref().unwrap();
    assert!(std::path::Path::new(image).exists(), "{image}");
}

// ---- スナップショットとコメントの画像の保存（R-PAGE-SESSION の 20 MB の規則） ----

/// 実際のセッションに書く sink。`<id>.files/` の上限を小さくして規則を確かめる。
struct StoreSink {
    open: std::sync::Mutex<OpenSession>,
}

impl SessionSink for StoreSink {
    fn describe_review(&self, _title: &str, _total_files: usize) {}

    fn save_state(&self, state: kemi_core::session::SessionState) -> Result<(), SessionError> {
        self.open.lock().unwrap().save_state(state)
    }

    fn save_copy(&self, copy: kemi_core::session::SessionCopy) -> Result<(), SessionError> {
        self.open.lock().unwrap().save_copy(copy)
    }

    fn mark_unresumable(&self, reason: &str) -> Result<(), SessionError> {
        self.open.lock().unwrap().mark_unresumable(reason)
    }

    fn delete(&self) -> Result<(), SessionError> {
        self.open.lock().unwrap().delete()
    }

    fn save_image(
        &self,
        comment_id: &str,
        bytes: &[u8],
        comments: &std::collections::BTreeSet<String>,
    ) -> FilesWrite {
        self.open
            .lock()
            .unwrap()
            .save_image(comment_id, bytes, comments)
    }

    fn save_snapshot(
        &self,
        snapshot: &PageSnapshot,
        comments: &std::collections::BTreeSet<String>,
    ) -> FilesWrite {
        self.open.lock().unwrap().save_snapshot(snapshot, comments)
    }
}

/// `start_stored_review` が開くセッションの情報。
fn stored_session_info(authority: &str, sessions: &Scratch) -> kemi_core::session::SessionInfo {
    use kemi_core::session::{SessionInfo, SessionMode};
    SessionInfo {
        id: "01HF7YAT00PAGE000000000000".to_string(),
        created: 1,
        updated: 1,
        workspace: sessions.0.clone(),
        workspace_key: "00000000000000aa".to_string(),
        mode: SessionMode::Live {
            page: kemi_core::domain::live::LivePage::Url(format!("http://{authority}/")),
            root: sessions.0.clone(),
        },
        title: "live".to_string(),
        total_files: 0,
    }
}

/// `<id>.files/` の上限を `files_limit` にしたセッションで、`--live` のレビューを始める。
/// 復元と同じく、`<id>.files/` に残るスナップショットの番号の続きから振る。
async fn start_stored_review(
    authority: &str,
    sessions: &Scratch,
    files_limit: u64,
    snapshots: Vec<PageSnapshot>,
) -> Running {
    let open = kemi_core::session::SessionStore::new(sessions.0.clone())
        .with_files_limit(files_limit)
        .create(stored_session_info(authority, sessions))
        .unwrap();
    let last_snapshot_number = open.last_snapshot_number();
    start_restored_review(
        LiveTarget::Url {
            authority: authority.to_string(),
            start: "/".to_string(),
            display: format!("http://{authority}/"),
        },
        sessions.0.clone(),
        Some(Arc::new(StoreSink {
            open: std::sync::Mutex::new(open),
        })),
        snapshots,
        last_snapshot_number,
    )
    .await
}

/// gzip でほとんど縮まない文字列（`seed` ごとに違う）。
fn incompressible_text(size: usize, seed: u64) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ seed.wrapping_mul(0x1234_5678_9abc_def1);
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(ALPHABET[(state % 64) as usize])
        })
        .collect()
}

/// 縮まないバイト列を base64 にしたもの（コメントの画像の代わり）。
fn incompressible_image(size: usize) -> String {
    use base64::Engine;
    let bytes: Vec<u8> = incompressible_text(size * 2, 99)
        .bytes()
        .collect::<Vec<u8>>()
        .chunks(2)
        .map(|pair| pair[0].wrapping_mul(31) ^ pair[1].wrapping_mul(7))
        .collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

async fn take_snapshot(running: &Running, kind: &str, html: &str) -> serde_json::Value {
    let taken = post_review(
        running,
        "api/snapshot",
        serde_json::json!({ "page": "/", "width": 390, "kind": kind, "html": html }),
    )
    .await;
    assert_eq!(taken.status(), StatusCode::OK);
    taken.json().await.unwrap()
}

async fn listed_ids(running: &Running) -> Vec<String> {
    get_review_json(running, "api/snapshots").await["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|snapshot| snapshot["id"].as_str().unwrap().to_string())
        .collect()
}

async fn add_page_comment_with_image(running: &Running, image: String) -> serde_json::Value {
    let added = post_review(
        running,
        "api/comment",
        serde_json::json!({
            "op": "add_page",
            "page": { "url": "/", "width": 390, "places": [page_place(1, "element")] },
            "body": "with an image",
            "image": image,
        }),
    )
    .await;
    assert_eq!(added.status(), StatusCode::OK);
    added.json().await.unwrap()
}

#[tokio::test]
async fn a_snapshot_taken_near_the_limit_drops_the_oldest_handed_one_from_the_choices() {
    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("snapshot-limit-sessions");
    let running = start_stored_review(&authority, &sessions, 3000, Vec::new()).await;
    take_snapshot(&running, "start", &incompressible_text(1000, 1)).await;
    let handed = take_snapshot(&running, "handed", &incompressible_text(1000, 2)).await;
    let manual = take_snapshot(&running, "manual", &incompressible_text(1000, 3)).await;

    let newest = take_snapshot(&running, "manual", &incompressible_text(1000, 4)).await;

    let ids = listed_ids(&running).await;
    assert!(
        !ids.contains(&handed["id"].as_str().unwrap().to_string()),
        "{ids:?}"
    );
    assert!(
        ids.contains(&manual["id"].as_str().unwrap().to_string()),
        "{ids:?}"
    );
    assert_eq!(newest["unsaved"], false, "{newest}");
}

#[tokio::test]
async fn a_snapshot_with_no_room_beside_the_start_one_and_current_images_stays_usable_and_marked_unsaved()
 {
    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("snapshot-no-room-sessions");
    let running = start_stored_review(&authority, &sessions, 3000, Vec::new()).await;
    take_snapshot(&running, "start", &incompressible_text(1200, 1)).await;
    add_page_comment_with_image(&running, incompressible_image(1200)).await;

    let taken = take_snapshot(&running, "manual", &incompressible_text(1200, 2)).await;

    let listed = get_review_json(&running, "api/snapshots").await;
    let entry = listed["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|snapshot| snapshot["id"] == taken["id"])
        .cloned()
        .unwrap();
    assert_eq!(taken["unsaved"], true, "{taken}");
    assert_eq!(entry["unsaved"], true, "{entry}");
    let body = get_review_json(
        &running,
        &format!("api/snapshot/{}", taken["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(body["html"], incompressible_text(1200, 2));
}

#[tokio::test]
async fn a_comment_image_with_no_room_is_reported_in_the_answer() {
    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("image-no-room-sessions");
    let running = start_stored_review(&authority, &sessions, 1000, Vec::new()).await;

    let added = add_page_comment_with_image(&running, incompressible_image(1200)).await;

    assert_eq!(added["image_unsaved"], true, "{added}");
}

/// SSE を読み、`event: <name>` が届くまで待つ。届かなければ false。
async fn receive_event(events: &mut reqwest::Response, name: &str) -> bool {
    let wanted = format!("event: {name}");
    let mut received = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while !received.contains(&wanted) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, events.chunk()).await {
            Ok(Ok(Some(chunk))) => received.push_str(&String::from_utf8_lossy(&chunk)),
            Ok(Ok(None) | Err(_)) | Err(_) => return false,
        }
    }
    true
}

#[tokio::test]
async fn a_comment_image_that_pushes_a_snapshot_out_tells_the_page_to_read_the_choices_again() {
    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("image-pushes-sessions");
    let running = start_stored_review(&authority, &sessions, 2500, Vec::new()).await;
    take_snapshot(&running, "start", &incompressible_text(900, 1)).await;
    let manual = take_snapshot(&running, "manual", &incompressible_text(900, 2)).await;
    let mut events = reqwest::get(format!("{}api/events", running.review))
        .await
        .unwrap();

    add_page_comment_with_image(&running, incompressible_image(1200)).await;

    assert!(receive_event(&mut events, "snapshots").await);
    let ids = listed_ids(&running).await;
    assert!(
        !ids.contains(&manual["id"].as_str().unwrap().to_string()),
        "{ids:?}"
    );
}

#[tokio::test]
async fn a_snapshot_taken_after_restoring_gets_an_id_no_restored_one_has() {
    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("restored-ids-sessions");
    let restored = |number, kind| PageSnapshot {
        number,
        kind,
        page: "/".to_string(),
        width: 390,
        html: "<p>restored</p>".to_string(),
        description: None,
    };
    let running = start_stored_review(
        &authority,
        &sessions,
        kemi_core::session::FILES_LIMIT,
        vec![
            restored(1, SnapshotKind::Start),
            restored(3, SnapshotKind::Manual),
        ],
    )
    .await;

    let taken = take_snapshot(&running, "manual", "<p>new</p>").await;

    assert_eq!(
        listed_ids(&running).await,
        vec!["s1", "s3", taken["id"].as_str().unwrap()]
    );
    assert!(taken["id"] != "s1" && taken["id"] != "s3", "{taken}");
}

#[tokio::test]
async fn a_snapshot_taken_after_restoring_stays_when_the_rule_removes_a_file_that_could_not_be_read()
 {
    // 強制終了で途中まで書いたスナップショットのファイルは、復元で読めずに飛ばされるが
    // `<id>.files/` に残る。復元後の新しいスナップショットがその番号を使うと、20 MB の規則が
    // そのファイルを消したときに、新しいものが選択肢から消える（R-PAGE-SESSION）。
    let (authority, _dev) = start_dev_server().await;
    let sessions = Scratch::new("restored-unreadable-sessions");
    let snapshot = |number, kind, html: String| PageSnapshot {
        number,
        kind,
        page: "/".to_string(),
        width: 390,
        html,
        description: None,
    };
    let start = snapshot(1, SnapshotKind::Start, incompressible_text(900, 1));
    {
        let open = kemi_core::session::SessionStore::new(sessions.0.clone())
            .create(stored_session_info(&authority, &sessions))
            .unwrap();
        let none = std::collections::BTreeSet::new();
        open.save_snapshot(&start, &none);
        let handed = open.save_snapshot(
            &snapshot(2, SnapshotKind::Handed, "<p>handed</p>".to_string()),
            &none,
        );
        let kemi_core::session::FileWritten::Saved(path) = handed.written else {
            panic!("not saved: {:?}", handed.written);
        };
        std::fs::write(path, incompressible_text(1500, 3)).unwrap();
    }
    let running = start_stored_review(&authority, &sessions, 3000, vec![start]).await;

    let taken = take_snapshot(&running, "manual", &incompressible_text(1200, 2)).await;

    assert_eq!(taken["unsaved"], false, "{taken}");
    assert_eq!(
        listed_ids(&running).await,
        vec!["s1", taken["id"].as_str().unwrap()]
    );
}
