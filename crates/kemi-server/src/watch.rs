//! 新側の供給元の監視（R-LIVE）。変更を debounce して SSE の更新通知にする。
//!
//! worktree では作業ツリー全体を見張る（`.git` と git が無視するものを除く）。Linux は
//! ディレクトリごとに、macOS と Windows は根の再帰の見張り 1 つで見張る。ほかの供給元と、
//! 作業ツリー全体を見張れないときは、
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
        let mut diff = DiffWatch::default();
        for path in paths {
            let canonical = canonical(path);
            if let Some(parent) = canonical.parent() {
                diff.directories.insert(parent.to_path_buf());
            }
            files.insert(canonical);
        }
        let strategy = Strategy::of_this_os();
        // ディレクトリごとに見張るときは、差分のファイルの見張りを先に足す。作業ツリー全体の
        // 登録に時間がかかっても、その間の差分のファイルの変更は取りこぼさない。根の再帰の
        // 見張りはその下を全部含むので、見張れなかったときだけ足す（Windows ではディレクトリの
        // 見張りがそのディレクトリの改名や削除を妨げる）。
        if strategy == Strategy::PerDirectory || tree.is_none() {
            diff.ensure(&mut watcher);
        }
        let mut whole = tree.and_then(|tree| {
            TreeWatch::start(&mut watcher, tree, strategy, diff.directories.clone())
                .map_err(|fallback| notices.notify(Notice::WorkTreeNotWatched(fallback)))
                .ok()
        });
        if whole.is_none() {
            diff.ensure(&mut watcher);
        }

        let mut debounce = Debounce::new(DEBOUNCE);
        // 無視の規則が変わったら辿り直す。保存やブランチの切り替えは規則のファイルを
        // 続けて書くので、書き込みが静まってから 1 回だけ辿る。
        let mut rules = Debounce::new(DEBOUNCE);
        loop {
            let now = Instant::now();
            let wait = [debounce.remaining(now), rules.remaining(now)]
                .into_iter()
                .flatten()
                .min();
            let received = match wait {
                None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(wait) => receiver.recv_timeout(wait),
            };
            match received {
                Ok(Ok(event)) => {
                    // 溜まっているイベントをまとめて判定し、git を呼ぶのを 1 回にする。
                    // ブランチの切り替えやコード生成は数千のイベントを一度に出す。
                    let mut batch = vec![event];
                    while let Ok(next) = receiver.try_recv() {
                        batch.extend(next.ok());
                    }
                    let mut changed = batch
                        .iter()
                        .any(|event| changes_a_watched_file(event, &files));
                    if let Some(tree) = whole.as_mut() {
                        match tree.changes(&mut watcher, &batch, &files) {
                            Ok(tree_changed) => changed |= tree_changed,
                            Err(fallback) => fall_back(
                                &mut whole,
                                &mut diff,
                                &mut watcher,
                                notices.as_ref(),
                                fallback,
                            ),
                        }
                    }
                    if whole.as_mut().is_some_and(TreeWatch::take_rules_changed) {
                        rules.note(Instant::now());
                    }
                    if changed {
                        debounce.note(Instant::now());
                    }
                }
                Ok(Err(_)) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if rules.take_due(Instant::now())
                && let Some(tree) = whole.as_mut()
                && let Err(fallback) = tree.rewalk(&mut watcher)
            {
                fall_back(
                    &mut whole,
                    &mut diff,
                    &mut watcher,
                    notices.as_ref(),
                    fallback,
                );
            }
            if debounce.take_due(Instant::now()) {
                let _ = events.send(Event::Update);
            }
        }
    });
}

/// 作業ツリー全体の見張りをやめ、差分のファイルの見張りだけにして知らせる。
fn fall_back(
    whole: &mut Option<TreeWatch>,
    diff: &mut DiffWatch,
    watcher: &mut impl Watcher,
    notices: &dyn NoticeSink,
    fallback: WatchFallback,
) {
    if let Some(mut tree) = whole.take() {
        tree.stop(watcher);
        diff.watch_again(watcher);
        notices.notify(Notice::WorkTreeNotWatched(fallback));
    }
}

/// 起動時の差分のファイルの親ディレクトリの見張り。
#[derive(Default)]
struct DiffWatch {
    /// 見張るディレクトリ（実体の場所）。
    directories: HashSet<PathBuf>,
    watched: bool,
}

