use super::*;
use crate::{welcome_guide, WelcomeGuideState};

#[test]
fn welcome_guide_opens_and_closes_cleanly() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.global::<WelcomeGuideState>().get_open());
    welcome_guide::open(&app);
    assert!(app.global::<WelcomeGuideState>().get_open());
    assert_eq!(app.global::<WelcomeGuideState>().get_step(), 0);
    app.global::<WelcomeGuideState>().set_step(3);
    assert_eq!(app.global::<WelcomeGuideState>().get_step(), 3);
    app.global::<WelcomeGuideState>().set_open(false);
    assert!(!app.global::<WelcomeGuideState>().get_open());
}

#[test]
fn welcome_guide_platform_mod_key_matches_os() {
    let key = welcome_guide::platform_mod_key();
    if cfg!(target_os = "macos") {
        assert_eq!(key, "Cmd+");
    } else {
        assert_eq!(key, "Ctrl+");
    }
}
