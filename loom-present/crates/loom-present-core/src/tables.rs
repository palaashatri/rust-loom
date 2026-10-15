//! Slide table object support, in-slide cell navigation, and PPTX DrawingML table interop.

use crate::{ElementType, SlideElement, SLIDE_HEIGHT, SLIDE_WIDTH};
use loom_objects::table::{CellContent, Table};

const EMU_PER_UNIT: f64 = 12_192.0;

/// Cardinal direction for navigating between adjacent table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Generates a default slide table with `rows` and `cols` styled with Medium 1 theme.
pub fn default_slide_table(rows: usize, cols: usize) -> Table {
    let r_count = rows.max(1);
    let c_count = cols.max(1);
    let mut table = Table::new(r_count, c_count);
    table.style_id = "Medium 1".to_string();
    table.header_row = true;
    table.banded_rows = true;

    for col in &mut table.columns {
        col.width_pt = 100.0;
    }
    for c in 0..c_count {
        if let Some(cell) = table.cell_mut(0, c) {
            cell.content = CellContent::plain(format!("Header {}", c + 1));
        }
    }

    for r in 1..r_count {
        for c in 0..c_count {
            if let Some(cell) = table.cell_mut(r, c) {
                cell.content = CellContent::plain(format!("R{}C{}", r + 1, c + 1));
            }
        }
    }

    table
}

impl SlideElement {
    /// Constructs a new table element positioned on the slide canvas.
    pub fn new_table(
        id: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        table: Table,
    ) -> Self {
        let content = serde_json::to_string(&table).unwrap_or_default();
        Self {
            id: id.into(),
            element_type: ElementType::Table,
            content,
            x,
            y,
            width,
            height,
            rotation_deg: 0.0,
            action: None,
        }
    }

    /// Deserializes the table object if this element is an [`ElementType::Table`].
    pub fn table(&self) -> Option<Table> {
        if self.element_type != ElementType::Table {
            return None;
        }
        serde_json::from_str(&self.content).ok()
    }

    /// Updates the table model stored in this element.
    pub fn set_table(&mut self, table: &Table) {
        if self.element_type == ElementType::Table {
            if let Ok(json) = serde_json::to_string(table) {
                self.content = json;
            }
        }
    }

    /// Returns the cell `(row, col)` coordinates at local element coordinates `(local_x, local_y)`.
    pub fn table_cell_at_local_coords(&self, local_x: f32, local_y: f32) -> Option<(usize, usize)> {
        let table = self.table()?;
        if local_x < 0.0 || local_y < 0.0 || local_x > self.width || local_y > self.height {
            return None;
        }
        let total_w: f32 = table.columns.iter().map(|c| c.width_pt).sum();
        let total_h: f32 = table.rows.iter().map(|r| r.height_pt.unwrap_or(20.0)).sum();
        let scale_x = if total_w > 0.0 {
            self.width / total_w
        } else {
            1.0
        };
        let scale_y = if total_h > 0.0 {
            self.height / total_h
        } else {
            1.0
        };

        let mut curr_x = 0.0;
        let mut target_c = None;
        for (c, col) in table.columns.iter().enumerate() {
            let col_w = col.width_pt * scale_x;
            if local_x >= curr_x && local_x <= curr_x + col_w {
                target_c = Some(c);
                break;
            }
            curr_x += col_w;
        }

        let mut curr_y = 0.0;
        let mut target_r = None;
        for (r, row) in table.rows.iter().enumerate() {
            let row_h = row.height_pt.unwrap_or(20.0) * scale_y;
            if local_y >= curr_y && local_y <= curr_y + row_h {
                target_r = Some(r);
                break;
            }
            curr_y += row_h;
        }

        match (target_r, target_c) {
            (Some(r), Some(c)) => Some((r, c)),
            _ => None,
        }
    }

    /// Updates the plain text content of a cell in this table.
    pub fn update_table_cell_text(&mut self, row: usize, col: usize, text: &str) -> bool {
        let Some(mut table) = self.table() else {
            return false;
        };
        if let Some(cell) = table.cell_mut(row, col) {
            cell.content = CellContent::plain(text);
            self.set_table(&table);
            true
        } else {
            false
        }
    }

