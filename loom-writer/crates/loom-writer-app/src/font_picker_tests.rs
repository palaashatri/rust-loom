//! The font family control: what the list holds and in what order, how a
//! choice reaches the document, and how the page and the list follow the
//! installed fonts.

use super::*;
use crate::actions_tests::{test_state, text_document};
use crate::document_formatting::{set_selection_font_family, DocumentSelection};
use crate::{FontPicker, FontPickerLine, GuiState, WriterApp};
use loom_writer_core::fonts::{FamilyResolution, FamilyStanding};
use loom_writer_core::test_fonts;
use loom_writer_core::TextSelection;
use slint::Model;
use std::rc::Rc;

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Resolution against a pretend machine that has exactly `installed`.
fn resolver(installed: &'static [&'static str]) -> impl Fn(&str) -> FamilyResolution {
    move |name: &str| {
        let present = name.trim().is_empty()
            || name.eq_ignore_ascii_case("inter")
            || installed
                .iter()
                .any(|known| known.eq_ignore_ascii_case(name));
        FamilyResolution {
            requested: name.to_owned(),
            drawn: if present && !name.trim().is_empty() {
                name.to_owned()
            } else {
                "Inter".to_owned()
            },
            standing: if present {
                FamilyStanding::Installed
            } else {
                FamilyStanding::NotInstalled
            },
        }
    }
}

const INSTALLED: [&str; 5] = ["Arial", "Courier", "Garamond", "Georgia", "Inter"];

fn all_installed() -> Vec<String> {
    names(&INSTALLED)
}

#[test]
fn recent_fonts_come_first_then_the_documents_then_everything_installed() {
    let resolve = resolver(&["Arial", "Courier", "Georgia", "Inter"]);
    let rows = build_rows(
        &Inputs {
            recents: &names(&["Georgia", "Courier"]),
            document: &names(&["Garamond", "Georgia"]),
            installed: &all_installed(),
            query: "",
        },
        &resolve,
    );
    let shape: Vec<(Section, &str, &str)> = rows
        .iter()
        .map(|row| (row.section, row.value.as_str(), row.tag))
        .collect();
    assert_eq!(
        shape,
        [
            (Section::Recent, "Georgia", TAG_IN_USE),
            (Section::Recent, "Courier", ""),
            (Section::InDocument, "", TAG_IN_USE),
            (Section::InDocument, "Garamond", TAG_MISSING),
            // Garamond is not installed here, but it is still in the list of all
            // fonts only if it is installed: the catalogue lists what exists.
            (Section::AllFonts, "Arial", ""),
            (Section::AllFonts, "Inter", ""),
        ]
    );
    // The family a document asks for keeps its own name in the row, and the
    // preview is the stand-in that will really be drawn.
    let missing = rows.iter().find(|row| row.value == "Garamond").unwrap();
    assert_eq!(missing.label, "Garamond");
    assert_eq!(missing.preview, "Inter");
    let default = rows.iter().find(|row| row.value.is_empty()).unwrap();
    assert_eq!(default.label, "Default (Inter)");
}

#[test]
fn typing_filters_and_offers_an_unknown_name() {
    let resolve = resolver(&["Arial", "Courier", "Georgia", "Inter"]);
    let installed = all_installed();
    let inputs = |query| Inputs {
        recents: &[],
        document: &[],
        installed: &installed,
        query,
    };
    let values = |rows: &[Row]| rows.iter().map(|row| row.value.clone()).collect::<Vec<_>>();
    // Case does not matter, and names that start with the text come first.
    let rows = build_rows(&inputs("GA"), &resolve);
    assert_eq!(values(&rows)[0], "Garamond");
    assert!(
        rows.iter().all(|row| row.section != Section::Typed)
            || rows.last().unwrap().label == "Use \"GA\""
    );
    // A name that is not installed can still be used: the document keeps it.
    let rows = build_rows(&inputs("Palatino"), &resolve);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].section, Section::Typed);
    assert_eq!(rows[0].value, "Palatino");
    assert_eq!(rows[0].label, "Use \"Palatino\"");
    assert_eq!(rows[0].tag, TAG_MISSING);
    // An exact (case-insensitive) match is the font itself, not a typed name.
    let rows = build_rows(&inputs("georgia"), &resolve);
    assert_eq!(values(&rows), ["Georgia"]);
}

