//! Every workbook-changing callback is refused, with the paused status and no
//! change, while recovery is paused; read-only actions stay available; and a
//! callback nobody classified cannot appear unnoticed.

use super::recovery_pause_ui_tests::{pause_recovery, Session};
use super::*;
use crate::mutation_guard::{ALLOWED, MUTATING};
use loom_production::fault_injection::{self, FaultStep};
use std::collections::BTreeSet;

fn wire_everything(session: &Session) {
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let (app, state) = (&session.app, &session.state);
    register_sheet_actions(app, state, &menu_service);
    register_history_actions(app, state, &menu_service);
    object_actions::register_object_actions(app, state, &menu_service);
    crate::cell_actions::register_quick_edit_actions(app, state, &menu_service);
}

fn fingerprint(state: &GuiState) -> String {
    let (sheets, active) = workbook_sheets(state);
    format!(
        "{}|undo {}|redo {}|rev {}",
        workbook_to_json(&sheets, active),
        state.undo_stack.borrow().len(),
        state.redo_stack.borrow().len(),
        state.worker_revision.get()
    )
}

type Action = Box<dyn Fn(&SheetsApp)>;

fn table() -> Vec<(&'static str, Action)> {
    fn a(name: &'static str, f: impl Fn(&SheetsApp) + 'static) -> (&'static str, Action) {
        (name, Box::new(f))
    }
    vec![
        a("add_row", |app| app.invoke_add_row()),
        a("add_sheet", |app| app.invoke_add_sheet()),
        a("add_table_col", |app| app.invoke_add_table_col()),
        a("adjust_col_width", |app| app.invoke_adjust_col_width(10)),
        a("adjust_decimals", |app| app.invoke_adjust_decimals(1)),
        a("adjust_font", |app| app.invoke_adjust_font(1)),
        a("adjust_row_height", |app| app.invoke_adjust_row_height(5)),
        a("clear_selected_cells", |app| {
            app.invoke_clear_selected_cells()
        }),
        a("commit_selected_cell", |app| {
            app.invoke_commit_selected_cell("zzz".into())
        }),
        a("create_template", |app| app.invoke_create_template(1)),
        a("cut_selection", |app| app.invoke_cut_selection()),
        a("cycle_chart_kind", |app| app.invoke_cycle_chart_kind()),
        a("cycle_fill", |app| app.invoke_cycle_fill()),
        a("delete_col", |app| app.invoke_delete_col()),
        a("delete_row", |app| app.invoke_delete_row()),
        a("delete_sheet", |app| app.invoke_delete_sheet()),
        a("fill_selection", |app| app.invoke_fill_selection()),
        a("freeze_panes", |app| app.invoke_freeze_panes()),
        a("header_autofit", |app| {
            app.global::<GridPointer>().invoke_header_autofit(false, 0)
        }),
        a("header_resized", |app| {
            app.global::<GridPointer>()
                .invoke_header_resized(false, 0, 40.0, true)
        }),
        a("insert_chart", |app| app.invoke_insert_chart()),
        a("insert_image", |app| app.invoke_insert_image()),
        a("insert_shape", |app| app.invoke_insert_shape()),
        a("object_keyboard_action", |app| {
            app.invoke_object_keyboard_action(0, -1, 0, 0)
        }),
        a("object_move_ended", |app| app.invoke_object_move_ended(0)),
        a("object_move_started", |app| {
            app.invoke_object_move_started(0)
        }),
        a("object_resize_ended", |app| {
            app.invoke_object_resize_ended(0)
        }),
        a("object_resize_started", |app| {
            app.invoke_object_resize_started(0)
        }),
        a("organize", |app| app.invoke_organize()),
        a("paste_selection", |app| app.invoke_paste_selection()),
        a("pivot_summary", |app| app.invoke_pivot_summary(0)),
        a("quick_formula", |app| {
            app.invoke_quick_formula("SUM".into())
        }),
        a("redo", |app| app.invoke_redo()),
        a("rename_sheet", |app| {
            app.invoke_rename_sheet("Renamed".into())
        }),
        a("set_cell_alignment", |app| app.invoke_set_cell_alignment(1)),
        a("set_cell_format", |app| app.invoke_set_cell_format(1)),
        a("set_fill", |app| app.invoke_set_fill(1)),
        a("sort_ascending", |app| app.invoke_sort_ascending()),
        a("sort_descending", |app| app.invoke_sort_descending()),
        a("toggle_bold", |app| app.invoke_toggle_bold()),
        a("toggle_borders", |app| app.invoke_toggle_borders()),
        a("toggle_italic", |app| app.invoke_toggle_italic()),
        a("toggle_underline", |app| app.invoke_toggle_underline()),
        a("undo", |app| app.invoke_undo()),
        a("unfreeze_panes", |app| app.invoke_unfreeze_panes()),
    ]
}

