use super::*;
use crate::domain::explanation::ReferenceUnit;
use crate::testutil::TempRepo;

#[test]
fn opposite_unit_references_keep_group_path_and_side_without_building_a_plan() {
    let repo = TempRepo::new();
    repo.write("keep.txt", "keep\n");
    repo.write("old.txt", "old\n");
    let base = repo.add_and_commit("base");
    repo.write("first.txt", "first\n");
    let first = repo.add_and_commit("first");
    repo.write("second.txt", "second\n");
    let second = repo.add_and_commit("second");
    let source = GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base,
            to: second,
            group_by: GroupBy::File,
        },
    );
    let candidate = source.detached().unwrap();
    let code = candidate
        .select_references(&[
            (ReferenceUnit::Commit, first.clone(), "first.txt".into()),
            (ReferenceUnit::Commit, first.clone(), "second.txt".into()),
            (ReferenceUnit::Commit, first, "keep.txt".into()),
        ])
        .unwrap();
    assert_eq!(code.len(), 1);
    assert_eq!(code[0].path, "first.txt");
    assert!(!code[0].old_present && code[0].new_present);
    assert_eq!(code[0].new_lines, None);
    assert!(candidate.store.state.lock().unwrap().is_empty());
    assert_eq!(candidate.ids.lock().unwrap().next, 0);
}

#[test]
fn references_to_merge_commits_are_not_per_commit_groups_even_when_the_path_is_in_their_tree() {
    let repo = TempRepo::new();
    repo.write("base.txt", "base\n");
    let base = repo.add_and_commit("base");
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("topic.txt", "topic\n");
    let topic = repo.add_and_commit("topic");
    repo.git(&["checkout", "-q", "-"]);
    repo.write("main.txt", "main\n");
    repo.add_and_commit("main");
    repo.git(&["merge", "--no-ff", "-q", "topic", "-m", "merge"]);
    let merged = repo.head();
    let source = GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base,
            to: merged.clone(),
            group_by: GroupBy::File,
        },
    )
    .detached()
    .unwrap();
    let entries = source
        .select_references(&[
            (ReferenceUnit::Commit, merged, "topic.txt".into()),
            (ReferenceUnit::Commit, topic.clone(), "topic.txt".into()),
        ])
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].group_id, topic);
}

#[test]
fn selective_references_reject_wrong_groups_and_keep_rename_add_delete_sides() {
    let repo = TempRepo::new();
    repo.write("renamed.txt", "rename me\n");
    repo.write("deleted.txt", "delete me\n");
    repo.write("unchanged.txt", "unchanged\n");
    let base = repo.add_and_commit("base");
    repo.git(&["mv", "renamed.txt", "new-name.txt"]);
    repo.remove("deleted.txt");
    repo.write("added.txt", "added\n");
    let change = repo.add_and_commit("change");
    let source = GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base.clone(),
            to: change.clone(),
            group_by: GroupBy::Commit,
        },
    )
    .detached()
    .unwrap();
    let requested: Vec<_> = [
        "new-name.txt",
        "renamed.txt",
        "deleted.txt",
        "added.txt",
        "unchanged.txt",
    ]
    .iter()
    .map(|path| (ReferenceUnit::File, "all".into(), (*path).into()))
    .chain(std::iter::once((
        ReferenceUnit::Commit,
        base,
        "added.txt".into(),
    )))
    .chain(std::iter::once((
        ReferenceUnit::File,
        change,
        "added.txt".into(),
    )))
    .collect();
    let entries = source.select_references(&requested).unwrap();
    assert_eq!(entries.len(), 3);
    for entry in entries {
        match entry.path.as_str() {
            "new-name.txt" => assert!(entry.old_present && entry.new_present),
            "deleted.txt" => assert!(entry.old_present && !entry.new_present),
            "added.txt" => assert!(!entry.old_present && entry.new_present),
            other => panic!("unexpected entry {other}"),
        }
    }
}

#[cfg(unix)]
#[test]
fn selective_references_preserve_ambiguous_lossy_paths_for_the_validator_to_reject() {
    use std::os::unix::ffi::OsStrExt;
    let repo = TempRepo::new();
    repo.write("base.txt", "base\n");
    let base = repo.add_and_commit("base");
    for path in [b"b\xffad.txt", b"b\xfead.txt"] {
        std::fs::write(
            repo.path.join(std::ffi::OsStr::from_bytes(path)),
            "content\n",
        )
        .unwrap();
    }
    let head = repo.add_and_commit("paths");
    let source = GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base,
            to: head,
            group_by: GroupBy::Commit,
        },
    )
    .detached()
    .unwrap();
    let entries = source
        .select_references(&[(ReferenceUnit::File, "all".into(), "b�ad.txt".into())])
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, entries[1].path);
}
