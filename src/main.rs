//! kemi の CLI（R-INPUT-6, R-SUBMIT）。stdout は submit の JSON だけに使う。
//!
//! ここは引数の振り分けと、確かめる順だけを持つ。

mod agent;
mod cli;
mod input;
mod local_time;
mod notice;
mod place;
mod report;
mod results;
mod resume;
mod review;
mod session;

use std::path::PathBuf;
use std::sync::Arc;

use cli::{
    InputMode, InputModeError, parse_args, validate_explanation_flags, validate_input_mode,
    validate_live_flags, validate_removed_flags, validate_result_flags, validate_resume_flags,
};
use input::{build_source, live_target, open_session, with_focus};
use kemi_core::result;
use kemi_core::source::ReviewSource;
use place::LaunchPlace;
use report::{fail, warn};
use results::{endpoints_dir, print_result, sessions_dir};
use resume::run_resume;
use review::{LiveRun, run_review};

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
  kemi explain <id> [--file <file> | --context]  replace an explanation or print its reference map

common flags:
  --focus <path>     focus layer JSON (relative to --base)
  --explanation <file>  initial explanation JSON (never reads stdin)
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

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // 最初の引数が wait / reply / explain ならサブコマンド。その名前の manifest は ./explain と書く
    // （R-AGENT-CLI）。
    if let Some(name @ ("wait" | "reply" | "explain")) = args.first().map(String::as_str) {
        let command = match agent::parse(name, &args[1..]) {
            Ok(command) => command,
            Err(message) => {
                warn(&message);
                eprintln!("{}", agent::USAGE);
                std::process::exit(report::ERROR);
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
        report::write_stdout(&format!("kemi {}\n", env!("CARGO_PKG_VERSION")));
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
    if let Err(message) = validate_explanation_flags(&cli) {
        fail(&message);
    }
    if let Err(message) = validate_removed_flags(&cli) {
        fail(&message);
    }

    if cli.resume.is_some() {
        run_resume(&cli).await;
    }

    let mode = match validate_input_mode(&cli) {
        Ok(mode) => mode,
        Err(InputModeError::Usage) => {
            eprintln!("{USAGE}");
            std::process::exit(report::ERROR);
        }
        Err(InputModeError::Message(message)) => fail(message),
        Err(InputModeError::InvalidValue(message)) => fail(&message),
    };

    let place = LaunchPlace::at(PathBuf::from("."));
    let input = match mode.resolve_live(|target| live_target(target, &place)) {
        Ok(input) => input,
        Err(message) => fail(&message),
    };
    let source =
        match build_source(&cli, &input, &place).and_then(|source| with_focus(source, &cli)) {
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
                        report::write_stdout(&format!("{json}\n"));
                        return;
                    }
                    Err(error) => fail(&format!("cannot build the digest JSON: {error}")),
                }
            }
            Err(error) => fail(&error.to_string()),
        }
    }

    let stored_session = open_session(&input, &place);
    let initial_explanation = cli.explanation.as_ref().map(|file| {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|error| fail(&format!("cannot read the explanation: {error}")));
        let document =
            serde_json::from_str::<kemi_core::domain::explanation::ExplanationDocument>(&text)
                .unwrap_or_else(|error| fail(&format!("invalid explanation: {error}")));
        kemi_core::domain::explanation::validate_document(&document)
            .unwrap_or_else(|error| fail(&error.to_string()));
        if stored_session.is_none() {
            fail("initial explanation requires persistent session storage");
        }
        document
    });
    let results_key = result::workspace_key(&place.workspace());
    let live = match input {
        InputMode::Live((page, root)) => Some(LiveRun {
            page,
            root,
            snapshots: Vec::new(),
            last_snapshot_number: 0,
            // 根は起動した場所の作業ツリーの根なので、git の中かは起動した場所と同じ。
            code_view: place.repo().is_ok(),
        }),
        InputMode::Manifest(_)
        | InputMode::Range { .. }
        | InputMode::Worktree
        | InputMode::Staged => None,
    };
    run_review(
        &cli,
        source,
        stored_session,
        results_key,
        live,
        initial_explanation,
    )
    .await
}
