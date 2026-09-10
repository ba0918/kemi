//! コメントと suggestion（R-COMMENT）の検証と状態。

use crate::domain::review::{LineRange, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentError {
    LineNumberOutOfRange,
    ReversedRange,
    FileWideMustBeNewSide,
    SuggestionRequiresNewSide,
    SuggestionRequiresRange,
    QuoteOutOfBounds,
}

/// R-COMMENT の行レンジと suggestion の規約を検証する。
pub fn validate_comment(
    side: Side,
    range: Option<LineRange>,
    suggestion: Option<&str>,
) -> Result<(), CommentError> {
    match range {
        None => {
            if side != Side::New {
                return Err(CommentError::FileWideMustBeNewSide);
            }
            if suggestion.is_some() {
                return Err(CommentError::SuggestionRequiresRange);
            }
        }
        Some(range) => {
            if range.start < 1 {
                return Err(CommentError::LineNumberOutOfRange);
            }
            if range.end < range.start {
                return Err(CommentError::ReversedRange);
            }
            if suggestion.is_some() && side != Side::New {
                return Err(CommentError::SuggestionRequiresNewSide);
            }
        }
    }
    Ok(())
}

/// 指定した行レンジの行テキストを取り出す。範囲外は黙って縮めない。
pub fn quote_for(lines: &[String], range: LineRange) -> Result<Vec<String>, CommentError> {
    if range.start < 1 || range.end < range.start {
        return Err(CommentError::LineNumberOutOfRange);
    }
    if range.end as usize > lines.len() {
        return Err(CommentError::QuoteOutOfBounds);
    }
    Ok(lines[range.start as usize - 1..range.end as usize].to_vec())
}

/// 正規化済みの行内容のハッシュ。outdated 判定に使う（D6 のハッシュ選択）。
pub fn content_hash(lines: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for line in lines {
        for byte in line.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn is_outdated(created_hash: &str, current_hash: &str) -> bool {
    created_hash != current_hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{LineRange, Side};

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn comment_accepts_inclusive_range_starting_at_one() {
        let range = LineRange { start: 1, end: 3 };
        assert_eq!(validate_comment(Side::New, Some(range), None), Ok(()));
    }

    #[test]
    fn comment_rejects_line_number_zero() {
        let range = LineRange { start: 0, end: 1 };
        assert!(validate_comment(Side::New, Some(range), None).is_err());
    }

    #[test]
    fn comment_rejects_reversed_range() {
        let range = LineRange { start: 5, end: 2 };
        assert!(validate_comment(Side::New, Some(range), None).is_err());
    }

    #[test]
    fn comment_file_whole_must_be_new_side() {
        assert_eq!(validate_comment(Side::New, None, None), Ok(()));
        assert!(validate_comment(Side::Old, None, None).is_err());
    }

    #[test]
    fn comment_suggestion_is_allowed_on_new_side_range() {
        let range = LineRange { start: 2, end: 2 };
        assert_eq!(
            validate_comment(Side::New, Some(range), Some("replacement")),
            Ok(())
        );
    }

    #[test]
    fn comment_suggestion_is_rejected_on_old_side() {
        let range = LineRange { start: 2, end: 2 };
        assert!(validate_comment(Side::Old, Some(range), Some("replacement")).is_err());
    }

    #[test]
    fn comment_suggestion_is_rejected_on_file_whole() {
        assert!(validate_comment(Side::New, None, Some("replacement")).is_err());
    }

    #[test]
    fn comment_quote_returns_the_range_texts() {
        let content = lines(&["a", "b", "c"]);
        let quote = quote_for(&content, LineRange { start: 2, end: 3 }).unwrap();
        assert_eq!(quote, vec!["b", "c"]);
    }

    #[test]
    fn comment_quote_rejects_range_beyond_end_of_file() {
        let content = lines(&["a"]);
        assert!(quote_for(&content, LineRange { start: 1, end: 2 }).is_err());
    }

    #[test]
    fn comment_content_hash_is_stable_and_line_sensitive() {
        assert_eq!(
            content_hash(&lines(&["a", "b"])),
            content_hash(&lines(&["a", "b"]))
        );
        assert_ne!(
            content_hash(&lines(&["a", "b"])),
            content_hash(&lines(&["a", "c"]))
        );
    }

    #[test]
    fn comment_is_outdated_when_content_hash_differs() {
        let created = content_hash(&lines(&["a", "b"]));
        let changed = content_hash(&lines(&["a", "c"]));
        assert!(is_outdated(&created, &changed));
        assert!(!is_outdated(&created, &created));
    }
}