#[test]
fn the_window_keeps_the_highlight_in_view_and_a_section_with_its_heading() {
    let resolve = resolver(&INSTALLED);
    let installed: Vec<String> = (0..40).map(|n| format!("Family {n:02}")).collect();
    let rows = build_rows(
        &Inputs {
            recents: &[],
            document: &[],
            installed: &installed,
            query: "",
        },
        &resolve,
    );
    let lines = layout(&rows);
    // Headings: "In this document", "All fonts".
    let headings = lines
        .iter()
        .filter(|l| matches!(l, Line::Heading(_)))
        .count();
    assert_eq!(headings, 2);
    assert_eq!(lines.len(), rows.len() + 2);
    // The highlight on the first font of a section brings its heading along.
    let first_all = rows
        .iter()
        .position(|r| r.section == Section::AllFonts)
        .unwrap();
    let first = scrolled(&lines, first_all, 0, WINDOW_LINES);
    let heading_at = lines
        .iter()
        .position(|l| matches!(l, Line::Heading("All fonts")))
        .unwrap();
    assert!(first <= heading_at && heading_at < first + WINDOW_LINES);
    // Moving down scrolls by the minimum, keeping the highlight on screen.
    let mut first = 0;
    for selected in 0..rows.len() {
        let next = scrolled(&lines, selected, first, WINDOW_LINES);
        assert!(next >= first, "the list never jumps back while moving down");
        assert!(next - first <= 3, "it scrolls a line or two at a time");
        let at = lines
            .iter()
            .position(|l| matches!(l, Line::Item(i) if *i == selected))
            .unwrap();
        assert!(
            at >= next && at < next + WINDOW_LINES,
            "{selected} is visible"
        );
        first = next;
    }
}

#[test]
fn only_printable_keys_open_the_list_filtered() {
    assert!(is_printable("g"));
    assert!(is_printable("7"));
    assert!(is_printable("\u{e9}"));
    assert!(!is_printable(" "));
    assert!(
        !is_printable("\u{f700}"),
        "arrow keys are private-use characters"
    );
    assert!(!is_printable("\u{8}"));
    assert!(!is_printable("ab"));
    assert!(!is_printable(""));
}

// --- the window -------------------------------------------------------------

fn session(text: &str) -> (WriterApp, Rc<GuiState>) {
    crate::typing_style::clear();
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(text), dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    (app, state)
}

fn lines(app: &WriterApp) -> Vec<FontPickerLine> {
    app.global::<FontPicker>().get_lines().iter().collect()
}

fn index_of(app: &WriterApp, label: &str) -> i32 {
    lines(app)
        .iter()
        .find(|line| !line.heading && line.label == label)
        .unwrap_or_else(|| {
            panic!(
                "{label:?} is not in {:?}",
                lines(app)
                    .iter()
                    .map(|l| l.label.to_string())
                    .collect::<Vec<_>>()
            )
        })
        .index
}

fn family_runs(state: &GuiState) -> Vec<(usize, usize, String)> {
    state.current.borrow().blocks[0]
        .runs
        .iter()
        .map(|run| (run.start, run.end, run.style.font_family.clone()))
        .collect()
}

#[test]
fn choosing_a_family_for_a_selection_is_one_undoable_edit() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let (app, state) = session("Hello world");
    app.invoke_selection_changed(0, 5);
    let picker = app.global::<FontPicker>();

    picker.invoke_open_requested();
    assert!(picker.get_open());
    assert_eq!(picker.get_current().as_str(), "Inter");
    // Everything installed is listed, the bundled Inter among it.
    picker.invoke_query_edited("wid".into());
    let index = index_of(&app, "Wider");
    picker.invoke_choose(index);

    assert!(!picker.get_open(), "choosing closes the list");
    assert_eq!(
        family_runs(&state),
        [(0, 5, "Wider".to_owned()), (5, 11, String::new())]
    );
    assert_eq!(picker.get_current().as_str(), "Wider");
    let undo_steps = state.history.borrow().undo_len();
    assert_eq!(undo_steps, 1, "the whole selection is one history entry");

    app.invoke_undo();
    assert!(
        family_runs(&state)
            .iter()
            .all(|(_, _, family)| family.is_empty()),
        "one undo puts the family back"
    );
    assert_eq!(picker.get_current().as_str(), "Inter");
}

