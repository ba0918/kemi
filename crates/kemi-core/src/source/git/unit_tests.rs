use super::*;
use crate::source::{FileContent, FocusSource};
use crate::testutil::TempRepo;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 3 コミットの範囲。`b.txt` は範囲の途中で足して消すので、最終形には出ない。
fn range_repo() -> (TempRepo, String, Vec<String>) {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.add_and_commit("base");
    repo.write("a.txt", "two\n");
    repo.write("b.txt", "temporary\n");
    let first = repo.add_and_commit("feat: a and b");
    repo.remove("b.txt");
    let second = repo.add_and_commit("fix: drop b");
    repo.write("a.txt", "three\n");
    let third = repo.add_and_commit("feat: a again");
    (repo, base, vec![first, second, third])
}

fn range_source(repo: &TempRepo, from: &str, group_by: GroupBy) -> GitSource {
    GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: from.to_string(),
            to: "HEAD".to_string(),
            group_by,
        },
    )
}

fn ids_by_group_and_path(review: &ReviewMeta) -> Vec<(String, String, String)> {
    review
        .groups
        .iter()
        .flat_map(|group| {
            group
                .files
                .iter()
                .map(|file| (group.id.clone(), file.path.clone(), file.id.clone()))
        })
        .collect()
}

#[test]
fn unit_both_units_serve_content_from_one_source() {
    let (repo, base, commits) = range_repo();
    let source = range_source(&repo, &base, GroupBy::File);

    let final_form = source.review_unit(GroupBy::File).unwrap();
    let per_commit = source.review_unit(GroupBy::Commit).unwrap();

    assert_eq!(final_form.groups.len(), 1);
    assert_eq!(final_form.groups[0].id, "all");
    let group_ids: Vec<&str> = per_commit
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    assert_eq!(
        group_ids,
        commits.iter().map(String::as_str).collect::<Vec<_>>()
    );
    let final_a = &final_form.groups[0].files[0];
    let first_a = &per_commit.groups[0].files[0];
    assert_ne!(final_a.id, first_a.id);
    assert_eq!(
        source.content(&final_a.id).unwrap().new.unwrap(),
        b"three\n"
    );
    assert_eq!(source.content(&first_a.id).unwrap().new.unwrap(), b"two\n");
}

#[test]
fn unit_refetch_keeps_other_unit_content_reachable() {
    let (repo, base, _) = range_repo();
    let source = range_source(&repo, &base, GroupBy::Commit);
    let per_commit = source.review_unit(GroupBy::Commit).unwrap();
    source.review_unit(GroupBy::File).unwrap();

    source.review_unit(GroupBy::File).unwrap();

    for (_, _, id) in ids_by_group_and_path(&per_commit) {
        assert!(source.content(&id).is_ok(), "{id} became unreachable");
    }
}

#[test]
fn unit_ids_stable_across_refetch_of_both_units() {
    let (repo, base, _) = range_repo();
    let source = range_source(&repo, &base, GroupBy::File);
    let first = (
        source.review_unit(GroupBy::File).unwrap(),
        source.review_unit(GroupBy::Commit).unwrap(),
    );

    let second = (
        source.review_unit(GroupBy::Commit).unwrap(),
        source.review_unit(GroupBy::File).unwrap(),
    );

    assert_eq!(
        ids_by_group_and_path(&first.0),
        ids_by_group_and_path(&second.1)
    );
    assert_eq!(
        ids_by_group_and_path(&first.1),
        ids_by_group_and_path(&second.0)
    );
}

#[test]
fn unit_commit_groups_keep_subject_and_body_in_range_order() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    let base = repo.add_and_commit("base");
    repo.write("a.txt", "two\n");
    repo.add_and_commit("feat: two\n\nwhy it changed\nsecond line");
    repo.write("a.txt", "three\n");
    repo.add_and_commit("fix: three");
    let source = range_source(&repo, &base, GroupBy::Commit);

    let review = source.review().unwrap();

    let titles: Vec<(&str, &str)> = review
        .groups
        .iter()
        .map(|group| (group.title.as_str(), group.why.as_str()))
        .collect();
    assert_eq!(
        titles,
        vec![
            ("feat: two", "why it changed\nsecond line"),
            ("fix: three", "")
        ]
    );
}

