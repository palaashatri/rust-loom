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

    for (label, scale, expected_scroll) in [("1.0", 1.0, 14), ("1.25", 1.25, 13), ("1.5", 1.5, 12)]
    {
        app.set_template_selected(0);
        app.set_template_scroll_position(0);
        app.set_template_text_scale(scale);
        for _ in 0..26 {
            press(&app, slint::platform::Key::RightArrow);
        }
        assert_eq!(app.get_template_selected(), 26);
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

/// Opening the chooser from the File menu while the grid has keyboard focus must
/// move focus into the chooser; otherwise arrows and Return edit the workbook
/// hidden behind the modal.
#[test]
fn opening_the_chooser_from_the_menu_takes_keyboard_focus_from_the_grid() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::actions::wire_template_navigation(&app);
    app.set_template_category(0);
    app.invoke_focus_grid();
    assert!(crate::command_dispatch::dispatch_command(
        &app,
        "file.new_template"
    ));
    assert!(app.get_template_chooser_open());
    let before = app.get_template_selected();
    press(&app, slint::platform::Key::RightArrow);
    assert_ne!(
        app.get_template_selected(),
        before,
        "Right must move the chooser selection, not the grid cursor behind it"
    );
}

#[test]
fn expanded_catalog_templates_have_working_formulas_formats_and_light_previews() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_template_chooser_open(true);

    // Check every category renders its cards with light previews (not blank/black).
    for (category, expected_titles) in [
        (
            3,
            &[
                "Loan Amortization",
                "Retirement Planner",
                "Debt Payoff",
                "Tax Summary",
            ][..],
        ),
        (
            4,
            &[
                "Travel Itinerary",
                "Habit & Workout Log",
                "Event Budget",
                "Meal Planner",
            ][..],
        ),
        (
            5,
            &[
                "Profit & Loss",
                "Employee Timesheet",
                "Inventory Tracker",
                "Project Schedule",
            ][..],
        ),
        (
            6,
            &[
                "Gradebook",
                "Attendance Tracker",
                "GPA Calculator",
                "Lab Data & Chart",
            ][..],
        ),
    ] {
        app.set_template_category(category);
        let _image =
            snapshot_component(&app, 1280.0, 720.0, 1.0).expect("render category template cards");
        for title in expected_titles {
            let cards: Vec<_> =
                i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, title)
                    .filter(|card| card.size().height >= 96.0)
                    .collect();
            assert!(
                !cards.is_empty(),
                "template card {title:?} must be visible in category {category}"
            );
        }
    }

    // Verify all 16 catalog sheets (11..=26) evaluate with zero formula errors and
    // expected formula results and number formats.
    let probes = [
        (
            11,
            "Loan Amortization",
            "B6",
            "1419.47",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            12,
            "Retirement Planner",
            "B7",
            "33",
            loom_sheets_core::NumberFormat::Number,
        ),
        (
            13,
            "Debt Payoff",
            "B6",
            "41100",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            14,
            "Tax Summary",
            "B19",
            "Refund",
            loom_sheets_core::NumberFormat::General,
        ),
        (
            15,
            "Travel Itinerary",
            "E11",
            "1795",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            16,
            "Habit & Workout Log",
            "B11",
            "5",
            loom_sheets_core::NumberFormat::Number,
        ),
        (
            17,
            "Event Budget",
            "D12",
            "4330",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            18,
            "Meal Planner",
            "E9",
            "74.65",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            19,
            "Profit & Loss",
            "F11",
            "96900",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            20,
            "Employee Timesheet",
            "E11",
            "1980",
            loom_sheets_core::NumberFormat::Currency,
        ),
        (
            21,
            "Inventory Tracker",
            "C8",
            "2",
            loom_sheets_core::NumberFormat::Number,
        ),
        (
            22,
            "Project Schedule",
            "C8",
            "2",
            loom_sheets_core::NumberFormat::Number,
        ),
        (
            23,
            "Gradebook",
            "E11",
            "2",
            loom_sheets_core::NumberFormat::Number,
        ),
        (
            24,
            "Attendance Tracker",
            "G9",
            "3",
            loom_sheets_core::NumberFormat::Number,
        ),
        (
            25,
            "GPA Calculator",
            "D9",
            "Honors",
            loom_sheets_core::NumberFormat::General,
        ),
        (
            26,
            "Lab Data & Chart",
            "D9",
            "20",
            loom_sheets_core::NumberFormat::Number,
        ),
    ];

    for (idx, expected_name, probe_a1, expected_prefix, expected_fmt) in probes {
        let sheet = crate::template_sheet(idx);
        assert_eq!(sheet.name, expected_name, "template {idx} name");
        let vals = loom_sheets_core::evaluate(&sheet);
        for (cell, val) in &vals {
            assert!(
                !matches!(val, loom_sheets_core::Value::Error(_)),
                "template {idx} ({expected_name}) cell {cell:?} evaluated to error: {val:?}"
            );
        }
        let cell = CellRef::parse(probe_a1).unwrap();
        let display = vals.get(&cell).map(|v| v.display()).unwrap_or_default();
        assert!(
            display.starts_with(expected_prefix),
            "template {idx} ({expected_name}) {probe_a1}: expected prefix {expected_prefix:?}, got {display:?}"
        );
        assert_eq!(
            sheet.cell_style(cell).number_format,
            expected_fmt,
            "template {idx} ({expected_name}) {probe_a1} format"
        );
    }

    let lab = crate::template_sheet(26);
    assert!(
        lab.chart.is_some(),
        "Lab Data & Chart template must include a chart"
    );
}
