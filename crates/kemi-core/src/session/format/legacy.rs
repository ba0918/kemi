//! 版 2・3 のセッションを現在の DTO が読める形へ変換する。

use serde_json::Value;

use crate::domain::review::Author;

pub(super) fn upgrade(version: u8, value: &mut Value) -> Result<(), String> {
    if !value.get("state").is_some_and(Value::is_object) {
        return Err("state must be an object".into());
    }
    if version == 2 {
        upgrade_version_2(value)?;
    }
    if let Some(comments) = value["state"]["comments"].as_array_mut() {
        for comment in comments {
            legacy_comment(comment)?;
        }
    }
    if let Some(messages) = value["state"].get_mut("messages") {
        legacy_posts(messages)?;
    }
    if let Some(events) = value
        .get_mut("state")
        .and_then(|state| state.get_mut("channel"))
        .and_then(|channel| channel.get_mut("events"))
        .and_then(Value::as_array_mut)
    {
        for event in events {
            if let Some(changes) = event.get_mut("comments").and_then(Value::as_array_mut) {
                for change in changes {
                    if let Some(comment) = change.get_mut("comment") {
                        legacy_comment(comment)?;
                    }
                }
            }
            if let Some(replies) = event.get_mut("replies").and_then(Value::as_array_mut) {
                for reply in replies {
                    if let Some(post) = reply.get_mut("reply") {
                        legacy_post(post)?;
                    }
                }
            }
            if let Some(messages) = event.get_mut("messages") {
                legacy_posts(messages)?;
            }
        }
    }
    let id = value["info"]["id"]
        .as_str()
        .ok_or("legacy session has no id")?
        .to_string();
    let live = value["info"]["mode"]["kind"] == "live";
    let frozen = !live && value["copy"]["state"] == "ready";
    value["state"]["diff_display_version"] = Value::String(format!("{id}:1"));
    value["state"]["issued_display_generation"] = Value::String("1".into());
    value["state"]["frozen_display_version"] = if frozen {
        Value::String(format!("{id}:1"))
    } else {
        Value::Null
    };
    value["state"]["explanation"] = Value::Null;
    Ok(())
}

fn legacy_post(value: &mut Value) -> Result<(), String> {
    if !value["body"].is_string() {
        return Err("legacy body must be a string".into());
    }
    value["parts"] = Value::Null;
    Ok(())
}

fn legacy_posts(value: &mut Value) -> Result<(), String> {
    if let Some(values) = value.as_array_mut() {
        for value in values {
            legacy_post(value)?;
        }
    }
    Ok(())
}

fn legacy_comment(value: &mut Value) -> Result<(), String> {
    if let Some(object) = value.as_object_mut() {
        object.remove("explanation");
    }
    if let Some(replies) = value.get_mut("replies") {
        legacy_posts(replies)?;
    }
    Ok(())
}

/// 版 2 の返信は文字列の並びで、画面側だけが書けたので人間の返信として読む。
fn upgrade_version_2(meta: &mut Value) -> Result<(), String> {
    let state = meta
        .get_mut("state")
        .ok_or_else(|| "no state".to_string())?;
    let mut last_reply = 0u32;
    if let Some(comments) = state.get_mut("comments").and_then(Value::as_array_mut) {
        for comment in comments {
            let Some(replies) = comment.get_mut("replies").and_then(Value::as_array_mut) else {
                continue;
            };
            for reply in replies.iter_mut() {
                let body = reply
                    .as_str()
                    .ok_or_else(|| "a version 2 reply is not a string".to_string())?
                    .to_string();
                last_reply += 1;
                *reply = serde_json::json!({
                    "id": format!("r{last_reply}"),
                    "author": Author::Reviewer.as_str(),
                    "body": body,
                });
            }
        }
    }
    state["last_reply"] = Value::from(last_reply);
    Ok(())
}
