use std::sync::Arc;

use axum::body::Bytes;
use axum::{
    Json,
    extract::{Query, State},
};
use kemi_core::domain::agent::{AgentEvent, HandedComment};
use kemi_core::domain::explanation::{
    CodeReferenceEntry, ExplanationDocument, ExplanationHistory, ExplanationSection,
    ExplanationVersion, ReferenceContext, ReferenceUnit, RetainedVersions, history_candidate,
    validate_update, validate_version,
};
use kemi_core::domain::review::{Comment, CommentTarget, GroupBy, ReviewMeta, Status};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, ensure_display_persisted};
use crate::{AppState, Event, SubmitState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    expected_revision: u64,
    base_display_version: String,
    sections: Vec<ExplanationSection>,
}

pub(crate) fn initial_history(
    document: ExplanationDocument,
    source: &dyn kemi_core::source::ReviewSource,
    review: &ReviewMeta,
    units: &[GroupBy],
    display: &kemi_core::domain::display_version::DisplayVersions,
    review_id: &str,
    pages: &[String],
) -> Result<ExplanationHistory, crate::ServerError> {
    use kemi_core::domain::explanation::ExplanationReference;
    let mut code = reference_map(review, units.first().copied(), source);
    let requested: Vec<_> = document
        .sections
        .iter()
        .flat_map(|section| &section.blocks)
        .flat_map(|block| &block.references)
        .filter_map(|reference| match reference {
            ExplanationReference::Code {
                unit: Some(unit),
                group_id,
                path,
                ..
            } if Some(match unit {
                ReferenceUnit::File => GroupBy::File,
                ReferenceUnit::Commit => GroupBy::Commit,
            }) != units.first().copied() =>
            {
                Some((*unit, group_id.clone(), path.clone()))
            }
            ExplanationReference::Code { .. } | ExplanationReference::Page { .. } => None,
        })
        .collect();
    code.extend(
        source
            .select_references(&requested)
            .map_err(crate::ServerError::Source)?,
    );
    let revision = validate_update(
        &document,
        0,
        0,
        &display.current,
        &ReferenceContext {
            review_id,
            current_token: Some(&display.current),
            range: !units.is_empty(),
            issued_generation: display.issued,
            code: &code,
            pages,
        },
    )
    .map_err(|error| crate::ServerError::Stopped(error.to_string()))?;
    let version = ExplanationVersion {
        revision,
        base_display_version: display.current.clone(),
        sections: document.sections,
    };
    validate_version(&version).map_err(|error| crate::ServerError::Stopped(error.to_string()))?;
    Ok(ExplanationHistory {
        current_revision: revision,
        versions: vec![version],
    })
}

pub(crate) fn reference_map(
    meta: &ReviewMeta,
    unit: Option<GroupBy>,
    source: &dyn kemi_core::source::ReviewSource,
) -> Vec<CodeReferenceEntry> {
    let unit = unit.map(|unit| match unit {
        GroupBy::File => ReferenceUnit::File,
        GroupBy::Commit => ReferenceUnit::Commit,
    });
    meta.groups
        .iter()
        .flat_map(|group| {
            group.files.iter().map(move |file| {
                let known = entry_reference_metadata(file, source);
                CodeReferenceEntry {
                    unit,
                    group_id: group.id.clone(),
                    path: file.path.clone(),
                    old_present: known.old_present,
                    new_present: known.new_present,
                    old_lines: known.old_lines,
                    new_lines: known.new_lines,
                }
            })
        })
        .collect()
}

pub(crate) fn entry_reference_metadata(
    file: &kemi_core::domain::review::FileEntry,
    source: &dyn kemi_core::source::ReviewSource,
) -> kemi_core::source::ReferenceMetadata {
    let mut known =
        source
            .reference_metadata(&file.id)
            .unwrap_or(kemi_core::source::ReferenceMetadata {
                old_present: file.status != Status::Add,
                new_present: file.status != Status::Delete,
                old_lines: None,
                new_lines: None,
            });
    if file.binary || file.content_skipped {
        known.old_lines = None;
        known.new_lines = None;
    }
    known
}

fn current_map(state: &AppState) -> Result<Vec<CodeReferenceEntry>, ApiError> {
    let review = state.review.read().expect("review poisoned");
    if let Some(reason) = &review.code_error {
        return Err(ApiError::conflict(reason.clone()));
    }
    let source = state.sources.current();
    let mut code = reference_map(&review.startup, review.startup_unit, source.as_ref());
    if let Some(other) = &review.other {
        let (_, meta) = review.meta(Some(other.unit)).map_err(|_| {
            ApiError::conflict("reference list is not ready for this display version")
        })?;
        code.extend(reference_map(&meta, Some(other.unit), source.as_ref()));
    }
    Ok(code)
}

fn pages(state: &AppState) -> Vec<String> {
    state.live.as_ref().map_or_else(Vec::new, |live| {
        std::iter::once(live.start.clone())
            .chain(
                live.snapshots
                    .lock()
                    .expect("snapshots poisoned")
                    .taken
                    .iter()
                    .map(|s| s.record.page.clone()),
            )
            .chain(
                state
                    .registered_pages
                    .lock()
                    .expect("registered pages poisoned")
                    .iter()
                    .cloned(),
            )
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    })
}

