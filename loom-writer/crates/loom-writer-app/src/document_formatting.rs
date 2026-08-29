//! Writer formatting semantics.
//!
//! Selection-aware operations are the primary editing path. Legacy
//! document-wide helpers remain available only for migration/tests; the live
//! Slint editor sends explicit anchor/focus offsets through the controller.

use std::collections::BTreeSet;

use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};
use loom_writer_core::{RichBlock, WriterDocument};

/// Formatting state reflected by the current document-wide toolbar controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentFormattingState {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub heading_level: i32,
    pub alignment: i32,
}

/// UTF-8 byte offsets in Writer's canonical `editor_text()` representation.
/// Anchor/focus retain selection direction; formatting always uses the ordered
/// range. Offsets are clamped down to valid character boundaries before use.
//
// The legacy TextEdit surface does not expose selection offsets yet, so this
// range-aware API is intentionally ahead of its caller. Keep the lint
// exception scoped to these domain helpers rather than hiding dead code in
// the rest of the formatting module.
pub type DocumentSelection = loom_writer_core::TextSelection;

#[allow(dead_code)]
fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut value = offset.min(text.len());
    while value > 0 && !text.is_char_boundary(value) {
        value -= 1;
    }
    value
}

#[allow(dead_code)]
fn normalized_selection(document: &WriterDocument, selection: DocumentSelection) -> (usize, usize) {
    let text = document.editor_text();
    let (start, end) = selection.normalized_range();
    (
        floor_char_boundary(&text, start),
        floor_char_boundary(&text, end),
    )
}

/// Return `(block_index, local_start, local_end)` for text actually covered by
/// a non-collapsed selection. Newline separators are document structure, not
/// styleable characters, and are intentionally omitted from spans.
#[allow(dead_code)]
pub fn selection_text_spans(
    document: &WriterDocument,
    selection: DocumentSelection,
) -> Vec<(usize, usize, usize)> {
    let (start, end) = normalized_selection(document, selection);
    if start == end {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut global_start = 0usize;
    for (index, block) in document.blocks.iter().enumerate() {
        let block_len = block.text.as_str().len();
        let global_end = global_start + block_len;
        let overlap_start = start.max(global_start);
        let overlap_end = end.min(global_end);
        if overlap_start < overlap_end {
            let local_start =
                floor_char_boundary(block.text.as_str(), overlap_start - global_start);
            let local_end = floor_char_boundary(block.text.as_str(), overlap_end - global_start);
            if local_start < local_end {
                spans.push((index, local_start, local_end));
            }
        }
        global_start = global_end + usize::from(index + 1 < document.blocks.len());
    }
    spans
}

#[allow(dead_code)]
fn block_at_offset(document: &WriterDocument, offset: usize) -> Option<usize> {
    if document.blocks.is_empty() {
        return None;
    }
    let text = document.editor_text();
    let offset = floor_char_boundary(&text, offset);
    let mut global_start = 0usize;
    for (index, block) in document.blocks.iter().enumerate() {
        let global_end = global_start + block.text.as_str().len();
        if offset <= global_end || index + 1 == document.blocks.len() {
            return Some(index);
        }
        global_start = global_end + 1;
    }
    Some(document.blocks.len() - 1)
}

/// Paragraph blocks affected by the selection. A collapsed caret affects one
/// paragraph. A range crossing paragraph separators affects the paragraphs it
/// enters, even when a paragraph itself is empty.
#[allow(dead_code)]
pub fn selected_block_indices(
    document: &WriterDocument,
    selection: DocumentSelection,
) -> Vec<usize> {
    let (start, end) = normalized_selection(document, selection);
    let Some(first) = block_at_offset(document, start) else {
        return Vec::new();
    };
    if start == end {
        return vec![first];
    }
    let last_probe = end.saturating_sub(1);
    let last = block_at_offset(document, last_probe).unwrap_or(first);
    (first.min(last)..=first.max(last)).collect()
}

#[allow(dead_code)]
fn style_at(block: &RichBlock, byte_offset: usize) -> CharacterStyle {
    block
        .runs
        .iter()
        .find(|run| run.start <= byte_offset && byte_offset < run.end)
        .map(|run| run.style.clone())
        .unwrap_or_default()
}

#[allow(dead_code)]
fn coalesce_runs(runs: Vec<StyleRun>) -> Vec<StyleRun> {
    let mut result: Vec<StyleRun> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.start == run.end {
            continue;
        }
        if let Some(last) = result.last_mut() {
            if last.end == run.start && last.style == run.style {
                last.end = run.end;
                continue;
            }
        }
        result.push(run);
    }
    result
}

