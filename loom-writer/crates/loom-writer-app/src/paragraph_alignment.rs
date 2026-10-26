use loom_command::CommandSpec;
use loom_text::{Alignment, ParagraphDirection};
use loom_writer_core::{selected_block_indices, TextSelection, WriterDocument};

/// Convert a resolved physical paragraph alignment to the renderer's stable
/// row value. Logical Start/End values must be resolved by the paragraph first.
pub(crate) fn writer_render_alignment(alignment: loom_text::Alignment) -> i32 {
    match alignment {
        loom_text::Alignment::Left | loom_text::Alignment::Start => 0,
        loom_text::Alignment::Center => 1,
        loom_text::Alignment::Right | loom_text::Alignment::End => 2,
        loom_text::Alignment::Justify => 3,
    }
}

/// Whether the current selection contains a text paragraph that can be
/// justified by the canonical shaped renderer. Objects are excluded; RTL text
/// uses the same shaped line geometry as LTR text.
pub(crate) fn selection_can_be_justified(
    document: &loom_writer_core::WriterDocument,
    selection: loom_writer_core::TextSelection,
) -> bool {
    loom_writer_core::selected_block_indices(document, selection)
        .into_iter()
        .any(|index| is_text_paragraph(&document.blocks[index]))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ParagraphFormatResult {
    pub(crate) applied_paragraphs: usize,
    pub(crate) skipped_unsupported_items: usize,
}

pub(crate) fn selection_can_change_direction(
    document: &WriterDocument,
    selection: TextSelection,
) -> bool {
    selected_block_indices(document, selection)
        .into_iter()
        .any(|index| is_text_paragraph(&document.blocks[index]))
}

pub(crate) fn direction_index(document: &WriterDocument, selection: TextSelection) -> i32 {
    let mut indices = selected_block_indices(document, selection)
        .into_iter()
        .filter(|index| is_text_paragraph(&document.blocks[*index]));
    let Some(first) = indices
        .next()
        .map(|index| direction_to_index(document.blocks[index].style.direction))
    else {
        return -1;
    };
    if indices.all(|index| direction_to_index(document.blocks[index].style.direction) == first) {
        first
    } else {
        -1
    }
}

pub(crate) fn alignment_indices(document: &WriterDocument, selection: TextSelection) -> (i32, i32) {
    let indices: Vec<usize> = selected_block_indices(document, selection)
        .into_iter()
        .filter(|index| is_text_paragraph(&document.blocks[*index]))
        .collect();
    let Some(first_index) = indices.first().copied() else {
        return (-1, -1);
    };
    let first = document.blocks[first_index].style.alignment;
    if indices
        .iter()
        .any(|index| document.blocks[*index].style.alignment != first)
    {
        return (-1, -1);
    }
    match first {
        Alignment::Left => (0, -1),
        Alignment::Center => (1, -1),
        Alignment::Right => (2, -1),
        Alignment::Justify => (3, -1),
        Alignment::Start => (-1, 0),
        Alignment::End => (-1, 1),
    }
}

pub(crate) fn set_selection_direction(
    document: &mut WriterDocument,
    selection: TextSelection,
    direction: ParagraphDirection,
) -> ParagraphFormatResult {
    apply_to_selected_paragraphs(document, selection, |block| {
        block.style.direction = direction;
    })
}

pub(crate) fn set_selection_logical_alignment(
    document: &mut WriterDocument,
    selection: TextSelection,
    index: i32,
) -> Option<(ParagraphFormatResult, Alignment)> {
    let alignment = match index {
        0 => Alignment::Start,
        1 => Alignment::End,
        _ => return None,
    };
    Some((
        apply_to_selected_paragraphs(document, selection, |block| {
            block.style.alignment = alignment;
        }),
        alignment,
    ))
}

fn apply_to_selected_paragraphs(
    document: &mut WriterDocument,
    selection: TextSelection,
    mut apply: impl FnMut(&mut loom_writer_core::RichBlock),
) -> ParagraphFormatResult {
    let mut result = ParagraphFormatResult::default();
    for index in selected_block_indices(document, selection) {
        let block = &mut document.blocks[index];
        if is_text_paragraph(block) {
            apply(block);
            result.applied_paragraphs += 1;
        } else {
            result.skipped_unsupported_items += 1;
        }
    }
    result
}

fn direction_to_index(direction: ParagraphDirection) -> i32 {
    match direction {
        ParagraphDirection::Auto => 0,
        ParagraphDirection::LeftToRight => 1,
        ParagraphDirection::RightToLeft => 2,
    }
}

pub(crate) fn direction_name(index: i32) -> &'static str {
    match index {
        1 => "Left to right",
        2 => "Right to left",
        _ => "Automatic",
    }
}

