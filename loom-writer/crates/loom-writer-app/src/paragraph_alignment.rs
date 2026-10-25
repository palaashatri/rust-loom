use loom_command::CommandSpec;

/// Whether the current selection contains a paragraph the M2 renderer can
/// actually justify. Object blocks and deferred-bidi paragraphs are excluded.
pub(crate) fn selection_can_be_justified(
    document: &loom_writer_core::WriterDocument,
    selection: loom_writer_core::TextSelection,
) -> bool {
    loom_writer_core::selected_block_indices(document, selection)
        .into_iter()
        .any(|index| block_can_be_justified(&document.blocks[index]))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct JustificationResult {
    pub(crate) applied_paragraphs: usize,
    pub(crate) skipped_unsupported_items: usize,
}

/// Apply Justify only to selected text paragraphs for which M2 has real
/// geometry. Existing object alignment and unsupported bidi styles are kept;
/// the result lets the UI report exactly which selected paragraphs changed.
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
        if block.justification_deferred_for_bidi() {
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

fn block_can_be_justified(block: &loom_writer_core::RichBlock) -> bool {
    is_text_paragraph(block) && !block.justification_deferred_for_bidi()
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
