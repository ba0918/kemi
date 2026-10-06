//! kemi の CLI（R-INPUT-6, R-SUBMIT）。stdout は submit の JSON だけに使う。

mod agent;
mod local_time;
mod notice;
mod result;
mod session;

use std::io::IsTerminal;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::Arc;

use kemi_core::domain::live::{LiveError, LivePage, is_html, parse_live_url, served_path};
use kemi_core::session::{SessionInfo, SessionMode, SessionStore, SessionSummary, now_millis};
use kemi_core::source::git::{GitMode, GitSource, GroupBy};
use kemi_core::source::manifest::ManifestSource;
use kemi_core::source::{FocusSource, LiveSource, ReviewSource};
use kemi_server::{
    AgentParams, Asset, Assets, LiveParams, LiveTarget, ResultSaveError, ResultSink, ServeControl,
    ServeOutcome, ServeParams, SessionSink, bind_agent_listener, detect_share_address,
    exposure_warning, live_exposure_warning, serve, session_host, session_url,
};
use tokio::net::TcpListener;

const USAGE: &str = "\
kemi: usage:
  kemi <manifest.json | ->                 read a manifest
  kemi --from <ref> [--to <ref>]          diff a commit range (--to defaults to HEAD)
  kemi --worktree                          review uncommitted changes
  kemi --staged                            review staged changes
  kemi --live <url | file.html>            review a page on a loopback dev server, or a local HTML file
  kemi --result [--any | --workspace <path>]  print the JSON of the latest submitted result
  kemi wait <id> [--timeout <seconds>]    wait for what the reviewer hands over (agent)
  kemi reply <id>                          write replies and messages from stdin JSON (agent)

common flags:
  --focus <path>     focus layer JSON (relative to --base)
  --base <dir>       base for manifest and --focus relative paths (default .)
  --group-by <mode>  group unit at startup: file (final form) | commit (per commit). default file
  --port <n>         listen port (default 0 = pick a free one)
  --bind <addr>      listen address as an IPv4 literal (default 127.0.0.1)
  --no-open          do not open the browser automatically
  --serve            accepted for compatibility (serving is always on)
  --digest           print a digest to stdout and exit
  --digest-top <n>   number of top files in the digest (default 100)
  --live-port <n>    listen port of the --live page (default 0 = pick a free one)

flags used with --result:
  --any              print the latest result regardless of location
  --workspace <path> print the result for this place instead of the launch directory";

struct Cli {
    manifest: Option<String>,
    from: Option<String>,
    to: Option<String>,
    group_by: Option<String>,
    worktree: bool,
    staged: bool,
    /// `--live` の値（URL かファイル）。
    live: Option<String>,
    live_port: Option<u16>,
    focus: Option<String>,
    base: PathBuf,
    port: u16,
    bind: Ipv4Addr,
    no_open: bool,
    digest: bool,
    digest_top: usize,
    serve: bool,
    out: Option<String>,
    result: bool,
    any: bool,
    workspace: Option<PathBuf>,
    /// `--resume`。`Some(None)` は id の省略。
    resume: Option<Option<String>>,
    version: bool,
    help: bool,
    /// 指定されたフラグの名前（値は含めない）。`--result` と組み合わせられるかの判定に使う。
    flags: Vec<String>,
}

