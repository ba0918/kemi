//! セッション状態（R-SERVE）。コメント、見た、折りたたみ、解決をメモリに持つ。

use std::collections::{BTreeMap, BTreeSet};

use kemi_core::domain::review::Comment;
use serde_json::json;

#[derive(Default)]
pub struct Session {
    pub comments: Vec<Comment>,
    pub seen: BTreeSet<String>,
    pub collapsed: BTreeMap<String, bool>,
    pub next_comment: u32,
}

/// R-SUBMIT の契約に合わせたコメントの JSON。`content_hash` は出さない。
pub fn comment_json(comment: &Comment) -> serde_json::Value {
    json!({
        "id": comment.id,
        "group_id": comment.group_id,
        "group_title": comment.group_title,
        "path": comment.path,
        "side": comment.side.as_str(),
        "start_line": comment.start_line,
        "end_line": comment.end_line,
        "quote": comment.quote,
        "body": comment.body,
        "replies": comment.replies,
        "resolved": comment.resolved,
        "outdated": comment.outdated,
        "suggestion": comment
            .suggestion
            .as_ref()
            .map(|suggestion| json!({ "replacement": suggestion.replacement })),
    })
}
