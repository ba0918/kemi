//! コメントと suggestion（R-COMMENT）の検証と状態。

use crate::domain::fnv::Fnv1a;
use crate::domain::review::{Comment, LineRange, Place, Side};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceError {
    NoPlace,
    NumberFromOne,
    DuplicateNumber(u32),
    /// 番号が 1 からの連番でない。値は抜けている番号。
    MissingNumber(u32),
}

/// ページへのコメントの場所の規約（live.md の R-PAGE-COMMENT）を検証し、番号の順に並べて返す。
/// 場所は 1 つ以上で、番号は 1 からの連番（画面は場所を消すと詰め直す。R-SUBMIT の `places`）。
pub fn validate_places(mut places: Vec<Place>) -> Result<Vec<Place>, PlaceError> {
    if places.is_empty() {
        return Err(PlaceError::NoPlace);
    }
    places.sort_by_key(|place| place.n);
    if places[0].n == 0 {
        return Err(PlaceError::NumberFromOne);
    }
    if let Some(pair) = places.windows(2).find(|pair| pair[0].n == pair[1].n) {
        return Err(PlaceError::DuplicateNumber(pair[0].n));
    }
    if let Some((expected, _)) = (1u32..)
        .zip(&places)
        .find(|(expected, place)| place.n != *expected)
    {
        return Err(PlaceError::MissingNumber(expected));
    }
    Ok(places)
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
    let mut hash = Fnv1a::new();
    for line in lines {
        hash.write(line.as_bytes());
        hash.write(b"\n");
    }
    hash.finish_hex()
}

pub fn is_outdated(created_hash: &str, current_hash: &str) -> bool {
    created_hash != current_hash
}

