//! 新側の供給元の監視（R-LIVE）。変更を debounce して SSE の更新通知にする。
//!
//! 監視は対象ファイルの親ディレクトリごとに行い、イベントは対象ファイル
//! （または対象ディレクトリの直下）に限って採用する。読み取り（Access）は
//! 変更ではないので通知しない。kemi 自身や他のツールが同じディレクトリに
//! 書いてもバッジは出ない。

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::new_debouncer;
use tokio::sync::broadcast;

/// 短期間の複数書き込みを 1 回の通知にまとめる debounce 値（D6）。
const DEBOUNCE: Duration = Duration::from_millis(500);

pub(crate) fn start(paths: Vec<PathBuf>, events: broadcast::Sender<()>) {
    if paths.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        let (sender, receiver) = std::sync::mpsc::channel();
        let Ok(mut debouncer) = new_debouncer(DEBOUNCE, None, sender) else {
            return;
        };

        let mut files: HashSet<PathBuf> = HashSet::new();
        let mut dirs: HashSet<PathBuf> = HashSet::new();
        let mut watch_targets: HashSet<PathBuf> = HashSet::new();
        for path in paths {
            let canonical = canonical(path.clone());
            if path.is_dir() {
                dirs.insert(canonical.clone());
                watch_targets.insert(canonical);
            } else {
                files.insert(canonical.clone());
                if let Some(parent) = canonical.parent() {
                    watch_targets.insert(parent.to_path_buf());
                }
            }
        }
        for target in watch_targets {
            let _ = debouncer.watch(&target, RecursiveMode::NonRecursive);
        }

        for result in receiver {
            let Ok(batch) = result else {
                continue;
            };
            let changed = batch.iter().any(|debounced| {
                if matches!(
                    debounced.event.kind,
                    EventKind::Access(_) | EventKind::Other
                ) {
                    return false;
                }
                debounced.event.paths.iter().any(|path| {
                    let canonical = canonical(path.clone());
                    files.contains(&canonical)
                        || canonical
                            .parent()
                            .is_some_and(|parent| dirs.contains(parent))
                })
            });
            if changed {
                let _ = events.send(());
            }
        }
    });
}

fn canonical(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}
