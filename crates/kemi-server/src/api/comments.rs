//! コメントの作成・編集・削除・返信・解決のエンドポイント（`api/comment`、R-COMMENT）。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use kemi_core::domain::comment::{self, CommentError, PlaceError};
use kemi_core::domain::review::{
    Author, Comment, CommentTarget, FileTarget, LineRange, PageTarget, Place, PlaceElement,
    PlaceKind, Point, Rect, Reply, Side, Suggestion,
};
use kemi_core::session::FileWritten;
use serde::Deserialize;
use serde_json::{Value, json};

use super::channel::notify_agent_state;
use super::snapshot::{comment_ids, forget_removed};
use super::{ApiError, file_not_found, find_file, parse_side, side_lines, source_content};
use crate::session::{page_comment_json, persist};
use crate::{AppState, Event, Notice};

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub(super) enum CommentRequest {
    Add {
        file_id: String,
        side: String,
        #[serde(default)]
        start_line: Option<u32>,
        #[serde(default)]
        end_line: Option<u32>,
        body: String,
        #[serde(default)]
        suggestion: Option<String>,
    },
    /// `--live` のページへのコメント（live.md の R-PAGE-COMMENT）。
    #[serde(rename = "add_page")]
    AddPage {
        page: PageRequest,
        body: String,
        /// 描き込みを重ねた PNG（base64）。画像とコメントを 1 回の要求で保存し、渡す前に
        /// 画像のパスが決まっているようにする。
        #[serde(default)]
        image: Option<String>,
    },
    /// 本文と suggestion を書き換える。suggestion を省くか null にすると外す。
    Edit {
        id: String,
        body: String,
        #[serde(default)]
        suggestion: Option<String>,
    },
    Delete {
        id: String,
    },
    Reply {
        id: String,
        body: String,
    },
    Resolve {
        id: String,
        resolved: bool,
    },
}

#[derive(Debug, Deserialize)]
pub(super) struct PageRequest {
    url: String,
    width: u32,
    places: Vec<PlaceRequest>,
}

#[derive(Debug, Deserialize)]
struct PlaceRequest {
    n: u32,
    kind: String,
    #[serde(default)]
    points: Vec<PointRequest>,
    #[serde(default)]
    elements: Vec<ElementRequest>,
}

