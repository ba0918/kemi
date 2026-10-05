//! HTTP / SSE サーバ（R-SERVE, R-SUBMIT, R-LIVE）。

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod api;
mod highlight;
mod lan;
mod live;
mod session;
mod units;
mod watch;

use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};

use kemi_core::domain::review::Side;
use kemi_core::session::SessionError;
use kemi_core::source::ReviewSource;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch as shutdown_watch};

use api::AllowedHosts;
pub use api::{session_host, session_url};
pub use lan::{detect_share_address, exposure_warning, live_exposure_warning};
pub use live::{LiveParams, LiveTarget};
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

/// サーバを止めた理由。submit か、レビュー中の実行時エラーか、保留か。
pub(crate) enum Stop {
    Submitted(serde_json::Value),
    Failed(String),
    /// 待っている `kemi wait` に保留を知らせるために止めた（[`ServeControl::suspend`]）。
    Suspended,
}

/// 結果ファイルを残せなかった理由（R-RESULT）。完了画面には Display の文を出す。
#[derive(Debug)]
pub enum ResultSaveError {
    /// 保存先のディレクトリを決められない。中身は決められない理由。
    Unlocated(String),
    /// 保存先に書けない。
    Write {
        dir: std::path::PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ResultSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResultSaveError::Unlocated(reason) => {
                write!(
                    formatter,
                    "cannot determine where to store results ({reason})"
                )
            }
            ResultSaveError::Write { dir, source } => {
                write!(formatter, "{}: {source}", dir.display())
            }
        }
    }
}

impl std::error::Error for ResultSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResultSaveError::Unlocated(_) => None,
            ResultSaveError::Write { source, .. } => Some(source),
        }
    }
}

/// サーバが起動側に知らせること。ライブラリは stderr に書かず、書き方は起動側が決める。
#[derive(Debug)]
pub enum Notice {
    /// コメントを足した。`lines` は開始行と終了行で、ファイル全体のコメントでは None。
    CommentAdded {
        path: String,
        side: Side,
        lines: Option<(u32, u32)>,
    },
    /// 結果ファイルを残せなかった。submit の結果と終了コードは変わらない（R-RESULT）。
    ResultNotSaved(ResultSaveError),
    /// セッションを保存できなかった。レビューは続く（R-SESSION）。
    SessionNotSaved(SessionError),
    /// submit の後にセッションを消せなかった（R-SESSION）。
    SessionNotDeleted(SessionError),
    /// ファイルの由来を計算できなかった。由来は「特定できない」になる（R-ORIGIN）。
    OriginUnknown { path: String, reason: String },
}

/// [`Notice`] の受け取り先。
pub trait NoticeSink: Send + Sync {
    fn notify(&self, notice: Notice);
}

/// submit を確定した結果を残す先（R-RESULT）。
pub trait ResultSink: Send + Sync {
    /// stdout に出すのと同じ JSON の文字列を残し、書いたファイルを返す。
    fn save(&self, text: &str) -> Result<std::path::PathBuf, ResultSaveError>;
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
    /// 写しをこれから作る必要があるか。復元のように既に保存された写しがあるなら false。
    fn needs_copy(&self) -> bool {
        true
    }
    /// サーブ開始時。起動時の単位の題と全ファイル数。
    fn describe_review(&self, title: &str, total_files: usize);
    /// 状態が変わるたび。空の状態と写しだけで、書くべきものが無ければ何もしない。
    fn save_state(&self, state: kemi_core::session::SessionState) -> Result<(), SessionError>;
    /// 凍結が完成したとき。写しの上限は実装が判定する。
    fn save_copy(&self, copy: kemi_core::session::SessionCopy) -> Result<(), SessionError>;
    /// 凍結できなかったとき。状態と情報だけを残す。
    fn mark_unresumable(&self, reason: &str) -> Result<(), SessionError>;
    /// submit の確定後。
    fn delete(&self) -> Result<(), SessionError>;
}

