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

/// `value` を JSON にして [`write_atomic`] で書く（`<id>.endpoint` と `<id>.running`）。
pub(super) fn write_json_atomic(
    dir: &Path,
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<(), SessionError> {
    let bytes = serde_json::to_vec(value).map_err(|error| SessionError::Encode {
        reason: error.to_string(),
    })?;
    write_atomic(dir, path, &bytes)
}

/// JSON のファイルを読む。無ければ `None`、読み解けなければ `Corrupt`。
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind は non_exhaustive で、列挙しきれない"
)]
pub(super) fn read_json<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, SessionError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => {
                return Err(SessionError::Io {
                    path: path.to_path_buf(),
                    source: error,
                });
            }
        },
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| SessionError::Corrupt {
            path: path.to_path_buf(),
            reason: error.to_string(),
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
