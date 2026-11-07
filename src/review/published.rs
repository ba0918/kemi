//! レビューが書き、終わるときに消すファイル。エージェントとのつなぎ先 `<id>.endpoint`
//! （R-AGENT-LINK）と、起動中の記録 `<id>.running`（R-PS-RECORD）。

use std::path::PathBuf;

use kemi_core::session::{
    Endpoint, RunningMode, RunningRecord, now_millis, remove_endpoint, remove_running_record,
    write_endpoint, write_running_record,
};
use kemi_server::StoredSession;

use crate::report::warn;
use crate::results::{endpoints_dir, results_unset_reason};

/// 書いた `<id>.endpoint`。submit・保留・実行時エラーのどれで終わるときも消す
/// （R-AGENT-LINK）。
pub(super) struct EndpointFile {
    pub(super) endpoint: Endpoint,
    pub(super) dir: PathBuf,
    pub(super) id: String,
}

impl EndpointFile {
    pub(super) fn publish(&self) -> Result<(), String> {
        write_endpoint(&self.dir, &self.id, &self.endpoint)
            .map_err(|error| format!("cannot write the agent endpoint: {error}"))?;
        Ok(())
    }

    fn remove(&self) {
        if let Err(error) = remove_endpoint(&self.dir, &self.id) {
            warn(&format!("cannot remove the agent endpoint: {error}"));
        }
    }
}

/// 書く `<id>.running`（R-PS-RECORD）。書けなくても消せなくても、警告だけでレビューは続ける。
pub(super) struct RunningFile {
    dir: PathBuf,
    id: String,
    record: RunningRecord,
}

impl RunningFile {
    /// セッションのあるレビューの記録を組み立てる。エージェントの待ち受けの有無には
    /// 結び付けない（待ち受けが立たないレビューも kemi ps に出す）。
    pub(super) fn prepare(
        stored_session: Option<&StoredSession>,
        url: &str,
        live_url: Option<&str>,
    ) -> Option<Self> {
        let info = stored_session?.info();
        let Some(dir) = endpoints_dir() else {
            warn(&format!(
                "cannot determine where to write the running record ({})",
                results_unset_reason()
            ));
            return None;
        };
        let record = RunningRecord {
            pid: std::process::id(),
            url: url.to_string(),
            live_url: live_url.map(str::to_string),
            started_at: now_millis(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            mode: RunningMode::of(&info.mode),
            label: info.mode.label(&info.title),
            workspace: info.workspace,
            workspace_key: info.workspace_key,
        };
        Some(RunningFile {
            dir,
            id: info.id,
            record,
        })
    }

    fn publish(&self) {
        if let Err(error) = write_running_record(&self.dir, &self.id, &self.record) {
            warn(&format!("cannot write the running record: {error}"));
        }
    }

    fn remove(&self) {
        if let Err(error) = remove_running_record(&self.dir, &self.id) {
            warn(&format!("cannot remove the running record: {error}"));
        }
    }
}

/// レビューが書くファイルの組。無いものは書かず、消さない。終わり方はどれも
/// `process::exit` で Drop が走らないので、終わる前に [`Published::remove`] で明示して消す。
pub(super) struct Published {
    pub(super) endpoint: Option<EndpointFile>,
    pub(super) running: Option<RunningFile>,
}

impl Published {
    /// 書くのを起動時の説明の保存まで待たせたつなぎ先を書く（R-EXPLANATION-CLI）。
    pub(super) fn publish_endpoint(&self) -> Result<(), String> {
        self.endpoint.as_ref().map_or(Ok(()), EndpointFile::publish)
    }

    /// 起動中の記録を、URL を出す前に書く（R-PS-RECORD）。
    pub(super) fn publish_running(&self) {
        if let Some(running) = &self.running {
            running.publish();
        }
    }

    /// 両方を消す。セッションのロックを外す前に呼ぶ（外した後に消すと、その間に同じ id を
    /// 復元したレビューのものを消しうる）。
    pub(super) fn remove(&self) {
        if let Some(endpoint) = &self.endpoint {
            endpoint.remove();
        }
        if let Some(running) = &self.running {
            running.remove();
        }
    }
}
