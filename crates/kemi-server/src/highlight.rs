//! 構文ハイライト（R-VIEW）。サーバ側で行ごとのスタイルを作る。
//!
//! 複数行にまたがる文字列・コメントは `HighlightLines` が行をまたいで状態を
//! 保つため、行ごとに切らずに内容全体を順に処理する。

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

use kemi_core::domain::diff::Segment;

/// 片側がこの行数を超えたら自動ではハイライトしない。
pub const HIGHLIGHT_MAX_LINES: usize = 10_000;
/// 片側がこのバイト数を超えたら自動ではハイライトしない。
pub const HIGHLIGHT_MAX_BYTES: usize = 1_048_576;

/// 1 行分のスタイル付き断片。
pub type HighlightedLine = Vec<(Style, String)>;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    themes: EmbeddedLazyThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        Highlighter {
            syntaxes: two_face::syntax::extra_newlines(),
            themes: two_face::theme::extra(),
        }
    }

    /// 自動でハイライトしてよい規模か。片側が行数かバイト数の上限を超えたら false。
    pub fn capable(old: Option<&str>, new: Option<&str>) -> bool {
        let within = |text: &str| {
            text.len() <= HIGHLIGHT_MAX_BYTES && text.lines().count() <= HIGHLIGHT_MAX_LINES
        };
        old.is_none_or(within) && new.is_none_or(within)
    }

    /// 内容を行ごとのスタイル列に変える。`dark` でテーマを選ぶ。
    pub fn highlight(&self, path: &str, content: &str, dark: bool) -> Vec<HighlightedLine> {
        let theme = &self.themes[if dark {
            EmbeddedThemeName::TwoDark
        } else {
            EmbeddedThemeName::InspiredGithub
        }];
        let syntax = self.syntax_for(path);
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut lines = Vec::new();

        for line in LinesWithEndings::from(content) {
            let mut ranges: HighlightedLine = highlighter
                .highlight_line(line, &self.syntaxes)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, text)| (style, text.to_string()))
                .collect();
            if let Some((_, last)) = ranges.last_mut() {
                while last.ends_with('\n') || last.ends_with('\r') {
                    last.pop();
                }
                if last.is_empty() {
                    ranges.pop();
                }
            }
            lines.push(ranges);
        }
        lines
    }

    fn syntax_for(&self, path: &str) -> &SyntaxReference {
        let extension = path.rsplit('.').next().unwrap_or("");
        self.syntaxes
            .find_syntax_by_extension(extension)
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text())
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// 1 行を HTML にする。`segments` の changed 部分は単語単位の強調で包む。
pub fn render_line(ranges: &HighlightedLine, segments: &[Segment]) -> String {
    let changed = changed_ranges(segments);
    let mut html = String::new();
    let mut offset = 0;

    for (style, text) in ranges {
        let start = offset;
        let end = offset + text.len();
        offset = end;
        if text.is_empty() {
            continue;
        }
        let mut boundaries = vec![start, end];
        for (change_start, change_end) in &changed {
            if *change_start > start && *change_start < end {
                boundaries.push(*change_start);
            }
            if *change_end > start && *change_end < end {
                boundaries.push(*change_end);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        for window in boundaries.windows(2) {
            let (left, right) = (window[0], window[1]);
            let piece = &text[left - start..right - start];
            let is_changed = changed
                .iter()
                .any(|(change_start, change_end)| *change_start <= left && right <= *change_end);
            html.push_str(&span(style, piece, is_changed));
        }
    }

    html
}

fn changed_ranges(segments: &[Segment]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for segment in segments {
        let end = offset + segment.text.len();
        if segment.changed {
            ranges.push((offset, end));
        }
        offset = end;
    }
    ranges
}

fn span(style: &Style, text: &str, changed: bool) -> String {
    let inner = if changed {
        format!("<span class=\"word-changed\">{}</span>", escape(text))
    } else {
        escape(text)
    };
    format!("<span style=\"{}\">{}</span>", style_css(style), inner)
}

fn style_css(style: &Style) -> String {
    let mut css = format!(
        "color:#{:02x}{:02x}{:02x}",
        style.foreground.r, style.foreground.g, style.foreground.b
    );
    if style.font_style.contains(FontStyle::BOLD) {
        css.push_str(";font-weight:bold");
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        css.push_str(";font-style:italic");
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        css.push_str(";text-decoration:underline");
    }
    css
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_multiline_string_keeps_style_on_later_lines() {
        let highlighter = Highlighter::new();
        let code = "let s = \"first line\nsecond line\";\n";
        let lines = highlighter.highlight("a.rs", code, false);

        assert_eq!(lines.len(), 2);
        let string_color = lines[0]
            .last()
            .map(|(style, _)| style.foreground)
            .expect("first line has ranges");
        assert!(
            lines[1]
                .iter()
                .any(|(style, text)| style.foreground == string_color
                    && text.contains("second line")),
            "second line lost the string color: {:?}",
            lines[1]
        );
    }

    #[test]
    fn highlight_multiline_block_comment_keeps_style_on_later_lines() {
        let highlighter = Highlighter::new();
        let code = "/* first\nstill comment */\nfn main() {}\n";
        let lines = highlighter.highlight("a.rs", code, false);

        assert_eq!(lines.len(), 3);
        let comment_color = lines[0]
            .first()
            .map(|(style, _)| style.foreground)
            .expect("first line has ranges");
        assert!(
            lines[1]
                .iter()
                .any(|(style, text)| style.foreground == comment_color
                    && text.contains("still comment")),
            "second line lost the comment color: {:?}",
            lines[1]
        );
    }

    #[test]
    fn highlight_cap_limits_by_line_count() {
        let short = "line\n".repeat(HIGHLIGHT_MAX_LINES);
        let long = "line\n".repeat(HIGHLIGHT_MAX_LINES + 1);

        assert!(Highlighter::capable(Some(&short), Some(&short)));
        assert!(!Highlighter::capable(Some(&long), Some(&short)));
        assert!(!Highlighter::capable(Some(&short), Some(&long)));
    }

    #[test]
    fn highlight_cap_limits_by_size() {
        let short = "x".repeat(HIGHLIGHT_MAX_BYTES);
        let long = "x".repeat(HIGHLIGHT_MAX_BYTES + 1);

        assert!(Highlighter::capable(Some(&short), None));
        assert!(!Highlighter::capable(Some(&long), None));
    }

    #[test]
    fn highlight_cap_ignores_absent_side() {
        assert!(Highlighter::capable(None, None));
        assert!(Highlighter::capable(None, Some("fn main() {}\n")));
    }

    #[test]
    fn render_line_marks_changed_segments_inside_highlighted_spans() {
        let highlighter = Highlighter::new();
        let lines = highlighter.highlight("a.rs", "let value = 1;\n", false);
        let segments = vec![
            Segment {
                text: "let value = ".to_string(),
                changed: false,
            },
            Segment {
                text: "1".to_string(),
                changed: true,
            },
            Segment {
                text: ";".to_string(),
                changed: false,
            },
        ];

        let html = render_line(&lines[0], &segments);
        assert!(html.contains("word-changed"), "{html}");
        assert!(html.contains("color:#"), "{html}");
    }
}
