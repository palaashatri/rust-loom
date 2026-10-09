//! Markdown export of a Writer document. Each block becomes its Markdown
//! form, and the run formatting the dialect can express (bold, italic,
//! strikethrough) is written inline. Underline has no Markdown form, so it is
//! written with the `<u>` tags the page markup uses.

use std::collections::BTreeSet;

use loom_text::{CharacterStyle, FontWeight};

use crate::{RichBlock, WriterDocument, TABLE_BLOCK_KIND};

/// The document as Markdown.
pub(crate) fn document_markdown(doc: &WriterDocument) -> String {
    let mut out = String::new();
    let mut numbered_index = 0usize;
    // List items and table rows end without a blank line; the next block must
    // be separated or Markdown readers fold it into the list item or table.
    let mut open_block = false;
    for block in &doc.blocks {
        let kind = block.kind.as_str();
        let is_list = matches!(kind, "list-bulleted" | "list-numbered");
        if open_block && !is_list {
            out.push('\n');
        }
        open_block = is_list || kind == TABLE_BLOCK_KIND;
        match kind {
            "heading1" => out.push_str(&format!("# {}\n\n", inline(block))),
            "heading2" => out.push_str(&format!("## {}\n\n", inline(block))),
            "heading3" => out.push_str(&format!("### {}\n\n", inline(block))),
            "heading4" => out.push_str(&format!("#### {}\n\n", inline(block))),
            "heading5" => out.push_str(&format!("##### {}\n\n", inline(block))),
            "heading6" => out.push_str(&format!("###### {}\n\n", inline(block))),
            "list-bulleted" => {
                numbered_index = 0;
                out.push_str(&format!("- {}\n", inline(block)));
            }
            // A table block already holds its Markdown, so it is written as it is.
            TABLE_BLOCK_KIND => {
                numbered_index = 0;
                out.push_str(&format!("{}\n", block.text.as_str()));
            }
            "list-numbered" => {
                numbered_index += 1;
                out.push_str(&format!("{}. {}\n", numbered_index, inline(block)));
            }
            _ => {
                numbered_index = 0;
                out.push_str(&format!("{}\n\n", inline(block)));
            }
        }
    }
    out
}

/// The block's text with its runs written as Markdown emphasis.
fn inline(block: &RichBlock) -> String {
    let text = block.text.as_str();
    // Cut the text wherever a run starts or ends, so each piece has one style.
    let mut cuts = BTreeSet::from([0, text.len()]);
    for run in &block.runs {
        for edge in [run.start, run.end] {
            if edge <= text.len() && text.is_char_boundary(edge) {
                cuts.insert(edge);
            }
        }
    }
    let cuts: Vec<usize> = cuts.into_iter().collect();

    // Neighbouring pieces with the same formatting share one set of markers.
    let mut spans: Vec<(usize, usize, Markup)> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let markup = block
            .runs
            .iter()
            .find(|run| run.start <= start && start < run.end)
            .map_or_else(Markup::default, |run| Markup::of(&run.style));
        match spans.last_mut() {
            Some(last) if last.2 == markup => last.1 = end,
            _ => spans.push((start, end, markup)),
        }
    }

    let mut out = String::with_capacity(text.len());
    for (start, end, markup) in spans {
        markup.write(&mut out, &text[start..end]);
    }
    out
}

/// The emphasis one span carries.
#[derive(Clone, Copy, Default, PartialEq)]
struct Markup {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
}

impl Markup {
    fn of(style: &CharacterStyle) -> Self {
        Self {
            bold: style.weight.numeric() >= FontWeight::Bold.numeric(),
            italic: style.italic,
            strike: style.strikethrough,
            underline: style.underline,
        }
    }

    /// Writes `piece` with its markers, nested so that every pair closes in
    /// the order it opened. Whitespace at the edges stays outside the markers,
    /// because Markdown does not open or close emphasis beside a space.
    fn write(self, out: &mut String, piece: &str) {
        let core = piece.trim();
        if core.is_empty() || self == Markup::default() {
            out.push_str(&escape(piece));
            return;
        }
        let lead = &piece[..piece.len() - piece.trim_start().len()];
        let trail = &piece[piece.trim_end().len()..];
        let mut markers: Vec<(&str, &str)> = Vec::new();
        if self.underline {
            markers.push(("<u>", "</u>"));
        }
        if self.strike {
            markers.push(("~~", "~~"));
        }
        if self.italic {
            markers.push(("*", "*"));
        }
        if self.bold {
            markers.push(("**", "**"));
        }
        out.push_str(&escape(lead));
        for (open, _) in &markers {
            out.push_str(open);
        }
        out.push_str(&escape(core));
        for (_, close) in markers.iter().rev() {
            out.push_str(close);
        }
        out.push_str(&escape(trail));
    }
}

/// Backslash-escapes the characters a Markdown reader takes as formatting, so
/// the text reads back as it was typed.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\u{5c}' | '*' | '_' | '`' | '[' | ']' | '~') {
            out.push('\u{5c}');
        }
        out.push(ch);
    }
    out
}
