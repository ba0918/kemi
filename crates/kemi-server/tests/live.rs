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
            target: LiveTarget::Url {
                authority: authority.to_string(),
                start: "/".to_string(),
                display: format!("http://{authority}/"),
            },
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
