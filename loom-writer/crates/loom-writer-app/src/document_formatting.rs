//! Document-wide formatting operations used until Writer exposes a stable
//! selection/caret range from the Slint editor.
//!
//! These operations are intentionally explicit about their scope: they mutate
//! every non-empty block in the authoritative `WriterDocument`, preserve
//! existing style-run boundaries where present, and are persisted by the normal
//! Writer package serializer. They are not represented as selection-aware rich
//! text operations.

use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};
use loom_writer_core::{RichBlock, WriterDocument};

/// Formatting state reflected by the current document-wide toolbar controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentFormattingState {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub heading_level: i32,
    pub alignment: i32,
}

impl Default for DocumentFormattingState {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            heading_level: 0,
            alignment: 0,
        }
    }
}

fn mutate_character_styles(
    document: &mut WriterDocument,
    mut operation: impl FnMut(&mut CharacterStyle),
) {
    for block in &mut document.blocks {
        let text_len = block.text.as_str().len();
        if text_len == 0 {
            block.runs.clear();
            continue;
        }

        if block.runs.is_empty() {
            let mut style = CharacterStyle::default();
            operation(&mut style);
            block.runs.push(StyleRun {
                start: 0,
                end: text_len,
                style,
            });
            continue;
        }

        // Preserve any authored run boundaries. The existing Writer surface
        // creates whole-block runs, but imported documents may already contain
        // multiple ranges.
        for run in &mut block.runs {
            operation(&mut run.style);
        }

        // Ensure leading/trailing unstyled text also receives the document-wide
        // operation without destroying the existing runs.
        block.runs.sort_by_key(|run| (run.start, run.end));
        let first_start = block.runs.first().map_or(0, |run| run.start.min(text_len));
        if first_start > 0 {
            let mut style = CharacterStyle::default();
            operation(&mut style);
            block.runs.insert(
                0,
                StyleRun {
                    start: 0,
                    end: first_start,
                    style,
                },
            );
        }
        let last_end = block
            .runs
            .iter()
            .map(|run| run.end.min(text_len))
            .max()
            .unwrap_or(0);
        if last_end < text_len {
            let mut style = CharacterStyle::default();
            operation(&mut style);
            block.runs.push(StyleRun {
                start: last_end,
                end: text_len,
                style,
            });
        }
    }
}

pub fn set_document_bold(document: &mut WriterDocument, enabled: bool) {
    mutate_character_styles(document, |style| {
        style.weight = if enabled {
            FontWeight::Bold
        } else {
            FontWeight::Regular
        };
    });
}

pub fn set_document_italic(document: &mut WriterDocument, enabled: bool) {
    mutate_character_styles(document, |style| style.italic = enabled);
}

pub fn set_document_underline(document: &mut WriterDocument, enabled: bool) {
    mutate_character_styles(document, |style| style.underline = enabled);
}

pub fn set_document_heading(document: &mut WriterDocument, level: i32) {
    let kind = match level {
        1 => "heading1",
        2 => "heading2",
        3 => "heading3",
        _ => "paragraph",
    };
    for block in &mut document.blocks {
        block.kind = kind.to_string();
    }
}

pub fn set_document_alignment(document: &mut WriterDocument, index: i32) {
    let alignment = match index {
        1 => Alignment::Center,
        2 => Alignment::Right,
        3 => Alignment::Justify,
        _ => Alignment::Left,
    };
    for block in &mut document.blocks {
        block.style.alignment = alignment;
    }
}

fn block_character_styles(block: &RichBlock) -> impl Iterator<Item = &CharacterStyle> {
    block.runs.iter().map(|run| &run.style)
}

fn all_non_empty_blocks_match(
    document: &WriterDocument,
    mut predicate: impl FnMut(&CharacterStyle) -> bool,
) -> bool {
    let mut saw_style = false;
    for block in document
        .blocks
        .iter()
        .filter(|block| !block.text.as_str().is_empty())
    {
        if block.runs.is_empty() {
            return false;
        }
        for style in block_character_styles(block) {
            saw_style = true;
            if !predicate(style) {
                return false;
            }
        }
    }
    saw_style
}

fn uniform_heading_level(document: &WriterDocument) -> i32 {
    let mut levels = document.blocks.iter().map(|block| match block.kind.as_str() {
        "heading1" => 1,
        "heading2" => 2,
        "heading3" => 3,
        _ => 0,
    });
    let Some(first) = levels.next() else {
        return 0;
    };
    if levels.all(|level| level == first) {
        first
    } else {
        0
    }
}

fn uniform_alignment(document: &WriterDocument) -> i32 {
    let mut alignments = document.blocks.iter().map(|block| match block.style.alignment {
        Alignment::Center => 1,
        Alignment::Right => 2,
        Alignment::Justify => 3,
        Alignment::Left => 0,
    });
    let Some(first) = alignments.next() else {
        return 0;
    };
    if alignments.all(|alignment| alignment == first) {
        first
    } else {
        0
    }
}

pub fn formatting_state(document: &WriterDocument) -> DocumentFormattingState {
    DocumentFormattingState {
        bold: all_non_empty_blocks_match(document, |style| style.weight == FontWeight::Bold),
        italic: all_non_empty_blocks_match(document, |style| style.italic),
        underline: all_non_empty_blocks_match(document, |style| style.underline),
        heading_level: uniform_heading_level(document),
        alignment: uniform_alignment(document),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> WriterDocument {
        let mut document = WriterDocument::new("formatting", "Formatting");
        document.push(RichBlock::new(1, "paragraph", "First paragraph"));
        document.push(RichBlock::new(2, "paragraph", "Second paragraph"));
        document
    }

    #[test]
    fn document_wide_character_formatting_is_persisted() {
        let mut document = document();
        set_document_bold(&mut document, true);
        set_document_italic(&mut document, true);
        set_document_underline(&mut document, true);

        let state = formatting_state(&document);
        assert!(state.bold);
        assert!(state.italic);
        assert!(state.underline);
        assert!(document.blocks.iter().all(|block| {
            block.runs.len() == 1
                && block.runs[0].start == 0
                && block.runs[0].end == block.text.as_str().len()
        }));

        let bytes = loom_writer_core::save_document(&document).expect("save");
        let loaded = loom_writer_core::load_document(&bytes).expect("load");
        assert_eq!(formatting_state(&loaded), state);
    }

    #[test]
    fn paragraph_formatting_changes_authoritative_blocks() {
        let mut document = document();
        set_document_heading(&mut document, 2);
        set_document_alignment(&mut document, 1);

        assert!(document
            .blocks
            .iter()
            .all(|block| block.kind == "heading2" && block.style.alignment == Alignment::Center));
        assert_eq!(formatting_state(&document).heading_level, 2);
        assert_eq!(formatting_state(&document).alignment, 1);
    }

    #[test]
    fn existing_style_boundaries_are_preserved() {
        let mut document = document();
        document.blocks[0].runs = vec![
            StyleRun {
                start: 0,
                end: 5,
                style: CharacterStyle::default(),
            },
            StyleRun {
                start: 5,
                end: document.blocks[0].text.as_str().len(),
                style: CharacterStyle::default(),
            },
        ];

        set_document_bold(&mut document, true);

        assert_eq!(document.blocks[0].runs.len(), 2);
        assert!(document.blocks[0]
            .runs
            .iter()
            .all(|run| run.style.weight == FontWeight::Bold));
    }
}
