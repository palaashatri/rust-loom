//! Left and Right inside right-to-left text, through real window key events.
//! Left-to-right text keeps the input's own Left and Right.

use crate::keyboard_flow_tests::launched;
use crate::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_text::{CharacterStyle, FontWeight, ParagraphDirection, StyleRun};
use loom_writer_core::{visual_caret_move, CaretAffinity, VisualMove};
use slint::platform::{Key, WindowEvent};

fn press(app: &WriterApp, key: Key) {
    let text: SharedString = key.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn focus(state: &GuiState) -> usize {
    state.current.borrow().selection().focus
}

struct ResetScriptCatalog;

impl Drop for ResetScriptCatalog {
    fn drop(&mut self) {
        loom_writer_core::fonts::reset_font_catalog();
    }
}

fn install_script_catalog() -> ResetScriptCatalog {
    let mut config = loom_fonts::ScanConfig::with_dirs(Vec::new());
    config.policy = loom_fonts::FallbackPolicy::new(["Noto Naskh Arabic", "Noto Sans Hebrew"]);
    loom_writer_core::fonts::install_font_catalog(std::sync::Arc::new(
        loom_fonts::FontCatalog::scan(&config),
    ));
    ResetScriptCatalog
}

const MIXED_SCRIPT_TEXT: &str = "Latin שלום مرحبا Z";

fn mixed_script_session() -> keyboard_flow_tests::Session {
    let session = launched(MIXED_SCRIPT_TEXT);
    {
        let mut document = session.state.current.borrow_mut();
        let block = document.blocks.first_mut().expect("the paragraph exists");
        block.style.direction = ParagraphDirection::RightToLeft;
        for (text, style) in [
            (
                "Latin",
                CharacterStyle {
                    weight: FontWeight::Bold,
                    italic: true,
                    ..CharacterStyle::default()
                },
            ),
            (
                "שלום",
                CharacterStyle {
                    font_size: 17.0,
                    explicit_size: true,
                    color: Some("#145AC8FF".to_owned()),
                    superscript: true,
                    ..CharacterStyle::default()
                },
            ),
            (
                "مرحبا",
                CharacterStyle {
                    font_size: 18.0,
                    explicit_size: true,
                    color: Some("#C33218FF".to_owned()),
                    subscript: true,
                    ..CharacterStyle::default()
                },
            ),
        ] {
            let start = MIXED_SCRIPT_TEXT
                .find(text)
                .unwrap_or_else(|| panic!("the fixture contains {text:?}"));
            block.runs.push(StyleRun {
                start,
                end: start + text.len(),
                style,
            });
        }
        document.set_selection(TextSelection::caret(MIXED_SCRIPT_TEXT.len()));
    }
    apply_state(&session.app, &session.state);
    session.app.invoke_focus_page();
    session
}

fn shaped_affinity(affinity: CaretAffinity) -> loom_fonts::Affinity {
    match affinity {
        CaretAffinity::Downstream => loom_fonts::Affinity::Leading,
        CaretAffinity::Upstream => loom_fonts::Affinity::Trailing,
    }
}

fn editor_window_origin(app: &WriterApp) -> (f32, f32) {
    let label = app.get_document_accessible_label().to_string();
    let editor = ElementHandle::find_by_accessible_label(app, &label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("the document editor is exposed as a text input");
    let origin = editor.absolute_position();
    (origin.x, origin.y)
}

fn render(app: &WriterApp) {
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render the Writer page");
}

fn press_with_shift(app: &WriterApp, key: Key) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    press(app, key);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    render(app);
}

fn leftmost_rightward_step(
    document: &WriterDocument,
    line: &loom_writer_core::FlowLine,
    shaped: &loom_writer_core::WriterShapedVisualLine,
) -> (usize, CaretAffinity, f32, loom_writer_core::CaretTarget) {
    let mut offsets: Vec<usize> = MIXED_SCRIPT_TEXT.char_indices().map(|(at, _)| at).collect();
    offsets.push(MIXED_SCRIPT_TEXT.len());
    offsets
        .into_iter()
        .flat_map(|offset| {
            [CaretAffinity::Downstream, CaretAffinity::Upstream]
                .into_iter()
                .map(move |affinity| (offset, affinity))
        })
        .filter_map(|(offset, affinity)| {
            let right = visual_caret_move(document, offset, affinity, VisualMove::Right, None)?;
            let x = shaped.caret_x(offset, shaped_affinity(affinity));
            (right.x > line.bounds.x + x + 0.5).then_some((offset, affinity, x, right))
        })
        .min_by(|left, right| left.2.total_cmp(&right.2))
        .expect("the visual line has a rightward caret step with visible movement")
}

fn assert_selection_geometry(
    app: &WriterApp,
    document: &WriterDocument,
    selection: &TextSelection,
    flow: &loom_writer_core::DocumentFlow,
    style: &loom_writer_core::PageStyle,
) {
    let ranges = flow.selection_rects(document, selection, 0..flow.page_count());
    let expected = projection::ui_selection_rects(style, 1.0, flow.page_bounds()[0], &ranges);
    let actual = app.get_selection_rects();
    assert_eq!(actual.row_count(), expected.len());
    for (index, expected) in expected.iter().enumerate() {
        let actual = actual
            .row_data(index)
            .expect("the selection row is published");
        assert_eq!(actual.caret, selection.anchor == selection.focus);
        assert!((actual.x - expected.x).abs() < 0.05);
        assert!((actual.y - expected.y).abs() < 0.05);
        assert!((actual.width - expected.width).abs() < 0.05);
        assert!((actual.height - expected.height).abs() < 0.05);
    }
}

#[test]
fn right_and_left_move_visually_through_hebrew_text() {
    // Alef then bet: bet is drawn on the left, so the line's end (offset 4) is
    // on the left and Right moves over bet to alef's boundary.
    let s = launched("\u{5D0}\u{5D1}");
    s.app.invoke_focus_page();
    s.app.invoke_selection_changed(4, 4);
    press(&s.app, Key::RightArrow);
    assert_eq!(focus(&s.state), 2, "Right moves over bet");
    press(&s.app, Key::RightArrow);
    assert_eq!(focus(&s.state), 0, "Right moves over alef");
    press(&s.app, Key::LeftArrow);
    assert_eq!(focus(&s.state), 2, "Left moves back over alef");
}

#[test]
fn left_to_right_text_keeps_the_logical_arrow_moves() {
    let s = launched("abc");
    s.app.invoke_focus_page();
    s.app.invoke_selection_changed(1, 1);
    press(&s.app, Key::RightArrow);
    assert_eq!(focus(&s.state), 2);
    press(&s.app, Key::LeftArrow);
    assert_eq!(focus(&s.state), 1);
}

#[test]
fn mixed_styled_scripts_keep_visual_arrow_order_in_the_live_editor() {
    let _serial = loom_writer_core::test_fonts::serial();
    let _reset = install_script_catalog();
    let session = mixed_script_session();
    let document = session.state.current.borrow().clone();
    let style = document.page.page_style();
    let flow = document
        .flow(&style, PageViewport::default())
        .expect("the mixed-direction paragraph lays out");
    let line = flow.lines().first().expect("one visual line");
    let shaped = line
        .shaped_line
        .as_ref()
        .expect("mixed scripts use shared shaped geometry");

    let (start_offset, start_affinity, start_x, expected_right) =
        leftmost_rightward_step(&document, line, shaped);

    let mut current = session.state.current.borrow_mut();
    current.set_selection(TextSelection {
        anchor: start_offset,
        focus: start_offset,
        affinity: start_affinity,
    });
    drop(current);
    apply_state(&session.app, &session.state);
    session.app.invoke_focus_page();

    press(&session.app, Key::RightArrow);
    render(&session.app);
    let after_right = session.state.current.borrow().selection();
    assert_eq!(
        (after_right.focus, after_right.affinity),
        (expected_right.offset, expected_right.affinity)
    );
    let right_flow = session
        .state
        .current
        .borrow()
        .flow(&style, PageViewport::default())
        .expect("flow remains available after Right");
    let right_caret = right_flow
        .caret_rect(&session.state.current.borrow(), &after_right)
        .expect("the live editor projects the moved caret");
    assert!((right_caret.rect.x - expected_right.x).abs() < 0.05);

    let expected_left = visual_caret_move(
        &document,
        expected_right.offset,
        expected_right.affinity,
        VisualMove::Left,
        None,
    )
    .expect("Left reverses the selected visual caret step");
    press(&session.app, Key::LeftArrow);
    render(&session.app);
    let after_left = session.state.current.borrow().selection();
    let left_flow = session
        .state
        .current
        .borrow()
        .flow(&style, PageViewport::default())
        .expect("flow remains available after Left");
    let left_caret = left_flow
        .caret_rect(&session.state.current.borrow(), &after_left)
        .expect("the live editor projects the returned caret");
    assert_eq!(
        (after_left.focus, after_left.affinity),
        (expected_left.offset, expected_left.affinity)
    );
    assert_eq!(
        (after_left.focus, after_left.affinity),
        (start_offset, start_affinity)
    );
    assert!((left_caret.rect.x - (line.bounds.x + start_x)).abs() < 0.05);
}

#[test]
fn a_mixed_script_pointer_click_and_selection_use_shaped_geometry() {
    let _serial = loom_writer_core::test_fonts::serial();
    let _reset = install_script_catalog();
    let session = mixed_script_session();
    let document = session.state.current.borrow().clone();
    let style = document.page.page_style();
    let flow = document
        .flow(&style, PageViewport::default())
        .expect("the mixed-direction paragraph lays out");
    let line = flow.lines().first().expect("one visual line");
    let shaped = line
        .shaped_line
        .as_ref()
        .expect("mixed scripts use shared shaped geometry");
    let arabic = shaped
        .painted_runs()
        .iter()
        .find(|run| run.source_range.start == MIXED_SCRIPT_TEXT.find("مرحبا").unwrap())
        .expect("the Arabic run remains a distinct shaped style span");
    let line_x = arabic.x_pt + arabic.shaped.width * 0.5;
    let page = flow.page_bounds()[0];
    let pointer = (
        line.bounds.x - page.x - style.margin_left_pt + line_x,
        line.bounds.y - page.y - style.margin_top_pt + line.bounds.height * 0.5,
    );
    let expected = crate::pointer_hit_test::writer_pointer_offset(
        &document,
        PageViewport::default(),
        pointer.0,
        pointer.1,
    )
    .expect("the Arabic run's painted location maps to a source byte offset");
    let hit = shaped
        .hit_test(line_x)
        .expect("the click lands on a caret stop");
    assert_eq!(expected, line.global_start + hit.offset - line.start);

    let origin = editor_window_origin(&session.app);
    let position = slint::LogicalPosition::new(origin.0 + pointer.0, origin.1 + pointer.1);
    session
        .app
        .window()
        .dispatch_event(WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    session
        .app
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    render(&session.app);
    let clicked = session.state.current.borrow().selection();
    assert_eq!((clicked.anchor, clicked.focus), (expected, expected));

    let expected_flow = session
        .state
        .current
        .borrow()
        .flow(&style, PageViewport::default())
        .expect("flow remains available after click");
    assert_selection_geometry(
        &session.app,
        &session.state.current.borrow(),
        &clicked,
        &expected_flow,
        &style,
    );

    press_with_shift(&session.app, Key::RightArrow);
    let selected = session.state.current.borrow().selection();
    assert_ne!(
        selected.anchor, selected.focus,
        "Shift+Right extends the clicked caret"
    );
    let selected_flow = session
        .state
        .current
        .borrow()
        .flow(&style, PageViewport::default())
        .expect("flow remains available after extending the selection");
    assert_selection_geometry(
        &session.app,
        &session.state.current.borrow(),
        &selected,
        &selected_flow,
        &style,
    );
}
