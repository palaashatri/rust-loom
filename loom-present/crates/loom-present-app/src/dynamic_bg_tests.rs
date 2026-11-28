//! Behavioral tests for procedural Dynamic Background templates, inspector
//! undo/redo controls, reduced-motion timer pausing, and `.loomdeck` round-trip.

use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_desktop::ScriptedFileDialogs;
use loom_present_core::{load_presentation_session, save_presentation_session, DynamicBgPreset};
use loom_test_support::capture::snapshot_component;

fn launched(session: PresentationSession) -> (PresentApp, Rc<GuiState>) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(GuiState {
        session: RefCell::new(session.clone()),
        last_saved: RefCell::new(session.document),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    configure_responsive_width(&app, 1280);
    wire_app_callbacks(&app, &state);
    wire_palette(&app);
    refresh_without_recovery(&app, &state);
    (app, state)
}

#[test]
fn dynamic_category_and_template_selection_populate_dynamic_background() {
    let (app, state) = launched(empty_session());
    app.set_theme_chooser_open(true);
    app.set_theme_category(6);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render dynamic category");

    for name in [
        "Aurora Veil",
        "Solar Mesh",
        "Oceanic Silk",
        "Prism Glass",
        "Obsidian Orbs",
        "Botanical Mist",
    ] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, name)
                .next()
                .is_some(),
            "Dynamic template card {name:?} must appear in Dynamic category"
        );
    }

    // Creating from Aurora Veil (index 12) populates dynamic_bg on all slides and ElementInk.
    app.invoke_create_theme(12);
    {
        let session = state.session.borrow();
        assert_eq!(session.document.slides.len(), 2);
        assert_eq!(
            session.document.slides[0]
                .dynamic_bg
                .as_ref()
                .map(|b| b.preset),
            Some(DynamicBgPreset::AuroraVeil)
        );
        assert_eq!(
            session.document.slides[1]
                .dynamic_bg
                .as_ref()
                .map(|b| b.preset),
            Some(DynamicBgPreset::AuroraVeil)
        );
    }
    let ink = app.global::<ElementInk>();
    assert!(ink.get_has_dynamic_bg());
    assert_eq!(ink.get_dynamic_bg_preset(), 0);
    assert!(ink.get_dynamic_bg_animate());
    assert_eq!(ink.get_dynamic_bg_speed(), 1);
    assert!(ink.get_dynamic_bg_image().size().width > 0);
    assert!(
        dynamic_bg_view::is_timer_running(),
        "live timer runs when active slide has animated dynamic_bg"
    );
    dynamic_bg_view::stop_timer();
}

#[test]
fn inspector_dynamic_bg_controls_support_undo_redo_and_apply_all() {
    let (app, state) = launched(sample_session());
    assert!(!app.global::<ElementInk>().get_has_dynamic_bg());
    assert!(
        !dynamic_bg_view::is_timer_running(),
        "idle solid slide must not run dynamic background timer"
    );

    let ink = app.global::<ElementInk>();
    // Switch active slide to Dynamic (1 undo step).
    ink.invoke_dynamic_bg_mode_changed(true);
    assert!(ink.get_has_dynamic_bg());
    assert!(deck_is_dirty(&state));
    assert_eq!(
        state.session.borrow().document.slides[0]
            .dynamic_bg
            .as_ref()
            .map(|b| b.preset),
        Some(DynamicBgPreset::AuroraVeil)
    );

    // Pick Oceanic Silk preset, change speed to Lively, and Apply to All Slides.
    ink.invoke_dynamic_bg_preset_picked(2);
    assert_eq!(
        state.session.borrow().document.slides[0]
            .dynamic_bg
            .as_ref()
            .map(|b| b.preset),
        Some(DynamicBgPreset::OceanicSilk)
    );
    ink.invoke_dynamic_bg_speed_picked(2);
    assert_eq!(ink.get_dynamic_bg_speed(), 2);
    ink.invoke_dynamic_bg_apply_all();
    assert!(state.session.borrow().document.slides.iter().all(|s| s
        .dynamic_bg
        .as_ref()
        .map(|b| b.preset)
        == Some(DynamicBgPreset::OceanicSilk)));

    // Undo Apply to All -> second slide reverts to None while slide 0 stays Oceanic Silk.
    app.invoke_undo();
    assert!(state.session.borrow().document.slides[1]
        .dynamic_bg
        .is_none());
    assert_eq!(
        state.session.borrow().document.slides[0]
            .dynamic_bg
            .as_ref()
            .map(|b| b.preset),
        Some(DynamicBgPreset::OceanicSilk)
    );

    // Redo restores all slides; undo back through speed, preset, and mode restores solid.
    app.invoke_redo();
    assert!(state.session.borrow().document.slides[1]
        .dynamic_bg
        .is_some());
    app.invoke_undo();
    app.invoke_undo();
    app.invoke_undo();
    app.invoke_undo();
    assert!(state.session.borrow().document.slides[0]
        .dynamic_bg
        .is_none());
    assert!(!ink.get_has_dynamic_bg());
    assert!(!dynamic_bg_view::is_timer_running());
}

#[test]
fn reduced_motion_and_animate_toggle_pause_the_dynamic_bg_timer() {
    let (app, state) = launched(empty_session());
    app.invoke_create_theme(13); // Solar Mesh
    assert!(dynamic_bg_view::is_timer_running());

    // Enabling Theme.tokens.reduced-motion pauses the timer immediately on sync.
    let theme = Theme::get(&app);
    let mut tokens = theme.get_tokens();
    tokens.reduced_motion = true;
    theme.set_tokens(tokens.clone());
    refresh_without_recovery(&app, &state);
    assert!(
        !dynamic_bg_view::is_timer_running(),
        "reduced-motion must pause the live dynamic background timer"
    );

    // Clearing reduced-motion resumes animation.
    tokens.reduced_motion = false;
    theme.set_tokens(tokens);
    refresh_without_recovery(&app, &state);
    assert!(dynamic_bg_view::is_timer_running());

    // Toggling Animate off in the inspector also stops the timer.
    app.global::<ElementInk>()
        .invoke_dynamic_bg_animate_toggled(false);
    assert!(!app.global::<ElementInk>().get_dynamic_bg_animate());
    assert!(!dynamic_bg_view::is_timer_running());
}

#[test]
fn dynamic_background_round_trips_through_loomdeck_package() {
    let (app, state) = launched(empty_session());
    app.invoke_create_theme(15); // Prism Glass
    app.global::<ElementInk>().invoke_dynamic_bg_speed_picked(0); // Slow
    app.global::<ElementInk>()
        .invoke_dynamic_bg_animate_toggled(false);

    let bytes = save_presentation_session(&state.session.borrow()).expect("save .loomdeck");
    let reopened = load_presentation_session(&bytes).expect("reopen .loomdeck");
    let original_bg = state.session.borrow().document.slides[0]
        .dynamic_bg
        .clone()
        .expect("original dynamic_bg");
    let reopened_bg = reopened.document.slides[0]
        .dynamic_bg
        .clone()
        .expect("reopened dynamic_bg");
    assert_eq!(reopened_bg, original_bg);
    assert_eq!(reopened_bg.preset, DynamicBgPreset::PrismGlass);
    assert!(!reopened_bg.animate);
    assert!((reopened_bg.speed - 0.5).abs() < 0.01);
    dynamic_bg_view::stop_timer();
}
