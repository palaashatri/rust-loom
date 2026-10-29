use super::*;
use slint::Model;

fn table_text_hit_test_document() -> (loom_writer_core::WriterDocument, String) {
    let mut table = loom_writer_core::WriterTable::new("painted-cell-text", 1, 2);
    table.set(0, 0, "  café | tea  ");
    table.set(0, 1, "target");
    let markdown = table.to_markdown();
    let mut document = loom_writer_core::WriterDocument::new("painted-cell-text", "Document");
    document
        .blocks
        .push(loom_writer_core::markdown_table_block(1, &markdown));
    (document, markdown)
}

fn painted_table_cell_geometry(
    document: &loom_writer_core::WriterDocument,
    row: usize,
    column: usize,
    cell_width: f32,
) -> (String, Vec<usize>, loom_writer_core::TableCellGeometry) {
    let table_block = document
        .blocks
        .iter()
        .find(|block| block.typed_table().is_some())
        .expect("the document contains a typed table");
    let table = table_block.typed_table().expect("the table block is typed");
    let style = loom_objects::TableStyleCatalog::get(&table.style_id)
        .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
    let options = loom_objects::TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
        first_column: table.first_column,
    };
    let resolved = style.resolve_cell(
        row,
        column,
        table.row_count(),
        table.col_count(),
        &options,
        table.cell(row, column),
    );
    let block = crate::projection::cell_text_block_with_color(
        table_block.id,
        table,
        row,
        column,
        resolved.bold,
        resolved.text_color.as_deref(),
    )
    .expect("the table cell has text to shape");
    let geometry = loom_writer_core::shape_table_cell_geometry(
        &block,
        loom_writer_core::TABLE_CELL_BASE_FONT_SIZE_PT,
        (cell_width - 12.0).max(1.0),
    )
    .expect("the canonical cell text has shaped geometry");
    let (display_text, source_offsets) = document
        .table_cell_display_map(table_block.id, row, column)
        .expect("the typed cell has a display-to-source map");
    (display_text, source_offsets, geometry)
}

// User action: click within text drawn in a painted cell.
// The Markdown importer stores a trimmed, unescaped value in the typed cell.
// Pointer geometry must follow that canonical UTF-8 text and map back to the
// corresponding byte offset in the table block's editor text.
#[test]
fn painted_table_pointer_maps_trimmed_unescaped_utf8_text_to_source_offset() {
    let (document, _markdown) = table_text_hit_test_document();
    let projection = crate::projection::project(
        &document,
        loom_writer_core::PageViewport::default(),
        Some(loom_writer_core::PageStyle::default().height_pt),
    );
    let cell_rect = projection
        .table_cell_rects
        .first()
        .expect("the first painted table cell has geometry");
    let (visible_text, source_offsets, geometry) =
        painted_table_cell_geometry(&document, 0, 0, cell_rect.width);
    assert_eq!(
        visible_text, "café | tea",
        "the typed cell stores the trimmed, unescaped canonical value"
    );
    let tea_source_offset = document
        .editor_text()
        .find("tea")
        .expect("canonical source suffix");
    let location = document
        .table_cell_at_offset(tea_source_offset)
        .expect("the canonical source suffix belongs to the table");
    assert_eq!((location.row, location.col), (0, 0));
    let block_start = document
        .blocks
        .iter()
        .take(location.block_index)
        .map(|block| block.text.as_str().len() + 1)
        .sum::<usize>();
    let cell_source_byte = tea_source_offset - block_start;
    let display_byte = source_offsets
        .iter()
        .position(|source_byte| *source_byte == cell_source_byte)
        .expect("the source suffix maps to a displayed UTF-8 boundary");
    assert_eq!(
        display_byte,
        visible_text.find("tea").expect("visible suffix")
    );
    let line = geometry
        .lines()
        .iter()
        .find(|line| {
            let range = line.source_range();
            range.start <= display_byte && display_byte < range.end
        })
        .expect("the suffix belongs to a shaped line");
    let shaped = line.shaped().expect("the visible text has a shaped line");
    let t_start = shaped.caret_x(display_byte, loom_fonts::Affinity::Leading);
    let t_end = shaped.caret_x(display_byte + 1, loom_fonts::Affinity::Leading);
    assert!(t_end > t_start, "the visible 't' has positive shaped width");
    let local_x = t_start + (t_end - t_start) * 0.1;
    assert_eq!(
        shaped.hit_test(local_x).map(|stop| stop.offset),
        Some(display_byte),
        "the test coordinate lies inside the visible 't', after its leading boundary"
    );
    let x = cell_rect.x + 6.0 + local_x;
    let y = cell_rect.y + cell_rect.height / 2.0;
    let expected_source_offset = block_start + source_offsets[display_byte];
    assert_eq!(expected_source_offset, tea_source_offset);

    let hit =
        crate::writer_pointer_offset(&document, loom_writer_core::PageViewport::default(), x, y)
            .expect("the pointer hits the painted cell");
    assert_eq!(
        hit, expected_source_offset,
        "a click inside the visible 't' maps to its canonical source byte boundary"
    );
    let cell = document
        .table_cell_at_offset(hit)
        .expect("the mapped source offset belongs to a table cell");
    assert_eq!(
        (cell.row, cell.col),
        (0, 0),
        "the escaped literal pipe remains in the first typed cell"
    );
}

