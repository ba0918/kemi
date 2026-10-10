#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayVersionError {
    Invalid,
    OtherReview,
    Unissued,
    Exhausted,
}

impl std::fmt::Display for DisplayVersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "diff_display_version: invalid token shape",
            Self::OtherReview => "diff_display_version: token belongs to another review",
            Self::Unissued => "diff_display_version: generation has not been issued",
            Self::Exhausted => "diff_display_version: generation limit reached",
        })
    }
}
impl std::error::Error for DisplayVersionError {}

pub fn known_generation(
    token: &str,
    review_id: &str,
    issued: u128,
) -> Result<u128, DisplayVersionError> {
    if token.chars().count() > 128 {
        return Err(DisplayVersionError::Invalid);
    }
    let (review, value) = token.split_once(':').ok_or(DisplayVersionError::Invalid)?;
    if review.is_empty()
        || value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(DisplayVersionError::Invalid);
    }
    let generation = value
        .parse::<u128>()
        .map_err(|_| DisplayVersionError::Invalid)?;
    if review != review_id {
        return Err(DisplayVersionError::OtherReview);
    }
    if generation > issued {
        return Err(DisplayVersionError::Unissued);
    }
    Ok(generation)
}

pub fn next_token(review_id: &str, issued: u128) -> Result<String, DisplayVersionError> {
    let next = issued
        .checked_add(1)
        .ok_or(DisplayVersionError::Exhausted)?;
    let token = format!("{review_id}:{next}");
    known_generation(&token, review_id, next)?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_old_and_current_generations_do_not_require_a_token_index() {
        assert_eq!(
            super::known_generation("review:1", "review", 5000).unwrap(),
            1
        );
        assert_eq!(
            super::known_generation("review:5000", "review", 5000).unwrap(),
            5000
        );
    }

    #[test]
    fn other_review_zero_future_and_noncanonical_generations_are_refused() {
        for token in [
            "other:1",
            "review:0",
            "review:3",
            "review:01",
            "review:+1",
            "review: 1",
            "review:1:1",
            "review",
            "review:340282366920938463463374607431768211456",
        ] {
            assert!(
                super::known_generation(token, "review", 2).is_err(),
                "{token}"
            );
        }
    }

    #[test]
    fn next_generation_is_a_candidate_and_never_wraps() {
        assert_eq!(super::next_token("review", 4097).unwrap(), "review:4098");
        assert!(super::next_token("review", u128::MAX).is_err());
    }
}
