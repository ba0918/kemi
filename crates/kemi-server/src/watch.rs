//! 新側の供給元の監視（R-LIVE）。変更を debounce して SSE の更新通知にする。
//!
//! 監視は対象ファイルの親ディレクトリごとに行い、イベントは対象ファイルの
//! パスに完全一致する場合だけ採用する。読み取り（Access）は変更ではないので
//! 通知しない。ref の親ディレクトリを渡しても、その下の別ブランチの更新や
//! kemi 自身・他ツールの書き込みではバッジは出ない。
//!
//! packed refs のように対象ファイルがまだ無い場合は、存在しないパスを登録して
//! 親ディレクトリを監視し、パスの照合で loose ref の作成を検知する。

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::new_debouncer;
use tokio::sync::broadcast;

use crate::Event;

/// 短期間の複数書き込みを 1 回の通知にまとめる debounce 値（D6）。
const DEBOUNCE: Duration = Duration::from_millis(500);

pub(crate) fn start(paths: Vec<PathBuf>, events: broadcast::Sender<Event>) {
    if paths.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        let (sender, receiver) = std::sync::mpsc::channel();
        let Ok(mut debouncer) = new_debouncer(DEBOUNCE, None, sender) else {
            return;
        };

        let mut files: HashSet<PathBuf> = HashSet::new();
        let mut watch_targets: HashSet<PathBuf> = HashSet::new();
        for path in paths {
            let canonical = canonical(path);
            if let Some(parent) = canonical.parent() {
                watch_targets.insert(parent.to_path_buf());
            }
            files.insert(canonical);
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
                debounced
                    .event
                    .paths
                    .iter()
                    .any(|path| files.contains(&canonical(path.clone())))
            });
            if changed {
                let _ = events.send(Event::Update);
            }
        }
    });
}

/// ファイルがまだ無い場合でも、実在する親まで解決してファイル名を足す。
/// こうしないと、symlink を含むパス（macOS の /tmp など）で、登録時と
/// イベント時の canonical 表記が食い違って一致しない。
fn canonical(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(&path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => canonical(parent.to_path_buf()).join(name),
        _ => path,
    }
}
