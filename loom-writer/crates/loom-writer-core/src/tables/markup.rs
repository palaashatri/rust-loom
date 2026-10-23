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

use loom_objects::table::{HorizontalAlign, Table, TextRun};

use super::ParsedTableCellText;
use crate::WriterTable;

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

/// Writes a cell's runs as Markdown source. Plain text is written unchanged.
pub(crate) fn cell_markup(runs: &[TextRun]) -> String {
    let (mut bold, mut italic) = (false, false);
    let mut out = String::new();
    for run in runs.iter().filter(|run| !run.text.is_empty()) {
        match (run.bold != bold, run.italic != italic) {
            (true, true) => out.push_str("***"),
            (true, false) => out.push_str("**"),
            (false, true) => out.push('*'),
            (false, false) => {}
        }
        bold = run.bold;
        italic = run.italic;
        for ch in run.text.chars() {
            if ch == '*' {
                out.push_str("\\*");
            } else {
                out.push(ch);
            }
        }
    }
    match (bold, italic) {
        (true, true) => out.push_str("***"),
        (true, false) => out.push_str("**"),
        (false, true) => out.push('*'),
        (false, false) => {}
    }
    out
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

/// Alignment of each column: the first explicit alignment found in the column.
pub(crate) fn column_alignments(table: &Table) -> Vec<HorizontalAlign> {
    (0..table.col_count())
        .map(|column| {
            table
                .rows
                .iter()
                .find_map(|row| {
                    row.cells
                        .get(column)
                        .and_then(|cell| cell.style_override.as_ref())
                        .and_then(|style| style.horizontal_align)
                })
                .unwrap_or(HorizontalAlign::Left)
        })
        .collect()
}

/// Replaces the divider row of a Markdown table written by
/// `WriterTable::to_markdown` with one that carries the column alignments.
pub(crate) fn with_alignment(markdown: &str, alignments: &[HorizontalAlign]) -> String {
    let mut lines: Vec<String> = markdown.split('\n').map(str::to_string).collect();
    if lines.len() < 2 || !lines[1].trim_start().starts_with('|') {
        return markdown.to_string();
    }
    let mut divider = String::from("|");
    for alignment in alignments {
        divider.push_str(match alignment {
            HorizontalAlign::Center => " :---: |",
            HorizontalAlign::Right => " ---: |",
            _ => " --- |",
        });
    }
    lines[1] = divider;
    lines.join("\n")
}

/// A stored table as its text describes it: the displayed table (metadata and
/// plain cell text), each cell's Markdown source with its emphasis markers, and
/// each column's alignment from the divider row.
#[derive(Debug, Clone)]
pub(crate) struct StoredTable {
    pub(crate) table: WriterTable,
    pub(crate) sources: Vec<Vec<String>>,
    pub(crate) alignments: Vec<HorizontalAlign>,
}

impl StoredTable {
    /// The emphasis runs of one cell, over the text a reader sees.
    pub(crate) fn cell_runs(&self, row: usize, column: usize) -> Vec<TextRun> {
        self.sources
            .get(row)
            .and_then(|cells| cells.get(column))
            .map_or_else(Vec::new, |source| runs(source))
    }

    /// The table ready for an edit. Its rows hold the stored sources, so emphasis
    /// and escapes are written back as they were, and every column has an alignment.
    pub(crate) fn into_editable(self) -> (WriterTable, Vec<HorizontalAlign>) {
        let Self {
            mut table,
            sources,
            mut alignments,
        } = self;
        table.rows = sources;
        alignments.resize(table.columns(), HorizontalAlign::Left);
        (table, alignments)
    }
}

/// Reads a stored table for export and editing (see [`StoredTable`]).
pub(crate) fn read_stored(markdown: &str) -> StoredTable {
    let table = super::parse_table_markdown(markdown);
    let mut sources = Vec::new();
    let mut alignments = Vec::new();
    super::for_each_markdown_line(markdown, |line, line_offset| {
        if super::is_table_metadata_line(line.trim()) {
            return;
        }
        let Some(parsed) = super::parse_table_line_raw(line, line_offset) else {
            return;
        };
        if parsed.is_delimiter {
            alignments = parsed
                .cells
                .iter()
                .map(|cell| divider_alignment(&cell.text))
                .collect();
        } else {
            sources.push(parsed.cells.into_iter().map(|cell| cell.text).collect());
        }
    });
    StoredTable {
        table,
        sources,
        alignments,
    }
}

/// Writes an edited table back to Markdown. Cell text is written as stored, so
/// its emphasis markers come back unchanged, and each column's alignment is kept
/// in the divider row.
pub(crate) fn stored_markdown(table: &WriterTable, alignments: &[HorizontalAlign]) -> String {
    let mut aligned = alignments.to_vec();
    aligned.resize(table.columns(), HorizontalAlign::Left);
    with_alignment(&table.to_markdown(), &aligned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_style_and_a_stray_star_stays_literal() {
        let plain: String = parse("2*3").iter().map(|s| s.ch).collect();
        assert_eq!(plain, "2*3");
        let bold: Vec<(char, bool)> = parse("a**b**c").iter().map(|s| (s.ch, s.bold)).collect();
        assert_eq!(bold, vec![('a', false), ('b', true), ('c', false)]);
    }
}
