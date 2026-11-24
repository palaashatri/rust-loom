use super::*;
use crate::{find_replace_ui, FindReplaceState};
use loom_present_core::Slide;

#[test]
fn find_replace_opens_and_updates_match_counts() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.global::<FindReplaceState>().get_open());
    find_replace_ui::open(&app);
    assert!(app.global::<FindReplaceState>().get_open());
    find_replace_ui::update_match_counts(&app, 2, 5);
    assert_eq!(app.global::<FindReplaceState>().get_current_match(), 2);
    assert_eq!(app.global::<FindReplaceState>().get_total_matches(), 5);
}

#[test]
fn find_replace_mutates_multi_slide_elements_and_notes_with_undo() {
    let mut el1 = text_element(
        "e1",
        ElementType::BodyText,
        "Alpha one",
        40.0,
        40.0,
        200.0,
        40.0,
    );
    el1.id = "e1".into();
    let mut el2 = text_element(
        "e2",
        ElementType::BodyText,
        "Alpha two",
        40.0,
        100.0,
        200.0,
        40.0,
    );
    el2.id = "e2".into();
    let (app, state) = crate::element_style_tests::fixture(vec![el1, el2]);
    {
        let mut s = state.session.borrow_mut();
        s.document.slides[0].speaker_notes = "Note Alpha".into();
        let mut slide2 = Slide::new("s2", "Slide 2", "content");
        slide2.elements.push(text_element(
            "e3",
            ElementType::BodyText,
            "Alpha three",
            40.0,
            40.0,
            200.0,
            40.0,
        ));
        s.document.slides.push(slide2);
    }
    crate::modal_wiring::wire(&app, &state);
    find_replace_ui::open(&app);

    let fr = app.global::<FindReplaceState>();
    fr.set_query("Alpha".into());
    fr.invoke_query_changed("Alpha".into());
    assert_eq!(fr.get_total_matches(), 4);
    assert_eq!(fr.get_current_match(), 1);

    // Replace One mutates the first match
    fr.set_replacement("Beta".into());
    fr.invoke_replace_one();
    assert_eq!(
        state.session.borrow().document.slides[0].elements[0].content,
        "Beta one"
    );
    assert_eq!(fr.get_total_matches(), 3);

    // Replace All mutates remaining matches including speaker notes and slide 2
    fr.set_replacement("Gamma".into());
    fr.invoke_replace_all();
    {
        let s = state.session.borrow();
        assert_eq!(s.document.slides[0].elements[1].content, "Gamma two");
        assert_eq!(s.document.slides[0].speaker_notes, "Note Gamma");
        assert_eq!(s.document.slides[1].elements[0].content, "Gamma three");
    }

    // Undo restores Replace All, then Replace One
    assert!(state.session.borrow_mut().undo());
    assert_eq!(
        state.session.borrow().document.slides[0].speaker_notes,
        "Note Alpha"
    );
    assert!(state.session.borrow_mut().undo());
    assert_eq!(
        state.session.borrow().document.slides[0].elements[0].content,
        "Alpha one"
    );
}
