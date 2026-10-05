//! `--live` の中継（R-PAGE-PROXY）。レビュー画面とは別のポートで開発サーバのオリジンを
//! 中継し、HTML にページ用のスクリプトを差し込む。中継のポートは中継用の cookie を持つ
//! 要求だけを通す。cookie はレビュー画面のトークンの URL を開いたときに入れる。

mod relay;
mod rewrite;

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::Assets;

/// 中継の待ち受けと相手（R-PAGE-PROXY）。
pub struct LiveParams {
    /// `--live-port` と `--bind` で立てたリスナー。
    pub listener: TcpListener,
    pub target: LiveTarget,
    /// 中継用の cookie の値。ページのトークンとは別の秘密。
    pub cookie: String,
    /// コードの見方を出せるか（git の作業ツリーの中か）。
    pub code_view: bool,
}

/// 中継する相手。
pub enum LiveTarget {
    /// ループバックの開発サーバ。`authority` は `host:port`、`start` は最初に開くパスと
    /// クエリ、`display` は題に出す URL。
    Url {
        authority: String,
        start: String,
        display: String,
    },
}

/// レビュー画面と中継が共有する、中継の情報。
pub(crate) struct LiveInfo {
    pub port: u16,
    pub cookie_name: String,
    pub cookie: String,
    pub start: String,
    pub display: String,
    pub code_view: bool,
}

impl LiveInfo {
    /// トークンの URL を開いたときに入れる cookie（R-PAGE-PROXY）。cookie はポートを
    /// 区別しないので、同じホストの中継のポートへの要求にも付く。
    pub fn set_cookie(&self) -> HeaderValue {
        HeaderValue::from_str(&format!(
            "{}={}; HttpOnly; SameSite=Strict; Path=/",
            self.cookie_name, self.cookie
        ))
        .expect("the cookie name and value are ASCII without separators")
    }

    /// `api/review` に添える。ページは自分のホスト名とこのポートで中継の URL を作る。
    pub fn json(&self) -> Value {
        json!({
            "port": self.port,
            "start": self.start,
            "page": self.display,
            "code": self.code_view,
        })
    }
}

pub(crate) struct LiveState {
    pub info: Arc<LiveInfo>,
    pub target: LiveTarget,
    pub assets: Arc<dyn Assets>,
    /// レビュー画面の待ち受けポート。`frame-ancestors` に入れる。
    pub review_port: u16,
}

/// 中継のリスナーを分けて、レビュー画面と中継の両方が持つ情報を作る。
pub(crate) fn prepare(params: LiveParams) -> std::io::Result<(TcpListener, LiveInfo, LiveTarget)> {
    let port = params.listener.local_addr()?.port();
    let (start, display) = match &params.target {
        LiveTarget::Url { start, display, .. } => (start.clone(), display.clone()),
    };
    let info = LiveInfo {
        port,
        cookie_name: format!("kemi_live_{port}"),
        cookie: params.cookie,
        start,
        display,
        code_view: params.code_view,
    };
    Ok((params.listener, info, params.target))
}

pub(crate) fn router(state: Arc<LiveState>) -> Router {
    Router::new().fallback(handle).with_state(state)
}

async fn handle(State(state): State<Arc<LiveState>>, mut request: Request) -> Response {
    let header = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let (value, rest) = rewrite::take_cookie(&header, &state.info.cookie_name);
    if !value.is_some_and(|value| same_secret(&value, &state.info.cookie)) {
        return (
            StatusCode::FORBIDDEN,
            "kemi: open the review URL first; this page is shown only inside the review",
        )
            .into_response();
    }
    match rest.map(|rest| HeaderValue::from_str(&rest)) {
        Some(Ok(rest)) => {
            request.headers_mut().insert(header::COOKIE, rest);
        }
        Some(Err(_)) | None => {
            request.headers_mut().remove(header::COOKIE);
        }
    }
    let path = request.uri().path().to_string();
    if let Some(own) = path.strip_prefix("/__kemi/") {
        return own_file(&state, own).await;
    }
    relay::forward(&state, request).await
}

/// 中継のポートで kemi 自身が配るもの。差し込むスクリプトと、開発サーバに
/// つながるかの確かめ（待っているページが使う）。
async fn own_file(state: &LiveState, path: &str) -> Response {
    match path {
        "page.js" => match state.assets.get("live/page.js") {
            Some(asset) => (
                [(header::CONTENT_TYPE, asset.mime)],
                asset.bytes.into_owned(),
            )
                .into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        "alive" => {
            if relay::reachable(&state.target).await {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::BAD_GATEWAY.into_response()
            }
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 長さと中身を、早く抜けずに比べる。
fn same_secret(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
