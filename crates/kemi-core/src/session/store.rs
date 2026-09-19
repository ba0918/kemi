//! セッションファイルの保存・読み出し・掃除・ロック（R-SESSION）。
//!
//! 書き込みは一時ファイルから rename する原子的な差し替え。状態の変更のたびに全体を
//! 書き直すが、写しの payload は gzip 済みのバイト列をそのまま使い回す。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::encoding;
use super::{CopyMeta, CopyState, MetaDto, SessionCopy, SessionInfo, SessionState, SessionSummary};
use crate::source::FileContent;

/// 写しの上限（圧縮後のバイト数）。超えた写しは捨てて復元不可にする。
pub const COPY_LIMIT: u64 = 20 * 1024 * 1024;
/// 全ワークスペース合計で残すセッション数。
pub const KEEP_SESSIONS: usize = 100;
/// 全ワークスペース合計で残すバイト数。
pub const KEEP_BYTES: u64 = 500 * 1024 * 1024;

#[derive(Debug)]
pub enum SessionError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Locked {
        id: String,
    },
    NotFound {
        id: String,
    },
    NotReady {
        id: String,
    },
    Unusable {
        id: String,
        reason: String,
    },
    Corrupt {
        path: PathBuf,
        reason: String,
    },
    UnsupportedVersion {
        path: PathBuf,
        version: u8,
    },
    Range {
        revision: String,
        reason: String,
    },
    Encode {
        reason: String,
    },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Io { path, source } => {
                write!(formatter, "cannot access {}: {source}", path.display())
            }
            SessionError::Locked { id } => write!(formatter, "session {id} is in use"),
            SessionError::NotFound { id } => write!(formatter, "no such session: {id}"),
            SessionError::NotReady { id } => write!(
                formatter,
                "session {id} cannot be resumed yet: the review copy is not complete"
            ),
            SessionError::Unusable { id, reason } => {
                write!(formatter, "session {id} cannot be resumed: {reason}")
            }
            SessionError::Corrupt { path, reason } => {
                write!(
                    formatter,
                    "cannot read session {}: {reason}",
                    path.display()
                )
            }
            SessionError::UnsupportedVersion { path, version } => write!(
                formatter,
                "session {} has an unsupported format version {version}",
                path.display()
            ),
            SessionError::Range { revision, reason } => {
                write!(formatter, "cannot resolve {revision}: {reason}")
            }
            SessionError::Encode { reason } => {
                write!(formatter, "cannot encode the session: {reason}")
            }
        }
    }
}

impl std::error::Error for SessionError {}

/// 保存済みのセッション 1 つ。
#[derive(Clone, Debug, PartialEq)]
pub struct StoredSession {
    pub info: SessionInfo,
    pub state: SessionState,
    pub copy: CopyState,
}

