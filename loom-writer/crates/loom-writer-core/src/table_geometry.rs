//! Shared shaped measurements for typed table rows.

use std::cell::RefCell;
use std::fmt;
use std::ops::Range;

use loom_objects::{table::Table, TableStyleCatalog, TableStyleOptions};

use crate::layout::{shape_wrapped_block_lines_with_inline_advances, InlineAdvance};
use crate::{editor_column_widths, line_pitch, table_cell_rich_block, WriterDocument};

/// Horizontal inset applied on each side of table-cell text, in points.
pub const TABLE_CELL_HORIZONTAL_INSET_PT: f32 = 6.0;
/// Vertical inset applied above and below table-cell text, in points.
pub const TABLE_CELL_VERTICAL_INSET_PT: f32 = 4.0;
/// Minimum height of a table row, in points.
pub const TABLE_ROW_MIN_HEIGHT_PT: f32 = 24.0;
/// Baseline line pitch used to place wrapped cell lines.
pub const TABLE_CELL_LINE_PITCH_EM: f32 = 1.25;

/// Why a typed table could not receive authoritative shaped row geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableGeometryError {
    InvalidAvailableWidth,
    InvalidFontSize,
    CouldNotShapeText,
    InvalidLineMetrics,
    CouldNotShapeCell { row: usize, column: usize },
    InvalidCellMetrics { row: usize, column: usize },
}

impl fmt::Display for TableGeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAvailableWidth => f.write_str("table width is invalid"),
            Self::InvalidFontSize => f.write_str("table font size is invalid"),
            Self::CouldNotShapeText => f.write_str("could not shape nonempty table-cell text"),
            Self::InvalidLineMetrics => f.write_str("table-cell line metrics are invalid"),
            Self::CouldNotShapeCell { row, column } => {
                write!(f, "could not shape nonempty table cell {row},{column}")
            }
            Self::InvalidCellMetrics { row, column } => {
                write!(f, "table cell {row},{column} produced invalid geometry")
            }
        }
    }
}

impl std::error::Error for TableGeometryError {}

/// One wrapped line of a table cell, positioned using the Writer screen pitch.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCellLineGeometry {
    source_range: Range<usize>,
    shaped: Option<crate::WriterShapedVisualLine>,
    pitch_pt: f32,
}

impl TableCellLineGeometry {
    /// UTF-8 byte range in the original cell text.
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    /// Shaped glyph/caret geometry, absent only for an empty visual line.
    pub fn shaped(&self) -> Option<&crate::WriterShapedVisualLine> {
        self.shaped.as_ref()
    }

    /// The reserved vertical pitch of this line, in points.
    pub fn pitch_pt(&self) -> f32 {
        self.pitch_pt
    }
}

/// Exact wrapped geometry for one typed table cell.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCellGeometry {
    lines: Vec<TableCellLineGeometry>,
    content_height_pt: f32,
}

impl TableCellGeometry {
    /// Wrapped line geometry in visual top-to-bottom order.
    pub fn lines(&self) -> &[TableCellLineGeometry] {
        &self.lines
    }

    /// Sum of line pitches, in points.
    pub fn content_height_pt(&self) -> f32 {
        self.content_height_pt
    }
}

pub fn shape_table_cell_geometry(
    block: &crate::RichBlock,
    base_size_pt: f32,
    available_width_pt: f32,
) -> Result<TableCellGeometry, TableGeometryError> {
    shape_table_cell_geometry_with_advances(block, base_size_pt, available_width_pt, &[])
}

/// Shape one typed table cell with the reserved advances for its anchored footnotes.
pub fn shape_table_cell_geometry_with_footnotes(
    doc: &WriterDocument,
    table_block_id: u64,
    table: &Table,
    row: usize,
    column: usize,
    block: &crate::RichBlock,
    base_size_pt: f32,
    available_width_pt: f32,
) -> Result<TableCellGeometry, TableGeometryError> {
    let inline_advances = crate::layout::inline_advances_for_table_cell(
        doc,
        table_block_id,
        table,
        row,
        column,
        base_size_pt,
    );
    shape_table_cell_geometry_with_advances(
        block,
        base_size_pt,
        available_width_pt,
        &inline_advances,
    )
}

fn shape_table_cell_geometry_with_advances(
    block: &crate::RichBlock,
    base_size_pt: f32,
    available_width_pt: f32,
    inline_advances: &[InlineAdvance],
) -> Result<TableCellGeometry, TableGeometryError> {
    if !available_width_pt.is_finite() || available_width_pt <= 0.0 {
        return Err(TableGeometryError::InvalidAvailableWidth);
    }
    if !base_size_pt.is_finite() || base_size_pt <= 0.0 {
        return Err(TableGeometryError::InvalidFontSize);
    }
    let base_pitch = line_pitch(&block.runs, base_size_pt) * TABLE_CELL_LINE_PITCH_EM;
    if !base_pitch.is_finite() || base_pitch <= 0.0 {
        return Err(TableGeometryError::InvalidLineMetrics);
    }
    let shaped_lines = shape_wrapped_block_lines_with_inline_advances(
        block,
        base_size_pt,
        available_width_pt,
        inline_advances,
    )
    .ok_or(TableGeometryError::CouldNotShapeText)?;
    table_cell_geometry_from_lines(block, base_pitch, shaped_lines)
}

