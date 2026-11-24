use super::*;
use crate::font_picker;

#[test]
fn font_picker_builds_rows_and_publishes_current() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    font_picker::publish_current(&app, Some("Inter"), false);
    assert_eq!(
        app.global::<crate::FontPicker>()
            .get_current_family()
            .as_str(),
        "Inter"
    );
    font_picker::publish_current(&app, None, true);
    assert_eq!(
        app.global::<crate::FontPicker>()
            .get_current_family()
            .as_str(),
        ""
    );
}

#[test]
fn font_picker_to_lines_excludes_hidden_dot_fonts_and_preserves_usable_families() {
    let rows = font_picker::build_rows(&font_picker::Inputs {
        in_deck: &["Inter".to_string()],
        installed: &[
            ".SF NS".to_string(),
            ".AppleSystemUIFont".to_string(),
            "Arial".to_string(),
            "Georgia".to_string(),
        ],
        query: "",
    });
    let lines = font_picker::to_lines(&rows);
    assert!(lines.iter().any(|l| l.family.contains("Inter")));
    assert!(lines.iter().any(|l| l.family.contains("Arial")));
    assert!(lines.iter().any(|l| l.family.contains("Georgia")));
    assert!(!lines.iter().any(|l| l.family.starts_with('.')));
}

#[test]
fn font_picker_selection_changes_element_font_and_undo_reverts() {
    let el = text_element(
        "e1",
        ElementType::BodyText,
        "Font test",
        40.0,
        40.0,
        200.0,
        40.0,
    );
    let (app, state) = crate::element_style_tests::fixture(vec![el]);
    state.session.borrow_mut().select_element("e1", false);
    crate::refresh(&app, &state);

    let initial_font = state.session.borrow().document.slides[0].elements[0]
        .style
        .font_family
        .clone();
    let fp = app.global::<crate::FontPicker>();
    fp.invoke_open_requested();
    fp.invoke_query_edited("Georgia".into());
    fp.invoke_choose(0);
    assert_eq!(
        state.session.borrow().document.slides[0].elements[0]
            .style
            .font_family
            .as_deref(),
        Some("Georgia")
    );

    assert!(state.session.borrow_mut().undo());
    assert_eq!(
        state.session.borrow().document.slides[0].elements[0]
            .style
            .font_family,
        initial_font
    );
}
