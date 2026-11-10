//! 構文ハイライト（R-VIEW）。サーバ側で行ごとのスタイルを作る。
//!
//! 複数行にまたがる文字列・コメントは `HighlightLines` が行をまたいで状態を
//! 保つため、行ごとに切らずに内容全体を順に処理する。

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style, Theme};
use syntect::parsing::{Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

use kemi_core::domain::diff::Segment;

use kemi_core::domain::content::within_auto_limit;

/// スタイル付きの断片。
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub style: Style,
    pub text: String,
    /// テーマがコメントに使う色の断片か。この断片の色はページが決める（`render_line`）。
    pub comment: bool,
}

/// 1 行分のスタイル付き断片。
pub type HighlightedLine = Vec<Token>;

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
        within_auto_limit(old, new)
    }

    /// 内容を行ごとのスタイル列に変える。`dark` でテーマを選ぶ。
    pub fn highlight(&self, path: &str, content: &str, dark: bool) -> Vec<HighlightedLine> {
        self.highlight_with(self.syntax_for(path), content, dark)
    }

    /// 描画表示のコード塊。フェンスの言語名で構文を引き、行ごとの HTML にする。言語が
    /// 引けなければ None（無地）。
    pub fn highlight_code(&self, lang: &str, code: &str, dark: bool) -> Option<Vec<String>> {
        let syntax = self.syntaxes.find_syntax_by_token(lang)?;
        let mut content = code.to_string();
        content.push('\n');
        Some(
            self.highlight_with(syntax, &content, dark)
                .iter()
                .map(|ranges| render_line(ranges, &[]))
                .collect(),
        )
    }

    fn highlight_with(
        &self,
        syntax: &SyntaxReference,
        content: &str,
        dark: bool,
    ) -> Vec<HighlightedLine> {
        let theme = &self.themes[if dark {
            EmbeddedThemeName::TwoDark
        } else {
            EmbeddedThemeName::InspiredGithub
        }];
        let comment = comment_color(theme);
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut lines = Vec::new();

        for line in LinesWithEndings::from(content) {
            let mut ranges: HighlightedLine = highlighter
                .highlight_line(line, &self.syntaxes)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, text)| Token {
                    style,
                    text: text.to_string(),
                    comment: style.foreground == comment,
                })
                .collect();
            if let Some(last) = ranges.last_mut() {
                while last.text.ends_with('\n') || last.text.ends_with('\r') {
                    last.text.pop();
                }
                if last.text.is_empty() {
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

/// テーマがコメントに付ける文字色。
fn comment_color(theme: &Theme) -> Color {
    let scope = Scope::new("comment").expect("`comment` is a valid scope name");
    syntect::highlighting::Highlighter::new(theme)
        .style_for_stack(ScopeStack::from_vec(vec![scope]).as_slice())
        .foreground
}

/// 1 行を HTML にする。`segments` の changed 部分は単語単位の強調で包む。
///
/// コメントの断片には色を書かず、`tok-comment` の印だけを付ける。テーマのコメントの色は薄く、追加・削除の
/// 行の背景の上では読めないため、ページがプリセットごとの色を付ける。
pub fn render_line(ranges: &HighlightedLine, segments: &[Segment]) -> String {
    let changed = changed_ranges(segments);
    let mut html = String::new();
    let mut offset = 0;
    // changed は位置の順で重ならないので、断片ごとに全体を見直さずカーソルで進める。
    let mut change_index = 0;

    for token in ranges {
        let text = &token.text;
        let start = offset;
        let end = offset + text.len();
        offset = end;
        if text.is_empty() {
            continue;
        }
        while change_index < changed.len() && changed[change_index].1 <= start {
            change_index += 1;
        }
        let mut overlap_end = change_index;
        while overlap_end < changed.len() && changed[overlap_end].0 < end {
            overlap_end += 1;
        }
        let mut boundaries = vec![start, end];
        for (change_start, change_end) in &changed[change_index..overlap_end] {
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
            while change_index < overlap_end && changed[change_index].1 <= left {
                change_index += 1;
            }
            let is_changed = change_index < overlap_end
                && changed[change_index].0 <= left
                && right <= changed[change_index].1;
            html.push_str(&span(token, piece, is_changed));
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

fn span(token: &Token, text: &str, changed: bool) -> String {
    let inner = if changed {
        format!("<span class=\"word-changed\">{}</span>", escape(text))
    } else {
        escape(text)
    };
    let class = if token.comment {
        " class=\"tok-comment\""
    } else {
        ""
    };
    let css = style_css(token);
    if css.is_empty() {
        format!("<span{class}>{inner}</span>")
    } else {
        format!("<span{class} style=\"{css}\">{inner}</span>")
    }
}

fn style_css(token: &Token) -> String {
    let style = &token.style;
    let mut declarations = Vec::new();
    if !token.comment {
        declarations.push(format!(
            "color:#{:02x}{:02x}{:02x}",
            style.foreground.r, style.foreground.g, style.foreground.b
        ));
    }
    if style.font_style.contains(FontStyle::BOLD) {
        declarations.push("font-weight:bold".to_string());
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        declarations.push("font-style:italic".to_string());
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        declarations.push("text-decoration:underline".to_string());
    }
    declarations.join(";")
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
    use kemi_core::domain::content::{AUTO_MAX_BYTES, AUTO_MAX_LINES};

    #[test]
    fn highlight_multiline_string_keeps_style_on_later_lines() {
        let highlighter = Highlighter::new();
        let code = "let s = \"first line\nsecond line\";\n";
        let lines = highlighter.highlight("a.rs", code, false);

        assert_eq!(lines.len(), 2);
        let string_color = lines[0]
            .last()
            .map(|token| token.style.foreground)
            .expect("first line has ranges");
        assert!(
            lines[1]
                .iter()
                .any(|token| token.style.foreground == string_color
                    && token.text.contains("second line")),
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
            .map(|token| token.style.foreground)
            .expect("first line has ranges");
        assert!(
            lines[1]
                .iter()
                .any(|token| token.style.foreground == comment_color
                    && token.text.contains("still comment")),
            "second line lost the comment color: {:?}",
            lines[1]
        );
    }

    #[test]
    fn highlight_cap_limits_by_line_count() {
        let short = "line\n".repeat(AUTO_MAX_LINES);
        let long = "line\n".repeat(AUTO_MAX_LINES + 1);

        assert!(Highlighter::capable(Some(&short), Some(&short)));
        assert!(!Highlighter::capable(Some(&long), Some(&short)));
        assert!(!Highlighter::capable(Some(&short), Some(&long)));
    }

    #[test]
    fn highlight_cap_limits_by_size() {
        let short = "x".repeat(AUTO_MAX_BYTES);
        let long = "x".repeat(AUTO_MAX_BYTES + 1);

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

    #[test]
    fn render_line_leaves_the_comment_color_to_the_page() {
        let highlighter = Highlighter::new();
        for dark in [false, true] {
            let lines = highlighter.highlight("a.rs", "let value = 1; // note\n", dark);

            let html = render_line(&lines[0], &[]);
            let comment = html
                .split("<span")
                .find(|span| span.contains(" note"))
                .expect("the comment is rendered");
            assert!(comment.contains("class=\"tok-comment\""), "{html}");
            assert!(!comment.contains("color:#"), "{html}");
            assert!(
                html.contains("color:#"),
                "code keeps the theme colors: {html}"
            );
        }
    }

    #[test]
    fn render_line_keeps_the_changed_mark_inside_a_comment() {
        let highlighter = Highlighter::new();
        let lines = highlighter.highlight("a.rs", "// note\n", true);
        let segments = vec![Segment {
            text: "// note".to_string(),
            changed: true,
        }];

        let html = render_line(&lines[0], &segments);
        assert!(
            html.contains("class=\"tok-comment\"") && html.contains("word-changed"),
            "{html}"
        );
    }
}
