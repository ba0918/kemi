//! CLI 引数の読み取りと組み合わせの検証。出力と終了は main が担当する。

use std::net::Ipv4Addr;
use std::path::PathBuf;

pub(crate) struct Cli {
    pub explanation: Option<String>,
    pub manifest: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub group_by: Option<String>,
    pub worktree: bool,
    pub staged: bool,
    pub live: Option<String>,
    pub live_port: Option<u16>,
    pub focus: Option<String>,
    pub base: PathBuf,
    pub port: u16,
    pub bind: Ipv4Addr,
    pub no_open: bool,
    pub digest: bool,
    pub digest_top: usize,
    pub out: Option<String>,
    pub result: bool,
    pub any: bool,
    pub workspace: Option<PathBuf>,
    /// `Some(None)` は復元する id の省略。
    pub resume: Option<Option<String>>,
    pub version: bool,
    pub help: bool,
    flags: Vec<String>,
}

pub(crate) fn parse_args(args: Vec<String>) -> Result<Cli, String> {
    let mut cli = Cli {
        explanation: None,
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
            "--explanation" => {
                if cli.explanation.is_some() {
                    return Err("--explanation may be given only once".into());
                }
                cli.explanation = Some(value(&mut index)?);
            }
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
            // 互換のための受理のみ。既定で常にサーブする（R-INPUT-6）。
            "--serve" => {}
            "--out" => cli.out = Some(value(&mut index)?),
            "--result" => cli.result = true,
            "--any" => cli.any = true,
            "--workspace" => cli.workspace = Some(PathBuf::from(value(&mut index)?)),
            "--resume" => {
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

pub(crate) fn validate_result_flags(cli: &Cli) -> Result<(), String> {
    if !cli.result {
        if cli.any || cli.workspace.is_some() {
            return Err("--any and --workspace require --result".to_string());
        }
        return Ok(());
    }
    only_flags(
        cli,
        &["--result", "--any", "--workspace"],
        "--result accepts only --any and --workspace",
    )?;
    if cli.any && cli.workspace.is_some() {
        return Err("--any and --workspace cannot be used together".to_string());
    }
    Ok(())
}

pub(crate) enum InputModeError {
    Usage,
    Message(&'static str),
}

pub(crate) fn validate_input_mode(cli: &Cli) -> Result<(), InputModeError> {
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
        return Err(InputModeError::Message("specify exactly one input mode"));
    }
    if modes == 0 {
        return Err(InputModeError::Usage);
    }
    if cli.to.is_some() && cli.from.is_none() {
        return Err(InputModeError::Message("--to requires --from"));
    }
    if cli.group_by.is_some() && cli.from.is_none() {
        return Err(InputModeError::Message("--group-by requires --from"));
    }
    Ok(())
}

pub(crate) fn validate_resume_flags(cli: &Cli) -> Result<(), String> {
    if cli.resume.is_none() {
        return Ok(());
    }
    only_flags(
        cli,
        &[
            "--resume",
            "--port",
            "--bind",
            "--no-open",
            "--serve",
            "--live-port",
        ],
        "--resume accepts only --port, --bind, --no-open, --serve and --live-port",
    )
}

pub(crate) fn validate_live_flags(cli: &Cli) -> Result<(), String> {
    if cli.live.is_none() {
        if cli.live_port.is_some() && cli.resume.is_none() {
            return Err("--live-port requires --live or --resume".to_string());
        }
        return Ok(());
    }
    only_flags(
        cli,
        &[
            "--live",
            "--explanation",
            "--port",
            "--bind",
            "--no-open",
            "--live-port",
            "--focus",
            "--serve",
        ],
        "--live accepts only --port, --bind, --no-open, --live-port, --focus, --explanation and --serve",
    )
}

/// 起動時の説明は、ページを出さない `--digest` とは組まない（R-EXPLANATION-CLI）。
pub(crate) fn validate_explanation_flags(cli: &Cli) -> Result<(), String> {
    if cli.explanation.is_some() && cli.digest {
        return Err("--explanation cannot be used with --digest".to_string());
    }
    Ok(())
}

/// 廃止したフラグ。黙って無視せず理由を出す（R-INPUT-6）。
pub(crate) fn validate_removed_flags(cli: &Cli) -> Result<(), String> {
    if let Some(out) = &cli.out {
        return Err(format!(
            "--out is removed; static files are no longer written ({out} will not be written)"
        ));
    }
    Ok(())
}

/// `allowed` 以外のフラグと manifest を断る。モードごとに組めるものを限るときに使う。
fn only_flags(cli: &Cli, allowed: &[&str], message: &str) -> Result<(), String> {
    let others = cli
        .flags
        .iter()
        .any(|flag| !allowed.contains(&flag.as_str()));
    if others || cli.manifest.is_some() {
        return Err(message.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        parse_args(args.iter().map(|s| s.to_string()).collect()).expect("valid arguments")
    }

    #[test]
    fn omitted_options_keep_loopback_ephemeral_port_and_default_digest_size() {
        let cli = parse(&["--worktree"]);
        assert_eq!(cli.bind, Ipv4Addr::LOCALHOST);
        assert_eq!(cli.port, 0);
        assert_eq!(cli.digest_top, 100);
        assert_eq!(cli.base, PathBuf::from("."));
    }

    #[test]
    fn optional_resume_id_does_not_consume_the_next_flag() {
        let omitted = parse(&["--resume", "--no-open"]);
        assert_eq!(omitted.resume, Some(None));
        assert!(omitted.no_open);
        assert_eq!(
            parse(&["--resume", "review", "--port", "42"]).resume,
            Some(Some("review".into()))
        );
    }

    #[test]
    fn argument_errors_keep_their_specific_messages() {
        for (args, expected) in [
            (vec!["--port"], "--port requires a value"),
            (vec!["--port", "65536"], "--port requires a number"),
            (
                vec!["--bind", "localhost"],
                "--bind requires an IPv4 address literal: localhost",
            ),
            (
                vec!["--explanation", "a", "--explanation", "b"],
                "--explanation may be given only once",
            ),
            (vec!["a.json", "b.json"], "only one manifest can be given"),
            (vec!["--unknown"], "unknown flag: --unknown"),
        ] {
            assert_eq!(
                parse_args(args.iter().map(|s| s.to_string()).collect())
                    .err()
                    .as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn mode_validation_preserves_missing_mode_and_conflict_precedence() {
        assert!(matches!(
            validate_input_mode(&parse(&["--to", "HEAD"])),
            Err(InputModeError::Usage)
        ));
        assert!(matches!(
            validate_input_mode(&parse(&["--worktree", "--staged", "--to", "HEAD"])),
            Err(InputModeError::Message("specify exactly one input mode"))
        ));
        assert!(matches!(
            validate_input_mode(&parse(&["--worktree", "--to", "HEAD"])),
            Err(InputModeError::Message("--to requires --from"))
        ));
    }
}