    /// Tab navigation between table cells.
    ///
    /// When `shift` is true, navigates backwards. When false, navigates forwards.
    /// If navigating forward from the final cell in the table, automatically appends
    /// a new row, proportionally grows the element height, and places the cursor at
    /// `(new_row, 0)`.
    pub fn tab_navigate_table(
        &mut self,
        current_row: usize,
        current_col: usize,
        shift: bool,
    ) -> (usize, usize) {
        let Some(mut table) = self.table() else {
            return (current_row, current_col);
        };
        let rows = table.row_count();
        let cols = table.col_count();
        if rows == 0 || cols == 0 {
            return (0, 0);
        }

        if shift {
            if current_col > 0 {
                (current_row, current_col - 1)
            } else if current_row > 0 {
                (current_row - 1, cols - 1)
            } else {
                (0, 0)
            }
        } else if current_col + 1 < cols {
            (current_row, current_col + 1)
        } else if current_row + 1 < rows {
            (current_row + 1, 0)
        } else {
            // At last cell: append new row
            let new_row_idx = table.insert_row(rows);
            let old_rows = rows as f32;
            let new_rows = (rows + 1) as f32;
            self.height = (self.height * (new_rows / old_rows)).min(SLIDE_HEIGHT - self.y);
            self.set_table(&table);
            (new_row_idx, 0)
        }
    }

    /// Arrow navigation between adjacent cells.
    pub fn arrow_navigate_table(
        &self,
        row: usize,
        col: usize,
        direction: TableDirection,
    ) -> (usize, usize) {
        let Some(table) = self.table() else {
            return (row, col);
        };
        let rows = table.row_count();
        let cols = table.col_count();
        if rows == 0 || cols == 0 {
            return (0, 0);
        }
        match direction {
            TableDirection::Up => (if row > 0 { row - 1 } else { 0 }, col),
            TableDirection::Down => (if row + 1 < rows { row + 1 } else { rows - 1 }, col),
            TableDirection::Left => (row, if col > 0 { col - 1 } else { 0 }),
            TableDirection::Right => (row, if col + 1 < cols { col + 1 } else { cols - 1 }),
        }
    }

    /// Inserts a row above or below `at_row`.
    pub fn insert_table_row(&mut self, at_row: usize, below: bool) -> bool {
        let Some(mut table) = self.table() else {
            return false;
        };
        let old_rows = table.row_count() as f32;
        let insert_idx = if below { at_row + 1 } else { at_row };
        table.insert_row(insert_idx);
        let new_rows = table.row_count() as f32;
        if old_rows > 0.0 {
            self.height = (self.height * (new_rows / old_rows)).min(SLIDE_HEIGHT - self.y);
        }
        self.set_table(&table);
        true
    }

    /// Deletes the row at `at_row` (guards deleting the last remaining row).
    pub fn delete_table_row(&mut self, at_row: usize) -> bool {
        let Some(mut table) = self.table() else {
            return false;
        };
        if table.row_count() <= 1 {
            return false;
        }
        let old_rows = table.row_count() as f32;
        table.delete_row(at_row);
        let new_rows = table.row_count() as f32;
        self.height *= new_rows / old_rows;
        self.set_table(&table);
        true
    }

    /// Inserts a column to the left or right of `at_col`.
    pub fn insert_table_column(&mut self, at_col: usize, right: bool) -> bool {
        let Some(mut table) = self.table() else {
            return false;
        };
        let old_cols = table.col_count() as f32;
        let insert_idx = if right { at_col + 1 } else { at_col };
        table.insert_column(insert_idx, Some(80.0));
        let new_cols = table.col_count() as f32;
        if old_cols > 0.0 {
            self.width = (self.width * (new_cols / old_cols)).min(SLIDE_WIDTH - self.x);
        }
        self.set_table(&table);
        true
    }

    /// Deletes the column at `at_col` (guards deleting the last remaining column).
    pub fn delete_table_column(&mut self, at_col: usize) -> bool {
        let Some(mut table) = self.table() else {
            return false;
        };
        if table.col_count() <= 1 {
            return false;
        }
        let old_cols = table.col_count() as f32;
        table.delete_column(at_col);
        let new_cols = table.col_count() as f32;
        self.width *= new_cols / old_cols;
        self.set_table(&table);
        true
    }
}

