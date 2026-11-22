use super::*;
use crate::font_picker;

#[test]
fn font_picker_builds_rows_and_publishes_current() {
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
        "Mixed"
    );
}

#[test]
fn font_picker_to_lines_preserves_rows() {
    let rows = font_picker::build_rows(&font_picker::Inputs {
        in_deck: &["Inter".to_string()],
        installed: &["Arial".to_string(), "Georgia".to_string()],
        query: "",
    });
    let lines = font_picker::to_lines(&rows);
    assert!(!lines.is_empty());
}
