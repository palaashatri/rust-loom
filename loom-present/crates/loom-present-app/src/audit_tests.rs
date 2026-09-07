use super::*;
use loom_desktop::{CommandSource, MenuItem, NativeMenuBar};
use loom_present_core::{
    export_pdf, export_pptx_from_titles, load_presentation, save_presentation, ElementType,
    PresentationDocument, PresentationSession, PresenterSession, SlideAspectRatio, SlideElement,
};

fn test_audit_doc() -> PresentationDocument {
    let mut doc = PresentationDocument::new("audit-deck", "Loom Present Suite");
    // Slide 1: Title slide
    let slide1 = doc.active_slide_mut().expect("slide 1");
    slide1.title = "Architecture Overview".into();
    slide1.speaker_notes = "Welcome to the Loom Present deep audit.".into();
    slide1.elements.clear();
    slide1.elements.push(SlideElement {
        id: "title-1".into(),
        element_type: ElementType::Title,
        content: "Loom Present".into(),
        x: 100.0,
        y: 120.0,
        width: 800.0,
        height: 80.0,
        rotation_deg: 0.0,
        action: None,
    });
    slide1.elements.push(SlideElement {
        id: "subtitle-1".into(),
        element_type: ElementType::Subtitle,
        content: "High performance native presentations".into(),
        x: 100.0,
        y: 220.0,
        width: 800.0,
        height: 50.0,
        rotation_deg: 0.0,
        action: None,
    });

    // Slide 2: Diagram slide with shapes
    doc.add_slide("Components", "System design details");
    let slide2 = doc.slides.get_mut(1).expect("slide 2");
    slide2.speaker_notes = "Discussing modular UI and vector graphics.".into();
    slide2.elements.push(SlideElement {
        id: "rect-1".into(),
        element_type: ElementType::ShapeRectangle,
        content: "Slint UI Layer".into(),
        x: 150.0,
        y: 200.0,
        width: 250.0,
        height: 120.0,
        rotation_deg: 0.0,
        action: None,
    });
    slide2.elements.push(SlideElement {
        id: "rect-2".into(),
        element_type: ElementType::ShapeRectangle,
        content: "Core Engine".into(),
        x: 450.0,
        y: 200.0,
        width: 250.0,
        height: 120.0,
        rotation_deg: 0.0,
        action: None,
    });

    // Ensure slide 0 is the active slide for session tests
    doc.active_index = 0;
    doc
}

#[test]
fn test_deep_audit_scene_graph_and_structure() {
    let doc = test_audit_doc();
    assert_eq!(doc.len(), 2);
    assert_eq!(doc.title, "Loom Present Suite");

    let slide1 = &doc.slides[0];
    assert_eq!(slide1.elements.len(), 2);
    assert_eq!(slide1.elements[0].id, "title-1");
    assert_eq!(slide1.elements[0].element_type, ElementType::Title);

    let slide2 = &doc.slides[1];
    assert_eq!(slide2.elements.len(), 2);
    assert_eq!(slide2.elements[0].element_type, ElementType::ShapeRectangle);

    // Bounding box hit testing
    assert!(slide1.elements[0].contains_point(150.0, 150.0));
    assert!(!slide1.elements[0].contains_point(50.0, 50.0));
}

#[test]
fn test_deep_audit_selection_and_marquee() {
    let mut session = PresentationSession::new(test_audit_doc());

    // Select single element
    session.select_element("title-1", false);
    assert_eq!(session.selected_elements.len(), 1);
    assert!(session.selected_elements.contains(&"title-1".to_string()));
    assert!(!session
        .selected_elements
        .contains(&"subtitle-1".to_string()));

    // Multi-selection with shift
    session.select_element("subtitle-1", true);
    assert_eq!(session.selected_elements.len(), 2);
    assert!(session.selected_elements.contains(&"title-1".to_string()));
    assert!(session
        .selected_elements
        .contains(&"subtitle-1".to_string()));

    // Marquee hit testing
    session.marquee_select(90.0, 100.0, 820.0, 180.0, false);
    assert_eq!(session.selected_elements.len(), 2);

    // Deselection
    session.clear_selection();
    assert!(session.selected_elements.is_empty());
}

