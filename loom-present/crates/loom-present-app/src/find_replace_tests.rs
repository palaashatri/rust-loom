use super::*;
use crate::{find_replace_ui, FindReplaceState};

#[test]
fn find_replace_opens_and_updates_match_counts() {
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.global::<FindReplaceState>().get_open());
    find_replace_ui::open(&app);
    assert!(app.global::<FindReplaceState>().get_open());
    find_replace_ui::update_match_counts(&app, 2, 5);
    assert_eq!(app.global::<FindReplaceState>().get_current_match(), 2);
    assert_eq!(app.global::<FindReplaceState>().get_total_matches(), 5);
}
