use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use kemi_core::domain::parts::Part;
use kemi_core::domain::parts_validation::validate_parts;
use kemi_core::render::render_part_markdown;
use serde_json::{Value, json};

use super::{ApiError, ExpandQuery};
use crate::AppState;
use crate::highlight::Highlighter;

pub(super) async fn render_part(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExpandQuery>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    let part: Part =
        serde_json::from_str(&body).map_err(|error| ApiError::bad_request(error.to_string()))?;
    validate_parts(std::slice::from_ref(&part))
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let Part::Text { body } = part else {
        return Err(ApiError::bad_request(
            "only text parts use Markdown rendering",
        ));
    };
    let highlight = |lang: &str, code: &str| {
        state
            .highlighter
            .get_or_init(Highlighter::new)
            .highlight_code(lang, code, query.dark.as_deref() == Some("1"))
    };
    let html = render_part_markdown(&body, Some(&highlight))
        .map_err(|error| ApiError::unprocessable(error.to_string()))?;
    Ok(Json(json!({"html": html})))
}
