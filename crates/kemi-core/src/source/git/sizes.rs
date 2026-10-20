//! バイナリの左右のサイズ。Git オブジェクトはリポジトリごとにまとめて問い合わせる。

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use super::command::{git_with_stdin, object_size};
use super::parse::path_bytes;
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::source::git::git_spec;
    use crate::testutil::TempRepo;

    #[test]
    fn batched_side_sizes_preserve_missing_objects_and_unusual_paths() {
        let repo = TempRepo::new();
        #[cfg(unix)]
        let raw_name = {
            use std::os::unix::ffi::OsStrExt;
            OsStr::from_bytes(b"binary-\xff.bin")
        };
        let names = [OsStr::new("plain.bin"), OsStr::new("with space.bin")];
        for name in names {
            std::fs::write(repo.path.join(name), [0, 1, 2]).unwrap();
        }
        #[cfg(unix)]
        {
            std::fs::write(repo.path.join(raw_name), [0, 1, 2, 3]).unwrap();
            std::fs::write(repo.path.join("with\nnewline.bin"), [0, 1, 2]).unwrap();
        }
        repo.add_and_commit("binary files");
        let mut sides: Vec<_> = names
            .iter()
            .map(|name| SideRef::Git {
                repo: repo.path.clone(),
                spec: git_spec("HEAD:", Path::new(name)),
            })
            .collect();
        let mut expected = vec![3, 3];
        #[cfg(unix)]
        {
            sides.push(SideRef::Git {
                repo: repo.path.clone(),
                spec: git_spec("HEAD:", Path::new(raw_name)),
            });
            sides.push(SideRef::Git {
                repo: repo.path.clone(),
                spec: git_spec("HEAD:", Path::new("with\nnewline.bin")),
            });
            expected.extend([4, 3]);
        }
        sides.extend([
            SideRef::Git {
                repo: repo.path.clone(),
                spec: OsString::from("HEAD:missing.bin"),
            },
            SideRef::Git {
                repo: repo.path.clone(),
                spec: OsString::from(":plain.bin"),
            },
            SideRef::Absent,
            SideRef::Inline(vec![0; 7]),
            SideRef::Disk(repo.path.join("plain.bin")),
            SideRef::Disk(repo.path.join("missing.bin")),
        ]);
        expected.extend([0, 3, 0, 7, 3, 0]);
        assert_eq!(side_sizes(&sides.iter().collect::<Vec<_>>()), expected);
        assert_eq!(sides.iter().map(side_size).collect::<Vec<_>>(), expected);
        let other = TempRepo::new();
        std::fs::write(other.path.join("plain.bin"), [0; 5]).unwrap();
        other.add_and_commit("other repository");
        sides.push(SideRef::Git {
            repo: other.path.clone(),
            spec: OsString::from("HEAD:plain.bin"),
        });
        expected.push(5);
        assert_eq!(side_sizes(&sides.iter().collect::<Vec<_>>()), expected);
        assert_eq!(sides.iter().map(side_size).collect::<Vec<_>>(), expected);
    }
}