#[test]
fn unit_units_list_startup_unit_first_only_for_ranges() {
    let (repo, base, _) = range_repo();

    assert_eq!(
        range_source(&repo, &base, GroupBy::File).units(),
        vec![GroupBy::File, GroupBy::Commit]
    );
    assert_eq!(
        range_source(&repo, &base, GroupBy::Commit).units(),
        vec![GroupBy::Commit, GroupBy::File]
    );
    assert!(
        GitSource::new(repo.path.clone(), GitMode::Worktree)
            .units()
            .is_empty()
    );
}

fn focus_source(inner: Box<dyn ReviewSource>, repo: &TempRepo, json: &str) -> FocusSource {
    repo.write("focus.json", json);
    FocusSource::from_path(inner, Path::new("focus.json"), &repo.path).unwrap()
}

#[test]
fn focus_accepts_commit_group_id_at_final_startup() {
    let (repo, base, commits) = range_repo();
    let json = format!(r#"{{"groups":{{"{}":{{"watch":"ここ"}}}}}}"#, commits[1]);
    let source = focus_source(
        Box::new(range_source(&repo, &base, GroupBy::File)),
        &repo,
        &json,
    );

    source.review().unwrap();
    let per_commit = source.review_unit(GroupBy::Commit).unwrap();

    let group = per_commit
        .groups
        .iter()
        .find(|group| group.id == commits[1])
        .unwrap();
    assert_eq!(group.watch, "ここ");
}

#[test]
fn focus_accepts_path_removed_within_range_and_marks_both_units() {
    let (repo, base, _) = range_repo();
    let source = focus_source(
        Box::new(range_source(&repo, &base, GroupBy::File)),
        &repo,
        r#"{"files":[{"path":"b.txt","note":"途中で消えた"},{"path":"a.txt"}]}"#,
    );

    let final_form = source.review().unwrap();
    let per_commit = source.review_unit(GroupBy::Commit).unwrap();

    assert!(final_form.groups[0].files[0].focus);
    let b_entries: Vec<&FileEntry> = per_commit
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .filter(|file| file.path == "b.txt")
        .collect();
    assert_eq!(b_entries.len(), 2);
    assert!(
        b_entries
            .iter()
            .all(|file| file.focus && file.note == "途中で消えた")
    );
}

#[test]
fn focus_rejects_unknown_path_in_range() {
    let (repo, base, _) = range_repo();
    let source = focus_source(
        Box::new(range_source(&repo, &base, GroupBy::File)),
        &repo,
        r#"{"files":[{"path":"missing.txt"}]}"#,
    );

    let message = source.review().unwrap_err().to_string();

    assert!(message.contains("missing.txt"), "{message}");
}

#[test]
fn focus_rejects_unknown_group_id_in_range() {
    let (repo, base, _) = range_repo();
    let source = focus_source(
        Box::new(range_source(&repo, &base, GroupBy::Commit)),
        &repo,
        r#"{"groups":{"0000000":{"watch":"x"}}}"#,
    );

    assert!(source.review().is_err());
}

/// 内容の取得（`content`）の呼び出しを数える。
struct CountingSource {
    inner: GitSource,
    reads: Arc<AtomicUsize>,
}

impl ReviewSource for CountingSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        self.inner.review()
    }

    fn content(&self, file_id: &str) -> Result<FileContent, SourceError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.content(file_id)
    }

    fn units(&self) -> Vec<GroupBy> {
        self.inner.units()
    }

    fn review_unit(&self, unit: GroupBy) -> Result<ReviewMeta, SourceError> {
        self.inner.review_unit(unit)
    }

    fn extra_focus_targets(&self) -> Result<FocusTargets, SourceError> {
        self.inner.extra_focus_targets()
    }
}

#[test]
fn unit_startup_reads_no_content_even_with_focus() {
    let (repo, base, commits) = range_repo();
    for group_by in [GroupBy::File, GroupBy::Commit] {
        let reads = Arc::new(AtomicUsize::new(0));
        let counting = CountingSource {
            inner: range_source(&repo, &base, group_by),
            reads: reads.clone(),
        };
        let json = format!(
            r#"{{"groups":{{"{}":{{"watch":"w"}}}},"files":[{{"path":"b.txt"}}]}}"#,
            commits[0]
        );
        let source = focus_source(Box::new(counting), &repo, &json);

        source.review().unwrap();

        assert_eq!(reads.load(Ordering::SeqCst), 0, "{group_by:?}");
    }
}