#[test]
fn painted_table_caret_uses_trimmed_unescaped_utf8_cell_text() {
    let (mut document, _markdown) = table_text_hit_test_document();
    let tea_source_offset = document
        .editor_text()
        .find("tea")
        .expect("canonical source suffix");
    document.set_selection(loom_writer_core::TextSelection::caret(tea_source_offset));
    let selected_cell = document
        .table_cell_at_offset(tea_source_offset)
        .expect("the source caret belongs to the table");
    assert_eq!(
        (selected_cell.row, selected_cell.col),
        (0, 0),
        "the escaped literal pipe remains part of the first typed cell"
    );
    let projection = crate::projection::project(
        &document,
        loom_writer_core::PageViewport::default(),
        Some(loom_writer_core::PageStyle::default().height_pt),
    );
    let cell_rect = projection
        .table_cell_rects
        .first()
        .expect("the first painted table cell has geometry");
    let (visible_text, source_offsets, geometry) = painted_table_cell_geometry(
        &document,
        selected_cell.row,
        selected_cell.col,
        cell_rect.width,
    );
    assert_eq!(
        visible_text, "café | tea",
        "the typed cell stores the trimmed, unescaped canonical value"
    );
    let block_start = document
        .blocks
        .iter()
        .take(selected_cell.block_index)
        .map(|block| block.text.as_str().len() + 1)
        .sum::<usize>();
    let cell_source_byte = tea_source_offset - block_start;
    let display_byte = source_offsets
        .iter()
        .position(|source_byte| *source_byte == cell_source_byte)
        .expect("the source caret maps to a displayed UTF-8 boundary");
    assert_eq!(
        display_byte,
        visible_text.find("tea").expect("visible suffix")
    );
    let line = geometry
        .lines()
        .iter()
        .find(|line| {
            let range = line.source_range();
            range.start <= display_byte && display_byte < range.end
        })
        .expect("the caret belongs to a shaped line");
    let shaped = line.shaped().expect("the visible text has a shaped line");
    let expected_x =
        cell_rect.x + 6.0 + shaped.caret_x(display_byte, loom_fonts::Affinity::Leading);
    let caret = projection
        .selection_rects
        .iter()
        .find(|rect| rect.caret)
        .expect("the table caret is projected");

    assert!(
        (caret.x - expected_x).abs() < 0.01,
        "the caret at canonical byte offset {tea_source_offset} follows the 12 pt shaped display boundary at x {expected_x}, got {}",
        caret.x
    );
}

