//! 新側の供給元の監視（R-LIVE）。変更を debounce して SSE の更新通知にする。
//!
//! worktree では作業ツリー全体を、ディレクトリごとに見張る（`.git` と git が無視するものを
//! 除く）。ほかの供給元と、作業ツリー全体を見張れないときは、
//! 監視は対象ファイルの親ディレクトリごとに行い、イベントは対象ファイルの
//! パスに完全一致する場合だけ採用する。読み取り（Access）は変更ではないので
//! 通知しない。ref の親ディレクトリを渡しても、その下の別ブランチの更新や
//! kemi 自身・他ツールの書き込みではバッジは出ない。
//!
//! packed refs のように対象ファイルがまだ無い場合は、存在しないパスを登録して
//! 親ディレクトリを監視し、パスの照合で loose ref の作成を検知する。

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use kemi_core::source::work_tree::{WATCH_DIRECTORY_LIMIT, WorkTree};
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::broadcast;

use crate::{Event, Notice, NoticeSink, WatchFallback};

/// 短期間の複数書き込みを 1 回の通知にまとめる debounce 値（D6）。
const DEBOUNCE: Duration = Duration::from_millis(500);

/// 新側の供給元を見張り、変われば `Event::Update` を送る。`tree` があれば作業ツリー全体を
/// 見張り、見張れなければ `paths`（起動時の差分のファイル）だけに落として `notices` に 1 回
/// 知らせる。登録は別スレッドで行い、起動の応答を待たせない。
pub(crate) fn start(
    paths: Vec<PathBuf>,
    tree: Option<WorkTree>,
    events: broadcast::Sender<Event>,
    notices: Arc<dyn NoticeSink>,
) {
    if paths.is_empty() && tree.is_none() {
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
        // 差分のファイルの見張りを先に足す。作業ツリー全体の登録に時間がかかっても、
        // その間の差分のファイルの変更は取りこぼさない。
        for target in &watch_targets {
            let _ = watcher.watch(target, RecursiveMode::NonRecursive);
        }
        let mut whole = tree.and_then(|tree| {
            TreeWatch::start(&mut watcher, tree, watch_targets)
                .map_err(|fallback| notices.notify(Notice::WorkTreeNotWatched(fallback)))
                .ok()
        });

        let mut debounce = Debounce::new(DEBOUNCE);
        loop {
            let received = match debounce.remaining(Instant::now()) {
                None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(wait) => receiver.recv_timeout(wait),
            };
            match received {
                Ok(Ok(event)) => {
                    let mut changed = changes_a_watched_file(&event, &files);
                    if let Some(tree) = whole.as_mut() {
                        match tree.changes(&mut watcher, &event) {
                            Ok(tree_changed) => changed |= tree_changed,
                            Err(fallback) => {
                                tree.stop(&mut watcher);
                                whole = None;
                                notices.notify(Notice::WorkTreeNotWatched(fallback));
                            }
                        }
                    }
                    if changed {
                        debounce.note(Instant::now());
                    }
                }
                Ok(Err(_)) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if debounce.take_due(Instant::now()) {
                let _ = events.send(Event::Update);
            }
        }
    });
}

/// 無視の判定を覚えておく数の上限。超えたら忘れて聞き直す。
const VERDICT_LIMIT: usize = 100_000;

/// 作業ツリー全体の見張り（R-LIVE の worktree）。`.git` と git が無視するものは見張らず、
/// そこへの変化は変更として数えない。
struct TreeWatch {
    tree: WorkTree,
    /// 作業ツリーの根の実体の場所。差分のファイルの見張りはこの表記でイベントを返す。
    canonical_root: PathBuf,
    /// 見張っている作業ツリーのディレクトリ（数を上限と比べる）。
    directories: HashSet<PathBuf>,
    /// このうち、ここで足した見張り。落とすときに外すのはこれだけ。
    added: HashSet<PathBuf>,
    /// 差分のファイルのために見張っているディレクトリ（実体の場所）。落としても残す。
    kept: HashSet<PathBuf>,
    /// パスごとの「git が無視するか」。消えたディレクトリは git に聞いても無視と
    /// 分からないので、見えていたときの判定を使う。
    verdicts: HashMap<PathBuf, bool>,
}

