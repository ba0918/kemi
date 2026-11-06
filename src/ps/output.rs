//! `kemi ps` の出力（R-PS-OUTPUT）。JSON はキーを約束し、text は人が読む表で見た目を約束しない
//! （D18）。

use std::path::Path;

use super::listing::{Listing, Row};
use crate::local_time;
use crate::session::column;

/// 1 行 1 要素の配列。記録が無い・読めないレビューは `id` 以外が `null`。
pub(crate) fn json(rows: &[Row]) -> serde_json::Value {
    rows.iter()
        .map(|row| {
            let record = row.record.as_ref();
            serde_json::json!({
                "id": row.id,
                "workspace": record.map(|record| record.workspace.to_string_lossy()),
                "mode": record.map(|record| record.mode),
                "label": record.map(|record| &record.label),
                "started_at": record.map(|record| local_time::format_utc_rfc3339(record.started_at)),
                "pid": record.map(|record| record.pid),
                "url": record.map(|record| &record.url),
                "live_url": record.and_then(|record| record.live_url.as_ref()),
                "version": record.map(|record| &record.version),
            })
        })
        .collect()
}

/// 人が読む表。URL は切らない。`any` のときはワークスペースも出し、`home` を `~` に縮める。
/// 範囲の外にレビューがあれば、その数と `--any` を最後の 1 行で知らせる。
pub(crate) fn text(listing: &Listing, any: bool, home: Option<&Path>) -> String {
    let mut out = if listing.rows.is_empty() {
        "no running reviews\n".to_string()
    } else {
        table(&listing.rows, any, home)
    };
    if !any && listing.outside > 0 {
        let reviews = if listing.outside == 1 {
            "review"
        } else {
            "reviews"
        };
        out.push_str(&format!(
            "{} running {reviews} in other places (kemi ps --any lists them)\n",
            listing.outside
        ));
    }
    out
}

fn table(rows: &[Row], any: bool, home: Option<&Path>) -> String {
    let live = rows
        .iter()
        .any(|row| row.record.as_ref().is_some_and(|r| r.live_url.is_some()));
    let mut header = vec!["ID", "MODE", "STARTED", "PID", "URL"];
    if live {
        header.push("LIVE");
    }
    if any {
        header.push("WORKSPACE");
    }
    let mut lines: Vec<Vec<String>> = vec![header.iter().map(ToString::to_string).collect()];
    lines.extend(
        rows.iter()
            .map(|row| cells(row, header.len(), live, any, home)),
    );
    let columns = lines[0].len();
    let widths: Vec<usize> = (0..columns)
        .map(|index| {
            lines
                .iter()
                .map(|line| line[index].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    lines
        .iter()
        .map(|line| {
            let padded: Vec<String> = line
                .iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect();
            format!("{}\n", padded.join("  ").trim_end())
        })
        .collect()
}

/// 1 行の値。`columns` は見出しの列の数。記録が無い・読めないレビューは `id` のほかを
/// `-` にする。
fn cells(row: &Row, columns: usize, live: bool, any: bool, home: Option<&Path>) -> Vec<String> {
    let Some(record) = &row.record else {
        let mut cells = vec![row.id.clone()];
        cells.resize(columns, "-".to_string());
        return cells;
    };
    let mut cells = vec![
        row.id.clone(),
        column(&record.label),
        local_time::format_local(record.started_at),
        record.pid.to_string(),
        record.url.clone(),
    ];
    if live {
        cells.push(record.live_url.clone().unwrap_or_else(|| "-".to_string()));
    }
    if any {
        cells.push(column(&with_tilde(&record.workspace, home)));
    }
    cells
}

/// ホームディレクトリの下のパスを `~` から書く。
fn with_tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => Path::new("~").join(rest).to_string_lossy().into_owned(),
        None => path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn workspaces_under_the_home_directory_start_with_a_tilde() {
        let home = PathBuf::from("/home/someone");

        assert_eq!(
            with_tilde(&home.join("work").join("repo"), Some(&home)),
            Path::new("~").join("work").join("repo").to_string_lossy()
        );
        assert_eq!(with_tilde(&home, Some(&home)), "~");
        assert_eq!(with_tilde(Path::new("/srv/repo"), Some(&home)), "/srv/repo");
        assert_eq!(with_tilde(&home, None), "/home/someone");
    }
}