#[test]
fn synthetic_slint_pointer_click_on_painted_c3_places_the_caret_in_c3() {
    crate::table_actions::clear_table_cell_selection_range();
    let (small_document, _, _, _) = populated_document_with_table(
        "painted-table-pointer-cache-seed",
        "Painted Table Pointer Cache Seed",
        2,
        2,
        "Body after table",
        "seed",
    );
    let (cache_seed, cache_seed_state) = crate::actions_tests::test_state(
        small_document,
        std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
    );
    cache_seed
        .window()
        .set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&cache_seed, &cache_seed_state, None);
    crate::apply_state(&cache_seed, &cache_seed_state);
    let _ = loom_test_support::capture::snapshot_component(&cache_seed, 1280.0, 800.0, 1.0)
        .expect("prime the table-rect cache with a different geometry");

    let (document, table_id, _, _) = populated_document_with_table(
        "painted-table-pointer",
        "Painted Table Pointer",
        3,
        3,
        "Body after table",
        "cell",
    );
    let (primed_app, primed_state) = crate::actions_tests::test_state(
        document.clone(),
        std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
    );
    primed_app
        .window()
        .set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&primed_app, &primed_state, None);
    crate::apply_state(&primed_app, &primed_state);
    let _ = loom_test_support::capture::snapshot_component(&primed_app, 1280.0, 800.0, 1.0)
        .expect("publish the target geometry in the warmed app");
    assert_eq!(
        primed_app.get_table_cell_rects().row_count(),
        9,
        "the warmed app owns the expected nine-cell geometry"
    );

    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = crate::actions_tests::test_state(document, dialogs);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render the painted table before clicking it");
    crate::focus_page_at_launch(&app);

    let rects = app.get_table_cell_rects();
    assert_eq!(
        rects.row_count(),
        9,
        "a fresh app receives table geometry even when another app published identical geometry"
    );
    let rect = rects.row_data(8).expect("the app publishes C3 geometry");
    let current = state.current.borrow();
    let projected = crate::projection::project(
        &current,
        crate::PageViewport::default(),
        Some(app.get_page_view_height()),
    );
    let expected = &projected.table_cell_rects[8];
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (expected.x, expected.y, expected.width, expected.height),
        "the app model contains the document's actual C3 geometry"
    );
    assert!(rect.has_bg, "C3's app geometry carries its table fill");
    let before = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("capture the app's painted C3 before the synthetic click");
    assert_eq!(
        cell_pixel(&before, &app, &current, table_id, 2, 2),
        [
            rect.bg_color.red(),
            rect.bg_color.green(),
            rect.bg_color.blue(),
            rect.bg_color.alpha(),
        ],
        "the app's C3 geometry and fill are present in the rendered pixels before clicking"
    );
    let origin = document_body_window_origin(&app);
    let zoom = app.get_page_zoom();
    let point = (
        origin.0 + (rect.x + rect.width / 2.0) * zoom,
        origin.1 + (rect.y + rect.height / 2.0) * zoom,
    );
    drop(current);
    dispatch_window_pointer_click(&app, point);

    let current = state.current.borrow();
    let selection = current.selection();
    assert!(selection.is_collapsed(), "a click places a caret");
    let selected = current
        .table_cell_at_offset(selection.focus)
        .expect("the pointer caret remains inside the table");
    assert_eq!(
        (selected.block_id, selected.row, selected.col),
        (table_id, 2, 2),
        "the native pointer point is the center of painted C3"
    );
}

