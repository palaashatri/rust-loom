use super::*;

fn press(app: &SheetsApp, key: slint::platform::Key) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.into() });
}

#[test]
fn keyboard_template_selection_tracks_visible_category_and_create() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::actions::wire_template_navigation(&app);
    app.set_template_category(2);
    app.set_template_selected(0);
    app.set_template_chooser_open(true);
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render template chooser");

    let created = Rc::new(Cell::new(-1));
    let created_ref = created.clone();
    app.on_create_template(move |index| created_ref.set(index));

    press(&app, slint::platform::Key::RightArrow);
    assert_eq!(app.get_template_selected(), 3);

    press(&app, slint::platform::Key::Return);
    assert_eq!(created.get(), 3);
}

#[test]
fn keyboard_template_selection_wraps_within_the_visible_category() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::actions::wire_template_navigation(&app);
    app.set_template_category(2);
    app.set_template_selected(0);
    app.set_template_chooser_open(true);

    press(&app, slint::platform::Key::LeftArrow);
    assert_eq!(app.get_template_selected(), 6);
    assert_eq!(app.get_template_scroll_position(), 1);

    press(&app, slint::platform::Key::RightArrow);
    assert_eq!(app.get_template_selected(), 0);
    assert_eq!(app.get_template_scroll_position(), 0);
}

#[test]
fn keyboard_template_selection_uses_real_recents_and_larger_text_scale() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::actions::wire_template_navigation(&app);
    app.set_template_recents(Rc::new(VecModel::from(vec![9, 2])).into());
    app.set_template_category(1);
    app.set_template_selected(9);
    app.set_template_text_scale(1.5);
    app.set_template_chooser_open(true);

    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render larger-text chooser");
    press(&app, slint::platform::Key::RightArrow);
    assert_eq!(app.get_template_selected(), 2);
    assert_eq!(app.get_template_scroll_position(), 0);
}

#[test]
fn text_scale_cli_accepts_accessibility_sizes_and_rejects_smaller_values() {
    let args = parse_args_from(["--text-scale", "1.5"]).expect("parse larger text size");
    assert_eq!(args.text_scale, 1.5);
    assert!(parse_args_from(["--text-scale", "0.9"]).is_err());
}

#[test]
fn keyboard_selection_scrolls_to_the_selected_template_in_all_templates() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::actions::wire_template_navigation(&app);
    app.set_template_category(0);
    app.set_template_chooser_open(true);

    for (label, scale, expected_scroll) in [("1.0", 1.0, 7), ("1.25", 1.25, 6), ("1.5", 1.5, 5)] {
        app.set_template_selected(0);
        app.set_template_scroll_position(0);
        app.set_template_text_scale(scale);
        for _ in 0..10 {
            press(&app, slint::platform::Key::RightArrow);
        }
        assert_eq!(app.get_template_selected(), 2);
        assert_eq!(app.get_template_scroll_position(), expected_scroll);

        let image = snapshot_component(&app, 1024.0, 720.0, 1.0)
            .expect("render selected template at text scale");
        let output = std::env::temp_dir().join(format!(
            "loom-sheets-template-chooser-keyboard-selection-{label}.png"
        ));
        loom_test_support::png::save_png(&output, &image)
            .expect("save keyboard selection screenshot");
    }
}

#[test]
fn escape_closes_template_chooser_without_replacing_the_workbook() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_template_chooser_open(true);
    app.set_sheet_name("Keep this workbook".into());

    let cancelled = Rc::new(Cell::new(false));
    let cancelled_ref = cancelled.clone();
    app.on_cancel_template(move || cancelled_ref.set(true));
    let created = Rc::new(Cell::new(false));
    let created_ref = created.clone();
    app.on_create_template(move |_| created_ref.set(true));

    press(&app, slint::platform::Key::Escape);

    assert!(cancelled.get());
    assert!(!created.get());
    assert_eq!(app.get_sheet_name(), "Keep this workbook");
}

/// UI-31: the Checklist card must preview the workbook it creates. It previously
/// drew an empty dark grid while `template_sheet(3)` generates a light
/// two-column task table with headers, four task rows, and summary rows.
#[test]
fn the_checklist_card_previews_the_workbook_it_creates() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_template_chooser_open(true);
    app.set_template_category(2); // Basic
    app.set_template_selected(3); // Checklist
    let image = snapshot_component(&app, 1280.0, 720.0, 1.0).expect("render the Basic section");

    // The card is a button labelled with its template name; its upper band is
    // the preview frame.
    let cards: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, "Checklist")
            .collect();
    // The chooser also lists the template name as text, so keep only the card
    // itself: a button whose frame is at least the preview's height.
    let cards: Vec<_> = cards
        .into_iter()
        .filter(|card| card.size().height >= 96.0)
        .collect();
    assert!(!cards.is_empty(), "the Checklist card must be visible");
    // Every place the card appears must preview the real worksheet: Recents and
    // the category section both used the same preview component.
    for card in cards {
        let position = card.absolute_position();
        let size = card.size();

        // `BlankBlackPreview` painted #18181b over most of the frame. The
        // created worksheet is a light table, so the preview must be light.
        let mut dark = 0;
        let mut light = 0;
        for step in 1..=12 {
            let x = (position.x + 6.0 + (size.width - 12.0) * (step as f32 / 13.0)) as u32;
            for y in [
                (position.y + 4.0) as u32,
                (position.y + size.height * 0.25) as u32,
                (position.y + size.height * 0.5) as u32,
            ] {
                let pixel = image.get_pixel(x, y);
                let luminance =
                    (u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])) / 3;
                if luminance < 80 {
                    dark += 1;
                } else {
                    light += 1;
                }
            }
        }
        assert!(
            light > dark * 4,
            "the Checklist preview must be a light worksheet, not a dark grid: {light} light vs {dark} dark samples"
        );
    }

    // Identity must stay bound to the template ID, not to display order, and
    // that ID must still create the advertised sheet.
    let created = std::rc::Rc::new(std::cell::RefCell::new(None));
    let sink = created.clone();
    app.on_create_template(move |idx| {
        sink.borrow_mut().replace(idx);
    });
    app.invoke_create_template(3);
    assert_eq!(created.borrow().as_ref().copied(), Some(3));
    let sheet = crate::template_sheet(3);
    assert_eq!(sheet.name, "Checklist");
    let raw = |a1: &str| {
        CellRef::parse(a1)
            .and_then(|cell| sheet.raw(cell))
            .map(str::to_string)
    };
    assert_eq!(raw("A1").as_deref(), Some("Task"));
    assert_eq!(raw("B1").as_deref(), Some("Done"));
    assert_eq!(raw("A3").as_deref(), Some("Pay rent"));
    assert_eq!(raw("B7").as_deref(), Some("=COUNTIF(B2:B5, \"x\")"));
}
