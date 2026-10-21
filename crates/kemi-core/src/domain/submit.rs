//! submit の結果の契約（R-SUBMIT / R-RESULT）。

use super::explanation::{ExplanationHistory, ExplanationVersion};
use super::review::{Comment, CommentTarget};

/// 結果 JSON の契約の版（`kemi`）。wait の出力（R-AGENT-EVENTS）も同じ版を名乗るので、
/// 片方だけ上げると契約が食い違う。説明の context は別に数える（`CONTEXT_CONTRACT_VERSION`）。
pub const RESULT_CONTRACT_VERSION: u32 = 4;

/// submit で示す判断。終了コードは承認 0 / 変更要求 1（R-SUBMIT）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Approved,
    ChangesRequested,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::ChangesRequested => "changes_requested",
        }
    }

    pub fn parse(verdict: &str) -> Option<Self> {
        match verdict {
            "approved" => Some(Verdict::Approved),
            "changes_requested" => Some(Verdict::ChangesRequested),
            _ => None,
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Verdict::Approved => 0,
            Verdict::ChangesRequested => 1,
        }
    }
}

/// 結果に載せる説明の版（R-EXPLANATION-RESULT）。説明はエージェント自身が書いて手元に
/// 持っているので、説明へのコメントが 1 つも無ければ、結果を読むのに説明の本文は要らず空にする。
/// 1 つ以上あれば、説明を参照するコメントを結果だけで読めるよう、保持中の版をすべて保存値の
/// まま返す。どの旧版を保持するかは保存の規則（R-EXPLANATION-SESSION）で決まり、ここで
/// 参照されていない旧版を除くことはしない。
pub fn result_explanation_versions<'a>(
    history: &'a ExplanationHistory,
    comments: &[Comment],
) -> &'a [ExplanationVersion] {
    let on_explanation = comments.iter().any(|comment| match comment.target {
        CommentTarget::Explanation(_) => true,
        CommentTarget::File(_) | CommentTarget::Page(_) => false,
    });
    if on_explanation {
        &history.versions
    } else {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::explanation::{ExplanationDocument, quote_block};
    use crate::domain::review::{ExplanationTarget, PageTarget};
    use crate::testutil::file_comment;

    fn document(body: &str) -> ExplanationDocument {
        serde_json::from_value(
            serde_json::json!({"sections":[{"id":"s","title":"Section","blocks":[
                {"id":"b","part":{"type":"text","body":body},"references":[]}
            ]}]}),
        )
        .unwrap()
    }

    /// 版 1 と版 2 を保持し、今の版が 2 の説明。
    fn history() -> ExplanationHistory {
        ExplanationHistory {
            current_revision: 2,
            versions: [(1, "first"), (2, "second")]
                .into_iter()
                .map(|(revision, body)| ExplanationVersion {
                    revision,
                    base_display_version: "01K00000000000000000000000:1".into(),
                    sections: document(body).sections,
                })
                .collect(),
        }
    }

    fn explanation_comment(id: &str, revision: u64) -> Comment {
        Comment {
            target: CommentTarget::Explanation(ExplanationTarget {
                revision,
                block_id: "b".into(),
                quote: quote_block(&document("first"), "b").unwrap(),
                selection: None,
            }),
            ..file_comment(id)
        }
    }

    fn page_comment(id: &str) -> Comment {
        Comment {
            target: CommentTarget::Page(PageTarget {
                url: "/".into(),
                width: 390,
                places: Vec::new(),
                image: None,
            }),
            ..file_comment(id)
        }
    }

    #[test]
    fn a_result_without_explanation_comments_carries_no_explanation_versions() {
        let history = history();

        assert!(result_explanation_versions(&history, &[]).is_empty());
        assert!(
            result_explanation_versions(&history, &[file_comment("c1"), page_comment("c2")])
                .is_empty()
        );
    }

    /// 参照されていない旧版も除かず、保持中の版を保存値のまま並びも変えずに返す。
    #[test]
    fn one_explanation_comment_carries_every_retained_version() {
        let history = history();
        let comments = [file_comment("c1"), explanation_comment("c2", 2)];

        assert_eq!(
            result_explanation_versions(&history, &comments),
            history.versions.as_slice()
        );
    }
}
