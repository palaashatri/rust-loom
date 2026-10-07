//! Reordering slides: edge cases, undo, persistence and export order.

use loom_present_core::{
    export_pdf, export_pptx, extract_pptx_titles, load_presentation_session,
    save_presentation_session, ElementType, PresentationDocument, PresentationSession, Slide,
    SlideElement, TransitionKind,
};

const NAMES: [&str; 5] = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];

fn titled_slide(id: &str, title: &str) -> Slide {
    let mut slide = Slide::new(id, title, "content");
    slide.add_element(SlideElement {
        id: format!("{id}-title"),
        element_type: ElementType::Title,
        content: title.to_string(),
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 80.0,
        rotation_deg: 0.0,
        action: None,
    });
    slide.speaker_notes = format!("notes for {title}");
    slide
}

fn deck() -> PresentationSession {
    let mut document = PresentationDocument::new("deck", "Deck");
    document.slides = NAMES
        .iter()
        .enumerate()
        .map(|(n, name)| titled_slide(&format!("s{}", n + 1), name))
        .collect();
    PresentationSession::new(document)
}

fn order(session: &PresentationSession) -> Vec<String> {
    session
        .document
        .slides
        .iter()
        .map(|slide| slide.title.clone())
        .collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn moving_a_slide_reorders_the_deck_and_selects_it() {
    let mut session = deck();
    assert!(session.move_slide(1, 3));
    assert_eq!(
        order(&session),
        names(&["Alpha", "Gamma", "Delta", "Beta", "Epsilon"])
    );
    assert_eq!(session.document.active_index, 3);
    assert!(session.move_slide(4, 0));
    assert_eq!(
        order(&session),
        names(&["Epsilon", "Alpha", "Gamma", "Delta", "Beta"])
    );
    assert_eq!(session.document.active_index, 0);
}

#[test]
fn a_move_that_changes_nothing_is_refused_and_leaves_no_undo_step() {
    let mut session = deck();
    assert!(!session.move_slide(2, 2), "same position");
    assert!(!session.move_slide(5, 0), "source out of range");
    assert!(!session.move_slide(0, 5), "target out of range");
    assert!(!session.can_undo(), "refused moves must not touch history");
    assert_eq!(order(&session), names(&NAMES));
    assert_eq!(session.document.active_index, 0);

    let mut single = PresentationSession::new(PresentationDocument::new("one", "One"));
    assert!(!single.move_slide(0, 0));
    assert!(!single.can_move_active_slide_up());
    assert!(!single.can_move_active_slide_down());
    assert_eq!(single.move_active_slide(1), None);
    assert_eq!(single.move_active_slide(-1), None);
    assert!(!single.can_undo());
}

#[test]
fn moving_the_active_slide_up_or_down_is_disabled_at_the_ends() {
    let mut session = deck();
    session.document.select_slide(0);
    assert!(!session.can_move_active_slide_up());
    assert!(session.can_move_active_slide_down());
    assert_eq!(session.move_active_slide(-1), None);
    assert!(!session.can_undo());

    session.document.select_slide(4);
    assert!(session.can_move_active_slide_up());
    assert!(!session.can_move_active_slide_down());
    assert_eq!(session.move_active_slide(1), None);
    assert!(!session.can_undo());

    session.document.select_slide(2);
    assert_eq!(session.move_active_slide(-1), Some((2, 1)));
    assert_eq!(session.document.active_index, 1);
    assert_eq!(session.move_active_slide(1), Some((1, 2)));
    assert_eq!(order(&session), names(&NAMES));
}

#[test]
fn each_move_is_exactly_one_undo_step_and_undo_restores_order_and_selection() {
    let mut session = deck();
    session.document.select_slide(2);
    assert_eq!(session.move_active_slide(-1), Some((2, 1)));
    assert!(session.move_slide(0, 4));
    let after_two = order(&session);
    assert_eq!(
        after_two,
        names(&["Gamma", "Beta", "Delta", "Epsilon", "Alpha"])
    );

    assert!(session.undo());
    assert_eq!(
        order(&session),
        names(&["Alpha", "Gamma", "Beta", "Delta", "Epsilon"])
    );
    assert_eq!(
        session.document.active_index, 1,
        "the first move's selection"
    );
    assert!(session.undo());
    assert_eq!(order(&session), names(&NAMES));
    assert_eq!(
        session.document.active_index, 2,
        "selection before any move"
    );
    assert!(!session.undo(), "two moves, two undo steps");

    assert!(session.redo());
    assert!(session.redo());
    assert_eq!(order(&session), after_two);
    assert_eq!(
        session.document.active_index, 4,
        "selection after the last move"
    );
}

#[test]
fn moving_keeps_ids_unique_and_transitions_with_their_slide() {
    let mut session = deck();
    session.checkpoint();
    assert!(session.set_transition("s2", TransitionKind::Push));
    assert!(session.move_slide(1, 4));
    let ids: Vec<&str> = session
        .document
        .slides
        .iter()
        .map(|slide| slide.id.as_str())
        .collect();
    assert_eq!(ids, ["s1", "s3", "s4", "s5", "s2"]);
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "slide ids must stay unique");
    assert_eq!(session.transition_for("s2"), TransitionKind::Push);
    assert_eq!(session.document.slides[4].speaker_notes, "notes for Beta");

    // Duplicating and adding after a reorder still yields fresh ids (CODE-16).
    assert!(session.duplicate_slide(0));
    session.document.add_slide("Zeta", "content");
    let mut all: Vec<&str> = session
        .document
        .slides
        .iter()
        .map(|slide| slide.id.as_str())
        .collect();
    let total = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), total);
}

#[test]
fn the_new_order_survives_save_and_reopen() {
    let mut session = deck();
    assert!(session.move_slide(4, 1));
    let bytes = save_presentation_session(&session).expect("save");
    let reopened = load_presentation_session(&bytes).expect("reopen");
    assert_eq!(
        order(&reopened),
        names(&["Alpha", "Epsilon", "Beta", "Gamma", "Delta"])
    );
    assert_eq!(reopened.document.active_index, 1);
}

#[test]
fn pptx_export_follows_the_new_order() {
    let mut session = deck();
    assert!(session.move_slide(0, 3));
    let pptx = export_pptx(&session).expect("export");
    assert_eq!(
        extract_pptx_titles(&pptx).expect("titles"),
        names(&["Beta", "Gamma", "Delta", "Alpha", "Epsilon"])
    );
}

fn position(haystack: &[u8], needle: &str) -> usize {
    haystack
        .windows(needle.len())
        .position(|window| window == needle.as_bytes())
        .unwrap_or_else(|| panic!("{needle} is missing from the PDF"))
}

#[test]
fn pdf_export_follows_the_new_order() {
    let mut session = deck();
    assert!(session.move_slide(4, 0));
    let pdf = export_pdf(&session.document);
    let at: Vec<usize> = ["Epsilon", "Alpha", "Beta", "Gamma", "Delta"]
        .iter()
        .map(|name| position(&pdf, name))
        .collect();
    assert!(
        at.windows(2).all(|pair| pair[0] < pair[1]),
        "pages must appear in the new order: {at:?}"
    );
}
