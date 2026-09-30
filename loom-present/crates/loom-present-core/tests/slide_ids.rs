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
