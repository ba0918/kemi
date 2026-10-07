//! モックの割り当てと配信（live-compare.md の R-PAGE-MOCK）。
//!
//! モックとそれが参照するファイルは、ページのトークンを含まない `/m/<値>/<パス>` で配る。
//! 値はレビューごとの推測できない値で、配れる範囲のファイルしか返さない（DC1）。モックは
//! `sandbox="allow-scripts"` の枠に出すので不透明なオリジンで動き、中継用の cookie は
//! 付かない。そこで中継のポートではなくレビュー画面のポートで、cookie を見ずに配る。
//! 根からの参照（`/style.css`）は、配るときに `/m/<値>/` の下へ向け直す。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use kemi_core::domain::live::{LiveError, is_html, served_path, url_path};
use kemi_core::source::html_files::html_files;
use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::AppState;
use crate::live::LiveInfo;
use crate::live::files::{clean_path, content_type, resolve};
use crate::live::rewrite::{root_relative_css, root_relative_html};
use crate::session::persist;

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
        state
            .session
            .lock()
            .expect("session poisoned")
            .mocks
            .remove(&request.page);
        persist(&state);
        return Ok(Json(json!({ "page": request.page, "path": null })));
    };
    let path = mock_path(live, input.trim()).map_err(ApiError::unprocessable)?;
    state
        .session
        .lock()
        .expect("session poisoned")
        .mocks
        .insert(request.page.clone(), path.clone());
    // 割り当ては保存するが、会話にはならない。会話の無いセッションは残らない（R-PAGE-SESSION）。
    persist(&state);
    Ok(Json(mock_json(live, &request.page, &path)))
}

/// モックのパネルの一覧に出す数の上限（R-PAGE-MOCK）。集める速さではなく、目で探せる量で決めた。
const MOCK_FILES_SHOWN: usize = 200;

#[derive(Debug, Deserialize)]
pub(super) struct MockFilesQuery {
    /// パスに含む文字（大文字小文字を区別しない）。空ならすべて。
    #[serde(default)]
    q: String,
}

/// モックに選べるファイルの一覧（R-PAGE-MOCK）。検索に合うものをパスの辞書順に 200 件まで返し、
/// 検索に合う全体の数を添える。
pub(super) async fn list_mock_files(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Query(query): Query<MockFilesQuery>,
) -> Result<Json<Value>, ApiError> {
    let root = live(&state)?.root.clone();
    let files = tokio::task::spawn_blocking(move || html_files(&root))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    let wanted = query.q.trim().to_lowercase();
    let matching: Vec<String> = files
        .into_iter()
        .filter(|path| path.to_lowercase().contains(&wanted))
        .collect();
    Ok(Json(json!({
        "files": matching.iter().take(MOCK_FILES_SHOWN).collect::<Vec<_>>(),
        "total": matching.len(),
    })))
}

pub(super) async fn list_mocks(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let live = live(&state)?;
    let mocks = state
        .session
        .lock()
        .expect("session poisoned")
        .mocks
        .clone();
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