fn parse_args(args: Vec<String>) -> Result<Cli, String> {
    let mut cli = Cli {
        manifest: None,
        from: None,
        to: None,
        group_by: None,
        worktree: false,
        staged: false,
        live: None,
        live_port: None,
        focus: None,
        base: PathBuf::from("."),
        port: 0,
        bind: Ipv4Addr::LOCALHOST,
        no_open: false,
        digest: false,
        digest_top: 100,
        serve: false,
        out: None,
        result: false,
        any: false,
        workspace: None,
        resume: None,
        version: false,
        help: false,
        flags: Vec::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = |index: &mut usize| -> Result<String, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{arg} requires a value"))
        };
        if arg.starts_with("--") {
            cli.flags.push(arg.to_string());
        }
        match arg {
            "--version" => cli.version = true,
            "--help" | "-h" => cli.help = true,
            "--from" => cli.from = Some(value(&mut index)?),
            "--to" => cli.to = Some(value(&mut index)?),
            "--group-by" => cli.group_by = Some(value(&mut index)?),
            "--worktree" => cli.worktree = true,
            "--staged" => cli.staged = true,
            "--live" => cli.live = Some(value(&mut index)?),
            "--live-port" => {
                cli.live_port = Some(
                    value(&mut index)?
                        .parse()
                        .map_err(|_| "--live-port requires a number".to_string())?,
                )
            }
            "--focus" => cli.focus = Some(value(&mut index)?),
            "--base" => cli.base = PathBuf::from(value(&mut index)?),
            "--port" => {
                cli.port = value(&mut index)?
                    .parse()
                    .map_err(|_| "--port requires a number".to_string())?
            }
            "--bind" => {
                let value = value(&mut index)?;
                cli.bind = value
                    .parse()
                    .map_err(|_| format!("--bind requires an IPv4 address literal: {value}"))?
            }
            "--no-open" => cli.no_open = true,
            "--digest" => cli.digest = true,
            "--digest-top" => {
                cli.digest_top = value(&mut index)?
                    .parse()
                    .map_err(|_| "--digest-top requires a number".to_string())?
            }
            "--serve" => cli.serve = true,
            "--out" => cli.out = Some(value(&mut index)?),
            "--result" => cli.result = true,
            "--any" => cli.any = true,
            "--workspace" => cli.workspace = Some(PathBuf::from(value(&mut index)?)),
            "--resume" => {
                // id は省略できる。次の引数がフラグでなければ id として取る。
                let id = args
                    .get(index + 1)
                    .filter(|next| !next.starts_with('-'))
                    .cloned();
                if id.is_some() {
                    index += 1;
                }
                cli.resume = Some(id);
            }
            "-" => cli.manifest = Some("-".to_string()),
            other if other.starts_with('-') => return Err(format!("unknown flag: {other}")),
            path => {
                if cli.manifest.is_some() {
                    return Err("only one manifest can be given".to_string());
                }
                cli.manifest = Some(path.to_string());
            }
        }
        index += 1;
    }
    Ok(cli)
}

/// `--result` / `--any` / `--workspace` の組み合わせを確かめる（R-INPUT-6）。
fn validate_result_flags(cli: &Cli) -> Result<(), String> {
    if !cli.result {
        if cli.any || cli.workspace.is_some() {
            return Err("--any and --workspace require --result".to_string());
        }
        return Ok(());
    }
    let others = cli
        .flags
        .iter()
        .any(|flag| !matches!(flag.as_str(), "--result" | "--any" | "--workspace"));
    if others || cli.manifest.is_some() {
        return Err("--result accepts only --any and --workspace".to_string());
    }
    if cli.any && cli.workspace.is_some() {
        return Err("--any and --workspace cannot be used together".to_string());
    }
    Ok(())
}

/// `--resume` と一緒に使えるのは `--port` / `--bind` / `--no-open` / `--serve` /
/// `--live-port` だけ（R-INPUT-6）。
fn validate_resume_flags(cli: &Cli) -> Result<(), String> {
    if cli.resume.is_none() {
        return Ok(());
    }
    let allowed = [
        "--resume",
        "--port",
        "--bind",
        "--no-open",
        "--serve",
        "--live-port",
    ];
    let others = cli
        .flags
        .iter()
        .any(|flag| !allowed.contains(&flag.as_str()));
    if others || cli.manifest.is_some() {
        return Err(
            "--resume accepts only --port, --bind, --no-open, --serve and --live-port".to_string(),
        );
    }
    Ok(())
}

