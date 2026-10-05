//! レビューのドメイン型。入力モード（manifest / コミット範囲 / worktree / staged）に
//! よらず共通で使う。

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Add,
    Delete,
    Rename,
    Modify,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Add => "add",
            Status::Delete => "delete",
            Status::Rename => "rename",
            Status::Modify => "modify",
        }
    }
}

/// コミット範囲のグループ単位（R-UNIT）。最終形（`file`）とコミットごと（`commit`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupBy {
    Commit,
    File,
}

impl GroupBy {
    pub fn as_str(self) -> &'static str {
        match self {
            GroupBy::Commit => "commit",
            GroupBy::File => "file",
        }
    }

    /// コミット範囲の、もう片方の単位。
    pub fn other(self) -> GroupBy {
        match self {
            GroupBy::Commit => GroupBy::File,
            GroupBy::File => GroupBy::Commit,
        }
    }
}

/// コメントや suggestion が指す側。行番号はこの側のファイルのもの。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Old => "old",
            Side::New => "new",
        }
    }
}

/// 1 始まり、両端を含む行レンジ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub replacement: String,
}

/// 返信と発言を書いた人。人間の中は区別しない（P5）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Author {
    Reviewer,
    Agent,
}

impl Author {
    pub fn as_str(self) -> &'static str {
        match self {
            Author::Reviewer => "reviewer",
            Author::Agent => "agent",
        }
    }
}

/// スレッドの中の書き込み。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub id: String,
    /// 書いた順をレビュー全体で比べる通し番号（R-SESSION）。
    pub seq: u32,
    pub author: Author,
    pub body: String,
}

/// どのコメントにも属さない、レビュー全体への書き込み。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    /// 書いた順をレビュー全体で比べる通し番号（R-SESSION）。
    pub seq: u32,
    pub author: Author,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    /// 書いた順をレビュー全体で比べる通し番号（R-SESSION）。作成のときにだけ振る。
    pub seq: u32,
    pub file_id: String,
    pub group_id: String,
    pub group_title: String,
    pub path: String,
    pub side: Side,
    /// ファイル全体のコメントでは None。
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub quote: Vec<String>,
    pub body: String,
    /// 作成順。
    pub replies: Vec<Reply>,
    pub resolved: bool,
    pub outdated: bool,
    /// 作成時の内容ハッシュ。現在のハッシュと違えば outdated。
    pub content_hash: String,
    pub suggestion: Option<Suggestion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub id: String,
    pub group_id: String,
    pub path: String,
    pub old_path: Option<String>,
    pub status: Status,
    pub add: u32,
    pub del: u32,
    pub binary: bool,
    pub old_size: u64,
    pub new_size: u64,
    pub focus: bool,
    pub note: String,
    pub noise: bool,
    /// 上限を超えて内容を読まなかった untracked（R-INPUT-3）。内容が無いので、
    /// 描画表示の対象にしない（R-RENDER）。worktree 以外の入力では常に false。
    pub content_skipped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub title: String,
    pub why: String,
    pub watch: String,
    pub files: Vec<FileEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub path: String,
    pub identity: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReviewMeta {
    pub title: String,
    pub subtitle: String,
    pub meta: Value,
    pub groups: Vec<Group>,
    pub approval: Vec<Approval>,
}
