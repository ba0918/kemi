//! `<id>.files/` の置き方と 20 MB の規則（live.md の R-PAGE-SESSION）。
//!
//! `<id>.files/` は 1 段のディレクトリで、中身は名前だけで分かる。
//!
//! - スナップショット: `snapshot-<番号>-<種類>.json.gz`。番号は取った順。
//! - コメントの画像: `<コメントの id>.png`。
//!
//! 20 MB の規則と掃除は、中身を読まずに名前と大きさだけで決める。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::encoding;

/// 1 セッションの `<id>.files/` の上限（R-PAGE-SESSION）。
pub const FILES_LIMIT: u64 = 20 * 1024 * 1024;

/// スナップショットを取った時点（R-PAGE-SNAPSHOT）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotKind {
    Start,
    Handed,
    Manual,
}

impl SnapshotKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "start" => Some(SnapshotKind::Start),
            "handed" => Some(SnapshotKind::Handed),
            "manual" => Some(SnapshotKind::Manual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotKind::Start => "start",
            SnapshotKind::Handed => "handed",
            SnapshotKind::Manual => "manual",
        }
    }
}

const SNAPSHOT_PREFIX: &str = "snapshot-";
const SNAPSHOT_SUFFIX: &str = ".json.gz";
const IMAGE_SUFFIX: &str = ".png";

pub(crate) fn snapshot_name(number: u32, kind: SnapshotKind) -> String {
    format!(
        "{SNAPSHOT_PREFIX}{number}-{}{SNAPSHOT_SUFFIX}",
        kind.as_str()
    )
}

/// スナップショットのファイルの名前から、番号と種類を読む。
pub(crate) fn snapshot_of_name(name: &str) -> Option<(u32, SnapshotKind)> {
    let rest = name
        .strip_prefix(SNAPSHOT_PREFIX)?
        .strip_suffix(SNAPSHOT_SUFFIX)?;
    let (number, kind) = rest.split_once('-')?;
    Some((number.parse().ok()?, SnapshotKind::parse(kind)?))
}

pub(crate) fn image_name(comment_id: &str) -> String {
    format!("{comment_id}{IMAGE_SUFFIX}")
}

/// `<id>.files/` に置くスナップショット 1 つ（R-PAGE-SNAPSHOT）。中の形は kemi が決める
/// （live.md の DL3）。番号と種類はファイルの名前に、残りは gzip した JSON に入れる。
#[derive(Clone, Debug, PartialEq)]
pub struct PageSnapshot {
    /// 取った順の番号。レビューの間と、保存したものの中で重ならない。
    pub number: u32,
    pub kind: SnapshotKind,
    /// ページ（パスとクエリ）。
    pub page: String,
    pub width: u32,
    pub html: String,
    /// 要素の記述。形はページ用のスクリプトが決め、kemi は中身を読まない。
    pub description: Option<Value>,
}

#[derive(Serialize, Deserialize)]
struct SnapshotDto {
    page: String,
    width: u32,
    html: String,
    #[serde(default)]
    description: Option<Value>,
}

pub(crate) fn encode_snapshot(snapshot: &PageSnapshot) -> Result<Vec<u8>, String> {
    let dto = SnapshotDto {
        page: snapshot.page.clone(),
        width: snapshot.width,
        html: snapshot.html.clone(),
        description: snapshot.description.clone(),
    };
    let json = serde_json::to_vec(&dto).map_err(|error| error.to_string())?;
    Ok(encoding::gzip(&json))
}

/// 名前（番号と種類）と中身からスナップショットに戻す。
pub(crate) fn decode_snapshot(name: &str, bytes: &[u8]) -> Result<PageSnapshot, String> {
    let (number, kind) =
        snapshot_of_name(name).ok_or_else(|| "not a snapshot file name".to_string())?;
    let json =
        encoding::gunzip(bytes, encoding::DECOMPRESS_LIMIT).map_err(|error| error.to_string())?;
    let dto: SnapshotDto = serde_json::from_slice(&json).map_err(|error| error.to_string())?;
    Ok(PageSnapshot {
        number,
        kind,
        page: dto.page,
        width: dto.width,
        html: dto.html,
        description: dto.description,
    })
}

/// 新しいファイルを書く前に消すものと、消した後に入るか。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Room {
    pub remove: Vec<String>,
    pub fits: bool,
}

/// `<id>.files/` の今のファイル（名前と大きさ）に、`new_size` のファイルを足せるようにする。
/// 古い渡した時点のスナップショット → 古い手で取ったスナップショット → 削除したコメント
/// （`comments` に無いコメント）の画像の順に、入るまで消す。開始時のスナップショット、
/// 今あるコメントの画像、kemi が名付けていないファイルは消さない（R-PAGE-SESSION）。
pub(crate) fn make_room(
    files: &[(String, u64)],
    comments: &BTreeSet<String>,
    new_size: u64,
    limit: u64,
) -> Room {
    let mut total: u64 = files.iter().map(|(_, size)| size).sum::<u64>() + new_size;
    let mut handed = Vec::new();
    let mut manual = Vec::new();
    let mut images = Vec::new();
    for (name, size) in files {
        if let Some((number, kind)) = snapshot_of_name(name) {
            match kind {
                SnapshotKind::Start => {}
                SnapshotKind::Handed => handed.push((number, name, *size)),
                SnapshotKind::Manual => manual.push((number, name, *size)),
            }
        } else if let Some(comment) = name.strip_suffix(IMAGE_SUFFIX)
            && !comments.contains(comment)
        {
            images.push((comment_number(comment), name, *size));
        }
    }
    handed.sort();
    manual.sort();
    images.sort();
    let mut remove = Vec::new();
    for (_, name, size) in handed.into_iter().chain(manual).chain(images) {
        if total <= limit {
            break;
        }
        total -= size;
        remove.push(name.clone());
    }
    Room {
        remove,
        fits: total <= limit,
    }
}

