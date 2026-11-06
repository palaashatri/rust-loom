//! Markdown pasted as text becomes document structure.
//!
//! When the plain text on the clipboard reads as Markdown (headings, list
//! items, pipe tables, or bold text between `**` markers), it is converted into
//! real content: heading styles, bulleted and numbered lists, bold and italic
//! runs, and tables. Markup characters are never inserted as text. Text that
//! does not read as Markdown pastes exactly as before.

use loom_objects::Table;
use loom_writer_core::{TextSelection, WriterDocument};

use crate::document_formatting::set_selection_list_style;

/// One block of pasted Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    /// `#` to `######`; levels past three use the third heading style.
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph(Vec<Span>),
    Bullet(Vec<Span>),
    Numbered(Vec<Span>),
    /// Rows of plain cell text. The first row is the header.
    Table(Vec<Vec<String>>),
}

/// A run of text with its emphasis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}

/// What a Markdown paste produced, for the status line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Summary {
    pub(crate) headings: usize,
    pub(crate) list_items: usize,
    pub(crate) tables: usize,
}

/// Whether plain text reads as Markdown: a heading, a list item, a pipe table,
/// or bold text between `**` markers. Anything else pastes unchanged.
pub(crate) fn looks_like_markdown(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines
        .iter()
        .any(|line| is_heading(line) || list_item(line).is_some())
    {
        return true;
    }
    if lines
        .windows(2)
        .any(|pair| table_cells(pair[0]).is_some() && is_table_separator(pair[1]))
    {
        return true;
    }
    lines.iter().any(|line| line.matches("**").count() >= 2)
}