#[test]
fn shift_arrow_table_range_changes_pixels_inside_selected_cells() {
    crate::table_actions::clear_table_cell_selection_range();
    let (document, table_id, table_offset, _) = populated_document_with_table(
        "painted-table-range",
        "Painted Table Range",
        3,
        3,
        "Body after table",
        "cell",
    );
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = crate::actions_tests::test_state(document, dialogs);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("instantiate the page before focusing it");
    crate::focus_page_at_launch(&app);
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    let blocks_before = state.current.borrow().blocks.clone();
    let undo_depth_before = state.history.borrow().undo_len();
    let before = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render before extending the table range");

    dispatch_shifted_arrows(
        &app,
        &[
            slint::platform::Key::RightArrow,
            slint::platform::Key::DownArrow,
        ],
    );
    let after = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render the selected A1:B2 range");
    if let Some(directory) = std::env::var_os("LOOM_CAPTURE_TABLE_SELECTION_DIR") {
        let path = std::path::PathBuf::from(directory).join("writer-table-selection-after.png");
        std::fs::create_dir_all(path.parent().expect("capture path has a directory"))
            .expect("create the opt-in table selection capture directory");
        after
            .save(&path)
            .expect("write the opt-in table selection capture");
    }
    let current = state.current.borrow();

    assert_eq!(
        app.get_document_accessible_label().as_str(),
        "Table 3 by 3, cells A1:B2 selected",
        "the range remains available to assistive technology"
    );
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        Some(crate::table_actions::TableCellRange {
            block_id: table_id,
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        }),
        "Shift+Right then Shift+Down selects exactly A1:B2"
    );
    assert_eq!(
        current.selection(),
        loom_writer_core::TextSelection::caret(table_offset),
        "the 2D table range does not expand the collapsed text selection"
    );
    assert_eq!(
        current.blocks, blocks_before,
        "moving the table range does not change document content"
    );
    assert_eq!(
        state.history.borrow().undo_len(),
        undo_depth_before,
        "moving the table range does not create an undo entry"
    );
    for (row, column, name) in [(0, 0, "A1"), (0, 1, "B1"), (1, 0, "A2"), (1, 1, "B2")] {
        assert_ne!(
            cell_pixel(&before, &app, &current, table_id, row, column),
            cell_pixel(&after, &app, &current, table_id, row, column),
            "selected {name} has a visible range treatment"
        );
    }
    assert_eq!(
        cell_pixel(&before, &app, &current, table_id, 1, 2),
        cell_pixel(&after, &app, &current, table_id, 1, 2),
        "unselected C2 keeps its original fill"
    );
    drop(current);

    let same_caret_point = pointer_position_for_offset(&state.current.borrow(), table_offset);
    app.invoke_pointer_pressed(same_caret_point.0, same_caret_point.1, false);
    app.invoke_pointer_released(same_caret_point.0, same_caret_point.1);
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        None,
        "a same-caret pointer press clears the virtual table range"
    );
    let cleared = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render after clearing the table range with a same-caret click");
    let current = state.current.borrow();
    assert_eq!(
        current.selection(),
        loom_writer_core::TextSelection::caret(table_offset),
        "clearing the range keeps the text caret collapsed at A1"
    );
    assert_eq!(
        cell_pixel(&before, &app, &current, table_id, 0, 0),
        cell_pixel(&cleared, &app, &current, table_id, 0, 0),
        "a same-caret click removes the rendered A1 range highlight"
    );
}

