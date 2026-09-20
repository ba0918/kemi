//! ファイルの内容のエンドポイント（`api/file`）と、その応答の組み立て。
//!
//! 行データ（R-SERVE）、ハイライト、コメントの outdated の更新（R-COMMENT）、
//! 描画表示の可否（R-RENDER）を 1 つの応答にまとめる。

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use kemi_core::domain::comment;
use kemi_core::domain::content;
use kemi_core::domain::diff::{self, DisplayRow, Line, Row, Segment};
use kemi_core::domain::review::{FileEntry, Side};
use kemi_core::render::{self, ImageKind, Target, IMAGE_MAX_BYTES};
use serde_json::{json, Value};

use super::{file_not_found, find_file, side_lines, source_content, ApiError, ExpandQuery};
use crate::highlight::{self, HighlightedLine, Highlighter};
use crate::session::comment_json;
use crate::AppState;

/// 1 ファイル分の左右のハイライト結果。
struct Highlighted {
    old: Vec<HighlightedLine>,
    new: Vec<HighlightedLine>,
}

/// 1 回の展開要求で返す行数の上限。巨大な折りたたみを一度に読まないため。
const EXPAND_LIMIT: usize = 5_000;

pub(super) async fn file(
    State(state): State<Arc<AppState>>,
    Path((_token, id)): Path<(String, String)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Json<Value>, ApiError> {
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
        })));
    }

    let content = source_content(&state, &id)
        .await?
        .ok_or_else(file_not_found)?;
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
    let render_info = render_info(&file, old_text.as_deref(), new_text.as_deref());

    update_outdated(&state, &id, &old_lines, &new_lines);

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
            "comments": comments_for(&state, &id),
            "highlight": highlight_info,
            "render": render_info,
        })));
    }

    // 行コメントの付いた行は、変更の行と同じく畳まない。止まる場所と吹き出しは見えている
    // 行にしか置けないので、畳むと、展開したかどうかで止まる場所の数が変わり、コメント
    // 一覧からも移れなくなる（R-NAV, R-VIEW）。
    let (old_commented, new_commented) = commented_lines(&state, &id);
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
        "comments": comments_for(&state, &id),
        "highlight": highlight_info,
        "render": render_info,
    })))
}

/// ファイルが描画表示の対象か、切り替えを持つか、既定はどちらか、事前に分かる描画不可の
/// 理由（R-RENDER）。理由の文言は契約ではなく、"Cannot render:" の前置はページが 1 か所で
/// 付ける。
fn render_info(file: &FileEntry, old: Option<&str>, new: Option<&str>) -> Value {
    const TOO_LARGE: &str = "too large (over 10,000 lines or 1 MB on one side)";
    // 上限超えで内容を読まなかった untracked（R-INPUT-3）は、内容が無いので描画できない。
    const NOT_READ: &str = "the file exceeds the untracked limit and its content was not read";
    let not_read = file.content_skipped.then(|| NOT_READ.to_string());
    match render::target_of_file(file) {
        Some(Target::Markdown) => {
            let reason = not_read.or_else(|| {
                (!content::within_auto_limit(old, new)).then_some(TOO_LARGE.to_string())
            });
            json!({ "target": "markdown", "toggle": true, "initial": "source", "reason": reason })
        }
        Some(Target::Table { delimiter }) => {
            let reason = not_read.or_else(|| {
                if !content::within_auto_limit(old, new) {
                    Some(TOO_LARGE.to_string())
                } else {
                    [new, old]
                        .into_iter()
                        .flatten()
                        .find_map(|text| render::unbalanced_line(text, delimiter))
                        .map(|line| format!("unbalanced quote on line {line}"))
                }
            });
            json!({ "target": "table", "toggle": true, "initial": "source", "reason": reason })
        }
        Some(Target::Image { .. }) => {
            // 画像には行数・バイト数の描画不可を当てず、5 MB の規則だけが効く。内容を
            // 読まなかったものは並べる画像が無いので、バイト数の増減だけになる。
            let (old_bytes, new_bytes) = side_bytes(file, old, new);
            let within = !file.content_skipped
                && old_bytes <= IMAGE_MAX_BYTES
                && new_bytes <= IMAGE_MAX_BYTES;
            match render::image_kind(file) {
                Some(kind) if within => json!({
                    "target": "image",
                    "toggle": kind == ImageKind::Svg,
                    "initial": "rendered",
                    "reason": null,
                }),
                _ => {
                    json!({ "target": null, "toggle": false, "initial": "source", "reason": null })
                }
            }
        }
        _ => json!({ "target": null, "toggle": false, "initial": "source", "reason": null }),
    }
}

/// 各側のバイト数。内容を読んだテキストはその長さ、読んでいないバイナリはエントリの
/// サイズ。
fn side_bytes(file: &FileEntry, old: Option<&str>, new: Option<&str>) -> (u64, u64) {
    if file.binary {
        (file.old_size, file.new_size)
    } else {
        (
            old.map_or(0, |text| text.len() as u64),
            new.map_or(0, |text| text.len() as u64),
        )
    }
}

fn comments_for(state: &AppState, file_id: &str) -> Vec<Value> {
    let session = state.session.lock().expect("session poisoned");
    session
        .comments
        .iter()
        .filter(|comment| comment.file_id == file_id)
        .map(comment_json)
        .collect()
}

/// そのファイルの行コメントが範囲に含む行番号（旧側、新側）。
fn commented_lines(state: &AppState, file_id: &str) -> (HashSet<u32>, HashSet<u32>) {
    let session = state.session.lock().expect("session poisoned");
    let (mut old, mut new) = (HashSet::new(), HashSet::new());
    for comment in session
        .comments
        .iter()
        .filter(|comment| comment.file_id == file_id)
    {
        let Some(start) = comment.start_line else {
            continue;
        };
        let end = comment.end_line.unwrap_or(start);
        match comment.side {
            Side::Old => old.extend(start..=end),
            Side::New => new.extend(start..=end),
        }
    }
    (old, new)
}

fn update_outdated(state: &AppState, file_id: &str, old_lines: &[String], new_lines: &[String]) {
    let mut session = state.session.lock().expect("session poisoned");
    for comment in session
        .comments
        .iter_mut()
        .filter(|comment| comment.file_id == file_id)
    {
        let lines = match comment.side {
            Side::Old => old_lines,
            Side::New => new_lines,
        };
        comment.outdated =
            comment::is_outdated(&comment.content_hash, &comment::content_hash(lines));
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
