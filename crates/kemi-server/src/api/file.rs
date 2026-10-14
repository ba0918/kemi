//! ファイルの内容のエンドポイント（`api/file`）と、その応答の組み立て。
//!
//! 行データ（R-SERVE）、ハイライト、コメントの outdated の更新（R-COMMENT）、
//! 描画表示の可否（R-RENDER）を 1 つの応答にまとめる。

use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use kemi_core::domain::comment;
use kemi_core::domain::content;
use kemi_core::domain::diff::{self, DisplayRow, Line, Row, Segment};
use kemi_core::domain::review::{FileEntry, Side};
use kemi_core::render::{self, InitialView, Target};
use kemi_core::source::FileContent;
use serde_json::{Value, json};

use super::{ApiError, ExpandQuery, find_file, prepare_content, side_lines};
use crate::AppState;
use crate::highlight::{self, HighlightedLine, Highlighter};
use crate::session::page_comment_json;

/// 1 ファイル分の左右のハイライト結果。
struct Highlighted {
    old: Vec<HighlightedLine>,
    new: Vec<HighlightedLine>,
}

/// 1 回の展開要求で返す行数の上限。巨大な折りたたみを一度に読まないため。
const EXPAND_LIMIT: usize = 5_000;

pub(super) async fn file_api(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Response, ApiError> {
    let (file, _) = find_file(&state, &id)?;

    let dark = query.dark.as_deref() == Some("1");

    if file.binary {
        return Ok(Json(json!({
            "id": id,
            "binary": true,
            "old_size": file.old_size,
            "new_size": file.new_size,
            "rows": [],
            "comments": comments_for(&state, &id),
            "highlight": { "capable": false, "enabled": false, "dark": dark },
            "render": render_info(&file, None, None),
        }))
        .into_response());
    }

    prepare_content(state, id, move |state, id, content| {
        prepare_file(state, id, &file, content, &query).map(IntoResponse::into_response)
    })
    .await
}

fn prepare_file(
    state: &AppState,
    id: &str,
    file: &FileEntry,
    content: &FileContent,
    query: &ExpandQuery,
) -> Result<Json<Value>, ApiError> {
    let dark = query.dark.as_deref() == Some("1");
    let old_text = content
        .old
        .as_deref()
        .map(|bytes| content::normalize(&String::from_utf8_lossy(bytes)));
    let new_text = content
        .new
        .as_deref()
        .map(|bytes| content::normalize(&String::from_utf8_lossy(bytes)));
    let old_lines = side_lines(&content.old);
    let new_lines = side_lines(&content.new);
    let rows = diff::align(&old_lines, &new_lines);

    let capable = Highlighter::capable(old_text.as_deref(), new_text.as_deref());
    let forced = query.highlight.as_deref() == Some("on");
    let enabled = forced || (capable && query.highlight.as_deref() != Some("off"));
    let highlighted = enabled.then(|| {
        let highlighter = state.highlighter.get_or_init(Highlighter::new);
        Highlighted {
            old: highlighter.highlight(&file.path, old_text.as_deref().unwrap_or(""), dark),
            new: highlighter.highlight(&file.path, new_text.as_deref().unwrap_or(""), dark),
        }
    });
    let highlight_info = json!({ "capable": capable, "enabled": enabled, "dark": dark });
    // 描画表示の対象かと、事前に分かる描画不可は、ここで読んだ内容だけで決める（R-RENDER）。
    let render_info = render_info(file, old_text.as_deref(), new_text.as_deref());

    update_outdated(state, id, &old_lines, &new_lines);

    if let (Some(from), Some(to)) = (query.from, query.to) {
        let from = from.min(rows.len());
        let to = to.min(rows.len()).max(from);
        let end = to.min(from + EXPAND_LIMIT);
        let slice: Vec<Value> = rows[from..end]
            .iter()
            .map(|row| row_json(row, highlighted.as_ref()))
            .collect();
        let next = if end < to { Some(end) } else { None };
        return Ok(Json(json!({
            "id": id,
            "binary": false,
            "rows": slice,
            "next": next,
            "comments": comments_for(state, id),
            "highlight": highlight_info,
            "render": render_info,
        })));
    }

    // 行コメントの付いた行は、変更の行と同じく畳まない。止まる場所と吹き出しは見えている
    // 行にしか置けないので、畳むと、展開したかどうかで止まる場所の数が変わり、コメント
    // 一覧からも移れなくなる（R-NAV, R-VIEW）。
    let (old_commented, new_commented) = commented_lines(state, id);
    let display = diff::collapse(&rows, diff::DEFAULT_CONTEXT, |row| {
        row.old
            .as_ref()
            .is_some_and(|line| old_commented.contains(&line.number))
            || row
                .new
                .as_ref()
                .is_some_and(|line| new_commented.contains(&line.number))
    });
    let rows_json = display
        .iter()
        .map(|row| display_row_json(row, &rows, highlighted.as_ref()))
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "id": id,
        "binary": false,
        "old_total": old_lines.len(),
        "new_total": new_lines.len(),
        "context": diff::DEFAULT_CONTEXT,
        "rows": rows_json,
        "comments": comments_for(state, id),
        "highlight": highlight_info,
        "render": render_info,
    })))
}