/// Parses Markdown text into blocks. Consecutive plain lines form one paragraph,
/// as they do in Markdown; a blank line ends it.
pub(crate) fn parse(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            index += 1;
            continue;
        }
        if index + 1 < lines.len()
            && table_cells(line).is_some()
            && is_table_separator(lines[index + 1])
        {
            flush_paragraph(&mut paragraph, &mut blocks);
            let mut rows = vec![table_cells(line).unwrap_or_default()];
            let mut next = index + 2;
            while next < lines.len() {
                match table_cells(lines[next]) {
                    Some(cells) => rows.push(cells),
                    None => break,
                }
                next += 1;
            }
            let width = rows.iter().map(Vec::len).max().unwrap_or(0);
            let plain_rows: Vec<Vec<String>> = rows
                .iter()
                .map(|row| {
                    (0..width)
                        .map(|col| {
                            row.get(col)
                                .map(|cell| plain_text(&inline(cell)))
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .collect();
            if width > 0 {
                blocks.push(Block::Table(plain_rows));
            }
            index = next;
            continue;
        }
        if is_heading(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            let trimmed = line.trim_start();
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            let content = trimmed[hashes..].trim();
            blocks.push(Block::Heading {
                level: hashes.min(3) as u8,
                spans: inline(content),
            });
            index += 1;
            continue;
        }
        if let Some((kind, content)) = list_item(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            let spans = inline(content);
            blocks.push(match kind {
                ListKind::Bullet => Block::Bullet(spans),
                ListKind::Numbered => Block::Numbered(spans),
            });
            index += 1;
            continue;
        }
        paragraph.push(line.trim());
        index += 1;
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    blocks
}

/// The text the pasted blocks occupy in the document, one line per block. A
/// table keeps an empty line where it will be placed. A paste that ends in a
/// table gets one more empty line after it, so the caret and any text typed
/// next land in a paragraph below the table, not in its last cell.
pub(crate) fn plain_lines(blocks: &[Block]) -> String {
    let mut text = blocks.iter().map(block_text).collect::<Vec<_>>().join("\n");
    if matches!(blocks.last(), Some(Block::Table(_))) {
        text.push('\n');
    }
    text
}

/// Formats the plain lines that `plain_lines` produced, once they are in the
/// document starting at `insert_at`: heading and list styles, bold and italic
/// runs, and tables placed on their empty lines. Returns what was produced and
/// leaves the caret after the pasted content.
pub(crate) fn format_inserted(
    document: &mut WriterDocument,
    blocks: &[Block],
    insert_at: usize,
) -> Summary {
    let mut summary = Summary::default();
    let mut tables: Vec<(usize, Vec<Vec<String>>)> = Vec::new();
    let mut cursor = insert_at;
    // The length of exactly the text that `plain_lines` inserted at `insert_at`.
    let total = plain_lines(blocks).len();
    for block in blocks {
        let text = block_text(block);
        let start = cursor;
        let end = start + text.len();
        match block {
            Block::Heading { level, spans } => {
                summary.headings += 1;
                loom_writer_core::set_selection_heading(
                    document,
                    TextSelection::range(start, end),
                    i32::from(*level),
                );
                add_emphasis(document, start, spans);
            }
            Block::Paragraph(spans) => add_emphasis(document, start, spans),
            Block::Bullet(spans) | Block::Numbered(spans) => {
                summary.list_items += 1;
                let index = if matches!(block, Block::Bullet(_)) {
                    1
                } else {
                    2
                };
                set_selection_list_style(document, TextSelection::range(start, end), index);
                add_emphasis(document, start, spans);
            }
            Block::Table(rows) => {
                summary.tables += 1;
                tables.push((start, rows.clone()));
            }
        }
        cursor = end + 1;
    }
    // Later tables first, so the offsets of earlier ones stay valid.
    let before = document.editor_text().len();
    for (offset, rows) in tables.iter().rev() {
        let width = rows.first().map(Vec::len).unwrap_or(0);
        if rows.is_empty() || width == 0 {
            continue;
        }
        let mut table = Table::new(rows.len(), width);
        for (row_index, row) in rows.iter().enumerate() {
            for (col_index, cell) in row.iter().enumerate() {
                table.set_cell_text(row_index, col_index, cell.clone());
            }
        }
        // A rejected table leaves its empty line in place, which is harmless.
        let _ = document.insert_table_object(*offset, &table);
    }
    let added_by_tables = document.editor_text().len().saturating_sub(before);
    document.set_selection(TextSelection::caret(insert_at + total + added_by_tables));
    summary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Numbered,
}

fn flush_paragraph(paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if paragraph.is_empty() {
        return;
    }
    let joined = paragraph.join(" ");
    blocks.push(Block::Paragraph(inline(&joined)));
    paragraph.clear();
}

fn block_text(block: &Block) -> String {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) => plain_text(spans),
        Block::Bullet(spans) | Block::Numbered(spans) => plain_text(spans),
        Block::Table(_) => String::new(),
    }
}

fn plain_text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

fn add_emphasis(document: &mut WriterDocument, start: usize, spans: &[Span]) {
    let mut position = start;
    for span in spans {
        let end = position + span.text.len();
        if span.bold && end > position {
            loom_writer_core::set_selection_bold(
                document,
                TextSelection::range(position, end),
                true,
            );
        }
        if span.italic && end > position {
            loom_writer_core::set_selection_italic(
                document,
                TextSelection::range(position, end),
                true,
            );
        }
        position = end;
    }
}

fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ')
}

fn list_item(line: &str) -> Option<(ListKind, &str)> {
    let trimmed = line.trim_start();
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some((ListKind::Bullet, rest.trim_start()));
        }
    }
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) {
        let rest = &trimmed[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((ListKind::Numbered, rest.trim_start()));
        }
    }
    None
}

/// The cells of a pipe-table row, or `None` when the line is not one.
fn table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    Some(
        inner
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect(),
    )
}

fn is_table_separator(line: &str) -> bool {
    match table_cells(line) {
        Some(cells) if !cells.is_empty() => cells.iter().all(|cell| {
            let core = cell.trim().trim_start_matches(':').trim_end_matches(':');
            core.len() >= 3 && core.chars().all(|c| c == '-')
        }),
        _ => false,
    }
}

