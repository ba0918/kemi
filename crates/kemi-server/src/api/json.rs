//! 契約の JSON（R-SUBMIT の結果・`kemi wait` の起きたこと）と、画面に返すコメントと発言の形。

use kemi_core::domain::agent::{AgentEvent, HandedComment};
use kemi_core::domain::review::{Comment, CommentTarget, Message, PageTarget, Reply};
use serde_json::json;

/// R-SUBMIT の契約に合わせたコメントの JSON。`content_hash` は出さない。ページへのコメントの
/// 画像のパスは出さない（submit と画面への応答では `null`）。
pub(super) fn comment_json(comment: &Comment) -> serde_json::Value {
    comment_json_with(comment, false)
}

/// `kemi wait` に返すコメントの JSON。ページへのコメントの画像は絶対パスで渡す（R-AGENT-EVENTS）。
fn agent_comment_json(comment: &Comment) -> serde_json::Value {
    comment_json_with(comment, true)
}

fn comment_json_with(comment: &Comment, with_image: bool) -> serde_json::Value {
    let mut value = json!({
        "id": comment.id,
        "group_id": comment.group_id,
        "group_title": comment.group_title,
        "path": null,
        "side": null,
        "start_line": null,
        "end_line": null,
        "quote": [],
        "body": comment.body,
        "page": null,
        "target_type":match &comment.target {CommentTarget::File(_)=>"file",CommentTarget::Page(_)=>"page",CommentTarget::Explanation(_)=>"explanation"},
        "explanation":null,
        "replies": comment.replies.iter().map(reply_json).collect::<Vec<_>>(),
        "resolved": comment.resolved,
        "outdated": comment.outdated,
        "suggestion": null,
    });
    if let CommentTarget::Page(page) = &comment.target {
        value["page"] = page_json(page);
        if with_image {
            value["page"]["image"] = json!(page.image);
        }
    }
    if let CommentTarget::Explanation(target) = &comment.target {
        value["group_id"] = json!(null);
        value["group_title"] = json!(null);
        value["explanation"] = json!(target);
    }
    if let Some(file) = comment.file() {
        value["path"] = json!(file.path);
        value["side"] = json!(file.side.as_str());
        value["start_line"] = json!(file.start_line);
        value["end_line"] = json!(file.end_line);
        value["quote"] = json!(file.quote);
        value["suggestion"] = json!(
            file.suggestion
                .as_ref()
                .map(|suggestion| json!({ "replacement": suggestion.replacement }))
        );
    }
    value
}

/// R-SUBMIT の `page`。画像のパスは `kemi wait` の応答でだけ渡すので、ここでは `null`
/// （submit の確定でファイルごと消える）。
fn page_json(page: &PageTarget) -> serde_json::Value {
    json!({
        "url": page.url,
        "width": page.width,
        "places": page.places.iter().map(|place| json!({
            "n": place.n,
            "kind": place.kind.as_str(),
            "points": place.points.iter().map(|point| json!({
                "x": coordinate_json(point.x),
                "y": coordinate_json(point.y),
            })).collect::<Vec<_>>(),
            "elements": place.elements.iter().map(|element| json!({
                "selector": element.selector,
                "text": element.text,
                "rect": {
                    "x": element.rect.x,
                    "y": element.rect.y,
                    "w": element.rect.w,
                    "h": element.rect.h,
                },
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "image": null,
    })
}

/// 点の座標 1 つ。結果 JSON の契約の版 4 の kemi で置いた点は整数で、`12.0` でなく `12` と書く
/// （R-PAGE-COMMENT）。それより前の kemi で保存した小数の点（`20.5`）は値を変えずに書く。点を整数の
/// 型にせず f64 のまま持つのは、その小数の点を復元で読むため。整数として書くのは、`as i64` が
/// 値を変えない範囲のうち、JavaScript の数が整数を正確に表せる範囲（±(2^53 − 1)）に限る。
fn coordinate_json(value: f64) -> serde_json::Value {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// R-SUBMIT の契約に合わせた返信の JSON。案（`variants`・`chosen`・`applied`）は
/// `--live` のレビューでだけ持つので、ここでは常に空。
pub(super) fn reply_json(reply: &Reply) -> serde_json::Value {
    json!({
        "id": reply.id,
        "author": reply.author.as_str(),
        "body": reply.body.plain(),
        "parts":reply.body.parts(),
        "variants": [],
        "chosen": null,
        "applied": null,
    })
}

/// R-SUBMIT の契約に合わせた発言の JSON。
pub(super) fn message_json(message: &Message) -> serde_json::Value {
    json!({
        "id": message.id,
        "author": message.author.as_str(),
        "body": message.body.plain(),
        "parts":message.body.parts(),
    })
}

/// ページに返すコメントの JSON。R-SUBMIT の形に、会話パネルの並びに使う通し番号
/// （`seq`）をコメントと返信に足す。番号は結果 JSON と `kemi wait` には出さない（R-SESSION）。
pub(super) fn page_comment_json(comment: &Comment) -> serde_json::Value {
    let mut value = comment_json(comment);
    value["seq"] = json!(comment.seq);
    let replies = value["replies"]
        .as_array_mut()
        .expect("comment_json は replies を必ず配列として生成する");
    for (value, reply) in replies.iter_mut().zip(&comment.replies) {
        value["seq"] = json!(reply.seq);
    }
    value
}

/// ページに返す発言の JSON。R-SUBMIT の形に通し番号（`seq`）を足す。
pub(super) fn page_message_json(message: &Message) -> serde_json::Value {
    let mut value = message_json(message);
    value["seq"] = json!(message.seq);
    value
}

/// `kemi wait` が返す起きたこと 1 つ（R-AGENT-EVENTS）。コメント・返信・発言の形は
/// submit の結果と同じ。削除したコメントは id だけを持つ。
pub(super) fn agent_event_json(event: &AgentEvent) -> serde_json::Value {
    match event {
        AgentEvent::Handed(handed) => json!({
            "type": "handed",
            "comments": handed.comments.iter().map(|change| match change {
                HandedComment::Added(comment) => {
                    json!({ "change": "added", "comment": agent_comment_json(comment) })
                }
                HandedComment::Edited(comment) => {
                    json!({ "change": "edited", "comment": agent_comment_json(comment) })
                }
                HandedComment::Deleted(id) => json!({ "change": "deleted", "comment": { "id": id } }),
            }).collect::<Vec<_>>(),
            "replies": handed.replies.iter().map(|handed| json!({
                "comment_id": handed.comment_id,
                "reply": reply_json(&handed.reply),
            })).collect::<Vec<_>>(),
            "messages": handed.messages.iter().map(message_json).collect::<Vec<_>>(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::coordinate_json;

    /// 契約の版 4 の点は整数で `12.0` と書かない。それより前に保存した小数の点は値を変えずに書く。
    /// JavaScript の数が整数を正確に表せる範囲を超える値は、整数にせずそのまま書く。
    #[test]
    fn whole_coordinates_are_written_as_integers_and_others_keep_their_value() {
        for (value, written) in [
            (12.0, "12"),
            (-3.0, "-3"),
            (0.0, "0"),
            (20.5, "20.5"),
            (-0.25, "-0.25"),
            (9_007_199_254_740_991.0, "9007199254740991"),
            (9_007_199_254_740_992.0, "9007199254740992.0"),
        ] {
            assert_eq!(coordinate_json(value).to_string(), written, "{value}");
        }
    }
}
