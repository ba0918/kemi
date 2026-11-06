//! プロセスの終了時にも OS が解放するセッションロック。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::SessionError;
use super::storage_io::{create_private_dir, open_lock_file};

pub(crate) struct SessionLock {
    file: Option<std::fs::File>,
    path: PathBuf,
}

/// 起動中かを確かめる側（ps、`--resume` の一覧、掃除）は一瞬ロックを取るので、新しいレビューと
/// 復元はこの間だけ取り直す（R-SESSION）。
pub(super) const ACQUIRE_PATIENCE: Duration = Duration::from_secs(1);

/// 取り直しの間隔。確かめる側が持つのは開いて閉じるまでの一瞬なので、短くてよい。
const RETRY_INTERVAL: Duration = Duration::from_millis(20);

impl SessionLock {
    /// 一度だけ取りに行く。取られていれば `Locked`。
    pub(super) fn acquire(dir: &Path, id: &str) -> Result<Self, SessionError> {
        Self::try_acquire(dir, id)?.ok_or_else(|| SessionError::Locked { id: id.to_string() })
    }

    /// 取れるまで `patience` の間だけ取り直す。過ぎても取れなければ `Locked`。
    pub(super) fn acquire_within(
        dir: &Path,
        id: &str,
        patience: Duration,
    ) -> Result<Self, SessionError> {
        let deadline = Instant::now() + patience;
        loop {
            if let Some(lock) = Self::try_acquire(dir, id)? {
                return Ok(lock);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(SessionError::Locked { id: id.to_string() });
            }
            std::thread::sleep(RETRY_INTERVAL.min(deadline - now));
        }
    }

    /// 開き直してから取りに行く。持ち主が手放すときは名前を外すので、前に開いたファイルで
    /// 取り直すと、誰も見ない古いファイルのロックを取ってしまう。
    fn try_acquire(dir: &Path, id: &str) -> Result<Option<Self>, SessionError> {
        create_private_dir(dir)?;
        Ok(try_lock_file(dir, id)?.map(|(file, path)| SessionLock {
            file: Some(file),
            path,
        }))
    }
}

/// ロックが外れていれば取り、持ったまま `action` を行ってから閉じる。取られていれば何もしない。
/// [`SessionLock`] と違い、放すときにロックのファイルの名前を外さない。外してから閉じると、
/// その前に同じパスを開いた `--resume` が名前の外れた古いファイルのロックを取り、次の
/// `--resume` も新しいファイルで取れて、二重に復元される（R-SESSION）。掃除が一覧を読めた
/// `dir` で使うので、ディレクトリは作らない（無ければ取れず、何もしない）。
pub(super) fn with_free_lock(dir: &Path, id: &str, action: impl FnOnce()) {
    if let Ok(Some((file, _))) = try_lock_file(dir, id) {
        action();
        drop(file);
    }
}

fn try_lock_file(dir: &Path, id: &str) -> Result<Option<(std::fs::File, PathBuf)>, SessionError> {
    let path = dir.join(format!("{id}.lock"));
    let file = open_lock_file(&path).map_err(|source| SessionError::Io {
        path: path.clone(),
        source,
    })?;
    match file.try_lock() {
        Ok(()) => Ok(Some((file, path))),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(source)) => Err(SessionError::Io { path, source }),
    }
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        // 閉じてから名前を外すと、別プロセスが古いファイルを掴める。Windows では
        // 開いたファイルを消せないので、閉じてから試す。
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
        drop(self.file.take());
        #[cfg(not(unix))]
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) fn is_locked(dir: &Path, id: &str) -> bool {
    let path = dir.join(format!("{id}.lock"));
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
    else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => false,
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(std::fs::TryLockError::Error(_)) => false,
    }
}