#[test]
fn ordinary_arrow_clears_table_range_before_copy_and_render() {
    let _clipboard_guard = MockClipboardGuard::new();
    crate::table_actions::clear_table_cell_selection_range();
    let (document, table_id, table_offset, _) = populated_document_with_table(
        "plain-arrow-clears-table-range",
        "Plain Arrow Clears Table Range",
        3,
        3,
        "Body after table",
        "cell",
    );
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = crate::actions_tests::test_state(document, dialogs);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("instantiate the page before focusing it");
    crate::focus_page_at_launch(&app);
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    let before = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render before extending the table range");

    dispatch_shifted_arrows(
        &app,
        &[
            slint::platform::Key::RightArrow,
            slint::platform::Key::DownArrow,
        ],
    );
    app.invoke_copy();
    let copied_range = resolve_table_with_note(&read_clipboard().expect("read the copied range"))
        .map(|(table, _)| (table.row_count(), table.col_count()));
    assert_eq!(
        copied_range,
        Some((2, 2)),
        "Copy uses the selected A1:B2 range"
    );

    dispatch_plain_arrow(&app, slint::platform::Key::RightArrow);
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        None,
        "an unmodified arrow collapses the virtual table range"
    );
    let current = state.current.borrow();
    assert!(
        current.selection().is_collapsed(),
        "the editor caret stays collapsed"
    );
    assert_ne!(
        current.selection().focus,
        table_offset,
        "the unmodified arrow still moves the text caret"
    );
    assert!(
        current
            .table_cell_at_offset(current.selection().focus)
            .is_some_and(|location| location.block_id == table_id),
        "the caret remains within the same table"
    );
    let after = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render after the ordinary arrow movement");
    for (row, column, name) in [(0, 0, "A1"), (0, 1, "B1"), (1, 0, "A2"), (1, 1, "B2")] {
        assert_eq!(
            cell_pixel(&before, &app, &current, table_id, row, column),
            cell_pixel(&after, &app, &current, table_id, row, column),
            "the ordinary arrow removes the stale {name} highlight"
        );
    }
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData {
        plain_text: Some("sentinel".into()),
        ..Default::default()
    })
    .expect("write the sentinel");
    app.invoke_copy();
    assert_eq!(
        read_clipboard()
            .expect("read the clipboard")
            .plain_text
            .as_deref(),
        Some("sentinel"),
        "Copy with a collapsed caret copies nothing, so no stale cells are copied"
    );
}

#[test]
fn high_contrast_table_range_has_a_seven_to_one_boundary() {
    crate::table_actions::clear_table_cell_selection_range();
    let (document, table_id, table_offset, _) = populated_document_with_table(
        "high-contrast-table-range",
        "High Contrast Table Range",
        3,
        3,
        "Body after table",
        "cell",
    );
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = crate::actions_tests::test_state(document, dialogs);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::appearance::apply_id(&app, "high-contrast");
    crate::apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("instantiate the high-contrast page before focusing it");
    crate::focus_page_at_launch(&app);
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    dispatch_shifted_arrows(
        &app,
        &[
            slint::platform::Key::RightArrow,
            slint::platform::Key::DownArrow,
        ],
    );
    let frame = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render the high-contrast A1:B2 selection");
    let current = state.current.borrow();
    let (_, (x, y, width, height)) = painted_cell_window_geometry(&app, &current, table_id, 1, 1);
    let origin = document_body_window_origin(&app);
    let zoom = app.get_page_zoom();
    let boundary_x = (origin.0 + (x + width - 0.5) * zoom).floor() as u32;
    let boundary_y = (origin.1 + (y + height / 2.0) * zoom).floor() as u32;
    let selected_boundary = frame.get_pixel(boundary_x, boundary_y).0;
    let adjacent_cell = cell_pixel(&frame, &app, &current, table_id, 1, 2);
    let measured = contrast_ratio(
        [
            selected_boundary[0],
            selected_boundary[1],
            selected_boundary[2],
        ],
        [adjacent_cell[0], adjacent_cell[1], adjacent_cell[2]],
    );
    assert!(
        measured >= 7.0,
        "high-contrast B2's right selection boundary measures {measured:.2}:1 against C2"
    );
}