fn table_cell_geometry_from_lines(
    block: &crate::RichBlock,
    base_pitch: f32,
    shaped_lines: Vec<(Range<usize>, Option<crate::WriterShapedVisualLine>)>,
) -> Result<TableCellGeometry, TableGeometryError> {
    let mut lines = Vec::with_capacity(shaped_lines.len());
    let mut content_height_pt = 0.0;
    for (source_range, shaped) in shaped_lines {
        let pitch = match shaped.as_ref() {
            Some(line) => base_pitch.max(line.ascent_pt() + line.descent_pt() + line.line_gap_pt()),
            None if source_range.is_empty() => base_pitch,
            None => return Err(TableGeometryError::CouldNotShapeText),
        };
        if !pitch.is_finite() || pitch <= 0.0 {
            return Err(TableGeometryError::InvalidLineMetrics);
        }
        content_height_pt += pitch;
        lines.push(TableCellLineGeometry {
            source_range,
            shaped,
            pitch_pt: pitch,
        });
    }
    if lines.is_empty() {
        content_height_pt = base_pitch;
        lines.push(TableCellLineGeometry {
            source_range: 0..block.text.as_str().len(),
            shaped: None,
            pitch_pt: base_pitch,
        });
    }
    if !content_height_pt.is_finite() || content_height_pt <= 0.0 {
        return Err(TableGeometryError::InvalidLineMetrics);
    }
    Ok(TableCellGeometry {
        lines,
        content_height_pt,
    })
}

/// The authoritative per-row heights of a typed table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRowGeometry {
    row_heights_pt: Vec<f32>,
}

impl TableRowGeometry {
    /// One height per table row, in points.
    pub fn row_heights_pt(&self) -> &[f32] {
        &self.row_heights_pt
    }

    /// The height of `row`, in points.
    pub fn row_height_pt(&self, row: usize) -> Option<f32> {
        self.row_heights_pt.get(row).copied()
    }

    /// The distance from the table's top edge to `row`'s top edge, in points.
    pub fn row_top_pt(&self, row: usize) -> Option<f32> {
        (row < self.row_heights_pt.len()).then(|| self.row_heights_pt[..row].iter().sum())
    }

    /// The complete table height, in points.
    pub fn total_height_pt(&self) -> f32 {
        self.row_heights_pt.iter().sum()
    }
}

struct CacheEntry {
    table: Table,
    width_bits: u32,
    size_bits: u32,
    font_epoch: u64,
    result: Result<TableRowGeometry, TableGeometryError>,
}

thread_local! {
    /// One bounded entry prevents pointer, projection, and flow queries from
    /// reshaping every cell of an unchanged large table.
    static CACHE: RefCell<Option<CacheEntry>> = const { RefCell::new(None) };
}

/// Measure typed table rows with the same wrapping and line pitch used by the
/// Writer screen renderer. The single-entry cache compares the complete typed
/// table, so run-only formatting changes invalidate row geometry.
pub fn measure_table_row_geometry(
    table: &Table,
    available_width_pt: f32,
    base_size_pt: f32,
) -> Result<TableRowGeometry, TableGeometryError> {
    if !available_width_pt.is_finite() || available_width_pt <= 0.0 {
        return Err(TableGeometryError::InvalidAvailableWidth);
    }
    if !base_size_pt.is_finite() || base_size_pt <= 0.0 {
        return Err(TableGeometryError::InvalidFontSize);
    }
    let (width_bits, size_bits, font_epoch) = (
        available_width_pt.to_bits(),
        base_size_pt.to_bits(),
        crate::fonts::font_epoch(),
    );
    if let Some(geometry) = CACHE.with(|cache| {
        let cache = cache.borrow();
        cache.as_ref().and_then(|entry| {
            (entry.table == *table
                && entry.width_bits == width_bits
                && entry.size_bits == size_bits
                && entry.font_epoch == font_epoch)
                .then(|| entry.result.clone())
        })
    }) {
        return geometry;
    }

    let result = measure_uncached(table, available_width_pt, base_size_pt);
    CACHE.with(|cache| {
        *cache.borrow_mut() = Some(CacheEntry {
            table: table.clone(),
            width_bits,
            size_bits,
            font_epoch,
            result: result.clone(),
        });
    });
    result
}

