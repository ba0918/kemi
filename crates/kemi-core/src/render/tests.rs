use super::*;
use crate::domain::review::{FileEntry, Status};

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

fn review_paths() -> ReviewPaths {
    ReviewPaths {
        old: vec![("f7".to_string(), "docs/img/old.png".to_string())],
        new: vec![("f9".to_string(), "docs/img/new.png".to_string())],
    }
}

#[test]
fn images_outside_the_repo_or_without_an_image_extension_or_unreadable_get_a_frame() {
    let mut input = input(
        None,
        Some("![a](../../a.png) ![b](notes.txt) ![c](img/c.png)\n"),
    );
    input.repo_readable = false;
    let rendered = render_markdown(&input, no_highlight(), &plain_url).unwrap();

    let html = &rendered.html;
    assert!(!html.contains("<img"), "{html}");
    assert_eq!(html.matches("kb-img-frame").count(), 3, "{html}");
    assert!(html.contains("../../a.png"), "{html}");
    assert!(rendered.blocks[0].images.is_empty());
}

#[test]
fn relative_image_matching_a_review_file_refers_to_that_file_and_side() {
    let mut input = input(None, Some("![n](img/new.png)\n"));
    input.review_paths = review_paths();
    let rendered = render_markdown(&input, no_highlight(), &plain_url).unwrap();

    assert_eq!(
        rendered.blocks[0].images,
        vec![ImageRef::Review {
            file_id: "f9".to_string(),
            side: Side::New,
        }]
    );
    assert!(
        rendered
            .html
            .contains("<img src=\"review:f9:new\" alt=\"n\" data-kemi-path=\"img/new.png\">"),
        "{}",
        rendered.html
    );
}

#[test]
fn relative_image_not_in_the_review_refers_to_the_normalized_repo_path() {
    let mut input = input(None, Some("![o](/assets/../logo.PNG)\n"));
    input.review_paths = review_paths();
    let rendered = render_markdown(&input, no_highlight(), &plain_url).unwrap();

    assert_eq!(
        rendered.blocks[0].images,
        vec![ImageRef::Repo {
            path: "logo.PNG".to_string(),
            side: Side::New,
        }]
    );
    assert!(
        rendered.html.contains("src=\"repo:new:logo.PNG\""),
        "{}",
        rendered.html
    );
}

#[test]
fn code_blocks_are_plain_without_a_highlighter_and_use_it_when_given() {
    let text = "```rust\nlet a = <1>;\nlet b = 2;\n```\n";
    let plain = render_new(text);
    assert!(
        plain
            .html
            .contains("<code class=\"language-rust\">let a = &lt;1&gt;;\nlet b = 2;</code>"),
        "{}",
        plain.html
    );

    let highlight = |lang: &str, code: &str| -> Option<Vec<String>> {
        assert_eq!(lang, "rust");
        Some(
            code.lines()
                .map(|line| format!("<i>{}</i>", line.len()))
                .collect(),
        )
    };
    let highlighted =
        render_markdown(&input(None, Some(text)), Some(&highlight), &plain_url).unwrap();
    assert!(
        highlighted
            .html
            .contains("<code class=\"language-rust\"><i>12</i>\n<i>10</i></code>"),
        "{}",
        highlighted.html
    );
}

fn render_pair(old: &str, new: &str) -> Rendered {
    render_markdown(&input(Some(old), Some(new)), no_highlight(), &plain_url).unwrap()
}

fn marks(rendered: &Rendered) -> Vec<(Side, u32, u32, Mark)> {
    rendered
        .blocks
        .iter()
        .map(|block| (block.side, block.start, block.end, block.mark))
        .collect()
}

#[test]
fn one_word_change_in_a_paragraph_with_the_same_structure_is_one_block_with_both_marks() {
    let rendered = render_pair(
        "intro\n\nthe *quick* brown fox\n\nend\n",
        "intro\n\nthe *quick* red fox\n\nend\n",
    );

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::New, 1, 1, Mark::Unchanged),
            (Side::New, 3, 3, Mark::Modified),
            (Side::New, 5, 5, Mark::Unchanged),
        ]
    );
    let html = &rendered.html;
    assert!(
        html.contains("<span class=\"kw-del\">brown</span>"),
        "{html}"
    );
    assert!(html.contains("<span class=\"kw-add\">red</span>"), "{html}");
    assert!(html.contains("<em>quick</em>"), "{html}");
}

