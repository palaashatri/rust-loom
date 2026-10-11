//! Shared office object models: tables, charts, images, shapes and styles.
//!
//! Provides the cross-application object architecture for Loom Office
//! (Writer, Sheets, and Present) without Slint or UI framework dependencies.

pub mod chart;
pub mod chart_ooxml;
pub mod image;
pub mod markdown;
pub mod ooxml;
pub mod shape;
pub mod styles;
pub mod table;

pub use chart::{Chart, ChartAxis, ChartDataSnapshot, ChartSeries, ChartStyle, ChartType};
pub use chart_ooxml::{c_chart_xml_to_chart, chart_to_c_chart_xml};
pub use image::{Image, ImageFormat};
pub use markdown::{markdown_to_table, table_to_markdown};
pub use ooxml::{table_to_docx_wtbl, table_to_pptx_atbl};
pub use shape::{Shape, ShapeType};
pub use styles::{
    CellFormat, ResolvedCellFormat, TableStyle, TableStyleCatalog, TableStyleCategory,
    TableStyleOptions,
};
pub use table::{
    Border, BorderStyle, CellBorders, CellContent, CellPadding, CellStyleOverride, HorizontalAlign,
    Table, TableCell, TableColumn, TableRow, TextRun, VerticalAlign, DEFAULT_COLUMN_WIDTH_PT,
    DEFAULT_ROW_HEIGHT_PT, TABLE_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_root_round_trip() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Test");
        let json = table.to_json().unwrap();
        let loaded = Table::from_json(&json).unwrap();
        assert_eq!(table, loaded);

        let md = table_to_markdown(&table);
        let from_md = markdown_to_table(&md);
        assert_eq!(from_md.cell_text(0, 0).as_deref(), Some("Test"));
    }
}