/// Measure table rows with footnote-marker widths included in cell wrapping.
/// This document-aware path is intentionally uncached because note anchors can
/// change independently of the typed table.
pub fn measure_table_row_geometry_with_footnotes(
    doc: &WriterDocument,
    table_block_id: u64,
    table: &Table,
    available_width_pt: f32,
    base_size_pt: f32,
) -> Result<TableRowGeometry, TableGeometryError> {
    if !available_width_pt.is_finite() || available_width_pt <= 0.0 {
        return Err(TableGeometryError::InvalidAvailableWidth);
    }
    if !base_size_pt.is_finite() || base_size_pt <= 0.0 {
        return Err(TableGeometryError::InvalidFontSize);
    }
    measure_uncached_with_cell_shape(
        table,
        available_width_pt,
        base_size_pt,
        |row, column, block, inner_width| {
            shape_table_cell_geometry_with_footnotes(
                doc,
                table_block_id,
                table,
                row,
                column,
                block,
                base_size_pt,
                inner_width,
            )
        },
    )
}

fn measure_uncached(
    table: &Table,
    available_width_pt: f32,
    base_size_pt: f32,
) -> Result<TableRowGeometry, TableGeometryError> {
    measure_uncached_with_cell_shape(
        table,
        available_width_pt,
        base_size_pt,
        |_, _, block, inner_width| shape_table_cell_geometry(block, base_size_pt, inner_width),
    )
}

fn measure_uncached_with_cell_shape(
    table: &Table,
    available_width_pt: f32,
    base_size_pt: f32,
    mut shape_cell: impl FnMut(
        usize,
        usize,
        &crate::RichBlock,
        f32,
    ) -> Result<TableCellGeometry, TableGeometryError>,
) -> Result<TableRowGeometry, TableGeometryError> {
    let columns = table.col_count().max(1);
    let widths = editor_column_widths(table, available_width_pt);
    let style =
        TableStyleCatalog::get(&table.style_id).unwrap_or_else(TableStyleCatalog::default_style);
    let options = TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        first_column: table.first_column,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
    };
    let baseline_pitch = base_size_pt * TABLE_CELL_LINE_PITCH_EM;
    if !baseline_pitch.is_finite() || baseline_pitch <= 0.0 {
        return Err(TableGeometryError::InvalidFontSize);
    }

    let mut row_heights_pt = Vec::with_capacity(table.row_count());
    assert_eq!(widths.len(), columns, "table column widths are incomplete");
    for row in 0..table.row_count() {
        let mut row_content_height = baseline_pitch;
        for (column, width) in widths.iter().enumerate().take(columns) {
            let format = style.resolve_cell(
                row,
                column,
                table.row_count(),
                columns,
                &options,
                table.cell(row, column),
            );
            let Some(block) =
                table_cell_rich_block(0, table, row, column, base_size_pt, format.bold, None)
            else {
                continue;
            };
            let inner_width = (*width - 2.0 * TABLE_CELL_HORIZONTAL_INSET_PT).max(1.0);
            let cell_geometry =
                shape_cell(row, column, &block, inner_width).map_err(|error| match error {
                    TableGeometryError::CouldNotShapeText => {
                        TableGeometryError::CouldNotShapeCell { row, column }
                    }
                    TableGeometryError::InvalidLineMetrics => {
                        TableGeometryError::InvalidCellMetrics { row, column }
                    }
                    other => other,
                })?;
            row_content_height = row_content_height.max(cell_geometry.content_height_pt());
        }
        let row_height =
            (row_content_height + 2.0 * TABLE_CELL_VERTICAL_INSET_PT).max(TABLE_ROW_MIN_HEIGHT_PT);
        if !row_height.is_finite() || row_height <= 0.0 {
            return Err(TableGeometryError::InvalidCellMetrics { row, column: 0 });
        }
        row_heights_pt.push(row_height);
    }
    Ok(TableRowGeometry { row_heights_pt })
}

#[cfg(test)]
mod tests {
    use loom_objects::table::TextRun;

    use super::{measure_table_row_geometry, TableGeometryError};
    use crate::TABLE_CELL_BASE_FONT_SIZE_PT;

    #[test]
    fn typed_wrapping_grows_only_the_row_that_needs_more_height() {
        let _serial = crate::test_fonts::serial();
        let mut table = loom_objects::Table::new(2, 1);
        table.header_row = false;
        table.set_cell_rich_text(
            0,
            0,
            vec![TextRun {
                text: "word ".repeat(12),
                font_size_pt: Some(24.0),
                ..TextRun::plain("")
            }],
        );
        table.set_cell_text(1, 0, "short");
        let geometry = measure_table_row_geometry(&table, 300.0, TABLE_CELL_BASE_FONT_SIZE_PT)
            .expect("bundled Writer fonts shape typed cell runs");
        assert!(geometry.row_height_pt(0).unwrap() > geometry.row_height_pt(1).unwrap() + 20.0);
        assert!(geometry.total_height_pt() > geometry.row_height_pt(0).unwrap());
        assert_eq!(geometry.row_top_pt(1), geometry.row_height_pt(0));
    }

    #[test]
    fn invalid_width_and_nonempty_shape_failure_are_reported() {
        let table = loom_objects::Table::new(1, 1);
        assert_eq!(
            measure_table_row_geometry(&table, f32::NAN, TABLE_CELL_BASE_FONT_SIZE_PT).unwrap_err(),
            TableGeometryError::InvalidAvailableWidth
        );
    }
}
