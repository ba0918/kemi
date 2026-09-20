//! 描画表示と画像のエンドポイント（R-RENDER）。
//!
//! ルーティングは親の `api` が持ち、ファイルの応答に載せる描画表示の可否の判定は
//! `api::file` の `render_info` が持つ。ここは `api/render` と `api/image/*` の
//! 応答の組み立てだけを持つ。

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use kemi_core::domain::content;
use kemi_core::domain::review::{FileEntry, Side, Status};
use kemi_core::render::{self, ImageRef, ReviewPaths, Target, IMAGE_MAX_BYTES};
use serde_json::{json, Value};

use super::{
    file_not_found, find_file, parse_side, runtime_error, side_lines, source_content, ApiError,
    ExpandQuery,
};
use crate::highlight::Highlighter;
use crate::AppState;

fn review_image_url(id: &str, side: Side) -> String {
    format!("api/image/review/{}/{}", url_segment(id), side.as_str())
}

/// レビュー対象の画像の旧と新（R-RENDER）。改名でバイト列が同じなら `same`。
async fn render_image_file(
    state: &AppState,
    id: &str,
    file: &FileEntry,
) -> Result<Json<Value>, ApiError> {
    if render::image_kind(file).is_none() {
        return Err(ApiError::bad_request("this file is not an image"));
    }
    let content = source_content(state, id)
        .await?
        .ok_or_else(file_not_found)?;
    let over = |bytes: &Option<Vec<u8>>| {
        bytes
            .as_ref()
            .is_some_and(|bytes| bytes.len() as u64 > IMAGE_MAX_BYTES)
    };
    if over(&content.old) || over(&content.new) {
        return Err(ApiError::unprocessable(
            "the image exceeds 5 MB on one side",
        ));
    }
    let side_json = |bytes: &Option<Vec<u8>>, side: Side| {
        bytes
            .as_ref()
            .map(|bytes| json!({ "url": review_image_url(id, side), "size": bytes.len() }))
    };
    Ok(Json(json!({
        "id": id,
        "kind": "image",
        "old": side_json(&content.old, Side::Old),
        "new": side_json(&content.new, Side::New),
        "same": content.old.is_some() && content.old == content.new,
    })))
}

/// レビュー対象の画像のバイト列（R-SERVE）。kemi がそのレビューで読んだものをそのまま配る
/// ので、入力モードを問わず、復元でも写しから出る。
pub(super) async fn review_image(
    State(state): State<Arc<AppState>>,
    Path((_token, id, side)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let (file, _) = find_file(&state, &id)?;
    if render::image_kind(&file).is_none() {
        return Err(file_not_found());
    }
    let side = parse_side(&side).map_err(|_| file_not_found())?;
    let content = source_content(&state, &id)
        .await?
        .ok_or_else(file_not_found)?;
    let (bytes, path) = match side {
        Side::Old => (
            content.old,
            file.old_path.clone().unwrap_or_else(|| file.path.clone()),
        ),
        Side::New => (content.new, file.path.clone()),
    };
    let bytes = bytes.ok_or_else(file_not_found)?;
    if bytes.len() as u64 > IMAGE_MAX_BYTES {
        return Err(file_not_found());
    }
    image_response(bytes, &path)
}

/// Markdown の相対パス画像を、その Markdown のその側の版のリポジトリから読んで配る
/// （R-RENDER, R-SERVE）。symlink・上限超え・無いファイル・読めない入力は 404。
pub(super) async fn repository_image(
    State(state): State<Arc<AppState>>,
    Path((_token, id, side, path)): Path<(String, String, String, String)>,
) -> Result<Response, ApiError> {
    find_file(&state, &id)?;
    let side = parse_side(&side).map_err(|_| file_not_found())?;
    if render::image_content_type(&path).is_none() {
        return Err(file_not_found());
    }
    let source = state.source.clone();
    let file_id = id.clone();
    let repository_path = path.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        source.repository_file(&file_id, side, &repository_path, IMAGE_MAX_BYTES)
    })
    .await
    .map_err(|error| runtime_error(&state, error))?
    .map_err(|error| runtime_error(&state, error))?
    .ok_or_else(file_not_found)?;
    image_response(bytes, &path)
}

/// 画像の応答。拡張子から決めた `Content-Type` に、`nosniff` と `sandbox` を付ける。
/// 同じオリジンで直接開かれた SVG の中のスクリプトが kemi の API に届かないため。
fn image_response(bytes: Vec<u8>, path: &str) -> Result<Response, ApiError> {
    let content_type = render::image_content_type(path).ok_or_else(file_not_found)?;
    let mut response = axum::body::Bytes::from(bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static("sandbox"),
    );
    Ok(response)
}

