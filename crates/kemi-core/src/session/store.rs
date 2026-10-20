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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::encoding;
use super::files::{self, FILES_LIMIT, PageSnapshot};
use super::format::{CopyMeta, MetaDto};
use super::lock::{SessionLock, is_locked};
use super::storage_io::{
    create_private_dir, list_files, remove_dir_if_present, remove_if_present, write_atomic,
    write_private_file,
};
use super::{CopyState, SessionCopy, SessionInfo, SessionState, SessionSummary, checked_id};

/// 写しの上限（`<id>.payload` 全体のバイト数）。超えた写しは捨てて復元不可にする。
pub(crate) const COPY_LIMIT: u64 = 20 * 1024 * 1024;
/// 全ワークスペース合計で残すセッション数。
pub(crate) const KEEP_SESSIONS: usize = 100;
/// 全ワークスペース合計で残すバイト数。
pub(crate) const KEEP_BYTES: u64 = 500 * 1024 * 1024;
/// セッションのファイルが無いロックを孤児として片付けるまでの猶予。始まったばかりの
/// セッションは、ロックを取ってから最初の `<id>.session` を書くまでこの形になる。
const ORPHAN_LOCK_MIN_AGE: Duration = Duration::from_secs(60);

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
pub struct SavedSession {
    pub info: SessionInfo,
    pub state: SessionState,
    pub copy: CopyState,
}

/// ディレクトリ 1 つに置かれたセッション群。
#[derive(Clone, Debug)]
pub struct SessionStore {
    dir: PathBuf,
    /// 1 セッションの `<id>.files/` の上限（R-PAGE-SESSION）。
    files_limit: u64,
}

impl SessionStore {
    pub fn new(dir: PathBuf) -> Self {
        SessionStore {
            dir,
            files_limit: FILES_LIMIT,
        }
    }

    /// `<id>.files/` の上限を差し替える。既定は [`FILES_LIMIT`]。
    pub fn with_files_limit(self, files_limit: u64) -> Self {
        SessionStore {
            files_limit,
            ..self
        }
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
            lifecycle: Lifecycle::Active,
            files_limit: self.files_limit,
            _lock: lock,
        })
    }

    /// 復元できるセッションを開く。ロックが取れない・読めない・写しが無いときは理由を返す。
    pub fn open(&self, id: &str) -> Result<OpenSession, SessionError> {
        checked_id(id)?;
        let lock = SessionLock::acquire(&self.dir, id)?;
        let (info, state, copy) = self.read_parts(id)?;
        let copy = match copy {
            // `--live` のセッションは写しを持たない（R-PAGE-SESSION）。
            CopyMeta::Pending if info.mode.is_live() => CopyState::Pending,
            CopyMeta::Pending => return Err(SessionError::NotReady { id: id.to_string() }),
            CopyMeta::Unusable(reason) => {
                return Err(SessionError::Unusable {
                    id: id.to_string(),
                    reason,
                });
            }
            CopyMeta::Ready => CopyState::Ready(self.read_copy(id)?),
        };
        Ok(OpenSession {
            dir: self.dir.clone(),
            info,
            state,
            copy,
            lifecycle: Lifecycle::Active,
            files_limit: self.files_limit,
            _lock: lock,
        })
    }

    /// 保存済みのセッションを、ロックを取らずに読む。
    pub fn read(&self, id: &str) -> Result<SavedSession, SessionError> {
        checked_id(id)?;
        let (info, state, copy) = self.read_parts(id)?;
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
        Ok(SavedSession { info, state, copy })
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
                });
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
            let Ok(meta) = read_meta(&self.path(&id)) else {
                continue;
            };
            if !meta.is_resumable() {
                continue;
            }
            // 写しの状態が「使える」と書いてあっても `<id>.payload` が無ければ復元
            // できない（R-SESSION）。`--live` は写しを持たない（R-PAGE-SESSION）。
            let (info, seen) = meta.summary();
            if !info.mode.is_live() && !payloads.contains(&id) {
                continue;
            }
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

    /// 読めない版の `<id>.session` があるか（R-SESSION）。復元できるセッションが
    /// 1 つも無いとき、その理由が「読めない版しか無い」ことかを見分けるために使う。
    /// 版は先頭の固定長だけで分かるので、読み手の無い版のファイルを丸ごと読まない。
    pub fn has_unreadable_version(&self) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return false;
            };
            name.ends_with(".session")
                && read_version(&entry.path())
                    .is_some_and(|version| !encoding::is_readable(version))
        })
    }

    /// そのセッションのレビューが今動いているか（ロックが生きているか）。`<id>.endpoint` が
    /// 強制終了で残っていても、ロックが死んでいれば無いものとして扱うために使う
    /// （R-AGENT-LINK）。
    pub fn is_running(&self, id: &str) -> bool {
        checked_id(id).is_ok() && is_locked(&self.dir, id)
    }

    fn path(&self, id: &str) -> PathBuf {
        session_path(&self.dir, id)
    }

    fn payload_path(&self, id: &str) -> PathBuf {
        payload_path(&self.dir, id)
    }

    /// `<id>.session` を読み、情報・状態・写しの状態に分ける。
    fn read_parts(&self, id: &str) -> Result<(SessionInfo, SessionState, CopyMeta), SessionError> {
        let path = self.path(id);
        let meta = match read_meta(&path) {
            Err(SessionError::NotFound { .. }) => {
                return Err(SessionError::NotFound { id: id.to_string() });
            }
            other => other?,
        };
        meta.into_parts()
            .map_err(|reason| SessionError::Corrupt { path, reason })
    }

    /// `<id>.payload` を読んで写しに戻す。ファイルが無いときは「使えない」。
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "std::io::ErrorKind は non_exhaustive で、列挙しきれない"
    )]
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Active,
    Suspended,
    Deleted,
}

