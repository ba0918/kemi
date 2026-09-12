//! focus レイヤ（R-INPUT-5, R-FOCUS）。`--focus` の後付けと検証。

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::domain::review::ReviewMeta;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusError {
    Parse(String),
    UnknownGroup(String),
    UnknownPath(String),
}

impl std::fmt::Display for FocusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusError::Parse(message) => write!(formatter, "focus を読めません: {message}"),
            FocusError::UnknownGroup(id) => {
                write!(formatter, "focus のグループ id が見つかりません: {id}")
            }
            FocusError::UnknownPath(path) => {
                write!(formatter, "focus のパスが見つかりません: {path}")
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FocusLayer {
    #[serde(default)]
    pub groups: BTreeMap<String, GroupFocus>,
    #[serde(default)]
    pub files: Vec<FileFocus>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GroupFocus {
    #[serde(default)]
    pub watch: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FileFocus {
    pub path: String,
    #[serde(default = "default_focus")]
    pub focus: bool,
    #[serde(default)]
    pub note: String,
}

fn default_focus() -> bool {
    true
}

pub fn parse_focus(json: &str) -> Result<FocusLayer, FocusError> {
    serde_json::from_str(json).map_err(|error| FocusError::Parse(error.to_string()))
}

/// `--focus` の照合で「存在する」とみなすグループ id とパス。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusTargets {
    pub group_ids: BTreeSet<String>,
    pub paths: BTreeSet<String>,
}

impl FocusTargets {
    /// レビューに現れるグループ id とパス。
    pub fn of(review: &ReviewMeta) -> Self {
        FocusTargets {
            group_ids: review.groups.iter().map(|group| group.id.clone()).collect(),
            paths: review
                .groups
                .iter()
                .flat_map(|group| group.files.iter().map(|file| file.path.clone()))
                .collect(),
        }
    }

    pub fn extend(&mut self, other: FocusTargets) {
        self.group_ids.extend(other.group_ids);
        self.paths.extend(other.paths);
    }
}

/// focus レイヤが指すグループ id とパスが存在するかを確かめる。黙って無視しない。
pub fn validate_focus(layer: &FocusLayer, targets: &FocusTargets) -> Result<(), FocusError> {
    for id in layer.groups.keys() {
        if !targets.group_ids.contains(id) {
            return Err(FocusError::UnknownGroup(id.clone()));
        }
    }
    for entry in &layer.files {
        if !targets.paths.contains(&entry.path) {
            return Err(FocusError::UnknownPath(entry.path.clone()));
        }
    }
    Ok(())
}

/// focus レイヤをレビューへ後付けする。照合は `validate_focus` で先に済ませる。
pub fn apply_focus(review: &mut ReviewMeta, layer: &FocusLayer) {
    for group in &mut review.groups {
        if let Some(overlay) = layer.groups.get(&group.id) {
            group.watch = overlay.watch.clone();
        }
        for file in &mut group.files {
            if let Some(overlay) = layer.files.iter().find(|entry| entry.path == file.path) {
                file.focus = overlay.focus;
                file.note = overlay.note.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::review::{FileEntry, Group, ReviewMeta, Status};
    use serde_json::json;

    fn file(path: &str) -> FileEntry {
        FileEntry {
            id: format!("f-{path}"),
            group_id: "g1".to_string(),
            path: path.to_string(),
            old_path: None,
            status: Status::Modify,
            add: 1,
            del: 1,
            binary: false,
            old_size: 0,
            new_size: 0,
            focus: false,
            note: String::new(),
            noise: false,
        }
    }

    fn review() -> ReviewMeta {
        ReviewMeta {
            title: "変更のレビュー".to_string(),
            subtitle: String::new(),
            meta: json!(null),
            groups: vec![
                Group {
                    id: "g1".to_string(),
                    title: "最初".to_string(),
                    why: String::new(),
                    watch: "manifest の watch".to_string(),
                    files: vec![file("src/a.rs"), file("src/b.rs")],
                },
                Group {
                    id: "g2".to_string(),
                    title: "次".to_string(),
                    why: String::new(),
                    watch: String::new(),
                    files: vec![file("src/a.rs")],
                },
            ],
            approval: Vec::new(),
        }
    }

    #[test]
    fn focus_parse_reads_watch_and_files() {
        let layer = parse_focus(
            r#"{"groups":{"g1":{"watch":"ここ"}},"files":[{"path":"src/a.rs","focus":true,"note":"決定点"}]}"#,
        )
        .unwrap();

        assert_eq!(layer.groups["g1"].watch, "ここ");
        assert_eq!(layer.files[0].path, "src/a.rs");
        assert!(layer.files[0].focus);
        assert_eq!(layer.files[0].note, "決定点");
    }

    #[test]
    fn focus_parse_rejects_unknown_group_key() {
        assert!(parse_focus(r#"{"groups":{"g1":{"note":"n"}}}"#).is_err());
    }

    #[test]
    fn focus_apply_overrides_manifest_watch() {
        let mut review = review();
        let layer = parse_focus(r#"{"groups":{"g1":{"watch":"focus の watch"}}}"#).unwrap();

        apply_focus(&mut review, &layer);
        assert_eq!(review.groups[0].watch, "focus の watch");
        assert_eq!(review.groups[1].watch, "");
    }

    #[test]
    fn focus_apply_sets_file_focus_and_note() {
        let mut review = review();
        let layer =
            parse_focus(r#"{"files":[{"path":"src/b.rs","focus":true,"note":"ここ"}]}"#).unwrap();

        apply_focus(&mut review, &layer);
        let b = &review.groups[0].files[1];
        assert!(b.focus);
        assert_eq!(b.note, "ここ");
        assert!(!review.groups[0].files[0].focus);
    }

    #[test]
    fn focus_apply_matches_path_in_every_group() {
        let mut review = review();
        let layer = parse_focus(r#"{"files":[{"path":"src/a.rs","focus":true}]}"#).unwrap();

        apply_focus(&mut review, &layer);
        assert!(review.groups[0].files[0].focus);
        assert!(review.groups[1].files[0].focus);
    }

    #[test]
    fn focus_apply_entry_without_flag_is_focused() {
        let mut review = review();
        let layer = parse_focus(r#"{"files":[{"path":"src/a.rs"}]}"#).unwrap();

        apply_focus(&mut review, &layer);
        assert!(review.groups[0].files[0].focus);
    }

    #[test]
    fn focus_validate_rejects_unknown_group_id() {
        let layer = parse_focus(r#"{"groups":{"g99":{"watch":"x"}}}"#).unwrap();

        let error = validate_focus(&layer, &FocusTargets::of(&review())).unwrap_err();
        assert_eq!(error, FocusError::UnknownGroup("g99".to_string()));
    }

    #[test]
    fn focus_validate_rejects_unknown_path() {
        let layer = parse_focus(r#"{"files":[{"path":"src/missing.rs"}]}"#).unwrap();

        let error = validate_focus(&layer, &FocusTargets::of(&review())).unwrap_err();
        assert_eq!(error, FocusError::UnknownPath("src/missing.rs".to_string()));
    }

    #[test]
    fn focus_validate_accepts_targets_beyond_the_review() {
        let layer =
            parse_focus(r#"{"groups":{"sha1":{"watch":"x"}},"files":[{"path":"src/gone.rs"}]}"#)
                .unwrap();
        let mut targets = FocusTargets::of(&review());
        targets.extend(FocusTargets {
            group_ids: ["sha1".to_string()].into(),
            paths: ["src/gone.rs".to_string()].into(),
        });

        assert_eq!(validate_focus(&layer, &targets), Ok(()));
    }

    #[test]
    fn focus_apply_of_empty_layer_adds_no_focus() {
        let mut review = review();
        review.groups[0].files[0].add = 100_000;

        let layer = parse_focus("{}").unwrap();
        apply_focus(&mut review, &layer);

        assert!(review
            .groups
            .iter()
            .all(|group| group.files.iter().all(|file| !file.focus)));
    }
}
