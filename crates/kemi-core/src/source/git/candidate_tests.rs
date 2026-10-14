
use super::*;
use crate::source::testutil::TempRepo;

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
    let expected: Vec<_> = sides.iter().map(side_size).collect();
    assert_eq!(side_sizes(&sides.iter().collect::<Vec<_>>()), expected);
    let other = TempRepo::new();
    std::fs::write(other.path.join("plain.bin"), [0; 5]).unwrap();
    other.add_and_commit("other repository");
    sides.push(SideRef::Git {
        repo: other.path.clone(),
        spec: OsString::from("HEAD:plain.bin"),
    });
    let expected: Vec<_> = sides.iter().map(side_size).collect();
    assert_eq!(side_sizes(&sides.iter().collect::<Vec<_>>()), expected);
}

#[test]
fn unaccepted_candidate_cannot_replace_published_content_references() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.add_and_commit("base");
    repo.write("a.txt", "two\n");
    let factory = GitSource::new(repo.path.clone(), GitMode::Worktree);
    let published = factory.fork().unwrap().unwrap();
    published.review().unwrap();
    repo.write("b.txt", "new\n");
    let candidate = factory.fork().unwrap().unwrap();
    let meta = candidate.review().unwrap();
    let added = meta.groups[0]
        .files
        .iter()
        .find(|f| f.path == "b.txt")
        .unwrap();
    assert!(matches!(
        published.content(&added.id),
        Err(SourceError::UnknownFileId(_))
    ));
    assert_eq!(candidate.content(&added.id).unwrap().new.unwrap(), b"new\n");
}

#[test]
fn deferred_opposite_unit_uses_the_range_captured_for_its_display_generation() {
    let repo = TempRepo::new();
    repo.write("a.txt", "one\n");
    repo.add_and_commit("base");
    let base = git_text(&repo.path, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    repo.write("a.txt", "two\n");
    repo.add_and_commit("D1");
    let factory = GitSource::new(
        repo.path.clone(),
        GitMode::Range {
            from: base,
            to: "HEAD".into(),
            group_by: GroupBy::File,
        },
    );
    let d1 = factory.fork().unwrap().unwrap();
    d1.review().unwrap();
    repo.write("b.txt", "three\n");
    repo.add_and_commit("D2");
    let d2 = factory.fork().unwrap().unwrap();
    d2.review().unwrap();
    assert_eq!(d1.review_unit(GroupBy::Commit).unwrap().groups.len(), 1);
    assert_eq!(d2.review_unit(GroupBy::Commit).unwrap().groups.len(), 2);
}
