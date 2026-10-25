use super::*;

struct MockClipboardGuard;

impl MockClipboardGuard {
    fn new() -> Self {
        loom_clipboard::set_mock_clipboard_enabled(true);
        loom_clipboard::set_mock_clipboard_fails(false);
        Self
    }
}

impl Drop for MockClipboardGuard {
    fn drop(&mut self) {
        loom_clipboard::set_mock_clipboard_fails(false);
        loom_clipboard::set_mock_clipboard_enabled(false);
    }
}

fn cut_test_app(document: loom_writer_core::WriterDocument) -> (WriterApp, std::rc::Rc<GuiState>) {
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = crate::actions_tests::test_state(document, dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    (app, state)
}

fn tiny_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 80, 120, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode a small picture fixture");
    bytes
}

fn block_range(document: &loom_writer_core::WriterDocument, block_id: u64) -> (usize, usize) {
    let mut start = 0;
    for block in &document.blocks {
        let end = start + block.text.as_str().len();
        if block.id == block_id {
            return (start, end);
        }
        start = end + 1;
    }
    panic!("block {block_id} exists in the editor text");
}

fn populated_document_with_table(
    document_id: &str,
    title: &str,
    rows: usize,
    columns: usize,
    body_text: &str,
    cell_prefix: &str,
) -> (loom_writer_core::WriterDocument, u64, usize, usize) {
    let mut document = loom_writer_core::WriterDocument::new(document_id, title);
    document.replace_paragraphs(&format!("Intro\n{body_text}"));
    let table_id = document.insert_table_block(0, rows, columns).unwrap();
    let mut table = document.table_from_block(table_id).unwrap();
    for row in 0..rows {
        for column in 0..columns {
            table.set(row, column, format!("{cell_prefix}-{row}-{column}"));
        }
    }
    document
        .blocks
        .iter_mut()
        .find(|block| block.id == table_id)
        .unwrap()
        .set_table(table.to_loom_table());

    let editor_text = document.editor_text();
    let first_cell_offset = (0..=editor_text.len())
        .find_map(|offset| document.table_cell_at_offset(offset))
        .map(|cell| cell.cell_start)
        .expect("first table cell offset");
    let body_end = editor_text
        .find(body_text)
        .map(|start| start + body_text.len())
        .expect("body paragraph follows the table");
    document.set_selection(loom_writer_core::TextSelection::caret(first_cell_offset));
    (document, table_id, first_cell_offset, body_end)
}

fn table_cell_offset(
    document: &loom_writer_core::WriterDocument,
    row: usize,
    column: usize,
) -> usize {
    let text = document.editor_text();
    (0..=text.len())
        .find(|offset| {
            document
                .table_cell_at_offset(*offset)
                .is_some_and(|location| location.row == row && location.col == column)
        })
        .unwrap_or_else(|| panic!("table cell ({row}, {column}) has an editor offset"))
}

fn select_top_left_two_by_two(
    app: &WriterApp,
    state: &GuiState,
    table_id: u64,
    table_offset: usize,
) {
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    let right: slint::SharedString = slint::platform::Key::RightArrow.into();
    let down: slint::SharedString = slint::platform::Key::DownArrow.into();
    assert!(crate::table_actions::handle_table_key(
        app,
        state,
        right.as_str(),
        true,
        false,
        false
    ));
    assert!(crate::table_actions::handle_table_key(
        app,
        state,
        down.as_str(),
        true,
        false,
        false
    ));
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        Some(crate::table_actions::TableCellRange {
            block_id: table_id,
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        }),
        "the intended 2x2 cell range is established before the pointer event"
    );
}

