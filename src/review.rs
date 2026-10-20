//! レビューの起動・サーブ・終わり方（R-SERVE、R-SUBMIT、R-SESSION、R-AGENT-CLI、
//! R-PAGE-MODE、R-EXPLANATION-CLI）。

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::Arc;

use kemi_core::domain::live::LivePage;
use kemi_core::domain::submit::Verdict;
use kemi_core::session::PageSnapshot;
use kemi_core::source::ReviewSource;
use kemi_server::{
    AgentParams, Asset, Assets, LiveParams, LiveTarget, ResultSink, ServeControl, ServeOutcome,
    ServeParams, SessionSink, StoredSession, SuspendOutcome, bind_agent_listener,
    detect_share_address, exposure_warning, live_exposure_warning, serve, session_host,
    session_url,
};
use tokio::net::TcpListener;

use crate::cli::Cli;
use crate::notice;
use crate::report::{self, fail, warn};
use crate::results::{ResultStore, endpoints_dir, results_dir, results_unset_reason};

/// 書いた `<id>.endpoint`。submit・保留・実行時エラーのどれで終わるときも消す
/// （R-AGENT-LINK）。終わり方はどれも `process::exit` で Drop が走らないので、明示して消す。
struct EndpointFile {
    endpoint: kemi_core::session::Endpoint,
    dir: PathBuf,
    id: String,
}

impl EndpointFile {
    fn publish(&self) -> Result<(), String> {
        kemi_core::session::write_endpoint(&self.dir, &self.id, &self.endpoint)
            .map_err(|error| format!("cannot write the agent endpoint: {error}"))?;
        Ok(())
    }
    fn remove(&self) {
        if let Err(error) = kemi_core::session::remove_endpoint(&self.dir, &self.id) {
            warn(&format!("cannot remove the agent endpoint: {error}"));
        }
    }
}

/// エージェント用の API を立て、`<id>.endpoint` を書く（R-AGENT-CLI）。往復はセッションの
/// id と置き場所に頼るので、セッションのあるレビューだけ。立てられない・書けないときは
/// 警告だけで、今どおり submit だけでやりとりする。`defer` では書くのを呼び出し側に任せる。
async fn start_agent_channel(
    stored_session: Option<&StoredSession>,
    control: &ServeControl,
    defer: bool,
) -> Option<(AgentParams, EndpointFile)> {
    let id = stored_session?.review_id()?;
    let listener = match bind_agent_listener().await {
        Ok(listener) => listener,
        Err(error) => {
            warn(&format!(
                "cannot listen for kemi wait and kemi reply: {error}"
            ));
            return None;
        }
    };
    let token = random_token();
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => {
            warn(&format!(
                "cannot listen for kemi wait and kemi reply: {error}"
            ));
            return None;
        }
    };
    let Some(dir) = endpoints_dir() else {
        warn(&format!(
            "cannot determine where to write the agent endpoint ({})",
            results_unset_reason()
        ));
        return None;
    };
    let endpoint = kemi_core::session::Endpoint {
        port,
        token: token.clone(),
    };
    let endpoint_file = EndpointFile {
        dir,
        id: id.clone(),
        endpoint,
    };
    if !defer && let Err(error) = endpoint_file.publish() {
        warn(&error);
        return None;
    }
    Some((
        AgentParams {
            listener,
            token,
            review: id.clone(),
            control: control.clone(),
        },
        endpoint_file,
    ))
}

struct WebAssets;

impl Assets for WebAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        kemi_webview::get(path).map(|(bytes, mime)| Asset { bytes, mime })
    }
}