impl DiffWatch {
    /// まだ見張っていなければ見張る。
    fn ensure(&mut self, watcher: &mut impl Watcher) {
        if std::mem::replace(&mut self.watched, true) {
            return;
        }
        for directory in &self.directories {
            let _ = watcher.watch(directory, RecursiveMode::NonRecursive);
        }
    }

    /// 見張っていても、もう一度見張る。消えて作り直されたディレクトリは作業ツリー全体の
    /// 見張りが足し直していて、それを外すとこちらの見張りも無くなっている。
    fn watch_again(&mut self, watcher: &mut impl Watcher) {
        self.watched = false;
        self.ensure(watcher);
    }
}

/// 作業ツリー全体の見張り方（R-LIVE の worktree）。OS で分ける。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strategy {
    /// ディレクトリごとに見張り、無視したディレクトリの下には潜らない（Linux）。
    PerDirectory,
    /// 作業ツリーの根に OS の再帰の見張りを 1 つだけ付け、無視するものへの変化は届いた後で
    /// 捨てる（macOS と Windows）。
    RecursiveRoot,
}

impl Strategy {
    fn of_this_os() -> Self {
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            Strategy::RecursiveRoot
        } else {
            Strategy::PerDirectory
        }
    }
}

/// 無視の判定を覚えておく数の上限。超えたら忘れて聞き直す。
const VERDICT_LIMIT: usize = 100_000;

/// 作業ツリー全体の見張り（R-LIVE の worktree）。`.git` と git が無視するものは見張らず、
/// そこへの変化は変更として数えない。
struct TreeWatch {
    tree: WorkTree,
    strategy: Strategy,
    /// 作業ツリーの根の実体の場所。差分のファイルの見張りはこの表記でイベントを返す。
    canonical_root: PathBuf,
    /// 見張っている作業ツリーのディレクトリ（数を上限と比べる）。根の再帰の見張りでは空。
    directories: HashSet<PathBuf>,
    /// このうち、ここで足した見張り。落とすときに外すのはこれだけ。
    added: HashSet<PathBuf>,
    /// 差分のファイルのために見張っているディレクトリ（実体の場所）。落としても残す。
    kept: HashSet<PathBuf>,
    /// パスごとの「git が無視するか」。消えたディレクトリは git に聞いても無視と
    /// 分からないので、見えていたときの判定を使う。
    verdicts: HashMap<PathBuf, bool>,
    /// `info/exclude`（`.git` の中の無視の規則）の実体の場所。変化が届く場合だけ持つ。
    exclude: Option<PathBuf>,
    /// `info/exclude` のためにここで足した、その親のディレクトリの見張り。
    exclude_watch: Option<PathBuf>,
    /// 無視の規則が変わり、辿り直していない。
    rules_changed: bool,
}

