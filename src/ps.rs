//! `kemi ps`: 起動中のレビューの一覧（ps.md の R-PS-RUNNING、R-PS-OUTPUT）。
//!
//! 起動中かはセッションのロックだけで決め、サーバーには問い合わせない。値は各レビューが書いた
//! 起動中の記録から読む。セッションの置き場所には何も書かず、何も消さない。

mod listing;
mod output;

use std::path::{Path, PathBuf};
use std::time::Duration;

use kemi_core::result;
use kemi_core::session::{SessionStore, read_running_record};

use listing::{RecordRead, assemble};

use crate::place::LaunchPlace;
use crate::report::{self, warn};
use crate::results::results_unset_reason;

/// 記録の無いレビューを確かめ直すまでの間。ほかのプロセス（掃除、`--resume` の一覧、別の
/// ps）がロックを確かめる一瞬に当たると、保留中のセッションが起動中に見えるため。
const RECHECK_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Text,
    Json,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
    any: bool,
    format: Format,
}

/// `ps` の後ろの引数を読む。`--any` と `--format text|json` のほかは受け取らない。
pub(crate) fn parse(rest: &[String]) -> Result<Args, String> {
    let mut any = false;
    let mut format = None;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--any" if !any => any = true,
            "--any" => return Err("--any may be given only once".into()),
            "--format" if format.is_some() => {
                return Err("--format may be given only once".into());
            }
            "--format" => {
                index += 1;
                format = Some(match rest.get(index).map(String::as_str) {
                    Some("text") => Format::Text,
                    Some("json") => Format::Json,
                    Some(other) => {
                        return Err(format!("--format takes text or json, not {other}"));
                    }
                    None => return Err("--format requires text or json".into()),
                });
            }
            other if other.starts_with('-') => {
                return Err(format!("kemi ps does not take {other}"));
            }
            other => return Err(format!("kemi ps takes no arguments: {other}")),
        }
        index += 1;
    }
    Ok(Args {
        any,
        format: format.unwrap_or(Format::Text),
    })
}

/// 一覧を stdout に出し、終了コードを返す。セッションの置き場所が決められないときだけ 2。
pub(crate) fn run(args: &Args, sessions: Option<PathBuf>, records: Option<PathBuf>) -> i32 {
    let Some(sessions) = sessions else {
        warn(&format!(
            "cannot determine where sessions are stored ({})",
            results_unset_reason()
        ));
        return report::ERROR;
    };
    let store = SessionStore::new(sessions);
    let running = match read_running(&store, records.as_deref()) {
        Ok(running) => running,
        Err(message) => {
            warn(&message);
            return report::ERROR;
        }
    };
    let here = result::workspace_key(&LaunchPlace::at(PathBuf::from(".")).workspace());
    let listing = assemble(running, &here, args.any);
    for warning in &listing.warnings {
        warn(warning);
    }
    let text = match args.format {
        Format::Json => format!("{}\n", output::json(&listing.rows)),
        Format::Text => output::text(&listing, args.any, std::env::home_dir().as_deref()),
    };
    report::write_stdout(&text);
    report::SUCCESS
}

/// 起動中のレビューの id と記録。記録の置き場所が決められないときは、どれも記録が無いものとして
/// 扱う（書く側も書けずに警告して続けるので）。記録の無いものは少し置いて確かめ直し、そのときの
/// 記録を読む。保留中のセッションは記録を消してからロックを外すので、記録のあるものは確かめ
/// 直さない。
fn read_running(
    store: &SessionStore,
    records: Option<&Path>,
) -> Result<Vec<(String, RecordRead)>, String> {
    let read = |id: &str| match records.map(|dir| read_running_record(dir, id)) {
        Some(Ok(Some(record))) => RecordRead::Found(record),
        None | Some(Ok(None)) => RecordRead::Missing,
        Some(Err(error)) => RecordRead::Unreadable(error.to_string()),
    };
    let ids = store
        .running_ids()
        .map_err(|error| format!("cannot list the running reviews: {error}"))?;
    let first: Vec<(String, RecordRead)> = ids
        .into_iter()
        .map(|id| {
            let record = read(&id);
            (id, record)
        })
        .collect();
    if first
        .iter()
        .all(|(_, record)| matches!(record, RecordRead::Found(_)))
    {
        return Ok(first);
    }
    std::thread::sleep(RECHECK_DELAY);
    Ok(first
        .into_iter()
        .filter_map(|(id, record)| match record {
            RecordRead::Found(_) => Some((id, record)),
            RecordRead::Missing | RecordRead::Unreadable(_) => store.is_running(&id).then(|| {
                let record = read(&id);
                (id, record)
            }),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Args, String> {
        parse(&args.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn text_is_the_default_and_only_any_and_format_are_taken() {
        assert_eq!(
            parse_args(&[]),
            Ok(Args {
                any: false,
                format: Format::Text
            })
        );
        assert_eq!(
            parse_args(&["--format", "json", "--any"]),
            Ok(Args {
                any: true,
                format: Format::Json
            })
        );
        for rejected in [
            &["--all"][..],
            &["extra"],
            &["--format", "xml"],
            &["--format"],
            &["--any", "--any"],
            &["--format", "text", "--format", "json"],
        ] {
            assert!(parse_args(rejected).is_err(), "{rejected:?}");
        }
    }
}
