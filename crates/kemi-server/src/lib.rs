//! HTTP / SSE サーバ（R-SERVE, R-SUBMIT, R-LIVE）。

mod api;
mod session;

use std::borrow::Cow;
use std::sync::{Arc, Mutex, RwLock};

use kemi_core::domain::review::ReviewMeta;
use kemi_core::source::ReviewSource;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Notify};

pub use api::session_url;
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

pub struct ServeParams {
    pub source: Arc<dyn ReviewSource>,
    pub assets: Arc<dyn Assets>,
    pub token: String,
}

/// submit の同時受理を 1 つに絞るための状態。
pub(crate) enum SubmitState {
    Open,
    Claimed,
}

pub(crate) struct AppState {
    pub source: Arc<dyn ReviewSource>,
    pub assets: Arc<dyn Assets>,
    pub token: String,
    pub origin: String,
    pub host: String,
    pub meta: RwLock<Arc<ReviewMeta>>,
    pub session: Mutex<Session>,
    pub events: broadcast::Sender<()>,
    pub shutdown: Notify,
    pub outcome: Mutex<Option<ServeOutcome>>,
    pub submit_state: Mutex<SubmitState>,
}

pub async fn serve(
    listener: TcpListener,
    params: ServeParams,
) -> Result<ServeOutcome, ServerError> {
    let address = listener.local_addr().map_err(ServerError::Io)?;
    let origin = format!("http://127.0.0.1:{}", address.port());
    let host = format!("127.0.0.1:{}", address.port());

    let review = params.source.review().map_err(ServerError::Source)?;
    let (events, _) = broadcast::channel(16);

    let state = Arc::new(AppState {
        source: params.source,
        assets: params.assets,
        token: params.token,
        origin,
        host,
        meta: RwLock::new(Arc::new(review)),
        session: Mutex::new(Session::default()),
        events,
        shutdown: Notify::new(),
        outcome: Mutex::new(None),
        submit_state: Mutex::new(SubmitState::Open),
    });

    let app = api::router(state.clone());
    let shutdown_state = state.clone();
    let shutdown = async move { shutdown_state.shutdown.notified().await };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ServerError::Io)?;

    let outcome = state
        .outcome
        .lock()
        .expect("outcome poisoned")
        .take()
        .ok_or_else(|| ServerError::Stopped("submit なしでサーバが停止しました".to_string()))?;
    Ok(outcome)
}
