//! モックの割り当てと配信（live-compare.md の R-PAGE-MOCK）。
//!
//! モックとそれが参照するファイルは、ページのトークンを含まない `/m/<値>/<パス>` で配る。
//! 値はレビューごとの推測できない値で、配れる範囲のファイルしか返さない（DC1）。モックは
//! `sandbox="allow-scripts"` の枠に出すので不透明なオリジンで動き、中継用の cookie は
//! 付かない。そこで中継のポートではなくレビュー画面のポートで、cookie を見ずに配る。
//! 根からの参照（`/style.css`）は、配るときに `/m/<値>/` の下へ向け直す。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use kemi_core::domain::live::{LiveError, is_html, served_path, url_path};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::AppState;
use crate::live::LiveInfo;
use crate::live::files::{clean_path, content_type, resolve};
use crate::live::rewrite::{root_relative_css, root_relative_html};

#[derive(Debug, Deserialize)]
pub(super) struct MockRequest {
    page: String,
    /// 範囲の根からの相対パスか、絶対パス。null で外す。
    path: Option<String>,
}

fn live(state: &AppState) -> Result<&LiveInfo, ApiError> {
    state
        .live
        .as_deref()
        .ok_or_else(|| ApiError::not_found("mocks exist only in a review of a running page"))
}

fn mock_json(live: &LiveInfo, page: &str, path: &str) -> Value {
    json!({
        "page": page,
        "path": path,
        "url": format!("/m/{}/{}", live.mock_secret, url_path(path)),
    })
}

/// 入れたパスを、範囲の根からの相対パスにする。範囲の外と HTML でないものは理由を返す。
fn mock_path(live: &LiveInfo, input: &str) -> Result<String, String> {
    let real = std::fs::canonicalize(live.root.join(input))
        .map_err(|error| format!("cannot read {input}: {error}"))?;
    let relative = served_path(&live.root, &real).map_err(|error| match error {
        LiveError::OutsideRange(_) => {
            format!("{input} is outside the directory kemi can serve")
        }
        LiveError::NotHttp(_)
        | LiveError::Malformed(_)
        | LiveError::NotLoopback(_)
        | LiveError::GitDirectory(_)
        | LiveError::NotHtml(_)
        | LiveError::NotUtf8 => error.to_string(),
    })?;
    if !is_html(&relative) || !real.is_file() {
        return Err(format!("a mock must be a .html or .htm file: {input}"));
    }
    Ok(relative)
}

pub(super) async fn assign_mock(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<MockRequest>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let Some(input) = request.path else {
        live.mocks
            .lock()
            .expect("mocks poisoned")
            .remove(&request.page);
        return Ok(Json(json!({ "page": request.page, "path": null })));
    };
    let path = mock_path(live, input.trim()).map_err(ApiError::unprocessable)?;
    live.mocks
        .lock()
        .expect("mocks poisoned")
        .insert(request.page.clone(), path.clone());
    Ok(Json(mock_json(live, &request.page, &path)))
}

pub(super) async fn list_mocks(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let mocks = live.mocks.lock().expect("mocks poisoned");
    Ok(Json(json!({
        "mocks": mocks
            .iter()
            .map(|(page, path)| mock_json(live, page, path))
            .collect::<Vec<_>>(),
    })))
}

/// `/m/<値>/<パス>`。値が違うもの、範囲の外、`.git` の中、無いものは 404。
pub(crate) async fn mock_file(
    State(state): State<Arc<AppState>>,
    Path((secret, path)): Path<(String, String)>,
) -> Response {
    let not_found = || (StatusCode::NOT_FOUND, "kemi: not found").into_response();
    let Some(live) = state.live.as_deref() else {
        return not_found();
    };
    if secret != live.mock_secret {
        return not_found();
    }
    // パスは Path がもう復号している。もう一度復号すると `%25` を含む名前が別の名前になる。
    let Some(relative) = clean_path(&path) else {
        return not_found();
    };
    let Some(real) = resolve(&live.root, &relative) else {
        return not_found();
    };
    let bytes = match tokio::task::spawn_blocking(move || std::fs::read(real)).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) | Err(_) => return not_found(),
    };
    let prefix = format!("/m/{}", live.mock_secret);
    let bytes = if is_html(&relative) {
        root_relative_html(&bytes, &prefix)
    } else if relative.to_ascii_lowercase().ends_with(".css") {
        root_relative_css(&bytes, &prefix)
    } else {
        bytes
    };
    // 直接開かれても、レビュー画面のオリジンでスクリプトを動かさない（R-PAGE-MOCK）。
    (
        [
            (header::CONTENT_TYPE, content_type(&relative, &bytes)),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox allow-scripts".to_string(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        bytes,
    )
        .into_response()
}