pub struct ServeParams {
    pub source: Arc<dyn ReviewSource>,
    pub assets: Arc<dyn Assets>,
    pub token: String,
    pub results: Option<Arc<dyn ResultSink>>,
    /// セッションの保存先。無ければ保存しない（R-SESSION）。
    pub session: Option<Arc<dyn SessionSink>>,
    /// 警告や進み具合の知らせを受け取る先。
    pub notices: Arc<dyn NoticeSink>,
    /// LAN に案内する共有アドレス。`--bind 0.0.0.0` のときだけ意味を持ち、
    /// 特定できなければ `None`（R-SERVE）。URL と警告は起動側が組み立てる。
    pub share_address: Option<Ipv4Addr>,
    /// エージェント用の API（agent-channel.md）。セッションの無いレビューでは `None`。
    pub agent: Option<AgentParams>,
    /// `--live` の中継（live.md の R-PAGE-PROXY）。`--live` でなければ `None`。
    pub live: Option<LiveParams>,
}

/// エージェント用の API の待ち受け（R-AGENT-LINK）。
pub struct AgentParams {
    /// [`bind_agent_listener`] で立てたリスナー。
    pub listener: TcpListener,
    /// ページのトークンとは別の秘密。`<id>.endpoint` にだけ書く。
    pub token: String,
    /// 保留を待っている `kemi wait` に知らせる入口。起動側が持つ。
    pub control: ServeControl,
}

/// 起動側からサーバへの入口。保留（SIGINT / SIGTERM）は submit や実行時エラーと違って
/// サーバの中を通らず、起動側が serve の future を捨てて終わるので、待っている
/// `kemi wait` に理由を返すにはここから知らせる（R-AGENT-CLI）。
#[derive(Clone, Default)]
pub struct ServeControl {
    state: Arc<std::sync::OnceLock<std::sync::Weak<AppState>>>,
}

impl ServeControl {
    pub fn new() -> Self {
        ServeControl::default()
    }

    /// 保留で終わることを、待っている `kemi wait` に知らせてサーバを止め始める。待っている
    /// ものが無ければ何もせず false を返し、起動側は今までどおりすぐ終わってよい。true の
    /// ときは、serve が返る（理由を返し終えて接続が閉じる）のを待ってから終わる。
    pub fn suspend(&self) -> bool {
        let Some(state) = self.state.get().and_then(std::sync::Weak::upgrade) else {
            return false;
        };
        if !state.agent.lock().expect("agent poisoned").waiting {
            return false;
        }
        {
            let mut stop = state.stop.lock().expect("stop poisoned");
            if stop.is_none() {
                *stop = Some(Stop::Suspended);
            }
        }
        let _ = state.shutdown.send(true);
        true
    }
}

/// エージェント用のリスナーを立てる。`--bind` に関わらず `127.0.0.1` で待つので、
/// LAN からは届かず、`--bind` が具体アドレスでも同じマシンの `kemi wait` は届く
/// （R-AGENT-LINK）。
pub async fn bind_agent_listener() -> std::io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await
}

/// SSE でページへ知らせること。
#[derive(Clone, Debug)]
pub(crate) enum Event {
    /// 新側の供給元が変わった（R-LIVE の更新バッジ）。
    Update,
    /// もう片方のグループ単位の作成の状態が変わった（R-UNIT）。
    Unit,
    /// スレッドに返信が増えた。中身はそのコメントの JSON。
    Thread(serde_json::Value),
    /// 発言が増えた。中身はその発言の JSON。
    Message(serde_json::Value),
    /// エージェントの状態か未渡しの件数が変わった（R-AGENT-STATE）。
    Agent(serde_json::Value),
    /// エージェントに渡した。`--live` のページはスナップショットを取る（R-PAGE-SNAPSHOT）。
    Handed,
}

