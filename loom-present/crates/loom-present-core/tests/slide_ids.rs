use loom_present_core::PresentationDocument;

#[test]
fn slide_ids_stay_unique_after_deleting_and_adding_or_duplicating() {
    let mut doc = PresentationDocument::new("deck-1", "Deck");
    doc.add_slide("Two", "content");
    doc.add_slide("Three", "content");
    assert!(doc.remove_slide(1).is_some());
    doc.add_slide("Four", "content");
    doc.duplicate_slide(0);
    doc.duplicate_slide(0);
    let ids: Vec<&str> = doc.slides.iter().map(|s| s.id.as_str()).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "slide ids must be unique: {ids:?}");
}

#[test]
fn undo_and_redo_reverse_a_transition_change() {
    use loom_present_core::{PresentationSession, TransitionKind};
    let mut session = PresentationSession::new(PresentationDocument::new("deck-1", "Deck"));
    let id = session.document.slides[0].id.clone();
    assert_eq!(session.transition_for(&id), TransitionKind::None);

    session.checkpoint();
    assert!(session.set_transition(&id, TransitionKind::Dissolve));
    assert!(session.can_undo());

    assert!(session.undo());
    assert_eq!(session.transition_for(&id), TransitionKind::None);
    assert!(session.redo());
    assert_eq!(session.transition_for(&id), TransitionKind::Dissolve);
}
