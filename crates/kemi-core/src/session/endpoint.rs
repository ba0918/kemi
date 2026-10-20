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

pub(crate) fn endpoint_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.endpoint"))
}

/// `<id>.endpoint` を所有者だけが読める形で書く（unix は `0600`）。
pub fn write_endpoint(dir: &Path, id: &str, endpoint: &Endpoint) -> Result<PathBuf, SessionError> {
    super::checked_id(id)?;
    let bytes = serde_json::to_vec(endpoint).map_err(|error| SessionError::Encode {
        reason: error.to_string(),
    })?;
    let path = endpoint_path(dir, id);
    super::storage_io::write_atomic(dir, &path, &bytes)?;
    Ok(path)
}

/// `<id>.endpoint` を読む。無ければ `None`。
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind は non_exhaustive で、列挙しきれない"
)]
pub fn read_endpoint(dir: &Path, id: &str) -> Result<Option<Endpoint>, SessionError> {
    super::checked_id(id)?;
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
    super::checked_id(id)?;
    super::storage_io::remove_if_present(&endpoint_path(dir, id))
}

#[cfg(test)]
mod tests {
    use crate::testutil::Scratch;

    #[test]
    fn endpoint_roundtrips_and_is_owner_only() {
        use crate::session::{Endpoint, read_endpoint, write_endpoint};
        let scratch = Scratch::new("session");
        let id = "01HF7YAT00AAAAAAAAAAAAAAAA";
        let endpoint = Endpoint {
            port: 41234,
            token: "secret".to_string(),
        };

        write_endpoint(&scratch.dir(), id, &endpoint).unwrap();

        assert_eq!(read_endpoint(&scratch.dir(), id).unwrap(), Some(endpoint));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(crate::session::endpoint_path(&scratch.dir(), id))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert_eq!(
            read_endpoint(&scratch.dir(), "01HF7YAT00BBBBBBBBBBBBBBBB").unwrap(),
            None
        );
    }
}
