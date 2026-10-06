//! `--live` の中継（R-PAGE-PROXY）。レビュー画面とは別のポートで開発サーバのオリジンを
//! 中継し、HTML にページ用のスクリプトを差し込む。中継のポートは中継用の cookie を持つ
//! 要求だけを通す。cookie はレビュー画面のトークンの URL を開いたときに入れる。

pub(crate) mod files;
mod relay;
pub(crate) mod rewrite;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use kemi_core::domain::live::url_path;

use crate::Assets;

/// 中継の待ち受けと相手（R-PAGE-PROXY）。
pub struct LiveParams {
    /// `--live-port` と `--bind` で立てたリスナー。
    pub listener: TcpListener,
    pub target: LiveTarget,
    /// 配れる範囲の根（実体の場所）。ファイルのページとモックはこの中からだけ配る。
    pub root: PathBuf,
    /// 中継用の cookie の値。ページのトークンとは別の秘密。
    pub cookie: String,
    /// モックを配る URL に含める、レビューごとの推測できない値（live-compare.md の DC1）。
    pub mock_secret: String,
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
    /// 配れる範囲の HTML ファイル。`path` は範囲の根からの相対パス（`/` 区切り）。
    File { path: String },
}

/// レビュー画面と中継が共有する、中継の情報。
pub(crate) struct LiveInfo {
    pub port: u16,
    pub cookie_name: String,
    pub cookie: String,
    pub start: String,
    pub display: String,
    pub code_view: bool,
    /// 取ったスナップショット。保存はまだ無く、メモリにだけ持つ（R-PAGE-SNAPSHOT）。
    pub snapshots: std::sync::Mutex<Vec<Snapshot>>,
    pub root: PathBuf,
    pub mock_secret: String,
    /// ページ（パスとクエリ）ごとのモックの割り当て（範囲の根からの相対パス）。保存はまだ
    /// 無く、メモリにだけ持つ（R-PAGE-MOCK）。
    pub mocks: std::sync::Mutex<BTreeMap<String, String>>,
}

/// スナップショット 1 つ。中身はスクリプトを含まない HTML（形はページ用のスクリプトが決める）。
pub(crate) struct Snapshot {
    pub id: String,
    /// ページ（パスとクエリ）。
    pub page: String,
    pub width: u32,
    pub kind: SnapshotKind,
    pub html: String,
}

/// 取った時点（R-PAGE-SNAPSHOT）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotKind {
    Start,
    Handed,
    Manual,
}

impl SnapshotKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "start" => Some(SnapshotKind::Start),
            "handed" => Some(SnapshotKind::Handed),
            "manual" => Some(SnapshotKind::Manual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotKind::Start => "start",
            SnapshotKind::Handed => "handed",
            SnapshotKind::Manual => "manual",
        }
    }
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
    /// 配れる範囲の見張り（`<ファイル>` のときだけ）。
    pub served: Option<crate::watch::ServedWatch>,
    /// 配れる範囲のファイルが保存されたときの知らせ。ページは読み込み直す（R-LIVE の例外）。
    pub reload: tokio::sync::broadcast::Sender<()>,
}

impl LiveState {
    pub fn new(
        info: Arc<LiveInfo>,
        target: LiveTarget,
        assets: Arc<dyn Assets>,
        review_port: u16,
    ) -> Self {
        let (reload, _) = tokio::sync::broadcast::channel(16);
        let served = match &target {
            LiveTarget::Url { .. } => None,
            LiveTarget::File { .. } => Some(crate::watch::start_served(
                info.root.clone(),
                reload.clone(),
            )),
        };
        LiveState {
            info,
            target,
            assets,
            review_port,
            served,
            reload,
        }
    }
}

/// 中継のリスナーを分けて、レビュー画面と中継の両方が持つ情報を作る。
pub(crate) fn prepare(params: LiveParams) -> std::io::Result<(TcpListener, LiveInfo, LiveTarget)> {
    let port = params.listener.local_addr()?.port();
    let (start, display) = match &params.target {
        LiveTarget::Url { start, display, .. } => (start.clone(), display.clone()),
        LiveTarget::File { path, .. } => (format!("/{}", url_path(path)), format!("/{path}")),
    };
    let info = LiveInfo {
        port,
        cookie_name: format!("kemi_live_{port}"),
        cookie: params.cookie,
        start,
        display,
        code_view: params.code_view,
        snapshots: std::sync::Mutex::new(Vec::new()),
        root: params.root,
        mock_secret: params.mock_secret,
        mocks: std::sync::Mutex::new(BTreeMap::new()),
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
        .map(|value| value.as_bytes().to_vec())
        .unwrap_or_default();
    let (value, rest) = rewrite::take_cookie(&header, &state.info.cookie_name);
    if !value.is_some_and(|value| same_secret(&value, state.info.cookie.as_bytes())) {
        return (
            StatusCode::FORBIDDEN,
            "kemi: open the review URL first; this page is shown only inside the review",
        )
            .into_response();
    }
    match rest.map(|rest| HeaderValue::from_bytes(&rest)) {
        Some(Ok(rest)) => {
            request.headers_mut().insert(header::COOKIE, rest);
        }
        Some(Err(_)) | None => {
            request.headers_mut().remove(header::COOKIE);
        }
    }
    let path = request.uri().path().to_string();
    if let Some(own) = path.strip_prefix("/__kemi/") {
        return own_file(&state, own, request.uri().query()).await;
    }
    match &state.target {
        LiveTarget::Url {
            authority, display, ..
        } => relay::forward(&state, authority, display, request).await,
        LiveTarget::File { .. } => files::serve(&state, &state.info.root, request).await,
    }
}

/// 中継のポートで kemi 自身が配るもの。差し込むスクリプトと、開発サーバに
/// つながるかの確かめ（待っているページが使う）。
async fn own_file(state: &LiveState, path: &str, query: Option<&str>) -> Response {
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
            let page = query
                .and_then(|query| query.strip_prefix("path="))
                .and_then(files::percent_decode);
            if relay::reachable(state, page.as_deref()).await {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::BAD_GATEWAY.into_response()
            }
        }
        "events" => reload_events(state),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 配れる範囲のファイルが保存されたら `reload` を送る SSE（R-LIVE の `--live <ファイル>`）。
fn reload_events(state: &LiveState) -> Response {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::StreamExt;
    let stream = tokio_stream::wrappers::BroadcastStream::new(state.reload.subscribe()).filter_map(
        |received| async move {
            // data の無いイベントはブラウザが配らないので、中身の無い印を入れる。
            received.ok().map(|()| {
                Ok::<_, std::convert::Infallible>(Event::default().event("reload").data("saved"))
            })
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// 長さと中身を、早く抜けずに比べる。
fn same_secret(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
