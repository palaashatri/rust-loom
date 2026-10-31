//! Geometry regressions for table cell pointer hit testing and painted range
//! selection. The pure tests drive the projection and hit-test path without a
//! window; the window tests press through Slint's real pointer dispatch.

use super::writer_pointer_offset_with_model;
use crate::projection;
use loom_fonts::Affinity;
use loom_writer_core::{
    PageStyle, PageViewport, RichBlock, TextSelection, WriterDocument, WriterTable,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

/// Window-space band the editor's visible cells must fall inside for a press
/// to reach the page. Points outside it sit under the toolbar, the status bar
/// or beyond the window, so no press can land there.
const VISIBLE_TOP: f32 = 150.0;
const VISIBLE_BOTTOM: f32 = 700.0;
/// Height of the window the drag test renders into: a point below it is below
/// anything the window shows.
const WINDOW_HEIGHT: f32 = 800.0;

struct ResetTestFontCatalog;

impl Drop for ResetTestFontCatalog {
    fn drop(&mut self) {
        loom_writer_core::fonts::reset_font_catalog();
    }
}

/// A table block with `rows` by `cols` cells, each holding its own position
/// as text, so a wrong hit is visible in the failure message.
fn table_document(rows: usize, cols: usize) -> WriterDocument {
    let mut table = WriterTable::new("table-pointer-geometry", rows, cols);
    for row in 0..rows {
        for col in 0..cols {
            table.set(row, col, format!("r{row}c{col}"));
        }
    }
    let mut document = WriterDocument::new("table-pointer-geometry", "Table pointer geometry");
    document.blocks.push(loom_writer_core::markdown_table_block(
        1,
        &table.to_markdown(),
    ));
    document
}

/// Intro prose, the table, then prose after it, as a Writer page holds them.
fn page_with_table(rows: usize, cols: usize) -> (WriterDocument, u64) {
    let mut table = WriterTable::new("table-pointer-page", rows, cols);
    for row in 0..rows {
        for col in 0..cols {
            table.set(row, col, format!("r{row}c{col}"));
        }
    }
    let mut document = WriterDocument::new("table-pointer-page", "Table pointer page");
    document.blocks.push(RichBlock::new(
        1,
        "paragraph",
        "Intro paragraph above the table",
    ));
    document.blocks.push(loom_writer_core::markdown_table_block(
        2,
        &table.to_markdown(),
    ));
    document
        .blocks
        .push(RichBlock::new(3, "paragraph", "Body after the table"));
    (document, 2)
}

/// The row and column of the cell the current selection focus is in.
fn focused_cell(state: &crate::GuiState, table_id: u64) -> Option<(usize, usize)> {
    let current = state.current.borrow();
    current
        .table_cell_at_offset(current.selection().focus)
        .filter(|location| location.block_id == table_id)
        .map(|location| (location.row, location.col))
}

/// The window point of the editor's content origin, measured now: a press
/// that reveals the caret can scroll the page, so an earlier reading is stale.
fn fresh_document_origin(app: &crate::WriterApp) -> (f32, f32) {
    let label = app.get_document_accessible_label().to_string();
    let body = i_slint_backend_testing::ElementHandle::find_by_accessible_label(app, &label)
        .find(|element| {
            element.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::TextInput)
        })
        .expect("the document editor is in the tree");
    let origin = body.absolute_position();
    (origin.x, origin.y)
}

fn dispatch_window_press(app: &crate::WriterApp, point: (f32, f32)) {
    let position = slint::LogicalPosition::new(point.0, point.1);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
}

/// Find the center of real nontransparent pixels in the shaped glyph tiles for
/// one table cell and map it back through the tile, row, page zoom, and current
/// editor origin. This remains a visual-paint probe when the logical Slint
/// Text segments are hidden to avoid independent reshaping.
fn painted_cell_ink_center(app: &crate::WriterApp, label: &str, zoom: f32) -> (f32, f32) {
    let render_blocks = app.get_render_blocks();
    let row = render_blocks
        .iter()
        .find(|row| {
            row.table_cell
                && row
                    .segments
                    .iter()
                    .map(|segment| segment.text.to_string())
                    .collect::<String>()
                    == label
        })
        .unwrap_or_else(|| panic!("the logical render row for {label} is present"));
    let mut ink = None::<(f32, f32, f32, f32)>;
    for tile in row.glyph_tiles.iter() {
        let Some(pixels) = tile.source.to_rgba8() else {
            continue;
        };
        let width = pixels.width();
        let height = pixels.height();
        if width == 0 || height == 0 {
            continue;
        }
        let mut tile_ink = None::<(u32, u32, u32, u32)>;
        for (index, pixel) in pixels.as_bytes().chunks_exact(4).enumerate() {
            if pixel[3] == 0 {
                continue;
            }
            let x = (index % width as usize) as u32;
            let y = (index / width as usize) as u32;
            tile_ink = Some(match tile_ink {
                Some((left, top, right, bottom)) => {
                    (left.min(x), top.min(y), right.max(x), bottom.max(y))
                }
                None => (x, y, x, y),
            });
        }
        if let Some((left, top, right, bottom)) = tile_ink {
            let page_left = row.x + tile.x + left as f32 / width as f32 * tile.width;
            let page_top = row.y + tile.y + top as f32 / height as f32 * tile.height;
            let page_right = row.x + tile.x + (right + 1) as f32 / width as f32 * tile.width;
            let page_bottom = row.y + tile.y + (bottom + 1) as f32 / height as f32 * tile.height;
            ink = Some(match ink {
                Some((left, top, right, bottom)) => (
                    left.min(page_left),
                    top.min(page_top),
                    right.max(page_right),
                    bottom.max(page_bottom),
                ),
                None => (page_left, page_top, page_right, page_bottom),
            });
        }
    }
    let (left, top, right, bottom) =
        ink.unwrap_or_else(|| panic!("the shaped tiles for {label} contain ink"));
    let origin = fresh_document_origin(app);
    (
        origin.0 + (left + right) / 2.0 * zoom,
        origin.1 + (top + bottom) / 2.0 * zoom,
    )
}

