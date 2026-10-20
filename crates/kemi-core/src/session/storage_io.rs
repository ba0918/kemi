//! セッション保存領域のファイル操作と所有者だけのアクセス権限。

use std::path::Path;

use super::SessionError;
use crate::private_fs::{self, Existing};

pub(super) fn list_files(dir: &Path) -> Result<Vec<(String, u64)>, SessionError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(SessionError::Io {
                path: dir.to_path_buf(),
                source,
            });
        }
    };
    Ok(entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            Some((entry.file_name().into_string().ok()?, metadata.len()))
        })
        .collect())
}

/// 一時ファイルは掃除が見る `.session` と `.payload` の接尾辞にしない。一時ファイルの名前は
/// 書き先ごとに決まっているので、強制終了で残ったものは書き直す。
pub(super) fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), SessionError> {
    create_private_dir(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = dir.join(format!(".{name}.tmp"));
    let written = private_fs::write_via_temporary(&temporary, path, bytes, Existing::Truncate);
    written.map_err(|source| SessionError::Io {
        path: temporary,
        source,
    })
}

pub(super) fn remove_dir_if_present(path: &Path) -> Result<(), SessionError> {
    remove_if_present_with(path, |path| std::fs::remove_dir_all(path))
}

pub(super) fn remove_if_present(path: &Path) -> Result<(), SessionError> {
    remove_if_present_with(path, |path| std::fs::remove_file(path))
}

/// `remove` で消す。無いのは成功と同じ。
fn remove_if_present_with(
    path: &Path,
    remove: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), SessionError> {
    match remove(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SessionError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(super) fn create_private_dir(dir: &Path) -> Result<(), SessionError> {
    private_fs::create_private_dir(dir).map_err(|source| SessionError::Io {
        path: dir.to_path_buf(),
        source,
    })
}

pub(super) fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// `<id>.files/` のファイルを所有者だけが読み書きできる形で書く。
pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    private_fs::write_private_file(path, bytes, Existing::Truncate)
}