#[allow(dead_code)]
fn mutate_character_range(
    block: &mut RichBlock,
    start: usize,
    end: usize,
    mut operation: impl FnMut(&mut CharacterStyle),
) {
    let text = block.text.as_str();
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start >= end || text.is_empty() {
        return;
    }

    let mut boundaries = BTreeSet::from([0usize, text.len(), start, end]);
    for run in &block.runs {
        boundaries.insert(floor_char_boundary(text, run.start));
        boundaries.insert(floor_char_boundary(text, run.end));
    }
    let boundaries: Vec<usize> = boundaries.into_iter().collect();
    let mut next = Vec::new();
    for pair in boundaries.windows(2) {
        let interval_start = pair[0];
        let interval_end = pair[1];
        if interval_start >= interval_end {
            continue;
        }
        let mut style = style_at(block, interval_start);
        if interval_start < end && interval_end > start {
            operation(&mut style);
        }
        next.push(StyleRun {
            start: interval_start,
            end: interval_end,
            style,
        });
    }
    block.runs = coalesce_runs(next);
}

#[allow(dead_code)]
fn mutate_selection_character_styles(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    mut operation: impl FnMut(&mut CharacterStyle),
) {
    let spans = selection_text_spans(document, selection.clone());
    for (block_index, start, end) in spans {
        mutate_character_range(
            &mut document.blocks[block_index],
            start,
            end,
            &mut operation,
        );
    }
}

#[allow(dead_code)]
pub fn set_selection_bold(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    enabled: bool,
) {
    mutate_selection_character_styles(document, selection, |style| {
        style.weight = if enabled {
            FontWeight::Bold
        } else {
            FontWeight::Regular
        };
    });
}

#[allow(dead_code)]
pub fn set_selection_italic(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    enabled: bool,
) {
    mutate_selection_character_styles(document, selection, |style| style.italic = enabled);
}

#[allow(dead_code)]
pub fn set_selection_underline(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    enabled: bool,
) {
    mutate_selection_character_styles(document, selection, |style| style.underline = enabled);
}

#[allow(dead_code)]
pub fn set_selection_heading(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    level: i32,
) {
    let kind = match level {
        1 => "heading1",
        2 => "heading2",
        3 => "heading3",
        _ => "paragraph",
    };
    for index in selected_block_indices(document, selection) {
        document.blocks[index].kind = kind.to_string();
    }
}

#[allow(dead_code)]
pub fn set_selection_alignment(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    index: i32,
) {
    let alignment = match index {
        1 => Alignment::Center,
        2 => Alignment::Right,
        3 => Alignment::Justify,
        _ => Alignment::Left,
    };
    for block_index in selected_block_indices(document, selection) {
        document.blocks[block_index].style.alignment = alignment;
    }
}

// -------------------------------------------------------------------------
// Explicit legacy document-wide operations retained for package migration and
// older headless callers. The live application does not expose these paths.
// -------------------------------------------------------------------------

#[allow(dead_code)]
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

        for run in &mut block.runs {
            operation(&mut run.style);
        }

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

#[allow(dead_code)]
pub fn set_document_bold(document: &mut WriterDocument, enabled: bool) {
    mutate_character_styles(document, |style| {
        style.weight = if enabled {
            FontWeight::Bold
        } else {
            FontWeight::Regular
        };
    });
}

#[allow(dead_code)]
pub fn set_document_italic(document: &mut WriterDocument, enabled: bool) {
    mutate_character_styles(document, |style| style.italic = enabled);
}

