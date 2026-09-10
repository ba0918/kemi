//! digest（R-DIGEST）。行内容を含まない有界なレビューの地図。

use std::collections::BTreeMap;

use serde::Serialize;

use crate::domain::review::{FileEntry, ReviewMeta};

/// 文字列フィールドの上限。切った場合は末尾に `…` を付ける。
const FIELD_LIMIT: usize = 2_000;

/// digest 全体の直列化後バイト数の予算（R-DIGEST は 100 KB 未満）。
/// 余裕を持たせて、数値や構造の差で上限を越えないようにする。
const OUTPUT_BUDGET: usize = 90_000;

/// 出力に残すグループ数の上限。超えた分は末尾の集約項目にまとめる。
const GROUP_LIMIT: usize = 400;

/// 出力に残すルートディレクトリ数の上限。超えた分は末尾の集約項目にまとめる。
const DIRECTORY_LIMIT: usize = 200;

const KEMI_CONTRACT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Digest {
    pub kemi: u32,
    pub title: String,
    pub totals: DigestTotals,
    pub groups: Vec<DigestGroup>,
    pub directories: Vec<DigestDirectory>,
    pub top_files: Vec<DigestFile>,
    pub top_n: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DigestTotals {
    pub files: usize,
    pub add: u64,
    pub del: u64,
    pub noise_files: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DigestGroup {
    pub id: String,
    pub title: String,
    pub why: String,
    pub watch: String,
    pub files: usize,
    pub add: u64,
    pub del: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DigestDirectory {
    pub path: String,
    pub files: usize,
    pub add: u64,
    pub del: u64,
    pub noise_files: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DigestFile {
    pub path: String,
    pub status: String,
    pub add: u64,
    pub del: u64,
    pub noise: bool,
    pub focus: bool,
    pub note: String,
}

/// 出力の組み立てに使う上限。文字列の文字数、グループ数、ディレクトリ数。
#[derive(Clone, Copy)]
struct Limits {
    strings: usize,
    groups: usize,
    directories: usize,
}

pub fn build_digest(review: &ReviewMeta, top_n: usize) -> Digest {
    let mut limits = Limits {
        strings: FIELD_LIMIT,
        groups: GROUP_LIMIT,
        directories: DIRECTORY_LIMIT,
    };
    loop {
        if let Some(digest) = fit_strings(review, top_n, limits) {
            return digest;
        }
        // 文字列を空にしても予算を超えるのは、項目の構造だけで大きい場合。
        // グループ数・ディレクトリ数を減らして再挑戦する。
        if limits.groups <= 1 && limits.directories <= 1 {
            return assemble(review, top_n, limits);
        }
        limits.groups = (limits.groups / 2).max(1);
        limits.directories = (limits.directories / 2).max(1);
    }
}

/// 文字列の上限だけを二分探索し、予算に収まる最大の文字数を探す。
/// 文字列をすべて空にしても収まらない場合は None。
fn fit_strings(review: &ReviewMeta, top_n: usize, limits: Limits) -> Option<Digest> {
    let empty = assemble(
        review,
        top_n,
        Limits {
            strings: 0,
            ..limits
        },
    );
    if serialized_len(&empty) >= OUTPUT_BUDGET {
        return None;
    }
    let mut low = 0;
    let mut high = limits.strings;
    let mut best = empty;
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate = assemble(
            review,
            top_n,
            Limits {
                strings: middle,
                ..limits
            },
        );
        if serialized_len(&candidate) < OUTPUT_BUDGET {
            low = middle;
            best = candidate;
        } else {
            high = middle - 1;
        }
    }
    Some(best)
}

fn serialized_len(digest: &Digest) -> usize {
    serde_json::to_string(digest).map_or(usize::MAX, |json| json.len())
}

fn assemble(review: &ReviewMeta, top_n: usize, limits: Limits) -> Digest {
    let mut totals = DigestTotals {
        files: 0,
        add: 0,
        del: 0,
        noise_files: 0,
    };
    let mut groups = Vec::new();
    let mut omitted_groups = 0usize;
    let mut omitted_group_files = 0usize;
    let mut omitted_group_add = 0u64;
    let mut omitted_group_del = 0u64;
    let mut directories: BTreeMap<String, DigestDirectory> = BTreeMap::new();
    let mut all_files: Vec<&FileEntry> = Vec::new();

    for (index, group) in review.groups.iter().enumerate() {
        let mut group_add = 0;
        let mut group_del = 0;
        for file in &group.files {
            totals.files += 1;
            totals.add += u64::from(file.add);
            totals.del += u64::from(file.del);
            if file.noise {
                totals.noise_files += 1;
            }
            group_add += u64::from(file.add);
            group_del += u64::from(file.del);

            let directory = root_directory(&file.path);
            let entry = directories
                .entry(directory.clone())
                .or_insert_with(|| DigestDirectory {
                    path: directory,
                    files: 0,
                    add: 0,
                    del: 0,
                    noise_files: 0,
                });
            entry.files += 1;
            entry.add += u64::from(file.add);
            entry.del += u64::from(file.del);
            if file.noise {
                entry.noise_files += 1;
            }

            all_files.push(file);
        }
        if index < limits.groups {
            groups.push(DigestGroup {
                id: truncate(&group.id, limits.strings),
                title: truncate(&group.title, limits.strings),
                why: truncate(&group.why, limits.strings),
                watch: truncate(&group.watch, limits.strings),
                files: group.files.len(),
                add: group_add,
                del: group_del,
            });
        } else {
            omitted_groups += 1;
            omitted_group_files += group.files.len();
            omitted_group_add += group_add;
            omitted_group_del += group_del;
        }
    }
    if omitted_groups > 0 {
        groups.push(DigestGroup {
            id: "…".to_string(),
            title: format!("…ほか {omitted_groups} グループ"),
            why: String::new(),
            watch: String::new(),
            files: omitted_group_files,
            add: omitted_group_add,
            del: omitted_group_del,
        });
    }

    let mut directories: Vec<DigestDirectory> = directories.into_values().collect();
    if directories.len() > limits.directories {
        let omitted = directories.split_off(limits.directories);
        let mut files = 0;
        let mut add = 0u64;
        let mut del = 0u64;
        let mut noise_files = 0;
        for directory in omitted {
            files += directory.files;
            add += directory.add;
            del += directory.del;
            noise_files += directory.noise_files;
        }
        directories.push(DigestDirectory {
            path: "…".to_string(),
            files,
            add,
            del,
            noise_files,
        });
    }

    all_files.sort_by(|left, right| {
        let left_total = u64::from(left.add) + u64::from(left.del);
        let right_total = u64::from(right.add) + u64::from(right.del);
        right_total
            .cmp(&left_total)
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut top_files: Vec<DigestFile> = all_files
        .into_iter()
        .take(top_n)
        .map(|file| DigestFile {
            path: truncate(&file.path, limits.strings),
            status: file.status.as_str().to_string(),
            add: u64::from(file.add),
            del: u64::from(file.del),
            noise: file.noise,
            focus: file.focus,
            note: truncate(&file.note, limits.strings),
        })
        .collect();
    // 文字列を切った後も「増減の合計の降順、同数はパス昇順」を保つ。
    top_files.sort_by(|left, right| {
        let left_total = left.add + left.del;
        let right_total = right.add + right.del;
        right_total
            .cmp(&left_total)
            .then_with(|| left.path.cmp(&right.path))
    });

    Digest {
        kemi: KEMI_CONTRACT_VERSION,
        title: truncate(&review.title, limits.strings),
        totals,
        groups,
        directories,
        top_files,
        top_n,
    }
}

fn root_directory(path: &str) -> String {
    match path.split_once('/') {
        Some((directory, _)) => directory.to_string(),
        None => ".".to_string(),
    }
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(limit).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{FileEntry, Group, ReviewMeta, Status};
    use serde_json::json;

    fn file(path: &str, add: u32, del: u32) -> FileEntry {
        FileEntry {
            id: format!("f-{path}"),
            group_id: "g1".to_string(),
            path: path.to_string(),
            old_path: None,
            status: Status::Modify,
            add,
            del,
            binary: false,
            old_size: 0,
            new_size: 0,
            focus: false,
            note: String::new(),
            noise: false,
        }
    }

    fn review_with(files: Vec<FileEntry>) -> ReviewMeta {
        ReviewMeta {
            title: "変更のレビュー".to_string(),
            subtitle: String::new(),
            meta: json!(null),
            groups: vec![Group {
                id: "g1".to_string(),
                title: "g1".to_string(),
                why: String::new(),
                watch: String::new(),
                files,
            }],
            approval: Vec::new(),
        }
    }

    #[test]
    fn digest_directories_aggregate_at_root_level() {
        let review = review_with(vec![
            file("src/a.rs", 1, 1),
            file("src/sub/b.rs", 2, 2),
            file("README.md", 3, 3),
            file("lib/x.rs", 4, 4),
        ]);

        let digest = build_digest(&review, 100);
        let directories: Vec<&DigestDirectory> = digest.directories.iter().collect();

        assert_eq!(directories.len(), 3);
        assert_eq!(directories[0].path, ".");
        assert_eq!(directories[0].files, 1);
        assert_eq!(directories[1].path, "lib");
        assert_eq!(directories[2].path, "src");
        assert_eq!(directories[2].files, 2);
        assert_eq!(directories[2].add, 3);
    }

    #[test]
    fn digest_top_files_sort_by_total_then_path() {
        let mut b = file("b.rs", 1, 1);
        b.noise = true;
        let review = review_with(vec![
            file("a.rs", 10, 1),
            file("c.rs", 2, 10),
            b,
            file("d.rs", 3, 0),
        ]);

        let digest = build_digest(&review, 100);
        let paths: Vec<&str> = digest.top_files.iter().map(|f| f.path.as_str()).collect();

        assert_eq!(paths, vec!["c.rs", "a.rs", "d.rs", "b.rs"]);
        assert!(digest.top_files[3].noise);
    }

    #[test]
    fn digest_top_files_limited_by_top_n() {
        let review = review_with(vec![
            file("a.rs", 1, 1),
            file("b.rs", 2, 2),
            file("c.rs", 3, 3),
        ]);

        let digest = build_digest(&review, 2);
        assert_eq!(digest.top_files.len(), 2);
        assert_eq!(digest.top_n, 2);
    }

    #[test]
    fn digest_totals_count_noise_files() {
        let mut noisy = file("Cargo.lock", 5, 5);
        noisy.noise = true;
        let mut binary = file("assets/logo.png", 0, 0);
        binary.binary = true;
        binary.noise = true;
        let review = review_with(vec![file("src/a.rs", 1, 2), noisy, binary]);

        let digest = build_digest(&review, 100);
        assert_eq!(digest.totals.files, 3);
        assert_eq!(digest.totals.add, 6);
        assert_eq!(digest.totals.del, 7);
        assert_eq!(digest.totals.noise_files, 2);
    }

    #[test]
    fn digest_groups_report_their_files_and_stats() {
        let review = review_with(vec![file("src/a.rs", 1, 2), file("src/b.rs", 3, 4)]);

        let digest = build_digest(&review, 100);
        assert_eq!(digest.groups.len(), 1);
        assert_eq!(digest.groups[0].id, "g1");
        assert_eq!(digest.groups[0].files, 2);
        assert_eq!(digest.groups[0].add, 4);
        assert_eq!(digest.groups[0].del, 6);
    }

    #[test]
    fn digest_truncates_long_strings_with_marker() {
        let long = "あ".repeat(2_100);
        let mut group = Group {
            id: "g1".to_string(),
            title: long.clone(),
            why: long.clone(),
            watch: long.clone(),
            files: vec![],
        };
        let mut entry = file("src/a.rs", 1, 1);
        entry.note = long.clone();
        group.files.push(entry);
        let review = ReviewMeta {
            title: long.clone(),
            subtitle: String::new(),
            meta: json!(null),
            groups: vec![group],
            approval: Vec::new(),
        };

        let digest = build_digest(&review, 100);
        let truncated: Vec<&str> = vec![
            digest.title.as_str(),
            digest.groups[0].title.as_str(),
            digest.groups[0].why.as_str(),
            digest.groups[0].watch.as_str(),
            digest.top_files[0].note.as_str(),
        ];
        for value in truncated {
            assert_eq!(value.chars().count(), 2_001);
            assert!(value.ends_with('…'));
        }
    }

    #[test]
    fn digest_bounded_thirty_thousand_files_under_hundred_kb() {
        let mut groups = Vec::new();
        for group_index in 0..100 {
            let mut files = Vec::new();
            for file_index in 0..300 {
                let path = format!("src/group{group_index}/file{file_index}.rs");
                let mut entry = file(&path, (file_index % 50) as u32, (group_index % 20) as u32);
                entry.noise = file_index % 7 == 0;
                entry.focus = file_index % 11 == 0;
                entry.note = "重点".to_string();
                files.push(entry);
            }
            groups.push(Group {
                id: format!("g{group_index}"),
                title: format!("コミット {group_index} の変更"),
                why: "理由".repeat(20),
                watch: "見てほしい点".repeat(20),
                files,
            });
        }
        let review = ReviewMeta {
            title: "3 万ファイル".to_string(),
            subtitle: String::new(),
            meta: json!(null),
            groups,
            approval: Vec::new(),
        };

        let digest = build_digest(&review, 100);
        let json = serde_json::to_string(&digest).unwrap();

        assert_eq!(digest.totals.files, 30_000);
        assert_eq!(digest.top_files.len(), 100);
        assert!(json.len() < 100_000, "digest was {} bytes", json.len());
        for pair in digest.top_files.windows(2) {
            let left = pair[0].add + pair[0].del;
            let right = pair[1].add + pair[1].del;
            assert!(
                left > right || (left == right && pair[0].path <= pair[1].path),
                "top_files not sorted: {:?}",
                pair
            );
        }
    }

    #[test]
    fn digest_bounded_when_every_group_carries_maximal_metadata() {
        let long = "あ".repeat(FIELD_LIMIT);
        let mut groups = Vec::new();
        for group_index in 0..400 {
            let mut files = Vec::new();
            for file_index in 0..75 {
                files.push(file(
                    &format!("src/g{group_index}/f{file_index}.rs"),
                    (file_index % 7) as u32,
                    (group_index % 5) as u32,
                ));
            }
            groups.push(Group {
                id: format!("g{group_index}"),
                title: long.clone(),
                why: long.clone(),
                watch: long.clone(),
                files,
            });
        }
        let review = ReviewMeta {
            title: long.clone(),
            subtitle: String::new(),
            meta: json!(null),
            groups,
            approval: Vec::new(),
        };

        let digest = build_digest(&review, 100);
        let json = serde_json::to_string(&digest).unwrap();

        assert_eq!(digest.totals.files, 30_000);
        assert_eq!(digest.top_files.len(), 100);
        assert!(json.len() < 100_000, "digest was {} bytes", json.len());
        assert!(
            digest.groups.iter().any(|group| group.title.ends_with('…')),
            "truncated metadata must keep the cut marker"
        );
    }

    #[test]
    fn digest_bounded_when_group_count_is_maximal() {
        let mut groups = Vec::new();
        for index in 0..30_000 {
            groups.push(Group {
                id: format!("g{index}"),
                title: format!("コミット {index}"),
                why: String::new(),
                watch: String::new(),
                files: vec![file(&format!("src/file{index}.rs"), 1, 1)],
            });
        }
        let review = ReviewMeta {
            title: "3 万グループ".to_string(),
            subtitle: String::new(),
            meta: json!(null),
            groups,
            approval: Vec::new(),
        };

        let digest = build_digest(&review, 100);
        let json = serde_json::to_string(&digest).unwrap();

        assert_eq!(digest.totals.files, 30_000);
        assert!(json.len() < 100_000, "digest was {} bytes", json.len());
    }

    #[test]
    fn digest_bounded_when_root_directory_count_is_maximal() {
        let files: Vec<FileEntry> = (0..30_000)
            .map(|index| file(&format!("d{index}/file.txt"), 1, 1))
            .collect();
        let mut review = review_with(files);
        review.title = "3 万ディレクトリ".to_string();

        let digest = build_digest(&review, 100);
        let json = serde_json::to_string(&digest).unwrap();

        assert_eq!(digest.totals.files, 30_000);
        assert!(json.len() < 100_000, "digest was {} bytes", json.len());
    }
}
