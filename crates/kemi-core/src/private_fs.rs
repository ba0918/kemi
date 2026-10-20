//! 所有者だけが使えるファイルとディレクトリ（unix はファイル `0600`、ディレクトリ `0700`）と、
//! 一時ファイルに書いてから名前を変える差し替え。結果ファイル（R-RESULT）とセッション
//! （R-SESSION）は同じ権限で置く。

use std::io::Write;
use std::path::Path;

/// 所有者だけが使えるディレクトリ（0700）を作る。新しく作る途中のディレクトリも 0700 に
/// なる。既にある `dir` は 0700 に直すが、その親は利用者の持ち物なので触らない。
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)
}

/// 書き込み先のファイルが既にあるときの扱い。
#[derive(Clone, Copy)]
pub(crate) enum Existing {
    /// 失敗する。名前が毎回違う一時ファイルで、既にあるものは別の書き手のもの。
    Fail,
    /// 中身を捨てて書き直す。名前が決まっている一時ファイルで、強制終了で残ったものを
    /// 断ると、その後の書き込みがずっと失敗する。
    Truncate,
}

/// 所有者だけが読み書きできるファイル（0600）に `bytes` を書く。
pub(crate) fn write_private_file(
    path: &Path,
    bytes: &[u8],
    existing: Existing,
) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    match existing {
        Existing::Fail => options.create_new(true),
        Existing::Truncate => options.create(true).truncate(true),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)
}

/// `bytes` を `temporary` に書き終えてから `path` に名前を変える。書き込みの途中で失敗した
/// ファイルが `path` として読まれないように。失敗したら一時ファイルを消す。
pub(crate) fn write_via_temporary(
    temporary: &Path,
    path: &Path,
    bytes: &[u8],
    existing: Existing,
) -> std::io::Result<()> {
    let written = write_private_file(temporary, bytes, existing)
        .and_then(|()| std::fs::rename(temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    written
}
