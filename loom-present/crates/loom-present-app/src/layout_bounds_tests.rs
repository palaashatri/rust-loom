//! Layout checked by measured bounds: the presenter's Current preview is the
//! primary, larger one; inspector headings start on the controls' left edge; and
//! the palette hint names its keys in words rather than a glyph.

use super::*;
use crate::keyboard_flow_tests::{launched, render};
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use loom_test_support::capture::snapshot_component;

/// Every element under `root`, with its accessible label, role and rectangle.
fn elements(root: ElementHandle) -> Vec<ElementHandle> {
    root.query_descendants()
        .match_predicate(|_| true)
        .find_all()
        .into_iter()
        .collect()
}

/// The labelled elements whose label satisfies `matches`.
fn labelled(root: ElementHandle, matches: impl Fn(&str) -> bool) -> Vec<ElementHandle> {
    elements(root)
        .into_iter()
        .filter(|element| {
            element
                .accessible_label()
                .is_some_and(|label| matches(label.as_str()))
        })
        .collect()
}

fn inside(element: &ElementHandle, width: f32, height: f32) -> bool {
    let (origin, size) = (element.absolute_position(), element.size());
    origin.x >= -0.5
        && origin.y >= -0.5
        && origin.x + size.width <= width + 0.5
        && origin.y + size.height <= height + 0.5
}

#[test]
fn the_presenter_shows_the_current_slide_first_and_largest_at_every_size() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    presenter::open(&app, &sample_session()).expect("the presenter opens");

    for (width, height) in [(960u32, 600u32), (640, 420)] {
        presenter::with_window(|window| {
            window
                .window()
                .set_size(slint::PhysicalSize::new(width, height));
            let _ = snapshot_component(window, width as f32, height as f32, 1.0)
                .expect("render the presenter");
            let previews = labelled(window.root_element(), |label| {
                label.starts_with("Current slide: ") || label.starts_with("Next slide: ")
            })
            .into_iter()
            .filter(|element| element.accessible_role() == Some(AccessibleRole::Image))
            .collect::<Vec<_>>();
            let (width, height) = (width as f32, height as f32);
            let current = previews
                .iter()
                .find(|element| {
                    element
                        .accessible_label()
                        .is_some_and(|label| label.starts_with("Current slide: "))
                })
                .unwrap_or_else(|| panic!("{width}x{height}: the current slide is shown"));
            assert!(
                inside(current, width, height),
                "{width}x{height}: the current preview stays in the window"
            );
            let area = |element: &ElementHandle| {
                let size = element.size();
                size.width * size.height
            };
            for next in previews.iter().filter(|element| {
                element
                    .accessible_label()
                    .is_some_and(|label| label.starts_with("Next slide: "))
            }) {
                assert!(
                    current.absolute_position().y < next.absolute_position().y,
                    "{width}x{height}: the current slide is listed above the next one"
                );
                assert!(
                    area(current) > area(next),
                    "{width}x{height}: the current preview is the larger one"
                );
                assert!(
                    inside(next, width, height),
                    "{width}x{height}: the next preview stays in the window"
                );
            }
        })
        .expect("the presenter window exists");
    }
    presenter::forget();
}

#[test]
fn inspector_headings_start_on_the_left_edge_of_the_controls() {
    let s = launched();
    let root = s.app.root_element();
    let heading = labelled(root.clone(), |label| label == "Slide Layout")
        .into_iter()
        .next()
        .expect("the Format tab shows the Slide Layout heading");
    let control = labelled(root, |label| label == "Layout Template")
        .into_iter()
        .next()
        .expect("the Format tab shows the layout control");
    let (heading_x, control_x) = (heading.absolute_position().x, control.absolute_position().x);
    assert!(
        (heading_x - control_x).abs() < 0.5,
        "the heading text starts at {heading_x}, the controls at {control_x}"
    );
}

#[test]
fn the_palette_hint_names_its_keys_in_words() {
    let s = launched();
    s.app.invoke_open_palette();
    render(&s.app);
    let hints = labelled(s.app.root_element(), |label| label.contains("esc close"));
    assert!(!hints.is_empty(), "the palette shows its key hint");
    for hint in hints {
        let label = hint.accessible_label().unwrap_or_default();
        assert!(
            !label.contains('\u{21b5}'),
            "the hint uses a word, not a glyph some fonts lack: {label:?}"
        );
        assert!(
            label.contains("Enter run"),
            "the hint names Enter: {label:?}"
        );
    }
}