#[test]
fn a_paragraph_whose_link_became_plain_text_is_shown_as_two_blocks() {
    let rendered = render_pair(
        "see [the docs](https://example.com) now\n",
        "see the docs now\n",
    );

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::Old, 1, 1, Mark::Deleted),
            (Side::New, 1, 1, Mark::Added),
        ]
    );
    assert!(
        rendered.html.contains("data-kemi-block=\"old:1-1\""),
        "{}",
        rendered.html
    );
    assert!(!rendered.html.contains("kw-del"), "{}", rendered.html);
}

#[test]
fn a_deleted_paragraph_is_inserted_at_its_position_with_the_old_line_range() {
    let rendered = render_pair(
        "# Head\n\nfirst\n\nsecond\nmore\n\nthird\n",
        "# Head\n\nfirst\n\nthird\n",
    );

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::New, 1, 1, Mark::Unchanged),
            (Side::New, 3, 3, Mark::Unchanged),
            (Side::Old, 5, 6, Mark::Deleted),
            (Side::New, 5, 5, Mark::Unchanged),
        ]
    );
    let html = &rendered.html;
    let deleted = html.find("old:5-6").unwrap();
    let third = html.find("new:5-5").unwrap();
    assert!(deleted < third, "{html}");
    assert!(html.contains("kb-del"), "{html}");
}

#[test]
fn a_deleted_file_renders_the_old_document_with_every_block_deleted() {
    let rendered =
        render_markdown(&input(Some("# A\n\nb\n"), None), no_highlight(), &plain_url).unwrap();

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::Old, 1, 1, Mark::Deleted),
            (Side::Old, 3, 3, Mark::Deleted),
        ]
    );
}

#[test]
fn an_added_file_renders_every_block_added() {
    let rendered = render_new("# A\n\nb\n");

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::New, 1, 1, Mark::Added),
            (Side::New, 3, 3, Mark::Added),
        ]
    );
}

#[test]
fn a_code_block_change_is_shown_as_old_and_new_blocks_stacked() {
    let rendered = render_pair("```\na\n```\n", "```\nb\n```\n");

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::Old, 1, 3, Mark::Deleted),
            (Side::New, 1, 3, Mark::Added),
        ]
    );
}

fn entry(status: Status, path: &str, old_path: Option<&str>) -> FileEntry {
    FileEntry {
        id: "f1".to_string(),
        group_id: "g".to_string(),
        path: path.to_string(),
        old_path: old_path.map(str::to_string),
        status,
        add: 0,
        del: 0,
        binary: false,
        old_size: 0,
        new_size: 0,
        focus: false,
        note: String::new(),
        noise: false,
    }
}

#[test]
fn target_is_decided_by_the_new_path_for_renames_and_the_old_path_for_deletions() {
    assert_eq!(
        target_of_file(&entry(Status::Rename, "docs/b.md", Some("docs/a.txt"))),
        Some(Target::Markdown)
    );
    assert_eq!(
        target_of_file(&entry(Status::Rename, "docs/b.txt", Some("docs/a.md"))),
        None
    );
    assert_eq!(
        target_of_file(&entry(Status::Delete, "notes/gone.MD", None)),
        Some(Target::Markdown)
    );
    assert_eq!(target_of("a.markdown"), Some(Target::Markdown));
    assert_eq!(
        target_of("data.CSV"),
        Some(Target::Table { delimiter: b',' })
    );
    assert_eq!(
        target_of("data.tsv"),
        Some(Target::Table { delimiter: b'\t' })
    );
    assert_eq!(target_of("logo.svg"), Some(Target::Image { svg: true }));
    assert_eq!(target_of("photo.JPEG"), Some(Target::Image { svg: false }));
    assert_eq!(target_of("README"), None);
    assert_eq!(target_of("archive.tar.gz"), None);
}

#[test]
fn a_side_over_the_line_or_byte_limit_is_not_rendered() {
    let long = "x\n".repeat(10_001);
    assert_eq!(
        render_markdown(&input(None, Some(&long)), no_highlight(), &plain_url).unwrap_err(),
        Unrenderable::TooLarge
    );
    let big = "y".repeat(1_048_577);
    assert_eq!(
        render_markdown(&input(Some(&big), Some("ok\n")), no_highlight(), &plain_url).unwrap_err(),
        Unrenderable::TooLarge
    );
    let limit = "x\n".repeat(10_000);
    assert!(render_markdown(&input(None, Some(&limit)), no_highlight(), &plain_url).is_ok());
}