/// 開いているセッション。ロックを保持し、状態か写しが変わるたびに書き直す。
pub struct OpenSession {
    dir: PathBuf,
    info: SessionInfo,
    state: SessionState,
    copy: CopyState,
    /// 停止後に届く保存で、submit や保留が片付けたファイルを作り直さない。
    lifecycle: Lifecycle,
    files_limit: u64,
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

    /// 復元の対象になるか。写しが完成しているか、`--live` なら残すべき会話があるか
    /// （R-PAGE-SESSION）。
    pub fn is_resumable(&self) -> bool {
        if self.info.mode.is_live() {
            return self.lifecycle != Lifecycle::Deleted && self.state.has_conversation();
        }
        matches!(self.copy, CopyState::Ready(_))
    }

    /// 状態を差し替えて保存する。空の状態で写しも無ければ、セッションは残さない。
    pub fn save_state(&mut self, state: SessionState) -> Result<(), SessionError> {
        self.save_state_at(state, super::now_millis())
    }

    pub fn save_issued_state(&mut self, state: SessionState) -> Result<(), SessionError> {
        self.save_atomic_state(state, self.info.updated)
    }

    pub fn save_explanation_state(&mut self, state: SessionState) -> Result<(), SessionError> {
        self.save_explanation_state_at(state, super::now_millis())
    }

    pub(crate) fn save_explanation_state_at(
        &mut self,
        state: SessionState,
        now: u128,
    ) -> Result<(), SessionError> {
        super::validate_additional(&state)?;
        self.save_atomic_state(state, now)
    }

    fn save_atomic_state(
        &mut self,
        state: SessionState,
        updated: u128,
    ) -> Result<(), SessionError> {
        self.ensure_active()?;
        let display = state.display.as_ref().ok_or_else(|| SessionError::Encode {
            reason: "diff_display_version: missing generation state".into(),
        })?;
        display
            .validate(&self.info.id)
            .map_err(|e| SessionError::Encode {
                reason: e.to_string(),
            })?;
        let mut info = self.info.clone();
        info.updated = updated;
        self.write_session(&info, &state, &self.copy)?;
        self.state = state;
        self.info = info;
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(())
    }

