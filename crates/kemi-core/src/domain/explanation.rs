use super::display_version::{DisplayVersionError, known_generation};
use super::parts::Part;
use super::parts_validation::{PartError, PartRule, bound, id, string_bytes, text, validate_part};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationHistory {
    pub current_revision: u64,
    pub versions: Vec<ExplanationVersion>,
}
impl ExplanationHistory {
    pub fn current(&self) -> Option<&ExplanationVersion> {
        self.versions
            .iter()
            .find(|v| v.revision == self.current_revision)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationDocument {
    pub sections: Vec<ExplanationSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationVersion {
    pub revision: u64,
    pub base_display_version: String,
    pub sections: Vec<ExplanationSection>,
}

pub fn validate_version(version: &ExplanationVersion) -> Result<(), ExplanationError> {
    if version.revision == 0 || version.revision > MAX_REVISION {
        return Err(ExplanationError::RevisionLimit);
    }
    let review = version
        .base_display_version
        .split_once(':')
        .map(|(review, _)| review)
        .unwrap_or("");
    known_generation(&version.base_display_version, review, u128::MAX)
        .map_err(ExplanationError::DisplayVersion)?;
    validate_document(&ExplanationDocument {
        sections: version.sections.clone(),
    })?;
    let value =
        serde_json::to_value(version).expect("ExplanationVersion contains only JSON values");
    bound("explanation UTF-8 bytes", string_bytes(&value), 0, 524_288)?;
    bound(
        "explanation canonical JSON bytes",
        canonical_size(&value),
        0,
        1_048_576,
    )?;
    Ok(())
}

pub struct RetainedVersions<'a> {
    pub comments: &'a [u64],
    pub events: &'a [u64],
    pub screens: &'a [u64],
    pub other_saved_bytes: usize,
}

pub fn canonical_size(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value)
        .expect("JSON Value contains only serializable JSON data")
        .len()
}

pub fn history_candidate(
    versions: &[ExplanationVersion],
    incoming: ExplanationVersion,
    held: &RetainedVersions<'_>,
) -> Result<Vec<ExplanationVersion>, ExplanationError> {
    let current = incoming.revision;
    let mut candidate = versions.to_vec();
    candidate.push(incoming);
    candidate.sort_by_key(|v| v.revision);
    loop {
        let bytes = candidate
            .iter()
            .map(|v| {
                canonical_size(
                    &serde_json::to_value(v).expect("ExplanationVersion contains only JSON values"),
                )
            })
            .fold(0usize, usize::saturating_add);
        if candidate.len() <= 128
            && bytes <= 8_388_608
            && bytes.saturating_add(held.other_saved_bytes) <= 16_777_216
        {
            return Ok(candidate);
        }
        let removable = candidate.iter().position(|v| {
            v.revision != current
                && !held.comments.contains(&v.revision)
                && !held.events.contains(&v.revision)
                && !held.screens.contains(&v.revision)
        });
        let index = removable.ok_or(ExplanationError::Capacity)?;
        candidate.remove(index);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationSection {
    pub id: String,
    pub title: String,
    pub blocks: Vec<ExplanationBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationBlock {
    pub id: String,
    pub part: Part,
    pub references: Vec<ExplanationReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceUnit {
    File,
    Commit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSide {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExplanationReference {
    Code {
        #[serde(deserialize_with = "nullable")]
        unit: Option<ReferenceUnit>,
        group_id: String,
        path: String,
        side: ReferenceSide,
        #[serde(deserialize_with = "nullable")]
        start_line: Option<u32>,
        #[serde(deserialize_with = "nullable")]
        end_line: Option<u32>,
    },
    Page {
        url: String,
    },
}
fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationQuote {
    pub section_title: String,
    pub block: ExplanationBlock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeReferenceEntry {
    pub unit: Option<ReferenceUnit>,
    pub group_id: String,
    pub path: String,
    pub old_present: bool,
    pub new_present: bool,
    pub old_lines: Option<u32>,
    pub new_lines: Option<u32>,
}

pub struct ReferenceContext<'a> {
    pub review_id: &'a str,
    pub current_token: &'a str,
    pub issued_generation: u128,
    pub code: &'a [CodeReferenceEntry],
    pub pages: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplanationError {
    Structure(PartError),
    DisplayVersion(DisplayVersionError),
    RevisionConflict {
        expected: u64,
        current: u64,
    },
    RevisionLimit,
    Reference {
        path: String,
        reason: ReferenceFailure,
    },
    UnknownBlock,
    Capacity,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceFailure {
    RangeShape,
    MissingOrAmbiguous,
    SideAbsent,
    LinesUnknown,
    LinesOutside,
    PageUnregistered,
}
impl std::fmt::Display for ExplanationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Structure(e) => e.fmt(f),
            Self::DisplayVersion(e) => e.fmt(f),
            Self::RevisionConflict { expected, current } => write!(
                f,
                "expected_revision: conflict, expected {expected}, current {current}"
            ),
            Self::RevisionLimit => f.write_str("revision: explanation revision limit reached"),
            Self::Reference { path, reason } => write!(
                f,
                "{path}: {}",
                match reason {
                    ReferenceFailure::RangeShape =>
                        "line range must have both ends null or increasing positive integers",
                    ReferenceFailure::MissingOrAmbiguous =>
                        "reference target is missing or ambiguous for this display version",
                    ReferenceFailure::SideAbsent => "referenced side is absent",
                    ReferenceFailure::LinesUnknown =>
                        "line count is unknown for this display version",
                    ReferenceFailure::LinesOutside => "line range is outside this display version",
                    ReferenceFailure::PageUnregistered =>
                        "page must already be registered in this live review",
                }
            ),
            Self::UnknownBlock => f.write_str("block_id: explanation block not found"),
            Self::Capacity => f.write_str(
                "explanation: retained history or additional saved content exceeds capacity",
            ),
        }
    }
}
impl std::error::Error for ExplanationError {}
impl From<PartError> for ExplanationError {
    fn from(e: PartError) -> Self {
        Self::Structure(e)
    }
}

pub fn quote_block(
    document: &ExplanationDocument,
    block_id: &str,
) -> Result<ExplanationQuote, ExplanationError> {
    for section in &document.sections {
        if let Some(block) = section.blocks.iter().find(|b| b.id == block_id) {
            return Ok(ExplanationQuote {
                section_title: section.title.clone(),
                block: block.clone(),
            });
        }
    }
    Err(ExplanationError::UnknownBlock)
}
pub fn block_outdated(original: &ExplanationBlock, document: &ExplanationDocument) -> bool {
    !document.sections.iter().flat_map(|s| &s.blocks).any(|b| {
        b.id == original.id && b.part == original.part && b.references == original.references
    })
}
pub fn validate_document(document: &ExplanationDocument) -> Result<(), ExplanationError> {
    bound("sections", document.sections.len(), 1, 128)?;
    let mut sections = HashSet::new();
    let mut blocks = HashSet::new();
    for (i, section) in document.sections.iter().enumerate() {
        let path = format!("sections[{i}]");
        id(&format!("{path}.id"), &section.id)?;
        if !sections.insert(section.id.as_str()) {
            return Err(PartError {
                path: format!("{path}.id"),
                rule: PartRule::Duplicate,
            }
            .into());
        }
        text(&format!("{path}.title"), &section.title, 1, 120, true)?;
        bound(&format!("{path}.blocks"), section.blocks.len(), 1, 512)?;
        for (j, block) in section.blocks.iter().enumerate() {
            let path = format!("{path}.blocks[{j}]");
            id(&format!("{path}.id"), &block.id)?;
            if !blocks.insert(block.id.as_str()) {
                return Err(PartError {
                    path: format!("{path}.id"),
                    rule: PartRule::Duplicate,
                }
                .into());
            }
            validate_part(&block.part, &format!("{path}.part"))?;
            bound(&format!("{path}.references"), block.references.len(), 0, 32)?;
            for (k, reference) in block.references.iter().enumerate() {
                validate_reference_shape(reference, &format!("{path}.references[{k}]"))?;
            }
        }
    }
    bound("blocks", blocks.len(), 1, 512)?;
    let value =
        serde_json::to_value(document).expect("ExplanationDocument contains only JSON values");
    bound("explanation UTF-8 bytes", string_bytes(&value), 0, 524_288)?;
    bound(
        "explanation canonical JSON bytes",
        canonical_size(&value),
        0,
        1_048_576,
    )?;
    Ok(())
}
fn reference_failure(path: &str, reason: ReferenceFailure) -> ExplanationError {
    ExplanationError::Reference {
        path: path.into(),
        reason,
    }
}
fn validate_reference_shape(
    reference: &ExplanationReference,
    path: &str,
) -> Result<(), ExplanationError> {
    match reference {
        ExplanationReference::Page { url } => text(&format!("{path}.url"), url, 1, 4096, true)?,
        ExplanationReference::Code {
            group_id,
            path: target,
            start_line,
            end_line,
            ..
        } => {
            text(&format!("{path}.group_id"), group_id, 1, 64, true)?;
            text(&format!("{path}.path"), target, 1, 4096, true)?;
            if !matches!((start_line, end_line), (None, None))
                && !matches!((start_line,end_line),(Some(a),Some(b)) if *a>0 && b>=a)
            {
                return Err(reference_failure(path, ReferenceFailure::RangeShape));
            }
        }
    }
    Ok(())
}
pub fn validate_references(
    document: &ExplanationDocument,
    base: &str,
    context: &ReferenceContext<'_>,
) -> Result<(), ExplanationError> {
    for (i, block) in document.sections.iter().flat_map(|s| &s.blocks).enumerate() {
        for (j, reference) in block.references.iter().enumerate() {
            let path = format!("blocks[{i}].references[{j}]");
            validate_reference_shape(reference, &path)?;
            match reference {
                ExplanationReference::Page { url } => {
                    if !context.pages.contains(url) {
                        return Err(reference_failure(&path, ReferenceFailure::PageUnregistered));
                    }
                }
                ExplanationReference::Code {
                    unit,
                    group_id,
                    path: target,
                    side,
                    start_line: _,
                    end_line,
                } => {
                    if base != context.current_token {
                        continue;
                    }
                    let mut matching = context.code.iter().filter(|e| {
                        e.unit == *unit && e.group_id == *group_id && e.path == *target
                    });
                    let entry = matching.next().ok_or_else(|| {
                        reference_failure(&path, ReferenceFailure::MissingOrAmbiguous)
                    })?;
                    if matching.next().is_some() {
                        return Err(reference_failure(
                            &path,
                            ReferenceFailure::MissingOrAmbiguous,
                        ));
                    }
                    let (present, lines) = match side {
                        ReferenceSide::Old => (entry.old_present, entry.old_lines),
                        ReferenceSide::New => (entry.new_present, entry.new_lines),
                    };
                    if !present {
                        return Err(reference_failure(&path, ReferenceFailure::SideAbsent));
                    }
                    if let Some(end) = end_line {
                        let lines = lines.ok_or_else(|| {
                            reference_failure(&path, ReferenceFailure::LinesUnknown)
                        })?;
                        if *end > lines {
                            return Err(reference_failure(&path, ReferenceFailure::LinesOutside));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
pub fn validate_update(
    document: &ExplanationDocument,
    expected: u64,
    current: u64,
    base: &str,
    context: &ReferenceContext<'_>,
) -> Result<u64, ExplanationError> {
    if expected != current {
        return Err(ExplanationError::RevisionConflict { expected, current });
    }
    if current >= MAX_REVISION {
        return Err(ExplanationError::RevisionLimit);
    }
    known_generation(base, context.review_id, context.issued_generation)
        .map_err(ExplanationError::DisplayVersion)?;
    validate_document(document)?;
    validate_references(document, base, context)?;
    Ok(current + 1)
}

#[cfg(test)]
mod tests {
    fn sized_version(revision: u64, size: usize) -> super::ExplanationVersion {
        let mut sections = document("").sections;
        sections[0].blocks = (0..32)
            .map(|i| super::ExplanationBlock {
                id: format!("b{i}"),
                part: super::Part::Code {
                    body: String::new(),
                    language: "x".into(),
                    role: super::super::parts::Role::Example,
                    title: None,
                },
                references: vec![],
            })
            .collect();
        let mut version = super::ExplanationVersion {
            revision,
            base_display_version: "review:1".into(),
            sections,
        };
        let baseline = super::canonical_size(&serde_json::to_value(&version).unwrap());
        let mut remaining = size - baseline;
        for block in &mut version.sections[0].blocks {
            let count = remaining.min(32_768);
            if let super::Part::Code { body, .. } = &mut block.part {
                *body = "x".repeat(count);
            }
            remaining -= count;
        }
        assert_eq!(remaining, 0);
        assert_eq!(
            super::canonical_size(&serde_json::to_value(&version).unwrap()),
            size
        );
        version
    }

    #[test]
    fn history_and_saved_content_accept_exact_byte_limits_and_refuse_one_more() {
        let versions: Vec<_> = (1..=16).map(|r| sized_version(r, 500_000)).collect();
        let current = sized_version(17, 388_608);
        let protected: Vec<_> = (1..=16).collect();
        let held = super::RetainedVersions {
            comments: &protected,
            events: &[],
            screens: &[],
            other_saved_bytes: 8_388_608,
        };
        assert!(super::history_candidate(&versions, current.clone(), &held).is_ok());
        assert!(super::history_candidate(&versions, sized_version(17, 388_609), &held).is_err());
        let held = super::RetainedVersions {
            other_saved_bytes: 8_388_609,
            ..held
        };
        assert!(super::history_candidate(&versions, current, &held).is_err());
        assert_eq!(versions.len(), 16);
    }

    #[test]
    fn stored_version_budget_includes_the_base_token_and_rejects_malformed_references() {
        let mut version = sized_version(1, 400_000);
        version.base_display_version = "review:1".into();
        assert!(super::validate_version(&version).is_ok());
        version.sections[0].blocks[0].references=serde_json::from_value(serde_json::json!([{"type":"code","unit":null,"group_id":"g","path":"a","side":"new","start_line":null,"end_line":1}])).unwrap();
        assert!(super::validate_version(&version).is_err());
        version.sections[0].blocks[0].references.clear();
        version.base_display_version = "review:01".into();
        assert!(super::validate_version(&version).is_err());
        version.base_display_version = "review:1".into();
        let mut remaining = 524_288 - super::string_bytes(&serde_json::to_value(&version).unwrap());
        for block in &mut version.sections[0].blocks {
            if let super::Part::Code { body, .. } = &mut block.part {
                let count = remaining.min(32_768 - body.len());
                body.push_str(&"x".repeat(count));
                remaining -= count;
            }
        }
        assert_eq!(remaining, 0);
        assert!(super::validate_version(&version).is_ok());
        version.base_display_version = "reviewx:1".into();
        assert!(super::validate_version(&version).is_err());
    }

    #[test]
    fn required_reference_fields_unknown_fields_null_parts_and_duplicate_keys_are_refused() {
        for reference in [
            r#"{"type":"code","group_id":"g","path":"a","side":"new","start_line":null,"end_line":null}"#,
            r#"{"type":"page","url":"/","url":"/two"}"#,
            r#"{"type":"page","url":"/","extra":1}"#,
        ] {
            assert!(serde_json::from_str::<super::ExplanationReference>(reference).is_err());
        }
        for document in [
            r#"{"sections":[],"sections":[]}"#,
            r#"{"sections":null}"#,
            r#"{"sections":[{"id":"s","title":"t","blocks":[{"id":"b","part":null,"references":[]}]}]}"#,
        ] {
            assert!(serde_json::from_str::<super::ExplanationDocument>(document).is_err());
        }
    }

    #[test]
    fn role_position_and_reference_changes_make_the_original_block_outdated() {
        let mut input = document("text");
        input.sections[0].blocks[0].part=serde_json::from_value(serde_json::json!({"type":"flow","title":"f","role":"example","nodes":[{"id":"a","kind":"process","label":"A","position":{"x":160,"y":100}}],"edges":[]})).unwrap();
        let original = input.sections[0].blocks[0].clone();
        if let super::Part::Flow { role, .. } = &mut input.sections[0].blocks[0].part {
            *role = super::super::parts::Role::Confirmed;
        }
        assert!(super::block_outdated(&original, &input));
        input.sections[0].blocks[0] = original.clone();
        if let super::Part::Flow { nodes, .. } = &mut input.sections[0].blocks[0].part {
            nodes[0].position.as_mut().unwrap().x += 1;
        }
        assert!(super::block_outdated(&original, &input));
        input.sections[0].blocks[0] = original.clone();
        input.sections[0].blocks[0].references =
            serde_json::from_value(serde_json::json!([{"type":"page","url":"/page"}])).unwrap();
        assert!(super::block_outdated(&original, &input));
    }

    #[test]
    fn explanation_budget_counts_all_string_values_without_normalization() {
        let mut input = document(&"日".repeat(16_384));
        for i in 1..11 {
            let mut block = input.sections[0].blocks[0].clone();
            block.id = format!("block{i}");
            input.sections[0].blocks.push(block);
        }
        assert!(super::validate_document(&input).is_err());
    }

    #[test]
    fn canonical_size_uses_utf8_and_required_escapes_without_whitespace() {
        assert_eq!(
            super::canonical_size(&serde_json::json!({"b":"日","a":"\n"})),
            "{\"a\":\"\\n\",\"b\":\"日\"}".len()
        );
    }

    #[test]
    fn only_unreferenced_old_versions_are_evicted_from_a_candidate() {
        let versions: Vec<_> = (1..=128)
            .map(|revision| super::ExplanationVersion {
                revision,
                base_display_version: "review:1".into(),
                sections: document("text").sections,
            })
            .collect();
        let next = super::ExplanationVersion {
            revision: 129,
            base_display_version: "review:1".into(),
            sections: document("next").sections,
        };
        let held = super::RetainedVersions {
            comments: &[1],
            events: &[2],
            screens: &[3],
            other_saved_bytes: 0,
        };
        let candidate = super::history_candidate(&versions, next.clone(), &held).unwrap();
        assert_eq!(candidate.len(), 128);
        assert_eq!(candidate.last().unwrap().revision, 129);
        assert!(candidate.iter().all(|v| v.revision != 4));
        for revision in 1..=3 {
            assert!(candidate.iter().any(|v| v.revision == revision));
        }
        let all: Vec<_> = (1..=128).collect();
        let held = super::RetainedVersions {
            screens: &all,
            ..held
        };
        assert!(super::history_candidate(&versions, next, &held).is_err());
        assert_eq!(versions.len(), 128);
    }

    #[test]
    fn current_base_checks_exact_group_path_side_and_known_line_count() {
        let mut input = document("text");
        input.sections[0].blocks[0].references = serde_json::from_value(serde_json::json!([{"type":"code","unit":null,"group_id":"worktree","path":"src/a.rs","side":"new","start_line":1,"end_line":2}])).unwrap();
        let entries = [super::CodeReferenceEntry {
            unit: None,
            group_id: "worktree".into(),
            path: "src/a.rs".into(),
            old_present: false,
            new_present: true,
            old_lines: None,
            new_lines: Some(2),
        }];
        let context = super::ReferenceContext {
            review_id: "review",
            current_token: "review:2",
            issued_generation: 2,
            code: &entries,
            pages: &[],
        };
        assert!(super::validate_update(&input, 0, 0, "review:2", &context).is_ok());
        input.sections[0].blocks[0].references = serde_json::from_value(serde_json::json!([{"type":"code","unit":null,"group_id":"other","path":"src/a.rs","side":"new","start_line":1,"end_line":2}])).unwrap();
        assert!(super::validate_update(&input, 0, 0, "review:2", &context).is_err());
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_ok());
        input.sections[0].blocks[0].references = serde_json::from_value(serde_json::json!([{"type":"code","unit":null,"group_id":"worktree","path":"src/a.rs","side":"old","start_line":null,"end_line":null}])).unwrap();
        assert!(super::validate_update(&input, 0, 0, "review:2", &context).is_err());
    }

    #[test]
    fn unknown_line_count_allows_whole_file_but_not_line_reference() {
        let mut input = document("text");
        let entries = [super::CodeReferenceEntry {
            unit: None,
            group_id: "worktree".into(),
            path: "src/a.rs".into(),
            old_present: false,
            new_present: true,
            old_lines: None,
            new_lines: None,
        }];
        let context = super::ReferenceContext {
            review_id: "review",
            current_token: "review:1",
            issued_generation: 1,
            code: &entries,
            pages: &[],
        };
        for (start, end, accepted) in [
            (None, None, true),
            (Some(1), Some(1), false),
            (None, Some(1), false),
            (Some(2), Some(1), false),
            (Some(0), Some(1), false),
        ] {
            input.sections[0].blocks[0].references = serde_json::from_value(serde_json::json!([{"type":"code","unit":null,"group_id":"worktree","path":"src/a.rs","side":"new","start_line":start,"end_line":end}])).unwrap();
            assert_eq!(
                super::validate_update(&input, 0, 0, "review:1", &context).is_ok(),
                accepted
            );
        }
    }

    #[test]
    fn page_references_only_use_registered_pages_even_at_old_base() {
        let mut input = document("text");
        input.sections[0].blocks[0].references =
            serde_json::from_value(serde_json::json!([{"type":"page","url":"/page?x=1"}])).unwrap();
        let context = super::ReferenceContext {
            review_id: "review",
            current_token: "review:2",
            issued_generation: 2,
            code: &[],
            pages: &[],
        };
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_err());
        let pages = ["/page?x=1".to_string()];
        let context = super::ReferenceContext {
            pages: &pages,
            ..context
        };
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_ok());
    }

    #[test]
    fn duplicate_sections_or_blocks_and_empty_sections_are_refused() {
        let context = super::ReferenceContext {
            review_id: "review",
            current_token: "review:1",
            issued_generation: 1,
            code: &[],
            pages: &[],
        };
        let mut input = document("text");
        input.sections.push(input.sections[0].clone());
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_err());
        input.sections[1].id = "other".into();
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_err());
        input.sections[1].blocks.clear();
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_err());
    }

    fn document(body: &str) -> super::ExplanationDocument {
        serde_json::from_value(serde_json::json!({"sections":[{"id":"overview","title":"Title","blocks":[{"id":"intent","part":{"type":"text","body":body},"references":[]}]}]})).unwrap()
    }

    #[test]
    fn typed_content_ignores_json_key_order_and_escapes_but_not_text_normalization() {
        let original = document("日本語\r\n");
        let reordered: super::ExplanationDocument = serde_json::from_str(r#"{"sections":[{"blocks":[{"references":[],"part":{"body":"\u65e5\u672c\u8a9e\r\n","type":"text"},"id":"intent"}],"title":"Title","id":"overview"}]}"#).unwrap();
        assert_eq!(original, reordered);
        assert_ne!(original, document("日本語\n"));
        assert_ne!(original, document("日本語\r\n "));
    }

    #[test]
    fn quote_stays_unchanged_when_block_moves_title_changes_or_content_returns() {
        let original = document("original");
        let quote = super::quote_block(&original, "intent").unwrap();
        let mut renamed = original.clone();
        renamed.sections[0].title = "Renamed".into();
        assert!(!super::block_outdated(&quote.block, &renamed));
        let blocks = std::mem::take(&mut renamed.sections[0].blocks);
        renamed.sections.push(super::ExplanationSection {
            id: "moved".into(),
            title: "Moved".into(),
            blocks,
        });
        assert!(!super::block_outdated(&quote.block, &renamed));
        assert!(super::block_outdated(&quote.block, &document("changed")));
        let empty = super::ExplanationDocument { sections: vec![] };
        assert!(super::block_outdated(&quote.block, &empty));
        assert!(!super::block_outdated(&quote.block, &original));
        assert_eq!(quote.section_title, "Title");
        assert_eq!(quote.block, original.sections[0].blocks[0]);
    }

    #[test]
    fn update_requires_expected_revision_and_a_known_review_base() {
        let input = document("text");
        let context = super::ReferenceContext {
            review_id: "review",
            current_token: "review:2",
            issued_generation: 2,
            code: &[],
            pages: &[],
        };
        assert!(super::validate_update(&input, 1, 0, "review:1", &context).is_err());
        assert!(super::validate_update(&input, 0, 0, "review:1", &context).is_ok());
        assert!(super::validate_update(&input, 0, 0, "other:1", &context).is_err());
        assert!(super::validate_update(&input, 0, 0, "review:3", &context).is_err());
        assert!(
            super::validate_update(
                &input,
                9_007_199_254_740_991,
                9_007_199_254_740_991,
                "review:1",
                &context
            )
            .is_err()
        );
    }
}
