use super::*;
use crate::{print_preview, PrintPreviewState};

#[test]
fn print_preview_layout_and_page_counts() {
    assert_eq!(
        print_preview::total_pages_for(0, print_preview::PrintLayout::SingleSlide),
        1
    );
    assert_eq!(
        print_preview::total_pages_for(5, print_preview::PrintLayout::SingleSlide),
        5
    );
    assert_eq!(
        print_preview::total_pages_for(5, print_preview::PrintLayout::Handouts2),
        3
    );
    assert_eq!(
        print_preview::total_pages_for(5, print_preview::PrintLayout::Handouts4),
        2
    );
    assert_eq!(
        print_preview::total_pages_for(7, print_preview::PrintLayout::Handouts6),
        2
    );
    assert_eq!(
        print_preview::total_pages_for(4, print_preview::PrintLayout::NotesPage),
        4
    );
}

#[test]
fn print_preview_opens_with_slide_count_and_title() {
    let app = PresentApp::new().expect("create PresentApp");
    print_preview::open(&app, 4, "Q4 Review", "Speaker notes");
    let state = app.global::<PrintPreviewState>();
    assert!(state.get_open());
    assert_eq!(state.get_total_pages(), 4);
}
