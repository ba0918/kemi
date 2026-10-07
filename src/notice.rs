//! サーバからの知らせを stderr の 1 行にする。ライブラリは出力せず、書き方はここで決める。

use kemi_server::{Notice, NoticeSink, WatchFallback};

/// 知らせを stderr に書く。
pub struct StderrNotices;

impl NoticeSink for StderrNotices {
    fn notify(&self, notice: Notice) {
        eprintln!("{}", notice_line(&notice));
    }
}

/// 知らせ 1 つを表す stderr の行。
pub fn notice_line(notice: &Notice) -> String {
    match notice {
        Notice::CommentAdded { path, side, lines } => {
            let location = match lines {
                Some((start, end)) => format!("{start}-{end}"),
                None => "file-wide".to_string(),
            };
            format!("kemi: comment {path} {} {location}", side.as_str())
        }
        Notice::PageCommentAdded { url, width, places } => {
            format!("kemi: comment page {url} {width} {places} places")
        }
        Notice::CommentImageNotSaved(error) => {
            format!("kemi: could not save the comment image: {error}")
        }
        Notice::SnapshotNotSaved(error) => {
            format!("kemi: could not save the snapshot: {error}")
        }
        Notice::ResultNotSaved(error) => format!("kemi: could not save the result file: {error}"),
        Notice::SessionNotSaved(error) => format!("kemi: could not save the session: {error}"),
        Notice::SessionNotDeleted(error) => format!("kemi: could not delete the session: {error}"),
        Notice::OriginUnknown { path, reason } => {
            format!("kemi: cannot compute the origin of {path}: {reason}")
        }
        Notice::ServedNotWatched { directory, reason } => format!(
            "kemi: cannot watch {} for page reloads: {reason}",
            directory.display()
        ),
        Notice::WorkTreeNotWatched(fallback) => {
            let reason = match fallback {
                WatchFallback::TooManyDirectories { limit } => {
                    format!("more than {limit} directories to watch")
                }
                WatchFallback::Refused(reason) => reason.clone(),
            };
            format!("kemi: watching only the files in the starting diff for updates: {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kemi_core::session::SessionError;
    use kemi_server::ResultSaveError;

    use kemi_core::domain::review::Side;

    use super::*;

    fn disk_full() -> SessionError {
        SessionError::Io {
            path: PathBuf::from("sessions/x.session"),
            source: std::io::Error::other("disk is full"),
        }
    }

    #[test]
    fn a_line_comment_names_the_path_side_and_range() {
        let notice = Notice::CommentAdded {
            path: "src/a.rs".to_string(),
            side: Side::New,
            lines: Some((2, 3)),
        };
        assert_eq!(notice_line(&notice), "kemi: comment src/a.rs new 2-3");
    }

    #[test]
    fn a_file_wide_comment_is_marked_file_wide() {
        let notice = Notice::CommentAdded {
            path: "src/a.rs".to_string(),
            side: Side::Old,
            lines: None,
        };
        assert_eq!(notice_line(&notice), "kemi: comment src/a.rs old file-wide");
    }

    #[test]
    fn failures_are_warnings_that_carry_the_reason() {
        let result = Notice::ResultNotSaved(ResultSaveError::Unlocated("no home".to_string()));
        assert_eq!(
            notice_line(&result),
            "kemi: could not save the result file: cannot determine where to store results (no home)"
        );
        let saved = notice_line(&Notice::SessionNotSaved(disk_full()));
        assert!(saved.starts_with("kemi: could not save the session: "));
        assert!(saved.contains("disk is full"));
        let deleted = notice_line(&Notice::SessionNotDeleted(disk_full()));
        assert!(deleted.starts_with("kemi: could not delete the session: "));
        let origin = Notice::OriginUnknown {
            path: "a.txt".to_string(),
            reason: "bad object".to_string(),
        };
        assert_eq!(
            notice_line(&origin),
            "kemi: cannot compute the origin of a.txt: bad object"
        );
    }
}
