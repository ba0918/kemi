//! 表（CSV / TSV）の描画表示。1 行が表の 1 行なので、行の整列がそのままブロックの
//! 整列になる（R-RENDER）。

use crate::domain::content;
use crate::domain::diff::{self, RowKind};

use super::inline::push_escaped;
use super::{Block, Mark, Rendered, Side, Unrenderable};

pub struct TableInput<'a> {
    pub old: Option<&'a str>,
    pub new: Option<&'a str>,
    /// `.csv` は `,`、`.tsv` はタブ。
    pub delimiter: u8,
}

/// `"` の対応が取れない行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnbalancedQuote;

/// 1 行をフィールドに分ける。フィールドの先頭の `"` で囲んだフィールドと、その中の `""`
/// を解釈する。フィールド内の改行は解釈しない（P19）。
pub fn split_fields(line: &str, delimiter: u8) -> Result<Vec<String>, UnbalancedQuote> {
    let delimiter = delimiter as char;
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut at_start = true;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        if character == delimiter {
            fields.push(std::mem::take(&mut field));
            at_start = true;
            continue;
        }
        if character == '"' && at_start {
            quoted = true;
        } else {
            field.push(character);
        }
        at_start = false;
    }
    if quoted {
        return Err(UnbalancedQuote);
    }
    fields.push(field);
    Ok(fields)
}

/// `"` の対応が取れない最初の行（1 始まり）。無ければ None。
pub fn unbalanced_line(text: &str, delimiter: u8) -> Option<u32> {
    content::lines(text)
        .iter()
        .position(|line| split_fields(line, delimiter).is_err())
        .map(|index| index as u32 + 1)
}

pub fn render_table(input: &TableInput<'_>) -> Result<Rendered, Unrenderable> {
    if !content::within_auto_limit(input.old, input.new) {
        return Err(Unrenderable::TooLarge);
    }
    for side in [input.new, input.old].into_iter().flatten() {
        if let Some(line) = unbalanced_line(side, input.delimiter) {
            return Err(Unrenderable::UnbalancedQuote { line });
        }
    }
    let old_lines = input.old.map(content::lines).unwrap_or_default();
    let new_lines = input.new.map(content::lines).unwrap_or_default();
    let mut writer = TableWriter {
        delimiter: input.delimiter,
        out: String::from("<table class=\"kb-table\">\n"),
        blocks: Vec::new(),
        section: Section::Start,
    };
    for row in diff::align(&old_lines, &new_lines) {
        let old = row
            .old
            .as_ref()
            .map(|line| (line.number, line.text.as_str()));
        let new = row
            .new
            .as_ref()
            .map(|line| (line.number, line.text.as_str()));
        match row.kind {
            RowKind::Equal => writer.row(Side::New, new, Mark::Unchanged),
            RowKind::Insert => writer.row(Side::New, new, Mark::Added),
            RowKind::Delete => writer.row(Side::Old, old, Mark::Deleted),
            RowKind::Replace => {
                writer.row(Side::Old, old, Mark::Deleted);
                writer.row(Side::New, new, Mark::Added);
            }
        }
    }
    writer.finish();
    Ok(Rendered {
        html: writer.out,
        blocks: writer.blocks,
    })
}

#[derive(PartialEq, Eq)]
enum Section {
    Start,
    Head,
    Body,
}

struct TableWriter {
    delimiter: u8,
    out: String,
    blocks: Vec<Block>,
    section: Section,
}

impl TableWriter {
    /// 1 行目（どちらの側でも）は見出し行として `<th>` で出す。
    fn row(&mut self, side: Side, line: Option<(u32, &str)>, mark: Mark) {
        let Some((number, text)) = line else {
            return;
        };
        let header = number == 1;
        match (&self.section, header) {
            (Section::Start, true) => {
                self.out.push_str("<thead>\n");
                self.section = Section::Head;
            }
            (Section::Start, false) => {
                self.out.push_str("<tbody>\n");
                self.section = Section::Body;
            }
            (Section::Head, false) => {
                self.out.push_str("</thead>\n<tbody>\n");
                self.section = Section::Body;
            }
            _ => {}
        }
        self.out.push_str(&format!(
            "<tr class=\"kb{}\" data-kemi-block=\"{}:{number}-{number}\">\n",
            mark.class(),
            side.as_str()
        ));
        let tag = if header { "th" } else { "td" };
        // 対応の取れない行は事前に弾いてあるので、ここでは失敗しない。
        for field in split_fields(text, self.delimiter).unwrap_or_default() {
            self.out.push('<');
            self.out.push_str(tag);
            self.out.push('>');
            push_escaped(&mut self.out, &field);
            self.out.push_str("</");
            self.out.push_str(tag);
            self.out.push_str(">\n");
        }
        self.out.push_str("</tr>\n");
        self.blocks.push(Block {
            side,
            start: number,
            end: number,
            mark,
        });
    }

    fn finish(&mut self) {
        match self.section {
            Section::Start => {}
            Section::Head => self.out.push_str("</thead>\n"),
            Section::Body => self.out.push_str("</tbody>\n"),
        }
        self.out.push_str("</table>\n");
    }
}