fn dispatch_press_only(app: &crate::WriterApp, point: (f32, f32)) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(point.0, point.1),
            button: slint::platform::PointerEventButton::Left,
        });
}

fn dispatch_release_only(app: &crate::WriterApp, point: (f32, f32)) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(point.0, point.1),
            button: slint::platform::PointerEventButton::Left,
        });
}

/// A window with the page rendered at `width` x `height` and `scale`, the
/// table's rectangles published, and the page focused as at launch.
fn rendered_window(
    document: WriterDocument,
    width: u32,
    height: u32,
    scale: f32,
    scroll_y: f32,
) -> (crate::WriterApp, std::rc::Rc<crate::GuiState>) {
    let (app, state) = crate::actions_tests::test_state(
        document,
        std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
    );
    app.window()
        .set_size(slint::PhysicalSize::new(width, height));
    crate::wire_writer_shared_callbacks(&app, &state, None);
    state.viewport.borrow_mut().scroll_y = scroll_y;
    crate::apply_state(&app, &state);
    let _ =
        loom_test_support::capture::snapshot_component(&app, width as f32, height as f32, scale)
            .expect("render the page before pointer input");
    crate::focus_page_at_launch(&app);
    (app, state)
}

/// A pointer press at the centre of every projected cell rectangle must land
/// inside that same cell, using the published rectangle model as the page does.
fn assert_each_cell_centre_hits_its_own_cell(rows: usize, cols: usize) {
    crate::table_actions::clear_table_cell_selection_range();
    let document = table_document(rows, cols);
    let viewport = PageViewport::default();
    let projection = projection::project(&document, viewport, Some(PageStyle::default().height_pt));
    assert_eq!(
        projection.table_cell_rects.len(),
        rows * cols,
        "the table projects one rectangle per cell"
    );
    let model = ModelRc::new(VecModel::from(projection.table_cell_rects.clone()));

    let mut misses = Vec::new();
    for (index, rect) in projection.table_cell_rects.iter().enumerate() {
        let (row, col) = (index / cols, index % cols);
        let x = rect.x + rect.width / 2.0;
        let y = rect.y + rect.height / 2.0;
        let hit = writer_pointer_offset_with_model(&document, viewport, x, y, &model)
            .expect("a press inside a table cell resolves to a document offset");
        let location = document
            .table_cell_at_offset(hit)
            .expect("the resolved offset stays inside the table");
        if (location.row, location.col) != (row, col) {
            misses.push(format!(
                "pressing centre of r{row}c{col} at ({x:.1}, {y:.1}) hit r{}c{}",
                location.row, location.col
            ));
        }
    }
    assert!(
        misses.is_empty(),
        "{} of {} cell centres hit the wrong cell; first: {:?}",
        misses.len(),
        rows * cols,
        misses.first()
    );
}

#[test]
fn every_cell_centre_of_a_three_by_three_table_hits_that_cell() {
    assert_each_cell_centre_hits_its_own_cell(3, 3);
}