fn pointer_position_for_offset(
    document: &loom_writer_core::WriterDocument,
    target_offset: usize,
) -> (f32, f32) {
    let style = document.page.page_style();
    let flow = document
        .flow(
            &style,
            crate::PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .expect("layout table for pointer hit testing");
    let base_page = flow.page_bounds().first().copied().expect("first page");
    let line = flow
        .lines()
        .iter()
        .find(|line| {
            let end = line.global_start + (line.end - line.start);
            line.global_start <= target_offset && target_offset <= end
        })
        .expect("target cell is on a rendered line");
    let block = &document.blocks[line.block_index];
    let target_local_offset = line.start + target_offset - line.global_start;
    let content_width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
    let alignment_offset = match block.style.alignment.resolve(block.text.as_str()) {
        loom_text::Alignment::Center => ((content_width - line.bounds.width) / 2.0).max(0.0),
        loom_text::Alignment::Right => (content_width - line.bounds.width).max(0.0),
        loom_text::Alignment::Left
        | loom_text::Alignment::Justify
        | loom_text::Alignment::Start => 0.0,
    };
    let line_x = line.bounds.x - base_page.x - style.margin_left_pt + alignment_offset;
    let line_y = line.bounds.y - base_page.y - style.margin_top_pt + line.bounds.height / 2.0;
    let line_text = &block.text.as_str()[line.start..line.end];
    let font_size = style.font_size_for_kind(block.kind.as_str());
    let local_x = (0..=(line.bounds.width.max(0.0) * 4.0).ceil() as usize)
        .map(|step| step as f32 / 4.0)
        .find(|x| {
            loom_writer_core::offset_at_x(line_text, line.start, &block.runs, font_size, *x)
                == target_local_offset
        })
        .expect("target cell offset has a pointer position");
    let point = (line_x + local_x, line_y);
    assert_eq!(
        crate::writer_pointer_offset(document, crate::PageViewport::default(), point.0, point.1),
        Some(target_offset),
        "the test pointer position resolves to the intended cell offset"
    );
    point
}

fn pointer_position_for_cell(
    document: &loom_writer_core::WriterDocument,
    table_id: u64,
    target_row: usize,
    target_column: usize,
) -> ((f32, f32), usize) {
    let target_offset = table_cell_offset(document, target_row, target_column);
    let target_location = document
        .table_cell_at_offset(target_offset)
        .expect("target cell has an editor offset");
    assert_eq!(
        (
            target_location.block_id,
            target_location.row,
            target_location.col
        ),
        (table_id, target_row, target_column),
        "the target offset belongs to the intended table cell"
    );

    // The painted cell's centre, in the page's own coordinates. A table row is one
    // line of text in the layout, so the painted rectangles are the reliable target.
    let columns = document
        .loom_table_from_block(table_id)
        .expect("the painted table exists")
        .col_count();
    let projection = crate::projection::project(
        document,
        crate::PageViewport::default(),
        Some(loom_writer_core::PageStyle::default().height_pt),
    );
    let rect = &projection.table_cell_rects[target_row * columns + target_column];
    let point = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let actual_offset =
        crate::writer_pointer_offset(document, crate::PageViewport::default(), point.0, point.1)
            .expect("the pointer hits the painted cell");
    assert_eq!(
        document.table_cell_at_offset(actual_offset).map(|actual| (
            actual.block_id,
            actual.row,
            actual.col
        )),
        Some((table_id, target_row, target_column)),
        "the painted centre of cell ({target_row}, {target_column}) hit-tests into that cell"
    );
    (point, actual_offset)
}

fn document_body_window_origin(app: &WriterApp) -> (f32, f32) {
    let label = app.get_document_accessible_label().to_string();
    let body =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(app, label.as_str())
            .find(|element| {
                element.accessible_role()
                    == Some(i_slint_backend_testing::AccessibleRole::TextInput)
            })
            .expect("the visible document editor has its current accessible body bounds");
    let origin = body.absolute_position();
    (origin.x, origin.y)
}

/// Window-space point derived from the cell rectangles the page renderer
/// publishes. This deliberately does not consult `writer_pointer_offset`:
/// the regression must prove that painted C3 receives a synthetic Slint
/// pointer event at its on-screen location.
fn painted_cell_window_geometry(
    app: &WriterApp,
    document: &loom_writer_core::WriterDocument,
    table_id: u64,
    target_row: usize,
    target_column: usize,
) -> ((f32, f32), (f32, f32, f32, f32)) {
    let table = document
        .table_from_block(table_id)
        .expect("the painted table exists");
    let projection = crate::projection::project(
        document,
        crate::PageViewport::default(),
        Some(app.get_page_view_height()),
    );
    let columns = table.columns();
    assert!(target_row < table.rows.len() && target_column < columns);
    assert_eq!(
        projection.table_cell_rects.len(),
        table.rows.len() * columns,
        "the fixture projects one painted rectangle per table cell"
    );
    let rect = &projection.table_cell_rects[target_row * columns + target_column];
    let origin = document_body_window_origin(app);
    let zoom = app.get_page_zoom();
    (
        (
            origin.0 + (rect.x + rect.width / 2.0) * zoom,
            origin.1 + (rect.y + rect.height / 2.0) * zoom,
        ),
        (rect.x, rect.y, rect.width, rect.height),
    )
}

fn dispatch_window_pointer_click(app: &WriterApp, point: (f32, f32)) {
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

fn dispatch_shifted_arrows(app: &WriterApp, arrows: &[slint::platform::Key]) {
    use slint::platform::WindowEvent;

    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Shift.into(),
    });
    for arrow in arrows {
        let text: slint::SharedString = (*arrow).into();
        app.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        app.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    }
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Shift.into(),
    });
}

fn dispatch_plain_arrow(app: &WriterApp, arrow: slint::platform::Key) {
    use slint::platform::WindowEvent;

    let text: slint::SharedString = arrow.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn cell_pixel(
    frame: &image::RgbaImage,
    app: &WriterApp,
    document: &loom_writer_core::WriterDocument,
    table_id: u64,
    row: usize,
    column: usize,
) -> [u8; 4] {
    let (_, (x, y, width, height)) =
        painted_cell_window_geometry(app, document, table_id, row, column);
    let origin = document_body_window_origin(app);
    let zoom = app.get_page_zoom();
    let sample_x = (origin.0 + (x + width * 0.8) * zoom).round() as u32;
    let sample_y = (origin.1 + (y + height * 0.6) * zoom).round() as u32;
    assert!(
        sample_x < frame.width() && sample_y < frame.height(),
        "sample point ({sample_x}, {sample_y}) lies in the captured window"
    );
    frame.get_pixel(sample_x, sample_y).0
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let channel = |value: u8| {
        let channel = f64::from(value) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn contrast_ratio(left: [u8; 3], right: [u8; 3]) -> f64 {
    let (left, right) = (relative_luminance(left), relative_luminance(right));
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

#[path = "clipboard_actions_cut_tests.rs"]
mod cut_tests;
#[path = "clipboard_table_interop_tests.rs"]
mod table_interop_tests;
#[path = "clipboard_table_range_tests.rs"]
mod table_range_tests;
