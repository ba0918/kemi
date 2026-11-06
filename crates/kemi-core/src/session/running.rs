//! 起動中の記録 `<id>.running`（ps.md の R-PS-RECORD）。
//!
//! 起動中のレビューが、`kemi ps` が出す値（PID・ページの URL・起動した時刻など）を自分で書く。
//! ページのトークンを持つので、`<id>.endpoint` と同じ置き場所・同じ権限で置く。エージェントの
//! トークンは持たない（`kemi ps` が `<id>.endpoint` を読まずに済むように）。中身は JSON（D17）。
//! 強制終了で残ったものは、そのセッションのロックが外れていれば無いものとして扱い、掃除と
//! セッションの削除で消す。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{SessionError, SessionMode};

/// 起動中のレビュー 1 つの値。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningRecord {
    /// serve している kemi のプロセス。
    pub pid: u32,
    /// ページの URL（stderr の `kemi: <url>` と同じ）。
    pub url: String,
    /// `--live` の中継の URL（stderr の `kemi: live <url>` と同じ）。
    pub live_url: Option<String>,
    /// このプロセスがサーバーを起動した時刻（UNIX エポックからのミリ秒）。
    pub started_at: u128,
    /// 製品の版。
    pub version: String,
    pub mode: RunningMode,
    /// `kemi --resume` の一覧のモードの列と同じ文字。
    pub label: String,
    /// セッションの記録した元のワークスペースと、その識別（R-RESULT）。
    pub workspace: PathBuf,
    pub workspace_key: String,
}

/// 入力モードの種類。`kemi ps --format json` の `mode` の値と同じ綴り。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunningMode {
    Worktree,
    Staged,
    Range,
    Manifest,
    Live,
}

impl RunningMode {
    pub fn of(mode: &SessionMode) -> Self {
        match mode {
            SessionMode::Worktree => RunningMode::Worktree,
            SessionMode::Staged => RunningMode::Staged,
            SessionMode::Range { .. } => RunningMode::Range,
            SessionMode::Manifest => RunningMode::Manifest,
            SessionMode::Live { .. } => RunningMode::Live,
        }
    }
}

pub(crate) fn running_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.running"))
}

/// `<id>.running` を所有者だけが読める形で、一時ファイルから名前を変えて書く（unix は `0600`）。
pub fn write_running_record(
    dir: &Path,
    id: &str,
    record: &RunningRecord,
) -> Result<PathBuf, SessionError> {
    super::checked_id(id)?;
    let path = running_path(dir, id);
    super::storage_io::write_json_atomic(dir, &path, record)?;
    Ok(path)
}

/// `<id>.running` を読む。無ければ `None`、読み解けなければ `Corrupt`。
pub fn read_running_record(dir: &Path, id: &str) -> Result<Option<RunningRecord>, SessionError> {
    super::checked_id(id)?;
    super::storage_io::read_json(&running_path(dir, id))
}

/// `<id>.running` を消す。無いのは成功と同じ。
pub fn remove_running_record(dir: &Path, id: &str) -> Result<(), SessionError> {
    super::checked_id(id)?;
    super::storage_io::remove_if_present(&running_path(dir, id))
}

#[cfg(test)]
mod tests {
    use crate::session::store::tests::fixtures::running_record;
    use crate::session::{SessionError, read_running_record, running_path, write_running_record};
    use crate::testutil::Scratch;

    const ID: &str = "01HF7YAT00AAAAAAAAAAAAAAAA";

    #[test]
    fn running_mode_names_the_kind_of_input() {
        use crate::domain::live::LivePage;
        use crate::session::{RunningMode, SessionMode};
        let range = SessionMode::Range {
            from: "main".to_string(),
            to: "HEAD".to_string(),
            from_sha: "a".repeat(40),
            to_sha: "b".repeat(40),
        };
        let live = SessionMode::Live {
            page: LivePage::Url("http://127.0.0.1:9/app".to_string()),
            root: std::path::PathBuf::from("/tmp/workspace"),
        };

        assert_eq!(
            [
                SessionMode::Worktree,
                SessionMode::Staged,
                range,
                SessionMode::Manifest,
                live
            ]
            .iter()
            .map(RunningMode::of)
            .collect::<Vec<_>>(),
            [
                RunningMode::Worktree,
                RunningMode::Staged,
                RunningMode::Range,
                RunningMode::Manifest,
                RunningMode::Live
            ]
        );
    }

    #[test]
    fn running_record_roundtrips_and_is_owner_only() {
        let scratch = Scratch::new("session");

        write_running_record(&scratch.dir(), ID, &running_record()).unwrap();

        assert_eq!(
            read_running_record(&scratch.dir(), ID).unwrap(),
            Some(running_record())
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(running_path(&scratch.dir(), ID))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn missing_running_record_reads_as_absent_and_broken_one_as_unreadable() {
        let scratch = Scratch::new("session");
        std::fs::create_dir_all(scratch.dir()).unwrap();
        std::fs::write(running_path(&scratch.dir(), ID), b"{not json").unwrap();

        assert_eq!(
            read_running_record(&scratch.dir(), "01HF7YAT00BBBBBBBBBBBBBBBB").unwrap(),
            None
        );
        let broken = read_running_record(&scratch.dir(), ID);
        assert!(
            matches!(&broken, Err(SessionError::Corrupt { reason, .. }) if !reason.is_empty()),
            "{broken:?}"
        );
    }
}
