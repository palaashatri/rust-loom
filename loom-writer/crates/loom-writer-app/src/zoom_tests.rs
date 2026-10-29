//! Zoom actions use the page canvas and document geometry, and keep the
//! projected caret, selection and pointer coordinates on the same scale.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::snapshot_component;

#[derive(Clone, Copy)]
struct ViewportSize {
    width: u32,
    height: u32,
}

const WIDE: ViewportSize = ViewportSize {
    width: 1280,
    height: 800,
};
const COMPACT: ViewportSize = ViewportSize {
    width: 1024,
    height: 720,
};

fn launched(text: &str) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document(text), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    wire_writer_inspector_toggle(&app, &state, None);
    app.window()
        .set_size(PhysicalSize::new(WIDE.width, WIDE.height));
    apply_layout_breakpoints(&app, WIDE.width);
    toolbar_commands::start_with_inspector_open(&app, WIDE.width);
    apply_state(&app, &state);
    render(&app, WIDE);
    (app, state)
}

fn render(app: &WriterApp, size: ViewportSize) {
    let _ = snapshot_component(app, size.width as f32, size.height as f32, 1.0)
        .expect("render Writer window");
}

fn button(app: &WriterApp, label: &str, size: ViewportSize) {
    render(app, size);
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| {
            element.accessible_role() == Some(AccessibleRole::Button)
                && element.size().height >= 40.0
        })
        .unwrap_or_else(|| panic!("toolbar button {label:?}"))
        .invoke_accessible_default_action();
}

fn row(app: &WriterApp, label: &str, size: ViewportSize) {
    render(app, size);
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::ListItem))
        .unwrap_or_else(|| panic!("zoom menu row {label:?}"))
        .invoke_accessible_default_action();
}

fn zoom_button(app: &WriterApp, size: ViewportSize) {
    let label = format!("Zoom {}%", (app.get_page_zoom() * 100.0).round() as i32);
    button(app, &label, size);
}

fn expected_fit_zoom(app: &WriterApp, include_height: bool) -> f32 {
    let width = ((app.get_view_width() - 48.0) / app.get_page_width_pt()).clamp(0.5, 2.0);
    if include_height {
        let height =
            ((app.get_page_view_height() - 48.0) / app.get_page_height_pt()).clamp(0.5, 2.0);
        width.min(height)
    } else {
        width
    }
}

fn assert_caret_hit_and_selection_alignment(state: &GuiState) {
    let mut document = state.current.borrow().clone();
    let style = document.page.page_style();
    let viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: state.viewport.borrow().zoom,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };

    for offset in [1, 3, 5] {
        document.set_selection(TextSelection::caret(offset));
        let layout = document.layout(&style, viewport).expect("layout caret");
        let page = layout.page_bounds.first().expect("first page");
        let caret = layout
            .selection_rects
            .iter()
            .find(|rect| rect.start == rect.end)
            .expect("projected caret");
        let x = caret.rect.x - page.x - style.margin_left_pt * viewport.zoom;
        let y =
            caret.rect.y - page.y - style.margin_top_pt * viewport.zoom + caret.rect.height / 2.0;
        assert_eq!(
            writer_pointer_offset(&document, viewport, x, y),
            Some(offset),
            "the hit point on caret {offset} must resolve at {}%",
            (viewport.zoom * 100.0).round()
        );
    }

    document.set_selection(TextSelection::range(1, 5));
    let layout = document.layout(&style, viewport).expect("layout selection");
    let page = layout.page_bounds.first().expect("first page");
    let selected = layout
        .selection_rects
        .iter()
        .filter(|rect| rect.start < 5 && rect.end > 1)
        .collect::<Vec<_>>();
    assert!(!selected.is_empty(), "selection must have visible geometry");
    assert!(selected.iter().all(|rect| rect.rect.width > 0.0));
    let rect = selected[0];
    let x = rect.rect.x + rect.rect.width / 2.0 - page.x - style.margin_left_pt * viewport.zoom;
    let y = rect.rect.y + rect.rect.height / 2.0 - page.y - style.margin_top_pt * viewport.zoom;
    let hit = writer_pointer_offset(&document, viewport, x, y).expect("hit within selection");
    assert!(
        (1..=5).contains(&hit),
        "hit offset {hit} must stay in the selected range at {}%",
        (viewport.zoom * 100.0).round()
    );
}

#[test]
fn fit_actions_track_inspector_and_resize_geometry_without_breaking_text_alignment() {
    let (app, state) = launched("Hello world");
    assert!(ElementHandle::find_by_accessible_label(&app, "Zoom 100%").count() > 0);

    let inspector_width = app.get_view_width();
    zoom_button(&app, WIDE);
    row(&app, "Fit Width", WIDE);
    let fit_with_inspector = expected_fit_zoom(&app, false);
    assert!((state.viewport.borrow().zoom - fit_with_inspector).abs() < 1e-3);
    assert!(app.get_page_width_pt() * fit_with_inspector <= inspector_width - 47.0);
    assert_caret_hit_and_selection_alignment(&state);

    button(&app, "View options", WIDE);
    row(&app, "Format Inspector", WIDE);
    render(&app, WIDE);
    assert!(!app.get_show_inspector());
    let without_inspector_width = app.get_view_width();
    assert!(without_inspector_width > inspector_width);
    zoom_button(&app, WIDE);
    row(&app, "Fit Width", WIDE);
    let fit_without_inspector = expected_fit_zoom(&app, false);
    assert!((state.viewport.borrow().zoom - fit_without_inspector).abs() < 1e-3);
    assert!(fit_without_inspector > fit_with_inspector);
    assert_caret_hit_and_selection_alignment(&state);

    app.window()
        .set_size(PhysicalSize::new(COMPACT.width, COMPACT.height));
    apply_layout_breakpoints(&app, COMPACT.width);
    render(&app, COMPACT);
    let resized_width = app.get_view_width();
    assert!(resized_width < without_inspector_width);
    zoom_button(&app, COMPACT);
    row(&app, "Fit Width", COMPACT);
    let fit_after_resize = expected_fit_zoom(&app, false);
    assert!((state.viewport.borrow().zoom - fit_after_resize).abs() < 1e-3);
    assert!((fit_after_resize - fit_without_inspector).abs() > 0.1);
    assert_caret_hit_and_selection_alignment(&state);

    zoom_button(&app, COMPACT);
    row(&app, "Fit Page", COMPACT);
    let fit_page = expected_fit_zoom(&app, true);
    assert!((state.viewport.borrow().zoom - fit_page).abs() < 1e-3);
    assert!(app.get_page_width_pt() * fit_page <= resized_width - 47.0);
    assert!(app.get_page_height_pt() * fit_page <= app.get_page_view_height() - 47.0);
    assert_caret_hit_and_selection_alignment(&state);

    zoom_button(&app, COMPACT);
    row(&app, "Zoom In", COMPACT);
    assert!((state.viewport.borrow().zoom - (fit_page + 0.25).min(2.0)).abs() < 1e-3);
    zoom_button(&app, COMPACT);
    row(&app, "Zoom Out", COMPACT);
    assert!((state.viewport.borrow().zoom - fit_page).abs() < 1e-3);
    zoom_button(&app, COMPACT);
    row(&app, "Actual Size", COMPACT);
    assert!((state.viewport.borrow().zoom - 1.0).abs() < 1e-3);
    assert!(ElementHandle::find_by_accessible_label(&app, "Zoom 100%").count() > 0);
}