#[test]
fn typed_table_projection_text_caret_and_hit_follow_rows_across_page_breaks() {
    let _serial = loom_writer_core::test_fonts::serial();
    let _reset = ResetTestFontCatalog;
    let mut config = loom_writer_core::fonts::ScanConfig::with_dirs(Vec::new());
    config.policy = loom_writer_core::fonts::FallbackPolicy::new(["Inter"]);
    loom_writer_core::fonts::install_font_catalog(std::sync::Arc::new(
        loom_writer_core::fonts::FontCatalog::scan(&config),
    ));

    let (rows, cols) = (40, 2);
    let mut table = loom_objects::table::Table::new(rows, cols);
    for row in 0..rows {
        for col in 0..cols {
            table.set_cell_text(row, col, "x");
        }
    }
    let mut document = WriterDocument::new("table-page-boundary", "Table page boundary");
    let table_id = document
        .insert_table_object(0, &table)
        .expect("insert the typed table");
    let style = document.page.page_style();
    let viewport = PageViewport::default();
    let flow = document
        .flow(&style, projection::layout_viewport(&style, viewport))
        .expect("the table flows over multiple pages");
    let flow_rows: Vec<_> = flow
        .lines()
        .iter()
        .filter(|line| line.block_index == 0)
        .collect();
    assert_eq!(flow_rows.len(), rows, "one flow line is reserved per row");
    let second_page_row = flow_rows
        .iter()
        .position(|line| line.page > 0)
        .expect("the tall table crosses a page boundary");

    let projection = projection::project(&document, viewport, None);
    let first_page = flow
        .page_bounds()
        .first()
        .copied()
        .expect("flow has its first page");
    let zoom = flow.zoom();
    let cell_ranges = document
        .table_cell_ranges(table_id)
        .expect("typed cells have source ranges");
    for row_index in 0..rows {
        let flow_line = flow_rows[row_index];
        let expected_y = (flow_line.bounds.y - first_page.y - style.margin_top_pt * zoom) / zoom;
        let rect = &projection.table_cell_rects[row_index * cols];
        assert!(
            (rect.y - expected_y).abs() < 0.05,
            "cell row {row_index} rectangle y {} follows its flow row y {expected_y}",
            rect.y
        );
        assert!(
            (rect.height - flow_line.bounds.height / zoom).abs() < 0.05,
            "cell row {row_index} rectangle height matches its flow row"
        );
        let text_row = projection
            .rows
            .iter()
            .filter(|row| row.table_cell)
            .nth(row_index * cols)
            .expect("the projected text row exists");
        assert!(
            (text_row.y - (expected_y + 4.0)).abs() < 0.05,
            "cell row {row_index} glyph top follows its flow row"
        );
        let cell_offset = cell_ranges[row_index][0].0;
        let (caret_top, caret_bottom) =
            projection::table_row_view_span(&document, &style, &flow, cell_offset)
                .expect("caret geometry uses the table row span");
        assert!(
            (caret_top - flow_line.bounds.y).abs() < 0.05
                && (caret_bottom - (flow_line.bounds.y + flow_line.bounds.height)).abs() < 0.05,
            "caret span for row {row_index} follows its paginated flow bounds"
        );
    }

    let flow_line = flow_rows[second_page_row];
    let rect = &projection.table_cell_rects[second_page_row * cols];
    let expected_y = (flow_line.bounds.y - first_page.y - style.margin_top_pt * zoom) / zoom;
    let cell_ranges = document
        .table_cell_ranges(table_id)
        .expect("typed cells have source ranges");
    let model = ModelRc::new(VecModel::from(projection.table_cell_rects.clone()));
    let hit = super::writer_pointer_offset_with_model(
        &document,
        viewport,
        rect.x + 7.0,
        expected_y + flow_line.bounds.height / 2.0,
        &model,
    )
    .expect("pointer on the second-page row finds document text");
    let location = document
        .table_cell_at_offset(hit)
        .expect("pointer hit resolves to a table cell");
    assert_eq!(
        (location.row, location.col),
        (second_page_row, 0),
        "pointer hit at the paginated glyph row selects the matching cell"
    );
    assert_eq!(cell_ranges[second_page_row][0].0, location.cell_start);
}