    pub fn save_frozen_copy(
        &mut self,
        prepared: super::PreparedCopy,
        mut state: SessionState,
    ) -> Result<bool, SessionError> {
        self.ensure_active()?;
        let ready = prepared.compressed.len() as u64 <= COPY_LIMIT;
        let copy = if ready {
            write_atomic(
                &self.dir,
                &payload_path(&self.dir, &self.info.id),
                &prepared.compressed,
            )?;
            CopyState::Ready(prepared.copy)
        } else {
            if let Some(display) = state.display.as_mut() {
                display.frozen = None;
            }
            CopyState::Unusable("the review copy is larger than 20 MB".into())
        };
        self.write_session(&self.info, &state, &copy)?;
        self.state = state;
        self.copy = copy;
        if !ready {
            self.remove_payload_file()?;
        }
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(ready)
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
        self.lifecycle = Lifecycle::Deleted;
        self.remove_session_and_copy()?;
        remove_dir_if_present(&files_path(&self.dir, &self.info.id))?;
        remove_if_present(&super::endpoint_path(&self.dir, &self.info.id))
    }

    /// スナップショットを `<id>.files/` に書く。書く前に 20 MB の規則で場所を空ける
    /// （R-PAGE-SESSION）。`comments` は今あるコメントの id で、その画像は消さない。
    pub fn save_snapshot(
        &self,
        snapshot: &PageSnapshot,
        comments: &BTreeSet<String>,
    ) -> FilesWrite {
        match files::encode_snapshot(snapshot) {
            Ok(bytes) => self.write_into_files(
                &files::snapshot_name(snapshot.number, snapshot.kind),
                &bytes,
                comments,
            ),
            Err(reason) => FilesWrite {
                written: FileWritten::Failed(SessionError::Encode { reason }),
                removed_snapshots: Vec::new(),
            },
        }
    }

    /// コメントの画像を `<id>.files/` に書く。規則はスナップショットと同じで、書いている
    /// 画像のコメントは `comments` に無くても今あるものとして扱う（状態に入る前に書くため）。
    pub fn save_image(
        &self,
        comment_id: &str,
        bytes: &[u8],
        comments: &BTreeSet<String>,
    ) -> FilesWrite {
        let mut kept = comments.clone();
        kept.insert(comment_id.to_string());
        self.write_into_files(&files::image_name(comment_id), bytes, &kept)
    }

