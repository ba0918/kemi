//! digest（R-DIGEST）。行内容を含まない有界なレビューの地図。

use std::collections::BTreeMap;

use serde::Serialize;

use crate::domain::review::{FileEntry, Group, ReviewMeta};

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
    pub top_files_omitted: DigestOmitted,
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

/// 予算または `top_n` で `top_files` に載せられなかった分の件数と増減の合計。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DigestOmitted {
    pub files: usize,
    pub add: u64,
    pub del: u64,
}

/// 出力の組み立てに使う上限。文字列の文字数、グループ数、ディレクトリ数、
/// top_files の件数。
#[derive(Clone, Copy)]
struct Limits {
    strings: usize,
    groups: usize,
    directories: usize,
    top_files: usize,
}

/// 上限の探索で変わらない集計とファイルの並び。候補ごとに作り直さない。
struct PreparedDigest<'a> {
    title: &'a str,
    totals: DigestTotals,
    groups: Vec<GroupStats<'a>>,
    directories: Vec<DigestDirectory>,
    all_files: Vec<&'a FileEntry>,
}

struct GroupStats<'a> {
    group: &'a Group,
    add: u64,
    del: u64,
}

pub fn build_digest(review: &ReviewMeta, top_n: usize) -> Digest {
    let prepared = prepare_digest(review);
    let mut limits = Limits {
        strings: FIELD_LIMIT,
        groups: GROUP_LIMIT,
        directories: DIRECTORY_LIMIT,
        top_files: top_n,
    };
    loop {
        if let Some(digest) = fit_strings(&prepared, top_n, limits) {
            return digest;
        }
        // 文字列を空にしても予算を超えるのは、項目の構造だけで大きい場合。
        // グループ数・ディレクトリ数を減らし、それでも収まらなければ
        // top_files の件数を減らして再挑戦する。
        if limits.groups > 1 || limits.directories > 1 {
            limits.groups = (limits.groups / 2).max(1);
            limits.directories = (limits.directories / 2).max(1);
            continue;
        }
        if limits.top_files > 0 {
            limits.top_files /= 2;
            continue;
        }
        return assemble(&prepared, top_n, limits);
    }
}

/// 文字列の上限だけを二分探索し、予算に収まる最大の文字数を探す。
/// 文字列をすべて空にしても収まらない場合は None。
fn fit_strings(prepared: &PreparedDigest<'_>, top_n: usize, limits: Limits) -> Option<Digest> {
    let empty = assemble(
        prepared,
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
            prepared,
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

fn prepare_digest(review: &ReviewMeta) -> PreparedDigest<'_> {
    let mut totals = DigestTotals {
        files: 0,
        add: 0,
        del: 0,
        noise_files: 0,
    };
    let mut groups = Vec::new();
    let mut directories: BTreeMap<String, DigestDirectory> = BTreeMap::new();
    let mut all_files: Vec<&FileEntry> = Vec::new();

    for group in &review.groups {
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
        groups.push(GroupStats {
            group,
            add: group_add,
            del: group_del,
        });
    }

    all_files.sort_by(|left, right| {
        let left_total = u64::from(left.add) + u64::from(left.del);
        let right_total = u64::from(right.add) + u64::from(right.del);
        right_total
            .cmp(&left_total)
            .then_with(|| left.path.cmp(&right.path))
    });
    PreparedDigest {
        title: &review.title,
        totals,
        groups,
        directories: directories.into_values().collect(),
        all_files,
    }
}

fn assemble(prepared: &PreparedDigest<'_>, top_n: usize, limits: Limits) -> Digest {
    let mut groups = Vec::new();
    let mut omitted_groups = 0usize;
    let mut omitted_group_files = 0usize;
    let mut omitted_group_add = 0u64;
    let mut omitted_group_del = 0u64;
    for (index, stats) in prepared.groups.iter().enumerate() {
        let group = stats.group;
        if index < limits.groups {
            groups.push(DigestGroup {
                id: truncate(&group.id, limits.strings),
                title: truncate(&group.title, limits.strings),
                why: truncate(&group.why, limits.strings),
                watch: truncate(&group.watch, limits.strings),
                files: group.files.len(),
                add: stats.add,
                del: stats.del,
            });
        } else {
            omitted_groups += 1;
            omitted_group_files += group.files.len();
            omitted_group_add += stats.add;
            omitted_group_del += stats.del;
        }
    }
    if omitted_groups > 0 {
        groups.push(DigestGroup {
            id: "…".to_string(),
            title: format!("… and {}", count_label(omitted_groups, "more group")),
            why: String::new(),
            watch: String::new(),
            files: omitted_group_files,
            add: omitted_group_add,
            del: omitted_group_del,
        });
    }

    let mut directories: Vec<DigestDirectory> = prepared
        .directories
        .iter()
        .take(limits.directories)
        .cloned()
        .collect();
    if prepared.directories.len() > limits.directories {
        let mut files = 0;
        let mut add = 0u64;
        let mut del = 0u64;
        let mut noise_files = 0;
        for directory in prepared.directories.iter().skip(limits.directories) {
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

    let all_files = &prepared.all_files;
    let omitted = DigestOmitted {
        files: all_files.len().saturating_sub(limits.top_files),
        add: all_files
            .iter()
            .skip(limits.top_files)
            .map(|file| u64::from(file.add))
            .sum(),
        del: all_files
            .iter()
            .skip(limits.top_files)
            .map(|file| u64::from(file.del))
            .sum(),
    };
    let mut top_files: Vec<DigestFile> = all_files
        .iter()
        .take(limits.top_files)
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
        title: truncate(prepared.title, limits.strings),
        totals: prepared.totals.clone(),
        groups,
        directories,
        top_files,
        top_files_omitted: omitted,
        top_n,
    }
}

fn root_directory(path: &str) -> String {
    match path.split_once('/') {
        Some((directory, _)) => directory.to_string(),
        None => ".".to_string(),
    }
}

/// 数と名詞を並べ、1 のときだけ単数形にする。名詞は s を足すだけで複数形になるものに限る。
fn count_label(count: usize, noun: &str) -> String {
    let suffix = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{suffix}")
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
#[path = "digest/tests.rs"]
mod tests;
