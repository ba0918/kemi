use super::*;

fn no_highlight() -> Option<Highlight<'static>> {
    None
}

fn plain_url(reference: &ImageRef) -> String {
    match reference {
        ImageRef::Review { file_id, side } => format!("review:{file_id}:{}", side.as_str()),
        ImageRef::Repo { path, side } => format!("repo:{}:{path}", side.as_str()),
    }
}

fn input<'a>(old: Option<&'a str>, new: Option<&'a str>) -> MarkdownInput<'a> {
    MarkdownInput {
        old,
        new,
        old_path: old.map(|_| "docs/a.md"),
        new_path: new.map(|_| "docs/a.md"),
        review_paths: ReviewPaths::default(),
        repo_readable: true,
    }
}

fn render_new(text: &str) -> Rendered {
    render_markdown(&input(None, Some(text)), no_highlight(), &plain_url).unwrap()
}

fn ranges(rendered: &Rendered) -> Vec<(u32, u32)> {
    rendered
        .blocks
        .iter()
        .map(|block| (block.start, block.end))
        .collect()
}

fn attributes(rendered: &Rendered) -> Vec<String> {
    rendered
        .html
        .match_indices("data-kemi-block=\"")
        .map(|(at, prefix)| {
            let rest = &rendered.html[at + prefix.len()..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

#[test]
fn paragraph_heading_and_code_block_get_their_source_line_ranges() {
    let rendered = render_new("# Title\n\nfirst line\nsecond line\n\n```rs\nlet a = 1;\n```\n");

    assert_eq!(ranges(&rendered), vec![(1, 1), (3, 4), (6, 8)]);
    assert_eq!(attributes(&rendered), vec!["new:1-1", "new:3-4", "new:6-8"]);
}

#[test]
fn gfm_table_strikethrough_task_list_autolink_and_footnote_are_rendered() {
    let rendered = render_new(
        "| a | b |\n|---|---|\n| 1 | 2 |\n\n~~gone~~ see https://example.com now[^1]\n\n- [x] done\n\n[^1]: the note\n",
    );

    let html = &rendered.html;
    assert!(html.contains("<table"), "{html}");
    assert!(html.contains("<th"), "{html}");
    assert!(html.contains("<del>gone</del>"), "{html}");
    assert!(html.contains("<a href=\"https://example.com\""), "{html}");
    assert!(html.contains("type=\"checkbox\""), "{html}");
    assert!(html.contains("footnote"), "{html}");
    assert!(html.contains("the note"), "{html}");
}

#[test]
fn nested_list_and_quote_are_containers_and_their_paragraphs_are_blocks() {
    let rendered =
        render_new("- one\n  - two\n  - three\n- four\n\n> quoted\n> still\n>\n> second\n");

    assert_eq!(
        ranges(&rendered),
        vec![(1, 1), (2, 2), (3, 3), (4, 4), (6, 7), (9, 9)]
    );
    assert!(rendered.html.contains("<ul"), "{}", rendered.html);
    assert!(rendered.html.contains("<blockquote"), "{}", rendered.html);
    assert!(
        !rendered.html.contains("<ul data-kemi-block"),
        "{}",
        rendered.html
    );
    assert!(
        !rendered.html.contains("<blockquote data-kemi-block"),
        "{}",
        rendered.html
    );
}

#[test]
fn table_rows_are_blocks_and_the_table_is_a_container() {
    let rendered = render_new("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");

    assert_eq!(ranges(&rendered), vec![(1, 1), (3, 3), (4, 4)]);
    assert!(
        !rendered.html.contains("<table data-kemi-block"),
        "{}",
        rendered.html
    );
}

#[test]
fn crlf_and_multibyte_text_keep_the_source_line_numbers() {
    let rendered = render_new("日本語の段落\r\n続き\r\n\r\n# 見出し\r\n\r\n最後\r\n");

    assert_eq!(ranges(&rendered), vec![(1, 2), (4, 4), (6, 6)]);
}

#[test]
fn front_matter_becomes_a_plain_chunk_covering_its_lines() {
    let rendered = render_new("---\ntitle: x\ntags: [a]\n---\n\n# Head\n\nbody\n");

    assert_eq!(ranges(&rendered), vec![(1, 4), (6, 6), (8, 8)]);
    assert!(rendered.html.starts_with("<pre"), "{}", rendered.html);
    assert!(rendered.html.contains("title: x"), "{}", rendered.html);
}

#[test]
fn front_matter_without_a_closing_fence_is_ordinary_markdown() {
    let rendered = render_new("---\ntitle: x\n\nbody\n");

    assert!(!rendered.html.starts_with("<pre"), "{}", rendered.html);
    assert!(rendered.html.contains("<hr"), "{}", rendered.html);
}

#[test]
fn raw_html_is_escaped_and_never_becomes_elements_or_attributes() {
    let rendered = render_new(
        "<script>alert(1)</script>\n\ntext <img src=x onerror=alert(1)> more\n\n<iframe src=\"https://x\"></iframe>\n",
    );

    let html = &rendered.html;
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("<iframe"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
    assert!(
        html.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "{html}"
    );
}

#[test]
fn links_with_disallowed_schemes_are_neutralized_even_with_surrounding_whitespace() {
    let rendered = render_new(
        "[a](<javascript:alert(1)>) [b](< data:text/html,x >) [c](<//evil.example/x>) [d](JAVASCRIPT:x)\n",
    );

    let html = &rendered.html;
    assert!(!html.to_ascii_lowercase().contains("javascript:"), "{html}");
    assert!(!html.contains("data:"), "{html}");
    assert!(!html.contains("//evil"), "{html}");
    assert!(!html.contains("<a "), "{html}");
}

#[test]
fn anchor_links_are_kept() {
    let rendered = render_new("[top](#top)\n");

    assert!(
        rendered.html.contains("<a href=\"#top\""),
        "{}",
        rendered.html
    );
}

#[test]
fn external_links_open_in_a_new_tab_with_noopener() {
    let rendered = render_new("[x]( HTTPS://example.com/a )\n");

    assert!(
        rendered.html.contains(
            "<a href=\"HTTPS://example.com/a\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>"
        ),
        "{}",
        rendered.html
    );
}

#[test]
fn relative_links_become_text_with_the_path_in_the_title() {
    let rendered = render_new("see [the other](../a.md) page\n");

    let html = &rendered.html;
    assert!(!html.contains("<a "), "{html}");
    assert!(html.contains("title=\"../a.md\""), "{html}");
    assert!(html.contains("the other"), "{html}");
}

#[test]
fn external_image_sources_are_passed_through() {
    let rendered = render_new("![logo](https://example.com/logo.png)\n");

    assert!(
        rendered
            .html
            .contains("<img src=\"https://example.com/logo.png\" alt=\"logo\""),
        "{}",
        rendered.html
    );
}