/// エージェントとのつながりの、メモリだけに置く部分（R-AGENT-STATE）。`kemi wait` が
/// 呼ばれたかどうかはセッション状態にあり、保留と復元をまたぐ。
pub(crate) struct AgentRuntime {
    /// `kemi wait` が待っているか。
    pub waiting: bool,
    /// 最後に `kemi wait` が返った（または切れた）か `kemi reply` が来た時刻（ミリ秒）。
    pub last_activity: u128,
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
    pub notices: Arc<dyn NoticeSink>,
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
    pub agent: Mutex<AgentRuntime>,
    /// エージェント用の API のトークン。無ければその API を立てない。
    pub agent_token: Option<String>,
    /// 待っている `kemi wait` を起こす（渡したとき）。
    pub wake: tokio::sync::Notify,
    /// true で停止。SSE もこれを見て終端する（R-SUBMIT）。
    pub shutdown: shutdown_watch::Sender<bool>,
    pub stop: Mutex<Option<Stop>>,
    pub submit_state: Mutex<SubmitState>,
    /// 凍結のタスクを 1 度だけ始めるための印（R-SESSION）。
    pub freeze_started: std::sync::atomic::AtomicBool,
    /// `--live` の中継の情報。トークンの URL で cookie を入れ、`api/review` に添える。
    pub live: Option<Arc<live::LiveInfo>>,
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

type ServerFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>>;

/// 停止の合図（R-SUBMIT）を待つ future。
fn stopping(state: &AppState) -> impl std::future::Future<Output = ()> + use<> {
    let mut receiver = state.shutdown.subscribe();
    async move {
        loop {
            if *receiver.borrow() {
                break;
            }
            if receiver.changed().await.is_err() {
                break;
            }
        }
    }
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
            )));
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
    // エージェントの返信は短い間に続けて届くことがあるので、取りこぼしにくい長さにする。
    let (events, _) = broadcast::channel(256);
    let (shutdown, _) = shutdown_watch::channel(false);
    let live = match params.live {
        Some(live) => {
            let (listener, info, target) = live::prepare(live).map_err(ServerError::Io)?;
            Some((listener, Arc::new(info), target))
        }
        None => None,
    };

    let state = Arc::new(AppState {
        source: params.source,
        highlighter: std::sync::OnceLock::new(),
        assets: params.assets,
        results: params.results,
        session_sink: params.session,
        notices: params.notices,
        token: params.token,
        allowed,
        review: RwLock::new(units::ReviewState::new(&units, review)),
        refresh: tokio::sync::Mutex::new(()),
        session: Mutex::new(Session::from_state(initial_state)),
        persist: Mutex::new(()),
        events,
        agent: Mutex::new(AgentRuntime {
            waiting: false,
            last_activity: kemi_core::session::now_millis(),
        }),
        agent_token: params.agent.as_ref().map(|agent| agent.token.clone()),
        wake: tokio::sync::Notify::new(),
        shutdown,
        stop: Mutex::new(None),
        submit_state: Mutex::new(SubmitState::Open),
        freeze_started: std::sync::atomic::AtomicBool::new(false),
        live: live.as_ref().map(|(_, info, _)| info.clone()),
    });

    if let Some(agent) = &params.agent {
        let _ = agent.control.state.set(Arc::downgrade(&state));
    }
    watch::start(state.source.watch_paths(), state.events.clone());

    let app = api::router(state.clone());
    let page = axum::serve(listener, app).with_graceful_shutdown(stopping(&state));
    let mut servers: Vec<ServerFuture> = vec![Box::pin(page.into_future())];
    if let Some(agent) = params.agent {
        api::start_status_ticker(&state);
        let agent_app =
            api::agent_router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        // graceful shutdown は処理中の接続が閉じるまで待つ。待っている kemi wait は
        // submit の結果を返し終えてから閉じる（R-AGENT-CLI）。
        let agent = axum::serve(agent.listener, agent_app).with_graceful_shutdown(stopping(&state));
        servers.push(Box::pin(agent.into_future()));
    }
    if let Some((live_listener, info, target)) = live {
        let live_state = Arc::new(live::LiveState::new(
            info,
            target,
            state.assets.clone(),
            address.port(),
        ));
        let relay = axum::serve(live_listener, live::router(live_state));
        let stop = stopping(&state);
        // 中継は開発サーバの長く続く応答（SSE など）を抱えうるので、graceful shutdown で
        // 接続が閉じるのを待たない。止まる合図で受け付けをやめるだけにする。
        servers.push(Box::pin(async move {
            tokio::select! {
                result = relay.into_future() => result,
                () = stop => Ok(()),
            }
        }));
    }
    futures_util::future::try_join_all(servers)
        .await
        .map_err(ServerError::Io)?;

    let stop = state.stop.lock().expect("stop poisoned").take();
    match stop {
        Some(Stop::Submitted(document)) => Ok(ServeOutcome::Submitted(document)),
        Some(Stop::Failed(message)) => Err(ServerError::Stopped(message)),
        Some(Stop::Suspended) => Err(ServerError::Stopped("the review was suspended".to_string())),
        None => Err(ServerError::Stopped(
            "server stopped without a submit".to_string(),
        )),
    }
}