/// Splits a line into spans of plain, bold and italic text. Code spans keep
/// their text and lose the backticks, links keep their label, and escaped
/// markup characters become literal characters.
pub(crate) fn inline(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c == '\\' && index + 1 < chars.len() && "*_`[]()#\\!".contains(chars[index + 1]) {
            buffer.push(chars[index + 1]);
            index += 2;
            continue;
        }
        if c == '`' {
            if let Some(end) = find_char(&chars, index + 1, '`') {
                buffer.extend(chars[index + 1..end].iter());
                index = end + 1;
                continue;
            }
        }
        if c == '!' && index + 1 < chars.len() && chars[index + 1] == '[' {
            index += 1;
            continue;
        }
        if c == '[' {
            if let Some((label, next)) = link_at(&chars, index) {
                buffer.push_str(&label);
                index = next;
                continue;
            }
        }
        if (c == '*' || c == '_') && index + 1 < chars.len() && chars[index + 1] == c {
            let closing = bold;
            let boundary_ok = if closing {
                index > 0 && !chars[index - 1].is_whitespace()
            } else {
                index + 2 < chars.len() && !chars[index + 2].is_whitespace()
            };
            if boundary_ok {
                push_span(&mut buffer, &mut spans, bold, italic);
                bold = !bold;
                index += 2;
                continue;
            }
        }
        if c == '*' || c == '_' {
            let prev_alnum = index > 0 && chars[index - 1].is_alphanumeric();
            let next_alnum = index + 1 < chars.len() && chars[index + 1].is_alphanumeric();
            let closing = italic;
            let boundary_ok = if closing {
                index > 0 && !chars[index - 1].is_whitespace()
            } else {
                index + 1 < chars.len() && !chars[index + 1].is_whitespace()
            };
            // A marker inside a word (snake_case, 2*3) is literal.
            if boundary_ok && !(prev_alnum && next_alnum) {
                push_span(&mut buffer, &mut spans, bold, italic);
                italic = !italic;
                index += 1;
                continue;
            }
        }
        buffer.push(c);
        index += 1;
    }
    push_span(&mut buffer, &mut spans, bold, italic);
    spans
}

fn push_span(buffer: &mut String, spans: &mut Vec<Span>, bold: bool, italic: bool) {
    if !buffer.is_empty() {
        spans.push(Span {
            text: std::mem::take(buffer),
            bold,
            italic,
        });
    }
}

fn find_char(chars: &[char], from: usize, wanted: char) -> Option<usize> {
    (from..chars.len()).find(|&index| chars[index] == wanted)
}