/// `--live` と組めるフラグだけかを確かめる（R-PAGE-MODE）。`--live-port` は `--live` か
/// `--resume` とだけ組める（R-INPUT-6）。
fn validate_live_flags(cli: &Cli) -> Result<(), String> {
    if cli.live.is_none() {
        if cli.live_port.is_some() && cli.resume.is_none() {
            return Err("--live-port requires --live or --resume".to_string());
        }
        return Ok(());
    }
    let allowed = [
        "--live",
        "--port",
        "--bind",
        "--no-open",
        "--live-port",
        "--focus",
        "--serve",
    ];
    let others = cli
        .flags
        .iter()
        .any(|flag| !allowed.contains(&flag.as_str()));
    if others || cli.manifest.is_some() {
        return Err(
            "--live accepts only --port, --bind, --no-open, --live-port, --focus and --serve"
                .to_string(),
        );
    }
    Ok(())
}

/// `--live` の見る対象と配れる範囲の根を決める（R-PAGE-MODE）。根は起動したディレクトリが
/// git の作業ツリーの中なら作業ツリーの根、外なら起動したディレクトリ。ファイルは
/// シンボリックリンクを解決した実体の場所で判定する。
fn live_target(value: &str) -> Result<(LivePage, PathBuf), String> {
    let root = std::fs::canonicalize(workspace_root(Path::new(".")))
        .map_err(|error| format!("cannot resolve the launch directory: {error}"))?;
    if value.contains("://") {
        let url = parse_live_url(value).map_err(|error| error.to_string())?;
        return Ok((LivePage::Url(url.text), root));
    }
    let real =
        std::fs::canonicalize(value).map_err(|error| format!("cannot read {value}: {error}"))?;
    let path = served_path(&root, &real).map_err(|error| error.to_string())?;
    if !is_html(&path) {
        return Err(LiveError::NotHtml(value.to_string()).to_string());
    }
    Ok((LivePage::File(path), root))
}

/// 結果の置き場所を決める環境変数が無いときの理由（R-RESULT）。OS ごとに使う変数が違う。
fn results_unset_reason() -> &'static str {
    if cfg!(windows) {
        "LOCALAPPDATA is not set or not absolute"
    } else {
        "HOME is not set"
    }
}

/// 結果ファイルの置き場所（R-RESULT）。
fn results_dir() -> Option<PathBuf> {
    result::results_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// セッションの置き場所（R-SESSION）。結果ファイルと同じ根の下。
fn sessions_dir() -> Option<PathBuf> {
    result::sessions_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// `<id>.endpoint` の置き場所（R-AGENT-LINK）。
fn endpoints_dir() -> Option<PathBuf> {
    result::endpoint_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// 書いた `<id>.endpoint`。submit・保留・実行時エラーのどれで終わるときも消す
/// （R-AGENT-LINK）。終わり方はどれも `process::exit` で Drop が走らないので、明示して消す。
struct EndpointFile {
    dir: PathBuf,
    id: String,
}

impl EndpointFile {
    fn remove(&self) {
        if let Err(error) = kemi_core::session::remove_endpoint(&self.dir, &self.id) {
            eprintln!("kemi: cannot remove the agent endpoint: {error}");
        }
    }
}

/// エージェント用の API を立て、`<id>.endpoint` を書き、`kemi: review <id>` を出す
/// （R-AGENT-CLI）。往復はセッションの id と置き場所に頼るので、セッションのあるレビュー
/// だけ。立てられない・書けないときは警告だけで、今どおり submit だけでやりとりする。
async fn start_agent_channel(
    stored_session: Option<&session::StoredSession>,
    control: &ServeControl,
) -> Option<(AgentParams, EndpointFile)> {
    let id = stored_session?.info().id;
    let listener = match bind_agent_listener().await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("kemi: cannot listen for kemi wait and kemi reply: {error}");
            return None;
        }
    };
    let token = random_token();
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => {
            eprintln!("kemi: cannot listen for kemi wait and kemi reply: {error}");
            return None;
        }
    };
    let Some(dir) = endpoints_dir() else {
        eprintln!(
            "kemi: cannot determine where to write the agent endpoint ({})",
            results_unset_reason()
        );
        return None;
    };
    let endpoint = kemi_core::session::Endpoint {
        port,
        token: token.clone(),
    };
    if let Err(error) = kemi_core::session::write_endpoint(&dir, &id, &endpoint) {
        eprintln!("kemi: cannot write the agent endpoint: {error}");
        return None;
    }
    eprintln!("kemi: review {id}");
    Some((
        AgentParams {
            listener,
            token,
            control: control.clone(),
        },
        EndpointFile { dir, id },
    ))
}