/// 描画表示の HTML とブロックの一覧（R-RENDER）。表示時に初めて計算する（R-SERVE）。
pub(super) async fn render_file(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Json<Value>, ApiError> {
    let (file, _) = find_file(&state, &id)?;
    match render::target_of_file(&file) {
        Some(Target::Markdown) => render_markdown_file(&state, &id, &file, &query).await,
        Some(Target::Table { delimiter }) => render_table_file(&state, &id, delimiter).await,
        Some(Target::Image { .. }) => render_image_file(&state, &id, &file).await,
        None => Err(ApiError::bad_request("this file has no rendered view")),
    }
}

async fn render_table_file(
    state: &AppState,
    id: &str,
    delimiter: u8,
) -> Result<Json<Value>, ApiError> {
    let content = source_content(state, id)
        .await?
        .ok_or_else(file_not_found)?;
    let old_text = side_text(&content.old);
    let new_text = side_text(&content.new);
    let rendered = render::render_table(&render::TableInput {
        old: old_text.as_deref(),
        new: new_text.as_deref(),
        delimiter,
    })
    .map_err(|error| ApiError::unprocessable(error.to_string()))?;
    Ok(Json(json!({
        "id": id,
        "kind": "table",
        "html": rendered.html,
        "blocks": blocks_json(&rendered),
        "old_lines": side_lines(&content.old),
        "new_lines": side_lines(&content.new),
    })))
}

/// 描画に渡す側のテキスト。`api/file` の事前判定と同じく改行を正規化してから渡し、
/// 上限を同じテキストで測る（CRLF のファイルで判定と描画の成否が食い違わないため）。
fn side_text(bytes: &Option<Vec<u8>>) -> Option<String> {
    bytes
        .as_deref()
        .map(|bytes| content::normalize(&String::from_utf8_lossy(bytes)))
}

fn blocks_json(rendered: &render::Rendered) -> Vec<Value> {
    rendered
        .blocks
        .iter()
        .map(|block| {
            json!({
                "side": block.side.as_str(),
                "start": block.start,
                "end": block.end,
                "mark": block.mark.as_str(),
            })
        })
        .collect()
}

async fn render_markdown_file(
    state: &AppState,
    id: &str,
    file: &FileEntry,
    query: &ExpandQuery,
) -> Result<Json<Value>, ApiError> {
    let content = source_content(state, id)
        .await?
        .ok_or_else(file_not_found)?;
    let old_text = side_text(&content.old);
    let new_text = side_text(&content.new);
    let dark = query.dark.as_deref() == Some("1");
    let capable = Highlighter::capable(old_text.as_deref(), new_text.as_deref());
    let forced = query.highlight.as_deref() == Some("on");
    let enabled = forced || (capable && query.highlight.as_deref() != Some("off"));
    let highlighter = state.highlighter.get_or_init(Highlighter::new);
    let highlight = |lang: &str, code: &str| highlighter.highlight_code(lang, code, dark);
    let file_id = id.to_string();
    let image_url = move |reference: &ImageRef| match reference {
        ImageRef::Review { file_id, side } => {
            format!(
                "api/image/review/{}/{}",
                url_segment(file_id),
                side.as_str()
            )
        }
        ImageRef::Repo { path, side } => format!(
            "api/image/repo/{}/{}/{}",
            url_segment(&file_id),
            side.as_str(),
            url_path(path)
        ),
    };
    let input = render::MarkdownInput {
        old: old_text.as_deref(),
        new: new_text.as_deref(),
        old_path: (file.status != Status::Add)
            .then(|| file.old_path.as_deref().unwrap_or(&file.path)),
        new_path: (file.status != Status::Delete).then_some(file.path.as_str()),
        review_paths: review_paths_of(state, file),
        repo_readable: state.source.reads_repository(),
    };
    let rendered = render::render_markdown(&input, enabled.then_some(&highlight), &image_url)
        .map_err(|error| ApiError::unprocessable(error.to_string()))?;
    Ok(Json(json!({
        "id": id,
        "kind": "markdown",
        "html": rendered.html,
        "blocks": blocks_json(&rendered),
        "old_lines": side_lines(&content.old),
        "new_lines": side_lines(&content.new),
        "highlight": { "capable": capable, "enabled": enabled, "dark": dark },
    })))
}

/// 同じグループのレビュー対象ファイルの、側ごとのパス。相対パス画像がレビュー対象に
/// 一致するかの判定に使う。
fn review_paths_of(state: &AppState, file: &FileEntry) -> ReviewPaths {
    let review = state.review.read().expect("review lock poisoned");
    let other = review.other.as_ref().and_then(|other| other.meta.as_ref());
    let mut paths = ReviewPaths::default();
    let group = std::iter::once(&review.startup)
        .chain(other)
        .flat_map(|meta| meta.groups.iter())
        .find(|group| group.id == file.group_id && group.files.iter().any(|f| f.id == file.id));
    if let Some(group) = group {
        for entry in &group.files {
            if entry.status != Status::Add {
                let old_path = entry.old_path.clone().unwrap_or_else(|| entry.path.clone());
                paths.old.push((entry.id.clone(), old_path));
            }
            if entry.status != Status::Delete {
                paths.new.push((entry.id.clone(), entry.path.clone()));
            }
        }
    }
    paths
}

/// URL の 1 セグメントに入れる。英数字と `-` `.` `_` `~` 以外はパーセント符号化する。
fn url_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// `/` で区切ったパスをセグメントごとに符号化する。
fn url_path(path: &str) -> String {
    path.split('/')
        .map(url_segment)
        .collect::<Vec<_>>()
        .join("/")
}
