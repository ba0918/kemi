//! Markdown の構文木からブロック列と HTML を作る。
//!
//! 構文木は 2 回歩く。1 回目でブロック（`CONTEXT.md`）の並びと行レンジを集め、旧新の
//! 整列から各ブロックの印を決める。2 回目で、その印と、差し込む旧のブロックを添えて
//! HTML を書く。どちらも同じ順で歩くので、ブロックの列は添字で対応づく。

use ox_content_allocator::Allocator;
use ox_content_ast::{AlignKind, Document, ListItem, Node, TableRow};
use ox_content_parser::{Parser, ParserOptions};

use crate::domain::content;

use super::inline::{self, ImageContext, Inline, Sink, WordMarks};
use super::lines::{split_front_matter, LineIndex};
use super::{
    Block, Highlight, ImageRef, ImageUrl, Mark, MarkdownInput, Rendered, Side, Unrenderable,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Paragraph,
    Heading(u8),
    Code,
    Html,
    Rule,
    FootnoteDefinition,
    TableRow {
        header: bool,
    },
    /// 中に段落を持たないリスト項目。
    ListItem,
    FrontMatter,
}

/// ブロックの元になるノード。参照の寿命（`'r`）と構文木の寿命（`'a`）は別で、
/// 構文木は寿命に対して不変なので、短い借用からも作れるようにしておく。
pub(super) enum Source<'r, 'a> {
    Node(&'r Node<'a>),
    Row(&'r TableRow<'a>, &'r [AlignKind]),
    Item(&'r ListItem<'a>),
    FrontMatter(&'r str),
}

pub(super) struct BlockInfo<'r, 'a> {
    pub kind: Kind,
    pub source: Source<'r, 'a>,
    pub start: u32,
    pub end: u32,
    /// 文のブロックのインライン。構造の比較と語の印に使う。
    pub inline: Option<Inline>,
}

/// 片側の文書。
pub(super) struct SideDoc<'a> {
    pub side: Side,
    pub document: &'a Document<'a>,
    pub front_matter: bool,
    pub blocks: Vec<BlockInfo<'a, 'a>>,
    image_context: ImageContext<'a>,
}

/// 新側の 1 ブロックをどう書くか。
pub(super) struct Decision {
    pub mark: Mark,
    pub words: Option<WordMarks>,
    /// このブロックの前に差し込む、旧側のブロックの添字。
    pub before: Vec<usize>,
}

pub(super) struct Plan {
    pub decisions: Vec<Decision>,
    /// 文書の末尾に差し込む旧側のブロック。
    pub trailing: Vec<usize>,
}

impl Plan {
    pub(super) fn uniform(count: usize, mark: Mark) -> Self {
        Plan {
            decisions: (0..count)
                .map(|_| Decision {
                    mark,
                    words: None,
                    before: Vec::new(),
                })
                .collect(),
            trailing: Vec::new(),
        }
    }
}

struct Prepared {
    front_lines: u32,
    front: Option<String>,
    body: String,
}

fn prepare(text: &str) -> Prepared {
    let normalized = content::normalize(text);
    let (front_lines, front, body) = split_front_matter(&normalized);
    Prepared {
        front_lines,
        front: front.map(str::to_string),
        body: body.to_string(),
    }
}

fn parse<'a>(allocator: &'a Allocator, body: &'a str) -> Result<Document<'a>, Unrenderable> {
    Parser::with_options(allocator, body, ParserOptions::gfm())
        .parse()
        .map_err(|error| Unrenderable::Failed(error.to_string()))
}

pub(super) fn render(
    input: &MarkdownInput<'_>,
    highlight: Option<Highlight<'_>>,
    image_url: ImageUrl<'_>,
) -> Result<Rendered, Unrenderable> {
    if !content::within_auto_limit(input.old, input.new) {
        return Err(Unrenderable::TooLarge);
    }
    let old = input.old.map(prepare);
    let new = input.new.map(prepare);
    let old_allocator = Allocator::new();
    let new_allocator = Allocator::new();
    let old_document = old
        .as_ref()
        .map(|prepared| parse(&old_allocator, &prepared.body))
        .transpose()?;
    let new_document = new
        .as_ref()
        .map(|prepared| parse(&new_allocator, &prepared.body))
        .transpose()?;
    let old_side = old
        .as_ref()
        .zip(old_document.as_ref())
        .map(|(prepared, document)| {
            collect_side(
                Side::Old,
                prepared,
                document,
                ImageContext {
                    side: Side::Old,
                    markdown_path: input.old_path,
                    review_paths: &input.review_paths.old,
                    repo_readable: input.repo_readable,
                },
            )
        });
    let new_side = new
        .as_ref()
        .zip(new_document.as_ref())
        .map(|(prepared, document)| {
            collect_side(
                Side::New,
                prepared,
                document,
                ImageContext {
                    side: Side::New,
                    markdown_path: input.new_path,
                    review_paths: &input.review_paths.new,
                    repo_readable: input.repo_readable,
                },
            )
        });

    let mut writer = Writer {
        out: String::new(),
        blocks: Vec::new(),
        highlight,
        image_url,
        plain: 0,
        in_table: false,
    };
    match (&old_side, &new_side) {
        (Some(old), None) => {
            let plan = Plan::uniform(old.blocks.len(), Mark::Deleted);
            writer.write_document(old, &plan, None);
        }
        (old, Some(new)) => {
            let plan = match old {
                Some(_) => Plan::uniform(new.blocks.len(), Mark::Unchanged),
                None => Plan::uniform(new.blocks.len(), Mark::Added),
            };
            writer.write_document(new, &plan, old.as_ref());
        }
        (None, None) => {}
    }
    Ok(Rendered {
        html: writer.out,
        blocks: writer.blocks,
    })
}

fn collect_side<'a>(
    side: Side,
    prepared: &'a Prepared,
    document: &'a Document<'a>,
    image_context: ImageContext<'a>,
) -> SideDoc<'a> {
    let mut collector = Collector {
        text: &prepared.body,
        index: LineIndex::new(&prepared.body),
        offset: prepared.front_lines,
        image_context: &image_context,
        blocks: Vec::new(),
    };
    if let Some(front) = &prepared.front {
        collector.blocks.push(BlockInfo {
            kind: Kind::FrontMatter,
            source: Source::FrontMatter(front),
            start: 1,
            end: prepared.front_lines,
            inline: None,
        });
    }
    collector.collect(&document.children);
    SideDoc {
        side,
        document,
        front_matter: prepared.front.is_some(),
        blocks: collector.blocks,
        image_context,
    }
}

