//! セッションのディレクトリの掃除（R-SESSION）。上限を超えたセッションと、強制終了で
//! 残った孤児のロック・写し・`<id>.files/`・つなぎ先・起動中の記録を片付ける。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::is_valid_id;
use super::lock::{SessionLock, is_locked, with_free_lock};
use super::storage_io::list_files;
use super::store::{files_path, payload_path, read_meta, session_path};

/// セッションのファイルが無いロックを孤児として片付けるまでの猶予。始まったばかりの
/// セッションは、ロックを取ってから最初の `<id>.session` を書くまでこの形になる。
pub(super) const ORPHAN_LOCK_MIN_AGE: Duration = Duration::from_secs(60);

/// 上限を超えた分を最終更新の古い順に消す。ロック中のセッションは消さず、件数とバイト数にも
/// 数えない。復元できないセッションも数える。1 セッションの大きさは `<id>.session` と
/// `<id>.payload` と `<id>.files/` の中のファイルの合算（R-SESSION）。
pub(crate) fn cleanup(dir: &Path, keep_count: usize, keep_bytes: u64) {
    cleanup_at(dir, keep_count, keep_bytes, SystemTime::now());
}

/// [`cleanup`] の本体。孤児のロックの古さを `now` から測る。
pub(crate) fn cleanup_at(dir: &Path, keep_count: usize, keep_bytes: u64, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let contents = classify(entries.flatten());
    remove_records_of_stopped_reviews(dir, &contents.records);
    remove_orphans(dir, &contents, now);
    remove_beyond_limits(dir, &contents, keep_count, keep_bytes);
}

/// セッションの置き場所のファイルを、id ごとに種類で振り分けたもの。
#[derive(Default)]
struct DirContents {
    /// id と `<id>.session` の大きさ。
    sessions: BTreeMap<String, u64>,
    /// id と `<id>.payload` の大きさ。
    payloads: BTreeMap<String, u64>,
    /// id と `<id>.files/` の中のファイルの大きさの合計。
    files: BTreeMap<String, u64>,
    /// `<id>.endpoint` か `<id>.running` のある id。
    records: BTreeSet<String>,
    /// id と `<id>.lock` の更新時刻。
    locks: Vec<(String, Option<SystemTime>)>,
}

impl DirContents {
    /// 1 セッションの大きさ（R-SESSION）。`session` は `<id>.session` の大きさ。
    fn size_of(&self, id: &str, session: u64) -> u64 {
        session
            + self.payloads.get(id).copied().unwrap_or(0)
            + self.files.get(id).copied().unwrap_or(0)
    }
}

/// 振り分けは名前の接尾辞だけで行う。どのファイルが孤児かを決めるのに `<id>.session` の
/// 中身は読まない（R-SESSION）。
fn classify(entries: impl IntoIterator<Item = std::fs::DirEntry>) -> DirContents {
    let mut contents = DirContents::default();
    for entry in entries {
        let name = entry.file_name();
        let Some((id, kind)) = name.to_str().and_then(|name| name.rsplit_once('.')) else {
            continue;
        };
        // id でない名前は kemi のファイルではないので、数えも消しもしない。
        if !is_valid_id(id) {
            continue;
        }
        let metadata = entry.metadata().ok();
        let size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
        match kind {
            "files" => {
                // `<id>.files/` は 1 段なので、中の一覧と大きさだけを読む。
                let size = list_files(&entry.path())
                    .map(|listed| listed.iter().map(|(_, size)| size).sum())
                    .unwrap_or(0);
                contents.files.insert(id.to_string(), size);
            }
            "session" => {
                contents.sessions.insert(id.to_string(), size);
            }
            "payload" => {
                contents.payloads.insert(id.to_string(), size);
            }
            "endpoint" | "running" => {
                contents.records.insert(id.to_string());
            }
            "lock" => {
                let modified = metadata.and_then(|meta| meta.modified().ok());
                contents.locks.push((id.to_string(), modified));
            }
            _ => {}
        }
    }
    contents
}

/// 動いていないレビューのつなぎ先と起動中の記録は、強制終了で残ったもの。消したセッションの
/// ものも含め、ロックの生死だけで片付ける（R-AGENT-LINK、R-PS-RECORD）。残ったロックのファイルは、
/// セッションの無いものを孤児のロックとして後で片付ける。
fn remove_records_of_stopped_reviews(dir: &Path, ids: &BTreeSet<String>) {
    for_each_stopped_review(dir, ids, |id| {
        let _ = std::fs::remove_file(super::endpoint_path(dir, id));
        let _ = std::fs::remove_file(super::running_path(dir, id));
    });
}

/// ロックの外れた id ごとに、そのロックを持ったまま `tidy` を行う。確かめてから消すまでの間に
/// 同じ id の `--resume` がロックを取ると、書き直したばかりのものを消してしまうので。復元は
/// 1 秒まで取り直すので断られない。
pub(super) fn for_each_stopped_review(
    dir: &Path,
    ids: &BTreeSet<String>,
    mut tidy: impl FnMut(&str),
) {
    for id in ids {
        with_free_lock(dir, id, || tidy(id));
    }
}

