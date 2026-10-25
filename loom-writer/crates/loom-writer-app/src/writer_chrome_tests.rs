//! Status-bar wording and the contrast of the empty-page placeholder.
use super::actions_tests::text_document;
use super::scale_surfaces_tests::editor;
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use loom_test_support::capture::snapshot_component;

const PLACEHOLDER: &str = "Type your text here...";

#[test]
fn status_bar_counts_paragraphs_and_drops_the_offline_prefix() {
    let app = editor(1280.0, 800.0, 1.0);
    apply_document(&app, &text_document("Hello"));
    assert_eq!(
        app.get_status_left(),
        "1 word · 5 chars · 1 paragraph · ~1 min read"
    );
    assert_eq!(app.get_status_right(), "Caret at character 1");

    let two = text_document("First\n\nSecond");
    apply_document(&app, &two);
    assert!(
        app.get_status_left().contains(" paragraphs"),
        "got {}",
        app.get_status_left()
    );
    assert!(!app.get_status_left().contains("block"));
    assert!(!app.get_status_right().contains("Offline"));
}

#[test]
fn the_page_keeps_one_accessible_name_while_the_caret_moves() {
    let app = editor(1280.0, 800.0, 1.0);
    let mut document = text_document("Hello world");
    apply_document(&app, &document);
    document.set_selection(TextSelection::caret(6));
    apply_document(&app, &document);
    // The caret position is still reported, in the status bar rather than in
    // the page's name, which must not change with every keystroke.
    assert_eq!(app.get_status_right(), "Caret at character 7");
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let named_page = ElementHandle::find_by_accessible_label(&app, "Document body")
        .any(|element| element.accessible_role() == Some(AccessibleRole::TextInput));
    assert!(
        named_page,
        "the page is named 'Document body' after the caret moves"
    );
}

fn shows_text(app: &WriterApp, text: &str) -> bool {
    app.root_element()
        .query_descendants()
        .match_predicate({
            let text = text.to_string();
            move |element| {
                element
                    .accessible_label()
                    .is_some_and(|label| label.contains(text.as_str()))
            }
        })
        .find_all()
        .into_iter()
        .any(|element| element.size().height > 0.0)
}