struct Collector<'c, 'a> {
    text: &'a str,
    index: LineIndex,
    offset: u32,
    image_context: &'c ImageContext<'a>,
    blocks: Vec<BlockInfo<'a, 'a>>,
}

impl<'c, 'a> Collector<'c, 'a> {
    fn range(&self, span: ox_content_ast::Span) -> (u32, u32) {
        let (start, end) = self
            .index
            .range_of(self.text, span.start as usize, span.end as usize);
        (start + self.offset, end + self.offset)
    }

    fn push(
        &mut self,
        kind: Kind,
        source: Source<'a, 'a>,
        span: ox_content_ast::Span,
        inline: Option<Inline>,
    ) {
        let (start, end) = self.range(span);
        self.blocks.push(BlockInfo {
            kind,
            source,
            start,
            end,
            inline,
        });
    }

    fn collect(&mut self, nodes: &'a [Node<'a>]) {
        for node in nodes {
            match node {
                Node::Paragraph(paragraph) => {
                    let inline = inline::flatten(&paragraph.children, self.image_context);
                    self.push(
                        Kind::Paragraph,
                        Source::Node(node),
                        paragraph.span,
                        Some(inline),
                    );
                }
                Node::Heading(heading) => {
                    let inline = inline::flatten(&heading.children, self.image_context);
                    self.push(
                        Kind::Heading(heading.depth),
                        Source::Node(node),
                        heading.span,
                        Some(inline),
                    );
                }
                Node::CodeBlock(code) => self.push(Kind::Code, Source::Node(node), code.span, None),
                Node::Html(html) => self.push(Kind::Html, Source::Node(node), html.span, None),
                Node::ThematicBreak(rule) => {
                    self.push(Kind::Rule, Source::Node(node), rule.span, None)
                }
                Node::FootnoteDefinition(definition) => self.push(
                    Kind::FootnoteDefinition,
                    Source::Node(node),
                    definition.span,
                    None,
                ),
                Node::BlockQuote(quote) => self.collect(&quote.children),
                Node::List(list) => {
                    for item in &list.children {
                        if has_paragraph(item) {
                            self.collect(&item.children);
                        } else {
                            self.push(Kind::ListItem, Source::Item(item), item.span, None);
                        }
                    }
                }
                Node::Table(table) => {
                    for (index, row) in table.children.iter().enumerate() {
                        self.push(
                            Kind::TableRow { header: index == 0 },
                            Source::Row(row, &table.align),
                            row.span,
                            None,
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn has_paragraph(item: &ListItem<'_>) -> bool {
    item.children
        .iter()
        .any(|child| matches!(child, Node::Paragraph(_)))
}

struct Writer<'w> {
    out: String,
    blocks: Vec<Block>,
    highlight: Option<Highlight<'w>>,
    image_url: ImageUrl<'w>,
    /// 0 より大きいあいだは、ブロックの属性を出さず、ブロックの列にも数えない
    /// （脚注の定義や、段落を持たないリスト項目の中身）。
    plain: usize,
    in_table: bool,
}

/// 文書を歩きながらブロックを書く。ブロックの列は `Collector` と同じ順で消費する。
struct Walk<'s, 'a, 'b> {
    doc: &'s SideDoc<'a>,
    plan: &'s Plan,
    old: Option<&'s SideDoc<'b>>,
    cursor: usize,
}

impl<'w> Writer<'w> {
    fn write_document(&mut self, doc: &SideDoc<'_>, plan: &Plan, old: Option<&SideDoc<'_>>) {
        let mut walk = Walk {
            doc,
            plan,
            old,
            cursor: 0,
        };
        if doc.front_matter {
            self.next_block(&mut walk);
        }
        self.write_nodes(&doc.document.children, &mut walk);
        if let Some(old) = old {
            self.write_old_blocks(old, &plan.trailing);
        }
    }

    fn write_nodes(&mut self, nodes: &[Node<'_>], walk: &mut Walk<'_, '_, '_>) {
        for node in nodes {
            match node {
                Node::Paragraph(_)
                | Node::Heading(_)
                | Node::CodeBlock(_)
                | Node::Html(_)
                | Node::ThematicBreak(_)
                | Node::FootnoteDefinition(_) => {
                    if self.plain > 0 {
                        self.write_plain_block(node, walk);
                    } else {
                        self.next_block(walk);
                    }
                }
                Node::BlockQuote(quote) => {
                    self.out.push_str("<blockquote>\n");
                    self.write_nodes(&quote.children, walk);
                    self.out.push_str("</blockquote>\n");
                }
                Node::List(list) => self.write_list(list, walk),
                Node::Table(table) => {
                    self.out.push_str("<table class=\"kb-table\">\n");
                    let was_in_table = std::mem::replace(&mut self.in_table, true);
                    for (index, row) in table.children.iter().enumerate() {
                        if index == 0 {
                            self.out.push_str("<thead>\n");
                        } else if index == 1 {
                            self.out.push_str("<tbody>\n");
                        }
                        if self.plain > 0 {
                            self.write_row(row, &table.align, index == 0, walk.doc, "");
                        } else {
                            self.next_block(walk);
                        }
                        if index == 0 {
                            self.out.push_str("</thead>\n");
                        }
                    }
                    if table.children.len() > 1 {
                        self.out.push_str("</tbody>\n");
                    }
                    self.in_table = was_in_table;
                    self.out.push_str("</table>\n");
                }
                _ => {}
            }
        }
    }

    fn write_list(&mut self, list: &ox_content_ast::List<'_>, walk: &mut Walk<'_, '_, '_>) {
        let tightness = if list.spread { "loose" } else { "tight" };
        if list.ordered {
            self.out.push_str("<ol class=\"");
            self.out.push_str(tightness);
            if let Some(start) = list.start.filter(|start| *start != 1) {
                self.out.push_str("\" start=\"");
                self.out.push_str(&start.to_string());
            }
            self.out.push_str("\">\n");
        } else {
            self.out.push_str("<ul class=\"");
            self.out.push_str(tightness);
            self.out.push_str("\">\n");
        }
        for item in &list.children {
            if has_paragraph(item) {
                self.out.push_str(if item.checked.is_some() {
                    "<li class=\"task\">"
                } else {
                    "<li>"
                });
                write_checkbox(&mut self.out, item);
                self.write_nodes(&item.children, walk);
                self.out.push_str("</li>\n");
            } else if self.plain > 0 {
                self.write_item(item, "", walk.doc);
            } else {
                self.next_block(walk);
            }
        }
        self.out
            .push_str(if list.ordered { "</ol>\n" } else { "</ul>\n" });
    }

    /// 次のブロックを、その印と、前に差し込む旧のブロックを添えて書く。
    fn next_block(&mut self, walk: &mut Walk<'_, '_, '_>) {
        let index = walk.cursor;
        walk.cursor += 1;
        let info = &walk.doc.blocks[index];
        let decision = &walk.plan.decisions[index];
        if let Some(old) = walk.old {
            self.write_old_blocks(old, &decision.before);
        }
        self.write_block(walk.doc, info, decision.mark, decision.words.as_ref());
    }

    /// 消した旧のブロックを差し込む。表の外に置く表の行は、続く分をまとめて表にする。
    fn write_old_blocks(&mut self, old: &SideDoc<'_>, indexes: &[usize]) {
        let mut open_table = false;
        for index in indexes {
            let info = &old.blocks[*index];
            let is_row = matches!(info.kind, Kind::TableRow { .. });
            if is_row && !self.in_table && !open_table {
                self.out
                    .push_str("<table class=\"kb-table kb-del-table\">\n");
                open_table = true;
            } else if !is_row && open_table {
                self.out.push_str("</table>\n");
                open_table = false;
            }
            self.write_block(old, info, Mark::Deleted, None);
        }
        if open_table {
            self.out.push_str("</table>\n");
        }
    }

    /// ブロックの属性。属性を出さない深さでは空で、ブロックの列にも数えない。
    fn attributes(
        &mut self,
        doc: &SideDoc<'_>,
        info: &BlockInfo<'_, '_>,
        mark: Mark,
        class: &str,
    ) -> String {
        if self.plain > 0 {
            return if class.is_empty() {
                String::new()
            } else {
                format!(" class=\"{class}\"")
            };
        }
        self.blocks.push(Block {
            side: doc.side,
            start: info.start,
            end: info.end,
            mark,
            images: Vec::new(),
        });
        let mark_class = match mark {
            Mark::Unchanged => "",
            Mark::Added => " kb-add",
            Mark::Deleted => " kb-del",
            Mark::Modified => " kb-mod",
        };
        format!(
            " class=\"kb{mark_class}{}{class}\" data-kemi-block=\"{}:{}-{}\"",
            if class.is_empty() { "" } else { " " },
            doc.side.as_str(),
            info.start,
            info.end
        )
    }

    fn write_block(
        &mut self,
        doc: &SideDoc<'_>,
        info: &BlockInfo<'_, '_>,
        mark: Mark,
        words: Option<&WordMarks>,
    ) {
        let block_index = self.blocks.len();
        let mut images = Vec::new();
        match &info.source {
            Source::Node(node) => match node {
                Node::Paragraph(_) | Node::Heading(_) => {
                    let (open, close) = match info.kind {
                        Kind::Heading(depth) => (format!("<h{depth}"), format!("</h{depth}>\n")),
                        _ => ("<p".to_string(), "</p>\n".to_string()),
                    };
                    let attributes = self.attributes(doc, info, mark, "");
                    self.out.push_str(&open);
                    self.out.push_str(&attributes);
                    self.out.push('>');
                    if let Some(inline) = &info.inline {
                        self.write_inline(inline, words, &mut images);
                    }
                    self.out.push_str(&close);
                }
                Node::CodeBlock(code) => {
                    let attributes = self.attributes(doc, info, mark, "");
                    self.out.push_str("<pre");
                    self.out.push_str(&attributes);
                    self.out.push('>');
                    self.write_code(code.lang, code.value);
                    self.out.push_str("</pre>\n");
                }
                Node::Html(html) => {
                    let attributes = self.attributes(doc, info, mark, "kb-html");
                    self.out.push_str("<pre");
                    self.out.push_str(&attributes);
                    self.out.push('>');
                    inline::push_escaped(&mut self.out, html.value.trim_end_matches('\n'));
                    self.out.push_str("</pre>\n");
                }
                Node::ThematicBreak(_) => {
                    let attributes = self.attributes(doc, info, mark, "");
                    self.out.push_str("<hr");
                    self.out.push_str(&attributes);
                    self.out.push_str(">\n");
                }
                Node::FootnoteDefinition(definition) => {
                    let attributes = self.attributes(doc, info, mark, "kb-footnote");
                    self.out.push_str("<div");
                    self.out.push_str(&attributes);
                    self.out.push_str("><sup class=\"kb-fnlabel\">[");
                    inline::push_escaped(
                        &mut self.out,
                        definition.label.unwrap_or(definition.identifier),
                    );
                    self.out.push_str("]</sup>\n");
                    self.plain += 1;
                    let plan = Plan::uniform(0, mark);
                    let mut inner = Walk {
                        doc,
                        plan: &plan,
                        old: None,
                        cursor: 0,
                    };
                    self.write_nodes(&definition.children, &mut inner);
                    self.plain -= 1;
                    self.out.push_str("</div>\n");
                }
                _ => {}
            },
            Source::Row(row, align) => {
                let header = matches!(info.kind, Kind::TableRow { header: true });
                let attributes = self.attributes(doc, info, mark, "");
                self.write_row(row, align, header, doc, &attributes);
            }
            Source::Item(item) => {
                let attributes = self.attributes(doc, info, mark, "");
                self.write_item(item, &attributes, doc);
            }
            Source::FrontMatter(text) => {
                let attributes = self.attributes(doc, info, mark, "kb-front");
                self.out.push_str("<pre");
                self.out.push_str(&attributes);
                self.out.push('>');
                inline::push_escaped(&mut self.out, text.trim_end_matches('\n'));
                self.out.push_str("</pre>\n");
            }
        }
        if let Some(block) = self.blocks.get_mut(block_index) {
            block.images = images;
        }
    }

    /// 属性を持たない（ブロックに数えない）ブロックの書き出し。脚注の定義や、段落を
    /// 持たないリスト項目の中身がこれになる。
    fn write_plain_block(&mut self, node: &Node<'_>, walk: &mut Walk<'_, '_, '_>) {
        let (kind, inline) = match node {
            Node::Paragraph(paragraph) => (
                Kind::Paragraph,
                Some(inline::flatten(
                    &paragraph.children,
                    &walk.doc.image_context,
                )),
            ),
            Node::Heading(heading) => (
                Kind::Heading(heading.depth),
                Some(inline::flatten(&heading.children, &walk.doc.image_context)),
            ),
            Node::CodeBlock(_) => (Kind::Code, None),
            Node::Html(_) => (Kind::Html, None),
            Node::ThematicBreak(_) => (Kind::Rule, None),
            Node::FootnoteDefinition(_) => (Kind::FootnoteDefinition, None),
            _ => return,
        };
        let info = BlockInfo {
            kind,
            source: Source::Node(node),
            start: 0,
            end: 0,
            inline,
        };
        self.write_block(walk.doc, &info, Mark::Unchanged, None);
    }

    fn write_inline(
        &mut self,
        inline: &Inline,
        words: Option<&WordMarks>,
        images: &mut Vec<ImageRef>,
    ) {
        let mut sink = Sink {
            out: &mut self.out,
            image_url: self.image_url,
            images,
        };
        inline::write(&mut sink, inline, words);
    }

    fn write_code(&mut self, lang: Option<&str>, value: &str) {
        let body = value.strip_suffix('\n').unwrap_or(value);
        let lines: Vec<&str> = if body.is_empty() {
            Vec::new()
        } else {
            body.split('\n').collect()
        };
        self.out.push_str("<code");
        let language = lang.map(str::trim).filter(|lang| !lang.is_empty());
        if let Some(language) = language {
            self.out.push_str(" class=\"language-");
            inline::push_attribute(&mut self.out, language);
            self.out.push('"');
        }
        self.out.push('>');
        let highlighted = self
            .highlight
            .and_then(|highlight| highlight(language.unwrap_or(""), body))
            .filter(|html| html.len() == lines.len());
        match highlighted {
            Some(html) => self.out.push_str(&html.join("\n")),
            None => {
                for (index, line) in lines.iter().enumerate() {
                    if index > 0 {
                        self.out.push('\n');
                    }
                    inline::push_escaped(&mut self.out, line);
                }
            }
        }
        self.out.push_str("</code>");
    }

    fn write_row(
        &mut self,
        row: &TableRow<'_>,
        align: &[AlignKind],
        header: bool,
        doc: &SideDoc<'_>,
        attributes: &str,
    ) {
        self.out.push_str("<tr");
        self.out.push_str(attributes);
        self.out.push_str(">\n");
        let tag = if header { "th" } else { "td" };
        for (index, cell) in row.children.iter().enumerate() {
            self.out.push('<');
            self.out.push_str(tag);
            match align.get(index).copied().unwrap_or(AlignKind::None) {
                AlignKind::Left => self.out.push_str(" style=\"text-align:left\""),
                AlignKind::Center => self.out.push_str(" style=\"text-align:center\""),
                AlignKind::Right => self.out.push_str(" style=\"text-align:right\""),
                AlignKind::None => {}
            }
            self.out.push('>');
            let inline = inline::flatten(&cell.children, &doc.image_context);
            let mut images = Vec::new();
            self.write_inline(&inline, None, &mut images);
            self.out.push_str("</");
            self.out.push_str(tag);
            self.out.push_str(">\n");
        }
        self.out.push_str("</tr>\n");
    }

    /// 段落を持たない項目。中身（入れ子のリストなど）は属性の無い入れ物として書く。
    fn write_item(&mut self, item: &ListItem<'_>, attributes: &str, doc: &SideDoc<'_>) {
        self.out.push_str("<li");
        self.out.push_str(attributes);
        self.out.push('>');
        write_checkbox(&mut self.out, item);
        self.plain += 1;
        let plan = Plan::uniform(0, Mark::Unchanged);
        let mut inner = Walk {
            doc,
            plan: &plan,
            old: None,
            cursor: 0,
        };
        self.write_nodes(&item.children, &mut inner);
        self.plain -= 1;
        self.out.push_str("</li>\n");
    }
}

fn write_checkbox(out: &mut String, item: &ListItem<'_>) {
    match item.checked {
        Some(true) => out.push_str("<input type=\"checkbox\" checked disabled> "),
        Some(false) => out.push_str("<input type=\"checkbox\" disabled> "),
        None => {}
    }
}
