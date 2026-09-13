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
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher};
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
        let Ok(mut watcher) = notify::recommended_watcher(sender) else {
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
            let _ = watcher.watch(&target, RecursiveMode::NonRecursive);
        }

        let mut debounce = Debounce::new(DEBOUNCE);
        loop {
            let received = match debounce.remaining(Instant::now()) {
                None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(wait) => receiver.recv_timeout(wait),
            };
            match received {
                Ok(Ok(event)) if changes_a_watched_file(&event, &files) => {
                    debounce.note(Instant::now());
                }
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if debounce.take_due(Instant::now()) {
                let _ = events.send(Event::Update);
            }
        }
    });
}

fn changes_a_watched_file(event: &notify::Event, files: &HashSet<PathBuf>) -> bool {
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return false;
    }
    event
        .paths
        .iter()
        .any(|path| files.contains(&canonical(path.clone())))
}

/// 最後の変更から window のあいだ次の変更が無ければ、1 回だけ通知する。
///
/// notify-debouncer-full などの既製の debouncer は時計を内部に持ち、まとめ方を
/// 固定の時刻で試せない。実時間で試すと共有の CI ランナーで通知の数がぶれるので、
/// 時刻を引数で受け取る形で自前に持つ。
struct Debounce {
    window: Duration,
    due: Option<Instant>,
}

impl Debounce {
    fn new(window: Duration) -> Self {
        Debounce { window, due: None }
    }

    fn note(&mut self, now: Instant) {
        self.due = Some(now + self.window);
    }

    fn take_due(&mut self, now: Instant) -> bool {
        match self.due {
            Some(due) if now >= due => {
                self.due = None;
                true
            }
            _ => false,
        }
    }

    fn remaining(&self, now: Instant) -> Option<Duration> {
        self.due.map(|due| due.saturating_duration_since(now))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(500);

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn rapid_changes_within_the_window_notify_once_after_the_last_one() {
        let base = Instant::now();
        let mut debounce = Debounce::new(WINDOW);
        for millis in [0, 20, 40, 60, 80] {
            debounce.note(at(base, millis));
        }

        assert!(!debounce.take_due(at(base, 579)));
        assert!(debounce.take_due(at(base, 580)));
        assert!(!debounce.take_due(at(base, 2_000)));
    }

    #[test]
    fn a_change_after_a_quiet_window_notifies_again() {
        let base = Instant::now();
        let mut debounce = Debounce::new(WINDOW);
        debounce.note(at(base, 0));
        assert!(debounce.take_due(at(base, 500)));

        debounce.note(at(base, 700));
        assert!(debounce.take_due(at(base, 1_200)));
    }

    #[test]
    fn remaining_wait_counts_down_only_while_a_change_is_pending() {
        let base = Instant::now();
        let mut debounce = Debounce::new(WINDOW);
        assert_eq!(debounce.remaining(at(base, 0)), None);

        debounce.note(at(base, 0));
        assert_eq!(
            debounce.remaining(at(base, 100)),
            Some(Duration::from_millis(400))
        );
        assert_eq!(debounce.remaining(at(base, 600)), Some(Duration::ZERO));

        assert!(debounce.take_due(at(base, 600)));
        assert_eq!(debounce.remaining(at(base, 600)), None);
    }
}