#[derive(Debug, Deserialize)]
struct PointRequest {
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
struct ElementRequest {
    selector: String,
    text: String,
    rect: RectRequest,
}

#[derive(Debug, Deserialize)]
struct RectRequest {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// ページへのコメントのグループ（R-INPUT の `--live` の行）。
const PAGE_GROUP_ID: &str = "page";
const PAGE_GROUP_TITLE: &str = "Page";

pub(super) async fn comment_api(
    State(state): State<Arc<AppState>>,
    Path(_token): Path<String>,
    Json(request): Json<CommentRequest>,
) -> Result<Json<Value>, ApiError> {
    match request {
        CommentRequest::Add {
            file_id,
            side,
            start_line,
            end_line,
            body,
            suggestion,
        } => {
            add_comment(
                &state, file_id, &side, start_line, end_line, body, suggestion,
            )
            .await
        }
        CommentRequest::AddPage { page, body, image } => {
            add_page_comment(&state, page, body, image)
        }
        CommentRequest::Edit {
            id,
            body,
            suggestion,
        } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            // 行レンジ・side・quote・作成時の内容ハッシュも、ページの場所と画像も変えない
            // （R-COMMENT、live.md の R-PAGE-COMMENT）。
            match comment.file_mut() {
                Some(file) => {
                    let range = file
                        .start_line
                        .zip(file.end_line)
                        .map(|(start, end)| LineRange { start, end });
                    comment::validate_comment(file.side, range, suggestion.as_deref())
                        .map_err(|error| ApiError::bad_request(comment_error_message(error)))?;
                    file.suggestion = suggestion.map(|replacement| Suggestion { replacement });
                }
                None if suggestion.is_some() => {
                    return Err(ApiError::bad_request(comment_error_message(
                        CommentError::SuggestionRequiresNewSide,
                    )));
                }
                None => {}
            }
            comment.body = body;
            let value = page_comment_json(comment);
            session.channel.note_comment(&id);
            drop(session);
            persist(&state);
            notify_agent_state(&state);
            Ok(Json(value))
        }
        CommentRequest::Delete { id } => {
            let mut session = state.session.lock().expect("session poisoned");
            let index = session
                .comments
                .iter()
                .position(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            // id は再利用しない。採番は last_comment が進むだけで、削除では戻さない。
            session.comments.remove(index);
            session.channel.note_comment(&id);
            // 返事を待っていたコメントが消えて残りの行が無くなったら、状態を移す（R-AGENT-STATE）。
            // ロックはほかと同じくセッション → エージェントの順に取る。
            state.agent.lock().expect("agent poisoned").comment_deleted(
                session.channel.called,
                kemi_core::session::now_millis(),
                &session.comments,
            );
            drop(session);
            persist(&state);
            notify_agent_state(&state);
            Ok(Json(json!({ "id": id, "deleted": true })))
        }
        CommentRequest::Reply { id, body } => {
            let mut session = state.session.lock().expect("session poisoned");
            let index = session
                .comments
                .iter()
                .position(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            session.last_reply += 1;
            let reply = Reply {
                id: format!("r{}", session.last_reply),
                seq: session.next_seq(),
                author: Author::Reviewer,
                body,
            };
            session.channel.note_reply(&id, &reply.id);
            let comment = &mut session.comments[index];
            comment.replies.push(reply);
            let value = page_comment_json(comment);
            drop(session);
            persist(&state);
            let _ = state.events.send(Event::Thread(value.clone()));
            notify_agent_state(&state);
            Ok(Json(value))
        }
        CommentRequest::Resolve { id, resolved } => {
            let mut session = state.session.lock().expect("session poisoned");
            let comment = session
                .comments
                .iter_mut()
                .find(|comment| comment.id == id)
                .ok_or_else(comment_not_found)?;
            comment.resolved = resolved;
            let value = page_comment_json(comment);
            drop(session);
            persist(&state);
            Ok(Json(value))
        }
    }
}

async fn add_comment(
    state: &AppState,
    file_id: String,
    side: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
    body: String,
    suggestion: Option<String>,
) -> Result<Json<Value>, ApiError> {
    let side = parse_side(side)?;
    let range = match (start_line, end_line) {
        (None, None) => None,
        (Some(start), Some(end)) => Some(LineRange { start, end }),
        _ => {
            return Err(ApiError::bad_request(
                "start_line and end_line must both be given",
            ));
        }
    };
    let (file, group_title) = find_file(state, &file_id)?;

    let content = source_content(state, &file_id)
        .await?
        .ok_or_else(file_not_found)?;
    let lines = match side {
        Side::Old => side_lines(&content.old),
        Side::New => side_lines(&content.new),
    };
    comment::validate_comment(side, range, suggestion.as_deref())
        .map_err(|error| ApiError::bad_request(comment_error_message(error)))?;
    let quote = match range {
        Some(range) => comment::quote_for(&lines, range)
            .map_err(|error| ApiError::bad_request(comment_error_message(error)))?,
        None => Vec::new(),
    };
    let content_hash = comment::content_hash(&lines);

    let mut session = state.session.lock().expect("session poisoned");
    session.last_comment += 1;
    let comment = Comment {
        id: format!("c{}", session.last_comment),
        seq: session.next_seq(),
        group_id: file.group_id.clone(),
        group_title,
        body,
        replies: Vec::new(),
        resolved: false,
        outdated: false,
        target: CommentTarget::File(FileTarget {
            file_id,
            path: file.path.clone(),
            side,
            start_line: range.map(|range| range.start),
            end_line: range.map(|range| range.end),
            quote,
            content_hash,
            suggestion: suggestion.map(|replacement| Suggestion { replacement }),
        }),
    };
    session.channel.note_comment(&comment.id);
    session.comments.push(comment.clone());
    drop(session);
    persist(state);
    notify_agent_state(state);

    if let Some(file) = comment.file() {
        state.notices.notify(Notice::CommentAdded {
            path: file.path.clone(),
            side: file.side,
            lines: file.start_line.zip(file.end_line),
        });
    }
    Ok(Json(page_comment_json(&comment)))
}

fn add_page_comment(
    state: &AppState,
    page: PageRequest,
    body: String,
    image: Option<String>,
) -> Result<Json<Value>, ApiError> {
    if state.live.is_none() {
        return Err(ApiError::bad_request(
            "page comments exist only in a review of a running page",
        ));
    }
    let places = page
        .places
        .into_iter()
        .map(place_from_request)
        .collect::<Result<Vec<_>, ApiError>>()?;
    let places = comment::validate_places(places)
        .map_err(|error| ApiError::bad_request(place_error_message(error)))?;

    let (id, seq) = {
        let mut session = state.session.lock().expect("session poisoned");
        session.last_comment += 1;
        (format!("c{}", session.last_comment), session.next_seq())
    };
    // 画像はセッションのロックの外で書く。書けなくてもコメントは画像なしで残す。
    // 書いてからコメントが入るまでは、スナップショットと別の画像の書き込みを止める。その間に
    // 20 MB の規則が掛かると、まだ状態に無いこの画像を、削除したコメントの画像と見誤る
    // ことがある。ロックは persist → session → snapshots の順。
    let saving = state.persist.lock().expect("persist poisoned");
    let image = image.map(|image| save_image(state, &id, &image));
    let image_unsaved = matches!(image, Some(ImageSaved::NoRoom));
    let image = match image {
        Some(ImageSaved::Saved(path)) => Some(path),
        Some(ImageSaved::NoRoom | ImageSaved::NotSaved) | None => None,
    };
    let comment = Comment {
        id,
        seq,
        group_id: PAGE_GROUP_ID.to_string(),
        group_title: PAGE_GROUP_TITLE.to_string(),
        body,
        replies: Vec::new(),
        resolved: false,
        outdated: false,
        target: CommentTarget::Page(PageTarget {
            url: page.url,
            width: page.width,
            places,
            image,
        }),
    };
    let mut session = state.session.lock().expect("session poisoned");
    session.channel.note_comment(&comment.id);
    session.page_comment_saved = true;
    // 画像を書く間に足されたコメントがあっても、作成順（通し番号の順）に並べる。
    let at = session
        .comments
        .partition_point(|other| other.seq < comment.seq);
    session.comments.insert(at, comment.clone());
    drop(session);
    drop(saving);
    persist(state);
    notify_agent_state(state);
    if let CommentTarget::Page(page) = &comment.target {
        state.notices.notify(Notice::PageCommentAdded {
            url: page.url.clone(),
            width: page.width,
            places: page.places.len(),
        });
    }
    let mut answer = page_comment_json(&comment);
    if image_unsaved {
        // 20 MB の規則で保存しなかったことは画面に出す（R-PAGE-SESSION）。
        answer["image_unsaved"] = json!(true);
    }
    Ok(Json(answer))
}

/// コメントの画像を保存した結果。
enum ImageSaved {
    /// 書いた画像の絶対パス。
    Saved(String),
    /// 20 MB の規則で入らなかった。
    NoRoom,
    /// セッションの無いレビュー、base64 として読めない値、書き込みの失敗。
    NotSaved,
}

/// 画像を `<id>.files/` に書く。保存しなかったときのパスは `null`（submit の契約が許す）。
fn save_image(state: &AppState, id: &str, image: &str) -> ImageSaved {
    use base64::Engine;
    let (Some(sink), Some(live)) = (state.session_sink.as_ref(), state.live.as_deref()) else {
        return ImageSaved::NotSaved;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(image) else {
        return ImageSaved::NotSaved;
    };
    let write = sink.save_image(id, &bytes, &comment_ids(state));
    forget_removed(state, live, &write.removed_snapshots);
    match write.written {
        FileWritten::Saved(path) => ImageSaved::Saved(path.to_string_lossy().into_owned()),
        FileWritten::NoRoom => ImageSaved::NoRoom,
        FileWritten::Failed(error) => {
            state.notices.notify(Notice::CommentImageNotSaved(error));
            ImageSaved::NotSaved
        }
    }
}

fn place_from_request(place: PlaceRequest) -> Result<Place, ApiError> {
    let kind = PlaceKind::parse(&place.kind)
        .ok_or_else(|| ApiError::bad_request("a place kind must be element, arrow or pen"))?;
    Ok(Place {
        n: place.n,
        kind,
        points: place
            .points
            .into_iter()
            .map(|point| Point {
                x: point.x,
                y: point.y,
            })
            .collect(),
        elements: place
            .elements
            .into_iter()
            .map(|element| PlaceElement {
                selector: element.selector,
                text: element.text,
                rect: Rect {
                    x: element.rect.x,
                    y: element.rect.y,
                    w: element.rect.w,
                    h: element.rect.h,
                },
            })
            .collect(),
    })
}

fn place_error_message(error: PlaceError) -> String {
    match error {
        PlaceError::NoPlace => "a page comment needs at least one place".to_string(),
        PlaceError::NumberFromOne => "place numbers start at 1".to_string(),
        PlaceError::DuplicateNumber(n) => format!("place number {n} is used twice"),
        PlaceError::MissingNumber(n) => {
            format!("place numbers run from 1 without gaps, and {n} is missing")
        }
    }
}

fn comment_not_found() -> ApiError {
    ApiError::not_found("comment not found")
}

fn comment_error_message(error: CommentError) -> String {
    match error {
        CommentError::LineNumberOutOfRange => "line numbers start at 1".to_string(),
        CommentError::ReversedRange => "line range is reversed".to_string(),
        CommentError::FileWideMustBeNewSide => {
            "file-wide comments can only be on the new side".to_string()
        }
        CommentError::SuggestionRequiresNewSide => {
            "suggestions can only be on new-side line comments".to_string()
        }
        CommentError::SuggestionRequiresRange => "suggestion requires a line range".to_string(),
        CommentError::QuoteOutOfBounds => "line range exceeds the file".to_string(),
    }
}