#[test]
fn mixed_script_table_cell_pointer_uses_the_painted_shaped_run_positions() {
    let _serial = loom_writer_core::test_fonts::serial();
    let latin_start = "مرحبا ".len();
    let mut table = loom_objects::table::Table::new(1, 1);
    table.set_cell_rich_text(
        0,
        0,
        vec![
            loom_objects::table::TextRun::plain("مرحبا "),
            loom_objects::table::TextRun {
                text: "office".to_owned(),
                bold: true,
                font_size_pt: Some(18.0),
                color: Some("#CC3300FF".to_owned()),
                ..loom_objects::table::TextRun::plain("")
            },
            loom_objects::table::TextRun::plain(" שלום"),
        ],
    );
    let mut document = WriterDocument::new("mixed-cell-hit", "Mixed cell hit");
    let table_id = document
        .insert_table_object(0, &table)
        .expect("insert typed table");
    let cell = document
        .table_cell_at_offset(0)
        .expect("the first table cell has an editor offset");
    let viewport = PageViewport::default();
    let projection = projection::project(&document, viewport, Some(PageStyle::default().height_pt));
    let rect = projection
        .table_cell_rects
        .first()
        .cloned()
        .expect("one projected cell rectangle");
    let model = ModelRc::new(VecModel::from(projection.table_cell_rects.clone()));
    let table_style = loom_objects::TableStyleCatalog::get(&table.style_id)
        .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
    let style_options = loom_objects::TableStyleOptions {
        header_row: table.header_row,
        banded_rows: table.banded_rows,
        first_column: table.first_column,
        ..Default::default()
    };
    let cell_style = table_style.resolve_cell(
        0,
        0,
        table.row_count(),
        table.col_count(),
        &style_options,
        None,
    );
    let cell_block = loom_writer_core::table_cell_rich_block(
        table_id,
        &table,
        0,
        0,
        12.0,
        cell_style.bold,
        cell_style.text_color.as_deref(),
    )
    .expect("cell text converts to its shared rich block");
    let lines = loom_writer_core::shape_wrapped_block_lines(&cell_block, 12.0, rect.width - 12.0)
        .expect("mixed-direction cell text has shaped lines");
    let line_index = lines
        .iter()
        .position(|(range, _)| range.start <= latin_start + 3 && latin_start + 3 <= range.end)
        .expect("the Latin run belongs to one visual line");
    let (line_range, line) = &lines[line_index];
    let line = line.as_ref().expect("the visual line has glyph geometry");
    let target = latin_start + 3;
    let x = rect.x + 6.0 + line.caret_x(target, Affinity::Leading);
    let max_line_height = lines
        .iter()
        .filter_map(|(_, line)| line.as_ref())
        .map(|line| line.ascent_pt() + line.descent_pt() + line.line_gap_pt())
        .reduce(f32::max)
        .unwrap_or(0.0);
    let line_pitch = (loom_writer_core::line_pitch(&cell_block.runs, 12.0)
        * loom_writer_core::TABLE_CELL_LINE_PITCH_EM)
        .max(max_line_height);
    let y = rect.y + 4.0 + (line_index as f32 + 0.5) * line_pitch;
    let hit = writer_pointer_offset_with_model(&document, viewport, x, y, &model)
        .expect("the shaped cell hit resolves to a document offset");
    let expected_local = line
        .hit_test(x - rect.x - 6.0)
        .expect("the point has a shaped caret stop")
        .offset;
    assert_eq!(
        expected_local, target,
        "the chosen point hits its caret stop"
    );
    assert!(line_range.contains(&expected_local) || expected_local == line_range.end);
    assert_eq!(
        hit,
        cell.cell_start + expected_local,
        "the pointer maps exact mixed-script glyph geometry back into table text"
    );
}

#[test]
fn leading_cell_whitespace_pointer_uses_the_original_cell_byte_offset() {
    let _serial = loom_writer_core::test_fonts::serial();
    let mut table = loom_objects::Table::new(1, 1);
    table.set_cell_text(0, 0, "  office  ");
    let mut document = WriterDocument::new("leading-cell-space", "Leading cell space");
    document
        .insert_table_object(0, &table)
        .expect("insert typed table");
    let cell = document
        .table_cell_at_offset(0)
        .expect("the first table cell has an editor offset");
    let viewport = PageViewport::default();
    let projection = projection::project(&document, viewport, Some(PageStyle::default().height_pt));
    let rect = projection
        .table_cell_rects
        .first()
        .cloned()
        .expect("one projected cell rectangle");
    let model = ModelRc::new(VecModel::from(projection.table_cell_rects.clone()));
    let typed_table = document
        .loom_table_from_block(cell.block_id)
        .expect("typed table remains available");
    let cell_block = loom_writer_core::table_cell_rich_block(
        cell.block_id,
        &typed_table,
        cell.row,
        cell.col,
        12.0,
        false,
        None,
    )
    .expect("whitespace is retained in the cell geometry");
    assert_eq!(cell_block.text.as_str(), "  office  ");
    let lines = loom_writer_core::shape_wrapped_block_lines(&cell_block, 12.0, rect.width - 12.0)
        .expect("the cell has shaped line geometry");
    let (range, line) = lines.first().expect("one visual line");
    let line = line.as_ref().expect("the visual line has glyph geometry");
    let local_target = 2;
    let x = rect.x + 6.0 + line.caret_x(local_target, Affinity::Leading);
    let y = rect.y + 4.0 + 7.5;
    let hit = writer_pointer_offset_with_model(&document, viewport, x, y, &model)
        .expect("a shaped cell hit resolves to a document offset");
    assert_eq!(range.start, 0);
    assert_eq!(
        hit,
        cell.cell_start + local_target,
        "leading whitespace is represented once in the source byte map"
    );
}

#[test]
fn every_cell_centre_of_a_hundred_by_twenty_table_hits_that_cell() {
    assert_each_cell_centre_hits_its_own_cell(100, 20);
}