pub(crate) async fn context(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    ensure_display_persisted(&state)?;
    let _gate = state.persist.lock().expect("persist poisoned");
    let code = current_map(&state)?;
    let pages = pages(&state);
    let session = state.session.lock().expect("session poisoned");
    let display = session.state.display.as_ref().expect("display assigned");
    Ok(Json(
        json!({"kemi":3,"review":state.display_id,"diff_display_version":display.current,
        "explanation_revision":session.state.explanation.as_ref().map_or(0, |h| h.current_revision),
        "explanation":session.state.explanation.as_ref().and_then(|h| h.current()),
        "references":{"code":code,"pages":pages.iter().map(|url| json!({"url":url})).collect::<Vec<_>>()}}),
    ))
}

fn comment_revisions(comments: &[Comment]) -> Vec<u64> {
    comments
        .iter()
        .filter_map(|comment| match &comment.target {
            CommentTarget::Explanation(target) => Some(target.revision),
            CommentTarget::File(_) | CommentTarget::Page(_) => None,
        })
        .collect()
}

pub(crate) async fn explain(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let update: Update =
        serde_json::from_slice(&body).map_err(|error| ApiError::bad_request(error.to_string()))?;
    ensure_display_persisted(&state)?;
    let document = ExplanationDocument {
        sections: update.sections,
    };
    kemi_core::domain::explanation::validate_document(&document)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let (captured_token, mut code, source, requested) = {
        let _gate = state.persist.lock().expect("persist poisoned");
        let review = state.review.read().expect("review poisoned");
        let session = state.session.lock().expect("session poisoned");
        let token = session
            .state
            .display
            .as_ref()
            .expect("display assigned")
            .current
            .clone();
        let token = review.code_error.is_none().then_some(token);
        let source = state.sources.current();
        let mut code = Vec::new();
        let mut requested = Vec::new();
        if Some(update.base_display_version.as_str()) == token.as_deref() {
            code = reference_map(&review.startup, review.startup_unit, source.as_ref());
            if let Some(other) = &review.other {
                match review.meta(Some(other.unit)) {
                    Ok((_, meta)) => {
                        code.extend(reference_map(&meta, Some(other.unit), source.as_ref()))
                    }
                    Err(_) => {
                        let unit = match other.unit {
                            GroupBy::File => ReferenceUnit::File,
                            GroupBy::Commit => ReferenceUnit::Commit,
                        };
                        for reference in document
                            .sections
                            .iter()
                            .flat_map(|s| &s.blocks)
                            .flat_map(|b| &b.references)
                        {
                            if let kemi_core::domain::explanation::ExplanationReference::Code {
                                unit: Some(wanted),
                                group_id,
                                path,
                                ..
                            } = reference
                                && *wanted == unit
                            {
                                requested.push((unit, group_id.clone(), path.clone()));
                            }
                        }
                    }
                }
            }
        }
        (token, code, source, requested)
    };
    if !requested.is_empty() {
        code.extend(
            tokio::task::spawn_blocking(move || source.select_references(&requested))
                .await
                .map_err(ApiError::internal)?
                .map_err(ApiError::internal)?,
        );
    }
    let _gate = state.persist.lock().expect("persist poisoned");
    if *state.shutdown.borrow()
        || matches!(
            *state.submit_state.lock().expect("submit poisoned"),
            SubmitState::Claimed
        )
    {
        return Err(ApiError::conflict("the review is being submitted"));
    }
    let sink = state
        .session_sink
        .as_ref()
        .filter(|_| state.agent_token.is_some())
        .ok_or_else(|| {
            ApiError::conflict("explanation requires a persistent session and agent endpoint")
        })?;
    let (code_available, range) = {
        let review = state.review.read().expect("review poisoned");
        (review.code_error.is_none(), review.startup_unit.is_some())
    };
    let mut session = state.session.lock().expect("session poisoned");
    let display = session.state.display.as_ref().expect("display assigned");
    let current_token = code_available.then_some(display.current.as_str());
    if Some(update.base_display_version.as_str()) == current_token
        && captured_token.as_deref() != current_token
    {
        return Err(ApiError::conflict(
            "display version changed during reference validation; retrieve context and retry",
        ));
    }
    let pages = pages(&state);
    let revision = validate_update(
        &document,
        update.expected_revision,
        session
            .state
            .explanation
            .as_ref()
            .map_or(0, |h| h.current_revision),
        &update.base_display_version,
        &ReferenceContext {
            review_id: &state.display_id,
            current_token,
            range,
            issued_generation: display.issued,
            code: &code,
            pages: &pages,
        },
    )
    .map_err(|error| match error {
        kemi_core::domain::explanation::ExplanationError::RevisionConflict { .. } => {
            ApiError::conflict(error.to_string())
        }
        kemi_core::domain::explanation::ExplanationError::Structure(_)
        | kemi_core::domain::explanation::ExplanationError::DisplayVersion(_)
        | kemi_core::domain::explanation::ExplanationError::RevisionLimit
        | kemi_core::domain::explanation::ExplanationError::Reference { .. }
        | kemi_core::domain::explanation::ExplanationError::UnknownBlock
        | kemi_core::domain::explanation::ExplanationError::Capacity => {
            ApiError::bad_request(error.to_string())
        }
    })?;
    let incoming = ExplanationVersion {
        revision,
        base_display_version: update.base_display_version,
        sections: document.sections,
    };
    validate_version(&incoming).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let comments = comment_revisions(&session.state.comments);
    let events: Vec<_> = session
        .state
        .channel
        .events
        .iter()
        .flat_map(|AgentEvent::Handed(event)| {
            event.comments.iter().filter_map(|change| match change {
                HandedComment::Added(comment) | HandedComment::Edited(comment) => {
                    match &comment.target {
                        CommentTarget::Explanation(target) => Some(target.revision),
                        CommentTarget::File(_) | CommentTarget::Page(_) => None,
                    }
                }
                HandedComment::Deleted(_) => None,
            })
        })
        .collect();
    let mut candidate = session.snapshot();
    candidate.explanation = Some(ExplanationHistory {
        current_revision: revision,
        versions: vec![],
    });
    let other_saved_bytes = kemi_core::session::additional_saved_size(&candidate);
    let screens: Vec<_> = state
        .explanation_holds
        .lock()
        .expect("holds poisoned")
        .values()
        .flatten()
        .copied()
        .collect();
    let versions = history_candidate(
        session
            .state
            .explanation
            .as_ref()
            .map_or(&[], |h| h.versions.as_slice()),
        incoming.clone(),
        &RetainedVersions {
            comments: &comments,
            events: &events,
            screens: &screens,
            other_saved_bytes,
        },
    )
    .map_err(|error| ApiError::too_large(error.to_string()))?;
    candidate.explanation = Some(ExplanationHistory {
        current_revision: revision,
        versions,
    });
    candidate.refresh_explanation_comments();
    kemi_core::session::validate_additional(&candidate)
        .map_err(|error| ApiError::too_large(error.to_string()))?;
    sink.save_explanation_state(candidate.clone())
        .map_err(ApiError::internal)?;
    session.state.explanation = candidate.explanation;
    session.state.comments = candidate.comments;
    let result = json!({"revision":revision,"base_display_version":incoming.base_display_version});
    let _ = state.events.send(Event::Explanation(result.clone()));
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VersionQuery {
    revision: Option<u64>,
}

pub(super) async fn get_version(
    State(state): State<Arc<AppState>>,
    Query(query): Query<VersionQuery>,
) -> Result<Json<Value>, ApiError> {
    let session = state.session.lock().expect("session poisoned");
    let history = session
        .state
        .explanation
        .as_ref()
        .ok_or_else(|| ApiError::not_found("no explanation yet"))?;
    let revision = query.revision.unwrap_or(history.current_revision);
    let version = history
        .versions
        .iter()
        .find(|version| version.revision == revision)
        .ok_or_else(|| ApiError::not_found("explanation revision is no longer available"))?;
    Ok(Json(json!(version)))
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum HoldRequest {
    RegisterPage { url: String },
    Hold { screen: String, revision: u64 },
    Release { screen: String, revision: u64 },
}

pub(super) async fn hold_version(
    State(state): State<Arc<AppState>>,
    Json(request): Json<HoldRequest>,
) -> Result<Json<Value>, ApiError> {
    let _gate = state.persist.lock().expect("persist poisoned");
    let session = state.session.lock().expect("session poisoned");
    let mut holds = state.explanation_holds.lock().expect("holds poisoned");
    match request {
        HoldRequest::RegisterPage { url } => {
            if state.live.is_none() {
                return Err(ApiError::bad_request(
                    "pages can be registered only in a live review",
                ));
            }
            if !url.starts_with('/')
                || url.starts_with("//")
                || url.contains('#')
                || url.contains('\\')
                || url.chars().any(char::is_control)
                || url.chars().count() > 4096
            {
                return Err(ApiError::bad_request(
                    "page must be a path and query in this live review",
                ));
            }
            state
                .registered_pages
                .lock()
                .expect("registered pages poisoned")
                .insert(url);
        }
        HoldRequest::Hold { screen, revision } => {
            if screen.is_empty() {
                return Err(ApiError::bad_request("screen identity is required"));
            }
            let exists = session
                .state
                .explanation
                .as_ref()
                .is_some_and(|h| h.versions.iter().any(|v| v.revision == revision));
            if !exists {
                return Err(ApiError::not_found(
                    "explanation revision is no longer available",
                ));
            }
            holds.entry(screen).or_default().insert(revision);
        }
        HoldRequest::Release { screen, revision } => {
            if let Some(held) = holds.get_mut(&screen) {
                held.remove(&revision);
                if held.is_empty() {
                    holds.remove(&screen);
                }
            }
        }
    }
    Ok(Json(json!({})))
}
