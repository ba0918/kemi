//! セッションの一覧と選択画面の行（R-SESSION）。

use kemi_core::session::SessionSummary;

use crate::local_time;

/// 一覧の 1 行（R-SESSION）。5 列のタブ区切り。
pub fn list_line(summary: &SessionSummary) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}/{}",
        summary.id,
        local_time::format_local(summary.updated),
        column(&summary.workspace.to_string_lossy()),
        column(&summary.mode_label()),
        summary.seen,
        summary.total_files,
    )
}

/// 選択画面の行（R-SESSION）。列をそろえた 5 列で、タブと改行は空白にする。
pub fn select_labels(sessions: &[SessionSummary]) -> Vec<String> {
    let rows: Vec<[String; 5]> = sessions
        .iter()
        .map(|summary| {
            [
                column(&summary.id),
                local_time::format_local(summary.updated),
                shorten(&column(&summary.workspace.to_string_lossy()), 48),
                shorten(&column(&summary.mode_label()), 40),
                format!("{}/{}", summary.seen, summary.total_files),
            ]
        })
        .collect();
    let widths: [usize; 5] = rows.iter().fold([0; 5], |mut widths, row| {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.chars().count());
        }
        widths
    });
    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(index, value)| format!("{:<width$}", value, width = widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        })
        .collect()
}

/// 選択画面の列を画面に収めるための短縮。末尾に `...` を付ける。
fn shorten(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut shortened: String = value.chars().take(max.saturating_sub(3)).collect();
    shortened.push_str("...");
    shortened
}

/// 一覧の列の値。タブと改行は空白に置き換える（R-SESSION）。
fn column(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\t' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kemi_core::session::SessionMode;
    use std::path::PathBuf;

    fn summary(id: &str, updated: u128, mode: SessionMode, seen: usize) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            updated,
            workspace: PathBuf::from("/tmp/workspace"),
            mode,
            title: "Working tree changes".to_string(),
            seen,
            total_files: 4,
        }
    }

    #[test]
    fn list_line_has_five_tab_separated_columns_with_tabs_and_newlines_as_spaces() {
        let mut listed = summary(
            "01HF7YAT00AAAAAAAAAAAAAAAA",
            1_700_000_000_000,
            SessionMode::Manifest,
            1,
        );
        listed.title = "a\tb\nc\rd".to_string();
        listed.workspace = PathBuf::from("/tmp/work\tspace");

        let line = list_line(&listed);

        let columns: Vec<&str> = line.split('\t').collect();
        let updated = local_time::format_local(1_700_000_000_000);
        assert_eq!(
            columns,
            vec![
                "01HF7YAT00AAAAAAAAAAAAAAAA",
                updated.as_str(),
                "/tmp/work space",
                "a b c d",
                "1/4",
            ],
            "{line:?}"
        );
        assert_eq!(updated.len(), "YYYY-MM-DD HH:MM".len());
        assert!(!line.contains('\n') && !line.contains('\r'), "{line:?}");
    }

    #[test]
    fn select_labels_align_the_five_columns_and_sanitize_values() {
        let sessions = vec![
            summary(
                "01HF7YAT00AAAAAAAAAAAAAAAA",
                1_700_000_000_000,
                SessionMode::Worktree,
                1,
            ),
            summary(
                "01HF7YAT00BBBBBBBBBBBBBBBB",
                1_700_000_060_000,
                SessionMode::Manifest,
                2,
            ),
        ];
        let mut listed = sessions.clone();
        listed[1].title = "a\tb\nc".to_string();
        listed[1].mode = SessionMode::Manifest;

        let labels = select_labels(&listed);

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].split("  ").count(), 5, "{}", labels[0]);
        assert_eq!(
            labels[0].split("  ").next().unwrap(),
            "01HF7YAT00AAAAAAAAAAAAAAAA"
        );
        assert!(labels[1].contains("a b c"), "{}", labels[1]);
        assert!(labels[1].contains("2/4"), "{}", labels[1]);
        assert!(!labels[1].contains('\t'));
        assert!(!labels[1].contains('\n'));

        let long = vec![{
            let mut summary = summary(
                "01HF7YAT00CCCCCCCCCCCCCCCC",
                1_700_000_120_000,
                SessionMode::Manifest,
                3,
            );
            summary.title = "x".repeat(100);
            summary
        }];
        let label = select_labels(&long).remove(0);
        assert!(label.contains("..."), "{label}");
        assert!(!label.contains(&"x".repeat(50)), "{label}");
    }
}