#[test]
fn a_table_range_marks_exactly_its_cells_as_painted_selected() {
    crate::table_actions::clear_table_cell_selection_range();
    let mut document = table_document(3, 3);
    let table_id = 1;
    let first_cell = document
        .table_cell_at_offset(0)
        .expect("offset zero is the first table cell");
    document.set_selection(TextSelection::caret(first_cell.cell_start));
    crate::table_actions::set_table_cell_selection_range(Some(
        crate::table_actions::TableCellRange {
            block_id: table_id,
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        },
    ));

    let projection = projection::project(
        &document,
        PageViewport::default(),
        Some(PageStyle::default().height_pt),
    );
    crate::table_actions::clear_table_cell_selection_range();

    let cols = 3;
    assert_eq!(projection.table_cell_rects.len(), 9);
    for (index, rect) in projection.table_cell_rects.iter().enumerate() {
        let (row, col) = (index / cols, index % cols);
        let expected = row <= 1 && col <= 1;
        assert_eq!(
            rect.selected, expected,
            "cell r{row}c{col} painted-selected should be {expected}"
        );
    }
}

#[path = "table_range_render_tests.rs"]
mod table_range_render_tests;

/// Every press in a 3x3 table, at each window size and scale factor, selects
/// the cell under the pointer. The editor origin is measured for each window.
#[test]
fn window_presses_on_a_three_by_three_table_select_the_pressed_cell_at_every_size_and_scale() {
    crate::table_actions::clear_table_cell_selection_range();
    let mut report = Vec::new();
    for (width, height) in [(1024, 720), (1280, 800), (1440, 900), (1920, 1200)] {
        for scale in [1.0_f32, 1.5, 2.0] {
            let (document, table_id) = page_with_table(3, 3);
            let (app, state) = rendered_window(document, width, height, scale, 0.0);
            let rects = app.get_table_cell_rects();
            let origin = fresh_document_origin(&app);
            for row in 0..3 {
                for col in 0..3 {
                    let rect = rects
                        .row_data(row * 3 + col)
                        .expect("one published rectangle per cell");
                    let point = (
                        origin.0 + rect.x + rect.width / 2.0,
                        origin.1 + rect.y + rect.height / 2.0,
                    );
                    crate::table_actions::clear_table_cell_selection_range();
                    dispatch_window_press(&app, point);
                    let got = focused_cell(&state, table_id);
                    if got != Some((row, col)) {
                        report.push(format!(
                            "{width}x{height} @{scale}: pressed r{row}c{col} at {point:?} -> {got:?}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        report.is_empty(),
        "{} window presses selected the wrong cell; first: {:?}",
        report.len(),
        report.first()
    );
}

/// On a 100x20 table, presses on visible cells select the pressed cell even
/// after a press scrolls the page to reveal the caret. Each press re-measures
/// the editor origin, because the page moves between presses.
#[test]
fn long_table_presses_on_visible_cells_select_the_pressed_cell() {
    let _serial = loom_writer_core::test_fonts::serial();
    crate::table_actions::clear_table_cell_selection_range();
    let (rows, cols) = (100, 20);
    let (document, table_id) = page_with_table(rows, cols);
    let (app, state) = rendered_window(document, 1280, 800, 1.0, 0.0);
    let rects = app.get_table_cell_rects();

    let origin = fresh_document_origin(&app);
    let targets: Vec<_> = (0..rows)
        .filter_map(|row| {
            // Vary the column for each visible row so the long-table probe
            // exercises horizontal and vertical hit geometry.
            let col = (row * 7) % cols;
            let rect = rects
                .row_data(row * cols + col)
                .expect("one published rectangle per cell");
            let point = (
                origin.0 + rect.x + rect.width / 2.0,
                origin.1 + rect.y + rect.height / 2.0,
            );
            ((VISIBLE_TOP..VISIBLE_BOTTOM).contains(&point.1) && (0.0..1280.0).contains(&point.0))
                .then_some((row, col))
        })
        .take(5)
        .collect();
    assert!(
        targets.len() >= 5,
        "at least five actual row/column centers are visible in the long-table viewport ({})",
        targets.len()
    );
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (row, col) in targets {
        let rect = app
            .get_table_cell_rects()
            .row_data(row * cols + col)
            .expect("one published rectangle per cell");
        let origin = fresh_document_origin(&app);
        let point = (
            origin.0 + rect.x + rect.width / 2.0,
            origin.1 + rect.y + rect.height / 2.0,
        );
        if !(VISIBLE_TOP..VISIBLE_BOTTOM).contains(&point.1) {
            continue;
        }
        checked += 1;
        crate::table_actions::clear_table_cell_selection_range();
        dispatch_window_press(&app, point);
        let got = focused_cell(&state, table_id);
        if got != Some((row, col)) {
            failures.push(format!(
                "pressed r{row}c{col} at {point:?} selected {got:?}"
            ));
        }
    }
    assert!(
        checked >= 5,
        "the window shows enough cells to press ({checked})"
    );
    assert!(
        failures.is_empty(),
        "{} visible presses selected a different cell; first: {:?}",
        failures.len(),
        failures.first()
    );
}

/// A press that scrolls the page near the top edge must not change which cell
/// the release selects: the release is handled after the page re-lays out.
#[test]
fn press_then_release_near_the_top_edge_keeps_the_pressed_cell() {
    let _serial = loom_writer_core::test_fonts::serial();
    crate::table_actions::clear_table_cell_selection_range();
    let (rows, cols) = (100, 20);
    let (document, table_id) = page_with_table(rows, cols);
    let (app, state) = rendered_window(document, 1280, 800, 1.0, 240.0);
    let rects = app.get_table_cell_rects();

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for row in 0..rows {
        let col = 2;
        let rect = rects
            .row_data(row * cols + col)
            .expect("one published rectangle per cell");
        let origin = fresh_document_origin(&app);
        let point = (
            origin.0 + rect.x + rect.width / 2.0,
            origin.1 + rect.y + rect.height / 2.0,
        );
        if !(VISIBLE_TOP..VISIBLE_BOTTOM).contains(&point.1) {
            continue;
        }
        checked += 1;
        crate::table_actions::clear_table_cell_selection_range();
        dispatch_press_only(&app, point);
        let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
            .expect("lay out the page between press and release");
        dispatch_release_only(&app, point);
        let got = focused_cell(&state, table_id);
        if got != Some((row, col)) {
            failures.push(format!(
                "press and release on r{row}c{col} at {point:?} selected {got:?}"
            ));
        }
    }
    assert!(
        checked >= 3,
        "the scrolled page shows cells to press ({checked})"
    );
    assert!(
        failures.is_empty(),
        "{} press-then-release cells selected a different cell; first: {:?}",
        failures.len(),
        failures.first()
    );
}

/// The document `--table` renders: a Region / Sessions / Change table with
/// North and South rows, as a native check saw it.
fn sample_table_document() -> (WriterDocument, u64) {
    let mut document = WriterDocument::new("sample-table", "Sample table");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "Sample prose"));
    let table_id = document
        .insert_table_block(
            usize::MAX,
            loom_writer_core::INSERT_ROWS,
            loom_writer_core::INSERT_COLUMNS,
        )
        .expect("insert the sample table");
    let mut table = document
        .table_from_block(table_id)
        .expect("the sample table parses");
    let sample = [
        ["Region", "Sessions", "Change"],
        ["North", "1,204", "+8.1%"],
        ["South", "987", "+5.4%"],
    ];
    for (row, values) in sample.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            table.set(row, column, *value);
        }
    }
    document
        .blocks
        .iter_mut()
        .find(|block| block.id == table_id)
        .expect("the inserted table block")
        .set_table(table.to_loom_table());
    (document, table_id)
}

/// The native check's table (North +8.1%, South +5.4%): every visible cell
/// selects itself after a press, a re-layout and a release, at several scrolls.
#[test]
fn the_sample_table_selects_the_pressed_cell_after_relayout_at_each_scroll() {
    let mut failures = Vec::new();
    for scroll in [0.0_f32, 30.0, 60.0] {
        let (document, table_id) = sample_table_document();
        let (app, state) = rendered_window(document, 1280, 800, 1.0, scroll);
        let rects = app.get_table_cell_rects();
        let mut checked = 0;
        for row in 0..3 {
            for col in 0..3 {
                let rect = rects.row_data(row * 3 + col).expect("rect");
                let origin = fresh_document_origin(&app);
                let point = (
                    origin.0 + rect.x + rect.width / 2.0,
                    origin.1 + rect.y + rect.height / 2.0,
                );
                if !(VISIBLE_TOP..VISIBLE_BOTTOM).contains(&point.1) {
                    continue;
                }
                checked += 1;
                crate::table_actions::clear_table_cell_selection_range();
                dispatch_press_only(&app, point);
                let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0);
                dispatch_release_only(&app, point);
                let got = focused_cell(&state, table_id);
                if got != Some((row, col)) {
                    failures.push(format!("r{row}c{col} at y {:.1} -> {got:?}", point.1));
                }
            }
        }
        assert!(
            checked >= 6,
            "the sample table shows its cells at scroll {scroll} ({checked})"
        );
    }
    assert!(
        failures.is_empty(),
        "{} sample-table presses selected a different cell; first: {:?}",
        failures.len(),
        failures.first()
    );
}

/// The published rectangle `(x, y, width, height)` that paints `cell` of table
/// `table_id`, in the page canvas's own coordinates: the coordinates its pointer
/// callbacks receive. `None` when no painted rectangle belongs to that cell.
fn painted_cell_rect(
    app: &crate::WriterApp,
    state: &crate::GuiState,
    table_id: u64,
    cell: (usize, usize),
) -> Option<(f32, f32, f32, f32)> {
    let rects = app.get_table_cell_rects();
    let current = state.current.borrow();
    let rect = (0..rects.row_count())
        .filter_map(|index| rects.row_data(index))
        .find(|rect| {
            usize::try_from(rect.caret_offset)
                .ok()
                .and_then(|offset| current.table_cell_at_offset(offset))
                .is_some_and(|location| {
                    location.block_id == table_id && (location.row, location.col) == cell
                })
        })?;
    Some((rect.x, rect.y, rect.width, rect.height))
}

/// A held drag below the visible bottom edge of a long table scrolls the page
/// to follow the pointer, so the focus stays on a visible line. The release at
/// the same point then keeps the dragged focus: after the scroll that point
/// lies over another cell, and a release that recomputed the selection would
/// move the focus there.
///
/// The pointer callbacks are invoked directly, as `picture_drag_tests` does. A
/// window press on the page waits out the surrounding Flickable's forwarding
/// delay, which the test window does not run, so it cannot hold a drag open.
#[test]
fn a_held_drag_below_the_visible_bottom_scrolls_the_page_to_keep_the_focus_visible() {
    crate::table_actions::clear_table_cell_selection_range();
    let (rows, cols) = (100, 20);
    let (document, table_id) = page_with_table(rows, cols);
    let (app, state) = rendered_window(document, 1280, WINDOW_HEIGHT as u32, 1.0, 0.0);

    // The canvas reports pointer positions relative to the document origin.
    let origin = fresh_document_origin(&app);
    let (x, y, width, height) = painted_cell_rect(&app, &state, table_id, (2, 2))
        .expect("row 2 of the long table is painted near the top of the page");
    let start = (x + width / 2.0, y + height / 2.0);
    assert!(
        (VISIBLE_TOP..VISIBLE_BOTTOM).contains(&(origin.1 + start.1)),
        "the press starts on a visible cell, at {start:?} from origin {origin:?}"
    );
    app.invoke_pointer_pressed(start.0, start.1, false);
    assert_eq!(
        focused_cell(&state, table_id),
        Some((2, 2)),
        "the press selects its cell"
    );

    // Hold the button and drag to a point below the bottom of the window, which
    // is below the visible page at the start.
    let drag_to = (start.0, WINDOW_HEIGHT + 40.0 - origin.1);
    app.invoke_pointer_moved(drag_to.0, drag_to.1);
    let scroll = state.viewport.borrow().scroll_y;
    assert!(
        scroll > 0.0,
        "a drag below the visible page scrolls it to follow the pointer; scroll is {scroll}"
    );
    let focus = focused_cell(&state, table_id).expect("the dragged focus is inside the table");
    assert_ne!(focus, (2, 2), "the drag moved the focus down the table");
    let (_, focus_y, _, focus_height) = painted_cell_rect(&app, &state, table_id, focus)
        .expect("the dragged-to cell is painted after the page scrolls");
    // Lay the scrolled page out again: the editor's position is only current after a layout.
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, WINDOW_HEIGHT, 1.0)
        .expect("lay out the page after the drag scrolls it");
    let origin_now = fresh_document_origin(&app);
    let (top, bottom) = (
        origin_now.1 + focus_y,
        origin_now.1 + focus_y + focus_height,
    );
    assert!(
        top >= 0.0 && bottom <= WINDOW_HEIGHT,
        "the focus {focus:?} stays inside the window, at y {top:.1} to {bottom:.1}"
    );

    app.invoke_pointer_released(drag_to.0, drag_to.1);
    assert_eq!(
        focused_cell(&state, table_id),
        Some(focus),
        "releasing at the same point keeps the dragged focus"
    );
}

