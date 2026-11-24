use super::*;
use crate::{
    find_replace_ui, print_preview, start_hub, welcome_guide, FindReplaceState, PrintPreviewState,
    StartHubState, WelcomeGuideState,
};

#[test]
fn welcome_guide_opens_progresses_and_closes_cleanly() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.global::<WelcomeGuideState>().get_open());
    welcome_guide::open(&app);
    assert!(app.global::<WelcomeGuideState>().get_open());
    assert_eq!(app.global::<WelcomeGuideState>().get_step(), 0);
    app.global::<WelcomeGuideState>().set_step(1);
    assert_eq!(app.global::<WelcomeGuideState>().get_step(), 1);
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

#[test]
fn opening_other_modals_closes_welcome_guide() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");

    welcome_guide::open(&app);
    assert!(app.global::<WelcomeGuideState>().get_open());

    start_hub::open(&app);
    assert!(app.global::<StartHubState>().get_open());
    assert!(!app.global::<WelcomeGuideState>().get_open());

    welcome_guide::open(&app);
    assert!(app.global::<WelcomeGuideState>().get_open());
    assert!(!app.global::<StartHubState>().get_open());

    print_preview::open(&app, 1, "Deck", "");
    assert!(app.global::<PrintPreviewState>().get_open());
    assert!(!app.global::<WelcomeGuideState>().get_open());

    welcome_guide::open(&app);
    assert!(app.global::<WelcomeGuideState>().get_open());
    assert!(!app.global::<PrintPreviewState>().get_open());

    find_replace_ui::open(&app);
    assert!(app.global::<FindReplaceState>().get_open());
    assert!(!app.global::<WelcomeGuideState>().get_open());

    welcome_guide::open(&app);
    assert!(app.global::<WelcomeGuideState>().get_open());
    assert!(!app.global::<FindReplaceState>().get_open());
}
