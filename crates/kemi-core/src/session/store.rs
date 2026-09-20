//! セッションファイルの保存・読み出し・掃除・ロック（R-SESSION）。
//!
//! 1 セッションは `<id>.session`（情報と状態）と `<id>.payload`（写し）の 2 つ。
//! どちらの書き込みも一時ファイルから rename する原子的な差し替え。2 つを同時に
//! 差し替える方法は無いので、書く順序で不変条件を保つ。写しを保存するときは
//! `<id>.payload` を先に書き、写しを捨てるときとセッションを消すときは
//! `<id>.session` を先にする。
//!
//! 一時ファイルの名前は `.session` でも `.payload` でも終わらせない。掃除は名前の
//! 接尾辞で孤児を見分けるので、書き込み中のものが候補に見えてしまう。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::encoding;
use super::{CopyMeta, CopyState, MetaDto, SessionCopy, SessionInfo, SessionState, SessionSummary};

/// 写しの上限（`<id>.payload` 全体のバイト数）。超えた写しは捨てて復元不可にする。
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
            deleted: false,
            _lock: lock,
        })
    }

    /// 復元できるセッションを開く。ロックが取れない・読めない・写しが無いときは理由を返す。
    pub fn open(&self, id: &str) -> Result<OpenSession, SessionError> {
        checked_id(id)?;
        let lock = SessionLock::acquire(&self.dir, id)?;
        let meta = self.read_meta_of(id)?;
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
            CopyMeta::Ready => CopyState::Ready(self.read_copy(id)?),
        };
        Ok(OpenSession {
            dir: self.dir.clone(),
            info,
            state,
            copy,
            deleted: false,
            _lock: lock,
        })
    }

    /// 保存済みのセッションを、ロックを取らずに読む。
    pub fn read(&self, id: &str) -> Result<StoredSession, SessionError> {
        checked_id(id)?;
        let meta = self.read_meta_of(id)?;
        let path = self.path(id);
        let (info, state, copy) = meta.into_parts().map_err(|reason| SessionError::Corrupt {
            path: path.clone(),
            reason,
        })?;
        let copy = match copy {
            CopyMeta::Pending => CopyState::Pending,
            CopyMeta::Unusable(reason) => CopyState::Unusable(reason),
            CopyMeta::Ready => match self.read_copy(id) {
                Ok(copy) => CopyState::Ready(copy),
                // 写しのファイルが無いセッションは「使えない」として読む（R-SESSION）。
                Err(SessionError::Unusable { reason, .. }) => CopyState::Unusable(reason),
                Err(error) => return Err(error),
            },
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
        let mut ids = Vec::new();
        let mut payloads = BTreeSet::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if let Some(id) = name.strip_suffix(".session") {
                ids.push(id.to_string());
            } else if let Some(id) = name.strip_suffix(".payload") {
                payloads.insert(id.to_string());
            }
        }
        let mut summaries = Vec::new();
        for id in ids {
            // 写しの状態が「使える」と書いてあっても `<id>.payload` が無ければ復元
            // できない（R-SESSION）。有無はディレクトリの一覧だけで決める。
            if !payloads.contains(&id) {
                continue;
            }
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
        // `<id>.session` を消してから `<id>.payload` を消す（R-SESSION の書く順序）。
        remove_if_present(&self.path(id))?;
        remove_if_present(&self.payload_path(id))
    }

    fn path(&self, id: &str) -> PathBuf {
        session_path(&self.dir, id)
    }

    fn payload_path(&self, id: &str) -> PathBuf {
        payload_path(&self.dir, id)
    }

    fn read_meta_of(&self, id: &str) -> Result<MetaDto, SessionError> {
        match read_meta(&self.path(id)) {
            Err(SessionError::NotFound { .. }) => {
                Err(SessionError::NotFound { id: id.to_string() })
            }
            other => other,
        }
    }

    /// `<id>.payload` を読んで写しに戻す。ファイルが無いときは「使えない」。
    fn read_copy(&self, id: &str) -> Result<SessionCopy, SessionError> {
        let path = self.payload_path(id);
        let payload = std::fs::read(&path).map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => SessionError::Unusable {
                id: id.to_string(),
                reason: "its copy file is missing".to_string(),
            },
            _ => SessionError::Io {
                path: path.clone(),
                source,
            },
        })?;
        let corrupt = |reason: String| SessionError::Corrupt {
            path: path.clone(),
            reason,
        };
        let body = encoding::gunzip(&payload, encoding::DECOMPRESS_LIMIT)
            .map_err(|error| corrupt(error.to_string()))?;
        super::decode_copy(&body).map_err(corrupt)
    }
}