/// ディレクトリ 1 つに置かれたセッション群。
#[derive(Clone, Debug)]
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    pub fn new(dir: PathBuf) -> Self {
        SessionStore { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 新しいセッションを開く。まだ何も書かず、ロックだけを取る。
    pub fn create(&self, info: SessionInfo) -> Result<OpenSession, SessionError> {
        checked_id(&info.id)?;
        let lock = SessionLock::acquire(&self.dir, &info.id)?;
        Ok(OpenSession {
            dir: self.dir.clone(),
            info,
            state: SessionState::default(),
            copy: CopyState::Pending,
            payload: None,
            deleted: false,
            _lock: lock,
        })
    }

    /// 復元できるセッションを開く。ロックが取れない・読めない・写しが無いときは理由を返す。
    pub fn open(&self, id: &str) -> Result<OpenSession, SessionError> {
        checked_id(id)?;
        let lock = SessionLock::acquire(&self.dir, id)?;
        let (meta, payload) = self.read_raw(id)?;
        let path = self.path(id);
        let (info, state, copy) = meta.into_parts().map_err(|reason| SessionError::Corrupt {
            path: path.clone(),
            reason,
        })?;
        let copy = match copy {
            CopyMeta::Pending => return Err(SessionError::NotReady { id: id.to_string() }),
            CopyMeta::Unusable(reason) => {
                return Err(SessionError::Unusable {
                    id: id.to_string(),
                    reason,
                })
            }
            CopyMeta::Ready {
                startup_unit,
                units,
            } => {
                let raw = payload.as_deref().ok_or_else(|| SessionError::Corrupt {
                    path: path.clone(),
                    reason: "the copy is missing".to_string(),
                })?;
                let contents = decode_contents(&path, raw)?;
                CopyState::Ready(SessionCopy {
                    startup_unit,
                    units,
                    contents,
                })
            }
        };
        Ok(OpenSession {
            dir: self.dir.clone(),
            info,
            state,
            copy,
            payload,
            deleted: false,
            _lock: lock,
        })
    }

    /// 保存済みのセッションを、ロックを取らずに読む。
    pub fn read(&self, id: &str) -> Result<StoredSession, SessionError> {
        checked_id(id)?;
        let (meta, payload) = self.read_raw(id)?;
        let path = self.path(id);
        let (info, state, copy) = meta.into_parts().map_err(|reason| SessionError::Corrupt {
            path: path.clone(),
            reason,
        })?;
        let copy = match copy {
            CopyMeta::Pending => CopyState::Pending,
            CopyMeta::Unusable(reason) => CopyState::Unusable(reason),
            CopyMeta::Ready {
                startup_unit,
                units,
            } => {
                let raw = payload.as_deref().ok_or_else(|| SessionError::Corrupt {
                    path: path.clone(),
                    reason: "the copy is missing".to_string(),
                })?;
                CopyState::Ready(SessionCopy {
                    startup_unit,
                    units,
                    contents: decode_contents(&path, raw)?,
                })
            }
        };
        Ok(StoredSession { info, state, copy })
    }

    /// 復元できるセッションの一覧。最終更新の新しい順（同時刻は id の大きい順）。
    pub fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(SessionError::Io {
                    path: self.dir.clone(),
                    source,
                })
            }
        };
        let mut summaries = Vec::new();
        for entry in entries.flatten() {
            let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".session"))
                .map(str::to_string)
            else {
                continue;
            };
            let Ok(meta) = read_meta(&self.path(&id)) else {
                continue;
            };
            if !meta.is_resumable() {
                continue;
            }
            let (info, seen) = meta.summary();
            summaries.push(SessionSummary {
                id: info.id,
                updated: info.updated,
                workspace: info.workspace,
                mode: info.mode,
                title: info.title,
                seen,
                total_files: info.total_files,
            });
        }
        summaries.sort_by(|left, right| (right.updated, &right.id).cmp(&(left.updated, &left.id)));
        Ok(summaries)
    }

    /// セッションのファイルを消す。
    pub fn delete(&self, id: &str) -> Result<(), SessionError> {
        checked_id(id)?;
        let path = self.path(id);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SessionError::Io { path, source }),
        }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.session"))
    }

    fn read_raw(&self, id: &str) -> Result<(MetaDto, Option<Vec<u8>>), SessionError> {
        let path = self.path(id);
        let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => SessionError::NotFound { id: id.to_string() },
            _ => SessionError::Io {
                path: path.clone(),
                source,
            },
        })?;
        let envelope = encoding::decode_file(&bytes).map_err(|error| SessionError::Corrupt {
            path: path.clone(),
            reason: error.to_string(),
        })?;
        if envelope.version != encoding::VERSION {
            return Err(SessionError::UnsupportedVersion {
                path,
                version: envelope.version,
            });
        }
        let meta: MetaDto =
            serde_json::from_slice(envelope.meta).map_err(|error| SessionError::Corrupt {
                path: path.clone(),
                reason: error.to_string(),
            })?;
        Ok((meta, envelope.payload.map(<[u8]>::to_vec)))
    }
}

/// 公開の入口で id を検証する（R-SESSION）。ULID でない id は存在しない id と同じ
/// 理由にし、保存領域の外へパスを組み立てない。
fn checked_id(id: &str) -> Result<&str, SessionError> {
    if super::is_valid_id(id) {
        Ok(id)
    } else {
        Err(SessionError::NotFound { id: id.to_string() })
    }
}