/// The painted text of each cell must sit inside that cell's own rectangle, and a
/// press on that painted text must select the cell. The presses above aim at
/// centres computed from the published rectangles, so they cannot see a cell
/// that is drawn somewhere other than the model places it. These tests read the
/// rendered tree and press on the painted text itself, through the pointer
/// surface, at each display scale and each page zoom.
#[test]
fn a_press_on_each_painted_cell_text_selects_that_cell_at_every_scale() {
    for scale in [1.0_f32, 1.5, 2.0] {
        let (app, state) = rendered_window(table_document(3, 3), 1280, 800, scale, 0.0);
        assert_painted_cells_select_themselves(&app, &state, 1.0, &format!("scale {scale}"));
    }
}

#[test]
fn a_press_on_each_painted_cell_text_selects_that_cell_at_every_zoom() {
    for zoom in [0.75_f32, 1.5, 2.0] {
        let (app, state) = crate::actions_tests::test_state(
            table_document(3, 3),
            std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
        );
        app.window().set_size(slint::PhysicalSize::new(1280, 800));
        crate::wire_writer_shared_callbacks(&app, &state, None);
        app.invoke_page_zoom_changed(zoom);
        crate::apply_state(&app, &state);
        let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
            .expect("render the page before pointer input");
        crate::focus_page_at_launch(&app);
        assert_painted_cells_select_themselves(&app, &state, zoom, &format!("zoom {zoom}"));
    }
}

