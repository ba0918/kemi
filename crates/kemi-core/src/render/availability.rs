//! ファイルの応答に載せる、描画表示の可否の事前判定（R-RENDER）。

use crate::domain::content;
use crate::domain::review::FileEntry;

use super::{IMAGE_MAX_BYTES, ImageKind, Target, image_kind, target_of_file, unbalanced_line};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialView {
    Source,
    Rendered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderInfo {
    pub target: Option<Target>,
    pub toggle: bool,
    pub initial: InitialView,
    pub reason: Option<String>,
}

/// ファイルが描画表示の対象か、切り替えを持つか、既定はどちらか、事前に分かる描画不可の
/// 理由。内容を余分に読まないよう、行データのために既に読んだ内容だけで決める。
pub fn render_info(file: &FileEntry, old: Option<&str>, new: Option<&str>) -> RenderInfo {
    const TOO_LARGE: &str = "too large (over 10,000 lines or 1 MB on one side)";
    // 上限超えで内容を読まなかった untracked（R-INPUT-3）は、内容が無いので描画できない。
    const NOT_READ: &str = "the file exceeds the untracked limit and its content was not read";
    let not_read = file.content_skipped.then(|| NOT_READ.to_string());
    match target_of_file(file) {
        Some(Target::Markdown) => {
            let reason = not_read.or_else(|| {
                (!content::within_auto_limit(old, new)).then_some(TOO_LARGE.to_string())
            });
            RenderInfo {
                target: Some(Target::Markdown),
                toggle: true,
                initial: InitialView::Source,
                reason,
            }
        }
        Some(Target::Table { delimiter }) => {
            let reason = not_read.or_else(|| {
                if !content::within_auto_limit(old, new) {
                    Some(TOO_LARGE.to_string())
                } else {
                    [new, old]
                        .into_iter()
                        .flatten()
                        .find_map(|text| unbalanced_line(text, delimiter))
                        .map(|line| format!("unbalanced quote on line {line}"))
                }
            });
            RenderInfo {
                target: Some(Target::Table { delimiter }),
                toggle: true,
                initial: InitialView::Source,
                reason,
            }
        }
        Some(target @ Target::Image { .. }) => {
            // 画像には行数・バイト数の描画不可を当てず、5 MB の規則だけが効く。内容を
            // 読まなかったものは並べる画像が無いので、バイト数の増減だけになる。
            let (old_bytes, new_bytes) = side_bytes(file, old, new);
            let within = !file.content_skipped
                && old_bytes <= IMAGE_MAX_BYTES
                && new_bytes <= IMAGE_MAX_BYTES;
            match image_kind(file) {
                Some(kind) if within => RenderInfo {
                    target: Some(target),
                    toggle: kind == ImageKind::Svg,
                    initial: InitialView::Rendered,
                    reason: None,
                },
                Some(ImageKind::Raster | ImageKind::Svg) | None => source_only(),
            }
        }
        None => source_only(),
    }
}

fn source_only() -> RenderInfo {
    RenderInfo {
        target: None,
        toggle: false,
        initial: InitialView::Source,
        reason: None,
    }
}

/// 各側のバイト数。内容を読んだテキストはその長さ、読んでいないバイナリはエントリの
/// サイズ。
fn side_bytes(file: &FileEntry, old: Option<&str>, new: Option<&str>) -> (u64, u64) {
    if file.binary {
        (file.old_size, file.new_size)
    } else {
        (
            old.map_or(0, |text| text.len() as u64),
            new.map_or(0, |text| text.len() as u64),
        )
    }
}