/// セッションの URL トークン（R-SERVE）。16 バイトの乱数を 32 桁の 16 進で返す
/// （出力の形は後方互換のため変えない）。失敗は終了コード 2 の一般エラーにする。
fn random_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .unwrap_or_else(|error| fail(&format!("cannot generate a random session token: {error}")));
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// ブラウザで URL を開く（R-INPUT の `--no-open` を付けなければ既定で開く）。
fn open_browser(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW（0x0800_0000）で、start のコンソールの窓を出さない。
        let _ = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(url)
            .creation_flags(0x0800_0000)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let command = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let _ = Command::new(command)
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// 復元できるセッションが残るときだけ、機械が読む 1 行を stderr に出す（R-SESSION）。
fn print_resume_hint(stored_session: Option<&StoredSession>) {
    if let Some(stored_session) = stored_session
        && let Some(id) = stored_session.resumable_id()
    {
        eprintln!("{}", report::resume_line(&id));
    }
}

/// `--live` の見る対象と配れる範囲の根（R-PAGE-MODE）。
pub(crate) struct LiveRun {
    pub(crate) page: LivePage,
    pub(crate) root: PathBuf,
    /// 復元で戻すスナップショット（R-PAGE-SESSION）。新しいレビューでは空。
    pub(crate) snapshots: Vec<PageSnapshot>,
    /// `<id>.files/` に残るスナップショットのファイルの最大の番号（読めないものも含む）。
    pub(crate) last_snapshot_number: u32,
    /// 根が git の作業ツリーの中で、コードの見方を出せるか。
    pub(crate) code_view: bool,
}

/// 中継のリスナーを立て、`kemi: live <url>` に出す中継の URL と一緒に返す（R-PAGE-MODE）。
/// `--bind` に従い、`--live-port` で固定できる（R-PAGE-PROXY）。
async fn start_live(cli: &Cli, live: LiveRun, host: Ipv4Addr) -> Option<(LiveParams, String)> {
    let target = LiveTarget::from_page(&live.page).ok()?;
    let start = target.start();
    let port = cli.live_port.unwrap_or(0);
    let listener = match TcpListener::bind((cli.bind, port)).await {
        Ok(listener) => listener,
        Err(error) => fail(&format!("cannot listen on {}:{port}: {error}", cli.bind)),
    };
    let actual = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => fail(&format!("cannot listen on {}:{port}: {error}", cli.bind)),
    };
    let url = format!("http://{host}:{actual}{start}");
    let params = LiveParams {
        listener,
        target,
        root: live.root,
        cookie: random_token(),
        mock_secret: random_token(),
        code_view: live.code_view,
        snapshots: live.snapshots,
        last_snapshot_number: live.last_snapshot_number,
    };
    Some((params, url))
}

/// 保留で待っている kemi wait に理由を返すのに待つ上限。返し終えればすぐ終わる。
const SUSPEND_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// 起動を知らせる stderr の行と、自動で開くブラウザ。
struct Announcement {
    url: String,
    /// 非ループバックで待つときの公開の警告（R-SERVE）。
    warning: Option<String>,
    /// 結果の保存先（R-RESULT）。決められないときは無い。
    results: Option<String>,
    /// エージェントとの往復があるときの、レビューの id（R-AGENT-CLI）。
    review: Option<String>,
    /// `--live` の中継の URL（R-PAGE-MODE）。
    live: Option<String>,
    /// 自動で開くときの URL。`0.0.0.0` で待つときも手元の loopback（R-SERVE）。
    browser: Option<String>,
}

impl Announcement {
    /// 行を `kemi: <url>` → 公開の警告 → 結果の保存先 → `kemi: review <id>` →
    /// `kemi: live <url>` の順に出してから、ブラウザを開く（R-PAGE-MODE）。起動時の説明が
    /// あると出すのは説明を保存した後になる（R-EXPLANATION-CLI）が、順は同じにする。
    fn announce(&self) {
        eprintln!("{}", report::url_line(&self.url));
        if let Some(warning) = &self.warning {
            eprintln!("{warning}");
        }
        match &self.results {
            Some(dir) => warn(&format!("results are saved to: {dir}")),
            None => warn(&format!(
                "cannot determine where to save results ({})",
                results_unset_reason()
            )),
        }
        if let Some(id) = &self.review {
            eprintln!("{}", report::review_line(id));
        }
        if let Some(url) = &self.live {
            eprintln!("{}", report::live_line(url));
        }
        if let Some(url) = &self.browser {
            open_browser(url);
        }
    }
}

/// サーブを始める前にそろえたもの。
struct ReviewStartup {
    listener: TcpListener,
    params: ServeParams,
    control: ServeControl,
    endpoint: Option<EndpointFile>,
    announcement: Announcement,
}

