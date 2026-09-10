//! kemi の CLI（R-INPUT-6, R-SUBMIT）。stdout は submit の JSON だけに使う。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use kemi_core::source::git::{GitMode, GitSource, GroupBy};
use kemi_core::source::manifest::ManifestSource;
use kemi_core::source::{FocusSource, ReviewSource};
use kemi_server::{serve, session_url, Asset, Assets, ServeOutcome, ServeParams};
use tokio::net::TcpListener;

const USAGE: &str = "\
kemi: 使い方:
  kemi <manifest.json | ->                 manifest を読む
  kemi --from <ref> [--to <ref>]          コミット範囲（--to の既定は HEAD）
  kemi --worktree                          未コミットの変更
  kemi --staged                            ステージ済みの変更

共通のフラグ:
  --focus <path>     focus レイヤの JSON（--base 相対）
  --base <dir>       manifest と --focus の相対パスの基準（既定 .）
  --group-by <mode>  commit | file（既定 commit）
  --port <n>         待ち受けポート（既定 0 = 空きを選ぶ）
  --no-open          ブラウザを自動で開かない
  --serve            互換のための受理のみ（既定で常にサーブする）
  --digest           ページを出さず digest を stdout に出す
  --digest-top <n>   digest のトップファイル数（既定 100）";

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
    version: bool,
    help: bool,
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
                .ok_or_else(|| format!("{arg} には値が必要です"))
        };
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
                    .map_err(|_| "--port には数値が必要です".to_string())?
            }
            "--no-open" => cli.no_open = true,
            "--digest" => cli.digest = true,
            "--digest-top" => {
                cli.digest_top = value(&mut index)?
                    .parse()
                    .map_err(|_| "--digest-top には数値が必要です".to_string())?
            }
            "--serve" => cli.serve = true,
            "--out" => cli.out = Some(value(&mut index)?),
            "-" => cli.manifest = Some("-".to_string()),
            other if other.starts_with('-') => return Err(format!("不明なフラグです: {other}")),
            path => {
                if cli.manifest.is_some() {
                    return Err("manifest は 1 つだけ指定できます".to_string());
                }
                cli.manifest = Some(path.to_string());
            }
        }
        index += 1;
    }
    Ok(cli)
}

fn group_by(cli: &Cli) -> Result<GroupBy, String> {
    match cli.group_by.as_deref() {
        None | Some("commit") => Ok(GroupBy::Commit),
        Some("file") => Ok(GroupBy::File),
        Some(other) => Err(format!("--group-by は commit か file です: {other}")),
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

fn random_token() -> String {
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let mut bytes = [0u8; 16];
        if file.read_exact(&mut bytes).is_ok() {
            return bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}{:x}", std::process::id())
}

fn open_browser(url: &str) {
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

    if let Some(out) = &cli.out {
        fail(&format!(
            "--out は廃止しました。静的書き出しは行いません（{out} は書き出されません）"
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
        fail("入力モードは 1 つだけ指定してください");
    }
    if modes == 0 {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    if cli.to.is_some() && cli.from.is_none() {
        fail("--to には --from が必要です");
    }
    if cli.group_by.is_some() && cli.from.is_none() {
        fail("--group-by には --from が必要です");
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
                    Err(error) => fail(&format!("digest の JSON を作れません: {error}")),
                }
            }
            Err(error) => fail(&error.to_string()),
        }
    }
    // --serve は互換のための受理のみ。既定で常にサーブする。
    let _ = cli.serve;

    let listener = match TcpListener::bind(("127.0.0.1", cli.port)).await {
        Ok(listener) => listener,
        Err(error) => fail(&format!(
            "127.0.0.1:{} を待ち受けできません: {error}",
            cli.port
        )),
    };
    let token = random_token();
    let url = match session_url(&listener, &token) {
        Ok(url) => url,
        Err(error) => fail(&error.to_string()),
    };
    eprintln!("kemi: {url}");
    if !cli.no_open {
        open_browser(&url);
    }

    let params = ServeParams {
        source,
        assets: Arc::new(WebAssets),
        token,
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
                Err(error) => fail(&format!("submit の JSON を作れません: {error}")),
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
