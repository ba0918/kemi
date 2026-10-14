//! セッション保存領域のファイル操作と所有者だけのアクセス権限。

use std::path::Path;

use super::SessionError;

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

/// 一時ファイルは掃除が見る `.session` と `.payload` の接尾辞にしない。
pub(super) fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), SessionError> {
    create_private_dir(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = dir.join(format!(".{name}.tmp"));
    let written =
        write_private_file(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path));
    if let Err(source) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(SessionError::Io {
            path: temporary,
            source,
        });
    }
    Ok(())
}

pub(super) fn remove_dir_if_present(path: &Path) -> Result<(), SessionError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SessionError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(super) fn remove_if_present(path: &Path) -> Result<(), SessionError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SessionError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(super) fn create_private_dir(dir: &Path) -> Result<(), SessionError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|source| SessionError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|source| {
            SessionError::Io {
                path: dir.to_path_buf(),
                source,
            }
        })
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir).map_err(|source| SessionError::Io {
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

pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)
}