/// 待ち受け・URL と警告・結果の置き場所・エージェント・`--live` を用意する。待ち受けは
/// ここで済ませ、知らせる行は出さずに返す。
async fn prepare_review(
    cli: &Cli,
    source: Arc<dyn ReviewSource>,
    stored_session: Option<&Arc<StoredSession>>,
    results_key: String,
    live: Option<LiveRun>,
    initial_explanation: Option<kemi_core::domain::explanation::ExplanationDocument>,
) -> ReviewStartup {
    let listener = match TcpListener::bind((cli.bind, cli.port)).await {
        Ok(listener) => listener,
        Err(error) => fail(&format!(
            "cannot listen on {}:{}: {error}",
            cli.bind, cli.port
        )),
    };
    let token = random_token();
    // 共有アドレスは wildcard のときだけ特定する（R-SERVE）。
    let share_address = if cli.bind.is_unspecified() {
        detect_share_address()
    } else {
        None
    };
    let url = match session_url(&listener, &token, session_host(cli.bind, share_address)) {
        Ok(url) => url,
        Err(error) => fail(&error.to_string()),
    };
    // ブラウザは手元の loopback で開く（`0.0.0.0` の表示 URL は LAN アドレス）。
    let browser_url = match session_url(&listener, &token, session_host(cli.bind, None)) {
        Ok(url) => url,
        Err(error) => fail(&error.to_string()),
    };
    let warning = if live.is_some() {
        live_exposure_warning(cli.bind, share_address)
    } else {
        exposure_warning(cli.bind, share_address)
    };
    let results = ResultStore {
        dir: results_dir(),
        key: results_key,
    };
    let control = ServeControl::new();
    let (agent, endpoint) = match start_agent_channel(
        stored_session.map(Arc::as_ref),
        &control,
        cli.explanation.is_some(),
    )
    .await
    {
        Some((agent, endpoint)) => (Some(agent), Some(endpoint)),
        None => (None, None),
    };
    if cli.explanation.is_some() && agent.is_none() {
        fail("initial explanation requires a persistent session and agent endpoint");
    }
    let (live, live_url) = match live {
        Some(live) => match start_live(cli, live, session_host(cli.bind, share_address)).await {
            Some((params, url)) => (Some(params), Some(url)),
            None => (None, None),
        },
        None => (None, None),
    };
    let announcement = Announcement {
        url,
        warning,
        results: results.location(),
        review: endpoint.as_ref().map(|endpoint| endpoint.id.clone()),
        live: live_url,
        browser: (!cli.no_open).then_some(browser_url),
    };

    let params = ServeParams {
        source,
        assets: Arc::new(WebAssets),
        token,
        results: Some(Arc::new(results)),
        session: stored_session
            .cloned()
            .map(|session| session as Arc<dyn SessionSink>),
        notices: Arc::new(notice::StderrNotices),
        share_address,
        agent,
        live,
        initial_explanation,
        clock: kemi_server::system_clock(),
    };
    ReviewStartup {
        listener,
        params,
        control,
        endpoint,
        announcement,
    }
}

/// サーブして、submit・保留・実行時エラーのどれかで終わる。
pub(crate) async fn run_review(
    cli: &Cli,
    source: Arc<dyn ReviewSource>,
    stored_session: Option<Arc<StoredSession>>,
    results_key: String,
    live: Option<LiveRun>,
    initial_explanation: Option<kemi_core::domain::explanation::ExplanationDocument>,
) -> ! {
    // シグナルの受け口は URL を出す前に作る。URL を見てすぐ Ctrl+C しても、
    // 保留として 130 で終われるように（R-SESSION）。
    let mut interrupts = Interrupts::install();
    let ReviewStartup {
        listener,
        params,
        control,
        endpoint,
        announcement,
    } = prepare_review(
        cli,
        source,
        stored_session.as_ref(),
        results_key,
        live,
        initial_explanation,
    )
    .await;
    let remove_endpoint = || {
        if let Some(endpoint) = &endpoint {
            endpoint.remove();
        }
    };
    if cli.explanation.is_none() {
        announcement.announce();
    }
    // 保留のとき待っている kemi wait に理由を返せるよう、serve は別のタスクで回し、
    // シグナルを受けても捨てない。
    let mut serving = tokio::spawn(serve(listener, params));
    // 起動時の説明は保存してから URL を出す（R-EXPLANATION-CLI）。それを待つ間の保留と
    // 失敗も、サーブを始めた後と同じ片付けで終える。
    let ending = if cli.explanation.is_some() {
        tokio::select! {
            biased;
            joined = &mut serving => Some(match joined {
                Ok(outcome) => Ending::Stopped(outcome),
                Err(_) => fail("initial explanation could not start the review"),
            }),
            () = interrupts.recv() => Some(Ending::Interrupted),
            () = control.ready() => match endpoint.as_ref().map(EndpointFile::publish).transpose() {
                Ok(_) => {
                    announcement.announce();
                    None
                }
                Err(error) => Some(Ending::Failed(error)),
            },
        }
    } else {
        None
    };
    let ending = match ending {
        Some(ending) => ending,
        None => tokio::select! {
            biased;
            joined = &mut serving => match joined {
                Ok(outcome) => Ending::Stopped(outcome),
                Err(error) => fail(&format!("the server stopped unexpectedly: {error}")),
            },
            () = interrupts.recv() => Ending::Interrupted,
        },
    };
    remove_endpoint();

    finish_review(ending, stored_session, &control, serving).await
}