impl TreeWatch {
    fn start(
        watcher: &mut impl Watcher,
        tree: WorkTree,
        kept: HashSet<PathBuf>,
    ) -> Result<Self, WatchFallback> {
        let found = tree
            .directories(WATCH_DIRECTORY_LIMIT)
            .map_err(|error| WatchFallback::Refused(error.to_string()))?;
        if found.over_limit {
            return Err(WatchFallback::TooManyDirectories {
                limit: WATCH_DIRECTORY_LIMIT,
            });
        }
        let mut this = TreeWatch {
            canonical_root: canonical(tree.root().to_path_buf()),
            tree,
            directories: HashSet::new(),
            added: HashSet::new(),
            kept,
            verdicts: found.ignored.into_iter().map(|path| (path, true)).collect(),
        };
        if let Err(fallback) = this.add(watcher, found.directories) {
            this.stop(watcher);
            return Err(fallback);
        }
        Ok(this)
    }

    /// ディレクトリの見張りを足す。上限を超えるか OS が断ったら Err で、呼び出し側が `stop` する。
    fn add(
        &mut self,
        watcher: &mut impl Watcher,
        directories: Vec<PathBuf>,
    ) -> Result<(), WatchFallback> {
        let mut paths = watcher.paths_mut();
        for directory in directories {
            if !self.directories.insert(directory.clone()) {
                continue;
            }
            if self.directories.len() > WATCH_DIRECTORY_LIMIT {
                return Err(WatchFallback::TooManyDirectories {
                    limit: WATCH_DIRECTORY_LIMIT,
                });
            }
            // 差分のファイルのために見張っている場所は、二重に足さない（外すと両方消える）。
            if self.kept.contains(&canonical(directory.clone())) {
                continue;
            }
            paths
                .add(&directory, RecursiveMode::NonRecursive)
                .map_err(|error| WatchFallback::Refused(error.to_string()))?;
            self.added.insert(directory);
        }
        paths
            .commit()
            .map_err(|error| WatchFallback::Refused(error.to_string()))
    }

    /// ここで足した見張りをすべて外し、差分のファイルの見張りだけを残す。
    fn stop(&mut self, watcher: &mut impl Watcher) {
        let mut paths = watcher.paths_mut();
        for directory in self.added.drain() {
            let _ = paths.remove(&directory);
        }
        let _ = paths.commit();
        self.directories.clear();
    }

    /// 作業ツリーを変えるイベントか。作られたディレクトリ（無視されないもの）は見張りに足す。
    fn changes(
        &mut self,
        watcher: &mut impl Watcher,
        event: &notify::Event,
    ) -> Result<bool, WatchFallback> {
        if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
            return Ok(false);
        }
        let paths: Vec<PathBuf> = event
            .paths
            .iter()
            .filter_map(|path| self.inside(path))
            .collect();
        if paths
            .iter()
            .any(|path| path.file_name() == Some(".gitignore".as_ref()))
            || self.verdicts.len() > VERDICT_LIMIT
        {
            self.verdicts.clear();
        }
        let unknown: Vec<PathBuf> = paths
            .iter()
            .filter(|path| !self.verdicts.contains_key(*path))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            // git に聞けないときは、無視しないものとして数える（変化を見落とさない側に倒す）。
            let ignored = self.tree.ignored(&unknown).unwrap_or_default();
            for path in unknown {
                let verdict = ignored.contains(&path);
                self.verdicts.insert(path, verdict);
            }
        }

        let mut changed = false;
        for path in paths {
            if self.verdicts.get(&path) == Some(&true) {
                continue;
            }
            changed = true;
            if path.is_dir() {
                if !self.directories.contains(&path) {
                    self.watch_new_directory(watcher, &path)?;
                }
            } else if self.directories.remove(&path) {
                // 消えたディレクトリの見張りは OS が外している。
                self.added.remove(&path);
            }
        }
        Ok(changed)
    }

    fn watch_new_directory(
        &mut self,
        watcher: &mut impl Watcher,
        directory: &Path,
    ) -> Result<(), WatchFallback> {
        let room = WATCH_DIRECTORY_LIMIT.saturating_sub(self.directories.len());
        let found = self
            .tree
            .directories_under(directory, room)
            .map_err(|error| WatchFallback::Refused(error.to_string()))?;
        for ignored in found.ignored {
            self.verdicts.insert(ignored, true);
        }
        if found.over_limit {
            return Err(WatchFallback::TooManyDirectories {
                limit: WATCH_DIRECTORY_LIMIT,
            });
        }
        self.add(watcher, found.directories)
    }

    /// 作業ツリーの中で `.git` の外のパスを、作業ツリーの根からの表記にして返す。
    /// 根そのもの・作業ツリーの外・`.git` の中は None。
    fn inside(&self, path: &Path) -> Option<PathBuf> {
        let relative = path
            .strip_prefix(self.tree.root())
            .or_else(|_| path.strip_prefix(&self.canonical_root))
            .ok()?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| component == Component::Normal(".git".as_ref()))
        {
            return None;
        }
        Some(self.tree.root().join(relative))
    }
}

