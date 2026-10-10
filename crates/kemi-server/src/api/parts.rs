use axum::Json;
use kemi_core::domain::parts::Part;
use kemi_core::domain::parts_validation::validate_parts;
use kemi_core::render::render_part_markdown;
use serde_json::{Value, json};

use super::ApiError;

pub(super) async fn render_part(body: String) -> Result<Json<Value>, ApiError> {
    let part: Part =
        serde_json::from_str(&body).map_err(|error| ApiError::bad_request(error.to_string()))?;
    validate_parts(std::slice::from_ref(&part))
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let Part::Text { body } = part else {
        return Err(ApiError::bad_request(
            "only text parts use Markdown rendering",
        ));
    };
    let html =
        render_part_markdown(&body).map_err(|error| ApiError::unprocessable(error.to_string()))?;
    Ok(Json(json!({"html": html})))
}
