use serde::Deserialize;

use crate::domain::conversation_content::ConversationContent;
use crate::domain::parts::Part;
use crate::domain::parts_validation::{PartError, bound, validate_parts};

#[derive(Debug)]
pub enum ContentError {
    Json(serde_json::Error),
    Structure(PartError),
}

impl std::fmt::Display for ContentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => error.fmt(f),
            Self::Structure(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ContentError {}

pub fn decode_content(input: &str) -> Result<ConversationContent, ContentError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PlainInput {
        body: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PartsInput {
        parts: Vec<Part>,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        Plain(PlainInput),
        Parts(PartsInput),
    }
    let value: Input = serde_json::from_str(input).map_err(ContentError::Json)?;
    Ok(match value {
        Input::Plain(value) => {
            bound("body UTF-8 bytes", value.body.len(), 0, 65_536)
                .map_err(ContentError::Structure)?;
            ConversationContent::Plain(value.body)
        }
        Input::Parts(value) => {
            validate_parts(&value.parts).map_err(ContentError::Structure)?;
            ConversationContent::Parts(value.parts)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixed_missing_null_unknown_and_duplicate_fields_are_refused() {
        for input in [
            r#"{}"#,
            r#"{"body":null}"#,
            r#"{"parts":null}"#,
            r#"{"body":"a","parts":null}"#,
            r#"{"body":null,"parts":[{"type":"text","body":"a"}]}"#,
            r#"{"body":"a","body":"b"}"#,
            r#"{"parts":[{"type":"text","body":"a","body":"b"}]}"#,
            r#"{"parts":[{"type":"text","type":"text","body":"a"}]}"#,
            r#"{"parts":[{"type":"text","body":"a","unknown":1}]}"#,
            r#"{"parts":[{"type":"code","body":"a","language":"x","role":"example","title":null}]}"#,
            r#"{"parts":[{"type":"code","body":"a","language":"x","role":"invalid"}]}"#,
            r#"{"parts":[{"type":"sequence","title":"s","role":"example","participants":[{"id":"a","label":"A","x":null}],"events":[{"type":"message","from":"a","to":"a","label":"m","kind":"call"}]}]}"#,
        ] {
            assert!(super::decode_content(input).is_err(), "{input}");
        }
    }

    #[test]
    fn omitted_and_explicit_defaults_are_distinct_content() {
        let omitted = super::decode_content(r#"{"parts":[{"type":"sequence","title":"s","role":"example","participants":[{"id":"a","label":"A"}],"events":[{"type":"message","from":"a","to":"a","label":"m","kind":"call"}]}]}"#).unwrap();
        let explicit = super::decode_content(r#"{"parts":[{"type":"sequence","title":"s","role":"example","layout":{"participant_gap":220},"participants":[{"id":"a","label":"A"}],"events":[{"type":"message","from":"a","to":"a","label":"m","kind":"call"}]}]}"#).unwrap();
        assert_ne!(omitted, explicit);
    }

    #[test]
    fn empty_parts_and_thirty_three_parts_are_refused() {
        for count in [0, 33] {
            let input = serde_json::json!({"parts":vec![serde_json::json!({"type":"text","body":""}); count]});
            assert!(super::decode_content(&input.to_string()).is_err());
        }
    }

    #[test]
    fn text_limit_counts_unicode_scalars_instead_of_utf8_bytes() {
        let input =
            |n| serde_json::json!({"parts":[{"type":"text","body":"日".repeat(n)}]}).to_string();
        assert!(super::decode_content(&input(16_384)).is_ok());
        assert!(super::decode_content(&input(16_385)).is_err());
    }

    #[test]
    fn plain_limit_counts_utf8_bytes_and_preserves_boundary() {
        let input = |s: String| serde_json::json!({"body":s}).to_string();
        assert!(super::decode_content(&input("日".repeat(21_845) + "x")).is_ok());
        assert!(super::decode_content(&input("日".repeat(21_845) + "xx")).is_err());
    }

    #[test]
    fn parts_budget_counts_repeated_roles_and_types_as_string_values() {
        let code = serde_json::json!({"type":"code","body":"a".repeat(32_768),"language":"x","role":"example"});
        let input = serde_json::json!({"parts":vec![code;16]});
        assert!(super::decode_content(&input.to_string()).is_err());
    }

    #[test]
    fn decision_edges_without_nonblank_conditions_are_refused() {
        let input = serde_json::json!({"parts":[{"type":"flow","title":"f","role":"example","nodes":[{"id":"a","kind":"decision","label":"?"}],"edges":[{"from":"a","to":"a"}]}]});
        assert!(super::decode_content(&input.to_string()).is_err());
    }

    #[test]
    fn messages_to_undefined_participants_are_refused() {
        let input = serde_json::json!({"parts":[{"type":"sequence","title":"s","role":"example","participants":[{"id":"a","label":"A"}],"events":[{"type":"message","from":"a","to":"b","label":"call","kind":"call"}]}]});
        assert!(super::decode_content(&input.to_string()).is_err());
    }

    #[test]
    fn grid_rejects_undefined_cells_duplicate_nodes_unequal_rows_and_lost_lines() {
        for source in [
            "a = A\n| b |",
            "a = A\na = B",
            "a = A\n| a |\n| a | a |",
            "a = A\u{2028}B",
            "a = A\rB",
            "a = A\u{2029}B",
            "unknown",
        ] {
            let input = serde_json::json!({"parts":[{"type":"diagram","source":source,"title":"g","role":"example"}]});
            assert!(
                super::decode_content(&input.to_string()).is_err(),
                "{source:?}"
            );
        }
    }

    fn roundtrip(part: serde_json::Value) {
        let input = serde_json::json!({"parts":[part.clone()]});
        let content = super::decode_content(&input.to_string()).unwrap();
        assert_eq!(
            serde_json::to_value(content).unwrap(),
            serde_json::json!({"body":null,"parts":[part]})
        );
    }

    #[test]
    fn code_keeps_literal_body_role_and_omitted_title() {
        roundtrip(
            serde_json::json!({"type":"code","body":"<script>\r\n","language":"pseudocode","role":"example"}),
        );
    }

    #[test]
    fn grid_keeps_source_and_role() {
        roundtrip(
            serde_json::json!({"type":"diagram","source":"a = 日本語\n| a |","title":"配置","role":"proposal"}),
        );
    }

    #[test]
    fn sequence_keeps_calls_returns_self_messages_and_frames() {
        roundtrip(
            serde_json::json!({"type":"sequence","title":"順序","role":"example","participants":[{"id":"a","label":"A"}],"events":[{"type":"message","from":"a","to":"a","label":"call","kind":"call"},{"type":"loop","condition":"repeat","messages":[{"type":"message","from":"a","to":"a","label":"return","kind":"return"}]},{"type":"alt","branches":[{"condition":"yes","messages":[{"type":"message","from":"a","to":"a","label":"yes","kind":"call"}]},{"condition":"no","messages":[{"type":"message","from":"a","to":"a","label":"no","kind":"return"}]}]}]}),
        );
    }

    #[test]
    fn flow_keeps_explicit_positions_ports_empty_via_and_condition() {
        roundtrip(
            serde_json::json!({"type":"flow","title":"条件","role":"confirmed","nodes":[{"id":"a","kind":"decision","label":"条件?","position":{"x":160,"y":100},"width":220,"height":96},{"id":"b","kind":"end","label":"終了"}],"edges":[{"from":"a","to":"b","from_port":"south","to_port":"north","via":[],"label":{"text":"yes","position":{"x":200,"y":180}}}],"canvas":{"width":640,"height":480}}),
        );
    }

    #[test]
    fn typed_text_parts_keep_their_content_and_order() {
        let content = super::decode_content(
            r#"{"parts":[{"type":"text","body":"**日本語**"},{"type":"text","body":""}]}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(content).unwrap(),
            serde_json::json!({"body":null,"parts":[{"type":"text","body":"**日本語**"},{"type":"text","body":""}]})
        );
    }

    #[test]
    fn old_plain_empty_body_is_preserved() {
        let content = super::decode_content(r#"{"body":""}"#).unwrap();
        assert_eq!(
            serde_json::to_value(content).unwrap(),
            serde_json::json!({"body":"", "parts":null})
        );
    }
}