fn decode_contents(
    path: &Path,
    payload: &[u8],
) -> Result<std::collections::BTreeMap<String, FileContent>, SessionError> {
    let inflated = encoding::gunzip(payload, encoding::DECOMPRESS_LIMIT).map_err(|error| {
        SessionError::Corrupt {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    })?;
    encoding::decode_contents(&inflated).map_err(|error| SessionError::Corrupt {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}

/// 開いているセッション。ロックを保持し、状態か写しが変わるたびに書き直す。
pub struct OpenSession {
    dir: PathBuf,
    info: SessionInfo,
    state: SessionState,
    copy: CopyState,
    /// 写しの gzip 済み payload。書き直しのたびに圧縮し直さないために持つ。
    payload: Option<Vec<u8>>,
    /// submit で消した後の保存を無視する印。凍結が submit と競争しても、消えた
    /// セッションを作り直さない（R-SESSION）。
    deleted: bool,
    /// このセッションのロック。フィールドとして持ち、Drop で解放する。
    _lock: SessionLock,
}

impl OpenSession {
    pub fn id(&self) -> &str {
        &self.info.id
    }

    pub fn info(&self) -> &SessionInfo {
        &self.info
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn copy(&self) -> &CopyState {
        &self.copy
    }

    /// 起動時に計算した題と全ファイル数を情報に写す。ファイルは書かない。
    pub fn describe(&mut self, title: &str, total_files: usize) {
        self.info.title = title.to_string();
        self.info.total_files = total_files;
    }

    /// 復元の対象になるか（写しが完成しているか）。
    pub fn is_resumable(&self) -> bool {
        matches!(self.copy, CopyState::Ready(_))
    }

    /// 状態を差し替えて保存する。空の状態で写しも無ければ、セッションは残さない。
    pub fn save_state(&mut self, state: SessionState) -> Result<(), SessionError> {
        self.save_state_at(state, super::now_millis())
    }

    /// 凍結した写しを保存する。圧縮後の上限を超えたら写しを捨てて復元不可にする。
    pub fn save_copy(&mut self, copy: SessionCopy) -> Result<(), SessionError> {
        self.save_copy_with_limit(copy, super::now_millis(), COPY_LIMIT)
    }

    /// 写しを作れなかったことを記録する。状態と情報だけが残る。
    pub fn mark_unresumable(&mut self, reason: &str) -> Result<(), SessionError> {
        self.mark_unresumable_at(reason, super::now_millis())
    }

    /// submit の確定後にセッションを消す。消した後の保存は無視する。
    pub fn delete(&mut self) -> Result<(), SessionError> {
        self.deleted = true;
        self.remove_file()
    }

    pub(crate) fn save_state_at(
        &mut self,
        state: SessionState,
        now: u128,
    ) -> Result<(), SessionError> {
        self.state = state;
        self.info.updated = now;
        self.persist()
    }

    pub(crate) fn save_copy_with_limit(
        &mut self,
        copy: SessionCopy,
        now: u128,
        limit: u64,
    ) -> Result<(), SessionError> {
        let raw = encoding::encode_contents(&copy.contents);
        let compressed = encoding::gzip(&raw);
        if compressed.len() as u64 > limit {
            self.copy = CopyState::Unusable("the review copy is larger than 20 MB".to_string());
            self.payload = None;
        } else {
            self.copy = CopyState::Ready(copy);
            self.payload = Some(compressed);
        }
        self.info.updated = now;
        self.persist()
    }

    pub(crate) fn mark_unresumable_at(
        &mut self,
        reason: &str,
        now: u128,
    ) -> Result<(), SessionError> {
        self.copy = CopyState::Unusable(reason.to_string());
        self.payload = None;
        self.info.updated = now;
        self.persist()
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("{}.session", self.info.id))
    }

    fn remove_file(&self) -> Result<(), SessionError> {
        let path = self.path();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SessionError::Io { path, source }),
        }
    }

    fn persist(&mut self) -> Result<(), SessionError> {
        if self.deleted {
            return Ok(());
        }
        // 写しを作れなかった理由を残すためでも、状態が空ならセッションは残さない
        // （R-SESSION）。
        if self.state.is_empty() && self.payload.is_none() {
            return self.remove_file();
        }
        let payload_len = self
            .payload
            .as_ref()
            .map_or(0, |payload| payload.len() as u64);
        let meta = MetaDto::from_parts(&self.info, &self.state, &self.copy, payload_len);
        let meta = serde_json::to_vec(&meta).map_err(|error| SessionError::Encode {
            reason: error.to_string(),
        })?;
        let bytes = encoding::encode_file(encoding::VERSION, &meta, self.payload.as_deref());
        write_atomic(&self.dir, &self.info.id, &bytes)?;
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(())
    }
}

/// セッションのロック。`File::try_lock` を使うので、プロセスが死ねば OS が解放する。
pub struct SessionLock {
    /// 開いている限りロックが生きる。
    file: Option<std::fs::File>,
    path: PathBuf,
}

impl SessionLock {
    fn acquire(dir: &Path, id: &str) -> Result<Self, SessionError> {
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
        // ロックファイルはプロセスの間だけのもの。名前を先に外してから閉じる（unix）。
        // 閉じてから外すと、その隙に別のプロセスが古いファイルを掴むことがある。
        // Windows は開いているファイルを消せないので、閉じてから試す。
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
        drop(self.file.take());
        #[cfg(not(unix))]
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_atomic(dir: &Path, id: &str, bytes: &[u8]) -> Result<(), SessionError> {
    create_private_dir(dir)?;
    let temporary = dir.join(format!(".{id}.tmp"));
    let path = dir.join(format!("{id}.session"));
    let written =
        write_private_file(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, &path));
    if let Err(source) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(SessionError::Io {
            path: temporary,
            source,
        });
    }
    Ok(())
}

/// 上限を超えた分を最終更新の古い順に消す。ロック中のセッションは消さず、件数とバイト数にも
/// 数えない。復元できないセッションも数える。
pub(crate) fn cleanup(dir: &Path, keep_count: usize, keep_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut sessions: Vec<(String, u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".session"))
            .map(str::to_string)
        else {
            continue;
        };
        let path = entry.path();
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        sessions.push((id, size, path));
    }
    // 孤児のロックは、セッションを消すかどうかと関係なく毎回片付ける。
    prune_orphan_locks(dir, Duration::from_secs(60));
    let total: u64 = sessions.iter().map(|(_, size, _)| size).sum();
    if sessions.len() <= keep_count && total <= keep_bytes {
        return;
    }
    // 最終更新は並べ替えにしか使わないので、消すものがあると分かってから読む。
    let mut sessions: Vec<(u128, String, u64, PathBuf)> = sessions
        .into_iter()
        .map(|(id, size, path)| (read_updated(&path).unwrap_or(0), id, size, path))
        .collect();
    sessions.sort_by(|left, right| (right.0, &right.1).cmp(&(left.0, &left.1)));
    let mut kept = 0usize;
    let mut bytes = 0u64;
    for (_, id, size, path) in sessions {
        if is_locked(dir, &id) {
            continue;
        }
        if kept < keep_count && bytes + size <= keep_bytes {
            kept += 1;
            bytes += size;
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// セッションのファイルが無いまま残ったロックファイルを片付ける。始まったばかりの
/// セッションと競合しないよう、`min_age` より新しいものは触らない。ロック中のものは
/// 他プロセスが使っているので残す。
fn prune_orphan_locks(dir: &Path, min_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".lock"))
            .map(str::to_string)
        else {
            continue;
        };
        if dir.join(format!("{id}.session")).exists() {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_none_or(|age| age >= min_age);
        if !old_enough {
            continue;
        }
        // acquire は既にあるファイルを開き直す。取れたら手放し、Drop が名前を外す。
        if let Ok(lock) = SessionLock::acquire(dir, &id) {
            drop(lock);
        }
    }
}

