//! バイナリの左右のサイズ。Git オブジェクトはリポジトリごとにまとめて問い合わせる。

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::{git_raw_os, path_bytes};
use crate::source::SideRef;

pub(super) fn side_sizes(sides: &[&SideRef]) -> Vec<u64> {
    let mut sizes = vec![0; sides.len()];
    let mut repositories: HashMap<&Path, Vec<(usize, &OsStr)>> = HashMap::new();
    for (index, side) in sides.iter().enumerate() {
        match side {
            SideRef::Git { repo, spec } => {
                repositories.entry(repo).or_default().push((index, spec));
            }
            SideRef::Absent | SideRef::Inline(_) | SideRef::Disk(_) => {
                sizes[index] = side_size(side)
            }
        }
    }
    for (repo, objects) in repositories {
        let specs: Vec<_> = objects.iter().map(|(_, spec)| *spec).collect();
        match batch_sizes(repo, &specs) {
            Some(found) => {
                for ((index, _), size) in objects.into_iter().zip(found) {
                    sizes[index] = size;
                }
            }
            // -Z を持たない Git でも、従来と同じパスと失敗時の 0 を保つ。
            None => {
                for (index, _) in objects {
                    sizes[index] = side_size(sides[index]);
                }
            }
        }
    }
    sizes
}

fn batch_sizes(repo: &Path, specs: &[&OsStr]) -> Option<Vec<u64>> {
    let mut input = Vec::new();
    for spec in specs {
        input.extend(path_bytes(Path::new(spec)));
        input.push(0);
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "--batch-check=%(objectsize)", "--buffer", "-Z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    std::thread::scope(|scope| {
        // 要求と応答がどちらもパイプ容量を超えうるので、書き込み中も応答を回収する。
        let writing = scope.spawn(move || stdin.write_all(&input));
        let output = child.wait_with_output().ok()?;
        writing.join().ok()?.ok()?;
        if !output.status.success() {
            return None;
        }
        let records: Vec<_> = output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
            .collect();
        if records.len() != specs.len() {
            return None;
        }
        Some(
            records
                .into_iter()
                .map(|record| {
                    std::str::from_utf8(record)
                        .ok()
                        .and_then(|text| text.parse().ok())
                        .unwrap_or(0)
                })
                .collect(),
        )
    })
}

pub(super) fn side_size(side: &SideRef) -> u64 {
    match side {
        SideRef::Absent => 0,
        SideRef::Inline(bytes) => bytes.len() as u64,
        SideRef::Disk(path) => std::fs::metadata(path).map_or(0, |meta| meta.len()),
        SideRef::Git { repo, spec } => git_raw_os(
            repo,
            &[OsStr::new("cat-file"), OsStr::new("-s"), spec.as_os_str()],
        )
        .ok()
        .and_then(|text| {
            std::str::from_utf8(&text)
                .ok()
                .and_then(|text| text.trim().parse().ok())
        })
        .unwrap_or(0),
    }
}
