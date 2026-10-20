//! バイナリの左右のサイズ。Git オブジェクトはリポジトリごとにまとめて問い合わせる。

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use super::{git_with_stdin, object_size, path_bytes};
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
    let (output, written) = git_with_stdin(
        repo,
        &["cat-file", "--batch-check=%(objectsize)", "--buffer", "-Z"],
        &input,
    )
    .ok()?;
    written.ok()?;
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
}

pub(super) fn side_size(side: &SideRef) -> u64 {
    match side {
        SideRef::Absent => 0,
        SideRef::Inline(bytes) => bytes.len() as u64,
        SideRef::Disk(path) => std::fs::metadata(path).map_or(0, |meta| meta.len()),
        SideRef::Git { repo, spec } => object_size(repo, spec).ok().flatten().unwrap_or(0),
    }
}
