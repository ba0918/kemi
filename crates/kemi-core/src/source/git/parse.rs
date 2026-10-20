//! git の出力（`-z` の numstat・name-status・diff-tree）の解析と、git が返したパスの
//! バイト列の運び方。

use std::path::{Path, PathBuf};

use crate::domain::review::Status;

/// git が返した生のバイト列をパスとして運ぶ。表示のときだけ lossy に変換する。
#[cfg(unix)]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

/// `path_from_bytes` の逆。運んだパスを git が返したときのバイト列に戻す。
#[cfg(unix)]
pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
}

/// `git diff-tree --stdin --raw --numstat -z` の出力を、コミットごとの状態と増減数に分ける。
/// 各コミットは「sha\0」に続いて raw の記録、その後に numstat の記録が並ぶ。
pub(super) fn parse_diff_tree(bytes: &[u8]) -> Vec<(String, Vec<NameStatus>, Vec<Numstat>)> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut commits: Vec<(String, Vec<NameStatus>, Vec<Numstat>)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.is_empty() {
            index += 1;
            continue;
        }
        if token[0] == b':' {
            let letter = token
                .rsplit(|byte| *byte == b' ')
                .next()
                .and_then(|field| field.first().copied())
                .unwrap_or(b'M');
            // 知らない状態の記録もパスを 1 つ持つので、変更として読んでパスを取り込む。
            let letter = match letter {
                b'R' | b'C' | b'A' | b'D' | b'M' | b'T' => letter,
                _ => b'M',
            };
            let (record, consumed) = name_status_record(letter, &tokens, index);
            if let (Some(record), Some((_, names, _))) = (record, commits.last_mut()) {
                names.push(record);
            }
            index += consumed;
            continue;
        }
        if token.contains(&b'\t') {
            let (record, consumed) = numstat_record(&tokens, index);
            if let (Some(record), Some((_, _, stats))) = (record, commits.last_mut()) {
                stats.push(record);
            }
            index += consumed;
            continue;
        }
        commits.push((
            String::from_utf8_lossy(token).into_owned(),
            Vec::new(),
            Vec::new(),
        ));
        index += 1;
    }
    commits
}

pub(super) struct Numstat {
    pub(super) old_path: Option<PathBuf>,
    pub(super) new_path: PathBuf,
    pub(super) add: u64,
    pub(super) del: u64,
    pub(super) binary: bool,
}

pub(super) fn parse_numstat(bytes: &[u8]) -> Vec<Numstat> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is_empty() {
            index += 1;
            continue;
        }
        let (record, consumed) = numstat_record(&tokens, index);
        entries.extend(record);
        index += consumed;
    }
    entries
}

/// `-z` の numstat の 1 記録を読む。`add\tdel\tpath`、改名なら `add\tdel\t` に続く
/// 旧・新のパス。読んだ記録と、使ったトークン数を返す。
fn numstat_record(tokens: &[&[u8]], index: usize) -> (Option<Numstat>, usize) {
    let fields: Vec<&[u8]> = tokens[index].splitn(3, |byte| *byte == b'\t').collect();
    if fields.len() < 3 {
        return (None, 1);
    }
    let (add, del, binary) = (
        parse_count(fields[0]),
        parse_count(fields[1]),
        fields[0] == b"-",
    );
    if !fields[2].is_empty() {
        let record = Numstat {
            old_path: None,
            new_path: path_from_bytes(fields[2]),
            add,
            del,
            binary,
        };
        return (Some(record), 1);
    }
    if index + 2 < tokens.len() {
        let record = Numstat {
            old_path: Some(path_from_bytes(tokens[index + 1])),
            new_path: path_from_bytes(tokens[index + 2]),
            add,
            del,
            binary,
        };
        return (Some(record), 3);
    }
    (None, 1)
}

pub(super) struct NameStatus {
    pub(super) path: PathBuf,
    pub(super) old_path: Option<PathBuf>,
    pub(super) status: Status,
}

pub(super) fn parse_name_status(bytes: &[u8]) -> Vec<NameStatus> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.is_empty() {
            index += 1;
            continue;
        }
        let (record, consumed) = name_status_record(token[0], &tokens, index);
        entries.extend(record);
        index += consumed;
    }
    entries
}

/// 状態の文字 `letter` を持つ記録（`tokens[index]`）に続くパスを読む。改名とコピーは
/// 旧・新の 2 つ、ほかは 1 つ。読んだ記録と、使ったトークン数を返す。
fn name_status_record(letter: u8, tokens: &[&[u8]], index: usize) -> (Option<NameStatus>, usize) {
    match letter {
        b'R' | b'C' if index + 2 < tokens.len() => (
            Some(NameStatus {
                path: path_from_bytes(tokens[index + 2]),
                old_path: Some(path_from_bytes(tokens[index + 1])),
                status: Status::Rename,
            }),
            3,
        ),
        b'A' | b'D' | b'M' | b'T' if index + 1 < tokens.len() => (
            Some(NameStatus {
                path: path_from_bytes(tokens[index + 1]),
                old_path: None,
                status: match letter {
                    b'A' => Status::Add,
                    b'D' => Status::Delete,
                    _ => Status::Modify,
                },
            }),
            2,
        ),
        _ => (None, 1),
    }
}

/// `--name-status -z --no-renames` の「状態\0パス\0」を読む。
pub(super) fn parse_name_status_z(bytes: &[u8]) -> Vec<(u8, PathBuf)> {
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        if tokens[index].is_empty() {
            index += 1;
            continue;
        }
        entries.push((tokens[index][0], path_from_bytes(tokens[index + 1])));
        index += 2;
    }
    entries
}

fn parse_count(field: &[u8]) -> u64 {
    if field == b"-" {
        return 0;
    }
    String::from_utf8_lossy(field).parse().unwrap_or(0)
}

pub(crate) fn split_z(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect()
}
