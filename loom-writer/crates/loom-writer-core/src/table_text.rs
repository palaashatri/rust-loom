//! Conversion from typed table-cell runs to the rich text used by Writer's
//! screen layout and PDF export.

use loom_objects::table::{HorizontalAlign, Table, TextRun};
use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};

use crate::RichBlock;

/// The base point size used to shape and measure Writer table-cell text.
pub const TABLE_CELL_BASE_FONT_SIZE_PT: f32 = 12.0;

/// Build the canonical text block for one table cell.
///
/// `bold` and `base_color` are table-style values applied to runs that do not
/// override them. The returned run ranges are byte offsets into the original
/// cell text, preserving leading/trailing whitespace and source positions for
/// screen and PDF geometry.
pub fn table_cell_rich_block(
    block_id: u64,
    table: &Table,
    row: usize,
    column: usize,
    base_size_pt: f32,
    bold: bool,
    base_color: Option<&str>,
) -> Option<RichBlock> {
    let cell = table.cell(row, column)?;
    let alignment = cell_alignment(table, row, column);
    text_runs_rich_block(
        block_id,
        &cell.content.runs,
        base_size_pt,
        alignment,
        bold,
        base_color,
    )
}

/// Convert typed cell runs to one canonical rich text block.
pub fn text_runs_rich_block(
    block_id: u64,
    runs: &[TextRun],
    base_size_pt: f32,
    alignment: Alignment,
    bold: bool,
    base_color: Option<&str>,
) -> Option<RichBlock> {
    let full: String = runs.iter().map(|run| run.text.as_str()).collect();
    if full.is_empty() {
        return None;
    }
    let mut block = RichBlock::new(block_id, "paragraph", &full);
    block.style.alignment = alignment;
    let mut offset = 0;
    for run in runs {
        let (start, end) = (offset, offset + run.text.len());
        offset = end;
        let has_style = bold
            || run.bold
            || run.italic
            || run.underline
            || run.strikethrough
            || run.superscript
            || run.subscript
            || run.font_family.is_some()
            || run.font_size_pt.is_some()
            || run.color.is_some()
            || base_color.is_some();
        if !has_style || start == end {
            continue;
        }
        block.runs.push(StyleRun {
            start,
            end,
            style: CharacterStyle {
                weight: if bold || run.bold {
                    FontWeight::Bold
                } else {
                    FontWeight::Regular
                },
                italic: run.italic,
                underline: run.underline,
                strikethrough: run.strikethrough,
                superscript: run.superscript,
                subscript: run.subscript,
                font_family: run.font_family.clone().unwrap_or_default(),
                font_size: run.font_size_pt.unwrap_or(base_size_pt),
                explicit_size: run.font_size_pt.is_some(),
                color: run.color.clone().or_else(|| base_color.map(str::to_owned)),
                ..Default::default()
            },
        });
    }
    Some(block)
}

fn cell_alignment(table: &Table, row: usize, column: usize) -> Alignment {
    let explicit = table
        .cell(row, column)
        .and_then(|cell| cell.style_override.as_ref())
        .and_then(|style| style.horizontal_align);
    match explicit.unwrap_or_default() {
        HorizontalAlign::Left => Alignment::Left,
        HorizontalAlign::Center => Alignment::Center,
        HorizontalAlign::Right => Alignment::Right,
        HorizontalAlign::Justify => Alignment::Justify,
    }
}

#[cfg(test)]
mod tests {
    use loom_objects::table::{CellStyleOverride, HorizontalAlign, TextRun};

    use super::table_cell_rich_block;

    #[test]
    fn cell_conversion_preserves_whitespace_byte_ranges_alignment_and_inherited_style() {
        let mut table = loom_objects::table::Table::new(1, 1);
        table.cell_mut(0, 0).expect("cell").style_override = Some(CellStyleOverride {
            horizontal_align: Some(HorizontalAlign::Center),
            ..Default::default()
        });
        table.set_cell_rich_text(
            0,
            0,
            vec![
                TextRun {
                    text: "  alpha".to_owned(),
                    bold: true,
                    ..TextRun::plain("")
                },
                TextRun {
                    text: " beta  ".to_owned(),
                    italic: true,
                    color: Some("#123456".to_owned()),
                    ..TextRun::plain("")
                },
            ],
        );

        let block = table_cell_rich_block(7, &table, 0, 0, 12.0, false, Some("#abcdef"))
            .expect("cell has visible text");

        assert_eq!(block.id, 7);
        assert_eq!(block.text.as_str(), "  alpha beta  ");
        assert_eq!(block.style.alignment, loom_text::Alignment::Center);
        assert_eq!(block.runs.len(), 2);
        assert_eq!((block.runs[0].start, block.runs[0].end), (0, 7));
        assert!(block.runs[0].style.weight.numeric() >= loom_text::FontWeight::Bold.numeric());
        assert_eq!(block.runs[0].style.color.as_deref(), Some("#abcdef"));
        assert_eq!((block.runs[1].start, block.runs[1].end), (7, 14));
        assert!(block.runs[1].style.italic);
        assert_eq!(block.runs[1].style.color.as_deref(), Some("#123456"));
    }

    #[test]
    fn table_script_and_strike_only_runs_are_preserved_as_character_styles() {
        let runs = [
            TextRun {
                text: "strike".to_owned(),
                strikethrough: true,
                ..TextRun::plain("")
            },
            TextRun {
                text: "super".to_owned(),
                superscript: true,
                ..TextRun::plain("")
            },
            TextRun {
                text: "sub".to_owned(),
                subscript: true,
                ..TextRun::plain("")
            },
        ];
        let block =
            super::text_runs_rich_block(8, &runs, 12.0, loom_text::Alignment::Left, false, None)
                .expect("styled cell text is visible");

        assert_eq!(block.text.as_str(), "strikesupersub");
        assert_eq!(block.runs.len(), 3);
        assert!(block.runs[0].style.strikethrough);
        assert!(block.runs[1].style.superscript);
        assert!(block.runs[2].style.subscript);
    }

    #[test]
    fn horizontal_alignment_override_belongs_to_its_cell() {
        let mut table = loom_objects::table::Table::new(2, 1);
        table.cell_mut(0, 0).expect("first cell").style_override = Some(CellStyleOverride {
            horizontal_align: Some(HorizontalAlign::Left),
            ..Default::default()
        });
        table.cell_mut(1, 0).expect("second cell").style_override = Some(CellStyleOverride {
            horizontal_align: Some(HorizontalAlign::Right),
            ..Default::default()
        });
        table.set_cell_text(0, 0, "same");
        table.set_cell_text(1, 0, "same");

        let first = super::table_cell_rich_block(1, &table, 0, 0, 12.0, false, None)
            .expect("first cell has text");
        let second = super::table_cell_rich_block(2, &table, 1, 0, 12.0, false, None)
            .expect("second cell has text");

        assert_eq!(first.style.alignment, loom_text::Alignment::Left);
        assert_eq!(second.style.alignment, loom_text::Alignment::Right);
    }
}
