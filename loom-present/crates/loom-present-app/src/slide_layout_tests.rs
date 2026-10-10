//! New slides and layouts start with empty placeholders, never filler text.

use super::*;
use loom_desktop::ScriptedFileDialogs;
use slint::Model;

const PROMPTS: [&str; 2] = ["Click to add title", "Click to add text"];
const FILLER: [&str; 3] = ["New Slide", "Add your story here", "Untitled Slide"];

fn state_with(session: PresentationSession, dialogs: ScriptedFileDialogs) -> Rc<GuiState> {
    Rc::new(GuiState {
        session: RefCell::new(session.clone()),
        last_saved: RefCell::new(session.document),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(dialogs),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    })
}

fn launched(session: PresentationSession) -> (PresentApp, Rc<GuiState>) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = state_with(session, ScriptedFileDialogs::default());
    wire_app_callbacks(&app, &state);
    refresh_without_recovery(&app, &state);
    (app, state)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn kinds(slide: &loom_present_core::Slide) -> Vec<ElementType> {
    slide
        .elements
        .iter()
        .map(|element| element.element_type.clone())
        .collect()
}

#[test]
fn a_new_slide_has_an_empty_title_and_body_not_filler_text() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();

    let session = state.session.borrow();
    assert_eq!(session.document.len(), 2);
    assert_eq!(session.document.active_index, 1);
    let slide = &session.document.slides[1];
    assert_eq!(
        kinds(slide),
        [ElementType::Title, ElementType::BodyText],
        "title and body placeholders"
    );
    for element in &slide.elements {
        assert!(
            element.content.is_empty(),
            "{} must start empty, not {:?}",
            element.id,
            element.content
        );
    }
    assert_eq!(slide.layout, "content");
    let ids: std::collections::BTreeSet<_> = slide.elements.iter().map(|e| &e.id).collect();
    assert_eq!(ids.len(), slide.elements.len(), "element ids are distinct");
    assert_ne!(slide.id, session.document.slides[0].id);
}

#[test]
fn every_add_slide_route_creates_the_same_empty_slide() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();
    app.invoke_add_slide();
    app.invoke_add_slide();
    let session = state.session.borrow();
    assert_eq!(session.document.len(), 4);
    let ids: std::collections::BTreeSet<_> =
        session.document.slides.iter().map(|s| &s.id).collect();
    assert_eq!(ids.len(), 4, "slide ids stay unique");
    for slide in &session.document.slides[1..] {
        assert!(slide.elements.iter().all(|e| e.content.is_empty()));
        assert_eq!(slide.elements.len(), 2);
    }
}

#[test]
fn the_canvas_prompts_over_a_new_slide_and_the_thumbnail_does_not() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();

    let prompts: Vec<String> = app
        .get_element_placeholders()
        .iter()
        .map(|text| text.to_string())
        .collect();
    assert_eq!(
        prompts, PROMPTS,
        "the canvas shows a prompt per placeholder"
    );
    assert!(app
        .get_element_contents()
        .iter()
        .all(|text| text.is_empty()));

    if let Ok(dir) = std::env::var("LOOM_LAYOUT_DUMP") {
        let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
        let _ = image.save(format!("{dir}/present-new-slide.png"));
    }

    // The strip draws the real slide: its rows carry no prompt text at all.
    let thumbs = app.get_slide_thumbs();
    let row = thumbs.row_data(1).expect("thumbnail of the new slide");
    assert_eq!(row.contents.row_count(), 2);
    for text in row.contents.iter() {
        assert!(text.is_empty(), "thumbnail text {text:?}");
    }
    for text in row.labels.iter() {
        assert!(
            !PROMPTS.iter().any(|prompt| text.contains(prompt)),
            "thumbnail label {text:?}"
        );
    }

    // Typing replaces the prompt.
    app.invoke_select_element(0);
    app.invoke_update_element_content("Real title".into());
    let prompts: Vec<String> = app
        .get_element_placeholders()
        .iter()
        .map(|text| text.to_string())
        .collect();
    assert_eq!(prompts, ["", PROMPTS[1]]);
    drop(state);
}

