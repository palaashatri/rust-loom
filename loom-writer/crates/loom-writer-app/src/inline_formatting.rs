//! Selection-aware colour and baseline-script formatting for text paragraphs.

use std::collections::BTreeSet;

use loom_text::CharacterStyle;
use loom_writer_core::{RichBlock, WriterDocument};

use crate::document_formatting::{
    block_at_offset, caret_style, mutate_character_range, selection_text_spans, style_at,
    DocumentSelection,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InlineSelectionState {
    pub(crate) superscript: bool,
    pub(crate) superscript_mixed: bool,
    pub(crate) subscript: bool,
    pub(crate) subscript_mixed: bool,
    pub(crate) foreground_color: Option<String>,
    pub(crate) foreground_color_mixed: bool,
}

/// Character state shown by the inspector. A mixed selection is left
/// unchecked, while its accessible description reports the mixed value.
pub(crate) fn selection_state(
    document: &WriterDocument,
    selection: DocumentSelection,
) -> InlineSelectionState {
    let styles = selected_character_styles(document, selection);
    if styles.is_empty() {
        return InlineSelectionState::default();
    }

    let (superscript, superscript_mixed) = boolean_state(&styles, |style| style.superscript);
    let (subscript, subscript_mixed) = boolean_state(&styles, |style| style.subscript);
    let first_color = styles[0].color.clone();
    let foreground_color_mixed = styles
        .iter()
        .skip(1)
        .any(|style| !same_color(style.color.as_deref(), first_color.as_deref()));

    InlineSelectionState {
        superscript,
        superscript_mixed,
        subscript,
        subscript_mixed,
        foreground_color: (!foreground_color_mixed).then_some(first_color).flatten(),
        foreground_color_mixed,
    }
}

pub(crate) fn can_format_selection(
    document: &WriterDocument,
    selection: DocumentSelection,
) -> bool {
    if crate::table_actions::table_format_actions::can_format_selection(document, selection.clone())
    {
        return true;
    }
    if selection.is_collapsed() {
        return block_at_offset(document, selection.focus)
            .and_then(|index| document.blocks.get(index))
            .is_some_and(is_text_paragraph);
    }
    selection_text_spans(document, selection)
        .into_iter()
        .any(|(index, _, _)| is_text_paragraph(&document.blocks[index]))
}

fn selected_character_styles(
    document: &WriterDocument,
    selection: DocumentSelection,
) -> Vec<CharacterStyle> {
    let spans = selection_text_spans(document, selection.clone());
    if spans.is_empty() {
        let focus = selection.focus;
        let table_styles = crate::table_actions::table_format_actions::selected_character_styles(
            document,
            selection.clone(),
        );
        if !table_styles.is_empty() {
            return table_styles;
        }
        return caret_style(document, selection)
            .filter(|_| block_is_text(document, block_at_offset(document, focus)))
            .into_iter()
            .collect();
    }

    let mut styles = Vec::new();
    for (block_index, start, end) in spans {
        let block = &document.blocks[block_index];
        if !is_text_paragraph(block) {
            continue;
        }
        let mut boundaries = BTreeSet::from([start, end]);
        for run in &block.runs {
            if run.start < end && run.end > start {
                boundaries.insert(run.start.max(start).min(end));
                boundaries.insert(run.end.max(start).min(end));
            }
        }
        let boundaries: Vec<usize> = boundaries.into_iter().collect();
        styles.extend(
            boundaries
                .windows(2)
                .filter(|pair| pair[0] < pair[1])
                .map(|pair| style_at(block, pair[0])),
        );
    }
    styles.extend(
        crate::table_actions::table_format_actions::selected_character_styles(document, selection),
    );
    styles
}

fn boolean_state(
    styles: &[CharacterStyle],
    value: impl Fn(&CharacterStyle) -> bool,
) -> (bool, bool) {
    let Some(first) = styles.first().map(&value) else {
        return (false, false);
    };
    let any = styles.iter().any(&value);
    let all = styles.iter().all(value);
    (all && first, any && !all)
}

fn same_color(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        (None, None) => true,
        _ => false,
    }
}

pub(crate) fn is_text_paragraph(block: &RichBlock) -> bool {
    block.kind != loom_writer_core::TABLE_BLOCK_KIND && !block.is_image() && !block.is_chart()
}

fn block_is_text(document: &WriterDocument, index: Option<usize>) -> bool {
    index
        .and_then(|index| document.blocks.get(index))
        .is_some_and(is_text_paragraph)
}

pub(crate) fn set_selection_superscript(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    enabled: bool,
) {
    mutate_text_selection(document, selection, |style| {
        style.superscript = enabled;
        if enabled {
            style.subscript = false;
        }
    });
}

pub(crate) fn set_selection_subscript(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    enabled: bool,
) {
    mutate_text_selection(document, selection, |style| {
        style.subscript = enabled;
        if enabled {
            style.superscript = false;
        }
    });
}

pub(crate) fn set_selection_foreground_color(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    color: &str,
) {
    mutate_text_selection(document, selection, |style| {
        style.color = Some(color.to_owned());
    });
}

pub(crate) fn set_selection_strikethrough(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    enabled: bool,
) {
    mutate_text_selection(document, selection, |style| {
        style.strikethrough = enabled;
    });
}

fn mutate_text_selection(
    document: &mut WriterDocument,
    selection: DocumentSelection,
    mut operation: impl FnMut(&mut CharacterStyle),
) {
    for (block_index, start, end) in selection_text_spans(document, selection) {
        let block = &mut document.blocks[block_index];
        if is_text_paragraph(block) {
            mutate_character_range(block, start, end, &mut operation);
        }
    }
}