/// 描画表示の可否（`kemi_core::render::render_info`）を応答の JSON にする。理由の文言は
/// 契約ではなく、"Cannot render:" の前置はページが 1 か所で付ける。
fn render_info(file: &FileEntry, old: Option<&str>, new: Option<&str>) -> Value {
    let info = render::render_info(file, old, new);
    let target = info.target.map(|target| match target {
        Target::Markdown => "markdown",
        Target::Table { .. } => "table",
        Target::Image { .. } => "image",
    });
    let initial = match info.initial {
        InitialView::Source => "source",
        InitialView::Rendered => "rendered",
    };
    json!({ "target": target, "toggle": info.toggle, "initial": initial, "reason": info.reason })
}

fn comments_for(state: &AppState, file_id: &str) -> Vec<Value> {
    let session = state.session.lock().expect("session poisoned");
    session
        .state
        .comments
        .iter()
        .filter(|comment| comment.file().is_some_and(|file| file.file_id == file_id))
        .map(page_comment_json)
        .collect()
}

/// そのファイルの行コメントが範囲に含む行番号（旧側、新側）。
fn commented_lines(state: &AppState, file_id: &str) -> (HashSet<u32>, HashSet<u32>) {
    let session = state.session.lock().expect("session poisoned");
    let (mut old, mut new) = (HashSet::new(), HashSet::new());
    for file in session
        .state
        .comments
        .iter()
        .filter_map(|comment| comment.file())
        .filter(|file| file.file_id == file_id)
    {
        let Some(start) = file.start_line else {
            continue;
        };
        let end = file.end_line.unwrap_or(start);
        match file.side {
            Side::Old => old.extend(start..=end),
            Side::New => new.extend(start..=end),
        }
    }
    (old, new)
}

fn update_outdated(state: &AppState, file_id: &str, old_lines: &[String], new_lines: &[String]) {
    let mut session = state.session.lock().expect("session poisoned");
    let mut old_hash = None;
    let mut new_hash = None;
    for comment in session.state.comments.iter_mut() {
        let Some(file) = comment.file().filter(|file| file.file_id == file_id) else {
            continue;
        };
        let current_hash = match file.side {
            Side::Old => old_hash.get_or_insert_with(|| comment::content_hash(old_lines)),
            Side::New => new_hash.get_or_insert_with(|| comment::content_hash(new_lines)),
        };
        comment.outdated = comment::is_outdated(&file.content_hash, current_hash);
    }
}

fn row_json(row: &Row, highlighted: Option<&Highlighted>) -> Value {
    let old_highlight = highlighted.map(|highlight| highlight.old.as_slice());
    let new_highlight = highlighted.map(|highlight| highlight.new.as_slice());
    match row.kind {
        diff::RowKind::Equal => json!({
            "kind": "equal",
            "old": line_json(row.old.as_ref(), old_highlight, &[]),
            "new": line_json(row.new.as_ref(), new_highlight, &[]),
        }),
        diff::RowKind::Insert => json!({
            "kind": "insert",
            "new": line_json(row.new.as_ref(), new_highlight, &[]),
        }),
        diff::RowKind::Delete => json!({
            "kind": "delete",
            "old": line_json(row.old.as_ref(), old_highlight, &[]),
        }),
        diff::RowKind::Replace => json!({
            "kind": "replace",
            "old": line_json(row.old.as_ref(), old_highlight, &row.old_segments),
            "new": line_json(row.new.as_ref(), new_highlight, &row.new_segments),
            "old_segments": segments_json(&row.old_segments),
            "new_segments": segments_json(&row.new_segments),
        }),
    }
}

fn display_row_json(row: &DisplayRow, rows: &[Row], highlighted: Option<&Highlighted>) -> Value {
    match row {
        DisplayRow::Diff(row) => row_json(row, highlighted),
        DisplayRow::Skip(skip) => {
            let (old_start, new_start) = rows
                .get(skip.from)
                .map(|row| {
                    (
                        row.old.as_ref().map(|line| line.number),
                        row.new.as_ref().map(|line| line.number),
                    )
                })
                .unwrap_or((None, None));
            json!({
                "kind": "skip",
                "count": skip.to - skip.from,
                "from": skip.from,
                "to": skip.to,
                "old_start": old_start,
                "new_start": new_start,
            })
        }
    }
}

fn line_json(
    line: Option<&Line>,
    highlighted: Option<&[HighlightedLine]>,
    segments: &[Segment],
) -> Value {
    match line {
        Some(line) => {
            let mut value = json!({ "number": line.number, "text": line.text });
            if let Some(ranges) = highlighted.and_then(|lines| lines.get(line.number as usize - 1))
            {
                value["html"] = Value::String(highlight::render_line(ranges, segments));
            }
            value
        }
        None => Value::Null,
    }
}

fn segments_json(segments: &[Segment]) -> Value {
    Value::Array(
        segments
            .iter()
            .map(|segment| json!({ "text": segment.text, "changed": segment.changed }))
            .collect(),
    )
}