fn session_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.session"))
}

fn payload_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.payload"))
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

/// 開いているセッション。ロックを保持し、状態か写しが変わるたびに書き直す。
pub struct OpenSession {
    dir: PathBuf,
    info: SessionInfo,
    state: SessionState,
    copy: CopyState,
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
        // `<id>.session` を消してから `<id>.payload` を消す（R-SESSION の書く順序）。
        self.remove_file()?;
        self.remove_payload_file()
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
        let body = super::encode_copy(&copy).map_err(|reason| SessionError::Encode { reason })?;
        let compressed = encoding::gzip(&body);
        self.info.updated = now;
        if compressed.len() as u64 > limit {
            self.copy = CopyState::Unusable("the review copy is larger than 20 MB".to_string());
            return self.persist_without_copy();
        }
        self.copy = CopyState::Ready(copy);
        self.persist_with_copy(&compressed)
    }

    pub(crate) fn mark_unresumable_at(
        &mut self,
        reason: &str,
        now: u128,
    ) -> Result<(), SessionError> {
        self.copy = CopyState::Unusable(reason.to_string());
        self.info.updated = now;
        self.persist_without_copy()
    }

    fn path(&self) -> PathBuf {
        session_path(&self.dir, &self.info.id)
    }

    fn remove_file(&self) -> Result<(), SessionError> {
        remove_if_present(&self.path())
    }

    fn remove_payload_file(&self) -> Result<(), SessionError> {
        remove_if_present(&payload_path(&self.dir, &self.info.id))
    }

    fn has_copy(&self) -> bool {
        matches!(self.copy, CopyState::Ready(_))
    }

    /// `<id>.session` だけを書く。写しのファイルは読みも書きもしないので、状態を
    /// 変える操作の費用が写しの大きさに左右されない（R-SESSION）。
    fn persist(&mut self) -> Result<(), SessionError> {
        if self.deleted {
            return Ok(());
        }
        // 写しを作れなかった理由を残すためでも、状態が空ならセッションは残さない
        // （R-SESSION）。
        if self.state.is_empty() && !self.has_copy() {
            return self.remove_file();
        }
        self.write_session()?;
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(())
    }

    /// 写しを保存する。`<id>.payload` を書いてから `<id>.session` を書く。途中で
    /// 止まっても、残るのはどの `<id>.session` からも「使える」と指されていない
    /// `<id>.payload` だけになる（R-SESSION の書く順序）。
    fn persist_with_copy(&mut self, payload: &[u8]) -> Result<(), SessionError> {
        if self.deleted {
            return Ok(());
        }
        write_atomic(&self.dir, &payload_path(&self.dir, &self.info.id), payload)?;
        self.write_session()?;
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(())
    }

    /// 写しを捨てた後のファイルをそろえる。`<id>.session` を書いて（残すものが無ければ
    /// 消して）から `<id>.payload` を消す。掃除を待たない（R-SESSION の書く順序）。
    fn persist_without_copy(&mut self) -> Result<(), SessionError> {
        if self.deleted {
            return Ok(());
        }
        self.persist()?;
        self.remove_payload_file()
    }

    fn write_session(&self) -> Result<(), SessionError> {
        let meta = MetaDto::from_parts(&self.info, &self.state, &self.copy);
        let meta = serde_json::to_vec(&meta).map_err(|error| SessionError::Encode {
            reason: error.to_string(),
        })?;
        let bytes = encoding::encode_session(encoding::VERSION, &meta);
        write_atomic(&self.dir, &self.path(), &bytes)
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

/// 一時ファイルへ書いてから rename する。一時ファイルの名前は、掃除が見る接尾辞
/// （`.session` と `.payload`）で終わらせない。
fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), SessionError> {
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

/// あれば消す。無いのは成功と同じに扱う。
fn remove_if_present(path: &Path) -> Result<(), SessionError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SessionError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// 上限を超えた分を最終更新の古い順に消す。ロック中のセッションは消さず、件数とバイト数にも
/// 数えない。復元できないセッションも数える。1 セッションの大きさは `<id>.session` と
/// `<id>.payload` の合算（R-SESSION）。
pub(crate) fn cleanup(dir: &Path, keep_count: usize, keep_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // 振り分けは名前の接尾辞だけで行う。どのファイルが孤児かを決めるのに
    // `<id>.session` の中身は読まない（R-SESSION）。
    let mut sessions: BTreeMap<String, u64> = BTreeMap::new();
    let mut payloads: BTreeMap<String, u64> = BTreeMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        if let Some(id) = name.strip_suffix(".session") {
            sessions.insert(id.to_string(), size);
        } else if let Some(id) = name.strip_suffix(".payload") {
            payloads.insert(id.to_string(), size);
        }
    }
    // 孤児のロックと写しは、セッションを消すかどうかと関係なく毎回片付ける。
    prune_orphan_locks(dir, Duration::from_secs(60));
    for id in payloads.keys().filter(|id| !sessions.contains_key(*id)) {
        // 起動中のレビューは `<id>.payload` を書いてから `<id>.session` を書くので、
        // その間だけこの形になる。ロックを持っているものは消さない（R-SESSION）。
        if is_locked(dir, id) {
            continue;
        }
        let _ = std::fs::remove_file(payload_path(dir, id));
    }
    let total: u64 = sessions
        .iter()
        .map(|(id, size)| size + payloads.get(id).copied().unwrap_or(0))
        .sum();
    if sessions.len() <= keep_count && total <= keep_bytes {
        return;
    }
    // 最終更新は並べ替えにしか使わないので、消すものがあると分かってから読む。
    let mut sessions: Vec<(u128, String, u64)> = sessions
        .into_iter()
        .map(|(id, size)| {
            let updated = read_updated(&session_path(dir, &id)).unwrap_or(0);
            let size = size + payloads.get(&id).copied().unwrap_or(0);
            (updated, id, size)
        })
        .collect();
    sessions.sort_by(|left, right| (right.0, &right.1).cmp(&(left.0, &left.1)));
    let mut kept = 0usize;
    let mut bytes = 0u64;
    for (_, id, size) in sessions {
        if is_locked(dir, &id) {
            continue;
        }
        if kept < keep_count && bytes + size <= keep_bytes {
            kept += 1;
            bytes += size;
        } else {
            // `<id>.session` を消してから `<id>.payload` を消す（R-SESSION の書く順序）。
            let _ = std::fs::remove_file(session_path(dir, &id));
            let _ = std::fs::remove_file(payload_path(dir, &id));
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

/// `<id>.session` を読む。写しは別のファイルなので、ここで読むのは情報と状態だけ
/// （一覧と掃除は写しを読まない）。
fn read_meta(path: &Path) -> Result<MetaDto, SessionError> {
    let bytes = std::fs::read(path).map_err(|source| match source.kind() {
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
    let (version, meta) =
        encoding::decode_session(&bytes).map_err(|error| corrupt(error.to_string()))?;
    if version != encoding::VERSION {
        return Err(SessionError::UnsupportedVersion {
            path: path.to_path_buf(),
            version,
        });
    }
    serde_json::from_slice(meta).map_err(|error| corrupt(error.to_string()))
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
        assert!(scratch.dir().join(format!("{id}.session")).exists());
        assert!(scratch.dir().join(format!("{id}.payload")).exists());
    }

    #[test]
    fn session_file_holds_no_frozen_review_paths() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        // 状態のどのコメントも指していないパスだけを写しに入れる。
        let mut only_in_the_copy = copy();
        only_in_the_copy.units[0].review.groups[0].files[0].path =
            "src/needle_marker.rs".to_string();
        open.save_state_at(state_with_comment(), 200).unwrap();
        open.save_copy_with_limit(only_in_the_copy, 300, COPY_LIMIT)
            .unwrap();
        let id = open.id().to_string();
        drop(open);

        let bytes = std::fs::read(scratch.dir().join(format!("{id}.session"))).unwrap();

        assert!(
            !bytes
                .windows(b"needle_marker".len())
                .any(|window| window == b"needle_marker"),
            "the frozen review paths must not be in the session file"
        );
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
    fn session_without_its_copy_file_is_not_listed_and_cannot_be_opened() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_state_at(state_with_comment(), 200).unwrap();
        open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();
        let id = open.id().to_string();
        drop(open);
        std::fs::remove_file(scratch.dir().join(format!("{id}.payload"))).unwrap();

        assert!(store.list().unwrap().is_empty());
        assert!(matches!(
            store.open(&id),
            Err(SessionError::Unusable { .. })
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

        let bytes = std::fs::read(scratch.dir().join(format!("{id}.payload"))).unwrap();
        // 写しのファイルは全体が 1 つの gzip で、目印も版も持たない。
        assert_eq!(&bytes[..3], &[0x1f, 0x8b, 0x08]);
        let inflated = encoding::gunzip(&bytes, encoding::DECOMPRESS_LIMIT).unwrap();

        assert_eq!(super::super::decode_copy(&inflated).unwrap(), copy());
    }

    #[test]
    fn session_state_write_leaves_the_copy_file_untouched() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 100))
            .unwrap();
        open.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();
        let id = open.id().to_string();
        let payload = scratch.dir().join(format!("{id}.payload"));
        // gzip として読めないバイト列に置き換える。状態の保存がここを書き直すなら
        // 中身が変わり、読むなら失敗する。
        std::fs::write(&payload, b"not a gzip").unwrap();

        for updated in [300, 400, 500] {
            open.save_state_at(state_with_comment(), updated).unwrap();
        }

        assert_eq!(std::fs::read(&payload).unwrap(), b"not a gzip");
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
        // 上限は `<id>.payload` 全体で判定する（R-SESSION）。変更ファイルの内容だけで
        // ちょうど収まる上限を渡し、凍結したレビューのメタデータの分で超えさせる。
        let contents_only = encoding::gzip(&encoding::encode_contents(&oversized.contents)).len();
        open.save_copy_with_limit(oversized, 200, contents_only as u64)
            .unwrap();
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
        write_atomic(
            &scratch.dir(),
            &scratch.dir().join(format!("{id}.session")),
            &encoding::encode_session(99, b"{}"),
        )
        .unwrap();

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
        let temporary = scratch.dir().join(format!(".{id}.session.tmp"));
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
        let size_of = |name: &str| std::fs::metadata(scratch.dir().join(name)).unwrap().len();
        // `<id>.session` だけを数えるなら 2 件とも収まる上限。古い方の
        // `<id>.payload` を足して初めて超える（R-SESSION: 合計は 2 ファイルの合算）。
        let limit = size_of("01HF7YAT00AAAAAAAAAAAAAAAA.session")
            + size_of("01HF7YAT00BBBBBBBBBBBBBBBB.session")
            + 10;

        cleanup(&scratch.dir(), KEEP_SESSIONS, limit);

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
    fn session_cleanup_removes_a_payload_without_its_session() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let held = store
            .create(info("01HF7YAT00DDDDDDDDDDDDDDDD", 100))
            .unwrap();
        std::fs::write(
            scratch.dir().join("01HF7YAT00CCCCCCCCCCCCCCCC.payload"),
            b"orphan",
        )
        .unwrap();
        // 起動中のレビューは `<id>.payload` を書いてから `<id>.session` を書くので、
        // その間だけ同じ形になる。ロックを持っているものは消さない（R-SESSION）。
        let held_payload = scratch.dir().join("01HF7YAT00DDDDDDDDDDDDDDDD.payload");
        std::fs::write(&held_payload, b"being written").unwrap();

        let mut open = store
            .create(info("01HF7YAT00AAAAAAAAAAAAAAAA", 200))
            .unwrap();
        open.save_state_at(state_with_comment(), 200).unwrap();

        assert!(!scratch
            .dir()
            .join("01HF7YAT00CCCCCCCCCCCCCCCC.payload")
            .exists());
        assert!(held_payload.exists());
        drop(held);
    }

    #[test]
    fn session_cleanup_evicts_a_stale_pair_by_the_limits() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
        let mut stale = store.create(info(id, 100)).unwrap();
        stale.save_state_at(state_with_comment(), 100).unwrap();
        stale.save_copy_with_limit(copy(), 100, COPY_LIMIT).unwrap();
        let session = scratch.dir().join(format!("{id}.session"));
        let payload = scratch.dir().join(format!("{id}.payload"));
        // 強制終了で残りうる形。「使える」でない `<id>.session` と `<id>.payload` が
        // 揃っている（公開 API だけでは作れないので、写しを置き直して作る）。
        let written = std::fs::read(&payload).unwrap();
        stale.mark_unresumable_at("interrupted", 100).unwrap();
        std::fs::write(&payload, &written).unwrap();
        drop(stale);
        let mut newest = store
            .create(info("01HF7YAT00BBBBBBBBBBBBBBBB", 200))
            .unwrap();
        newest.save_state_at(state_with_comment(), 200).unwrap();
        drop(newest);

        cleanup(&scratch.dir(), 1, u64::MAX);

        assert!(!session.exists());
        assert!(!payload.exists());
    }

    #[test]
    fn session_dropping_the_copy_removes_the_copy_file() {
        let scratch = Scratch::new();
        let store = SessionStore::new(scratch.dir());
        let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
        let mut open = store.create(info(id, 100)).unwrap();
        open.save_state_at(state_with_comment(), 200).unwrap();
        open.save_copy_with_limit(copy(), 300, COPY_LIMIT).unwrap();

        open.mark_unresumable_at("cannot read the contents", 400)
            .unwrap();

        // 状態は残るので `<id>.session` は残り、写しのファイルだけが消える。
        assert!(scratch.dir().join(format!("{id}.session")).exists());
        assert!(!scratch.dir().join(format!("{id}.payload")).exists());

        // 状態が空のまま写しが使えなくなる枝では、セッションごと消えて写しも残らない。
        let other = "01HF7YAT00BBBBBBBBBBBBBBBB";
        let mut empty = store.create(info(other, 100)).unwrap();
        empty.save_copy_with_limit(copy(), 200, COPY_LIMIT).unwrap();

        empty
            .mark_unresumable_at("cannot read the contents", 300)
            .unwrap();

        assert!(!scratch.dir().join(format!("{other}.session")).exists());
        assert!(!scratch.dir().join(format!("{other}.payload")).exists());
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
        assert!(!scratch.dir().join(format!("{id}.payload")).exists());
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
        let mode_of = |name: String| {
            std::fs::metadata(scratch.dir().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };

        assert_eq!(dir_mode, 0o700);
        assert_eq!(mode_of(format!("{id}.session")), 0o600);
        assert_eq!(mode_of(format!("{id}.payload")), 0o600);
    }
}
