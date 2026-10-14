
use super::*;
use crate::source::testutil::TempRepo;

#[test]
fn worktree_watch_paths_lists_new_side_files() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.add_and_commit("base");
    repo.write("a.txt", "two\n");
    let source = GitSource::new(repo.path.clone(), GitMode::Worktree);
    source.review().unwrap();

    assert_eq!(source.watch_paths(), vec![repo.path.join("a.txt")]);
}

#[test]
fn only_the_worktree_mode_watches_its_work_tree() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.add_and_commit("base");

    let worktree = GitSource::new(repo.path.clone(), GitMode::Worktree);
    let staged = GitSource::new(repo.path.clone(), GitMode::Staged);

    assert_eq!(worktree.work_tree(), Some(WorkTree::new(repo.path.clone())));
    assert_eq!(staged.work_tree(), None);
}

#[test]
fn ref_watch_paths_point_at_ref_files_not_their_directories() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.add_and_commit("base");
    let branch = repo.git(&["symbolic-ref", "--short", "HEAD"]);

    let paths = ref_watch_paths(&repo.path, "HEAD").unwrap();

    assert!(
        paths.iter().all(|path| !path.is_dir()),
        "watch paths must be files only: {paths:?}"
    );
    assert!(paths.iter().any(|path| path.ends_with("HEAD")));
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with(format!("refs/heads/{branch}"))),
        "the resolved branch ref must be watched: {paths:?}"
    );
}