impl TreeWatch {
    fn start(
        watcher: &mut impl Watcher,
        tree: WorkTree,
        strategy: Strategy,
        kept: HashSet<PathBuf>,
    ) -> Result<Self, WatchFallback> {
        if strategy == Strategy::RecursiveRoot {
            return Self::start_recursive(watcher, tree, kept);
        }
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
            strategy: Strategy::PerDirectory,
            directories: HashSet::new(),
            added: HashSet::new(),
            kept,
            verdicts: found.ignored.into_iter().map(|path| (path, true)).collect(),
            exclude: None,
            exclude_watch: None,
            rules_changed: false,
        };
        if let Err(fallback) = this.add(watcher, found.directories) {
            this.stop(watcher);
            return Err(fallback);
        }
        this.watch_exclude(watcher);
        Ok(this)
    }

    /// 根に再帰の見張りを 1 つだけ付ける。辿らないので上限も無い。
    fn start_recursive(
        watcher: &mut impl Watcher,
        tree: WorkTree,
        kept: HashSet<PathBuf>,
    ) -> Result<Self, WatchFallback> {
        watcher
            .watch(tree.root(), RecursiveMode::Recursive)
            .map_err(|error| WatchFallback::Refused(error.to_string()))?;
        let mut this = TreeWatch {
            canonical_root: canonical(tree.root().to_path_buf()),
            tree,
            strategy: Strategy::RecursiveRoot,
            directories: HashSet::new(),
            added: HashSet::new(),
            kept,
            verdicts: HashMap::new(),
            exclude: None,
            exclude_watch: None,
            rules_changed: false,
        };
        this.watch_exclude(watcher);
        Ok(this)
    }

    /// `.git` の中は見張らないが、`info/exclude` が変わると無視するものが変わるので、その変化を
    /// 受け取る。根の再帰の見張りの下にあれば足さない。見張れなくても作業ツリーの見張りは
    /// 続ける（無いリポジトリもある）。
    fn watch_exclude(&mut self, watcher: &mut impl Watcher) {
        let Ok(exclude) = self.tree.exclude_file() else {
            return;
        };
        let exclude = canonical(exclude);
        let covered =
            self.strategy == Strategy::RecursiveRoot && exclude.starts_with(&self.canonical_root);
        if covered {
            self.exclude = Some(exclude);
        } else if let Some(parent) = exclude.parent()
            && watcher.watch(parent, RecursiveMode::NonRecursive).is_ok()
        {
            self.exclude_watch = Some(parent.to_path_buf());
            self.exclude = Some(exclude);
        }
    }

    /// イベントのパスが `info/exclude` か。
    fn is_exclude(&self, path: &Path) -> bool {
        self.exclude.as_ref().is_some_and(|exclude| {
            path.file_name() == exclude.file_name() && canonical(path.to_path_buf()) == *exclude
        })
    }

    /// 無視の規則が変わったか。読むと戻す。
    fn take_rules_changed(&mut self) -> bool {
        std::mem::take(&mut self.rules_changed)
    }

    /// 作業ツリーを辿り直し、無視されなくなったディレクトリを見張りに足し、無視される
    /// ようになったディレクトリの見張りを外す。上限を超えるか OS が断ったら Err。
    /// 根の再帰の見張りは無視したものの下も見張っているので、何もしない（覚えた判定は
    /// 規則が変わったときに忘れている）。
    fn rewalk(&mut self, watcher: &mut impl Watcher) -> Result<(), WatchFallback> {
        if self.strategy == Strategy::RecursiveRoot {
            return Ok(());
        }
        let found = self
            .tree
            .directories(WATCH_DIRECTORY_LIMIT)
            .map_err(|error| WatchFallback::Refused(error.to_string()))?;
        if found.over_limit {
            return Err(WatchFallback::TooManyDirectories {
                limit: WATCH_DIRECTORY_LIMIT,
            });
        }
        let listed: HashSet<&PathBuf> = found.directories.iter().collect();
        let dropped: Vec<PathBuf> = self
            .directories
            .iter()
            .filter(|directory| !listed.contains(directory))
            .cloned()
            .collect();
        let mut paths = watcher.paths_mut();
        for directory in dropped {
            self.directories.remove(&directory);
            if self.added.remove(&directory) {
                let _ = paths.remove(&directory);
            }
        }
        paths
            .commit()
            .map_err(|error| WatchFallback::Refused(error.to_string()))?;
        for ignored in found.ignored {
            self.verdicts.insert(ignored, true);
        }
        self.add(watcher, found.directories)
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
        if self.strategy == Strategy::RecursiveRoot {
            let _ = paths.remove(self.tree.root());
        }
        for directory in self.added.drain() {
            let _ = paths.remove(&directory);
        }
        self.exclude = None;
        if let Some(parent) = self.exclude_watch.take() {
            let _ = paths.remove(&parent);
        }
        let _ = paths.commit();
        self.directories.clear();
    }

    /// 作業ツリーを変えるイベントがあるか。作られたディレクトリ（無視されないもの）は見張りに
    /// 足す。git に聞くのは、まとめて 1 回だけ。`files`（起動時の差分のファイル、実体の場所）
    /// は無視されないと分かっているので聞かない。
    fn changes(
        &mut self,
        watcher: &mut impl Watcher,
        events: &[notify::Event],
        files: &HashSet<PathBuf>,
    ) -> Result<bool, WatchFallback> {
        // 消えたかどうかはイベントで決める。続けて同じ名前に戻されると、処理する時点では
        // ディレクトリがあるように見える。
        let mut occurrences: Vec<Occurrence> = Vec::new();
        let mut gone: HashSet<PathBuf> = HashSet::new();
        for event in events {
            if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
                continue;
            }
            for (position, path) in event.paths.iter().enumerate() {
                if self.is_exclude(path) {
                    self.rules_changed = true;
                }
                let Some(path) = self.inside(path) else {
                    continue;
                };
                let moved = if moves_away(&event.kind, position) {
                    gone.insert(path.clone());
                    Moved::Away
                } else if arrives(&event.kind, position) {
                    Moved::In
                } else {
                    Moved::Stayed
                };
                if path.file_name() == Some(".gitignore".as_ref()) {
                    self.rules_changed = true;
                }
                occurrences.push(Occurrence {
                    path,
                    moved,
                    settled: None,
                });
            }
        }
        if self.rules_changed {
            self.verdicts.clear();
        }
        if self.verdicts.len() > VERDICT_LIMIT {
            self.verdicts.clear();
        }
        // 消えたパスは見えていたときの判定で決め、その判定は忘れる。現れたパスは判定を
        // 忘れて聞き直す。同じ名前でディレクトリとファイルが入れ替わると、判定も変わる。
        // 1 回のまとまりに消えたのと現れたのが続けて入るので、イベントの順に決める。
        for occurrence in &mut occurrences {
            match occurrence.moved {
                Moved::Away => occurrence.settled = self.verdicts.remove(&occurrence.path),
                Moved::In => {
                    self.verdicts.remove(&occurrence.path);
                }
                Moved::Stayed => {}
            }
        }
        let known: HashSet<PathBuf> = occurrences
            .iter()
            .map(|occurrence| &occurrence.path)
            .filter(|path| files.contains(&self.canonical_of(path)))
            .cloned()
            .collect();
        // 無視したディレクトリの下のパスも無視されるので、聞かない。聞くときは祖先も
        // 一緒に聞いて覚え、その下で続く書き込み（ビルドの出力など）は git を呼ばずに捨てる。
        let mut unknown: Vec<PathBuf> = Vec::new();
        let mut asking: HashSet<PathBuf> = HashSet::new();
        for Occurrence { path, settled, .. } in &occurrences {
            if known.contains(path) || settled.is_some() || self.under_ignored(path) {
                continue;
            }
            for asked in self.unjudged_ancestors(path).chain([path.clone()]) {
                if !self.verdicts.contains_key(&asked) && asking.insert(asked.clone()) {
                    unknown.push(asked);
                }
            }
        }
        if !unknown.is_empty() {
            // git に聞けないときは、無視しないものとして数える（変化を見落とさない側に倒す）。
            let ignored = self.tree.ignored(&unknown).unwrap_or_default();
            for path in unknown {
                let verdict = ignored.contains(&path);
                self.verdicts.insert(path, verdict);
            }
        }

        let mut changed = false;
        let mut created: Vec<PathBuf> = Vec::new();
        for Occurrence { path, settled, .. } in occurrences {
            if known.contains(&path) {
                changed = true;
                continue;
            }
            let ignored = settled
                .or_else(|| self.verdicts.get(&path).copied())
                .is_some_and(|verdict| verdict);
            if ignored || self.under_ignored(&path) {
                continue;
            }
            changed = true;
            if self.strategy == Strategy::RecursiveRoot {
                continue;
            }
            let is_directory = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir());
            if (gone.contains(&path) || !is_directory) && self.directories.contains(&path) {
                // 消えた・改名したディレクトリの見張りは、その下の分も含めて notify が外している。
                // 下の分を覚えたままだと、同じ名前に戻ったときに見張り直さない。
                self.directories
                    .retain(|directory| !directory.starts_with(&path));
                self.added.retain(|directory| !directory.starts_with(&path));
                // 差分のファイルのための見張りも消えている。残すと、戻ったときに足さない。
                let canonical = self.canonical_of(&path);
                self.kept
                    .retain(|directory| !directory.starts_with(&canonical));
            }
            if is_directory && !self.directories.contains(&path) && !created.contains(&path) {
                created.push(path);
            }
        }
        if !created.is_empty() {
            self.watch_new_directories(watcher, &created)?;
        }
        Ok(changed)
    }

    /// 作業ツリーの根からの表記の `path` を、実体の場所の表記にする。もう無いパスも変えられる
    /// よう、ファイルシステムには聞かずに根の表記だけを付け替える。
    fn canonical_of(&self, path: &Path) -> PathBuf {
        path.strip_prefix(self.tree.root()).map_or_else(
            |_| path.to_path_buf(),
            |relative| self.canonical_root.join(relative),
        )
    }

    /// `path` の祖先（根を除く）に、git が無視すると覚えたディレクトリがあるか。
    fn under_ignored(&self, path: &Path) -> bool {
        self.ancestors(path)
            .any(|ancestor| self.verdicts.get(ancestor) == Some(&true))
    }

    /// `path` の祖先（根を除く）のうち、判定を覚えていないもの。
    fn unjudged_ancestors<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = PathBuf> + 'a {
        self.ancestors(path)
            .filter(|ancestor| !self.verdicts.contains_key(*ancestor))
            .map(Path::to_path_buf)
    }

    /// 作業ツリーの根からの表記の `path` の祖先（`path` 自身と根を除く）。
    fn ancestors<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a Path> + 'a {
        path.ancestors()
            .skip(1)
            .take_while(|ancestor| *ancestor != self.tree.root())
    }

    fn watch_new_directories(
        &mut self,
        watcher: &mut impl Watcher,
        directories: &[PathBuf],
    ) -> Result<(), WatchFallback> {
        let room = WATCH_DIRECTORY_LIMIT.saturating_sub(self.directories.len());
        let found = self
            .tree
            .directories_under(directories, room)
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

/// イベントの `position` 番目のパスが、その場所から無くなったか（削除と改名の元）。
fn moves_away(kind: &EventKind, position: usize) -> bool {
    use notify::event::{ModifyKind, RenameMode};
    matches!(
        kind,
        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From))
    ) || (matches!(kind, EventKind::Modify(ModifyKind::Name(RenameMode::Both))) && position == 0)
}