#[test]
fn a_selection_with_two_families_reads_mixed() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let (app, state) = session("Hello world");
    {
        let mut document = state.current.borrow().clone();
        set_selection_font_family(&mut document, DocumentSelection::range(6, 11), "Wider");
        *state.current.borrow_mut() = document;
    }
    app.invoke_selection_changed(0, 11);
    crate::apply_state(&app, &state);
    let picker = app.global::<FontPicker>();
    assert_eq!(picker.get_current().as_str(), "Mixed");
    app.invoke_selection_changed(6, 11);
    crate::apply_state(&app, &state);
    assert_eq!(picker.get_current().as_str(), "Wider");
    assert_eq!(picker.get_current_note().as_str(), "");
}

#[test]
fn a_family_chosen_at_a_caret_is_taken_by_the_text_typed_next() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let (app, state) = session("Hello");
    app.invoke_selection_changed(5, 5);
    let picker = app.global::<FontPicker>();
    picker.invoke_open_requested();
    picker.invoke_query_edited("Wider".into());
    picker.invoke_choose(index_of(&app, "Wider"));
    assert!(
        family_runs(&state).is_empty(),
        "no text changed, so no history entry"
    );
    assert_eq!(state.history.borrow().undo_len(), 0);
    assert_eq!(app.get_status_right().as_str(), "Wider for new text");
    assert_eq!(
        picker.get_current().as_str(),
        "Wider",
        "the field shows what typing will use"
    );

    app.invoke_document_edited("HelloX".into(), 6, 6);
    assert_eq!(
        family_runs(&state),
        [(0, 5, String::new()), (5, 6, "Wider".to_owned())],
        "only the typed character takes the family"
    );
}

#[test]
fn a_font_the_document_names_but_this_machine_lacks_is_marked_not_installed() {
    let _fonts = test_fonts::install(&[]);
    let (app, state) = session("Hello world");
    {
        let mut document = state.current.borrow().clone();
        set_selection_font_family(
            &mut document,
            DocumentSelection::range(0, 5),
            "Zzyzx Display",
        );
        document.set_selection(TextSelection::range(0, 5));
        *state.current.borrow_mut() = document;
    }
    crate::apply_state(&app, &state);
    let picker = app.global::<FontPicker>();
    assert_eq!(picker.get_current().as_str(), "Zzyzx Display");
    assert_eq!(picker.get_current_note().as_str(), "Not installed");

    picker.invoke_open_requested();
    let all = lines(&app);
    let row = all
        .iter()
        .find(|line| line.label == "Zzyzx Display")
        .expect("the document's font is listed");
    assert_eq!(row.tag.as_str(), "Not installed");
    assert!(
        all.iter()
            .any(|line| line.heading && line.label == "In this document"),
        "under its own heading"
    );
    // The highlight starts on the font in use.
    assert_eq!(picker.get_selected(), row.index);
}

#[test]
fn the_keyboard_opens_filters_moves_and_chooses() {
    let _fonts = test_fonts::install(&[("Wider", 200), ("Wideish", 120)]);
    let (app, state) = session("Hello world");
    app.invoke_selection_changed(0, 5);
    let picker = app.global::<FontPicker>();

    // A letter typed on the closed field opens the list filtered by it.
    picker.invoke_type_ahead("w".into());
    assert!(picker.get_open());
    assert_eq!(picker.get_query().as_str(), "w");
    let shown: Vec<String> = lines(&app)
        .iter()
        .filter(|line| !line.heading)
        .map(|line| line.label.to_string())
        .collect();
    assert!(
        shown.iter().all(|label| label.to_lowercase().contains('w')),
        "{shown:?}"
    );
    assert_eq!(picker.get_selected(), 0, "the first match is highlighted");

    // Down moves to the next match; it stops at the end instead of wrapping.
    picker.invoke_move(1);
    assert_eq!(picker.get_selected(), 1);
    picker.invoke_move(100);
    assert_eq!(picker.get_selected(), picker.get_match_count() - 1);
    picker.invoke_move(-100);
    assert_eq!(picker.get_selected(), 0);

    // Enter chooses the highlighted font.
    let chosen = lines(&app)
        .iter()
        .find(|line| !line.heading && line.index == picker.get_selected())
        .map(|line| line.label.to_string())
        .unwrap();
    picker.invoke_choose(picker.get_selected());
    assert_eq!(family_runs(&state)[0].2, chosen);

    // Escape closes without changing anything.
    picker.invoke_open_requested();
    picker.invoke_close_requested();
    assert!(!picker.get_open());
    assert_eq!(family_runs(&state)[0].2, chosen);
}