/// A paused session with something to undo and redo, a table to sort, a shape
/// to move, and a two-column range selected.
fn paused_session(name: &str) -> Session {
    let session = Session::new(name);
    wire_everything(&session);
    session.commit("B2", "2");
    session.commit("B3", "3");
    session.settle();
    session.app.invoke_undo();
    session.settle();
    {
        let mut sheet = session.state.current.borrow().clone();
        sheet.set_str("A1", "Item");
        sheet.set_str("A2", "x");
        sheet.set_str("A3", "y");
        sheet.objects.push(loom_sheets_core::SheetObject::shape(
            CellRef::parse("D2").unwrap(),
            "Shape",
        ));
        *session.state.current.borrow_mut() = sheet;
    }
    // Pause through a failing full checkpoint so the undo and redo stacks stay.
    fault_injection::inject(
        &crate::cell_edit_recovery::versioned_directory_for(&session.recovery_dir).unwrap(),
        FaultStep::CheckpointPayload,
        0,
        1,
    );
    record_workbook_snapshot(&session.state).expect("queue checkpoint");
    session.settle();
    assert!(session.app.get_recovery_paused());
    {
        let sheet = session.state.current.borrow();
        let selection =
            GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("B3").unwrap());
        update_selection_range(&session.app, &sheet, &evaluate(&sheet), selection);
    }
    assert!(!session.state.undo_stack.borrow().is_empty());
    assert_eq!(session.state.redo_stack.borrow().len(), 1);
    session
}

#[test]
fn the_action_table_covers_exactly_the_mutating_callbacks() {
    let names: BTreeSet<&str> = table().iter().map(|(name, _)| *name).collect();
    let listed: BTreeSet<&str> = MUTATING.iter().copied().collect();
    assert_eq!(names, listed);
    assert_eq!(listed.len(), MUTATING.len(), "no duplicate entries");
}

#[test]
fn every_mutating_callback_is_refused_while_recovery_is_paused() {
    let session = paused_session("guard-every-callback");
    for (name, action) in table() {
        let before = fingerprint(&session.state);
        session.app.set_status_left("untouched".into());
        action(&session.app);
        assert_eq!(
            fingerprint(&session.state),
            before,
            "{name} changed the workbook while recovery was paused"
        );
        let status = session.app.get_status_left();
        assert!(
            status.starts_with("Edit not applied") && status.contains("Retry Recovery"),
            "{name} must say it was refused: {status}"
        );
        assert!(session.app.get_recovery_paused(), "{name}");
    }
}

#[test]
fn the_shared_command_dispatcher_cannot_reach_a_workbook_change_while_paused() {
    let session = paused_session("guard-dispatch");
    for id in [
        "edit.undo",
        "edit.paste",
        "table.sort_asc",
        "table.delete_row",
        "format.bold",
        "table.fill_down",
        "sheets.add_sheet",
        "sheets.insert_shape",
    ] {
        let before = fingerprint(&session.state);
        session.app.set_status_left("untouched".into());
        assert!(
            crate::command_dispatch::dispatch_command(&session.app, id),
            "{id}"
        );
        assert_eq!(fingerprint(&session.state), before, "{id}");
        assert!(
            session
                .app
                .get_status_left()
                .starts_with("Edit not applied"),
            "{id}: {}",
            session.app.get_status_left()
        );
    }
}

#[test]
fn read_only_actions_stay_available_while_paused() {
    let session = paused_session("guard-read-only");
    let before = fingerprint(&session.state);

    session.app.invoke_copy_selection();
    assert!(session.app.get_status_left().starts_with("Copied"));
    session.app.invoke_select_all();
    assert!(session.app.get_status_left().starts_with("Selected all"));
    session.app.invoke_zoom_in();
    assert!(session.app.get_status_left().starts_with("Zoom set"));
    // Retry is how the user recovers, so it is never refused.
    session.app.invoke_retry_recovery();
    assert_eq!(session.app.get_status_left().as_str(), "Retrying recovery…");
    assert_ne!(
        fingerprint(&session.state),
        before,
        "retry queues a revision"
    );
}

#[test]
fn pausing_helper_is_still_usable_by_the_cell_path() {
    // `pause_recovery` is the shared way to pause through a failed append.
    let session = Session::new("guard-helper");
    pause_recovery(&session);
    assert!(session.app.get_recovery_paused());
}

#[test]
fn a_new_mutating_callback_cannot_skip_the_guard_or_the_classification() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut registered: BTreeSet<String> = BTreeSet::new();
    let mut unguarded = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        if !file.ends_with(".rs") || file.contains("test") || file == "mutation_guard.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        let mut starts = Vec::new();
        let mut from = 0;
        while let Some(i) = text[from..].find(".on_") {
            let at = from + i;
            from = at + 4;
            let name: String = text[from..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            if !name.is_empty() && text[from + name.len()..].starts_with('(') {
                starts.push((at, name));
            }
        }
        for (i, (at, name)) in starts.iter().enumerate() {
            registered.insert(name.clone());
            let end = starts.get(i + 1).map_or(text.len(), |next| next.0);
            if MUTATING.contains(&name.as_str())
                && !text[*at..end].contains("mutation_guard::refused")
            {
                unguarded.push(format!("{name} in {file}"));
            }
        }
    }
    assert!(
        unguarded.is_empty(),
        "mutating callbacks without the guard: {unguarded:?}"
    );

    let slint = std::fs::read_to_string(src.join("../ui/app.slint")).unwrap();
    for line in slint.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("callback ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            registered.insert(name.replace('-', "_"));
        }
    }
    let known: BTreeSet<String> = MUTATING
        .iter()
        .chain(ALLOWED.iter())
        .map(|name| name.to_string())
        .collect();
    let unclassified: Vec<_> = registered.difference(&known).collect();
    assert!(
        unclassified.is_empty(),
        "classify each new callback in mutation_guard::MUTATING or ALLOWED: {unclassified:?}"
    );
    for name in MUTATING {
        assert!(
            registered.contains(*name),
            "{name} is listed but never registered"
        );
    }
}