/// For each cell of a 3x3 table on the rendered page: alpha-bearing pixels in
/// its shaped glyph tiles must lie inside its rectangle at `zoom`, and a press
/// on those painted pixels must select the cell.
fn assert_painted_cells_select_themselves(
    app: &crate::WriterApp,
    state: &crate::GuiState,
    zoom: f32,
    condition: &str,
) {
    let rects = app.get_table_cell_rects();
    for row in 0..3 {
        for col in 0..3 {
            let label = format!("r{row}c{col}");
            let centre = painted_cell_ink_center(app, &label, zoom);
            let (origin_x, origin_y) = fresh_document_origin(app);
            let rect = rects
                .row_data(row * 3 + col)
                .expect("one rectangle per cell");
            let (left, top) = (origin_x + rect.x * zoom, origin_y + rect.y * zoom);
            let (right, bottom) = (left + rect.width * zoom, top + rect.height * zoom);
            assert!(
                centre.0 >= left && centre.0 < right && centre.1 >= top && centre.1 < bottom,
                "{label} at {condition}: its painted glyph-ink centre is at {centre:?}, \
                 outside its rectangle {left}..{right} by {top}..{bottom}"
            );
            dispatch_window_press(app, centre);
            assert_eq!(
                focused_cell(state, 1),
                Some((row, col)),
                "a press on actual rasterized ink for {label} at {condition} selects that cell"
            );
        }
    }
}

