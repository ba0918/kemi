//! git の起動。引数（生のバイト列のパスを含む）と標準入力を渡し、失敗を SourceError にする。

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use crate::source::SourceError;

/// 標準入力を渡して git を呼ぶ。
pub(super) fn git_with_input(
    repo: &Path,
    args: &[&str],
    input: &[u8],
) -> Result<Vec<u8>, SourceError> {
    let (output, _) = git_with_stdin(repo, args, input).map_err(|source| SourceError::Io {
        path: repo.to_path_buf(),
        source,
    })?;
    if !output.status.success() {
        return Err(SourceError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

/// 標準入力に `input` を書きながら git の終わりを待つ。終了コードの解釈は呼び出し側に
/// 任せ、出力と、入力を書き終えられたかを返す。
pub(crate) fn git_with_stdin(
    repo: &Path,
    args: &[&str],
    input: &[u8],
) -> std::io::Result<(std::process::Output, std::io::Result<()>)> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    std::thread::scope(|scope| {
        // 要求と応答がどちらもパイプ容量を超えうるので、書き込み中も応答を回収する。
        let writer = scope.spawn(move || stdin.write_all(input));
        let output = child.wait_with_output()?;
        let written = writer
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("writing to git stdin panicked")));
        Ok((output, written))
    })
}

/// `git cat-file -s` でオブジェクトの大きさを読む。git が失敗したら Err、出力を数として
/// 読めなければ Ok(None)。
pub(super) fn object_size(repo: &Path, spec: &OsStr) -> Result<Option<u64>, SourceError> {
    let output = git_raw_os(repo, &[OsStr::new("cat-file"), OsStr::new("-s"), spec])?;
    Ok(std::str::from_utf8(&output)
        .ok()
        .and_then(|text| text.trim().parse().ok()))
}

pub(crate) fn show(repo: &Path, spec: &OsStr) -> Result<Vec<u8>, SourceError> {
    git_raw_os(repo, &[OsStr::new("show"), spec])
}

pub(crate) fn git_raw(repo: &Path, args: &[&str]) -> Result<Vec<u8>, SourceError> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git_raw_os(repo, &args)
}

/// git の引数に生のバイト列（非 UTF-8 のパスを含む pathspec）を渡せる版。
pub(crate) fn git_raw_os(repo: &Path, args: &[&OsStr]) -> Result<Vec<u8>, SourceError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|source| SourceError::Io {
            path: repo.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(SourceError::Git(format!(
            "git {} failed: {}",
            display_args(args),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn display_args(args: &[&OsStr]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

/// コミットを読む `git log`。利用者の設定で、読み取る出力の形が変わらないようにする。
/// log.showSignature は署名の検証結果を sha の前に混ぜ、i18n.logOutputEncoding は
/// 件名と本文を UTF-8 以外で出すため、どちらも引数で打ち消す。
pub(crate) fn git_log(repo: &Path, args: &[&str]) -> Result<String, SourceError> {
    let mut full = vec!["log", "--no-show-signature", "--encoding=UTF-8"];
    full.extend_from_slice(args);
    git_text(repo, &full)
}

pub(crate) fn git_text(repo: &Path, args: &[&str]) -> Result<String, SourceError> {
    let bytes = git_raw(repo, args)?;
    String::from_utf8(bytes)
        .map_err(|_| SourceError::Git(format!("output of git {} is not UTF-8", args.join(" "))))
}
