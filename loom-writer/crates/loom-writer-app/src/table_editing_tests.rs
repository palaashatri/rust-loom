//! Tests for real table editing in Loom Writer (Wave 1 W1).

#[cfg(test)]
mod tests {
    use crate::actions_tests::test_state;
    use crate::*;
    use loom_writer_core::{WriterTable, TABLE_BLOCK_KIND};
    use std::rc::Rc;

    fn make_test_app() -> (WriterApp, Rc<GuiState>) {
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let mut doc = WriterDocument::new("test-table", "Test Table");
        doc.replace_paragraphs("Initial text");
        let (app, state) = test_state(doc, dialogs);
        wire_writer_shared_callbacks(&app, &state, None);
        (app, state)
    }

    #[test]
    fn test_insert_table_with_dimensions_creates_table_and_places_caret() {
        let (app, state) = make_test_app();
        // Invoke custom table insertion: 4 rows x 5 columns
        app.invoke_insert_table_size(4, 5);

        let current = state.current.borrow();
        let table_block = current
            .blocks
            .iter()
            .find(|b| b.kind == TABLE_BLOCK_KIND)
            .expect("Table block must be inserted");
        let table = current
            .table_from_block(table_block.id)
            .expect("Must parse table");
        assert_eq!(table.rows.len(), 4, "Must have 4 rows");
        assert_eq!(table.columns(), 5, "Must have 5 columns");

        // Verify caret is inside the first cell
        let caret = current.selection().anchor;
        let loc = current
            .table_cell_at_offset(caret)
            .expect("Caret must be in table");
        assert_eq!(loc.row, 0);
        assert_eq!(loc.col, 0);
    }

    #[test]
    fn test_table_cell_tab_and_arrow_navigation_in_app() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(2, 3);