#[test]
fn the_page_re_measures_when_the_scan_delivers_the_installed_fonts() {
    let fonts = test_fonts::install(&[]);
    let (app, state) = session("Hello world");
    crate::font_picker::attach_for_test(&app, &state);
    {
        let mut document = state.current.borrow().clone();
        set_selection_font_family(&mut document, DocumentSelection::range(0, 11), "Wider");
        *state.current.borrow_mut() = document;
    }
    crate::apply_state(&app, &state);
    let first_row_family = |app: &WriterApp| {
        app.get_render_blocks()
            .iter()
            .next()
            .unwrap()
            .font_family
            .to_string()
    };
    let advance = || {
        let document = state.current.borrow();
        loom_writer_core::text_advance("Hello world", 0, &document.blocks[0].runs, 11.0)
    };
    let narrow = advance();
    assert_eq!(
        first_row_family(&app),
        "",
        "not installed: drawn in the document font"
    );
    assert_eq!(
        app.global::<FontPicker>().get_current_note().as_str(),
        "Not installed"
    );

    // The scan finishes: the family now exists, and everything follows.
    fonts.add(&[("Wider", 200)]);
    crate::font_picker::catalogue_ready(loom_writer_core::fonts::font_catalog());
    assert_eq!(first_row_family(&app), "Wider");
    assert!(
        advance() > narrow * 1.5,
        "the line is measured in Wider now"
    );
    assert_eq!(app.global::<FontPicker>().get_current_note().as_str(), "");
}

#[test]
fn a_line_with_two_families_is_drawn_as_rows_placed_by_the_measurement() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let (app, state) = session("Hello wide world");
    {
        let mut document = state.current.borrow().clone();
        set_selection_font_family(&mut document, DocumentSelection::range(6, 10), "Wider");
        *state.current.borrow_mut() = document;
    }
    crate::apply_state(&app, &state);
    let rows: Vec<_> = app.get_render_blocks().iter().collect();
    let families: Vec<String> = rows.iter().map(|row| row.font_family.to_string()).collect();
    assert_eq!(
        families,
        ["", "Wider", ""],
        "three stretches of the one line"
    );
    // Each stretch starts where the measurement says its first glyph is.
    let document = state.current.borrow();
    let runs = &document.blocks[0].runs;
    let before = loom_writer_core::text_advance("Hello ", 0, runs, 11.0);
    // The third stretch starts after the space that follows the wide word: the
    // page markup strips a row's leading white space, so it is not drawn.
    let through = loom_writer_core::text_advance("Hello wide ", 0, runs, 11.0);
    assert!(
        (rows[1].x - before).abs() < 0.05,
        "{} against {before}",
        rows[1].x
    );
    assert!(
        (rows[2].x - through).abs() < 0.05,
        "{} against {through}",
        rows[2].x
    );
    assert!(rows[1].x > rows[0].x && rows[2].x > rows[1].x);
}

#[test]
fn a_pdf_export_says_once_which_families_it_drew_in_inter() {
    let directory =
        std::env::temp_dir().join(format!("loom-writer-pdf-note-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("note.pdf");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(path.clone())],
    ));
    let mut document = text_document("Set in Georgia and Garamond");
    set_selection_font_family(&mut document, DocumentSelection::range(7, 14), "Georgia");
    set_selection_font_family(&mut document, DocumentSelection::range(19, 27), "Garamond");
    let (app, state) = test_state(document, dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    app.invoke_export_pdf();
    let status = app.get_status_left().to_string();
    assert!(path.is_file(), "the PDF was written: {status}");
    assert!(
        status
            .ends_with("PDF export used Inter for Georgia, Garamond (embedding not supported yet)"),
        "{status}"
    );
    assert_eq!(status.matches("PDF export used Inter").count(), 1);

    // A document in the document font gets no note.
    assert_eq!(
        crate::pdf_export_message(&path, &text_document("plain")),
        format!("Exported {}", path.display())
    );
    let _ = std::fs::remove_dir_all(&directory);
}