/// Generates a PowerPoint `<p:graphicFrame>` enclosing an `<a:tbl>` DrawingML table.
pub fn table_graphic_frame(id: u32, element: &SlideElement, table: &Table) -> String {
    let off_x = (f64::from(element.x) * EMU_PER_UNIT).round() as i64;
    let off_y = (f64::from(element.y) * EMU_PER_UNIT).round() as i64;
    let ext_cx = (f64::from(element.width) * EMU_PER_UNIT).round() as i64;
    let ext_cy = (f64::from(element.height) * EMU_PER_UNIT).round() as i64;

    let atbl = loom_objects::table_to_pptx_atbl(table);

    format!(
        "<p:graphicFrame>\
<p:nvGraphicFramePr>\
<p:cNvPr id=\"{id}\" name=\"Table {id}\"/>\
<p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr>\
<p:nvPr/>\
</p:nvGraphicFramePr>\
<p:xfrm>\
<a:off x=\"{off_x}\" y=\"{off_y}\"/>\
<a:ext cx=\"{ext_cx}\" cy=\"{ext_cy}\"/>\
</p:xfrm>\
<a:graphic>\
<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">\
{atbl}\
</a:graphicData>\
</a:graphic>\
</p:graphicFrame>"
    )
}

/// Extracts DrawingML tables from a PPTX slide XML string into `SlideElement` objects.
pub fn extract_pptx_slide_tables(slide_xml: &str) -> Vec<SlideElement> {
    let mut elements = Vec::new();
    let mut cursor = 0usize;
    let mut table_id_counter = 1usize;

    while let Some(rel) = slide_xml[cursor..].find("<p:graphicFrame") {
        let frame_start = cursor + rel;
        let Some(frame_rel_end) = slide_xml[frame_start..].find("</p:graphicFrame>") else {
            break;
        };
        let frame_end = frame_start + frame_rel_end + "</p:graphicFrame>".len();
        let frame_xml = &slide_xml[frame_start..frame_end];

        if frame_xml.contains("http://schemas.openxmlformats.org/drawingml/2006/table")
            || frame_xml.contains("<a:tbl")
        {
            if let Some(element) = parse_single_graphic_frame_table(frame_xml, table_id_counter) {
                elements.push(element);
                table_id_counter += 1;
            }
        }

        cursor = frame_end;
    }

    elements
}

fn parse_single_graphic_frame_table(frame_xml: &str, counter: usize) -> Option<SlideElement> {
    // Extract x, y, width, height from <p:xfrm>
    let (x, y, width, height) = parse_frame_xfrm(frame_xml)?;

    // Extract table from <a:tbl>
    let tbl_start = frame_xml.find("<a:tbl")?;
    let tbl_end = frame_xml.find("</a:tbl>")? + "</a:tbl>".len();
    let tbl_xml = &frame_xml[tbl_start..tbl_end];

    let table = parse_pptx_atbl_content(tbl_xml)?;

    Some(SlideElement::new_table(
        format!("imported-table-{counter}"),
        x,
        y,
        width,
        height,
        table,
    ))
}

pub(crate) fn parse_frame_xfrm(frame_xml: &str) -> Option<(f32, f32, f32, f32)> {
    // Office writes `<a:extLst><a:ext uri=..>` inside the frame's name before its
    // position, so look only inside `<p:xfrm>`, and match `<a:ext ` exactly.
    let frame_xml = match (frame_xml.find("<p:xfrm"), frame_xml.find("</p:xfrm>")) {
        (Some(start), Some(end)) if end > start => &frame_xml[start..end],
        _ => frame_xml,
    };
    let off_tag = frame_xml.find("<a:off")?;
    let off_end = frame_xml[off_tag..].find('>')? + off_tag;
    let off_snippet = &frame_xml[off_tag..off_end];

    let x_emu = parse_attr_i64(off_snippet, "x")?;
    let y_emu = parse_attr_i64(off_snippet, "y")?;

    let ext_tag = frame_xml.find("<a:ext ")?;
    let ext_end = frame_xml[ext_tag..].find('>')? + ext_tag;
    let ext_snippet = &frame_xml[ext_tag..ext_end];

    let cx_emu = parse_attr_i64(ext_snippet, "cx")?;
    let cy_emu = parse_attr_i64(ext_snippet, "cy")?;

    let x = (x_emu as f64 / EMU_PER_UNIT) as f32;
    let y = (y_emu as f64 / EMU_PER_UNIT) as f32;
    let width = (cx_emu as f64 / EMU_PER_UNIT) as f32;
    let height = (cy_emu as f64 / EMU_PER_UNIT) as f32;

    Some((x, y, width.max(10.0), height.max(10.0)))
}

