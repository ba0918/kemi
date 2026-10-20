//! セッションの保存のテストが共有する補助。

use std::collections::BTreeMap;

use super::super::*;
use crate::domain::content::FileContent;
use crate::domain::review::{Author, GroupBy, Reply, ReviewMeta, Suggestion};
use crate::session::files::{PageSnapshot, SnapshotKind};
use crate::session::{FrozenUnit, SessionMode};
use crate::testutil::{file_comment, file_entry, file_target};

pub(super) use crate::testutil::Scratch;

impl Scratch {
    /// セッションの置き場所。一時ディレクトリの下の `sessions/` で、まだ作らない。
    pub(in crate::session) fn dir(&self) -> PathBuf {
        self.0.join("sessions")
    }
}

pub(super) fn info(id: &str, updated: u128) -> SessionInfo {
    SessionInfo {
        id: id.to_string(),
        created: 1,
        updated,
        workspace: PathBuf::from("/tmp/workspace"),
        workspace_key: "00000000000000aa".to_string(),
        mode: SessionMode::Worktree,
        title: "Working tree changes".to_string(),
        total_files: 2,
    }
}

pub(in crate::session) fn comment() -> crate::domain::review::Comment {
    crate::domain::review::Comment {
        group_id: "all".to_string(),
        group_title: "final".to_string(),
        body: "please change".to_string(),
        replies: vec![Reply {
            id: "r1".to_string(),
            seq: 0,
            author: Author::Reviewer,
            body: "done".into(),
        }],
        resolved: true,
        target: crate::domain::review::CommentTarget::File(crate::domain::review::FileTarget {
            end_line: Some(2),
            quote: vec!["one".to_string(), "two".to_string()],
            suggestion: Some(Suggestion {
                replacement: "replaced".to_string(),
            }),
            ..file_target()
        }),
        ..file_comment("c1")
    }
}

pub(in crate::session) fn state_with_comment() -> SessionState {
    SessionState {
        comments: vec![comment()],
        seen: ["f1".to_string()].into_iter().collect(),
        collapsed: [("f1".to_string(), true)].into_iter().collect(),
        last_comment: 1,
        last_reply: 1,
        ..SessionState::default()
    }
}

pub(super) fn review() -> ReviewMeta {
    ReviewMeta {
        title: "Working tree changes".to_string(),
        subtitle: "sub".to_string(),
        meta: serde_json::json!({ "branch": "main" }),
        groups: vec![crate::domain::review::Group {
            id: "all".to_string(),
            title: "final".to_string(),
            why: "why".to_string(),
            watch: "watch".to_string(),
            files: vec![crate::domain::review::FileEntry {
                group_id: "all".to_string(),
                add: 1,
                del: 1,
                note: "note".to_string(),
                // 復元でも「内容を読まなかった」判定が同じになるよう、写しに残る。
                content_skipped: true,
                ..file_entry("src/a.rs")
            }],
        }],
        approval: vec![crate::domain::review::Approval {
            path: "src/a.rs".to_string(),
            identity: "sha256:abc".to_string(),
        }],
    }
}

pub(super) fn content(old: Option<&[u8]>, new: Option<&[u8]>) -> FileContent {
    FileContent {
        old: old.map(<[u8]>::to_vec),
        new: new.map(<[u8]>::to_vec),
    }
}

pub(super) fn copy() -> SessionCopy {
    SessionCopy {
        startup_unit: Some(GroupBy::File),
        units: vec![FrozenUnit {
            unit: Some(GroupBy::File),
            review: review(),
        }],
        contents: BTreeMap::from([
            (
                "f1".to_string(),
                content(Some(&[0xff, 0xfe]), Some(b"hello\n")),
            ),
            ("f2".to_string(), content(None, Some(b"added"))),
        ]),
    }
}

pub(super) fn files_dir(scratch: &Scratch, id: &str) -> PathBuf {
    scratch.dir().join(format!("{id}.files"))
}

pub(super) fn page_snapshot(number: u32, kind: SnapshotKind, html: &str) -> PageSnapshot {
    PageSnapshot {
        number,
        kind,
        page: "/products?x=1".to_string(),
        width: 390,
        html: html.to_string(),
        description: Some(serde_json::json!("H4sIAAAA")),
    }
}

pub(super) fn no_comments() -> BTreeSet<String> {
    BTreeSet::new()
}

pub(super) fn saved_path(write: &FilesWrite) -> PathBuf {
    match &write.written {
        FileWritten::Saved(path) => path.clone(),
        other @ (FileWritten::NoRoom | FileWritten::Failed(_)) => {
            panic!("not saved: {other:?}")
        }
    }
}

pub(super) fn incompressible(size: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_9abc_def0u64;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xff) as u8
        })
        .collect()
}

pub(super) fn prepared(copy: SessionCopy) -> crate::session::PreparedCopy {
    crate::session::prepare_copy(copy).unwrap()
}

pub(super) fn reply(id: &str, author: Author, body: &str) -> Reply {
    Reply {
        id: id.to_string(),
        seq: 0,
        author,
        body: body.into(),
    }
}

pub(super) fn live_info(id: &str, updated: u128) -> SessionInfo {
    SessionInfo {
        mode: SessionMode::Live {
            page: crate::domain::live::LivePage::Url("http://127.0.0.1:5173/".to_string()),
            root: PathBuf::from("/tmp/workspace"),
        },
        title: "Live review of http://127.0.0.1:5173/".to_string(),
        ..info(id, updated)
    }
}
