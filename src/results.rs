//! 結果ファイル・セッション・つなぎ先の置き場所と、結果ファイルの書き込みと読み出し
//! （R-RESULT、R-SESSION、R-AGENT-LINK）。

use std::path::PathBuf;

use kemi_core::domain::submit::Verdict;
use kemi_core::result;
use kemi_server::{ResultSaveError, ResultSink};

use crate::cli::Cli;
use crate::place::LaunchPlace;
use crate::report::{self, fail};

/// 結果の置き場所を決める環境変数が無いときの理由（R-RESULT）。OS ごとに使う変数が違う。
pub(crate) fn results_unset_reason() -> &'static str {
    if cfg!(windows) {
        "LOCALAPPDATA is not set or not absolute"
    } else {
        "HOME is not set"
    }
}

/// 結果ファイルの置き場所（R-RESULT）。
pub(crate) fn results_dir() -> Option<PathBuf> {
    result::results_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// セッションの置き場所（R-SESSION）。結果ファイルと同じ根の下。
pub(crate) fn sessions_dir() -> Option<PathBuf> {
    result::sessions_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// `<id>.endpoint` の置き場所（R-AGENT-LINK）。
pub(crate) fn endpoints_dir() -> Option<PathBuf> {
    result::endpoint_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// `kemi --result`: 最新の結果ファイルの中身を stdout に出し、元の verdict の終了コードで
/// 終わる。該当が無ければ何も出さず終了コード 2。
pub(crate) fn print_result(cli: &Cli) -> ! {
    let Some(dir) = results_dir() else {
        fail(&format!(
            "cannot determine where to store results ({})",
            results_unset_reason()
        ));
    };
    let key = (!cli.any).then(|| {
        let place = cli.workspace.clone().unwrap_or_else(|| PathBuf::from("."));
        result::workspace_key(&LaunchPlace::at(place).workspace())
    });
    let Some(text) = result::load_latest(&dir, key.as_deref()) else {
        fail("no result for this location");
    };
    // 壊れたファイルを stdout に出してから終了コード 2 にしないよう、先に読み解く。
    let verdict = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|document| document["verdict"].as_str().and_then(Verdict::parse));
    let Some(verdict) = verdict else {
        fail("cannot read the latest result file");
    };
    let code = verdict.exit_code();
    report::write_stdout(&text);
    std::process::exit(code);
}

/// 結果ファイルを書く先。submit を確定したときにサーバから呼ばれる。
pub(crate) struct ResultStore {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) key: String,
}

impl ResultSink for ResultStore {
    fn save(&self, text: &str) -> Result<PathBuf, ResultSaveError> {
        let dir = self
            .dir
            .as_ref()
            .ok_or_else(|| ResultSaveError::Unlocated(results_unset_reason().to_string()))?;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        result::save(dir, &self.key, text, millis, std::process::id()).map_err(|source| {
            ResultSaveError::Write {
                dir: dir.clone(),
                source,
            }
        })
    }

    fn location(&self) -> Option<String> {
        self.dir
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned())
    }
}