fn read_updated(path: &Path) -> Option<u128> {
    Some(read_meta(path).ok()?.summary().0.updated)
}

/// meta だけを読む。payload はファイルの後ろにまとまっているので、先頭の固定長から
/// meta の長さを知ってそこまでで読むのをやめる（一覧と掃除は payload を使わない）。
fn read_meta(path: &Path) -> Result<MetaDto, SessionError> {
    let mut file = std::fs::File::open(path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => SessionError::NotFound {
            id: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        },
        _ => SessionError::Io {
            path: path.to_path_buf(),
            source,
        },
    })?;
    let corrupt = |reason: String| SessionError::Corrupt {
        path: path.to_path_buf(),
        reason,
    };
    let mut head = [0u8; encoding::HEAD_LEN];
    file.read_exact(&mut head)
        .map_err(|error| corrupt(error.to_string()))?;
    let (version, length) =
        encoding::decode_head(&head).map_err(|error| corrupt(error.to_string()))?;
    if version != encoding::VERSION {
        return Err(SessionError::UnsupportedVersion {
            path: path.to_path_buf(),
            version,
        });
    }
    // 長さはファイルの中の値なので、壊れたファイルで巨大な確保を試みないように
    // 実際の大きさで頭打ちにする（`decode_file` の境界検査に当たる）。
    let rest = file
        .metadata()
        .map_err(|error| corrupt(error.to_string()))?
        .len()
        .saturating_sub(encoding::HEAD_LEN as u64);
    if length as u64 > rest {
        return Err(corrupt("metadata is cut off".to_string()));
    }
    let mut meta = vec![0u8; length];
    file.read_exact(&mut meta)
        .map_err(|error| corrupt(error.to_string()))?;
    serde_json::from_slice(&meta).map_err(|error| corrupt(error.to_string()))
}