#[allow(dead_code)]
pub fn set_document_underline(document: &mut WriterDocument, enabled: bool) {
    mutate_character_styles(document, |style| style.underline = enabled);
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn block_character_styles(block: &RichBlock) -> impl Iterator<Item = &CharacterStyle> {
    block.runs.iter().map(|run| &run.style)
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn uniform_heading_level(document: &WriterDocument) -> i32 {
    let mut levels = document
        .blocks
        .iter()
        .map(|block| match block.kind.as_str() {
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

#[allow(dead_code)]
fn uniform_alignment(document: &WriterDocument) -> i32 {
    let mut alignments = document
        .blocks
        .iter()
        .map(|block| match block.style.alignment {
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

#[allow(dead_code)]
pub fn formatting_state(document: &WriterDocument) -> DocumentFormattingState {
    DocumentFormattingState {
        bold: all_non_empty_blocks_match(document, |style| style.weight == FontWeight::Bold),
        italic: all_non_empty_blocks_match(document, |style| style.italic),
        underline: all_non_empty_blocks_match(document, |style| style.underline),
        heading_level: uniform_heading_level(document),
        alignment: uniform_alignment(document),
    }
}

/// Return formatting controls for the active text selection rather than for
/// the whole document. Inline controls are checked only when every character
/// covered by the selection has that style; a collapsed caret reflects the
/// character immediately before it (or the first character at the start of a
/// block), which is the style a subsequent insertion will inherit.
pub fn formatting_state_for_selection(
    document: &WriterDocument,
    selection: DocumentSelection,
) -> DocumentFormattingState {
    let spans = selection_text_spans(document, selection.clone());
    let style_matches = |predicate: &dyn Fn(&CharacterStyle) -> bool| {
        if spans.is_empty() {
            return caret_style(document, selection.clone()).is_some_and(|style| predicate(&style));
        }
        let mut saw_style = false;
        for (block_index, start, end) in &spans {
            let block = &document.blocks[*block_index];
            let mut boundaries = BTreeSet::from([*start, *end]);
            for run in &block.runs {
                if run.start < *end && run.end > *start {
                    boundaries.insert(run.start.max(*start).min(*end));
                    boundaries.insert(run.end.max(*start).min(*end));
                }
            }
            let boundaries: Vec<usize> = boundaries.into_iter().collect();
            for pair in boundaries.windows(2) {
                if pair[0] >= pair[1] {
                    continue;
                }
                saw_style = true;
                if !predicate(&style_at(block, pair[0])) {
                    return false;
                }
            }
        }
        saw_style
    };

    let selected_blocks = selected_block_indices(document, selection.clone());
    let heading_level = selected_blocks
        .first()
        .map(|index| heading_level_for_kind(&document.blocks[*index].kind))
        .filter(|first| {
            selected_blocks
                .iter()
                .all(|index| heading_level_for_kind(&document.blocks[*index].kind) == *first)
        })
        .unwrap_or(0);
    let alignment = selected_blocks
        .first()
        .map(|index| alignment_index(document.blocks[*index].style.alignment))
        .filter(|first| {
            selected_blocks
                .iter()
                .all(|index| alignment_index(document.blocks[*index].style.alignment) == *first)
        })
        .unwrap_or(0);

    DocumentFormattingState {
        bold: style_matches(&|style| style.weight == FontWeight::Bold),
        italic: style_matches(&|style| style.italic),
        underline: style_matches(&|style| style.underline),
        heading_level,
        alignment,
    }
}

fn heading_level_for_kind(kind: &str) -> i32 {
    match kind {
        "heading1" => 1,
        "heading2" => 2,
        "heading3" => 3,
        _ => 0,
    }
}

fn alignment_index(alignment: Alignment) -> i32 {
    match alignment {
        Alignment::Left => 0,
        Alignment::Center => 1,
        Alignment::Right => 2,
        Alignment::Justify => 3,
    }
}

fn caret_style(document: &WriterDocument, selection: DocumentSelection) -> Option<CharacterStyle> {
    let (start, _) = normalized_selection(document, selection);
    let block_index = block_at_offset(document, start)?;
    let block = &document.blocks[block_index];
    let text = block.text.as_str();
    if text.is_empty() {
        return Some(CharacterStyle::default());
    }
    let global_start = document.blocks[..block_index]
        .iter()
        .map(|block| block.text.as_str().len() + 1)
        .sum::<usize>();
    let local = start.saturating_sub(global_start).min(text.len());
    if local == text.len() {
        let previous = text[..local]
            .char_indices()
            .last()
            .map(|(offset, _)| offset);
        previous.map(|offset| style_at(block, offset))
    } else {
        Some(style_at(block, local))
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
    fn selection_offsets_are_direction_independent_and_utf8_safe() {
        let mut document = WriterDocument::new("utf8", "UTF-8");
        document.push(RichBlock::new(1, "paragraph", "AéB"));
        document.push(RichBlock::new(2, "paragraph", "C"));

        // Offset 2 lands inside the two-byte é. It must floor to byte 1.
        let spans = selection_text_spans(&document, DocumentSelection::range(4, 2));
        assert_eq!(spans, vec![(0, 1, 4)]);
    }

    #[test]
    fn selection_character_formatting_splits_and_coalesces_runs() {
        let mut document = WriterDocument::new("selection", "Selection");
        document.push(RichBlock::new(1, "paragraph", "abcdef"));

        set_selection_bold(&mut document, DocumentSelection::range(2, 5), true);
        assert_eq!(document.blocks[0].runs.len(), 3);
        assert_eq!(
            (
                document.blocks[0].runs[0].start,
                document.blocks[0].runs[0].end
            ),
            (0, 2)
        );
        assert_eq!(
            (
                document.blocks[0].runs[1].start,
                document.blocks[0].runs[1].end
            ),
            (2, 5)
        );
        assert_eq!(document.blocks[0].runs[1].style.weight, FontWeight::Bold);
        assert_eq!(
            (
                document.blocks[0].runs[2].start,
                document.blocks[0].runs[2].end
            ),
            (5, 6)
        );

        set_selection_bold(&mut document, DocumentSelection::range(2, 5), false);
        assert_eq!(document.blocks[0].runs.len(), 1);
        assert_eq!(
            (
                document.blocks[0].runs[0].start,
                document.blocks[0].runs[0].end
            ),
            (0, 6)
        );
        assert_eq!(document.blocks[0].runs[0].style.weight, FontWeight::Regular);
    }

    #[test]
    fn multi_paragraph_selection_formats_only_intersecting_text() {
        let mut document = WriterDocument::new("multi", "Multi");
        document.push(RichBlock::new(1, "paragraph", "alpha"));
        document.push(RichBlock::new(2, "paragraph", "beta"));
        document.push(RichBlock::new(3, "paragraph", "gamma"));

        // editor_text: "alpha\nbeta\ngamma". Select "ha\nbe".
        set_selection_italic(&mut document, DocumentSelection::range(3, 8), true);
        assert!(document.blocks[0]
            .runs
            .iter()
            .any(|run| run.start == 3 && run.end == 5 && run.style.italic));
        assert!(document.blocks[1]
            .runs
            .iter()
            .any(|run| run.start == 0 && run.end == 2 && run.style.italic));
        assert!(document.blocks[2].runs.is_empty());
    }

    #[test]
    fn formatting_state_for_selection_reflects_only_the_active_range() {
        let mut document = WriterDocument::new("selection-state", "Selection state");
        document.push(RichBlock::new(1, "paragraph", "bold italic"));
        set_selection_bold(&mut document, DocumentSelection::range(0, 4), true);
        set_selection_italic(&mut document, DocumentSelection::range(5, 11), true);

        let bold = formatting_state_for_selection(&document, DocumentSelection::range(0, 4));
        assert!(bold.bold);
        assert!(!bold.italic);

        let italic = formatting_state_for_selection(&document, DocumentSelection::range(5, 11));
        assert!(!italic.bold);
        assert!(italic.italic);
    }

    #[test]
    fn paragraph_formatting_uses_caret_or_selected_block_range() {
        let mut document = document();
        let second_start = document.blocks[0].text.as_str().len() + 1;

        set_selection_heading(&mut document, DocumentSelection::caret(second_start + 2), 2);
        set_selection_alignment(&mut document, DocumentSelection::caret(second_start + 2), 1);
        assert_eq!(document.blocks[0].kind, "paragraph");
        assert_eq!(document.blocks[1].kind, "heading2");
        assert_eq!(document.blocks[1].style.alignment, Alignment::Center);
    }

    #[test]
    fn collapsed_selection_does_not_retroactively_style_characters() {
        let mut document = document();
        set_selection_underline(&mut document, DocumentSelection::caret(3), true);
        assert!(document.blocks.iter().all(|block| block.runs.is_empty()));
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
    fn document_wide_paragraph_formatting_changes_authoritative_blocks() {
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
    fn existing_style_boundaries_are_preserved_by_document_operation() {
        let mut document = document();
        let first_block_len = document.blocks[0].text.as_str().len();
        document.blocks[0].runs = vec![
            StyleRun {
                start: 0,
                end: 5,
                style: CharacterStyle::default(),
            },
            StyleRun {
                start: 5,
                end: first_block_len,
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
