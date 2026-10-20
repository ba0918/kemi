//! 説明の初期化と、入力ソースのメタデータから作るコード参照。

use kemi_core::domain::explanation::{
    CodeReferenceEntry, ExplanationDocument, ExplanationHistory, ExplanationVersion,
    ReferenceContext, ReferenceUnit, validate_update, validate_version,
};
use kemi_core::domain::review::{GroupBy, ReviewMeta, Status};

pub(crate) fn initial_history(
    document: ExplanationDocument,
    source: &dyn kemi_core::source::ReviewSource,
    review: &ReviewMeta,
    units: &[GroupBy],
    display: &kemi_core::domain::display_version::DisplayVersions,
    review_id: &str,
    pages: &[String],
) -> Result<ExplanationHistory, crate::ServerError> {
    let mut code = reference_map(review, units.first().copied(), source);
    let requested: Vec<_> = document
        .unit_code_references()
        .filter(|(unit, ..)| Some(GroupBy::from(*unit)) != units.first().copied())
        .map(|(unit, group_id, path)| (unit, group_id.clone(), path.clone()))
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
    let unit = unit.map(ReferenceUnit::from);
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