/// 保留の合図。unix は SIGINT と SIGTERM で、受け口を作った後に届いた合図は失わない。
/// ほかは Ctrl+C。
struct Interrupts {
    #[cfg(unix)]
    signals: (tokio::signal::unix::Signal, tokio::signal::unix::Signal),
}

impl Interrupts {
    fn install() -> Self {
        Interrupts {
            #[cfg(unix)]
            signals: {
                use tokio::signal::unix::{SignalKind, signal};
                (
                    signal(SignalKind::interrupt()).expect("install the SIGINT handler"),
                    signal(SignalKind::terminate()).expect("install the SIGTERM handler"),
                )
            },
        }
    }

    async fn recv(&mut self) {
        #[cfg(unix)]
        tokio::select! {
            _ = self.signals.0.recv() => {}
            _ = self.signals.1.recv() => {}
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

/// レビューの終わり方。
enum Ending {
    /// サーバが止まった（submit か、サーバの中の実行時エラー）。
    Stopped(Result<ServeOutcome, kemi_server::ServerError>),
    /// サーバの外の実行時エラー（起動時の説明を保存した後に、つなぎ先を書けないなど）。
    Failed(String),
    /// SIGINT / SIGTERM による保留。
    Interrupted,
}

/// 終わり方を stdout・セッションの片付け・終了コードにする。
async fn finish_review(
    ending: Ending,
    stored_session: Option<Arc<StoredSession>>,
    control: &ServeControl,
    mut serving: tokio::task::JoinHandle<Result<ServeOutcome, kemi_server::ServerError>>,
) -> ! {
    match ending {
        Ending::Stopped(Ok(ServeOutcome::Submitted(document))) => finish_submitted(document),
        Ending::Stopped(Err(error)) => {
            fail_keeping_session(&error.to_string(), stored_session.as_deref())
        }
        Ending::Failed(message) => {
            serving.abort();
            fail_keeping_session(&message, stored_session.as_deref());
        }
        Ending::Interrupted => {
            let (submitted, waiting) = match control.suspend() {
                SuspendOutcome::Submitted { document, waiting } => (Some(document), waiting),
                SuspendOutcome::Suspended { waiting } => (None, waiting),
            };
            // submit と保留のどちらでも、待っている kemi wait の応答を返し終えるまで待つ。
            let joined = if waiting {
                match tokio::time::timeout(SUSPEND_GRACE, &mut serving).await {
                    Ok(joined) => joined,
                    Err(_) => {
                        serving.abort();
                        serving.await
                    }
                }
            } else {
                // サーバの状態がセッションを握ったままだと、下の drop でロックを解放できない。
                serving.abort();
                serving.await
            };
            // stop が既に取り出されていても、abort より先に完了した submit の結果は残る。
            if let Ok(Ok(ServeOutcome::Submitted(document))) = joined {
                finish_submitted(document);
            }
            if let Some(document) = submitted {
                finish_submitted(document);
            }
            // 片付けた後のセッションは `<id>.files/` に書かない。abort しても接続ごとの
            // タスクは止まらず、処理中のスナップショットや画像が後から届くことがあるが、
            // それは書かれず、消した `<id>.files/` は作り直されない。
            close_suspended(stored_session.as_deref());
            print_resume_hint(stored_session.as_deref());
            drop(stored_session);
            std::process::exit(report::INTERRUPTED);
        }
    }
}

/// 実行時エラーで終わる。保留と同じくセッションを片付け、復元できるセッションが残るなら
/// 案内を出す（R-SESSION）。
fn fail_keeping_session(message: &str, stored_session: Option<&StoredSession>) -> ! {
    close_suspended(stored_session);
    print_resume_hint(stored_session);
    fail(message);
}

fn finish_submitted(document: serde_json::Value) -> ! {
    match serde_json::to_string(&document) {
        Ok(json) => report::write_stdout(&format!("{json}\n")),
        Err(error) => fail(&format!("cannot build the submit JSON: {error}")),
    }
    let code = document
        .get("verdict")
        .and_then(|value| value.as_str())
        .and_then(Verdict::parse)
        .map_or(report::ERROR, Verdict::exit_code);
    std::process::exit(code);
}

/// 保留か実行時エラーで終わるとき、会話の無い `--live` のセッションの `<id>.files/` を消す
/// （R-PAGE-SESSION）。失敗は警告だけにする（R-SESSION）。
fn close_suspended(stored_session: Option<&StoredSession>) {
    if let Some(stored_session) = stored_session
        && let Err(error) = stored_session.close_suspended()
    {
        warn(&format!("could not remove the session files: {error}"));
    }
}
