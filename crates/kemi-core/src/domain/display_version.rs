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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayVersions {
    pub current: String,
    pub frozen: Option<String>,
    pub issued: u128,
}

/// レビューの始まり方。起動時の表示版の決め方が変わる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// 新しいレビュー。
    Fresh,
    /// 保留したレビューの復元。`rereads` は、写しでなく今の内容を読み直したか（コードを
    /// 読めた `--live`）。
    Resumed { rereads: bool },
}

impl DisplayVersions {
    /// 起動時の表示版。保存した版が無ければ最初の版から始める。復元で今の内容を読み直した
    /// なら次の世代を発行し、写しから開くなら凍結した版を今の版に戻す。
    pub fn at_start(
        saved: Option<Self>,
        review_id: &str,
        start: Start,
    ) -> Result<Self, DisplayVersionError> {
        // id が表示版の形にならないレビューは、保存した版があっても始めない。
        let initial = Self::initial(review_id)?;
        let mut display = saved.unwrap_or(initial);
        match start {
            Start::Fresh => {}
            Start::Resumed { rereads: true } => display = display.next(review_id)?,
            Start::Resumed { rereads: false } => {
                if let Some(frozen) = &display.frozen {
                    display.current = frozen.clone();
                }
            }
        }
        Ok(display)
    }

    pub fn initial(review_id: &str) -> Result<Self, DisplayVersionError> {
        Ok(Self {
            current: next_token(review_id, 0)?,
            frozen: None,
            issued: 1,
        })
    }
    pub fn validate(&self, review_id: &str) -> Result<(), DisplayVersionError> {
        known_generation(&self.current, review_id, self.issued)?;
        if let Some(frozen) = &self.frozen {
            known_generation(frozen, review_id, self.issued)?;
        }
        Ok(())
    }
    pub fn next(&self, review_id: &str) -> Result<Self, DisplayVersionError> {
        Ok(Self {
            current: next_token(review_id, self.issued)?,
            frozen: self.frozen.clone(),
            issued: self
                .issued
                .checked_add(1)
                .ok_or(DisplayVersionError::Exhausted)?,
        })
    }
}

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
    use super::{DisplayVersions, Start};

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

    fn saved(current: &str, frozen: Option<&str>, issued: u128) -> DisplayVersions {
        DisplayVersions {
            current: current.to_string(),
            frozen: frozen.map(str::to_string),
            issued,
        }
    }

    #[test]
    fn a_new_review_starts_at_the_first_generation() {
        assert_eq!(
            DisplayVersions::at_start(None, "review", Start::Fresh).unwrap(),
            saved("review:1", None, 1)
        );
    }

    #[test]
    fn a_review_resumed_from_its_copy_shows_the_frozen_generation() {
        let display = saved("review:3", Some("review:2"), 3);

        assert_eq!(
            DisplayVersions::at_start(Some(display), "review", Start::Resumed { rereads: false })
                .unwrap(),
            saved("review:2", Some("review:2"), 3)
        );
    }

    #[test]
    fn a_resumed_review_without_a_frozen_generation_keeps_its_current_one() {
        let display = saved("review:3", None, 3);

        assert_eq!(
            DisplayVersions::at_start(
                Some(display.clone()),
                "review",
                Start::Resumed { rereads: false }
            )
            .unwrap(),
            display
        );
        assert_eq!(
            DisplayVersions::at_start(None, "review", Start::Resumed { rereads: false }).unwrap(),
            saved("review:1", None, 1)
        );
    }

    #[test]
    fn a_resumed_review_that_rereads_its_contents_issues_the_next_generation() {
        let display = saved("review:3", Some("review:2"), 3);

        assert_eq!(
            DisplayVersions::at_start(Some(display), "review", Start::Resumed { rereads: true })
                .unwrap(),
            saved("review:4", Some("review:2"), 4)
        );
        assert_eq!(
            DisplayVersions::at_start(None, "review", Start::Resumed { rereads: true }).unwrap(),
            saved("review:2", None, 2)
        );
    }

    #[test]
    fn next_generation_is_a_candidate_and_never_wraps() {
        assert_eq!(super::next_token("review", 4097).unwrap(), "review:4098");
        assert!(super::next_token("review", u128::MAX).is_err());
    }
}
