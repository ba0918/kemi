//! エージェントとのつなぎ先 `<id>.endpoint`（agent-channel.md の R-AGENT-LINK）。
//!
//! 起動中のレビューが、`kemi wait` と `kemi reply` がサーバに届くための情報（エージェント用の
//! ポートとトークン）を書く。中身は JSON（DA1）。セッションと同じ権限で置き、レビューが
//! 終わるときとセッションを消すときに消す。強制終了で残ったものは、そのセッションのロックが
//! 生きていなければ無いものとして扱う（読む側が [`super::SessionStore::is_running`] で見る）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::SessionError;

/// つなぎ先。トークンはページのトークンとは別の秘密で、このファイルにだけ書く。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
}

pub fn endpoint_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.endpoint"))
}

/// `<id>.endpoint` を所有者だけが読める形で書く（unix は `0600`）。
pub fn write_endpoint(dir: &Path, id: &str, endpoint: &Endpoint) -> Result<PathBuf, SessionError> {
    super::store::checked_id(id)?;
    let bytes = serde_json::to_vec(endpoint).map_err(|error| SessionError::Encode {
        reason: error.to_string(),
    })?;
    let path = endpoint_path(dir, id);
    super::store::write_atomic(dir, &path, &bytes)?;
    Ok(path)
}

/// `<id>.endpoint` を読む。無ければ `None`。
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind は non_exhaustive で、列挙しきれない"
)]
pub fn read_endpoint(dir: &Path, id: &str) -> Result<Option<Endpoint>, SessionError> {
    super::store::checked_id(id)?;
    let path = endpoint_path(dir, id);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => {
                return Err(SessionError::Io {
                    path,
                    source: error,
                });
            }
        },
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| SessionError::Corrupt {
            path,
            reason: error.to_string(),
        })
}

/// `<id>.endpoint` を消す。無いのは成功と同じ。
pub fn remove_endpoint(dir: &Path, id: &str) -> Result<(), SessionError> {
    super::store::checked_id(id)?;
    super::store::remove_if_present(&endpoint_path(dir, id))
}
