//! kemi の CLI（R-INPUT-6, R-SUBMIT）。stdout は submit の JSON だけに使う。

mod result;

use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::Arc;

use kemi_core::source::git::{GitMode, GitSource, GroupBy};
use kemi_core::source::manifest::ManifestSource;
use kemi_core::source::{FocusSource, ReviewSource};
use kemi_server::{serve, session_url, Asset, Assets, ResultSink, ServeOutcome, ServeParams};
use tokio::net::TcpListener;

const USAGE: &str = "\
kemi: usage:
  kemi <manifest.json | ->                 read a manifest
  kemi --from <ref> [--to <ref>]          diff a commit range (--to defaults to HEAD)
  kemi --worktree                          review uncommitted changes
  kemi --staged                            review staged changes
  kemi --result [--any | --workspace <path>]  print the JSON of the latest submitted result

common flags:
  --focus <path>     focus layer JSON (relative to --base)
  --base <dir>       base for manifest and --focus relative paths (default .)
  --group-by <mode>  group unit at startup: file (final form) | commit (per commit). default file
  --port <n>         listen port (default 0 = pick a free one)
  --no-open          do not open the browser automatically
  --serve            accepted for compatibility (serving is always on)
  --digest           print a digest to stdout and exit
  --digest-top <n>   number of top files in the digest (default 100)

flags used with --result:
  --any              print the latest result regardless of location
  --workspace <path> print the result for this place instead of the launch directory";

#[derive(Default)]
struct Cli {
    manifest: Option<String>,
    from: Option<String>,
    to: Option<String>,
    group_by: Option<String>,
    worktree: bool,
    staged: bool,
    focus: Option<String>,
    base: PathBuf,
    port: u16,
    no_open: bool,
    digest: bool,
    digest_top: usize,
    serve: bool,
    out: Option<String>,
    result: bool,
    any: bool,
    workspace: Option<PathBuf>,
    version: bool,
    help: bool,
    /// 指定されたフラグの名前（値は含めない）。`--result` と組み合わせられるかの判定に使う。
    flags: Vec<String>,
}

fn parse_args(args: Vec<String>) -> Result<Cli, String> {
    let mut cli = Cli {
        base: PathBuf::from("."),
        digest_top: 100,
        ..Cli::default()
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
            "--focus" => cli.focus = Some(value(&mut index)?),
            "--base" => cli.base = PathBuf::from(value(&mut index)?),
            "--port" => {
                cli.port = value(&mut index)?
                    .parse()
                    .map_err(|_| "--port requires a number".to_string())?
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

/// 結果の置き場所を決める環境変数が無いときの理由（R-RESULT）。OS ごとに使う変数が違う。
fn results_unset_reason() -> &'static str {
    if cfg!(windows) {
        "LOCALAPPDATA is not set"
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
    fn save(&self, text: &str) -> Result<PathBuf, String> {
        let dir = self.dir.as_ref().ok_or_else(|| {
            format!(
                "cannot determine where to store results ({})",
                results_unset_reason()
            )
        })?;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        result::save(dir, &self.key, text, millis, std::process::id())
            .map_err(|error| format!("{}: {error}", dir.display()))
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

fn build_source(cli: &Cli) -> Result<Box<dyn ReviewSource>, String> {
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

#[tokio::main]
async fn main() {
    let cli = match parse_args(std::env::args().skip(1).collect()) {
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

    if let Some(out) = &cli.out {
        fail(&format!(
            "--out is removed; static files are no longer written ({out} will not be written)"
        ));
    }

    let modes = [
        cli.manifest.is_some(),
        cli.from.is_some(),
        cli.worktree,
        cli.staged,
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

    let source = match build_source(&cli).and_then(|source| with_focus(source, &cli)) {
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
    // --serve は互換のための受理のみ。既定で常にサーブする。
    let _ = cli.serve;

    let listener = match TcpListener::bind(("127.0.0.1", cli.port)).await {
        Ok(listener) => listener,
        Err(error) => fail(&format!("cannot listen on 127.0.0.1:{}: {error}", cli.port)),
    };
    let token = random_token();
    let url = match session_url(&listener, &token) {
        Ok(url) => url,
        Err(error) => fail(&error.to_string()),
    };
    eprintln!("kemi: {url}");
    let results = ResultStore {
        dir: results_dir(),
        key: result::workspace_key(&workspace_root(Path::new("."))),
    };
    match results.location() {
        Some(dir) => eprintln!("kemi: results are saved to: {dir}"),
        None => eprintln!(
            "kemi: cannot determine where to save results ({})",
            results_unset_reason()
        ),
    }
    if !cli.no_open {
        open_browser(&url);
    }

    let params = ServeParams {
        source,
        assets: Arc::new(WebAssets),
        token,
        results: Some(Arc::new(results)),
    };
    let outcome = tokio::select! {
        outcome = serve(listener, params) => outcome,
        _ = tokio::signal::ctrl_c() => {
            std::process::exit(130);
        }
    };

    match outcome {
        Ok(ServeOutcome::Submitted(document)) => {
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
        Err(error) => fail(&error.to_string()),
    }
}
