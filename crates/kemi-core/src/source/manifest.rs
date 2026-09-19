//! manifest モード（R-INPUT-1）。旧 `diff-review` の manifest と互換のキーを読む。

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::domain::noise::{classify, linguist_generated, NoiseInput};
use crate::domain::review::{Approval, FileEntry, Group, ReviewMeta, Status};
use crate::source::{
    read_side, text_stats, Plan, PlanStore, PlannedFile, ReviewSource, SideRef, SourceError,
};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default)]
    pub groups: Vec<ManifestGroup>,
    #[serde(default)]
    pub approval: Vec<ManifestApproval>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ManifestGroup {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub watch: String,
    #[serde(default)]
    pub diffs: Vec<ManifestDiff>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ManifestDiff {
    pub path: String,
    #[serde(default)]
    pub old: Option<String>,
    #[serde(default)]
    pub new: Option<String>,
    #[serde(default)]
    pub old_path: Option<String>,
    #[serde(default)]
    pub new_path: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub renamed_from: Option<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ManifestApproval {
    pub path: String,
    pub identity: String,
}

/// manifest の JSON を読み、レビューと内容参照の計画を作る。
pub(crate) fn build_review(
    manifest: &Manifest,
    base: &Path,
) -> Result<(ReviewMeta, Plan), SourceError> {
    let attributes = std::fs::read_to_string(base.join(".gitattributes")).unwrap_or_default();

    let mut groups = Vec::new();
    let mut plan = Plan::default();
    let mut next_file = 1usize;

    for (group_index, group) in manifest.groups.iter().enumerate() {
        let group_id = group
            .id
            .clone()
            .unwrap_or_else(|| format!("g{}", group_index + 1));
        let mut files = Vec::new();
        for diff in &group.diffs {
            let file_id = format!("f{next_file}");
            next_file += 1;
            let (entry, planned) = build_diff(diff, base, &file_id, &group_id, &attributes)?;
            files.push(entry);
            plan.files.push(planned);
        }
        groups.push(Group {
            id: group_id,
            title: group.title.clone(),
            why: group.why.clone(),
            watch: group.watch.clone(),
            files,
        });
    }

    let review = ReviewMeta {
        title: manifest
            .title
            .clone()
            .unwrap_or_else(|| "Review of changes".to_string()),
        subtitle: manifest.subtitle.clone().unwrap_or_default(),
        meta: manifest.meta.clone(),
        groups,
        approval: manifest
            .approval
            .iter()
            .map(|approval| Approval {
                path: approval.path.clone(),
                identity: approval.identity.clone(),
            })
            .collect(),
    };

    Ok((review, plan))
}

fn build_diff(
    diff: &ManifestDiff,
    base: &Path,
    file_id: &str,
    group_id: &str,
    attributes: &str,
) -> Result<(FileEntry, PlannedFile), SourceError> {
    let (old_ref, old_present) =
        resolve_side(diff.old.as_deref(), diff.old_path.as_deref(), base, "old")?;
    let (new_ref, new_present) =
        resolve_side(diff.new.as_deref(), diff.new_path.as_deref(), base, "new")?;
    if !old_present && !new_present {
        return Err(SourceError::Manifest(format!(
            "{}: neither old / old_path nor new / new_path is given",
            diff.path
        )));
    }

    let status = infer_status(
        diff.status.as_deref(),
        diff.renamed_from.as_deref(),
        old_present,
        new_present,
        &diff.path,
    )?;
    let old_bytes = read_side(&old_ref)?;
    let new_bytes = read_side(&new_ref)?;
    let (binary, add, del, old_size, new_size) = text_stats(&old_bytes, &new_bytes);
    let noise = classify(&NoiseInput {
        path: &diff.path,
        linguist_generated: linguist_generated(attributes, &diff.path),
        binary,
    })
    .is_some();

    let entry = FileEntry {
        id: file_id.to_string(),
        group_id: group_id.to_string(),
        path: diff.path.clone(),
        old_path: diff.renamed_from.clone(),
        status,
        add,
        del,
        binary,
        old_size,
        new_size,
        focus: diff.focus,
        note: diff.note.clone(),
        noise,
        content_skipped: false,
    };
    let planned = PlannedFile {
        id: file_id.to_string(),
        old: old_ref,
        new: new_ref,
    };
    Ok((entry, planned))
}

fn resolve_side(
    inline: Option<&str>,
    path: Option<&str>,
    base: &Path,
    side: &str,
) -> Result<(SideRef, bool), SourceError> {
    match (inline, path) {
        (Some(_), Some(_)) => Err(SourceError::Manifest(format!(
            "{side} and {side}_path cannot be given together"
        ))),
        (Some(text), None) => Ok((SideRef::Inline(text.as_bytes().to_vec()), true)),
        (None, Some(relative)) => {
            let full = base.join(relative);
            if !full.is_file() {
                return Err(SourceError::Manifest(format!("{relative} not found")));
            }
            Ok((SideRef::Disk(full), true))
        }
        (None, None) => Ok((SideRef::Absent, false)),
    }
}

fn infer_status(
    explicit: Option<&str>,
    renamed_from: Option<&str>,
    old_present: bool,
    new_present: bool,
    path: &str,
) -> Result<Status, SourceError> {
    match explicit {
        Some("add") => Ok(Status::Add),
        Some("delete") => Ok(Status::Delete),
        Some("rename") => Ok(Status::Rename),
        Some("modify") => Ok(Status::Modify),
        Some(other) => Err(SourceError::Manifest(format!(
            "{path}: unknown status: {other}"
        ))),
        None => {
            if renamed_from.is_some() {
                Ok(Status::Rename)
            } else if old_present && !new_present {
                Ok(Status::Delete)
            } else if !old_present && new_present {
                Ok(Status::Add)
            } else {
                Ok(Status::Modify)
            }
        }
    }
}

/// manifest を供給元にしたレビュー。
pub struct ManifestSource {
    manifest: Manifest,
    base: PathBuf,
    store: PlanStore,
}

impl ManifestSource {
    pub fn from_reader(mut reader: impl Read, base: &Path) -> Result<Self, SourceError> {
        let mut json = String::new();
        reader
            .read_to_string(&mut json)
            .map_err(|source| SourceError::Io {
                path: PathBuf::from("<stdin>"),
                source,
            })?;
        Self::from_json(&json, base)
    }

    pub fn from_json(json: &str, base: &Path) -> Result<Self, SourceError> {
        let manifest: Manifest = serde_json::from_str(json)
            .map_err(|error| SourceError::Manifest(format!("cannot read manifest: {error}")))?;
        Ok(ManifestSource {
            manifest,
            base: base.to_path_buf(),
            store: PlanStore::new(),
        })
    }

    pub fn from_path(path: &Path, base: &Path) -> Result<Self, SourceError> {
        if path == Path::new("-") {
            return Self::from_reader(std::io::stdin().lock(), base);
        }
        let json = std::fs::read_to_string(path).map_err(|source| SourceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json(&json, base)
    }
}

impl ReviewSource for ManifestSource {
    fn review(&self) -> Result<ReviewMeta, SourceError> {
        let (review, plan) = build_review(&self.manifest, &self.base)?;
        self.store.update(plan);
        Ok(review)
    }

    fn content(&self, file_id: &str) -> Result<crate::source::FileContent, SourceError> {
        self.store.content(file_id)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for group in &self.manifest.groups {
            for diff in &group.diffs {
                if let Some(new_path) = &diff.new_path {
                    paths.push(self.base.join(new_path));
                }
            }
        }
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn fixture(name: &str) -> PathBuf {
        repo_root().join("tests/fixtures").join(name)
    }

    fn meta_json(review: &ReviewMeta) -> Value {
        json!({
            "title": review.title,
            "groups": review.groups.iter().map(|group| json!({
                "id": group.id,
                "title": group.title,
                "watch": group.watch,
                "files": group.files.iter().map(|file| json!({
                    "path": file.path,
                    "old_path": file.old_path,
                    "status": file.status.as_str(),
                    "add": file.add,
                    "del": file.del,
                    "focus": file.focus,
                    "note": file.note,
                    "noise": file.noise,
                    "binary": file.binary,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "approval": review.approval.iter().map(|approval| json!({
                "path": approval.path,
                "identity": approval.identity,
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn manifest_compatible_keys_match_expected_metadata() {
        let source =
            ManifestSource::from_path(&fixture("legacy-manifest.json"), &repo_root()).unwrap();
        let review = source.review().unwrap();

        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(fixture("legacy-manifest.expected.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta_json(&review), expected);
    }

    #[test]
    fn manifest_omitted_ids_are_numbered_in_order() {
        let source = ManifestSource::from_json(
            r#"{"groups":[{"diffs":[]},{"id":"custom","diffs":[]},{"diffs":[]}]}"#,
            Path::new("."),
        )
        .unwrap();
        let review = source.review().unwrap();

        let ids: Vec<&str> = review
            .groups
            .iter()
            .map(|group| group.id.as_str())
            .collect();
        assert_eq!(ids, vec!["g1", "custom", "g3"]);
    }

    #[test]
    fn manifest_default_title_is_change_review() {
        let source = ManifestSource::from_json(r#"{"groups":[]}"#, Path::new(".")).unwrap();
        assert_eq!(source.review().unwrap().title, "Review of changes");
    }

    #[test]
    fn manifest_rejects_old_and_old_path_together() {
        let json = r#"{"groups":[{"diffs":[{"path":"a.rs","old":"x","old_path":"y","new":"z"}]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();

        let error = source.review().unwrap_err();
        assert!(error.to_string().contains("old_path"), "{error}");
    }

    #[test]
    fn manifest_rejects_missing_path_instead_of_empty_file() {
        let json =
            r#"{"groups":[{"diffs":[{"path":"a.rs","old_path":"does/not/exist.rs","new":"z"}]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();

        let error = source.review().unwrap_err();
        assert!(error.to_string().contains("does/not/exist.rs"), "{error}");
    }

    #[test]
    fn manifest_reads_from_any_reader() {
        let json = r#"{"groups":[]}"#;
        let source = ManifestSource::from_reader(json.as_bytes(), Path::new(".")).unwrap();
        assert_eq!(source.review().unwrap().groups.len(), 0);
    }

    #[test]
    fn manifest_infers_status_from_sides_and_renamed_from() {
        let json = r#"{"groups":[{"diffs":[
            {"path":"a.rs","new":"x\n"},
            {"path":"b.rs","old":"x\n"},
            {"path":"c.rs","old":"x\n","new":"y\n"},
            {"path":"d.rs","renamed_from":"old.rs","old":"x\n","new":"x\n"}
        ]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();
        let review = source.review().unwrap();

        let statuses: Vec<&str> = review.groups[0]
            .files
            .iter()
            .map(|file| file.status.as_str())
            .collect();
        assert_eq!(statuses, vec!["add", "delete", "modify", "rename"]);
    }

    #[test]
    fn manifest_counts_added_and_deleted_lines() {
        let json = r#"{"groups":[{"diffs":[
            {"path":"a.rs","old":"one\ntwo\nthree\n","new":"one\nTWO\nthree\n"},
            {"path":"b.rs","new":"one\ntwo\n"}
        ]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();
        let review = source.review().unwrap();

        assert_eq!(
            (review.groups[0].files[0].add, review.groups[0].files[0].del),
            (1, 1)
        );
        assert_eq!(
            (review.groups[0].files[1].add, review.groups[0].files[1].del),
            (2, 0)
        );
    }

    #[test]
    fn manifest_rejects_status_that_is_not_a_known_value() {
        let json =
            r#"{"groups":[{"diffs":[{"path":"a.rs","old":"x","new":"y","status":"copy"}]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();

        assert!(source.review().is_err());
    }

    #[test]
    fn manifest_marks_binary_inline_content() {
        let json =
            r#"{"groups":[{"diffs":[{"path":"a.bin","old":"\u0000","new":"\u0000\u0000"}]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();
        let review = source.review().unwrap();
        let file = &review.groups[0].files[0];

        assert!(file.binary);
        assert_eq!((file.add, file.del), (0, 0));
        assert_eq!((file.old_size, file.new_size), (1, 2));
        assert!(file.noise);
    }

    #[test]
    fn manifest_noise_from_gitattributes_is_applied() {
        let json = r#"{"groups":[{"diffs":[{"path":"src/schema.rs","old":"x\n","new":"y\n"}]}]}"#;
        let base = fixture("attr-base");
        let source = ManifestSource::from_json(json, &base).unwrap();
        let review = source.review().unwrap();

        assert!(review.groups[0].files[0].noise);
    }

    #[test]
    fn manifest_content_is_read_at_display_time() {
        let json =
            r#"{"groups":[{"diffs":[{"path":"a.rs","old":"one\ntwo\n","new":"one\nTWO\n"}]}]}"#;
        let source = ManifestSource::from_json(json, Path::new(".")).unwrap();
        source.review().unwrap();

        let content = source.content("f1").unwrap();
        assert_eq!(content.old.unwrap(), b"one\ntwo\n");
        assert_eq!(content.new.unwrap(), b"one\nTWO\n");
    }
}
