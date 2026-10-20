use super::*;
use crate::domain::review::{FileEntry, Group, ReviewMeta};
use crate::testutil::file_entry;
use serde_json::json;

fn file(path: &str, add: u32, del: u32) -> FileEntry {
    FileEntry {
        id: format!("f-{path}"),
        add,
        del,
        ..file_entry(path)
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
fn count_label_uses_singular_only_for_one() {
    assert_eq!(count_label(0, "apple"), "0 apples");
    assert_eq!(count_label(1, "apple"), "1 apple");
    assert_eq!(count_label(2, "apple"), "2 apples");
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
fn digest_top_files_omitted_reports_cut_files() {
    let review = review_with(vec![
        file("a.rs", 10, 0),
        file("b.rs", 5, 0),
        file("c.rs", 1, 2),
    ]);

    let digest = build_digest(&review, 2);

    assert_eq!(digest.top_files.len(), 2);
    assert_eq!(digest.top_files_omitted.files, 1);
    assert_eq!(digest.top_files_omitted.add, 1);
    assert_eq!(digest.top_files_omitted.del, 2);
    assert_eq!(
        digest.top_files.len() + digest.top_files_omitted.files,
        digest.totals.files
    );
}

#[test]
fn digest_top_files_omitted_is_zero_when_nothing_is_cut() {
    let review = review_with(vec![file("a.rs", 1, 1), file("b.rs", 2, 2)]);

    let digest = build_digest(&review, 100);

    assert_eq!(digest.top_files_omitted.files, 0);
    assert_eq!(digest.top_files_omitted.add, 0);
    assert_eq!(digest.top_files_omitted.del, 0);
}

#[test]
fn digest_top_files_omitted_covers_budget_cut() {
    let files: Vec<FileEntry> = (0..2_000)
        .map(|index| file(&format!("dir{}/{}", index % 50, "x".repeat(140)), 1, 1))
        .collect();
    let review = review_with(files);

    let digest = build_digest(&review, 2_000);
    let json = serde_json::to_string(&digest).unwrap();

    assert!(json.len() < 100_000, "digest was {} bytes", json.len());
    assert!(digest.top_files.len() < 2_000);
    assert_eq!(
        digest.top_files.len() + digest.top_files_omitted.files,
        digest.totals.files
    );
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
