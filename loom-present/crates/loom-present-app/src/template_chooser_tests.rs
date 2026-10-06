//! "New from Template...": the chooser creates what its cards say, asks before
//! it replaces unsaved work, and does not call itself a theme.

use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_desktop::ScriptedFileDialogs;

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
    wire_app_callbacks(&app, &state);
    wire_palette(&app);
    refresh_without_recovery(&app, &state);
    (app, state)
}

fn kinds(state: &GuiState) -> Vec<Vec<ElementType>> {
    state
        .session
        .borrow()
        .document
        .slides
        .iter()
        .map(|slide| {
            slide
                .elements
                .iter()
                .map(|element| element.element_type.clone())
                .collect()
        })
        .collect()
}

#[test]
fn each_template_creates_the_slides_its_card_describes_with_empty_placeholders() {
    use ElementType::{BodyText, Subtitle, Title};
    let expected: [(i32, Vec<Vec<ElementType>>); 4] = [
        (0, vec![vec![Title]]),
        (1, vec![vec![Title, Subtitle], vec![Title, BodyText]]),
        (2, vec![vec![Title, BodyText, BodyText]]),
        (3, vec![vec![Title, BodyText]]),
    ];
    for (choice, slides) in expected {
        let (app, state) = launched(empty_session());
        app.invoke_create_theme(choice);
        assert_eq!(kinds(&state), slides, "template {choice}");
        let session = state.session.borrow();
        assert!(
            session
                .document
                .slides
                .iter()
                .flat_map(|slide| &slide.elements)
                .all(|element| element.content.is_empty()),
            "template {choice} adds no filler"
        );
        assert_eq!(session.document.active_index, 0);
        assert_eq!(
            session.document.title, "Untitled Presentation",
            "the deck is not named after a preset"
        );
        assert!(
            !deck_is_dirty(&state),
            "a fresh template deck has nothing unsaved"
        );
        assert!(
            app.get_status_left().contains("template"),
            "{}",
            app.get_status_left()
        );
        assert!(!app.get_theme_chooser_open());
    }
}

#[test]
fn creating_from_a_template_asks_before_replacing_unsaved_work() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();
    app.invoke_select_element(0);
    app.invoke_update_element_content("Precious work".into());
    assert!(deck_is_dirty(&state));
    let before = state.session.borrow().document.clone();

    app.set_theme_chooser_open(true);
    app.invoke_create_theme(2);
    assert!(
        app.get_save_changes_open(),
        "the unsaved-changes dialog opens"
    );
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::NewFromTemplate(2))
    );
    assert!(
        presentation_documents_match(&state.session.borrow().document, &before),
        "nothing is replaced until the user chooses"
    );

    // Cancel keeps the deck.
    app.invoke_save_changes_cancel();
    assert!(presentation_documents_match(
        &state.session.borrow().document,
        &before
    ));
    assert_eq!(state.pending_replacement.get(), None);

    // Discard goes on to create the chosen template, not another one.
    app.invoke_create_theme(3);
    app.invoke_save_changes_discard();
    assert_eq!(state.session.borrow().document.len(), 1);
    assert_eq!(
        state.session.borrow().document.slides[0].layout,
        "image-text"
    );
    assert!(state.session.borrow().document.slides[0]
        .elements
        .iter()
        .all(|element| element.content.is_empty()));
    assert!(!deck_is_dirty(&state));
}

#[test]
fn the_palette_offers_new_from_template_and_no_theme_command() {
    let (app, _state) = launched(empty_session());
    let labels: Vec<_> = master_palette(&app)
        .iter()
        .map(|command| command.label)
        .collect();
    assert!(labels.contains(&"New from Template..."), "{labels:?}");
    assert!(
        labels
            .iter()
            .all(|label| !label.to_lowercase().contains("theme")),
        "{labels:?}"
    );

    app.set_palette_query("from template".into());
    app.invoke_palette_invoked(0);
    assert!(
        app.get_theme_chooser_open(),
        "the palette opens the chooser"
    );
}

#[test]
fn nothing_in_the_chooser_or_its_entry_points_is_called_a_theme() {
    let (app, _state) = launched(empty_session());
    app.set_show_inspector(true);
    app.set_inspector_tab(2);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    assert!(
        ElementHandle::find_by_accessible_label(&app, "New from Template...")
            .next()
            .is_some(),
        "the Document tab offers New from Template..."
    );
    for old in ["Choose Theme...", "Choose theme"] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, old)
                .next()
                .is_none(),
            "{old} is gone"
        );
    }

    app.set_theme_chooser_open(true);
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render chooser");
    if let Ok(dir) = std::env::var("LOOM_LAYOUT_DUMP") {
        let _ = image.save(format!("{dir}/present-template-chooser.png"));
    }
    for label in slide_layouts::TEMPLATE_NAMES {
        assert!(
            ElementHandle::find_by_accessible_label(&app, label)
                .next()
                .is_some(),
            "a card named {label}"
        );
    }
    // The aspect-ratio control only changed the editor's view and was never
    // stored or exported, so it is no longer offered here.
    for gone in [
        "Theme Aspect Ratio",
        "Basic White",
        "Basic Black",
        "Dynamic Accent",
    ] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, gone)
                .next()
                .is_none(),
            "{gone} must not be offered"
        );
    }
}