/// 起動時の入力から、セッションに記録する情報を組み立てる。コミット範囲は完全な sha に
/// 解決しておく（R-SESSION）。
fn session_info(cli: &Cli, live: Option<&(LivePage, PathBuf)>) -> Result<SessionInfo, String> {
    let workspace = workspace_root(Path::new("."));
    let mode = if let Some((page, root)) = live {
        SessionMode::Live {
            page: page.clone(),
            root: root.clone(),
        }
    } else if cli.worktree {
        SessionMode::Worktree
    } else if cli.staged {
        SessionMode::Staged
    } else if cli.manifest.is_some() {
        SessionMode::Manifest
    } else {
        let from = cli.from.clone().unwrap_or_default();
        let to = cli.to.clone().unwrap_or_else(|| "HEAD".to_string());
        let repo =
            kemi_core::source::git::repo_root(Path::new(".")).map_err(|error| error.to_string())?;
        let (from_sha, to_sha) = kemi_core::session::resolve_range(&repo, &from, &to)
            .map_err(|error| error.to_string())?;
        SessionMode::Range {
            from,
            to,
            from_sha,
            to_sha,
        }
    };
    let now = now_millis();
    Ok(SessionInfo {
        id: kemi_core::session::new_ulid(),
        created: now,
        updated: now,
        workspace_key: result::workspace_key(&workspace),
        workspace,
        mode,
        title: String::new(),
        total_files: 0,
    })
}

/// セッションの保存先を開く。決められない・開けないときは警告だけを出し、セッション
/// なしでレビューを続ける。
fn open_session(
    cli: &Cli,
    live: Option<&(LivePage, PathBuf)>,
) -> Option<Arc<session::StoredSession>> {
    let Some(dir) = sessions_dir() else {
        eprintln!(
            "kemi: cannot determine where to save sessions ({})",
            results_unset_reason()
        );
        return None;
    };
    let info = match session_info(cli, live) {
        Ok(info) => info,
        Err(message) => {
            eprintln!("kemi: cannot start a session: {message}");
            return None;
        }
    };
    match session::StoredSession::start(&SessionStore::new(dir), info) {
        Ok(session) => Some(Arc::new(session)),
        Err(error) => {
            eprintln!("kemi: cannot start a session: {error}");
            None
        }
    }
}