pub(crate) fn format_result_announcement(verb: &str, result: ParagraphFormatResult) -> String {
    if result.applied_paragraphs == 0 {
        return if result.skipped_unsupported_items == 0 {
            format!("{verb} was not applied because no supported paragraph was selected")
        } else {
            format!(
                "{verb} was not applied; skipped {}",
                count_label(
                    result.skipped_unsupported_items,
                    "unsupported item",
                    "unsupported items"
                )
            )
        };
    }
    let applied = count_label(result.applied_paragraphs, "paragraph", "paragraphs");
    if result.skipped_unsupported_items == 0 {
        return format!("{verb} applied to {applied}");
    }
    format!(
        "{verb} applied to {applied}; skipped {}",
        count_label(
            result.skipped_unsupported_items,
            "unsupported item",
            "unsupported items"
        )
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct JustificationResult {
    pub(crate) applied_paragraphs: usize,
    pub(crate) skipped_unsupported_items: usize,
}

/// Apply Justify to selected text paragraphs. Existing object alignment is
/// kept; the result lets the UI report exactly which selected paragraphs changed.
pub(crate) fn justify_selected_paragraphs(
    document: &mut loom_writer_core::WriterDocument,
    selection: loom_writer_core::TextSelection,
) -> JustificationResult {
    let mut result = JustificationResult::default();
    for index in loom_writer_core::selected_block_indices(document, selection) {
        let block = &mut document.blocks[index];
        if !is_text_paragraph(block) {
            result.skipped_unsupported_items += 1;
            continue;
        }
        block.style.alignment = loom_text::Alignment::Justify;
        result.applied_paragraphs += 1;
    }
    result
}

pub(crate) fn justification_announcement(result: JustificationResult) -> String {
    if result.applied_paragraphs == 0 {
        if result.skipped_unsupported_items == 0 {
            return "Justify was not applied because no supported paragraph was selected".into();
        }
        return format!(
            "Justify was not applied; skipped {}",
            count_label(
                result.skipped_unsupported_items,
                "unsupported item",
                "unsupported items"
            )
        );
    }

    let applied = count_label(result.applied_paragraphs, "paragraph", "paragraphs");
    if result.skipped_unsupported_items == 0 {
        return format!("Justify alignment applied to {applied}");
    }

    format!(
        "Justify alignment applied to {applied}; skipped {}",
        count_label(
            result.skipped_unsupported_items,
            "unsupported item",
            "unsupported items"
        )
    )
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

fn is_text_paragraph(block: &loom_writer_core::RichBlock) -> bool {
    block.kind != loom_writer_core::TABLE_BLOCK_KIND && !block.is_image() && !block.is_chart()
}

pub(crate) fn command_specs() -> Vec<CommandSpec> {
    vec![
        alignment(
            "writer.align.left",
            "Align Left",
            "Align selected paragraphs to the left",
            80,
        ),
        alignment(
            "writer.align.center",
            "Align Center",
            "Center-align selected paragraphs",
            90,
        ),
        alignment(
            "writer.align.right",
            "Align Right",
            "Align selected paragraphs to the right",
            100,
        ),
        alignment(
            "writer.align.justify",
            "Justify",
            "Justify wrapped lines in selected paragraphs",
            110,
        ),
        alignment(
            "writer.align.left-all",
            "Align All Left",
            "Align selected paragraphs to the left",
            81,
        ),
        alignment(
            "writer.align.center-all",
            "Align All Center",
            "Center-align selected paragraphs",
            91,
        ),
        alignment(
            "writer.align.right-all",
            "Align All Right",
            "Align selected paragraphs to the right",
            101,
        ),
        alignment(
            "writer.align.justify-all",
            "Align All Justify",
            "Justify wrapped lines in selected paragraphs",
            111,
        ),
        alignment(
            "writer.align.start",
            "Align to Start",
            "Align selected paragraphs to the start of their direction",
            85,
        ),
        alignment(
            "writer.align.end",
            "Align to End",
            "Align selected paragraphs to the end of their direction",
            105,
        ),
    ]
}

fn alignment(
    id: &'static str,
    label: &'static str,
    description: &'static str,
    order: u32,
) -> CommandSpec {
    CommandSpec::new(id, label)
        .with_undo_label(label)
        .with_description(description)
        .with_category("format")
        .with_order(order)
}

#[cfg(test)]
#[path = "paragraph_alignment_tests.rs"]
mod paragraph_alignment_tests;