#[test]
fn an_empty_document_shows_no_reading_time() {
    let empty = super::keyboard_flow_tests::launched("");
    let _ = snapshot_component(&empty.app, 1280.0, 800.0, 1.0).expect("render");
    assert_eq!(empty.app.get_reading_time_mins(), 0);
    assert!(
        !shows_text(&empty.app, "min read"),
        "an empty document has no '~1 min read' caption"
    );

    let short = super::keyboard_flow_tests::launched("Hello world");
    let _ = snapshot_component(&short.app, 1280.0, 800.0, 1.0).expect("render");
    assert_eq!(short.app.get_reading_time_mins(), 1);
    assert!(
        shows_text(&short.app, "~1 min read"),
        "short text reads as one minute"
    );
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let channel = |value: u8| {
        let c = f64::from(value) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Contrast between the darkest and lightest pixel of the placeholder's first
/// line as rendered on the empty page.
fn rendered_placeholder_contrast(theme: &str) -> f64 {
    let app = editor(1280.0, 800.0, 1.0);
    apply_theme(&app, theme);
    app.set_show_inspector(false);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let editor_element = app
        .root_element()
        .query_descendants()
        .match_predicate(|e| e.accessible_placeholder_text().as_deref() == Some(PLACEHOLDER))
        .find_first()
        .expect("the empty page shows its placeholder");
    let origin = editor_element.absolute_position();
    let size = editor_element.size();
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let (x0, y0) = (origin.x as u32, origin.y as u32);
    let x1 = (x0 + (size.width as u32).min(260)).min(image.width());
    let y1 = (y0 + 22).min(image.height());
    let mut darkest = [255u8; 3];
    let mut lightest = [0u8; 3];
    for y in y0..y1 {
        for x in x0..x1 {
            let px = image.get_pixel(x, y).0;
            let rgb = [px[0], px[1], px[2]];
            if relative_luminance(rgb) < relative_luminance(darkest) {
                darkest = rgb;
            }
            if relative_luminance(rgb) > relative_luminance(lightest) {
                lightest = rgb;
            }
        }
    }
    contrast(darkest, lightest)
}

#[test]
fn placeholder_meets_contrast_on_the_page_in_every_theme() {
    // WCAG AA for ordinary text in light and dark; AAA-level for high contrast.
    for (theme, floor) in [("light", 4.5), ("dark", 4.5), ("high-contrast", 7.0)] {
        let measured = rendered_placeholder_contrast(theme);
        assert!(
            measured >= floor,
            "{theme}: placeholder contrast {measured:.2}:1 is below {floor}:1"
        );
    }
}

#[test]
fn contrast_helper_matches_wcag_reference_values() {
    // Black on white is 21:1 by definition.
    assert!((contrast([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 0.01);
    // #6e6e73 on white is about 5.1:1.
    let grey = contrast([0x6e, 0x6e, 0x73], [255, 255, 255]);
    assert!((grey - 5.1).abs() < 0.1, "got {grey}");
}

/// Perceived brightness of one rendered pixel, 0 (black) to 255 (white).
fn luma(pixel: [u8; 4]) -> u32 {
    (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114) / 1000
}

/// A table of empty cells, so the only ink in its frame is its own lines.
fn empty_table_document(rows: usize, cols: usize) -> WriterDocument {
    let table = loom_writer_core::WriterTable::new("visual-table", rows, cols);
    let mut document = WriterDocument::new("visual-table", "Visual table");
    document.blocks.push(loom_writer_core::markdown_table_block(
        1,
        &table.to_markdown(),
    ));
    document
}

/// The window point of the page's document editor (the content box).
fn document_editor_origin(app: &WriterApp) -> (f32, f32) {
    let label = app.get_document_accessible_label().to_string();
    let origin = ElementHandle::find_by_accessible_label(app, &label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("the document editor is in the tree")
        .absolute_position();
    (origin.x, origin.y)
}

#[test]
fn an_interior_table_gridline_is_drawn_once_and_holds_three_to_one_on_its_fill() {
    use slint::Model;
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 1.0);
    apply_document(&app, &empty_table_document(2, 2));
    let image = snapshot_component(&app, width, height, 1.0).expect("render");
    let rects = app.get_table_cell_rects();
    let cell = |row: usize, col: usize| rects.row_data(row * 2 + col).expect("a cell rectangle");
    let (origin_x, origin_y) = document_editor_origin(&app);
    // Both rows: the line between two cells is drawn once, and it holds 3:1
    // against the fill beneath it, which the table style may set for the row.
    for row in 0..2 {
        let boundary = (origin_x + cell(row, 1).x) as u32;
        let y = (origin_y + cell(row, 0).y + cell(row, 0).height / 2.0) as u32;
        let fill = image.get_pixel(boundary - 6, y).0;
        let differs = |px: &[u8; 4]| {
            (0..3)
                .map(|c| (i32::from(px[c]) - i32::from(fill[c])).abs())
                .sum::<i32>()
                > 40
        };
        let columns: Vec<[u8; 4]> = (boundary - 4..boundary + 4)
            .map(|x| image.get_pixel(x, y).0)
            .collect();
        let line: Vec<[u8; 4]> = columns.iter().copied().filter(differs).collect();
        assert_eq!(
            line.len(),
            1,
            "row {row}: the line between two cells is drawn once, not as two borders: {columns:?}"
        );
        let ratio = contrast(
            [line[0][0], line[0][1], line[0][2]],
            [fill[0], fill[1], fill[2]],
        );
        assert!(
            ratio >= 3.0,
            "row {row}: the gridline {:?} is {ratio:.2}:1 against the fill {fill:?}",
            line[0]
        );
    }
}

#[test]
fn the_caret_keeps_a_gap_before_the_placeholder_glyph() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 1.0);
    apply_document(&app, &WriterDocument::new("caret-gap", "Caret gap"));
    // The caret is drawn while the page has focus, as it is at launch.
    crate::focus_page_at_launch(&app);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    let page = app
        .root_element()
        .query_descendants()
        .match_predicate(|e| e.accessible_placeholder_text().as_deref() == Some(PLACEHOLDER))
        .find_first()
        .expect("the empty page shows its placeholder");
    let origin = page.absolute_position();
    let image = snapshot_component(&app, width, height, 1.0).expect("render");
    let (x0, y0) = (origin.x as u32, origin.y as u32);
    // The first line of text, where the caret and the placeholder's 'T' sit.
    let column_has = |x: u32, test: &dyn Fn(u32) -> bool| {
        (y0..y0 + 14).any(|y| test(luma(image.get_pixel(x, y).0)))
    };
    let caret_right = (x0 - 6..x0 + 12)
        .filter(|x| column_has(*x, &|l| l < 80))
        .max()
        .expect("the caret is drawn in the first line");
    let glyph_left = (caret_right + 1..x0 + 12)
        .find(|x| column_has(*x, &|l| l < 200))
        .expect("the placeholder's first glyph is drawn");
    let gap = glyph_left - caret_right - 1;
    assert!(
        gap >= 2,
        "the caret touches the placeholder: {gap} px between the caret and its glyph"
    );
}

#[test]
fn the_status_bar_gives_its_text_room_at_double_text_scale() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 2.0);
    apply_document(&app, &text_document("Hello world"));
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    let bar = ElementHandle::find_by_element_type_name(&app, "WriterStatusBar")
        .next()
        .expect("the status bar is in the element tree");
    let (top, size) = (bar.absolute_position().y as u32, bar.size().height as u32);
    let image = snapshot_component(&app, width, height, 1.0).expect("render");
    let inked_rows: Vec<u32> = (top..(top + size).min(image.height()))
        .filter(|&y| (0..image.width()).any(|x| luma(image.get_pixel(x, y).0) < 160))
        .collect();
    let (first, last) = (
        *inked_rows.first().expect("the status text is drawn"),
        *inked_rows.last().expect("the status text is drawn"),
    );
    let gap_above = first - top;
    let gap_below = (top + size - 1) - last;
    assert!(
        gap_above >= 4 && gap_below >= 4,
        "the status text has {gap_above}px above and {gap_below}px below in a {size}px bar at 2.0x"
    );
}

#[test]
fn the_docked_inspector_has_a_divider_from_the_canvas() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 1.0);
    app.set_show_inspector(true);
    let image = snapshot_component(&app, width, height, 1.0).expect("render");
    let inspector = ElementHandle::find_by_element_type_name(&app, "WriterInspector")
        .find(|element| element.size().width > 0.0)
        .expect("the docked inspector is in the tree");
    let x = inspector.absolute_position().x as u32;
    let edge = image.get_pixel(x, 400).0;
    // The light border token (#e3e3e8) lies on the inspector's canvas-side edge.
    let border = [0xe3u8, 0xe3, 0xe8];
    for channel in 0..3 {
        assert!(
            (i32::from(edge[channel]) - i32::from(border[channel])).abs() <= 6,
            "the inspector's canvas-side edge is {edge:?}, not the border token {border:?}"
        );
    }
}

#[test]
fn the_document_tab_labels_each_of_its_choice_rows() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 1.0);
    app.set_show_inspector(true);
    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    for label in ["Paper Size", "Orientation", "Margins"] {
        let visible_text = ElementHandle::find_by_accessible_label(&app, label)
            .find(|element| {
                element.accessible_role() == Some(AccessibleRole::Text)
                    && element.size().height > 0.0
            })
            .unwrap_or_else(|| panic!("the Document tab row {label:?} has no visible label"));
        let control = ElementHandle::find_by_accessible_label(&app, label)
            .find(|element| element.accessible_role() != Some(AccessibleRole::Text))
            .unwrap_or_else(|| panic!("the Document tab row {label:?} has no choice control"));
        let label_bottom = visible_text.absolute_position().y + visible_text.size().height;
        assert!(
            label_bottom <= control.absolute_position().y + 0.5,
            "the label {label:?} must sit above its choices"
        );
    }
}