fn parse_attr_i64(snippet: &str, attr: &str) -> Option<i64> {
    let needle = format!("{attr}=\"");
    let start = snippet.find(&needle)? + needle.len();
    let end = snippet[start..].find('"')? + start;
    snippet[start..end].parse().ok()
}

fn parse_pptx_atbl_content(tbl_xml: &str) -> Option<Table> {
    // 1. Grid columns
    let mut col_widths_pt = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = tbl_xml[cursor..].find("<a:gridCol") {
        let tag_start = cursor + rel;
        let tag_end = tbl_xml[tag_start..].find('>')? + tag_start;
        let snippet = &tbl_xml[tag_start..tag_end];
        if let Some(w_emu) = parse_attr_i64(snippet, "w") {
            col_widths_pt.push((w_emu as f32) / 12700.0);
        } else {
            col_widths_pt.push(80.0);
        }
        cursor = tag_end;
    }
    if col_widths_pt.is_empty() {
        col_widths_pt.push(100.0);
    }
    let cols_count = col_widths_pt.len();

    // 2. Rows
    let mut rows_raw = Vec::new();
    cursor = 0usize;
    while let Some(rel) = tbl_xml[cursor..].find("<a:tr") {
        let row_start = cursor + rel;
        let Some(row_rel_end) = tbl_xml[row_start..].find("</a:tr>") else {
            break;
        };
        let row_end = row_start + row_rel_end + "</a:tr>".len();
        let row_xml = &tbl_xml[row_start..row_end];

        let mut cells_raw = Vec::new();
        let mut cell_cursor = 0usize;
        while let Some(c_rel) = row_xml[cell_cursor..].find("<a:tc") {
            let cell_start = cell_cursor + c_rel;
            let Some(cell_rel_end) = row_xml[cell_start..].find("</a:tc>") else {
                break;
            };
            let cell_end = cell_start + cell_rel_end + "</a:tc>".len();
            let cell_xml = &row_xml[cell_start..cell_end];

            let row_span = parse_attr_i64(cell_xml, "rowSpan").unwrap_or(1) as usize;
            let col_span = parse_attr_i64(cell_xml, "gridSpan").unwrap_or(1) as usize;
            let is_covered = cell_xml.contains("hMerge=\"1\"") || cell_xml.contains("vMerge=\"1\"");

            // Extract text from <a:t>
            let mut text = String::new();
            let mut t_cursor = 0usize;
            while let Some(t_rel) = cell_xml[t_cursor..].find("<a:t>") {
                let t_start = t_cursor + t_rel + "<a:t>".len();
                if let Some(t_end_rel) = cell_xml[t_start..].find("</a:t>") {
                    let raw_str = &cell_xml[t_start..t_start + t_end_rel];
                    text.push_str(raw_str);
                    t_cursor = t_start + t_end_rel + "</a:t>".len();
                } else {
                    break;
                }
            }

            cells_raw.push((text, row_span, col_span, is_covered));
            cell_cursor = cell_end;
        }

        rows_raw.push(cells_raw);
        cursor = row_end;
    }

    if rows_raw.is_empty() {
        return None;
    }

    let mut table = Table::new(rows_raw.len(), cols_count);
    for (c, &w) in col_widths_pt.iter().enumerate() {
        if c < table.columns.len() {
            table.columns[c].width_pt = w;
        }
    }

    for (r, row_cells) in rows_raw.into_iter().enumerate() {
        for (c, (text, row_span, col_span, is_covered)) in row_cells.into_iter().enumerate() {
            if c < cols_count {
                if let Some(cell) = table.cell_mut(r, c) {
                    cell.content = CellContent::plain(&text);
                    cell.row_span = row_span;
                    cell.col_span = col_span;
                    cell.is_covered = is_covered;
                }
            }
        }
    }

    Some(table)
}

/// Creates a new slide table element from a table model, centered or sized appropriately on the slide.
pub fn table_element_from_clipboard(table: Table, id: impl Into<String>) -> SlideElement {
    let cols = table.col_count().max(1);
    let rows = table.row_count().max(1);
    let width = ((cols as f32) * 120.0).clamp(200.0, SLIDE_WIDTH - 80.0);
    let height = ((rows as f32) * 30.0).clamp(100.0, SLIDE_HEIGHT - 120.0);
    let x = ((SLIDE_WIDTH - width) / 2.0).max(40.0);
    let y = ((SLIDE_HEIGHT - height) / 2.0).max(40.0);
    SlideElement::new_table(id, x, y, width, height, table)
}