#[test]
fn test_deep_audit_undo_redo_history_invariants() {
    let mut session = PresentationSession::new(test_audit_doc());
    let initial_digest = session.document.integrity_digest();

    assert!(!session.can_undo());
    assert!(!session.can_redo());

    // Mutation: Transform element
    assert!(session.transform_element("title-1", 120.0, 140.0, 850.0, 90.0));
    assert!(session.can_undo());
    assert!(!session.can_redo());
    assert_ne!(session.document.integrity_digest(), initial_digest);

    // Undo restores initial state
    assert!(session.undo());
    assert_eq!(session.document.integrity_digest(), initial_digest);
    assert!(!session.can_undo());
    assert!(session.can_redo());

    // Redo reapplies change
    assert!(session.redo());
    assert!(session.can_undo());
    assert!(!session.can_redo());
    let redo_digest = session.document.integrity_digest();
    assert_ne!(redo_digest, initial_digest);

    // Cancelled checkpoint leaves no trace, restoring checkpoint state
    session.checkpoint();
    assert!(session.transform_element_no_checkpoint("title-1", 200.0, 200.0, 500.0, 50.0));
    assert!(session.cancel_checkpoint());
    assert_eq!(session.document.integrity_digest(), redo_digest);
}

#[test]
fn test_deep_audit_speaker_notes_and_search() {
    let doc = test_audit_doc();
    let search_results = doc.search_speaker_notes("modular UI", false);
    assert!(!search_results.is_empty());
    assert_eq!(search_results[0].0, 1);

    let empty_search = doc.search_speaker_notes("nonexistent query text", false);
    assert!(empty_search.is_empty());

    let md = doc.speaker_notes_markdown();
    assert!(md.contains("# Speaker Notes: Loom Present Suite"));
    assert!(md.contains("modular UI and vector graphics"));
}

#[test]
fn test_deep_audit_persistence_roundtrip() {
    let doc = test_audit_doc();
    let bytes = save_presentation(&doc).expect("save presentation");
    assert!(!bytes.is_empty());

    let loaded = load_presentation(&bytes).expect("load presentation");
    assert_eq!(loaded.title, doc.title);
    assert_eq!(loaded.len(), doc.len());
    assert_eq!(
        loaded.slides[0].elements.len(),
        doc.slides[0].elements.len()
    );
    assert_eq!(loaded.slides[0].speaker_notes, doc.slides[0].speaker_notes);
    assert_eq!(loaded.integrity_digest(), doc.integrity_digest());
}

#[test]
fn test_deep_audit_pptx_and_pdf_export() {
    let doc = test_audit_doc();
    let pdf_bytes = export_pdf(&doc);
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    let titles: Vec<String> = doc.slides.iter().map(|s| s.title.clone()).collect();
    let pptx_bytes = export_pptx_from_titles(&titles).expect("export pptx");
    assert!(pptx_bytes.starts_with(b"PK")); // ZIP container
}

#[test]
fn test_deep_audit_macos_global_menu_bar_command_projection() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let menu = NativeMenuBar::new();
    let bar = build_present_menu_bar();
    menu.install_menu_bar(&bar).expect("install menu bar");

    let state = GuiState {
        session: RefCell::new(PresentationSession::new(test_audit_doc())),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(Some(PathBuf::from("audit.loomdeck"))),
        dialogs,
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };

    // Initial sync
    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu bar");

    // File menu
    assert!(matches!(
        installed.find_item("file.new"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("file.save"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("file.export_pdf"),
        Some(MenuItem::Action { enabled: true, .. })
    ));

    // Undo/Redo reflects initial history
    assert!(matches!(
        installed.find_item("edit.undo"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("edit.redo"),
        Some(MenuItem::Action { enabled: false, .. })
    ));

    // Slide navigation: active index 0 out of 2 slides -> next enabled, prev disabled
    assert!(matches!(
        installed.find_item("slide.prev"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.next"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.delete"),
        Some(MenuItem::Action { enabled: true, .. })
    ));

    // Move to slide index 1 -> prev enabled, next disabled
    state.session.borrow_mut().document.select_slide(1);
    sync_menu_state(&menu, &app, &state);
    let updated = menu.installed_menu_bar().expect("updated menu bar");
    assert!(matches!(
        updated.find_item("slide.prev"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        updated.find_item("slide.next"),
        Some(MenuItem::Action { enabled: false, .. })
    ));

    // Disabled guard check: dispatching unhandled command fails
    let err = menu
        .dispatch_action_from("edit.paste", CommandSource::Menu)
        .expect_err("disabled");
    assert!(err.to_string().contains("disabled"));
}

#[test]
fn test_deep_audit_presenter_mode_and_aspect_ratios() {
    assert_eq!(
        SlideAspectRatio::Widescreen16x9.dimensions(),
        (960.0, 540.0)
    );
    assert_eq!(SlideAspectRatio::Widescreen16x9.ratio(), (16, 9));
    assert_eq!(SlideAspectRatio::Standard4x3.dimensions(), (720.0, 540.0));
    assert_eq!(SlideAspectRatio::Standard4x3.ratio(), (4, 3));

    let session = PresenterSession::new(2);
    assert_eq!(session.current_slide_index, 0);
    assert_eq!(session.total_slides, 2);
}
