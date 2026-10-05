//! `--live` の見る対象の判定（R-PAGE-MODE）。URL がループバックの `http://` か、ファイルが
//! 配れる範囲の中の HTML かを、ファイルシステムに触らずに決める。実体の場所（シンボリック
//! リンクの解決）と範囲の根は呼び出し側が求めて渡す。

use std::net::Ipv4Addr;
use std::path::{Component, Path};

/// 見る対象。URL は正規化した文字列、ファイルは配れる範囲の根からの相対パス（`/` 区切り）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LivePage {
    Url(String),
    File(String),
}

impl LivePage {
    /// 画面の題・復元の一覧・submit に出すページの URL。ファイルは範囲の根からの
    /// 相対パスを `/` から始めたもの（R-PAGE-MODE）。
    pub fn display_url(&self) -> String {
        match self {
            LivePage::Url(url) => url.clone(),
            LivePage::File(path) => format!("/{path}"),
        }
    }

    /// レビューの題（R-INPUT）。
    pub fn title(&self) -> String {
        format!("Live review of {}", self.display_url())
    }
}

/// 中継するページの URL。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveUrl {
    /// `host:port`。ポートを省いた URL では 80 を補う。
    pub authority: String,
    /// パスとクエリ（`#` から後ろを含まない）。空なら `/`。
    pub path_and_query: String,
    /// 渡されたままの URL。題と保存に使う。
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveError {
    /// `http://` でない（`https://` を含む）。
    NotHttp(String),
    /// URL の形として読めない。
    Malformed(String),
    /// ホストがループバックでない。
    NotLoopback(String),
    /// 配れる範囲の外。
    OutsideRange(String),
    /// `.git/` の中。
    GitDirectory(String),
    /// `.html` / `.htm` でない。
    NotHtml(String),
    /// パスを UTF-8 で表せない。
    NotUtf8,
}

impl std::fmt::Display for LiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiveError::NotHttp(url) => write!(
                formatter,
                "--live accepts only http:// URLs on a loopback host: {url}"
            ),
            LiveError::Malformed(url) => write!(formatter, "--live cannot read the URL: {url}"),
            LiveError::NotLoopback(host) => write!(
                formatter,
                "--live accepts only localhost or 127.0.0.0/8, not {host}"
            ),
            LiveError::OutsideRange(path) => write!(
                formatter,
                "{path} is outside the directory kemi can serve (the git working tree, or the launch directory outside git)"
            ),
            LiveError::GitDirectory(path) => {
                write!(formatter, "{path} is inside .git and is not served")
            }
            LiveError::NotHtml(path) => {
                write!(formatter, "--live accepts only .html or .htm files: {path}")
            }
            LiveError::NotUtf8 => formatter.write_str("the path is not valid UTF-8"),
        }
    }
}

impl std::error::Error for LiveError {}

/// `--live <URL>` を読む。`http://` で、ホストが `localhost` か `127.0.0.0/8` のものだけ。
pub fn parse_live_url(text: &str) -> Result<LiveUrl, LiveError> {
    let Some(rest) = text.strip_prefix("http://") else {
        return Err(LiveError::NotHttp(text.to_string()));
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.is_empty() || authority.contains('@') {
        return Err(LiveError::Malformed(text.to_string()));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| LiveError::Malformed(text.to_string()))?,
        ),
        None => (authority, 80),
    };
    if !is_loopback_host(host) {
        return Err(LiveError::NotLoopback(host.to_string()));
    }
    let without_fragment = tail.split('#').next().unwrap_or_default();
    let path_and_query = if without_fragment.starts_with('/') {
        without_fragment.to_string()
    } else {
        format!("/{without_fragment}")
    };
    Ok(LiveUrl {
        authority: format!("{host}:{port}"),
        path_and_query,
        text: text.to_string(),
    })
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<Ipv4Addr>()
            .is_ok_and(|address| address.is_loopback())
}

/// 実体の場所 `real` を、配れる範囲の根 `root` からの相対パス（`/` 区切り）にする。
/// どちらもシンボリックリンクを解決した絶対パスで渡す。`.git` の中は配らない。
pub fn served_path(root: &Path, real: &Path) -> Result<String, LiveError> {
    let shown = || real.to_string_lossy().into_owned();
    let relative = real
        .strip_prefix(root)
        .map_err(|_| LiveError::OutsideRange(shown()))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => {
                if name == ".git" {
                    return Err(LiveError::GitDirectory(shown()));
                }
                parts.push(name.to_str().ok_or(LiveError::NotUtf8)?);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(LiveError::OutsideRange(shown()));
            }
        }
    }
    Ok(parts.join("/"))
}