/// 孤児のロックと写しと `<id>.files/` は、セッションを消すかどうかと関係なく毎回片付ける。
fn remove_orphans(dir: &Path, contents: &DirContents, now: SystemTime) {
    let without_session = |id: &&String| !contents.sessions.contains_key(*id);
    let orphan_locks = contents
        .locks
        .iter()
        .filter(|(id, _)| without_session(&id))
        .map(|(id, modified)| (id.as_str(), *modified));
    prune_orphan_locks(dir, orphan_locks, now, ORPHAN_LOCK_MIN_AGE);
    for id in contents.payloads.keys().filter(without_session) {
        // 起動中のレビューは `<id>.payload` を書いてから `<id>.session` を書くので、
        // その間だけこの形になる。ロックを持っているものは消さない（R-SESSION）。
        if is_locked(dir, id) {
            continue;
        }
        let _ = std::fs::remove_file(payload_path(dir, id));
    }
    // 強制終了で残った `<id>.files/` も同じ形で回収する。起動中の `--live` のレビューは
    // 会話が無い間も書くが、ロックを持っているので消さない（R-PAGE-SESSION）。
    for id in contents.files.keys().filter(without_session) {
        if is_locked(dir, id) {
            continue;
        }
        let _ = std::fs::remove_dir_all(files_path(dir, id));
    }
}

/// 上限を超えた分のセッションを消す。
fn remove_beyond_limits(dir: &Path, contents: &DirContents, keep_count: usize, keep_bytes: u64) {
    let total: u64 = contents
        .sessions
        .iter()
        .map(|(id, size)| contents.size_of(id, *size))
        .sum();
    // ロックを確かめる前の安い判定。ロック中を含めて上限に収まるなら、除いても収まる。
    if contents.sessions.len() <= keep_count && total <= keep_bytes {
        return;
    }
    let entries = contents
        .sessions
        .iter()
        .map(|(id, size)| CleanupEntry {
            id: id.clone(),
            size: contents.size_of(id, *size),
            locked: is_locked(dir, id),
        })
        .collect();
    let removed = sessions_to_remove(entries, keep_count, keep_bytes, |id| {
        read_updated(&session_path(dir, id)).unwrap_or(0)
    });
    for id in removed {
        // `<id>.session` を消してから `<id>.payload` を消す（R-SESSION の書く順序）。
        let _ = std::fs::remove_file(session_path(dir, &id));
        let _ = std::fs::remove_file(payload_path(dir, &id));
        let _ = std::fs::remove_dir_all(files_path(dir, &id));
    }
}

/// 掃除で見るセッション 1 つ。大きさは `<id>.session` と `<id>.payload` と `<id>.files/` の合算。
struct CleanupEntry {
    id: String,
    size: u64,
    locked: bool,
}

/// 上限を超えて消すセッションの id。最終更新の新しい順（同時刻は id の大きい順）に、件数と
/// バイト数の上限まで残す。ロック中のセッションは消さず、件数とバイト数にも数えない。
/// `updated_of` は最終更新を読む（`<id>.session` の中身を読む）。並べ替えにしか使わないので、
/// ロック中を除いて上限に収まるときは読まない（R-SESSION）。
fn sessions_to_remove(
    entries: Vec<CleanupEntry>,
    keep_count: usize,
    keep_bytes: u64,
    mut updated_of: impl FnMut(&str) -> u128,
) -> Vec<String> {
    let unlocked: Vec<CleanupEntry> = entries.into_iter().filter(|entry| !entry.locked).collect();
    let total: u64 = unlocked.iter().map(|entry| entry.size).sum();
    if unlocked.len() <= keep_count && total <= keep_bytes {
        return Vec::new();
    }
    let mut sorted: Vec<(u128, CleanupEntry)> = unlocked
        .into_iter()
        .map(|entry| (updated_of(&entry.id), entry))
        .collect();
    sorted.sort_by(|left, right| (right.0, &right.1.id).cmp(&(left.0, &left.1.id)));
    let mut kept = 0usize;
    let mut bytes = 0u64;
    let mut removed = Vec::new();
    for (_, entry) in sorted {
        if kept < keep_count && bytes + entry.size <= keep_bytes {
            kept += 1;
            bytes += entry.size;
        } else {
            removed.push(entry.id);
        }
    }
    removed
}

/// セッションのファイルが無いまま残ったロックファイル（id と更新時刻）を片付ける。始まった
/// ばかりのセッションと競合しないよう、`min_age` より新しいものは触らない。ロック中のものは
/// 他プロセスが使っているので残す。
fn prune_orphan_locks<'a>(
    dir: &Path,
    locks: impl IntoIterator<Item = (&'a str, Option<SystemTime>)>,
    now: SystemTime,
    min_age: Duration,
) {
    for (id, modified) in locks {
        let old_enough = modified
            .and_then(|modified| now.duration_since(modified).ok())
            .is_none_or(|age| age >= min_age);
        if !old_enough {
            continue;
        }
        // acquire は既にあるファイルを開き直す。取れたら手放し、Drop が名前を外す。
        if let Ok(lock) = SessionLock::acquire(dir, id) {
            drop(lock);
        }
    }
}

fn read_updated(path: &Path) -> Option<u128> {
    Some(read_meta(path).ok()?.summary().0.updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_reads_no_session_when_the_unlocked_ones_are_within_the_limits() {
        // 保留したセッションが上限ちょうどと、起動中のレビューが 1 件。ロック中は数えないので
        // 上限の内側で、消すものは無い。最終更新を読むと、状態を書くたびに全セッションの
        // 中身を読み直すことになる（R-SESSION の反例）。
        let mut entries: Vec<CleanupEntry> = (0..3)
            .map(|index| CleanupEntry {
                id: format!("held-{index}"),
                size: 10,
                locked: false,
            })
            .collect();
        entries.push(CleanupEntry {
            id: "running".to_string(),
            size: 1_000,
            locked: true,
        });

        let removed = sessions_to_remove(entries, 3, 30, |id| {
            panic!("the updated time of {id} must not be read")
        });

        assert!(removed.is_empty());
    }
}
