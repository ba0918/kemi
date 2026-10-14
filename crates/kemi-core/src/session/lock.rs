//! プロセスの終了時にも OS が解放するセッションロック。

use std::path::{Path, PathBuf};

use super::SessionError;
use super::storage_io::{create_private_dir, open_lock_file};

pub struct SessionLock {
    file: Option<std::fs::File>,
    path: PathBuf,
}

impl SessionLock {
    pub(super) fn acquire(dir: &Path, id: &str) -> Result<Self, SessionError> {
        create_private_dir(dir)?;
        let path = dir.join(format!("{id}.lock"));
        let file = open_lock_file(&path).map_err(|source| SessionError::Io {
            path: path.clone(),
            source,
        })?;
        match file.try_lock() {
            Ok(()) => Ok(SessionLock {
                file: Some(file),
                path,
            }),
            Err(std::fs::TryLockError::WouldBlock) => {
                Err(SessionError::Locked { id: id.to_string() })
            }
            Err(std::fs::TryLockError::Error(source)) => Err(SessionError::Io { path, source }),
        }
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