/// Page Down from a table cell moves the caret a screen down and keeps its column,
/// as Down does. The landing point comes from the caret's flat-text x, which says
/// nothing about the cell's column, so the caret can land in another column.
#[test]
fn page_down_from_a_table_cell_keeps_its_column() {
    let mut table = WriterTable::new("page-down-column", 60, 3);
    for row in 0..60 {
        for col in 0..3 {
            table.set(row, col, format!("r{row}c{col}"));
        }
    }
    let mut document = WriterDocument::new("page-down-column", "Page down column");
    document.blocks.push(loom_writer_core::markdown_table_block(
        1,
        &table.to_markdown(),
    ));
    let (app, state) = rendered_window(document, 1280, 800, 1.0, 0.0);
    let start = state
        .current
        .borrow()
        .table_cell_ranges(1)
        .expect("the table has cell ranges")[1][2]
        .0;
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(start));
    app.invoke_page_move(1, false);
    let landed = focused_cell(&state, 1).expect("the caret is still in the table");
    assert_eq!(
        landed.1, 2,
        "Page Down keeps the column; it landed in {landed:?}"
    );
    assert!(landed.0 > 1, "and moves down the table: {landed:?}");

    app.invoke_page_move(-1, false);
    let back = focused_cell(&state, 1).expect("the caret is still in the table");
    assert_eq!(
        back.1, 2,
        "Page Up keeps the column too; it landed in {back:?}"
    );
    assert!(
        back.0 < landed.0,
        "and moves back up: {back:?} after {landed:?}"
    );
}

/// Ctrl+End in a table lands the caret at the end of its last cell. The page
/// must scroll that cell's painted row into view. The core line the table text
/// is laid on is taller than the painted row, so a reveal measured from the core
/// line would scroll the page past the cell.
#[test]
fn ctrl_end_in_a_long_table_keeps_the_caret_cell_inside_the_window() {
    crate::table_actions::clear_table_cell_selection_range();
    let (rows, cols) = (100, 20);
    let document = table_document(rows, cols);
    let ranges = document
        .table_cell_ranges(document.blocks[0].id)
        .expect("table ranges");
    let end = ranges[rows - 1][cols - 1].1;
    let table_id = 1;
    let (app, state) = rendered_window(document, 1280, WINDOW_HEIGHT as u32, 1.0, 0.0);

    app.invoke_selection_changed(end as i32, end as i32);
    assert_eq!(
        focused_cell(&state, table_id),
        Some((rows - 1, cols - 1)),
        "the caret lands in the table's last cell"
    );
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, WINDOW_HEIGHT, 1.0)
        .expect("lay out the page after the caret moves");
    let origin = fresh_document_origin(&app);
    let (_, y, _, height) = painted_cell_rect(&app, &state, table_id, (rows - 1, cols - 1))
        .expect("the last cell is painted after the page scrolls to it");
    let (top, bottom) = (origin.1 + y, origin.1 + y + height);
    let scroll = state.viewport.borrow().scroll_y;
    assert!(
        top >= 0.0 && bottom <= WINDOW_HEIGHT,
        "the caret cell stays inside the window, at y {top:.1} to {bottom:.1} after scrolling to {scroll}"
    );
}