#[test]
fn exports_omit_prompts_and_filler_for_a_new_slide() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();
    app.invoke_select_slide(0);
    app.invoke_select_element(0);
    app.invoke_update_element_content("Cover words".into());

    let session = state.session.borrow();
    // Page text is glyph ids in the file; read it back through the font maps
    // so the "must not contain" checks below can fail.
    let pdf = export_pdf(&session.document);
    let pdf = loom_pdf::inspect::readable_content(&pdf)
        .expect("readable PDF")
        .into_bytes();
    let pptx = export_pptx(&session).expect("pptx");
    assert!(
        contains(&pdf, "Cover words"),
        "positive control: real text is exported"
    );
    assert!(contains(&pptx, "Cover words"));
    for text in PROMPTS.iter().chain(FILLER.iter()).chain(&["Click to add"]) {
        assert!(!contains(&pdf, text), "PDF must not contain {text:?}");
        assert!(!contains(&pptx, text), "PPTX must not contain {text:?}");
    }
    // The empty second slide is still exported as a slide of its own.
    assert!(contains(&pptx, "ppt/slides/slide2.xml"));
    let titles = loom_present_core::extract_pptx_titles(&pptx).expect("titles");
    assert_eq!(titles.len(), 2);
    assert_eq!(titles[1], "", "an untitled slide has an empty title");
}