// ---- 表（CSV / TSV） ----

fn render_table_pair(
    old: Option<&str>,
    new: Option<&str>,
    delimiter: u8,
) -> Result<Rendered, Unrenderable> {
    render_table(&TableInput {
        old,
        new,
        delimiter,
    })
}

#[test]
fn quoted_fields_keep_the_delimiter_and_doubled_quotes_become_one() {
    assert_eq!(
        split_fields("x,\"a,b\",\"say \"\"hi\"\"\",", b','),
        Ok(vec![
            "x".to_string(),
            "a,b".to_string(),
            "say \"hi\"".to_string(),
            String::new()
        ])
    );
}

#[test]
fn tsv_splits_on_tabs_and_leaves_commas_alone() {
    assert_eq!(
        split_fields("a,b\tc", b'\t'),
        Ok(vec!["a,b".to_string(), "c".to_string()])
    );
}

#[test]
fn a_line_with_an_unbalanced_quote_makes_the_table_unrenderable() {
    assert_eq!(split_fields("a,\"b", b','), Err(UnbalancedQuote));
    assert_eq!(
        render_table_pair(None, Some("h1,h2\na,\"b\n"), b',').unwrap_err(),
        Unrenderable::UnbalancedQuote { line: 2 }
    );
    assert_eq!(unbalanced_line("h\n\"ok\"\n\"no\n", b','), Some(3));
    assert_eq!(unbalanced_line("h\n\"ok\"\n", b','), None);
}

#[test]
fn a_rewritten_row_is_shown_as_the_old_row_deleted_and_the_new_row_added() {
    let rendered =
        render_table_pair(Some("h1,h2\n1,2\n3,4\n"), Some("h1,h2\n1,2\n3,5\n"), b',').unwrap();

    assert_eq!(
        marks(&rendered),
        vec![
            (Side::New, 1, 1, Mark::Unchanged),
            (Side::New, 2, 2, Mark::Unchanged),
            (Side::Old, 3, 3, Mark::Deleted),
            (Side::New, 3, 3, Mark::Added),
        ]
    );
    let html = &rendered.html;
    assert!(
        html.find("old:3-3").unwrap() < html.find("new:3-3").unwrap(),
        "{html}"
    );
    assert!(html.contains("<td>5</td>"), "{html}");
}

#[test]
fn the_first_row_is_the_header_and_uneven_rows_are_kept() {
    let rendered = render_table_pair(None, Some("h1,h2\n1\n2,3,4\n"), b',').unwrap();

    let html = &rendered.html;
    assert!(html.contains("<thead>\n<tr class=\"kb kb-add\" data-kemi-block=\"new:1-1\">\n<th>h1</th>\n<th>h2</th>"), "{html}");
    assert_eq!(html.matches("<td>").count(), 4, "{html}");
    assert_eq!(rendered.blocks.len(), 3);
}

// ---- 画像 ----

#[test]
fn image_files_are_binary_images_or_svg_and_text_images_are_not() {
    let binary_png = FileEntry {
        binary: true,
        ..entry(Status::Modify, "a/logo.PNG", None)
    };
    assert_eq!(image_kind(&binary_png), Some(ImageKind::Raster));
    let text_png = entry(Status::Modify, "a/pointer.png", None);
    assert_eq!(image_kind(&text_png), None);
    let svg = entry(Status::Modify, "a/icon.svg", None);
    assert_eq!(image_kind(&svg), Some(ImageKind::Svg));
    assert_eq!(
        image_kind(&entry(Status::Modify, "a/readme.md", None)),
        None
    );
}

#[test]
fn image_content_types_come_from_the_extension() {
    assert_eq!(image_content_type("x.png"), Some("image/png"));
    assert_eq!(image_content_type("x.JPG"), Some("image/jpeg"));
    assert_eq!(image_content_type("x.jpeg"), Some("image/jpeg"));
    assert_eq!(image_content_type("x.gif"), Some("image/gif"));
    assert_eq!(image_content_type("x.webp"), Some("image/webp"));
    assert_eq!(image_content_type("x.svg"), Some("image/svg+xml"));
    assert_eq!(image_content_type("x.txt"), None);
    assert_eq!(IMAGE_MAX_BYTES, 5 * 1_048_576);
}
