//! Names on a slide. Every element says what it is, every button on the slide
//! (thumbnails included) has a name, and the element text field exists only
//! while an element is selected.

use super::keyboard_flow_tests::{launched, render, Session};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::Model;

fn index_of(s: &Session, id: &str) -> usize {
    s.state.session.borrow().document.slides[0]
        .elements
        .iter()
        .position(|element| element.id == id)
        .expect("element on the first slide")
}

fn label_at(s: &Session, index: usize) -> String {
    s.app
        .get_element_labels()
        .row_data(index)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn an_empty_title_is_named_empty_and_a_filled_one_by_its_text() {
    let s = launched();
    let title = index_of(&s, "cover-title");
    // The title is selected, so its name says so.
    s.app.invoke_update_element_content("".into());
    assert_eq!(label_at(&s, title), "Selected Title, empty");
    s.app.invoke_update_element_content("Quarterly plan".into());
    assert_eq!(label_at(&s, title), "Selected Title: Quarterly plan");

    // Deselected, the plain name, with no separator left behind when the text goes.
    s.state.session.borrow_mut().selected_elements.clear();
    refresh(&s.app, &s.state);
    assert_eq!(label_at(&s, title), "Title: Quarterly plan");
    s.state.session.borrow_mut().document.slides[0].elements[title]
        .content
        .clear();
    refresh(&s.app, &s.state);
    assert_eq!(label_at(&s, title), "Title, empty");
}

#[test]
fn every_button_on_the_slide_and_its_thumbnail_has_a_name() {
    let s = launched();
    // An empty title is the case that used to leave a button unnamed.
    s.app.invoke_update_element_content("".into());
    render(&s.app);
    let unnamed = s
        .app
        .root_element()
        .query_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .find_all()
        .into_iter()
        .filter(|e| e.accessible_label().is_none_or(|label| label.is_empty()))
        .map(|e| format!("{:?} at {:?}", e.accessible_label(), e.absolute_position()))
        .collect::<Vec<_>>();
    assert!(unnamed.is_empty(), "unnamed buttons: {unnamed:?}");
}

#[test]
fn the_element_text_field_exists_only_while_an_element_is_selected() {
    let s = launched();
    s.app.set_show_inspector(true);
    render(&s.app);
    assert!(
        ElementHandle::find_by_accessible_label(&s.app, "Selected element text")
            .next()
            .is_some(),
        "the text field exists while the title is selected"
    );

    s.state.session.borrow_mut().selected_elements.clear();
    refresh(&s.app, &s.state);
    render(&s.app);
    assert!(
        ElementHandle::find_by_accessible_label(&s.app, "Selected element text")
            .next()
            .is_none(),
        "with nothing selected there is no text field to reach"
    );
}