/// Moves a newly inserted object down and right in 24-unit steps until no
/// existing object on the slide starts at the same corner, so inserting a
/// second table or chart does not hide the first exactly behind itself.
pub(crate) fn cascade_new_object(existing: &[SlideElement], element: &mut SlideElement) {
    const STEP: f32 = 24.0;
    for _ in 0..16 {
        let taken = existing
            .iter()
            .any(|e| (e.x - element.x).abs() < 1.0 && (e.y - element.y).abs() < 1.0);
        let fits = element.x + STEP + element.width <= SLIDE_WIDTH
            && element.y + STEP + element.height <= SLIDE_HEIGHT;
        if !taken || !fits {
            break;
        }
        element.x += STEP;
        element.y += STEP;
    }
}

impl crate::PresentationSession {
    /// Inserts a table element onto the active slide as an undoable step.
    pub fn insert_table(&mut self, table: Table) -> Result<String, String> {
        if self.document.active_slide().is_none() {
            return Err("there is no slide to insert the table on".into());
        }
        self.checkpoint();
        let slide = self.document.active_slide_mut().expect("checked above");
        let mut serial = slide.elements.len() + 1;
        let element_id = loop {
            let candidate = format!("{}-table-{serial}", slide.id);
            if !slide.elements.iter().any(|item| item.id == candidate) {
                break candidate;
            }
            serial += 1;
        };
        let mut elem = table_element_from_clipboard(table, element_id.clone());
        cascade_new_object(&slide.elements, &mut elem);
        slide.elements.push(elem);
        self.selected_elements = vec![element_id.clone()];
        Ok(element_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_inserted_table_does_not_sit_exactly_on_the_first() {
        let mut session =
            crate::PresentationSession::new(crate::PresentationDocument::new("d", "Deck"));
        let first = session.insert_table(Table::new(3, 3)).expect("first");
        let second = session.insert_table(Table::new(3, 3)).expect("second");
        let slide = session.document.active_slide().expect("slide");
        let a = slide.elements.iter().find(|e| e.id == first).unwrap();
        let b = slide.elements.iter().find(|e| e.id == second).unwrap();
        assert!(
            (a.x - b.x).abs() >= 1.0 || (a.y - b.y).abs() >= 1.0,
            "second table starts at ({}, {}) like the first",
            b.x,
            b.y
        );
    }

    #[test]
    fn test_sheets_clipboard_range_pastes_as_present_table() {
        use loom_clipboard::tsv_to_table;

        // TSV exported from Sheets range copy
        let tsv = "Product\tQ1\tQ2\tTotal\nWidget\t100\t150\t250\nGadget\t200\t300\t500";
        let table = tsv_to_table(tsv);
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.col_count(), 4);

        let mut elem = table_element_from_clipboard(table, "pasted-sheets-table");
        assert_eq!(elem.element_type, ElementType::Table);
        assert!(elem.width >= 200.0);
        assert!(elem.height >= 90.0);

        let retrieved = elem.table().expect("table must be deserializable");
        assert_eq!(retrieved.cell(0, 0).unwrap().content.text(), "Product");
        assert_eq!(retrieved.cell(1, 0).unwrap().content.text(), "Widget");
        assert_eq!(retrieved.cell(1, 3).unwrap().content.text(), "250");
        assert_eq!(retrieved.cell(2, 2).unwrap().content.text(), "300");

        // Can edit pasted cells in Present
        assert!(elem.update_table_cell_text(1, 3, "275"));
        let updated = elem.table().unwrap();
        assert_eq!(updated.cell(1, 3).unwrap().content.text(), "275");
    }

    #[test]
    fn test_slide_element_table_lifecycle_and_navigation() {
        let mut table = default_slide_table(3, 3);
        table.cell_mut(0, 0).unwrap().content = CellContent::plain("Q1");
        table.cell_mut(0, 1).unwrap().content = CellContent::plain("Q2");
        table.cell_mut(0, 2).unwrap().content = CellContent::plain("Q3");

        let mut elem = SlideElement::new_table("test-tbl", 100.0, 100.0, 600.0, 300.0, table);
        assert_eq!(elem.element_type, ElementType::Table);

        let retrieved = elem.table().expect("table must deserialize");
        assert_eq!(retrieved.row_count(), 3);
        assert_eq!(retrieved.col_count(), 3);
        assert_eq!(retrieved.cell(0, 0).unwrap().content.text(), "Q1");

        // Cell coordinate hit testing
        assert_eq!(elem.table_cell_at_local_coords(50.0, 30.0), Some((0, 0)));
        assert_eq!(elem.table_cell_at_local_coords(250.0, 30.0), Some((0, 1)));
        assert_eq!(elem.table_cell_at_local_coords(550.0, 250.0), Some((2, 2)));

        // Updating cell text
        assert!(elem.update_table_cell_text(1, 1, "UpdatedVal"));
        let updated_table = elem.table().unwrap();
        assert_eq!(
            updated_table.cell(1, 1).unwrap().content.text(),
            "UpdatedVal"
        );

        // Tab navigation forward
        assert_eq!(elem.tab_navigate_table(0, 0, false), (0, 1));
        assert_eq!(elem.tab_navigate_table(0, 2, false), (1, 0));

        // Tab navigation backward
        assert_eq!(elem.tab_navigate_table(1, 0, true), (0, 2));

        // Tab navigation at end appends row
        let (new_r, new_c) = elem.tab_navigate_table(2, 2, false);
        assert_eq!((new_r, new_c), (3, 0));
        let expanded = elem.table().unwrap();
        assert_eq!(expanded.row_count(), 4);

        // Arrow navigation
        assert_eq!(elem.arrow_navigate_table(1, 1, TableDirection::Up), (0, 1));
        assert_eq!(
            elem.arrow_navigate_table(1, 1, TableDirection::Down),
            (2, 1)
        );
        assert_eq!(
            elem.arrow_navigate_table(1, 1, TableDirection::Left),
            (1, 0)
        );
        assert_eq!(
            elem.arrow_navigate_table(1, 1, TableDirection::Right),
            (1, 2)
        );
    }

    #[test]
    fn test_table_row_and_column_mutations() {
        let table = default_slide_table(2, 2);
        let mut elem = SlideElement::new_table("mut-tbl", 50.0, 50.0, 400.0, 200.0, table);

        assert!(elem.insert_table_row(0, true));
        assert_eq!(elem.table().unwrap().row_count(), 3);

        assert!(elem.delete_table_row(1));
        assert_eq!(elem.table().unwrap().row_count(), 2);

        assert!(elem.insert_table_column(0, true));
        assert_eq!(elem.table().unwrap().col_count(), 3);

        assert!(elem.delete_table_column(1));
        assert_eq!(elem.table().unwrap().col_count(), 2);

        // Guard last row/col
        assert!(elem.delete_table_row(0));
        assert_eq!(elem.table().unwrap().row_count(), 1);
        assert!(!elem.delete_table_row(0)); // Cannot delete last remaining row
    }

    #[test]
    fn test_pptx_graphic_frame_export_and_import_round_trip() {
        let mut table = default_slide_table(2, 2);
        table.cell_mut(0, 0).unwrap().content = CellContent::plain("Col1");
        table.cell_mut(0, 1).unwrap().content = CellContent::plain("Col2");
        table.cell_mut(1, 0).unwrap().content = CellContent::plain("ValA");
        table.cell_mut(1, 1).unwrap().content = CellContent::plain("ValB");

        let elem = SlideElement::new_table("tbl-1", 100.0, 80.0, 500.0, 200.0, table.clone());

        let frame_xml = table_graphic_frame(2, &elem, &table);
        assert!(frame_xml.contains("<p:graphicFrame>"));
        assert!(frame_xml.contains("http://schemas.openxmlformats.org/drawingml/2006/table"));
        assert!(frame_xml.contains("<a:tbl>"));
        assert!(frame_xml.contains("Col1"));

        let imported_elements = extract_pptx_slide_tables(&frame_xml);
        assert_eq!(imported_elements.len(), 1);

        let imported_elem = &imported_elements[0];
        assert_eq!(imported_elem.element_type, ElementType::Table);
        assert!((imported_elem.x - 100.0).abs() < 1.0);
        assert!((imported_elem.y - 80.0).abs() < 1.0);

        let imported_table = imported_elem.table().expect("imported table");
        assert_eq!(imported_table.row_count(), 2);
        assert_eq!(imported_table.col_count(), 2);
        assert_eq!(imported_table.cell(0, 0).unwrap().content.text(), "Col1");
        assert_eq!(imported_table.cell(1, 1).unwrap().content.text(), "ValB");
    }
}