/// 配れる範囲を見張る（R-PAGE-MODE の `--live <ファイル>`）。範囲の中のファイルが保存・作成
/// されたら知らせる。範囲を再帰で見張れないとき（Linux の inotify の見張りの数の上限など）は、
/// 配ったファイルの親ディレクトリだけを見張り、配ったファイルの変更だけを知らせる。
pub(crate) struct ServedWatch {
    sender: std::sync::mpsc::Sender<RangeMessage>,
}

enum RangeMessage {
    Served(PathBuf),
    Changed(notify::Result<notify::Event>),
}

impl ServedWatch {
    /// 配ったファイル（実体の場所）を知らせる。範囲を再帰で見張れないときに見張りへ足す。
    pub(crate) fn served(&self, file: PathBuf) {
        let _ = self.sender.send(RangeMessage::Served(file));
    }
}

/// 範囲の中のファイルが変わったら `reload` に知らせる。配ったファイルのディレクトリを
/// 見張れなかったときは `notices` に知らせる。
pub(crate) fn start_served(
    root: PathBuf,
    reload: broadcast::Sender<()>,
    notices: Arc<dyn NoticeSink>,
) -> ServedWatch {
    let (sender, receiver) = std::sync::mpsc::channel();
    let events = sender.clone();
    std::thread::spawn(move || {
        let Ok(mut watcher) = notify::recommended_watcher(move |event| {
            let _ = events.send(RangeMessage::Changed(event));
        }) else {
            return;
        };
        let root = canonical(root);
        let whole = watcher.watch(&root, RecursiveMode::Recursive).is_ok();
        if !whole {
            // 再帰の見張りは途中で失敗しても、それまでに足した見張りを残す。残すと上限を
            // 使い切ったまま、配ったファイルの見張りもコードの見方の監視も足せない。
            let _ = watcher.unwatch(&root);
        }
        let mut files: HashSet<PathBuf> = HashSet::new();
        let mut directories: HashSet<PathBuf> = HashSet::new();
        let mut debounce = Debounce::new(DEBOUNCE);
        loop {
            let received = match debounce.remaining(Instant::now()) {
                None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(wait) => receiver.recv_timeout(wait),
            };
            match received {
                Ok(RangeMessage::Served(_)) if whole => {}
                Ok(RangeMessage::Served(file)) => {
                    let file = canonical(file);
                    if let Some(parent) = file.parent()
                        && directories.insert(parent.to_path_buf())
                        && let Err(error) = watcher.watch(parent, RecursiveMode::NonRecursive)
                    {
                        notices.notify(Notice::ServedNotWatched {
                            directory: parent.to_path_buf(),
                            reason: error.to_string(),
                        });
                    }
                    files.insert(file);
                }
                Ok(RangeMessage::Changed(Ok(event)))
                    if (whole && changes_the_range(&event, &root))
                        || changes_a_watched_file(&event, &files) =>
                {
                    debounce.note(Instant::now());
                }
                Ok(RangeMessage::Changed(_)) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if debounce.take_due(Instant::now()) {
                let _ = reload.send(());
            }
        }
    });
    ServedWatch { sender }
}

/// 範囲（`root` の下、`.git` の中を除く）のファイルを変えるイベントか。
fn changes_the_range(event: &notify::Event, root: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return false;
    }
    event.paths.iter().any(|path| {
        path.strip_prefix(root).is_ok_and(|inside| {
            !inside
                .components()
                .any(|component| component == Component::Normal(".git".as_ref()))
        })
    })
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

    fn event(kind: EventKind, path: &str) -> notify::Event {
        notify::Event::new(kind).add_path(PathBuf::from(path))
    }

    #[test]
    fn a_file_created_or_saved_anywhere_in_the_range_outside_dot_git_reloads() {
        use notify::event::{AccessKind, CreateKind, ModifyKind};
        let root = Path::new("/range");

        assert!(changes_the_range(
            &event(EventKind::Create(CreateKind::File), "/range/img/new.png"),
            root
        ));
        assert!(changes_the_range(
            &event(EventKind::Modify(ModifyKind::Any), "/range/other/page.css"),
            root
        ));
        assert!(!changes_the_range(
            &event(EventKind::Modify(ModifyKind::Any), "/range/.git/index"),
            root
        ));
        assert!(!changes_the_range(
            &event(EventKind::Modify(ModifyKind::Any), "/elsewhere/a.css"),
            root
        ));
        assert!(!changes_the_range(
            &event(EventKind::Access(AccessKind::Any), "/range/page.html"),
            root
        ));
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