/// `[label](url)` starting at `start`: the label and the index after `)`.
fn link_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    let close = find_char(chars, start + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find_char(chars, close + 2, ')')?;
    Some((chars[start + 1..close].iter().collect(), end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document_with(text: &str) -> WriterDocument {
        let mut document = WriterDocument::new("test", "Test");
        document.replace_paragraphs(text);
        document
    }

    #[test]
    fn plain_prose_is_not_markdown_so_it_pastes_unchanged() {
        assert!(!looks_like_markdown(
            "A plain sentence with a dash - inside it."
        ));
        assert!(!looks_like_markdown("snake_case_name and 5 * 3 = 15"));
        assert!(!looks_like_markdown("#hashtag without a space"));
    }

    #[test]
    fn headings_lists_tables_and_bold_are_markdown() {
        assert!(looks_like_markdown("# Title"));
        assert!(looks_like_markdown("text\n- item"));
        assert!(looks_like_markdown("| a | b |\n| --- | --- |\n| 1 | 2 |"));
        assert!(looks_like_markdown("This is **bold** text"));
    }

    #[test]
    fn headings_take_their_level_and_strip_the_hashes() {
        let blocks = parse("## Results\n\nBody.");
        assert_eq!(
            blocks[0],
            Block::Heading {
                level: 2,
                spans: vec![Span {
                    text: "Results".into(),
                    bold: false,
                    italic: false
                }],
            }
        );
        assert_eq!(
            blocks[1],
            Block::Paragraph(vec![Span {
                text: "Body.".into(),
                bold: false,
                italic: false
            }])
        );
    }

    #[test]
    fn list_items_are_bullets_or_numbers_without_their_markers() {
        let blocks = parse("- first\n* second\n1. third");
        assert_eq!(
            blocks,
            vec![
                Block::Bullet(vec![Span {
                    text: "first".into(),
                    bold: false,
                    italic: false
                }]),
                Block::Bullet(vec![Span {
                    text: "second".into(),
                    bold: false,
                    italic: false
                }]),
                Block::Numbered(vec![Span {
                    text: "third".into(),
                    bold: false,
                    italic: false
                }]),
            ]
        );
    }

    #[test]
    fn inline_emphasis_becomes_spans_and_markers_disappear() {
        let spans = inline("This is **bold** and *italic* with `code`.");
        let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts.concat(), "This is bold and italic with code.");
        assert!(spans
            .iter()
            .any(|s| s.text == "bold" && s.bold && !s.italic));
        assert!(spans
            .iter()
            .any(|s| s.text == "italic" && s.italic && !s.bold));
        assert!(!spans
            .iter()
            .any(|s| s.text.contains('`') || s.text.contains("**")));
    }

    #[test]
    fn underscores_inside_words_and_spaced_asterisks_stay_literal() {
        assert_eq!(plain_text(&inline("snake_case_name")), "snake_case_name");
        assert_eq!(plain_text(&inline("5 * 3 = 15")), "5 * 3 = 15");
    }

    #[test]
    fn links_keep_their_label_and_escapes_stay_literal() {
        assert_eq!(
            plain_text(&inline("see [the docs](https://example.com) now")),
            "see the docs now"
        );
        assert_eq!(
            plain_text(&inline("a \\*literal\\* star")),
            "a *literal* star"
        );
    }

    #[test]
    fn pipe_tables_keep_their_header_and_rows() {
        let blocks = parse("| Name | Score |\n| --- | --- |\n| Ada | **9** |");
        assert_eq!(
            blocks,
            vec![Block::Table(vec![
                vec!["Name".to_string(), "Score".to_string()],
                vec!["Ada".to_string(), "9".to_string()],
            ])]
        );
    }

    #[test]
    fn a_paste_ending_in_a_table_leaves_the_caret_below_it() {
        let blocks = parse("Intro line\n\n| a | b |\n| --- | --- |\n| 1 | 2 |");
        let mut document = document_with("");
        document.replace_paragraphs(&plain_lines(&blocks));
        format_inserted(&mut document, &blocks, 0);
        let caret = document.selection().focus;
        assert!(
            document.table_cell_at_offset(caret).is_none(),
            "the caret after a pasted table must not sit in its cell (caret {caret})"
        );
    }

    #[test]
    fn applying_markdown_to_a_document_creates_real_structure() {
        let text = "# Quarterly report\n\nThis is **bold** and *italic*.\n\n- first item\n- second item\n\n| Name | Score |\n| --- | --- |\n| Ada | 9 |\n\nAfter the table.";
        let blocks = parse(text);
        let plain = plain_lines(&blocks);
        assert!(!plain.contains('#') && !plain.contains("**") && !plain.contains("| "));
        let mut document = document_with("");
        document.replace_paragraphs(&plain);
        let summary = format_inserted(&mut document, &blocks, 0);
        assert_eq!(summary.headings, 1);
        assert_eq!(summary.list_items, 2);
        assert_eq!(summary.tables, 1);
        let kinds: Vec<&str> = document
            .blocks
            .iter()
            .map(|block| block.kind.as_str())
            .collect();
        assert!(kinds.contains(&"heading1"), "kinds: {kinds:?}");
        assert!(kinds.contains(&"list-bulleted"), "kinds: {kinds:?}");
        let editor = document.editor_text();
        assert!(editor.contains("Quarterly report"));
        assert!(
            !editor.contains("**"),
            "markup must not reach the text: {editor:?}"
        );
        assert!(
            !editor.contains("# "),
            "markup must not reach the text: {editor:?}"
        );
    }
}