/// `c12` の番号。古いコメントの画像から消すための並び順にだけ使う。
fn comment_number(comment: &str) -> u32 {
    comment
        .strip_prefix('c')
        .and_then(|number| number.parse().ok())
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    fn kept(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn file(name: String, size: u64) -> (String, u64) {
        (name, size)
    }

    #[test]
    fn handed_snapshots_go_oldest_first_before_manual_ones() {
        let files = vec![
            file(snapshot_name(1, SnapshotKind::Start), 4 * MB),
            file(snapshot_name(2, SnapshotKind::Manual), 4 * MB),
            file(snapshot_name(3, SnapshotKind::Handed), 4 * MB),
            file(snapshot_name(4, SnapshotKind::Handed), 4 * MB),
            file(snapshot_name(5, SnapshotKind::Manual), 2 * MB),
        ];

        let room = make_room(&files, &kept(&[]), 8 * MB, 20 * MB);

        assert!(room.fits);
        assert_eq!(
            room.remove,
            vec![
                snapshot_name(3, SnapshotKind::Handed),
                snapshot_name(4, SnapshotKind::Handed),
            ]
        );
    }

    #[test]
    fn manual_snapshots_go_oldest_first_once_no_handed_one_is_left() {
        let files = vec![
            file(snapshot_name(1, SnapshotKind::Start), 4 * MB),
            file(snapshot_name(2, SnapshotKind::Handed), 4 * MB),
            file(snapshot_name(3, SnapshotKind::Manual), 4 * MB),
            file(snapshot_name(10, SnapshotKind::Manual), 4 * MB),
        ];

        let room = make_room(&files, &kept(&[]), 9 * MB, 20 * MB);

        assert!(room.fits);
        assert_eq!(
            room.remove,
            vec![
                snapshot_name(2, SnapshotKind::Handed),
                snapshot_name(3, SnapshotKind::Manual),
            ]
        );
    }

    #[test]
    fn images_of_deleted_comments_go_after_the_snapshots() {
        let files = vec![
            file(image_name("c1"), 4 * MB),
            file(snapshot_name(1, SnapshotKind::Start), 4 * MB),
            file(snapshot_name(2, SnapshotKind::Manual), 4 * MB),
            file(image_name("c2"), 4 * MB),
            file(image_name("c3"), 4 * MB),
        ];

        let room = make_room(&files, &kept(&["c3"]), 5 * MB, 20 * MB);

        assert!(room.fits);
        assert_eq!(
            room.remove,
            vec![snapshot_name(2, SnapshotKind::Manual), image_name("c1")]
        );
    }

    #[test]
    fn the_start_snapshot_and_images_of_current_comments_are_kept_even_when_nothing_fits() {
        let files = vec![
            file(snapshot_name(1, SnapshotKind::Start), 8 * MB),
            file(snapshot_name(2, SnapshotKind::Manual), 2 * MB),
            file(image_name("c1"), 8 * MB),
        ];

        let room = make_room(&files, &kept(&["c1"]), 5 * MB, 20 * MB);

        assert!(!room.fits);
        assert_eq!(room.remove, vec![snapshot_name(2, SnapshotKind::Manual)]);
    }

    #[test]
    fn a_new_file_that_brings_the_total_exactly_to_the_limit_fits_without_removing() {
        let files = vec![
            file(snapshot_name(1, SnapshotKind::Start), 8 * MB),
            file(snapshot_name(2, SnapshotKind::Handed), 8 * MB),
        ];

        let room = make_room(&files, &kept(&[]), 4 * MB, 20 * MB);

        assert!(room.fits);
        assert!(room.remove.is_empty());
    }

    #[test]
    fn files_kemi_did_not_name_are_counted_but_never_removed() {
        let files = vec![
            file(".snapshot-3-manual.json.gz.tmp".to_string(), 8 * MB),
            file(snapshot_name(1, SnapshotKind::Start), 8 * MB),
        ];

        let room = make_room(&files, &kept(&[]), 5 * MB, 20 * MB);

        assert!(!room.fits);
        assert!(room.remove.is_empty());
    }

    #[test]
    fn a_snapshot_name_gives_back_its_number_and_kind() {
        let name = snapshot_name(12, SnapshotKind::Handed);

        assert_eq!(snapshot_of_name(&name), Some((12, SnapshotKind::Handed)));
        assert_eq!(snapshot_of_name(&image_name("c12")), None);
    }
}