/// 結果を識別するリポジトリのトップ。git の外ならその場所そのもの。
fn workspace_root(path: &Path) -> PathBuf {
    kemi_core::source::git::repo_root(path)
        .or_else(|_| std::fs::canonicalize(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// `kemi --result`: 最新の結果ファイルの中身を stdout に出し、元の verdict の終了コードで
/// 終わる。該当が無ければ何も出さず終了コード 2。
fn print_result(cli: &Cli) -> ! {
    let Some(dir) = results_dir() else {
        fail(&format!(
            "cannot determine where to store results ({})",
            results_unset_reason()
        ));
    };
    let key = (!cli.any).then(|| {
        let place = cli.workspace.clone().unwrap_or_else(|| PathBuf::from("."));
        result::workspace_key(&workspace_root(&place))
    });
    let Some(text) = result::load_latest(&dir, key.as_deref()) else {
        fail("no result for this location");
    };
    // 壊れたファイルを stdout に出してから終了コード 2 にしないよう、先に読み解く。
    let verdict = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|document| document["verdict"].as_str().map(str::to_string));
    let code = match verdict.as_deref() {
        Some("approved") => 0,
        Some("changes_requested") => 1,
        _ => fail("cannot read the latest result file"),
    };
    print!("{text}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}

/// 結果ファイルを書く先。submit を確定したときにサーバから呼ばれる。
struct ResultStore {
    dir: Option<PathBuf>,
    key: String,
}

impl ResultSink for ResultStore {
    fn save(&self, text: &str) -> Result<PathBuf, ResultSaveError> {
        let dir = self
            .dir
            .as_ref()
            .ok_or_else(|| ResultSaveError::Unlocated(results_unset_reason().to_string()))?;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        result::save(dir, &self.key, text, millis, std::process::id()).map_err(|source| {
            ResultSaveError::Write {
                dir: dir.clone(),
                source,
            }
        })
    }

    fn location(&self) -> Option<String> {
        self.dir
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned())
    }
}

fn group_by(cli: &Cli) -> Result<GroupBy, String> {
    match cli.group_by.as_deref() {
        None | Some("file") => Ok(GroupBy::File),
        Some("commit") => Ok(GroupBy::Commit),
        Some(other) => Err(format!("--group-by must be commit or file: {other}")),
    }
}

fn build_source(
    cli: &Cli,
    live: Option<&(LivePage, PathBuf)>,
) -> Result<Box<dyn ReviewSource>, String> {
    if let Some((page, root)) = live {
        return Ok(Box::new(LiveSource::new(page, root)));
    }
    if let Some(manifest) = &cli.manifest {
        let source = ManifestSource::from_path(Path::new(manifest), &cli.base)
            .map_err(|error| error.to_string())?;
        return Ok(Box::new(source));
    }
    if cli.worktree {
        return Ok(Box::new(
            GitSource::open(GitMode::Worktree).map_err(|error| error.to_string())?,
        ));
    }
    if cli.staged {
        return Ok(Box::new(
            GitSource::open(GitMode::Staged).map_err(|error| error.to_string())?,
        ));
    }
    let from = cli.from.clone().unwrap_or_default();
    let to = cli.to.clone().unwrap_or_else(|| "HEAD".to_string());
    Ok(Box::new(
        GitSource::open(GitMode::Range {
            from,
            to,
            group_by: group_by(cli)?,
        })
        .map_err(|error| error.to_string())?,
    ))
}

fn with_focus(source: Box<dyn ReviewSource>, cli: &Cli) -> Result<Box<dyn ReviewSource>, String> {
    match &cli.focus {
        Some(focus) => Ok(Box::new(
            FocusSource::from_path(source, Path::new(focus), &cli.base)
                .map_err(|error| error.to_string())?,
        )),
        None => Ok(source),
    }
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

fn fail(message: &str) -> ! {
    eprintln!("kemi: {message}");
    std::process::exit(2);
}

/// 復元できるセッションが残るときだけ、機械が読む 1 行を stderr に出す（R-SESSION）。
fn print_resume_hint(stored_session: Option<&session::StoredSession>) {
    if let Some(stored_session) = stored_session
        && let Some(line) = stored_session.resume_line()
    {
        eprintln!("{line}");
    }
}

/// `--live` の見る対象と配れる範囲の根（R-PAGE-MODE）。
struct LiveRun {
    page: LivePage,
    root: PathBuf,
}

impl LiveRun {
    /// 中継の相手と、最初に開くパス。
    fn target(&self) -> Option<(LiveTarget, String)> {
        match &self.page {
            LivePage::Url(text) => {
                let url = parse_live_url(text).ok()?;
                Some((
                    LiveTarget::Url {
                        authority: url.authority,
                        start: url.path_and_query.clone(),
                        display: url.text,
                    },
                    url.path_and_query,
                ))
            }
            LivePage::File(path) => Some((
                LiveTarget::File { path: path.clone() },
                format!("/{}", kemi_core::domain::live::url_path(path)),
            )),
        }
    }
}

/// 中継のリスナーを立て、`kemi: live <url>` を出す（R-PAGE-MODE）。`--bind` に従い、
/// `--live-port` で固定できる（R-PAGE-PROXY）。
async fn start_live(cli: &Cli, live: &LiveRun, host: Ipv4Addr) -> Option<LiveParams> {
    let (target, start) = live.target()?;
    let port = cli.live_port.unwrap_or(0);
    let listener = match TcpListener::bind((cli.bind, port)).await {
        Ok(listener) => listener,
        Err(error) => fail(&format!("cannot listen on {}:{port}: {error}", cli.bind)),
    };
    let actual = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => fail(&format!("cannot listen on {}:{port}: {error}", cli.bind)),
    };
    eprintln!("kemi: live http://{host}:{actual}{start}");
    Some(LiveParams {
        listener,
        target,
        root: live.root.clone(),
        cookie: random_token(),
        mock_secret: random_token(),
        code_view: kemi_core::source::git::repo_root(&live.root).is_ok(),
    })
}

/// 保留で待っている kemi wait に理由を返すのに待つ上限。返し終えればすぐ終わる。
const SUSPEND_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// サーブして、submit・保留・実行時エラーのどれかで終わる。
async fn run_review(
    cli: &Cli,
    source: Arc<dyn ReviewSource>,
    stored_session: Option<Arc<session::StoredSession>>,
    results_key: String,
    live: Option<LiveRun>,
) -> ! {
    // --serve は互換のための受理のみ。既定で常にサーブする。
    let _ = cli.serve;
    // シグナルの受け口は URL を出す前に作る。URL を見てすぐ Ctrl+C しても、
    // 保留として 130 で終われるように（R-SESSION）。
    #[cfg(unix)]
    let mut signals = {
        use tokio::signal::unix::{SignalKind, signal};
        (
            signal(SignalKind::interrupt()).expect("install the SIGINT handler"),
            signal(SignalKind::terminate()).expect("install the SIGTERM handler"),
        )
    };
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
    eprintln!("kemi: {url}");
    let warning = if live.is_some() {
        live_exposure_warning(cli.bind, share_address)
    } else {
        exposure_warning(cli.bind, share_address)
    };
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    let results = ResultStore {
        dir: results_dir(),
        key: results_key,
    };
    match results.location() {
        Some(dir) => eprintln!("kemi: results are saved to: {dir}"),
        None => eprintln!(
            "kemi: cannot determine where to save results ({})",
            results_unset_reason()
        ),
    }
    let control = ServeControl::new();
    let (agent, endpoint) = match start_agent_channel(stored_session.as_deref(), &control).await {
        Some((agent, endpoint)) => (Some(agent), Some(endpoint)),
        None => (None, None),
    };
    let live = match &live {
        Some(live) => start_live(cli, live, session_host(cli.bind, share_address)).await,
        None => None,
    };
    let remove_endpoint = || {
        if let Some(endpoint) = &endpoint {
            endpoint.remove();
        }
    };
    if !cli.no_open {
        // ブラウザは手元の loopback で開く（`0.0.0.0` の表示 URL は LAN アドレス）。
        let browser_url = match session_url(&listener, &token, session_host(cli.bind, None)) {
            Ok(url) => url,
            Err(error) => fail(&error.to_string()),
        };
        open_browser(&browser_url);
    }

    let params = ServeParams {
        source,
        assets: Arc::new(WebAssets),
        token,
        results: Some(Arc::new(results)),
        session: stored_session
            .clone()
            .map(|session| session as Arc<dyn SessionSink>),
        notices: Arc::new(notice::StderrNotices),
        share_address,
        agent,
        live,
    };
    // 保留のとき待っている kemi wait に理由を返せるよう、serve は別のタスクで回し、
    // シグナルを受けても捨てない。
    let mut serving = tokio::spawn(serve(listener, params));
    #[cfg(unix)]
    let outcome = tokio::select! {
        outcome = &mut serving => Some(outcome),
        _ = signals.0.recv() => None,
        _ = signals.1.recv() => None,
    };
    #[cfg(not(unix))]
    let outcome = tokio::select! {
        outcome = &mut serving => Some(outcome),
        _ = tokio::signal::ctrl_c() => None,
    };
    let outcome = outcome.map(|joined| match joined {
        Ok(outcome) => outcome,
        Err(error) => fail(&format!("the server stopped unexpectedly: {error}")),
    });
    remove_endpoint();

    match outcome {
        Some(Ok(ServeOutcome::Submitted(document))) => {
            match serde_json::to_string(&document) {
                Ok(json) => println!("{json}"),
                Err(error) => fail(&format!("cannot build the submit JSON: {error}")),
            }
            let code = match document.get("verdict").and_then(|value| value.as_str()) {
                Some("approved") => 0,
                Some("changes_requested") => 1,
                _ => 2,
            };
            std::process::exit(code);
        }
        Some(Err(error)) => {
            // 実行時エラーでも、復元できるセッションが残るなら案内を出す（R-SESSION）。
            print_resume_hint(stored_session.as_deref());
            fail(&error.to_string());
        }
        None => {
            // 待っている kemi wait があれば、保留で終わる理由を返し終えるまで待つ。無ければ
            // 今までどおりすぐ終わる（R-AGENT-CLI）。
            let finished = control.suspend()
                && tokio::time::timeout(SUSPEND_GRACE, &mut serving)
                    .await
                    .is_ok();
            if !finished {
                // サーバの状態がセッションを握ったままだと、下の drop でロックを解放できない。
                serving.abort();
                let _ = serving.await;
            }
            print_resume_hint(stored_session.as_deref());
            drop(stored_session);
            std::process::exit(130);
        }
    }
}

/// `--resume`: セッションを開き、凍結したレビューをサーブする（R-SESSION）。
async fn run_resume(cli: &Cli) -> ! {
    let Some(dir) = sessions_dir() else {
        fail(&format!(
            "cannot determine where sessions are stored ({})",
            results_unset_reason()
        ));
    };
    let store = SessionStore::new(dir);
    let id = match &cli.resume {
        Some(Some(id)) => id.clone(),
        Some(None) => choose_session(&store),
        None => unreachable!("run_resume is called only with --resume"),
    };
    let stored = match session::StoredSession::resume(&store, &id) {
        Ok(stored) => stored,
        Err(error) => fail(&error.to_string()),
    };
    // `--live` は写しを持たず、今の作業ツリーを読み直し、同じページにつなぎ直す
    // （R-PAGE-SESSION）。
    let mut live = None;
    let source = match stored.info().mode {
        SessionMode::Live { page, root } => {
            let source = Arc::new(LiveSource::new(&page, &root)) as Arc<dyn ReviewSource>;
            live = Some(LiveRun { page, root });
            source
        }
        SessionMode::Worktree
        | SessionMode::Staged
        | SessionMode::Manifest
        | SessionMode::Range { .. } => match stored.frozen_source() {
            Some(source) => Arc::new(source) as Arc<dyn ReviewSource>,
            None => fail(&format!("session {id} cannot be resumed")),
        },
    };
    let results_key = stored.info().workspace_key;
    run_review(cli, source, Some(Arc::new(stored)), results_key, live).await
}

/// `id` なしの起動。端末なら選択画面、端末でなければ一覧を出して終わる（R-SESSION）。
fn choose_session(store: &SessionStore) -> String {
    let sessions = match store.list() {
        Ok(sessions) => sessions,
        Err(error) => fail(&error.to_string()),
    };
    if sessions.is_empty() {
        fail(&no_resumable_reason(store));
    }
    if !std::io::stdin().is_terminal() {
        list_sessions(store, &sessions);
    }
    pick_session(&sessions)
}

/// 復元できるセッションが 1 つも無い理由（R-SESSION）。読めない版のセッションしか
/// 無いときは、1 つも保留していない場合と同じ案内で終わらせない。
fn no_resumable_reason(store: &SessionStore) -> String {
    if store.has_unreadable_version() {
        format!(
            "no resumable session: {} holds sessions saved in a format version this kemi cannot read",
            store.dir().display()
        )
    } else {
        "no resumable session".to_string()
    }
}

/// 端末の選択画面。Esc と Ctrl+C では何も変えず終了コード 130（R-SESSION）。
fn pick_session(sessions: &[SessionSummary]) -> String {
    use inquire::{InquireError, Select};
    let labels = session::select_labels(sessions);
    match Select::new("Select a session to resume (Esc cancels)", labels.clone()).prompt() {
        Ok(label) => {
            let index = labels
                .iter()
                .position(|candidate| candidate == &label)
                .expect("the chosen label came from the list");
            sessions[index].id.clone()
        }
        Err(InquireError::OperationCanceled) | Err(InquireError::OperationInterrupted) => {
            std::process::exit(130)
        }
        Err(error) => fail(&format!("cannot choose a session: {error}")),
    }
}

/// 復元できるセッションの一覧を stdout に出す（R-SESSION）。0 件なら理由を出して 2。
fn list_sessions(store: &SessionStore, sessions: &[SessionSummary]) -> ! {
    if sessions.is_empty() {
        fail(&no_resumable_reason(store));
    }
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for summary in sessions {
        let _ = writeln!(out, "{}", session::list_line(summary));
    }
    let _ = out.flush();
    std::process::exit(0);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // 最初の引数が wait / reply ならサブコマンド。その名前の manifest は ./wait と書く
    // （R-AGENT-CLI）。
    if let Some(name @ ("wait" | "reply")) = args.first().map(String::as_str) {
        let command = match agent::parse(name, &args[1..]) {
            Ok(command) => command,
            Err(message) => {
                eprintln!("kemi: {message}");
                eprintln!("{}", agent::USAGE);
                std::process::exit(2);
            }
        };
        let code = tokio::task::spawn_blocking(move || {
            agent::run(command, sessions_dir(), endpoints_dir())
        })
        .await
        .unwrap_or_else(|error| fail(&format!("kemi {name} stopped unexpectedly: {error}")));
        std::process::exit(code);
    }
    let cli = match parse_args(args) {
        Ok(cli) => cli,
        Err(message) => fail(&message),
    };

    if cli.version {
        println!("kemi {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if cli.help {
        eprintln!("{USAGE}");
        return;
    }
    if let Err(message) = validate_result_flags(&cli) {
        fail(&message);
    }
    if cli.result {
        print_result(&cli);
    }
    if let Err(message) = validate_resume_flags(&cli) {
        fail(&message);
    }
    if let Err(message) = validate_live_flags(&cli) {
        fail(&message);
    }

    if let Some(out) = &cli.out {
        fail(&format!(
            "--out is removed; static files are no longer written ({out} will not be written)"
        ));
    }

    if cli.resume.is_some() {
        run_resume(&cli).await;
    }

    let modes = [
        cli.manifest.is_some(),
        cli.from.is_some(),
        cli.worktree,
        cli.staged,
        cli.live.is_some(),
    ]
    .iter()
    .filter(|present| **present)
    .count();
    if modes > 1 {
        fail("specify exactly one input mode");
    }
    if modes == 0 {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    if cli.to.is_some() && cli.from.is_none() {
        fail("--to requires --from");
    }
    if cli.group_by.is_some() && cli.from.is_none() {
        fail("--group-by requires --from");
    }

    let live = match cli.live.as_deref().map(live_target).transpose() {
        Ok(live) => live,
        Err(message) => fail(&message),
    };
    let source = match build_source(&cli, live.as_ref()).and_then(|source| with_focus(source, &cli))
    {
        Ok(source) => source,
        Err(message) => fail(&message),
    };
    let source: Arc<dyn ReviewSource> = Arc::from(source);

    if cli.digest {
        match source.review() {
            Ok(review) => {
                let digest = kemi_core::domain::digest::build_digest(&review, cli.digest_top);
                match serde_json::to_string(&digest) {
                    Ok(json) => {
                        println!("{json}");
                        return;
                    }
                    Err(error) => fail(&format!("cannot build the digest JSON: {error}")),
                }
            }
            Err(error) => fail(&error.to_string()),
        }
    }

    let stored_session = open_session(&cli, live.as_ref());
    let results_key = result::workspace_key(&workspace_root(Path::new(".")));
    let live = live.map(|(page, root)| LiveRun { page, root });
    run_review(&cli, source, stored_session, results_key, live).await
}
