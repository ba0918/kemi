//! HTTP / SSE サーバ（R-SERVE, R-SUBMIT, R-LIVE）。

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod api;
mod explanation;
mod freeze;
mod highlight;
mod lan;
mod live;
mod preparation;
mod secret;
mod serve;
mod session;
mod stored_session;
mod units;
mod watch;

use std::borrow::Cow;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, RwLock};

use kemi_core::domain::agent::AgentLink;
use kemi_core::domain::explanation::ExplanationDocument;
use kemi_core::domain::review::Side;
use kemi_core::session::SessionError;
use kemi_core::source::ReviewSource;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch as shutdown_watch};

use api::AllowedHosts;
pub use api::{session_host, session_url};
pub use lan::{detect_share_address, exposure_warning, live_exposure_warning};
pub use live::{LiveParams, LiveTarget};
pub use serve::serve;
use session::Session;
pub use stored_session::StoredSession;

/// ページに配る資産。kemi-server は web の中身を知らない。
pub struct Asset {
    pub bytes: Cow<'static, [u8]>,
    pub mime: &'static str,
}

impl Asset {
    /// 応答の本文。バイナリに埋め込んだ資産は借用のまま渡し、配るたびに複製しない。
    pub(crate) fn into_body(self) -> axum::body::Bytes {
        match self.bytes {
            Cow::Borrowed(bytes) => axum::body::Bytes::from_static(bytes),
            Cow::Owned(bytes) => axum::body::Bytes::from(bytes),
        }
    }
}

pub trait Assets: Send + Sync {
    fn get(&self, path: &str) -> Option<Asset>;
}