/// `.html` か `.htm` か（大文字小文字を問わない）。
pub fn is_html(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_loopback_http_url_is_accepted_with_its_port_and_path() {
        let url = parse_live_url("http://127.0.0.1:5173/app?x=1#top").unwrap();
        assert_eq!(url.authority, "127.0.0.1:5173");
        assert_eq!(url.path_and_query, "/app?x=1");
        assert_eq!(url.text, "http://127.0.0.1:5173/app?x=1#top");
    }

    #[test]
    fn localhost_is_loopback_and_a_missing_port_and_path_get_defaults() {
        let url = parse_live_url("http://LocalHost").unwrap();
        assert_eq!(url.authority, "LocalHost:80");
        assert_eq!(url.path_and_query, "/");
    }

    #[test]
    fn any_address_in_127_slash_8_is_loopback() {
        assert!(parse_live_url("http://127.4.5.6:8000/").is_ok());
    }

    #[test]
    fn https_and_other_schemes_are_refused() {
        assert!(matches!(
            parse_live_url("https://localhost:5173/"),
            Err(LiveError::NotHttp(_))
        ));
        assert!(matches!(
            parse_live_url("ftp://localhost/"),
            Err(LiveError::NotHttp(_))
        ));
    }

    #[test]
    fn a_host_outside_the_loopback_is_refused() {
        assert!(matches!(
            parse_live_url("http://example.com/"),
            Err(LiveError::NotLoopback(_))
        ));
        assert!(matches!(
            parse_live_url("http://192.168.1.2:3000/"),
            Err(LiveError::NotLoopback(_))
        ));
        assert!(matches!(
            parse_live_url("http://localhost.example.com/"),
            Err(LiveError::NotLoopback(_))
        ));
    }

    #[test]
    fn user_info_and_a_bad_port_are_malformed() {
        assert!(matches!(
            parse_live_url("http://evil@127.0.0.1/"),
            Err(LiveError::Malformed(_))
        ));
        assert!(matches!(
            parse_live_url("http://127.0.0.1:99999/"),
            Err(LiveError::Malformed(_))
        ));
        assert!(matches!(
            parse_live_url("http:///"),
            Err(LiveError::Malformed(_))
        ));
    }

    #[test]
    fn a_file_inside_the_root_becomes_a_slash_separated_relative_path() {
        let root = PathBuf::from("/repo");
        assert_eq!(
            served_path(&root, &root.join("docs").join("design").join("mock.html")).unwrap(),
            "docs/design/mock.html"
        );
    }

    #[test]
    fn a_file_outside_the_root_is_refused() {
        assert!(matches!(
            served_path(Path::new("/repo"), Path::new("/other/page.html")),
            Err(LiveError::OutsideRange(_))
        ));
        assert!(matches!(
            served_path(Path::new("/repo"), Path::new("/repository/page.html")),
            Err(LiveError::OutsideRange(_))
        ));
    }

    #[test]
    fn a_file_inside_dot_git_is_refused_at_any_depth() {
        assert!(matches!(
            served_path(Path::new("/repo"), Path::new("/repo/.git/index.html")),
            Err(LiveError::GitDirectory(_))
        ));
        assert!(matches!(
            served_path(Path::new("/repo"), Path::new("/repo/sub/.git/x.html")),
            Err(LiveError::GitDirectory(_))
        ));
    }

    #[test]
    fn html_and_htm_are_html_in_any_case() {
        assert!(is_html("a/index.html"));
        assert!(is_html("a/INDEX.HTM"));
        assert!(!is_html("notes.txt"));
        assert!(!is_html("page.html.txt"));
    }

    #[test]
    fn a_file_page_is_shown_as_a_root_relative_url_in_the_title() {
        let page = LivePage::File("docs/mock.html".to_string());
        assert_eq!(page.display_url(), "/docs/mock.html");
        assert_eq!(page.title(), "Live review of /docs/mock.html");
        let page = LivePage::Url("http://localhost:5173/".to_string());
        assert_eq!(page.title(), "Live review of http://localhost:5173/");
    }
}
