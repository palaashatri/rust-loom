//! Emphasis and alignment inside stored table blocks.
//!
//! A cell's Markdown source may carry emphasis toggles: a run of two stars
//! turns bold on or off, a single star turns italic on or off, and a run of
//! three turns both. A literal asterisk is written `\*`. A kind of toggle whose
//! count in the cell is odd is not treated as emphasis, so a stray asterisk
//! stays visible as typed. Column alignment is written in the table's divider
//! row (`:---`, `:---:`, `---:`), as GitHub-flavoured Markdown does.
//!
//! Readers see the cell with its markers removed; the stored block keeps them.

use std::ops::Range;

use loom_objects::table::{HorizontalAlign, TextRun};

use super::legacy_markdown::ParsedTableCellText;

/// One character of a cell with its emphasis and its byte range in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Styled {
    pub(crate) ch: char,
    pub(crate) source: Range<usize>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}

enum Item {
    Char(char, Range<usize>),
    Stars(usize, Range<usize>),
}

fn items(text: &str) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch == '\\' && chars.peek().is_some_and(|(_, next)| *next == '*') {
            let (star, _) = chars.next().expect("peeked escaped star");
            out.push(Item::Char('*', index..star + 1));
        } else if ch == '*' {
            match out.last_mut() {
                Some(Item::Stars(count, range)) if range.end == index => {
                    *count += 1;
                    range.end = index + 1;
                }
                _ => out.push(Item::Stars(1, index..index + 1)),
            }
        } else {
            out.push(Item::Char(ch, index..index + ch.len_utf8()));
        }
    }
    out
}

/// Reads a cell's Markdown source into styled characters. Each character keeps
/// its byte range in `text`, so callers can map back to the source.
pub(crate) fn parse(text: &str) -> Vec<Styled> {
    let items = items(text);
    let mut bold_toggles = 0usize;
    let mut italic_toggles = 0usize;
    for item in &items {
        if let Item::Stars(count, _) = item {
            if *count == 2 || *count == 3 {
                bold_toggles += 1;
            }
            if *count == 1 || *count == 3 {
                italic_toggles += 1;
            }
        }
    }
    let bold_paired = bold_toggles % 2 == 0;
    let italic_paired = italic_toggles % 2 == 0;

    let (mut bold, mut italic) = (false, false);
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Char(ch, source) => out.push(Styled {
                ch,
                source,
                bold,
                italic,
            }),
            Item::Stars(count, range) => {
                let use_bold = (count == 2 || count == 3) && bold_paired;
                let use_italic = (count == 1 || count == 3) && italic_paired;
                if count >= 4 {
                    push_literal_stars(&mut out, &range, count, bold, italic);
                    continue;
                }
                if use_bold {
                    bold = !bold;
                }
                if use_italic {
                    italic = !italic;
                }
                let spent = (if use_bold { 2 } else { 0 }) + (if use_italic { 1 } else { 0 });
                push_literal_stars(&mut out, &range, count - spent, bold, italic);
            }
        }
    }
    out
}

fn push_literal_stars(
    out: &mut Vec<Styled>,
    range: &Range<usize>,
    count: usize,
    bold: bool,
    italic: bool,
) {
    for offset in 0..count {
        let start = range.start + offset;
        out.push(Styled {
            ch: '*',
            source: start..start + 1,
            bold,
            italic,
        });
    }
}

/// Groups a cell's styled characters into `loom_objects` text runs.
pub(crate) fn runs(text: &str) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    for styled in parse(text) {
        match runs.last_mut() {
            Some(last) if last.bold == styled.bold && last.italic == styled.italic => {
                last.text.push(styled.ch);
            }
            _ => runs.push(TextRun {
                text: styled.ch.to_string(),
                bold: styled.bold,
                italic: styled.italic,
                ..TextRun::plain("")
            }),
        }
    }
    runs
}

/// Returns the cell as a reader sees it: emphasis markers removed and each
/// displayed byte boundary mapped back to the stored source. Cells without
/// asterisks are returned unchanged.
pub(crate) fn display(cell: ParsedTableCellText) -> ParsedTableCellText {
    if !cell.text.contains('*') {
        return cell;
    }
    let styled = parse(&cell.text);
    let Some(first) = styled.first() else {
        return cell;
    };
    let mut text = String::with_capacity(cell.text.len());
    let mut source_offsets = vec![cell.source_offsets[first.source.start]];
    for styled in &styled {
        text.push(styled.ch);
        let width = styled.ch.len_utf8();
        let span = styled.source.len();
        for step in 1..=width {
            source_offsets.push(cell.source_offsets[styled.source.start + step * span / width]);
        }
    }
    ParsedTableCellText {
        text,
        source_offsets,
    }
}

/// Alignment written by one divider cell, as GitHub-flavoured Markdown reads it.
pub(crate) fn divider_alignment(cell: &str) -> HorizontalAlign {
    let trimmed = cell.trim();
    match (trimmed.starts_with(':'), trimmed.ends_with(':')) {
        (true, true) => HorizontalAlign::Center,
        (false, true) => HorizontalAlign::Right,
        _ => HorizontalAlign::Left,
    }
}