        // First cell (0, 0)
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 0));
        drop(current);

        // Tab to next cell (0, 1)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 1));
        drop(current);

        // Tab to (0, 2)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 2));
        drop(current);

        // Tab to (1, 0)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (1, 0));
        drop(current);

        // Backtab (Shift+Tab) to (0, 2)
        app.invoke_table_navigate_backwards();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 2));
        drop(current);

        // Tab three times to reach last cell (1, 2)
        app.invoke_insert_tab();
        app.invoke_insert_tab();
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (1, 2));
        drop(current);

        // Tab at the last cell appends a row! Now 3 rows, caret at (2, 0)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 3, "Table must now have 3 rows");
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (2, 0));
    }

    #[test]
    fn test_table_mutations_and_undo_redo_in_app() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);

        // Insert row below
        app.invoke_table_action("insert-row-below".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 4, "Must have 4 rows after insert");
        drop(current);

        // Undo row insertion
        app.invoke_undo();
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 3, "Must have 3 rows after undo");
        drop(current);

        // Insert column right
        app.invoke_table_action("insert-col-right".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.columns(), 4, "Must have 4 columns after insert");
        drop(current);

        // Delete row
        app.invoke_table_action("delete-row".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 2, "Must have 2 rows after delete row");
        drop(current);

        // Delete column
        app.invoke_table_action("delete-col".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.columns(), 3, "Must have 3 columns after delete col");
    }

    #[test]
    fn test_table_style_and_options_in_inspector() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);

        // Change table style to Dark 1
        app.invoke_set_table_style("table-style-dark-1".into());
        let current = state.current.borrow();
        let loom_table = current
            .table_from_block(current.blocks[1].id)
            .unwrap()
            .to_loom_table();
        assert_eq!(loom_table.style_id, "table-style-dark-1");
        drop(current);

        // Toggle banded rows
        app.invoke_toggle_table_option("banded_rows".into());
        let current = state.current.borrow();
        let loom_table = current
            .table_from_block(current.blocks[1].id)
            .unwrap()
            .to_loom_table();
        assert!(!loom_table.banded_rows, "Banded rows should be toggled off");
        drop(current);

        // Undo style change
        app.invoke_undo();
        let current = state.current.borrow();
        let loom_table = current
            .table_from_block(current.blocks[1].id)
            .unwrap()
            .to_loom_table();
        assert!(
            loom_table.banded_rows,
            "Banded rows should be restored to on"
        );
    }

    #[test]
    fn test_table_accessibility_semantics() {
        let (app, _state) = make_test_app();
        app.invoke_insert_table_size(3, 4);

        // Move to cell (1, 1) which is B2
        app.invoke_insert_tab(); // (0, 1)
        app.invoke_insert_tab(); // (0, 2)
        app.invoke_insert_tab(); // (0, 3)
        app.invoke_insert_tab(); // (1, 0)
        app.invoke_insert_tab(); // (1, 1)

        let a11y_name = app.get_document_accessible_label();
        assert!(
            a11y_name.contains("Table 3 by 4") && a11y_name.contains("cell B2"),
            "Accessible label must describe table dimensions and cell coordinates: got '{a11y_name}'"
        );
    }

    #[test]
    fn test_docx_export_contains_real_wtbl_and_shading() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);
        app.invoke_set_table_style("table-style-medium-1".into());

        // Populate cell text and merge cells (2, 0) and (2, 1)
        let tbl_id = state.current.borrow().blocks[1].id;
        let mut tbl = state.current.borrow().table_from_block(tbl_id).unwrap();
        tbl.set(0, 0, "Product");
        tbl.set(0, 1, "Qty");
        tbl.set(0, 2, "Price");
        tbl.set(1, 0, "Apples");
        tbl.set(1, 1, "50");
        tbl.set(1, 2, "$1.50");
        tbl.set(2, 0, "SpecialOffer");
        tbl.set(2, 2, "$0.99");
        let mut loom_tbl = tbl.to_loom_table();
        loom_tbl.merge_cells(2, 0, 1, 2).unwrap();
        let updated = WriterTable::from_loom_table(&loom_tbl);
        if let Some(b) = state
            .current
            .borrow_mut()
            .blocks
            .iter_mut()
            .find(|b| b.id == tbl_id)
        {
            b.text = loom_writer_core::Text::from_str(&updated.to_markdown());
        }

        // Save to target for external COM verification using the APP's export path
        let out_dir = std::path::Path::new("target");
        std::fs::create_dir_all(out_dir).ok();
        let out_path = out_dir.join("w1-table-verification.docx");
        let (_path, export) =
            crate::docx_export::export_docx_file(&out_path, &state.current.borrow())
                .expect("Must export docx through app export path");

        let archive = loom_package::zip::PackageArchive::from_bytes(&export.bytes)
            .expect("Must parse docx zip");
        let doc_xml = String::from_utf8(
            archive
                .get("word/document.xml")
                .expect("document.xml")
                .to_vec(),
        )
        .unwrap();

        assert!(doc_xml.contains("<w:tbl>"), "DOCX must contain <w:tbl>");
        assert!(
            doc_xml.contains("<w:tblGrid>"),
            "DOCX must contain <w:tblGrid>"
        );
        assert!(
            doc_xml.contains("<w:gridCol"),
            "DOCX must contain grid columns"
        );
        assert!(doc_xml.contains("<w:shd"), "DOCX must contain cell shading");
        assert!(
            doc_xml.contains("<w:tblHeader/>"),
            "DOCX must contain header row marker"
        );
        assert!(
            doc_xml.contains("<w:gridSpan w:val=\"2\"/>"),
            "DOCX must contain horizontal merge gridSpan=2"
        );
    }

    #[test]
    fn test_property_zero_text_loss_around_tables() {
        let (_app, state) = make_test_app();
        let mut doc = state.current.borrow().clone();
        doc.replace_paragraphs("Introduction paragraph with unique words Alpha Beta Gamma.");
        doc.insert_table_block(usize::MAX, 2, 2).unwrap();
        // Populate table cells with unique words
        let tbl_id = doc
            .blocks
            .iter()
            .find(|b| b.kind == TABLE_BLOCK_KIND)
            .unwrap()
            .id;
        let mut tbl = doc.table_from_block(tbl_id).unwrap();
        tbl.set(0, 0, "CellOneOne");
        tbl.set(0, 1, "CellOneTwo");
        tbl.set(1, 0, "CellTwoOne");
        tbl.set(1, 1, "CellTwoTwo");
        if let Some(b) = doc.blocks.iter_mut().find(|b| b.id == tbl_id) {
            b.text = loom_writer_core::Text::from_str(&tbl.to_markdown());
        }
        // Add paragraph after table
        doc.blocks.push(loom_writer_core::RichBlock::new(
            99,
            "paragraph",
            "Conclusion paragraph with Delta Epsilon Zeta.",
        ));

        let test_words = [
            "Alpha",
            "Beta",
            "Gamma",
            "CellOneOne",
            "CellOneTwo",
            "CellTwoOne",
            "CellTwoTwo",
            "Delta",
            "Epsilon",
            "Zeta",
        ];

        // 1. DOCX export
        let docx = loom_writer_core::export_docx(&doc).expect("Must export docx");
        let archive =
            loom_package::zip::PackageArchive::from_bytes(&docx.bytes).expect("Must parse docx");
        let docx_xml = String::from_utf8(
            archive
                .get("word/document.xml")
                .expect("document.xml")
                .to_vec(),
        )
        .unwrap();
        for word in &test_words {
            assert!(
                docx_xml.contains(word),
                "DOCX document.xml must contain word '{word}'"
            );
        }

        // 2. Markdown export
        let md = doc.to_markdown();
        for word in &test_words {
            assert!(
                md.contains(word),
                "Markdown export must contain word '{word}'"
            );
        }

        // 3. PDF export
        let pdf_bytes = loom_writer_core::export_pdf(&doc);
        let pdf_readable = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");
        for word in &test_words {
            assert!(
                pdf_readable.contains(word),
                "PDF export must contain word '{word}'"
            );
        }
    }

    #[test]
    fn test_typing_latency_in_100x20_table() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(100, 20);

        let initial_text = state.current.borrow().editor_text();
        let caret = state.current.borrow().selection().anchor;

        let mut latencies_ms = Vec::with_capacity(20);
        let mut cur_text = initial_text;
        let mut cur_caret = caret;

        for i in 0..20 {
            let ch = (b'a' + (i % 26) as u8) as char;
            let mut next_text = cur_text.clone();
            next_text.insert(cur_caret, ch);
            cur_caret += 1;

            let start = std::time::Instant::now();
            app.invoke_document_edited(
                next_text.clone().into(),
                cur_caret as i32,
                cur_caret as i32,
            );
            let elapsed = start.elapsed();
            latencies_ms.push(elapsed.as_secs_f64() * 1000.0);
            cur_text = next_text;
        }

        latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = ((latencies_ms.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
        let p95 = latencies_ms[p95_idx];
        println!("100x20 table typing latency: p95 = {:.2} ms", p95);
        assert!(
            p95 < 250.0,
            "Typing latency in debug build must stay responsive: got {p95} ms"
        );
    }
}