    /// `<id>.files/` に置いたスナップショットを、取った順に読み戻す。読めないものは
    /// そのファイルの誤りとして返し、ほかのものは読む。
    pub fn read_snapshots(&self) -> Vec<Result<PageSnapshot, SessionError>> {
        let dir = files_path(&self.dir, &self.info.id);
        let listed = match list_files(&dir) {
            Ok(listed) => listed,
            Err(error) => return vec![Err(error)],
        };
        let mut names: Vec<(u32, String)> = listed
            .into_iter()
            .filter_map(|(name, _)| Some((files::snapshot_of_name(&name)?.0, name)))
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|(_, name)| {
                let path = dir.join(&name);
                let bytes = std::fs::read(&path).map_err(|source| SessionError::Io {
                    path: path.clone(),
                    source,
                })?;
                files::decode_snapshot(&name, &bytes)
                    .map_err(|reason| SessionError::Corrupt { path, reason })
            })
            .collect()
    }

    /// `<id>.files/` にあるスナップショットのファイルの最大の番号。読めないファイルも数える
    /// （復元後に振る番号が、残っているファイルと重ならないように）。無ければ 0。
    pub fn last_snapshot_number(&self) -> u32 {
        list_files(&files_path(&self.dir, &self.info.id))
            .unwrap_or_default()
            .iter()
            .filter_map(|(name, _)| Some(files::snapshot_of_name(name)?.0))
            .max()
            .unwrap_or(0)
    }

    /// 保留で終わるとき。会話の無い `--live` のセッションは `<id>.files/` も残さない
    /// （R-PAGE-SESSION）。レビューの途中では会話が無くなっても `<id>.files/` を消さない
    /// （開始時のスナップショットを、後で会話ができたときのために持っておく）。
    /// 片付けた後は `<id>.files/` に書かない。
    pub fn close_suspended(&mut self) -> Result<(), SessionError> {
        if self.lifecycle == Lifecycle::Deleted {
            return Ok(());
        }
        self.lifecycle = Lifecycle::Suspended;
        if self.info.mode.is_live() && !self.state.has_conversation() {
            self.remove_session_and_copy()?;
            return remove_dir_if_present(&files_path(&self.dir, &self.info.id));
        }
        if !self.info.mode.is_live() && self.state.is_empty() && !self.has_copy() {
            self.remove_session_and_copy()?;
        }
        Ok(())
    }

    fn write_into_files(
        &self,
        name: &str,
        bytes: &[u8],
        comments: &BTreeSet<String>,
    ) -> FilesWrite {
        let dir = files_path(&self.dir, &self.info.id);
        let failed = |error, removed_snapshots| FilesWrite {
            written: FileWritten::Failed(error),
            removed_snapshots,
        };
        if self.lifecycle != Lifecycle::Active {
            let reason = match self.lifecycle {
                Lifecycle::Deleted => "the session is already deleted",
                Lifecycle::Suspended => "the session is already suspended",
                Lifecycle::Active => unreachable!("active sessions continue writing below"),
            };
            return failed(
                SessionError::Io {
                    path: dir,
                    source: std::io::Error::other(reason),
                },
                Vec::new(),
            );
        }
        let listed = match list_files(&dir) {
            // 同じ名前のファイルは書き直すので、今の大きさに数えない。
            Ok(listed) => listed
                .into_iter()
                .filter(|(other, _)| other != name)
                .collect::<Vec<_>>(),
            Err(error) => return failed(error, Vec::new()),
        };
        let room = files::make_room(&listed, comments, bytes.len() as u64, self.files_limit);
        let mut removed_snapshots = Vec::new();
        for removed in &room.remove {
            if let Err(error) = remove_if_present(&dir.join(removed)) {
                return failed(error, removed_snapshots);
            }
            if let Some((number, _)) = files::snapshot_of_name(removed) {
                removed_snapshots.push(number);
            }
        }
        if !room.fits {
            return FilesWrite {
                written: FileWritten::NoRoom,
                removed_snapshots,
            };
        }
        let written = create_private_dir(&dir).and_then(|()| {
            let path = std::path::absolute(dir.join(name)).map_err(|source| SessionError::Io {
                path: dir.join(name),
                source,
            })?;
            write_private_file(&path, bytes).map_err(|source| SessionError::Io {
                path: path.clone(),
                source,
            })?;
            Ok(path)
        });
        FilesWrite {
            written: match written {
                Ok(path) => FileWritten::Saved(path),
                Err(error) => FileWritten::Failed(error),
            },
            removed_snapshots,
        }
    }

    pub(crate) fn save_state_at(
        &mut self,
        state: SessionState,
        now: u128,
    ) -> Result<(), SessionError> {
        super::validate_additional(&state)?;
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
        self.persist_with_copy(copy, &compressed)
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

    /// `<id>.session` を消してから `<id>.payload` を消す（R-SESSION の書く順序）。
    fn remove_session_and_copy(&self) -> Result<(), SessionError> {
        self.remove_file()?;
        self.remove_payload_file()
    }

    /// 停止の後に届いた保存を断る。submit や保留が片付けたファイルを作り直さない。
    fn ensure_active(&self) -> Result<(), SessionError> {
        if self.lifecycle == Lifecycle::Active {
            return Ok(());
        }
        Err(SessionError::Unusable {
            id: self.info.id.clone(),
            reason: "the review is stopping".into(),
        })
    }

    fn has_copy(&self) -> bool {
        matches!(self.copy, CopyState::Ready(_))
    }

    /// `<id>.session` だけを書く。写しのファイルは読みも書きもしないので、状態を
    /// 変える操作の費用が写しの大きさに左右されない（R-SESSION）。
    fn persist(&mut self) -> Result<(), SessionError> {
        if self.lifecycle == Lifecycle::Deleted {
            return Ok(());
        }
        // 写しを作れなかった理由を残すためでも、状態が空ならセッションは残さない
        // （R-SESSION）。
        let keep = if self.state.display.is_some() && self.lifecycle == Lifecycle::Active {
            true
        } else if self.info.mode.is_live() {
            // `--live` は見た・折りたたみだけでは残さない（R-PAGE-SESSION）。
            self.state.has_conversation()
        } else {
            !self.state.is_empty() || self.has_copy()
        };
        if !keep {
            // `<id>.files/` は残す。`--live` のレビューは会話が無い間もスナップショットを
            // 書いており、保留で終わるときに消すかを決める（`close_suspended`）。
            return self.remove_file();
        }
        self.write_session(&self.info, &self.state, &self.copy)?;
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(())
    }

    /// 写しを保存する。`<id>.payload` を書いてから `<id>.session` を書く。途中で
    /// 止まっても、残るのはどの `<id>.session` からも「使える」と指されていない
    /// `<id>.payload` だけになる（R-SESSION の書く順序）。
    ///
    /// 写しを「使える」にするのも `<id>.payload` を置けてからにする。書けなかったのに
    /// 「使える」と覚えると、次に状態を書いたとき写しの無い `<id>.session` が
    /// 「使える」と主張してしまう。
    fn persist_with_copy(&mut self, copy: SessionCopy, payload: &[u8]) -> Result<(), SessionError> {
        if self.lifecycle == Lifecycle::Deleted {
            self.copy = CopyState::Ready(copy);
            return Ok(());
        }
        write_atomic(&self.dir, &payload_path(&self.dir, &self.info.id), payload)?;
        self.copy = CopyState::Ready(copy);
        self.write_session(&self.info, &self.state, &self.copy)?;
        cleanup(&self.dir, KEEP_SESSIONS, KEEP_BYTES);
        Ok(())
    }

    /// 写しを捨てた後のファイルをそろえる。`<id>.session` を書いて（残すものが無ければ
    /// 消して）から `<id>.payload` を消す。掃除を待たない（R-SESSION の書く順序）。
    fn persist_without_copy(&mut self) -> Result<(), SessionError> {
        if self.lifecycle == Lifecycle::Deleted {
            return Ok(());
        }
        self.persist()?;
        self.remove_payload_file()
    }

    /// `<id>.session` を書く。`info` の id はこのセッションのもの。
    fn write_session(
        &self,
        info: &SessionInfo,
        state: &SessionState,
        copy: &CopyState,
    ) -> Result<(), SessionError> {
        let meta = MetaDto::from_parts(info, state, copy);
        let meta = serde_json::to_vec(&meta).map_err(|error| SessionError::Encode {
            reason: error.to_string(),
        })?;
        let bytes = encoding::encode_session(encoding::VERSION, &meta);
        write_atomic(&self.dir, &self.path(), &bytes)
    }
}

