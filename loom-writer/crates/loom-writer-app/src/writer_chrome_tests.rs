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
