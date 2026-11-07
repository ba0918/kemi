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

/// 起動中に見えたレビューを確かめ直すまでの間。ほかのプロセス（掃除、`--resume` の一覧、別の
/// ps）がロックを確かめる一瞬に当たると、保留中のセッションや、強制終了で記録の残った
/// レビューが起動中に見えるため。
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
    // ps を打った場所の識別は git を走らせて決めるので、範囲で絞るときだけ求める。
    let here = (!args.any && !running.is_empty())
        .then(|| result::workspace_key(&LaunchPlace::at(PathBuf::from(".")).workspace()));
    let listing = assemble(running, here.as_deref());
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
/// 扱う（書く側も書けずに警告して続けるので）。ロックが取られていた id は、記録の有無に関わらず
/// 少し置いて確かめ直し、まだ取られているものだけを、そのときの記録とともに返す。強制終了で
/// 残った記録は、止まったレビューにもあるため。
fn read_running(
    store: &SessionStore,
    records: Option<&Path>,
) -> Result<Vec<(String, RecordRead)>, String> {
    read_running_with(store, records, || std::thread::sleep(RECHECK_DELAY))
}

/// [`read_running`] の本体。起動中に見えた id があれば、確かめ直す前に `wait` を 1 回だけ呼ぶ。
fn read_running_with(
    store: &SessionStore,
    records: Option<&Path>,
    wait: impl FnOnce(),
) -> Result<Vec<(String, RecordRead)>, String> {
    let read = |id: &str| match records.map(|dir| read_running_record(dir, id)) {
        Some(Ok(Some(record))) => RecordRead::Found(record),
        None | Some(Ok(None)) => RecordRead::Missing,
        Some(Err(error)) => RecordRead::Unreadable(error.to_string()),
    };
    let ids = store
        .running_ids()
        .map_err(|error| format!("cannot list the running reviews: {error}"))?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    wait();
    Ok(ids
        .into_iter()
        .filter(|id| store.is_running(id))
        .map(|id| {
            let record = read(&id);
            (id, record)
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

    /// テストのための空のディレクトリ。本体の crate には一時ディレクトリの補助が無い。
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "kemi-ps-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&path);
            ScratchDir(path)
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 別のスレッドが持つセッションのロック。ほかの kemi が起動中かを確かめる一瞬の代わり。
    /// `read_running` はセッションのファイルを見ないので、開いて持つだけで保留中のセッションの役になる。
    struct HeldLock {
        release: std::sync::mpsc::Sender<()>,
        holder: std::thread::JoinHandle<()>,
    }

    impl HeldLock {
        fn hold(store: &SessionStore, id: &str) -> Self {
            let (held, holding) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel::<()>();
            let store = store.clone();
            let info = kemi_core::session::SessionInfo {
                id: id.to_string(),
                created: 1,
                updated: 1,
                workspace: PathBuf::from("/tmp/workspace"),
                workspace_key: "00000000000000aa".to_string(),
                mode: kemi_core::session::SessionMode::Worktree,
                title: String::new(),
                total_files: 1,
            };
            let holder = std::thread::spawn(move || {
                let open = store.create(info).unwrap();
                held.send(()).unwrap();
                // テストが合図を送る前に落ちても、送り手が消えて受け取りが終わるので残らない。
                let _ = released.recv();
                drop(open);
            });
            holding.recv().unwrap();
            HeldLock { release, holder }
        }

        /// 手放す合図を送り、手放し終えるまで待つ。
        fn release(self) {
            let _ = self.release.send(());
            self.holder.join().unwrap();
        }
    }

    #[test]
    fn an_id_whose_lock_is_released_before_the_recheck_is_not_listed() {
        let scratch = ScratchDir::new();
        let store = SessionStore::new(scratch.0.join("sessions"));
        let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
        let held = HeldLock::hold(&store, id);
        let mut waited = 0;

        let running = read_running_with(&store, None, || {
            waited += 1;
            held.release();
        })
        .unwrap();

        // 最初の確認で起動中に見え、確かめ直しの前の待ちに入った。
        assert_eq!(waited, 1);
        assert!(running.is_empty(), "{running:?}");
    }
}