#[derive(Debug)]
pub enum ServerError {
    Io(std::io::Error),
    Source(kemi_core::source::SourceError),
    Stopped(String),
    Session(SessionError),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Io(error) => write!(formatter, "{error}"),
            ServerError::Source(error) => write!(formatter, "{error}"),
            ServerError::Stopped(message) => formatter.write_str(message),
            ServerError::Session(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ServerError {}

#[derive(Debug)]
pub enum ServeOutcome {
    Submitted(serde_json::Value),
}

/// 保留を頼んだ結果。submit が先に確定していれば、保留にせずその結果を返す。
#[derive(Debug)]
pub enum SuspendOutcome {
    Submitted {
        document: serde_json::Value,
        waiting: bool,
    },
    Suspended {
        waiting: bool,
    },
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
    /// ページへのコメントを足した（live.md の R-PAGE-COMMENT）。`places` は場所の数。
    PageCommentAdded {
        url: String,
        width: u32,
        places: usize,
    },
    /// 結果ファイルを残せなかった。submit の結果と終了コードは変わらない（R-RESULT）。
    ResultNotSaved(ResultSaveError),
    /// ページへのコメントの画像を保存できなかった。コメントは画像なしで残る（R-PAGE-COMMENT）。
    CommentImageNotSaved(SessionError),
    /// スナップショットを `<id>.files/` に書けなかった。レビューの間は比べる相手に使えるが、
    /// 復元すると消える（R-PAGE-SESSION）。
    SnapshotNotSaved(SessionError),
    /// セッションを保存できなかった。レビューは続く（R-SESSION）。
    SessionNotSaved(SessionError),
    /// submit の後にセッションを消せなかった（R-SESSION）。
    SessionNotDeleted(SessionError),
    /// ファイルの由来を計算できなかった。由来は「特定できない」になる（R-ORIGIN）。
    OriginUnknown { path: String, reason: String },
    /// `--live <ファイル>` で、配ったファイルのディレクトリを見張れなかった。そこのファイルを
    /// 保存してもページは読み込み直さない（R-LIVE）。
    ServedNotWatched {
        directory: std::path::PathBuf,
        reason: String,
    },
    /// worktree の作業ツリー全体を見張れず、起動時の差分のファイルだけを見張る形に落とした。
    /// レビューの間に 1 回だけ出す（R-LIVE）。
    WorkTreeNotWatched(WatchFallback),
}

/// 作業ツリー全体の見張りをやめた理由（R-LIVE）。
#[derive(Debug)]
pub enum WatchFallback {
    /// 見張るディレクトリが上限を超えた。
    TooManyDirectories { limit: usize },
    /// OS が見張りを断った、または見張るディレクトリを集められなかった。
    Refused(String),
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

/// レビューをセッションとして残す先（R-SESSION）。一般状態の保存失敗は警告だけにし、
/// 新しい世代は保存が成功するまで公開しない。
pub trait SessionSink: Send + Sync {
    /// 説明を差し替えるとき。説明を含む追加分の上限を確かめてから保存する。
    fn save_explanation_state(
        &self,
        state: kemi_core::session::SessionState,
    ) -> Result<(), SessionError>;
    fn initial_title(&self) -> Option<String> {
        None
    }
    /// 凍結が完成したとき。写しと、凍結の印を付けた状態を一緒に保存する。写しの上限は
    /// 実装が判定し、超えたら写しを捨てて `Ok(false)` を返す。
    fn save_frozen_copy(
        &self,
        copy: kemi_core::session::PreparedCopy,
        state: kemi_core::session::SessionState,
    ) -> Result<bool, SessionError>;
    fn review_id(&self) -> Option<String> {
        None
    }
    fn is_resumed(&self) -> bool {
        false
    }
    /// 新しい差分表示版を発行するとき。保存できてから版を公開する。
    fn save_issued_state(
        &self,
        state: kemi_core::session::SessionState,
    ) -> Result<(), SessionError>;
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
    /// 凍結できなかったとき。状態と情報だけを残す。
    fn mark_unresumable(&self, reason: &str) -> Result<(), SessionError>;
    /// submit の確定後。
    fn delete(&self) -> Result<(), SessionError>;
    /// コメントの画像を `<id>.files/` に書く（live.md の R-PAGE-SESSION）。書く前に 20 MB の
    /// 規則で場所を空ける。`comments` は今あるコメントの id で、その画像は消さない。
    fn save_image(
        &self,
        comment_id: &str,
        bytes: &[u8],
        comments: &std::collections::BTreeSet<String>,
    ) -> kemi_core::session::FilesWrite;
    /// スナップショットを `<id>.files/` に書く。規則はコメントの画像と同じ。
    fn save_snapshot(
        &self,
        snapshot: &kemi_core::session::PageSnapshot,
        comments: &std::collections::BTreeSet<String>,
    ) -> kemi_core::session::FilesWrite;
}

/// 今の時刻（UNIX エポックからのミリ秒）を返す時計。エージェントの状態の「作業中のまま
/// 10 分で応答なし」（R-AGENT-STATE）を決めるのに使う。起動側は [`system_clock`] を渡す。
pub type Clock = Arc<dyn Fn() -> u128 + Send + Sync>;

/// 壁時計。
pub fn system_clock() -> Clock {
    Arc::new(kemi_core::session::now_millis)
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
    /// 起動時に入れる説明（`--explanation`）。セッションとエージェント用の API がある
    /// レビューでだけ使える。
    pub initial_explanation: Option<ExplanationDocument>,
    /// エージェントの状態に使う時計。テストは時刻を進められる時計を渡す。
    pub clock: Clock,
}

/// エージェント用の API の待ち受け（R-AGENT-LINK）。
pub struct AgentParams {
    /// [`bind_agent_listener`] で立てたリスナー。
    pub listener: TcpListener,
    /// ページのトークンとは別の秘密。`<id>.endpoint` にだけ書く。
    pub token: String,
    /// レビューの id（`kemi wait <id>` の id）。stderr にも出していて秘密ではない。画面が
    /// 写すコマンドに使う（R-AGENT-STATE）。
    pub review: String,
    /// 保留を待っている `kemi wait` に知らせる入口。起動側が持つ。
    pub control: ServeControl,
}

/// 起動側からサーバへの入口。保留（SIGINT / SIGTERM）は submit や実行時エラーと違って
/// サーバの中を通らず、起動側が serve の future を捨てて終わるので、待っている
/// `kemi wait` に理由を返すにはここから知らせる（R-AGENT-CLI）。
#[derive(Clone, Default)]
pub struct ServeControl {
    ready: Arc<tokio::sync::Notify>,
    state: Arc<std::sync::OnceLock<std::sync::Weak<AppState>>>,
}

impl ServeControl {
    pub async fn ready(&self) {
        loop {
            let notified = self.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state.get().is_some() {
                return;
            }
            notified.await;
        }
    }
    pub fn new() -> Self {
        ServeControl::default()
    }

    /// 確定済みの submit は保留にせず起動側へ返す（R-SUBMIT）。保留で待っている
    /// `kemi wait` があるときは、理由を返し終えて接続が閉じるまで起動側で待つ。
    pub fn suspend(&self) -> SuspendOutcome {
        let Some(state) = self.state.get().and_then(std::sync::Weak::upgrade) else {
            return SuspendOutcome::Suspended { waiting: false };
        };
        let _gate = state.persist.lock().expect("persist poisoned");
        let waiting = state
            .agent
            .link
            .lock()
            .expect("agent poisoned")
            .is_waiting();
        {
            let mut stop = state.stopping.stop.lock().expect("stop poisoned");
            match stop.as_ref() {
                Some(Stop::Submitted(document)) => {
                    return SuspendOutcome::Submitted {
                        document: document.clone(),
                        waiting,
                    };
                }
                Some(Stop::Failed(_) | Stop::Suspended) => {}
                None => *stop = Some(Stop::Suspended),
            }
        }
        let _ = state.stopping.shutdown.send(true);
        state
            .initial_review
            .lock()
            .expect("initial review poisoned")
            .take();
        SuspendOutcome::Suspended { waiting }
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
    Explanation(serde_json::Value),
    /// 新側の供給元が変わった（R-LIVE の更新バッジ）。
    Update,
    /// もう片方のグループ単位の作成の状態が変わった（R-UNIT）。中身は今の単位の一覧
    /// （`{"units": [...]}`。`api/review` の `units` と同じ形）。
    Unit(serde_json::Value),
    /// スレッドに返信が増えた。中身はそのコメントの JSON。
    Thread(serde_json::Value),
    /// 発言が増えた。中身はその発言の JSON。
    Message(serde_json::Value),
    /// エージェントの状態か未渡しの件数が変わった（R-AGENT-STATE）。
    Agent(serde_json::Value),
    /// `--live` の比べる相手に選べるスナップショットが減った（R-PAGE-SESSION の 20 MB の規則）。
    Snapshots,
}

/// submit の同時受理を 1 つに絞るための状態。
pub(crate) enum SubmitState {
    Open,
    Claimed,
}

/// 停止理由と通知、submit の同時受理をまとめて扱うための状態。
pub(crate) struct Stopping {
    /// true で停止。SSE もこれを見て終端する（R-SUBMIT）。
    pub(crate) shutdown: shutdown_watch::Sender<bool>,
    pub(crate) stop: Mutex<Option<Stop>>,
    pub(crate) submit_state: Mutex<SubmitState>,
}

impl Stopping {
    /// 止まったか、submit を受理済みで、もう状態を変えないか（R-SUBMIT）。
    pub(crate) fn is_closing(&self) -> bool {
        *self.shutdown.borrow()
            || matches!(
                *self.submit_state.lock().expect("submit poisoned"),
                SubmitState::Claimed
            )
    }
}

/// エージェントとのつながり。メモリだけの状態、API のトークン、`kemi wait` を起こす通知。
pub(crate) struct AgentChannel {
    /// エージェントとのつながりの、メモリだけに置く部分（R-AGENT-STATE）。`kemi wait` が
    /// 呼ばれたかどうかはセッション状態にあり、保留と復元をまたぐ。
    pub(crate) link: Mutex<AgentLink>,
    /// エージェント用の API のトークン。無ければその API を立てない。
    pub(crate) token: Option<String>,
    /// 待っている `kemi wait` を起こす（渡したとき）。
    pub(crate) wake: tokio::sync::Notify,
}

/// 保存と説明の更新を同じ門で進めるための、登録ページと画面が保持する説明の版。
pub(crate) struct PersistGuarded {
    pub(crate) registered_pages: std::collections::BTreeSet<String>,
    pub(crate) explanation_holds:
        std::collections::BTreeMap<String, std::collections::BTreeSet<u64>>,
}

pub(crate) struct AppState {
    /// 内容の読み取り（`ReviewSource` として）と、公開中のソースの取得・差し替え
    /// （`current` / `publish`）の両方に使う。
    pub sources: Arc<kemi_core::source::SourceSlot>,
    pub initial_review: Mutex<Option<freeze::FreezeInput>>,
    pub display_id: String,
    pub display_ready: std::sync::atomic::AtomicBool,
    pub highlighter: std::sync::OnceLock<crate::highlight::Highlighter>,
    /// blocking pool の既定の上限は CPU 計算には大きすぎるため、巨大な行データの同時保持も絞る。
    pub preparation: Arc<tokio::sync::Semaphore>,
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
    /// persist のスナップショットと保存を 1 つずつ進める（R-SESSION）。登録ページと画面が
    /// 保持する説明の版も、この門の中身として守る。
    pub(crate) persist: Mutex<PersistGuarded>,
    pub events: broadcast::Sender<Event>,
    pub(crate) agent: AgentChannel,
    pub(crate) stopping: Stopping,
    /// 凍結のタスクを 1 度だけ始めるための印（R-SESSION）。
    pub freeze_started: std::sync::atomic::AtomicBool,
    /// `--live` の中継の情報。トークンの URL で cookie を入れ、`api/review` に添える。
    pub live: Option<Arc<live::LiveInfo>>,
    pub(crate) clock: Clock,
}

impl AppState {
    /// 今の時刻（[`Clock`]）。エージェントの状態の時刻はすべてここから読む。
    pub(crate) fn now(&self) -> u128 {
        (self.clock)()
    }
}

/// レビュー中の実行時エラーで停止する。stdout に JSON を出さず終了コード 2（R-SUBMIT）。
pub(crate) fn stop_with_error(state: &AppState, message: impl Into<String>) {
    let _gate = state.persist.lock().expect("persist poisoned");
    {
        let mut stop = state.stopping.stop.lock().expect("stop poisoned");
        if stop.is_none() {
            *stop = Some(Stop::Failed(message.into()));
        }
    }
    let _ = state.stopping.shutdown.send(true);
}
