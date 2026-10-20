//! 起動時の入力から、ソースとセッションを組む（R-INPUT、R-PAGE-MODE、R-SESSION）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kemi_core::domain::live::{LiveError, LivePage, is_html, parse_live_url, served_path};
use kemi_core::result;
use kemi_core::session::{SessionInfo, SessionMode, SessionStore, now_millis};
use kemi_core::source::git::{GitMode, GitSource};
use kemi_core::source::manifest::ManifestSource;
use kemi_core::source::{FocusSource, LiveSource, ReviewSource};
use kemi_server::StoredSession;

use crate::cli::{Cli, InputMode};
use crate::place::LaunchPlace;
use crate::report::warn;
use crate::results::{results_unset_reason, sessions_dir};

/// `--live` の見る対象と配れる範囲の根を決める（R-PAGE-MODE）。根は起動したディレクトリが
/// git の作業ツリーの中なら作業ツリーの根、外なら起動したディレクトリ。ファイルは
/// シンボリックリンクを解決した実体の場所で判定する。
pub(crate) fn live_target(value: &str, place: &LaunchPlace) -> Result<(LivePage, PathBuf), String> {
    let root = std::fs::canonicalize(place.workspace())
        .map_err(|error| format!("cannot resolve the launch directory: {error}"))?;
    if value.contains("://") {
        let url = parse_live_url(value).map_err(|error| error.to_string())?;
        return Ok((LivePage::Url(url.text), root));
    }
    let real =
        std::fs::canonicalize(value).map_err(|error| format!("cannot read {value}: {error}"))?;
    let path = served_path(&root, &real).map_err(|error| error.to_string())?;
    if !is_html(&path) {
        return Err(LiveError::NotHtml(value.to_string()).to_string());
    }
    Ok((LivePage::File(path), root))
}

/// 起動時の入力から、セッションに記録する情報を組み立てる。コミット範囲は完全な sha に
/// 解決しておく（R-SESSION）。
fn session_info(
    input: &InputMode<(LivePage, PathBuf)>,
    place: &LaunchPlace,
) -> Result<SessionInfo, String> {
    let workspace = place.workspace();
    let mode = match input {
        InputMode::Live((page, root)) => SessionMode::Live {
            page: page.clone(),
            root: root.clone(),
        },
        InputMode::Worktree => SessionMode::Worktree,
        InputMode::Staged => SessionMode::Staged,
        InputMode::Manifest(_) => SessionMode::Manifest,
        InputMode::Range { from, to, .. } => {
            let (from_sha, to_sha) = kemi_core::source::git::resolve_range(place.repo()?, from, to)
                .map_err(|error| error.to_string())?;
            SessionMode::Range {
                from: from.clone(),
                to: to.clone(),
                from_sha,
                to_sha,
            }
        }
    };
    let now = now_millis();
    Ok(SessionInfo {
        id: kemi_core::session::new_ulid(),
        created: now,
        updated: now,
        workspace_key: result::workspace_key(&workspace),
        workspace,
        mode,
        title: String::new(),
        total_files: 0,
    })
}

/// セッションの保存先を開く。決められない・開けないときは警告だけを出し、セッション
/// なしでレビューを続ける。
pub(crate) fn open_session(
    input: &InputMode<(LivePage, PathBuf)>,
    place: &LaunchPlace,
) -> Option<Arc<StoredSession>> {
    let Some(dir) = sessions_dir() else {
        warn(&format!(
            "cannot determine where to save sessions ({})",
            results_unset_reason()
        ));
        return None;
    };
    let info = match session_info(input, place) {
        Ok(info) => info,
        Err(message) => {
            warn(&format!("cannot start a session: {message}"));
            return None;
        }
    };
    match StoredSession::start(&SessionStore::new(dir), info) {
        Ok(session) => Some(Arc::new(session)),
        Err(error) => {
            warn(&format!("cannot start a session: {error}"));
            None
        }
    }
}

pub(crate) fn build_source(
    cli: &Cli,
    input: &InputMode<(LivePage, PathBuf)>,
    place: &LaunchPlace,
) -> Result<Box<dyn ReviewSource>, String> {
    let git = |mode| Ok::<_, String>(GitSource::new(place.repo()?.to_path_buf(), mode));
    Ok(match input {
        InputMode::Live((page, root)) => Box::new(LiveSource::new(page, root)),
        InputMode::Manifest(manifest) => Box::new(
            ManifestSource::from_path(Path::new(manifest), &cli.base)
                .map_err(|error| error.to_string())?,
        ),
        InputMode::Worktree => Box::new(git(GitMode::Worktree)?),
        InputMode::Staged => Box::new(git(GitMode::Staged)?),
        InputMode::Range { from, to, group_by } => Box::new(git(GitMode::Range {
            from: from.clone(),
            to: to.clone(),
            group_by: *group_by,
        })?),
    })
}

pub(crate) fn with_focus(
    source: Box<dyn ReviewSource>,
    cli: &Cli,
) -> Result<Box<dyn ReviewSource>, String> {
    match &cli.focus {
        Some(focus) => Ok(Box::new(
            FocusSource::from_path(source, Path::new(focus), &cli.base)
                .map_err(|error| error.to_string())?,
        )),
        None => Ok(source),
    }
}