fn rendered_high_contrast_selection_boundary_contrast(style_id: &str) -> (f64, [u8; 3], [u8; 3]) {
    crate::table_actions::clear_table_cell_selection_range();
    let (mut document, table_id, initial_offset, _) = populated_document_with_table(
        "high-contrast-table-style-boundary",
        "High Contrast Table Style Boundary",
        3,
        3,
        "Body after table",
        "cell",
    );
    document
        .set_table_style_at_offset(initial_offset, style_id)
        .expect("the table accepts the representative style");
    let source_table = document
        .table_from_block(table_id)
        .expect("the styled source table exists");
    let table_offset = (0..=document.editor_text().len())
        .find(|offset| document.table_cell_at_offset(*offset).is_some())
        .expect("the styled table has a first cell offset");

    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = crate::actions_tests::test_state(document, dialogs);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::appearance::apply_id(&app, "high-contrast");
    crate::apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("instantiate the high-contrast page before focusing it");
    crate::focus_page_at_launch(&app);
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    dispatch_shifted_arrows(&app, &[slint::platform::Key::RightArrow]);
    let frame = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render the high-contrast table cell selection");
    let current = state.current.borrow();
    let styled_table = current
        .table_from_block(table_id)
        .expect("the high-contrast render keeps the source table");
    assert_eq!(
        styled_table.style_id, style_id,
        "appearance changes preserve the document's table style id"
    );
    assert_eq!(
        styled_table, source_table,
        "High Contrast changes only the rendered cell surface, not stored table data or style"
    );
    if style_id == "table-style-medium-4" {
        let style = loom_objects::TableStyleCatalog::get(style_id).expect("Medium 4 exists");
        let options = loom_objects::TableStyleOptions {
            header_row: styled_table.header_row,
            banded_rows: styled_table.banded_rows,
            first_column: styled_table.first_column,
            ..Default::default()
        };
        assert_eq!(
            style
                .resolve_cell(0, 1, 3, 3, &options, None)
                .background_color
                .as_deref(),
            Some("#D35400"),
            "Medium 4's stored terracotta fill remains unchanged"
        );
    }
    let (_, (x, y, width, height)) = painted_cell_window_geometry(&app, &current, table_id, 0, 1);
    let origin = document_body_window_origin(&app);
    let zoom = app.get_page_zoom();
    let boundary_x = (origin.0 + (x + width - 0.5) * zoom).floor() as u32;
    let boundary_y = (origin.1 + (y + height / 2.0) * zoom).floor() as u32;
    let selected_boundary = frame.get_pixel(boundary_x, boundary_y).0;
    let adjacent_cell = cell_pixel(&frame, &app, &current, table_id, 0, 2);
    let boundary_rgb = [
        selected_boundary[0],
        selected_boundary[1],
        selected_boundary[2],
    ];
    let adjacent_rgb = [adjacent_cell[0], adjacent_cell[1], adjacent_cell[2]];
    let contrast = contrast_ratio(boundary_rgb, adjacent_rgb);
    crate::table_actions::clear_table_cell_selection_range();
    (contrast, boundary_rgb, adjacent_rgb)
}

// User action: select cells in each supported table style while High Contrast is on.
// Current failure: a monochrome boundary against Medium 4's terracotta fill
// cannot reach 7:1, even after the selected fill is composited.
// Required behavior: the rendered boundary contrasts at least 7:1 with the
// adjacent cell while the document retains its original table style.
#[test]
fn high_contrast_selection_boundaries_have_seven_to_one_contrast_for_all_table_styles() {
    for style_id in [
        "table-style-medium-4",
        "table-style-light-1",
        "table-style-light-2",
        "table-style-light-3",
        "table-style-light-4",
        "table-style-medium-1",
        "table-style-medium-2",
        "table-style-medium-3",
        "table-style-dark-1",
        "table-style-dark-2",
        "table-style-dark-3",
        "table-style-dark-4",
    ] {
        // Sample the B1-to-C1 boundary so Medium 4 exercises its #D35400 header.
        let (measured, boundary, adjacent) =
            rendered_high_contrast_selection_boundary_contrast(style_id);
        assert!(
            measured >= 7.0,
            "{style_id} selected-cell boundary #{:02X}{:02X}{:02X} against adjacent #{:02X}{:02X}{:02X} measures {measured:.2}:1",
            boundary[0],
            boundary[1],
            boundary[2],
            adjacent[0],
            adjacent[1],
            adjacent[2],
        );
    }
}
