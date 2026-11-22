use super::*;
use crate::{start_hub, StartHubState};
use std::path::Path;
use std::time::{Duration, SystemTime};

#[test]
fn start_hub_opens_and_formats_badges() {
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.global::<StartHubState>().get_open());
    start_hub::open(&app);
    assert!(app.global::<StartHubState>().get_open());
    assert_eq!(
        start_hub::format_badge(Path::new("deck.loomdeck")),
        "LOOMDECK"
    );
    assert_eq!(start_hub::format_badge(Path::new("deck.pptx")), "PPTX");
}

#[test]
fn start_hub_relative_time_label_formats_recent_times() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    assert_eq!(start_hub::relative_time_label(now, now), "Just now");
    let mins_ago = now - Duration::from_secs(180);
    assert_eq!(start_hub::relative_time_label(mins_ago, now), "3m ago");
}