fn is_locked(dir: &Path, id: &str) -> bool {
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

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> Result<(), SessionError> {
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
fn create_private_dir(dir: &Path) -> Result<(), SessionError> {
    std::fs::create_dir_all(dir).map_err(|source| SessionError::Io {
        path: dir.to_path_buf(),
        source,
    })
}

/// 所有者だけが読み書きできるファイル（0600）を新しく作る。あれば中身を捨てて作り直す。
#[cfg(unix)]
fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

#[cfg(unix)]
fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::review::{GroupBy, ReviewMeta, Side, Suggestion};
    use crate::session::{FrozenUnit, SessionMode};
    use crate::source::FileContent;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kemi-session-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&path);
            Scratch(path)
        }

        fn dir(&self) -> PathBuf {
            self.0.join("sessions")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn info(id: &str, updated: u128) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            created: 1,
            updated,
            workspace: PathBuf::from("/tmp/workspace"),
            workspace_key: "00000000000000aa".to_string(),
            mode: SessionMode::Worktree,
            title: "Working tree changes".to_string(),
            total_files: 2,
        }
    }

    fn comment() -> crate::domain::review::Comment {
        crate::domain::review::Comment {
            id: "c1".to_string(),
            file_id: "f1".to_string(),
            group_id: "all".to_string(),
            group_title: "final".to_string(),
            path: "src/a.rs".to_string(),
            side: Side::New,
            start_line: Some(1),
            end_line: Some(2),
            quote: vec!["one".to_string(), "two".to_string()],
            body: "please change".to_string(),
            replies: vec!["done".to_string()],
            resolved: true,
            outdated: false,
            content_hash: "hash".to_string(),
            suggestion: Some(Suggestion {
                replacement: "replaced".to_string(),
            }),
        }
    }

    fn state_with_comment() -> SessionState {
        SessionState {
            comments: vec![comment()],
            seen: ["f1".to_string()].into_iter().collect(),
            collapsed: [("f1".to_string(), true)].into_iter().collect(),
            last_comment: 1,
        }
    }

    fn review() -> ReviewMeta {
        ReviewMeta {
            title: "Working tree changes".to_string(),
            subtitle: "sub".to_string(),
            meta: serde_json::json!({ "branch": "main" }),
            groups: vec![crate::domain::review::Group {
                id: "all".to_string(),
                title: "final".to_string(),
                why: "why".to_string(),
                watch: "watch".to_string(),
                files: vec![crate::domain::review::FileEntry {
                    id: "f1".to_string(),
                    group_id: "all".to_string(),
                    path: "src/a.rs".to_string(),
                    old_path: None,
                    status: crate::domain::review::Status::Modify,
                    add: 1,
                    del: 1,
                    binary: false,
                    old_size: 0,
                    new_size: 0,
                    focus: false,
                    note: "note".to_string(),
                    noise: false,
                    // 復元でも「内容を読まなかった」判定が同じになるよう、写しに残る。
                    content_skipped: true,
                }],
            }],
            approval: vec![crate::domain::review::Approval {
                path: "src/a.rs".to_string(),
                identity: "sha256:abc".to_string(),
            }],
        }
    }

    fn content(old: Option<&[u8]>, new: Option<&[u8]>) -> FileContent {
        FileContent {
            old: old.map(<[u8]>::to_vec),
            new: new.map(<[u8]>::to_vec),
        }
    }

    fn copy() -> SessionCopy {
        SessionCopy {
            startup_unit: Some(GroupBy::File),
            units: vec![FrozenUnit {
                unit: Some(GroupBy::File),
                review: review(),
            }],
            contents: BTreeMap::from([
                (
                    "f1".to_string(),
                    content(Some(&[0xff, 0xfe]), Some(b"hello\n")),
                ),
                ("f2".to_string(), content(None, Some(b"added"))),
            ]),
        }
    }

    #[test]
    fn session_roundtrips_info_state_and_copy() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(state_with_comment(), 200).unwrap();
        open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();
        let id = open.id().to_string();
        drop(open);

        let opened = store.open(&id).unwrap();

        assert_eq!(opened.info().id, id);
        assert_eq!(opened.info().updated, 300);
        assert_eq!(opened.state(), &state_with_comment());
        assert_eq!(opened.copy(), &CopyState::Ready(copy()));
        assert!(opened.is_resumable());
    }

    #[test]
    fn session_without_a_copy_is_not_resumable() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(state_with_comment(), 200).unwrap();
        let id = open.id().to_string();
        drop(open);

        let stored = store.read(&id).unwrap();
        assert_eq!(stored.copy, CopyState::Pending);
        assert!(matches!(
            store.open(&id),
            Err(SessionError::NotReady { .. })
        ));
    }

    #[test]
    fn session_copy_is_stored_as_gzip() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
        let id = open.id().to_string();
        drop(open);

        let bytes = std::fs::read(scratch.dir().join(format!("{id}.session"))).unwrap();
        let envelope = encoding::decode_file(&bytes).unwrap();
        let inflated =
            encoding::gunzip(envelope.payload.unwrap(), encoding::DECOMPRESS_LIMIT).unwrap();

        assert_eq!(
            encoding::decode_contents(&inflated).unwrap(),
            copy().contents
        );
    }

    #[test]
    fn session_copy_over_the_limit_is_unusable() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(state_with_comment(), 110).unwrap();
        let mut oversized = copy();
        oversized.contents.insert(
            "big".to_string(),
            content(None, Some(&incompressible(1024))),
        );
        open.save_copy_with_limit(oversized, 200, 128).unwrap();
        let id = open.id().to_string();
        assert!(matches!(open.copy(), CopyState::Unusable(_)));
        drop(open);

        let stored = store.read(&id).unwrap();
        assert!(matches!(stored.copy, CopyState::Unusable(_)));
        assert!(matches!(
            store.open(&id),
            Err(SessionError::Unusable { .. })
        ));
    }

    fn incompressible(size: usize) -> Vec<u8> {
        let mut state = 0x1234_5678_9abc_def0u64;
        (0..size)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state & 0xff) as u8
            })
            .collect()
    }

    #[test]
    fn session_rejects_an_unknown_format_version() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        std::fs::create_dir_all(scratch.dir()).unwrap();
        let id = "01HF7YAT00BBBBBBBBBBBBBBBB";
        write_atomic(&scratch.dir(), id, &encoding::encode_file(99, b"{}", None)).unwrap();

        assert!(matches!(
            store.read(id),
            Err(SessionError::UnsupportedVersion { version: 99, .. })
        ));
    }

    #[test]
    fn session_file_is_not_left_empty() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(SessionState::default(), 200).unwrap();
        let id = open.id().to_string();

        assert!(matches!(
            store.read(&id),
            Err(SessionError::NotFound { .. })
        ));
    }

    #[test]
    fn session_without_state_and_an_unusable_copy_is_not_kept() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_copy_with_limit(copy(), 200, 1).unwrap();

        assert!(matches!(
            store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
            Err(SessionError::NotFound { .. })
        ));
    }

    #[test]
    fn session_atomic_replace_ignores_a_leftover_temporary() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
        let id = open.id().to_string();
        let temporary = scratch.dir().join(format!(".{id}.tmp"));
        std::fs::write(&temporary, b"part of the next write").unwrap();

        let stored = store.read(&id).unwrap();

        assert_eq!(stored.copy, CopyState::Ready(copy()));
        assert!(temporary.exists());
        open.save_state_at(state_with_comment(), 300).unwrap();
        assert!(!temporary.exists());
        assert_eq!(store.read(&id).unwrap().state, state_with_comment());
    }

    #[test]
    fn session_cleanup_keeps_the_newest_sessions() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        for (id, updated) in [
            ("01HF7YAT00AAAAAAAAAAAAAAAA", 100),
            ("01HF7YAT00BBBBBBBBBBBBBBBB", 200),
            ("01HF7YAT00CCCCCCCCCCCCCCCC", 300),
        ] {
            let mut open = store.create(info(id, updated)).unwrap();
            open.save_state_at(state_with_comment(), updated).unwrap();
            drop(open);
        }

        cleanup(&scratch.dir(), 2, u64::MAX);

        assert!(matches!(
            store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
            Err(SessionError::NotFound { .. })
        ));
        assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
        assert!(store.read("01HF7YAT00CCCCCCCCCCCCCCCC").is_ok());
    }

    #[test]
    fn session_cleanup_keeps_the_total_size() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        // 古い方が大きく、新しい方が小さい。合計の上限で古い方だけが消える。
        let mut old = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        let mut large = copy();
        large.contents.insert(
            "big".to_string(),
            content(None, Some(&incompressible(20_000))),
        );
        old.save_copy_with_limit(large, 100, COPY_LIMIT).unwrap();
        drop(old);
        let mut newest = store
            .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
            .unwrap();
        newest.save_state_at(state_with_comment(), 200).unwrap();
        drop(newest);
        let newest_size =
            std::fs::metadata(scratch.dir().join("01HF7YAT00BBBBBBBBBBBBBBBB.session"))
                .unwrap()
                .len();

        cleanup(&scratch.dir(), KEEP_SESSIONS, newest_size + 10);

        assert!(matches!(
            store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
            Err(SessionError::NotFound { .. })
        ));
        assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
    }

    #[test]
    fn session_cleanup_keeps_a_locked_session_out_of_the_count() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut locked = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        locked
            .save_copy_with_limit(copy(), 100, COPY_LIMIT)
            .unwrap();
        let mut middle = store
            .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
            .unwrap();
        middle
            .save_copy_with_limit(copy(), 200, COPY_LIMIT)
            .unwrap();
        let mut newest = store
            .create(info("01HF7YAT00CCCCCCCCCCCCCCCC", 300))
            .unwrap();
        newest
            .save_copy_with_limit(copy(), 300, COPY_LIMIT)
            .unwrap();

        // 最古を開いたまま 2 件だけ残す。ロック中は消えず、件数にも数えないので、
        // 2 番目に古いものも残る。
        cleanup(&scratch.dir(), 2, u64::MAX);

        assert!(store.read("01HF7YAT00AAAAAAAAAAAAAAAA").is_ok());
        assert!(store.read("01HF7YAT00BBBBBBBBBBBBBBBB").is_ok());
        assert!(store.read("01HF7YAT00CCCCCCCCCCCCCCCC").is_ok());
    }

    #[test]
    fn session_lock_is_exclusive_until_dropped() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let first = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();

        let second = store.create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100));
        assert!(matches!(second, Err(SessionError::Locked { .. })));

        drop(first);
        assert!(store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .is_ok());
    }

    #[test]
    fn session_list_orders_by_updated_and_skips_unusable() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut older = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        older.save_state_at(state_with_comment(), 100).unwrap();
        older.save_copy_with_limit(copy(), 100, COPY_LIMIT).unwrap();
        let mut newer = store
            .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
            .unwrap();
        newer.save_state_at(state_with_comment(), 200).unwrap();
        newer.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
        let mut unusable = store
            .create(info("01HF7YAT00CCCCCCCCCCCCCCCC", 300))
            .unwrap();
        unusable.save_state_at(state_with_comment(), 300).unwrap();
        unusable.save_copy_with_limit(copy(), 300, 1).unwrap();

        let listed = store.list().unwrap();

        assert_eq!(
            listed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["01HF7YAT00BBBBBBBBBBBBBBBB", "01HF7YAT00AAAAAAAAAAAAAAAA"]
        );
        assert_eq!(listed[0].seen, 1);
        assert_eq!(listed[0].total_files, 2);
    }

    #[test]
    fn session_cleanup_removes_orphan_lock_files() {
        let scratch = Scratch::new();
        std::fs::create_dir_all(scratch.dir()).unwrap();
        let lock_path = scratch.dir().join("01HF7YAT00AAAAAAAAAAAAAAAA.lock");
        std::fs::write(&lock_path, b"").unwrap();

        // 新しすぎるロックは、始まったばかりのセッションと競合しないよう残す。
        prune_orphan_locks(&scratch.dir(), Duration::from_secs(60));
        assert!(lock_path.exists());

        // ロック中のものは他のプロセスのものなので残す。
        let held = SessionLock::acquire(&scratch.dir(), "01HF7YAT00AAAAAAAAAAAAAAAA").unwrap();
        prune_orphan_locks(&scratch.dir(), Duration::ZERO);
        assert!(lock_path.exists());
        drop(held);

        // 使われていない古いロックは消える。
        std::fs::write(&lock_path, b"").unwrap();
        prune_orphan_locks(&scratch.dir(), Duration::ZERO);
        assert!(!lock_path.exists());
    }

    #[test]
    fn session_delete_removes_the_file() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
        let id = open.id().to_string();

        open.delete().unwrap();

        assert!(matches!(
            store.read(&id),
            Err(SessionError::NotFound { .. })
        ));
    }

    #[test]
    fn session_open_rejects_an_id_that_is_not_a_ulid_and_keeps_the_lock_alone() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        std::fs::create_dir_all(&scratch.0).unwrap();
        let sentinel = scratch.0.join("sentinel.lock");
        std::fs::write(&sentinel, b"keep").unwrap();

        let result = store.open("../sentinel");

        assert!(matches!(result, Err(SessionError::NotFound { .. })));
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
    }

    #[test]
    fn session_read_rejects_an_id_that_is_not_a_ulid() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(state_with_comment(), 100).unwrap();
        let id = open.id().to_string();
        drop(open);
        let outside = scratch.0.join("sentinel.session");
        std::fs::copy(scratch.dir().join(format!("{id}.session")), &outside).unwrap();

        let result = store.read("../sentinel");

        assert!(matches!(result, Err(SessionError::NotFound { .. })));
        assert!(outside.exists());
    }

    #[test]
    fn session_delete_rejects_an_id_that_is_not_a_ulid() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        std::fs::create_dir_all(scratch.dir()).unwrap();
        let outside = scratch.0.join("sentinel.session");
        std::fs::write(&outside, b"keep").unwrap();

        let result = store.delete("../sentinel");

        assert!(matches!(result, Err(SessionError::NotFound { .. })));
        assert!(outside.exists());
    }

    #[test]
    fn session_is_not_recreated_by_a_copy_saved_after_delete() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(state_with_comment(), 200).unwrap();
        open.delete().unwrap();

        // submit と競争した凍結が、消えたあとに写しを保存する。
        open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();

        assert!(matches!(
            store.read("01HF7YAT00AAAAAAAAAAAAAAAA"),
            Err(SessionError::NotFound { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn session_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
        let id = open.id().to_string();

        let dir_mode = std::fs::metadata(scratch.dir())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = std::fs::metadata(scratch.dir().join(format!("{id}.session")))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }
}