/// `<id>.files/` に書いた結果（R-PAGE-SESSION）。
#[derive(Debug)]
pub struct FilesWrite {
    pub written: FileWritten,
    /// 20 MB の規則で消したスナップショットの番号。書けなかったときも、消したものは返す。
    pub removed_snapshots: Vec<u32>,
}

#[derive(Debug)]
pub enum FileWritten {
    /// 書いたファイルの絶対パス。
    Saved(PathBuf),
    /// 20 MB の規則で消せるものを消しても入らないので、書かなかった。
    NoRoom,
    Failed(SessionError),
}

/// セッションに添えたファイルの置き場所（`<id>.files/`）。
fn files_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.files"))
}

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
    // 振り分けは名前の接尾辞だけで行う。どのファイルが孤児かを決めるのに
    // `<id>.session` の中身は読まない（R-SESSION）。
    let mut sessions: BTreeMap<String, u64> = BTreeMap::new();
    let mut payloads: BTreeMap<String, u64> = BTreeMap::new();
    let mut files: BTreeMap<String, u64> = BTreeMap::new();
    let mut endpoints: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        if let Some(id) = name.strip_suffix(".files") {
            // `<id>.files/` は 1 段なので、中の一覧と大きさだけを読む。
            let size = list_files(&entry.path())
                .map(|listed| listed.iter().map(|(_, size)| size).sum())
                .unwrap_or(0);
            files.insert(id.to_string(), size);
        } else if let Some(id) = name.strip_suffix(".session") {
            sessions.insert(id.to_string(), size);
        } else if let Some(id) = name.strip_suffix(".payload") {
            payloads.insert(id.to_string(), size);
        } else if let Some(id) = name.strip_suffix(".endpoint") {
            endpoints.push(id.to_string());
        }
    }
    // 動いていないレビューのつなぎ先は、強制終了で残ったもの。消したセッションのものも
    // 含め、ロックの生死だけで片付ける（R-AGENT-LINK）。
    for id in endpoints.iter().filter(|id| !is_locked(dir, id)) {
        let _ = std::fs::remove_file(super::endpoint_path(dir, id));
    }
    // 孤児のロックと写しは、セッションを消すかどうかと関係なく毎回片付ける。
    prune_orphan_locks(dir, now, ORPHAN_LOCK_MIN_AGE);
    for id in payloads.keys().filter(|id| !sessions.contains_key(*id)) {
        // 起動中のレビューは `<id>.payload` を書いてから `<id>.session` を書くので、
        // その間だけこの形になる。ロックを持っているものは消さない（R-SESSION）。
        if is_locked(dir, id) {
            continue;
        }
        let _ = std::fs::remove_file(payload_path(dir, id));
    }
    // 強制終了で残った `<id>.files/` も同じ形で回収する。起動中の `--live` のレビューは
    // 会話が無い間も書くが、ロックを持っているので消さない（R-PAGE-SESSION）。
    for id in files.keys().filter(|id| !sessions.contains_key(*id)) {
        if is_locked(dir, id) {
            continue;
        }
        let _ = std::fs::remove_dir_all(files_path(dir, id));
    }
    let size_of = |id: &str, session: u64| {
        session + payloads.get(id).copied().unwrap_or(0) + files.get(id).copied().unwrap_or(0)
    };
    let total: u64 = sessions.iter().map(|(id, size)| size_of(id, *size)).sum();
    if sessions.len() <= keep_count && total <= keep_bytes {
        return;
    }
    // 最終更新は並べ替えにしか使わないので、消すものがあると分かってから読む。
    let mut sessions: Vec<(u128, String, u64)> = sessions
        .into_iter()
        .map(|(id, size)| {
            let updated = read_updated(&session_path(dir, &id)).unwrap_or(0);
            let size = size_of(&id, size);
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
            let _ = std::fs::remove_dir_all(files_path(dir, &id));
        }
    }
}

/// セッションのファイルが無いまま残ったロックファイルを片付ける。始まったばかりの
/// セッションと競合しないよう、`min_age` より新しいものは触らない。ロック中のものは
/// 他プロセスが使っているので残す。
fn prune_orphan_locks(dir: &Path, now: SystemTime, min_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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

/// `<id>.session` の先頭だけを読んで版を返す。開けないファイルと、セッションの
/// 形をしていないファイルは、版が分からないものとして扱う。
fn read_version(path: &Path) -> Option<u8> {
    let mut head = [0u8; encoding::HEAD_LEN];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    encoding::decode_version(&head).ok()
}

/// `<id>.session` を読む。写しは別のファイルなので、ここで読むのは情報と状態だけ
/// （一覧と掃除は写しを読まない）。
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind は non_exhaustive で、列挙しきれない"
)]
fn read_meta(path: &Path) -> Result<MetaDto<'static>, SessionError> {
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
    if !encoding::is_readable(version) {
        return Err(SessionError::UnsupportedVersion {
            path: path.to_path_buf(),
            version,
        });
    }
    MetaDto::decode(version, meta).map_err(corrupt)
}

#[cfg(test)]
#[path = "store/tests.rs"]
mod tests;