/// 1 回のまとまりのイベントに出てきたパス 1 つ。
struct Occurrence {
    /// 作業ツリーの根からの表記。
    path: PathBuf,
    moved: Moved,
    /// 消えたパスの、見えていたときの判定（無視するなら true）。
    settled: Option<bool>,
}

/// イベントでパスがその場所から無くなったか、現れたか。
#[derive(Clone, Copy)]
enum Moved {
    /// 削除と改名の元。
    Away,
    /// 作成と改名の先。
    In,
    /// 書き込みなど、その場所のまま。
    Stayed,
}

/// イベントの `position` 番目のパスが、その場所に現れたか（作成と改名の先）。macOS の
/// 改名はどちら向きか分からないので、現れたものとして判定を聞き直す。
fn arrives(kind: &EventKind, position: usize) -> bool {
    use notify::event::{ModifyKind, RenameMode};
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To | RenameMode::Any))
    ) || (matches!(kind, EventKind::Modify(ModifyKind::Name(RenameMode::Both))) && position == 1)
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

    /// 足した見張りと外した見張り、いま効いている見張りを覚える偽の見張り。`refuse` なら
    /// 足すのを断る。
    #[derive(Default)]
    struct Recorder {
        watched: Vec<(PathBuf, RecursiveMode)>,
        unwatched: Vec<PathBuf>,
        live: HashSet<PathBuf>,
        refuse: bool,
    }

    impl Watcher for Recorder {
        fn new<F: notify::EventHandler>(_: F, _: notify::Config) -> notify::Result<Self> {
            Ok(Recorder::default())
        }

        fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
            if self.refuse {
                return Err(notify::Error::generic("refused"));
            }
            self.watched.push((path.to_path_buf(), mode));
            self.live.insert(path.to_path_buf());
            Ok(())
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            self.unwatched.push(path.to_path_buf());
            self.live.remove(path);
            Ok(())
        }

        fn kind() -> notify::WatcherKind {
            notify::WatcherKind::NullWatcher
        }
    }

    /// git の作業ツリーではない場所。見張りの登録だけを試す（git に聞く処理は失敗する）。
    fn missing_root() -> PathBuf {
        std::env::temp_dir().join(format!("kemi-watch-missing-{}", std::process::id()))
    }

    #[test]
    fn the_recursive_strategy_watches_only_the_root_recursively_and_unwatches_it_on_stop() {
        let root = missing_root();
        let mut watcher = Recorder::default();

        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(root.clone()),
            Strategy::RecursiveRoot,
            HashSet::new(),
        )
        .expect("the root is watched");

        assert_eq!(
            watcher.watched,
            vec![(root.clone(), RecursiveMode::Recursive)]
        );
        tree.stop(&mut watcher);
        assert_eq!(watcher.unwatched, vec![root]);
    }

    #[test]
    fn the_recursive_strategy_falls_back_when_the_root_cannot_be_watched() {
        let mut watcher = Recorder {
            refuse: true,
            ..Recorder::default()
        };

        let started = TreeWatch::start(
            &mut watcher,
            WorkTree::new(missing_root()),
            Strategy::RecursiveRoot,
            HashSet::new(),
        );

        assert!(matches!(started, Err(WatchFallback::Refused(_))));
    }

    /// `.gitignore` に `build/` を書いた一時リポジトリ。落とすと消す。
    struct IgnoringRepo(PathBuf);

    impl IgnoringRepo {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("kemi-watch-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let status = std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&root)
                .status()
                .unwrap();
            assert!(status.success());
            std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
            std::fs::create_dir_all(root.join("build/deep")).unwrap();
            IgnoringRepo(root)
        }
    }

    impl Drop for IgnoringRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_recursive_strategy_drops_changes_under_ignored_directories_and_dot_git() {
        use notify::event::{CreateKind, ModifyKind};
        let repo = IgnoringRepo::new("recursive-drops");
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::RecursiveRoot,
            HashSet::new(),
        )
        .unwrap();
        let mut changes = |kind, relative: &str| {
            let path = repo.0.join(relative);
            tree.changes(
                &mut watcher,
                &[notify::Event::new(kind).add_path(path)],
                &HashSet::new(),
            )
            .unwrap()
        };

        assert!(!changes(
            EventKind::Create(CreateKind::File),
            "build/deep/x.o"
        ));
        assert!(!changes(
            EventKind::Create(CreateKind::File),
            "build/deep/y.o"
        ));
        assert!(!changes(EventKind::Modify(ModifyKind::Any), ".git/index"));
        assert!(changes(EventKind::Create(CreateKind::File), "src/new.rs"));
        assert!(changes(EventKind::Modify(ModifyKind::Any), "a.txt"));
    }

    /// `tree` に `relative` への `kind` のイベントを 1 つ渡し、変化として数えたか。
    fn changes_by(
        tree: &mut TreeWatch,
        watcher: &mut Recorder,
        kind: EventKind,
        path: PathBuf,
    ) -> bool {
        tree.changes(
            watcher,
            &[notify::Event::new(kind).add_path(path)],
            &HashSet::new(),
        )
        .unwrap()
    }

    #[test]
    fn removing_an_ignored_directory_is_not_a_change_even_before_anything_under_it_was_seen() {
        use notify::event::RemoveKind;
        let repo = IgnoringRepo::new("remove-ignored");
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::RecursiveRoot,
            HashSet::new(),
        )
        .unwrap();
        std::fs::remove_dir_all(repo.0.join("build")).unwrap();

        assert!(!changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::Folder),
            repo.0.join("build/deep"),
        ));
        assert!(!changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::Folder),
            repo.0.join("build"),
        ));
    }

    #[test]
    fn removing_an_ignored_directory_right_after_the_ignore_rules_change_is_not_a_change() {
        use notify::event::{ModifyKind, RemoveKind};
        let repo = IgnoringRepo::new("remove-after-rules");
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::PerDirectory,
            HashSet::new(),
        )
        .unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Modify(ModifyKind::Any),
            repo.0.join(".gitignore"),
        );
        std::fs::remove_dir_all(repo.0.join("build")).unwrap();

        assert!(!changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::Folder),
            repo.0.join("build"),
        ));
    }

    #[test]
    fn a_file_that_replaces_an_ignored_directory_of_the_same_name_is_watched() {
        use notify::event::{CreateKind, ModifyKind, RemoveKind};
        let repo = IgnoringRepo::new("directory-to-file");
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::PerDirectory,
            HashSet::new(),
        )
        .unwrap();
        std::fs::remove_dir_all(repo.0.join("build")).unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::Folder),
            repo.0.join("build"),
        );
        std::fs::write(repo.0.join("build"), "now a file\n").unwrap();

        assert!(changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Create(CreateKind::File),
            repo.0.join("build"),
        ));
        assert!(changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Modify(ModifyKind::Any),
            repo.0.join("build"),
        ));
    }

    #[test]
    fn a_file_that_replaces_an_ignored_directory_in_the_same_batch_of_events_is_a_change() {
        use notify::event::{CreateKind, RemoveKind};
        let repo = IgnoringRepo::new("directory-to-file-batch");
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::PerDirectory,
            HashSet::new(),
        )
        .unwrap();
        std::fs::remove_dir_all(repo.0.join("build")).unwrap();
        std::fs::write(repo.0.join("build"), "now a file\n").unwrap();

        let changed = tree
            .changes(
                &mut watcher,
                &[
                    notify::Event::new(EventKind::Remove(RemoveKind::Folder))
                        .add_path(repo.0.join("build")),
                    notify::Event::new(EventKind::Create(CreateKind::File))
                        .add_path(repo.0.join("build")),
                ],
                &HashSet::new(),
            )
            .unwrap();

        assert!(changed);
    }

    #[test]
    fn an_ignored_directory_that_replaces_a_file_of_the_same_name_is_not_watched() {
        use notify::event::{CreateKind, ModifyKind, RemoveKind};
        let repo = IgnoringRepo::new("file-to-directory");
        std::fs::remove_dir_all(repo.0.join("build")).unwrap();
        std::fs::write(repo.0.join("build"), "a file\n").unwrap();
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::PerDirectory,
            HashSet::new(),
        )
        .unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Modify(ModifyKind::Any),
            repo.0.join("build"),
        );
        std::fs::remove_file(repo.0.join("build")).unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::File),
            repo.0.join("build"),
        );
        std::fs::create_dir(repo.0.join("build")).unwrap();

        assert!(!changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Create(CreateKind::Folder),
            repo.0.join("build"),
        ));
        assert!(
            !watcher
                .watched
                .iter()
                .any(|(path, _)| path == &repo.0.join("build"))
        );
    }

    #[test]
    fn a_recreated_directory_that_holds_a_diff_file_is_watched_again() {
        use notify::event::{CreateKind, RemoveKind};
        let repo = IgnoringRepo::new("recreated-diff-directory");
        let src = repo.0.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let mut watcher = Recorder::default();
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::PerDirectory,
            HashSet::from([canonical(src.clone())]),
        )
        .unwrap();
        std::fs::remove_dir_all(&src).unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::Folder),
            src.clone(),
        );
        std::fs::create_dir(&src).unwrap();

        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Create(CreateKind::Folder),
            src.clone(),
        );

        assert!(watcher.live.contains(&src), "{:?}", watcher.live);
    }

    /// 知らせを捨てる。
    struct Silent;

    impl NoticeSink for Silent {
        fn notify(&self, _: Notice) {}
    }

    #[test]
    fn a_recreated_diff_directory_stays_watched_after_falling_back() {
        use notify::event::{CreateKind, RemoveKind};
        let repo = IgnoringRepo::new("fall-back-recreated");
        let src = repo.0.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let mut diff = DiffWatch::default();
        diff.directories.insert(canonical(src.clone()));
        let mut watcher = Recorder::default();
        diff.ensure(&mut watcher);
        let mut tree = TreeWatch::start(
            &mut watcher,
            WorkTree::new(repo.0.clone()),
            Strategy::PerDirectory,
            diff.directories.clone(),
        )
        .unwrap();
        std::fs::remove_dir_all(&src).unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Remove(RemoveKind::Folder),
            src.clone(),
        );
        std::fs::create_dir(&src).unwrap();
        changes_by(
            &mut tree,
            &mut watcher,
            EventKind::Create(CreateKind::Folder),
            src.clone(),
        );

        fall_back(
            &mut Some(tree),
            &mut diff,
            &mut watcher,
            &Silent,
            WatchFallback::TooManyDirectories {
                limit: WATCH_DIRECTORY_LIMIT,
            },
        );

        assert!(
            watcher
                .live
                .iter()
                .any(|path| canonical(path.clone()) == canonical(src.clone())),
            "{:?}",
            watcher.live
        );
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
