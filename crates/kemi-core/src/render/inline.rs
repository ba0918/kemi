//! 文のブロックのインライン。構文木の子ノードを「書式付きのテキストの並び」にほどき、
//! 旧新の構造の比較と、語の印を入れた HTML の書き出しに使う。

use ox_content_ast::Node;

use super::url::{self, LinkTarget};
use super::{ImageRef, ImageUrl, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WrapKind {
    Emphasis,
    Strong,
    Strike,
    Link,
    Superscript,
    Subscript,
}

/// テキストを包む書式。リンクは飛び先の判定を持つ。
pub(super) struct Wrap {
    kind: WrapKind,
    target: Option<LinkTarget>,
    title: Option<String>,
}

pub(super) enum ImageSource {
    External(String),
    /// 参照と、元のパス（`data-kemi-path` に出す）。
    Reference(ImageRef, String),
    /// `src` を出さない枠。中身は元のパス。
    Frame(String),
}

pub(super) struct ImageSpec {
    alt: String,
    title: Option<String>,
    source: ImageSource,
}

/// テキストでない、それ自体で 1 つの要素。
pub(super) enum Atom {
    Break,
    Code(String),
    Image(Box<ImageSpec>),
    Footnote(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AtomKind {
    Break,
    Code,
    Image,
    Footnote,
}

impl Atom {
    fn kind(&self) -> AtomKind {
        match self {
            Atom::Break => AtomKind::Break,
            Atom::Code(_) => AtomKind::Code,
            Atom::Image(_) => AtomKind::Image,
            Atom::Footnote(_) => AtomKind::Footnote,
        }
    }
}

pub(super) enum Content {
    Text(String),
    Atom(Atom),
}

pub(super) struct Piece {
    /// 外側から内側への、`Inline::wraps` の添字。
    wraps: Vec<usize>,
    content: Content,
}

/// インラインの構造。旧と新でこれが同じ文のブロックだけを重ねる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sig {
    Open(WrapKind),
    Close(WrapKind),
    Text,
    Atom(AtomKind),
}

pub(super) struct Inline {
    wraps: Vec<Wrap>,
    pieces: Vec<Piece>,
    /// テキストの断片をつないだ全体。語の差分はこれで取る。
    pub(super) text: String,
    pub(super) signature: Vec<Sig>,
}

/// 相対パス画像を解決するのに要る、その側の情報。
pub(super) struct ImageContext<'a> {
    pub side: Side,
    pub markdown_path: Option<&'a str>,
    /// その側のレビュー対象ファイル（id とパス）。
    pub review_paths: &'a [(String, String)],
    pub repo_readable: bool,
}

pub(super) fn flatten(nodes: &[Node<'_>], context: &ImageContext<'_>) -> Inline {
    let mut inline = Inline {
        wraps: Vec::new(),
        pieces: Vec::new(),
        text: String::new(),
        signature: Vec::new(),
    };
    let mut stack = Vec::new();
    walk(nodes, &mut stack, &mut inline, context);
    inline.signature = signature(&inline);
    inline
}

fn walk(
    nodes: &[Node<'_>],
    stack: &mut Vec<usize>,
    inline: &mut Inline,
    context: &ImageContext<'_>,
) {
    for node in nodes {
        match node {
            Node::Text(text) => push_text(inline, stack, text.value),
            // 生 HTML は解釈せず、テキストとして出す（R-RENDER）。
            Node::Html(html) => push_text(inline, stack, html.value),
            Node::InlineMath(math) => push_text(inline, stack, math.value),
            Node::Emphasis(node) => wrapped(
                inline,
                stack,
                WrapKind::Emphasis,
                None,
                None,
                |inline, stack| walk(&node.children, stack, inline, context),
            ),
            Node::Strong(node) => wrapped(
                inline,
                stack,
                WrapKind::Strong,
                None,
                None,
                |inline, stack| walk(&node.children, stack, inline, context),
            ),
            Node::Delete(node) => wrapped(
                inline,
                stack,
                WrapKind::Strike,
                None,
                None,
                |inline, stack| walk(&node.children, stack, inline, context),
            ),
            Node::Superscript(node) => wrapped(
                inline,
                stack,
                WrapKind::Superscript,
                None,
                None,
                |inline, stack| walk(&node.children, stack, inline, context),
            ),
            Node::Subscript(node) => wrapped(
                inline,
                stack,
                WrapKind::Subscript,
                None,
                None,
                |inline, stack| walk(&node.children, stack, inline, context),
            ),
            Node::Link(link) => wrapped(
                inline,
                stack,
                WrapKind::Link,
                Some(url::classify(link.url)),
                link.title.map(str::to_string),
                |inline, stack| walk(&link.children, stack, inline, context),
            ),
            Node::InlineCode(code) => push_atom(inline, stack, Atom::Code(code.value.to_string())),
            Node::Break(_) => push_atom(inline, stack, Atom::Break),
            Node::Image(image) => push_atom(
                inline,
                stack,
                Atom::Image(Box::new(ImageSpec {
                    alt: image.alt.to_string(),
                    title: image.title.map(str::to_string),
                    source: resolve_image(image.url, context),
                })),
            ),
            Node::FootnoteReference(reference) => push_atom(
                inline,
                stack,
                Atom::Footnote(reference.label.unwrap_or(reference.identifier).to_string()),
            ),
            // ブロックのノードはインラインの中に現れない。
            _ => {}
        }
    }
}

fn wrapped(
    inline: &mut Inline,
    stack: &mut Vec<usize>,
    kind: WrapKind,
    target: Option<LinkTarget>,
    title: Option<String>,
    children: impl FnOnce(&mut Inline, &mut Vec<usize>),
) {
    inline.wraps.push(Wrap {
        kind,
        target,
        title,
    });
    stack.push(inline.wraps.len() - 1);
    children(inline, stack);
    stack.pop();
}

fn push_text(inline: &mut Inline, stack: &[usize], text: &str) {
    if text.is_empty() {
        return;
    }
    inline.text.push_str(text);
    if let Some(Piece {
        wraps,
        content: Content::Text(existing),
    }) = inline.pieces.last_mut()
    {
        if wraps.as_slice() == stack {
            existing.push_str(text);
            return;
        }
    }
    inline.pieces.push(Piece {
        wraps: stack.to_vec(),
        content: Content::Text(text.to_string()),
    });
}

fn push_atom(inline: &mut Inline, stack: &[usize], atom: Atom) {
    inline.pieces.push(Piece {
        wraps: stack.to_vec(),
        content: Content::Atom(atom),
    });
}

fn signature(inline: &Inline) -> Vec<Sig> {
    let mut signature = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for piece in &inline.pieces {
        let shared = common_prefix(&open, &piece.wraps);
        for index in open[shared..].iter().rev() {
            signature.push(Sig::Close(inline.wraps[*index].kind));
        }
        for index in &piece.wraps[shared..] {
            signature.push(Sig::Open(inline.wraps[*index].kind));
        }
        open = piece.wraps.clone();
        signature.push(match &piece.content {
            Content::Text(_) => Sig::Text,
            Content::Atom(atom) => Sig::Atom(atom.kind()),
        });
    }
    for index in open.iter().rev() {
        signature.push(Sig::Close(inline.wraps[*index].kind));
    }
    signature
}

fn common_prefix(left: &[usize], right: &[usize]) -> usize {
    left.iter().zip(right).take_while(|(a, b)| a == b).count()
}

fn resolve_image(raw: &str, context: &ImageContext<'_>) -> ImageSource {
    let relative = match url::classify(raw) {
        LinkTarget::External(url) => return ImageSource::External(url),
        LinkTarget::Relative(path) => path,
        LinkTarget::Anchor(_) | LinkTarget::Invalid => {
            return ImageSource::Frame(raw.trim().to_string())
        }
    };
    let Some(markdown_path) = context.markdown_path else {
        return ImageSource::Frame(relative);
    };
    let Some(normalized) = url::resolve_relative(markdown_path, &relative) else {
        return ImageSource::Frame(relative);
    };
    if !url::has_image_extension(&normalized) {
        return ImageSource::Frame(relative);
    }
    if let Some((file_id, _)) = context
        .review_paths
        .iter()
        .find(|(_, path)| *path == normalized)
    {
        return ImageSource::Reference(
            ImageRef::Review {
                file_id: file_id.clone(),
                side: context.side,
            },
            relative,
        );
    }
    if context.repo_readable {
        return ImageSource::Reference(
            ImageRef::Repo {
                path: normalized,
                side: context.side,
            },
            relative,
        );
    }
    ImageSource::Frame(relative)
}

/// 重ねたブロックに入れる語の印。オフセットは新側のテキスト全体のもの。
pub(super) struct WordMarks {
    /// 足した語の範囲。
    pub inserted: Vec<(usize, usize)>,
    /// 消した語と、それを差し込む新側のオフセット。
    pub deleted: Vec<(usize, String)>,
}

pub(super) struct Sink<'s> {
    pub out: &'s mut String,
    pub image_url: ImageUrl<'s>,
    pub images: &'s mut Vec<ImageRef>,
}

/// インラインを HTML にする。`marks` があれば、語の印を書式の中に入れる。
pub(super) fn write(sink: &mut Sink<'_>, inline: &Inline, marks: Option<&WordMarks>) {
    let mut open: Vec<usize> = Vec::new();
    let mut offset = 0;
    let mut deleted = marks.map(|marks| marks.deleted.as_slice()).unwrap_or(&[]);
    let inserted = marks.map(|marks| marks.inserted.as_slice()).unwrap_or(&[]);
    for piece in &inline.pieces {
        let shared = common_prefix(&open, &piece.wraps);
        for index in open[shared..].iter().rev() {
            close_wrap(sink.out, &inline.wraps[*index]);
        }
        for index in &piece.wraps[shared..] {
            open_wrap(sink.out, &inline.wraps[*index]);
        }
        open = piece.wraps.clone();
        match &piece.content {
            Content::Text(text) => {
                let end = offset + text.len();
                write_text(sink.out, text, offset, inserted, &mut deleted);
                offset = end;
            }
            Content::Atom(atom) => write_atom(sink, atom),
        }
    }
    for (_, text) in deleted {
        write_deleted(sink.out, text);
    }
    for index in open.iter().rev() {
        close_wrap(sink.out, &inline.wraps[*index]);
    }
}

/// テキストの断片を、足した語の範囲と消した語の差し込み位置で切りながら書く。
fn write_text(
    out: &mut String,
    text: &str,
    start: usize,
    inserted: &[(usize, usize)],
    deleted: &mut &[(usize, String)],
) {
    let end = start + text.len();
    let mut boundaries = vec![start, end];
    for (from, to) in inserted {
        for point in [from, to] {
            if *point > start && *point < end {
                boundaries.push(*point);
            }
        }
    }
    for (at, _) in deleted.iter() {
        if *at > start && *at < end {
            boundaries.push(*at);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    for window in boundaries.windows(2) {
        let (from, to) = (window[0], window[1]);
        while let Some(((at, removed), rest)) = deleted.split_first() {
            if *at > from {
                break;
            }
            write_deleted(out, removed);
            *deleted = rest;
        }
        let piece = &text[from - start..to - start];
        let added = inserted.iter().any(|(a, b)| *a <= from && to <= *b);
        if added {
            out.push_str("<span class=\"kw-add\">");
            push_escaped(out, piece);
            out.push_str("</span>");
        } else {
            push_escaped(out, piece);
        }
    }
    while let Some(((at, removed), rest)) = deleted.split_first() {
        if *at >= end {
            break;
        }
        write_deleted(out, removed);
        *deleted = rest;
    }
}

fn write_deleted(out: &mut String, text: &str) {
    out.push_str("<span class=\"kw-del\">");
    push_escaped(out, text);
    out.push_str("</span>");
}

fn open_wrap(out: &mut String, wrap: &Wrap) {
    match wrap.kind {
        WrapKind::Emphasis => out.push_str("<em>"),
        WrapKind::Strong => out.push_str("<strong>"),
        WrapKind::Strike => out.push_str("<del>"),
        WrapKind::Superscript => out.push_str("<sup>"),
        WrapKind::Subscript => out.push_str("<sub>"),
        WrapKind::Link => match &wrap.target {
            Some(LinkTarget::External(url)) => {
                out.push_str("<a href=\"");
                push_attribute(out, url);
                out.push_str("\" target=\"_blank\" rel=\"noopener noreferrer\"");
                push_title(out, wrap.title.as_deref());
                out.push('>');
            }
            Some(LinkTarget::Anchor(anchor)) => {
                out.push_str("<a href=\"");
                push_attribute(out, anchor);
                out.push('"');
                push_title(out, wrap.title.as_deref());
                out.push('>');
            }
            // 相対パスはリンクにせず、パスを title で見せる（R-RENDER）。
            Some(LinkTarget::Relative(path)) => {
                out.push_str("<span class=\"kb-link\" title=\"");
                push_attribute(out, path);
                out.push_str("\">");
            }
            Some(LinkTarget::Invalid) | None => out.push_str("<span class=\"kb-link\">"),
        },
    }
}

fn close_wrap(out: &mut String, wrap: &Wrap) {
    match wrap.kind {
        WrapKind::Emphasis => out.push_str("</em>"),
        WrapKind::Strong => out.push_str("</strong>"),
        WrapKind::Strike => out.push_str("</del>"),
        WrapKind::Superscript => out.push_str("</sup>"),
        WrapKind::Subscript => out.push_str("</sub>"),
        WrapKind::Link => match &wrap.target {
            Some(LinkTarget::External(_) | LinkTarget::Anchor(_)) => out.push_str("</a>"),
            _ => out.push_str("</span>"),
        },
    }
}

fn push_title(out: &mut String, title: Option<&str>) {
    if let Some(title) = title {
        out.push_str(" title=\"");
        push_attribute(out, title);
        out.push('"');
    }
}

fn write_atom(sink: &mut Sink<'_>, atom: &Atom) {
    match atom {
        Atom::Break => sink.out.push_str("<br>"),
        Atom::Code(code) => {
            sink.out.push_str("<code>");
            push_escaped(sink.out, code);
            sink.out.push_str("</code>");
        }
        Atom::Footnote(label) => {
            sink.out.push_str("<sup class=\"kb-fnref\">[");
            push_escaped(sink.out, label);
            sink.out.push_str("]</sup>");
        }
        Atom::Image(image) => write_image(sink, image),
    }
}

fn write_image(sink: &mut Sink<'_>, image: &ImageSpec) {
    match &image.source {
        ImageSource::External(url) => {
            sink.out.push_str("<img src=\"");
            push_attribute(sink.out, url);
            sink.out.push_str("\" alt=\"");
            push_attribute(sink.out, &image.alt);
            sink.out.push('"');
            push_title(sink.out, image.title.as_deref());
            sink.out.push('>');
        }
        ImageSource::Reference(reference, path) => {
            let url = (sink.image_url)(reference);
            sink.images.push(reference.clone());
            sink.out.push_str("<img src=\"");
            push_attribute(sink.out, &url);
            sink.out.push_str("\" alt=\"");
            push_attribute(sink.out, &image.alt);
            sink.out.push_str("\" data-kemi-path=\"");
            push_attribute(sink.out, path);
            sink.out.push('"');
            push_title(sink.out, image.title.as_deref());
            sink.out.push('>');
        }
        ImageSource::Frame(path) => write_image_frame(sink.out, &image.alt, path),
    }
}

/// `src` を出さない画像の枠。alt テキストとパスを示す。
pub(super) fn write_image_frame(out: &mut String, alt: &str, path: &str) {
    out.push_str("<span class=\"kb-img-frame\" title=\"");
    push_attribute(out, path);
    out.push_str("\"><span class=\"kb-img-alt\">");
    push_escaped(out, alt);
    out.push_str("</span><span class=\"kb-img-path\">");
    push_escaped(out, path);
    out.push_str("</span></span>");
}

pub(super) fn push_escaped(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(character),
        }
    }
}

pub(super) fn push_attribute(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
}