#[test]
fn an_empty_new_slide_survives_save_and_reopen_empty() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dir = std::env::temp_dir().join(format!("loom-present-layouts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("deck.loomdeck");
    let state = state_with(
        empty_session(),
        ScriptedFileDialogs::new([], [Some(path.clone())]),
    );
    wire_app_callbacks(&app, &state);
    app.invoke_add_slide();
    assert_eq!(save_current_deck(&app, &state, true), Ok(true));

    let bytes = std::fs::read(&path).expect("saved file");
    for text in PROMPTS.iter().chain(FILLER.iter().take(2)) {
        assert!(!contains(&bytes, text), "file must not contain {text:?}");
    }
    let reopened = load_session(&path).expect("reopen");
    assert_eq!(reopened.document.len(), 2);
    let slide = &reopened.document.slides[1];
    assert_eq!(kinds(slide), [ElementType::Title, ElementType::BodyText]);
    assert!(slide.elements.iter().all(|e| e.content.is_empty()));

    // The reopened slide prompts again, because the prompt is a view of "empty".
    let (reopened_app, _state) = launched(reopened);
    reopened_app.invoke_select_slide(1);
    let prompts: Vec<String> = reopened_app
        .get_element_placeholders()
        .iter()
        .map(|text| text.to_string())
        .collect();
    assert_eq!(prompts, PROMPTS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn duplicating_a_new_slide_keeps_it_empty_and_numbering_intact() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();
    app.invoke_duplicate_slide();
    let session = state.session.borrow();
    assert_eq!(session.document.len(), 3);
    assert_eq!(session.document.active_index, 2);
    let copy = &session.document.slides[2];
    assert!(copy.elements.iter().all(|e| e.content.is_empty()));
    assert_eq!(copy.elements.len(), 2);
    let ids: std::collections::BTreeSet<_> =
        session.document.slides.iter().map(|s| &s.id).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(app.get_slide_count_text(), "3 slides");
}

#[test]
fn undo_removes_the_new_slide_and_redo_restores_its_placeholders() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();
    app.invoke_undo();
    assert_eq!(state.session.borrow().document.len(), 1);
    app.invoke_redo();
    let session = state.session.borrow();
    assert_eq!(session.document.len(), 2);
    assert_eq!(session.document.slides[1].elements.len(), 2);
}

fn choose(app: &PresentApp, index: i32) {
    app.invoke_apply_template(index);
}

#[test]
fn each_layout_creates_empty_placeholders_and_keeps_typed_text() {
    for (index, layout, expected) in [
        (0, "cover", vec![ElementType::Title, ElementType::Subtitle]),
        (
            1,
            "content",
            vec![ElementType::Title, ElementType::BodyText],
        ),
        (
            2,
            "two-column",
            vec![
                ElementType::Title,
                ElementType::BodyText,
                ElementType::BodyText,
            ],
        ),
        (
            3,
            "image-text",
            vec![ElementType::Title, ElementType::BodyText],
        ),
    ] {
        // From a fresh blank slide: only a title exists, so the layout adds the rest.
        let (app, state) = launched(empty_session());
        choose(&app, index);
        let session = state.session.borrow();
        let slide = &session.document.slides[0];
        assert_eq!(slide.layout, layout);
        assert_eq!(kinds(slide), expected, "{layout}");
        assert!(
            slide.elements.iter().all(|e| e.content.is_empty()),
            "{layout} adds no filler"
        );
        let ids: std::collections::BTreeSet<_> = slide.elements.iter().map(|e| &e.id).collect();
        assert_eq!(ids.len(), slide.elements.len(), "{layout}: distinct ids");
        drop(session);

        // From a new slide the user already typed on: their text stays put.
        let (app, state) = launched(empty_session());
        app.invoke_add_slide();
        app.invoke_select_element(1);
        app.invoke_update_element_content("My own words".into());
        choose(&app, index);
        let session = state.session.borrow();
        let slide = session.document.active_slide().expect("slide");
        // Typed body text is kept, so the cover keeps the body and adds its subtitle.
        let mut after_typing = expected.clone();
        if layout == "cover" {
            after_typing.insert(1, ElementType::BodyText);
        }
        assert_eq!(kinds(slide), after_typing, "{layout} after typing");
        let typed: Vec<_> = slide
            .elements
            .iter()
            .filter(|e| !e.content.is_empty())
            .collect();
        assert_eq!(typed.len(), 1);
        assert_eq!(typed[0].content, "My own words");
        assert_eq!(
            (typed[0].x, typed[0].y, typed[0].width, typed[0].height),
            (82.0, 190.0, 760.0, 170.0),
            "{layout}: typed text is not moved"
        );
        assert!(slide
            .elements
            .iter()
            .all(|e| e.width > 0.0 && e.height > 0.0));
    }
}

#[test]
fn the_two_column_layout_puts_empty_columns_side_by_side() {
    let (app, state) = launched(empty_session());
    app.invoke_add_slide();
    choose(&app, 2);
    let session = state.session.borrow();
    let slide = session.document.active_slide().expect("slide");
    let bodies: Vec<_> = slide
        .elements
        .iter()
        .filter(|e| e.element_type == ElementType::BodyText)
        .collect();
    assert_eq!(bodies.len(), 2);
    assert!(
        bodies[0].x + bodies[0].width <= bodies[1].x,
        "columns do not overlap: {} + {} vs {}",
        bodies[0].x,
        bodies[0].width,
        bodies[1].x
    );
    assert_eq!(bodies[0].y, bodies[1].y);
}

#[test]
fn the_sample_deck_keeps_its_real_content() {
    let sample = sample_session();
    let texts: Vec<_> = sample
        .document
        .slides
        .iter()
        .flat_map(|slide| slide.elements.iter())
        .map(|element| element.content.as_str())
        .collect();
    assert_eq!(sample.document.len(), 3);
    assert!(texts.iter().all(|text| !text.is_empty()), "{texts:?}");
    assert!(texts.contains(&"Create without compromise"));
    assert!(texts.contains(&"The creative system"));
    assert!(texts.contains(&"Built around ownership"));

    // And the sample's own prompts never appear: nothing in it is empty.
    let (app, _state) = launched(sample);
    assert!(app
        .get_element_placeholders()
        .iter()
        .all(|text| text.is_empty()));
}
