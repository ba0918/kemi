//! HTTP / SSE サーバ（R-SERVE, R-SUBMIT, R-LIVE）。

mod api;
mod highlight;
mod lan;
mod session;
mod units;
mod watch;

use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex, RwLock};

use kemi_core::source::ReviewSource;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch as shutdown_watch};

use api::AllowedHosts;
pub use api::{session_host, session_url};
pub use lan::{detect_share_address, exposure_warning};
pub use session::Session;

/// ページに配る資産。kemi-server は web の中身を知らない。
pub struct Asset {
    pub bytes: Cow<'static, [u8]>,
    pub mime: &'static str,
}

pub trait Assets: Send + Sync {
    fn get(&self, path: &str) -> Option<Asset>;
}

#[derive(Debug)]
pub enum ServerError {
    Io(std::io::Error),
    Source(kemi_core::source::SourceError),
    Stopped(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Io(error) => write!(formatter, "{error}"),
            ServerError::Source(error) => write!(formatter, "{error}"),
            ServerError::Stopped(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ServerError {}

#[derive(Debug)]
pub enum ServeOutcome {
    Submitted(serde_json::Value),
}

/// サーバを止めた理由。submit か、レビュー中の実行時エラーか。
pub(crate) enum Stop {
    Submitted(serde_json::Value),
    Failed(String),
}

/// submit を確定した結果を残す先（R-RESULT）。
pub trait ResultSink: Send + Sync {
    /// stdout に出すのと同じ JSON の文字列を残し、書いたファイルを返す。
    fn save(&self, text: &str) -> Result<std::path::PathBuf, String>;
    /// 保存先のディレクトリ。完了画面に出す。決められなければ None。
    fn location(&self) -> Option<String>;
}

/// レビューをセッションとして残す先（R-SESSION）。保存の失敗は警告だけで、レビューは
/// 終わらせず終了コードも変えない。
pub trait SessionSink: Send + Sync {
    /// 復元のときに、サーバのメモリへ読み戻す状態。新規のセッションでは空。
    fn initial_state(&self) -> kemi_core::session::SessionState {
        kemi_core::session::SessionState::default()
    }
    /// サーブ開始時。起動時の単位の題と全ファイル数。
    fn describe_review(&self, title: &str, total_files: usize);
    /// 状態が変わるたび。空の状態と写しだけで、書くべきものが無ければ何もしない。
    fn save_state(&self, state: kemi_core::session::SessionState) -> Result<(), String>;
    /// 凍結が完成したとき。写しの上限は実装が判定する。
    fn save_copy(&self, copy: kemi_core::session::SessionCopy) -> Result<(), String>;
    /// 凍結できなかったとき。状態と情報だけを残す。
    fn mark_unresumable(&self, reason: &str) -> Result<(), String>;
    /// submit の確定後。
    fn delete(&self) -> Result<(), String>;
}

pub struct ServeParams {
    pub source: Arc<dyn ReviewSource>,
    pub assets: Arc<dyn Assets>,
    pub token: String,
    pub results: Option<Arc<dyn ResultSink>>,
    /// セッションの保存先。無ければ保存しない（R-SESSION）。
    pub session: Option<Arc<dyn SessionSink>>,
    /// LAN に案内する共有アドレス。`--bind 0.0.0.0` のときだけ意味を持ち、
    /// 特定できなければ `None`（R-SERVE）。URL と警告は起動側が組み立てる。
    pub share_address: Option<Ipv4Addr>,
}

/// SSE でページへ知らせること。
#[derive(Clone, Copy, Debug)]
pub(crate) enum Event {
    /// 新側の供給元が変わった（R-LIVE の更新バッジ）。
    Update,
    /// もう片方のグループ単位の作成の状態が変わった（R-UNIT）。
    Unit,
}

/// submit の同時受理を 1 つに絞るための状態。
pub(crate) enum SubmitState {
    Open,
    Claimed,
}

pub(crate) struct AppState {
    pub source: Arc<dyn ReviewSource>,
    pub highlighter: std::sync::OnceLock<crate::highlight::Highlighter>,
    pub assets: Arc<dyn Assets>,
    pub results: Option<Arc<dyn ResultSink>>,
    pub session_sink: Option<Arc<dyn SessionSink>>,
    pub token: String,
    /// POST を受理する Host の範囲（R-SERVE）。
    pub allowed: AllowedHosts,
    pub review: RwLock<units::ReviewState>,
    /// 再取得を 1 つずつ行う。同じ単位を同時に作り直して計画が入れ替わらないように。
    pub refresh: tokio::sync::Mutex<()>,
    pub session: Mutex<Session>,
    /// persist のスナップショットと保存を 1 つずつ進める（R-SESSION）。
    pub persist: Mutex<()>,
    pub events: broadcast::Sender<Event>,
    /// true で停止。SSE もこれを見て終端する（R-SUBMIT）。
    pub shutdown: shutdown_watch::Sender<bool>,
    pub stop: Mutex<Option<Stop>>,
    pub submit_state: Mutex<SubmitState>,
    /// 凍結のタスクを 1 度だけ始めるための印（R-SESSION）。
    pub freeze_started: std::sync::atomic::AtomicBool,
}

/// レビュー中の実行時エラーで停止する。stdout に JSON を出さず終了コード 2（R-SUBMIT）。
pub(crate) fn stop_with_error(state: &AppState, message: impl Into<String>) {
    {
        let mut stop = state.stop.lock().expect("stop poisoned");
        if stop.is_none() {
            *stop = Some(Stop::Failed(message.into()));
        }
    }
    let _ = state.shutdown.send(true);
}

pub async fn serve(
    listener: TcpListener,
    params: ServeParams,
) -> Result<ServeOutcome, ServerError> {
    let address = listener.local_addr().map_err(ServerError::Io)?;
    let bind = match address.ip() {
        IpAddr::V4(bind) => bind,
        IpAddr::V6(_) => {
            return Err(ServerError::Stopped(format!(
                "{address} is not an IPv4 listener"
            )))
        }
    };
    let allowed = AllowedHosts::new(bind, params.share_address, address.port());

    let review = params.source.review().map_err(ServerError::Source)?;
    // セッションの情報に、起動時の単位の題と全ファイル数を残す（R-SESSION）。
    if let Some(sink) = &params.session {
        let total_files = review.groups.iter().map(|group| group.files.len()).sum();
        sink.describe_review(&review.title, total_files);
    }
    let units = params.source.units();
    // 復元では、セッションの状態から見た・折りたたみ・コメント・解決を読み戻す（R-SESSION）。
    let initial_state = params
        .session
        .as_ref()
        .map(|sink| sink.initial_state())
        .unwrap_or_default();
    let (events, _) = broadcast::channel(16);
    let (shutdown, _) = shutdown_watch::channel(false);

    let state = Arc::new(AppState {
        source: params.source,
        highlighter: std::sync::OnceLock::new(),
        assets: params.assets,
        results: params.results,
        session_sink: params.session,
        token: params.token,
        allowed,
        review: RwLock::new(units::ReviewState::new(&units, review)),
        refresh: tokio::sync::Mutex::new(()),
        session: Mutex::new(Session::from_state(initial_state)),
        persist: Mutex::new(()),
        events,
        shutdown,
        stop: Mutex::new(None),
        submit_state: Mutex::new(SubmitState::Open),
        freeze_started: std::sync::atomic::AtomicBool::new(false),
    });

    watch::start(state.source.watch_paths(), state.events.clone());

    let app = api::router(state.clone());
    let mut shutdown_receiver = state.shutdown.subscribe();
    let shutdown = async move {
        loop {
            if *shutdown_receiver.borrow() {
                break;
            }
            if shutdown_receiver.changed().await.is_err() {
                break;
            }
        }
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ServerError::Io)?;

    let stop = state.stop.lock().expect("stop poisoned").take();
    match stop {
        Some(Stop::Submitted(document)) => Ok(ServeOutcome::Submitted(document)),
        Some(Stop::Failed(message)) => Err(ServerError::Stopped(message)),
        None => Err(ServerError::Stopped(
            "server stopped without a submit".to_string(),
        )),
    }
}