/// `file_id` のファイルへの行コメントの outdated を、今の内容に合わせて付け直す（R-COMMENT）。
/// `current` は今の旧側と新側の行。一覧から消えて内容を引けないファイルは None で、
/// そのコメントはすべて古い扱いにする。ハッシュはコメントのある側だけを 1 回ずつ求める。
pub fn refresh_outdated(
    comments: &mut [Comment],
    file_id: &str,
    current: Option<(&[String], &[String])>,
) {
    let mut old_hash = None;
    let mut new_hash = None;
    for comment in comments.iter_mut() {
        let Some(file) = comment.file().filter(|file| file.file_id == file_id) else {
            continue;
        };
        let outdated = current.is_none_or(|(old, new)| {
            let current_hash = match file.side {
                Side::Old => old_hash.get_or_insert_with(|| content_hash(old)),
                Side::New => new_hash.get_or_insert_with(|| content_hash(new)),
            };
            is_outdated(&file.content_hash, current_hash)
        });
        comment.outdated = outdated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{
        Comment, CommentTarget, FileTarget, LineRange, Place, PlaceKind, Side,
    };
    use crate::testutil::{file_comment, file_target};

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
        assert_eq!(
            validate_comment(Side::New, Some(range), None),
            Err(CommentError::LineNumberOutOfRange)
        );
        let range = LineRange { start: 1, end: 1 };
        assert_eq!(validate_comment(Side::New, Some(range), None), Ok(()));
    }

    #[test]
    fn comment_rejects_reversed_range() {
        let range = LineRange { start: 5, end: 2 };
        assert_eq!(
            validate_comment(Side::New, Some(range), None),
            Err(CommentError::ReversedRange)
        );
        let range = LineRange { start: 5, end: 5 };
        assert_eq!(validate_comment(Side::New, Some(range), None), Ok(()));
    }

    #[test]
    fn comment_file_whole_must_be_new_side() {
        assert_eq!(validate_comment(Side::New, None, None), Ok(()));
        assert_eq!(
            validate_comment(Side::Old, None, None),
            Err(CommentError::FileWideMustBeNewSide)
        );
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
        assert_eq!(
            validate_comment(Side::Old, Some(range), Some("replacement")),
            Err(CommentError::SuggestionRequiresNewSide)
        );
        assert_eq!(validate_comment(Side::Old, Some(range), None), Ok(()));
    }

    #[test]
    fn comment_suggestion_is_rejected_on_file_whole() {
        assert_eq!(
            validate_comment(Side::New, None, Some("replacement")),
            Err(CommentError::SuggestionRequiresRange)
        );
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
        assert_eq!(
            quote_for(&content, LineRange { start: 1, end: 2 }),
            Err(CommentError::QuoteOutOfBounds)
        );
        assert_eq!(
            quote_for(&content, LineRange { start: 1, end: 1 }),
            Ok(lines(&["a"]))
        );
    }

    #[test]
    fn comment_content_hash_is_stable_line_sensitive_and_decides_outdated() {
        let created = content_hash(&lines(&["a", "b"]));
        let same = content_hash(&lines(&["a", "b"]));
        let changed = content_hash(&lines(&["a", "c"]));

        assert_eq!(created, same);
        assert_ne!(created, changed);
        assert!(!is_outdated(&created, &same));
        assert!(is_outdated(&created, &changed));
    }

    /// `file_id` の `side` 側への行コメント。作成時の内容は `created`。
    fn line_comment(id: &str, file_id: &str, side: Side, created: &[&str]) -> Comment {
        Comment {
            target: CommentTarget::File(FileTarget {
                file_id: file_id.to_string(),
                side,
                content_hash: content_hash(&lines(created)),
                ..file_target()
            }),
            ..file_comment(id)
        }
    }

    fn outdated(comments: &[Comment]) -> Vec<(&str, bool)> {
        comments
            .iter()
            .map(|comment| (comment.id.as_str(), comment.outdated))
            .collect()
    }

    #[test]
    fn refreshing_marks_a_line_comment_outdated_only_when_its_own_side_changed() {
        let mut comments = vec![
            line_comment("c1", "f1", Side::Old, &["a"]),
            line_comment("c2", "f1", Side::New, &["b"]),
            // 別のファイルのコメントは、作成時と内容が違っても触らない。
            line_comment("c3", "f2", Side::New, &["x"]),
        ];
        comments[0].outdated = true;

        let (old, new) = (lines(&["a"]), lines(&["b", "changed"]));
        refresh_outdated(&mut comments, "f1", Some((&old, &new)));

        assert_eq!(
            outdated(&comments),
            [("c1", false), ("c2", true), ("c3", false)]
        );
    }

    #[test]
    fn refreshing_a_file_missing_from_the_review_marks_all_its_comments_outdated() {
        let mut comments = vec![
            line_comment("c1", "f1", Side::Old, &["a"]),
            line_comment("c2", "f1", Side::New, &["b"]),
            line_comment("c3", "f2", Side::New, &["x"]),
        ];

        refresh_outdated(&mut comments, "f1", None);

        assert_eq!(
            outdated(&comments),
            [("c1", true), ("c2", true), ("c3", false)]
        );
    }

    fn place(n: u32) -> Place {
        Place {
            n,
            kind: PlaceKind::Element,
            points: Vec::new(),
            elements: Vec::new(),
        }
    }

    #[test]
    fn page_comment_without_a_place_is_rejected() {
        assert_eq!(validate_places(Vec::new()), Err(PlaceError::NoPlace));
    }

    #[test]
    fn page_comment_places_sharing_a_number_are_rejected() {
        assert_eq!(
            validate_places(vec![place(1), place(2), place(1)]),
            Err(PlaceError::DuplicateNumber(1))
        );
    }

    #[test]
    fn page_comment_place_numbers_start_at_one() {
        assert_eq!(
            validate_places(vec![place(0)]),
            Err(PlaceError::NumberFromOne)
        );
    }

    #[test]
    fn page_comment_places_are_put_in_number_order() {
        let places = validate_places(vec![place(2), place(1)]).unwrap();
        let numbers: Vec<u32> = places.iter().map(|place| place.n).collect();
        assert_eq!(numbers, vec![1, 2]);
    }

    #[test]
    fn page_comment_place_numbers_with_a_gap_are_rejected() {
        assert_eq!(
            validate_places(vec![place(1), place(3)]),
            Err(PlaceError::MissingNumber(2))
        );
        assert_eq!(
            validate_places(vec![place(2)]),
            Err(PlaceError::MissingNumber(1))
        );
    }

    #[test]
    fn place_kinds_are_element_arrow_and_pen_only() {
        assert_eq!(PlaceKind::parse("element"), Some(PlaceKind::Element));
        assert_eq!(PlaceKind::parse("arrow"), Some(PlaceKind::Arrow));
        assert_eq!(PlaceKind::parse("pen"), Some(PlaceKind::Pen));
        assert_eq!(PlaceKind::parse("circle"), None);
    }
}
