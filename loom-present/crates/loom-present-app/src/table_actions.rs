use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use loom_present_core::{default_slide_table, PresentationSession, SlideElement, TableDirection};
use slint::{ComponentHandle, SharedString};

use crate::{refresh, set_status, startup_recovery, GuiState, PresentApp};

#[derive(Debug, Clone)]
pub struct PendingPptxImport {
    pub path: PathBuf,
    pub session: PresentationSession,
    pub warnings: Vec<String>,
    pub kept: Option<crate::startup_recovery::KeptDraft>,
    /// A failed Continue may have partially published the candidate. Cancel must
    /// checkpoint the still-visible deck before releasing the warning.
    pub preserved_path: Option<PathBuf>,
    pub cancel_must_checkpoint_visible_session: bool,
}

thread_local! {
    static ACTIVE_TABLE_CELL: RefCell<Option<(String, usize, usize)>> = const { RefCell::new(None) };
    static TABLE_EDITING_CELL: RefCell<bool> = const { RefCell::new(false) };
    pub static PENDING_PPTX_IMPORT: RefCell<Option<PendingPptxImport>> = const { RefCell::new(None) };
}

fn report_import_warning_error(app: &PresentApp, pending: &PendingPptxImport, status: String) {
    app.set_pptx_import_warning_message(
        crate::pptx_import::warning_message_with_error(&pending.warnings, &status).into(),
    );
    set_status(app, status);
}

pub fn is_table_editing_cell() -> bool {
    TABLE_EDITING_CELL.with(|c| *c.borrow())
}

pub fn set_table_editing_cell(editing: bool) {
    TABLE_EDITING_CELL.with(|c| *c.borrow_mut() = editing);
}

pub fn get_active_table_cell(element_id: &str) -> (usize, usize) {
    ACTIVE_TABLE_CELL.with(|cell| {
        if let Some((ref id, r, c)) = *cell.borrow() {
            if id == element_id {
                return (r, c);
            }
        }
        (0, 0)
    })
}

/// A press on a table object makes the cell under the pointer the active one,
/// so a double-click edits the cell that was clicked.
pub(crate) fn select_table_cell_under_press(app: &PresentApp, state: &GuiState, index: usize) {
    let parts = app.global::<crate::ObjectParts>();
    let (fx, fy) = (parts.get_press_fx(), parts.get_press_fy());
    let session = state.session.borrow();
    let Some(element) = session
        .document
        .active_slide()
        .and_then(|slide| slide.elements.get(index))
    else {
        return;
    };
    if element.element_type != loom_present_core::ElementType::Table {
        return;
    }
    if let Some((row, col)) =
        element.table_cell_at_local_coords(fx * element.width, fy * element.height)
    {
        set_active_table_cell(&element.id, row, col);
    }
}

/// The chart the clipboard holds, or one made from a copied range of numbers.
fn chart_from_clipboard() -> Option<loom_objects::Chart> {
    let clip = loom_clipboard::read_clipboard().ok()?;
    if let Some(chart) = clip.to_chart() {
        return Some(chart);
    }
    let table = clip.to_table().or_else(|| {
        let text = clip.tsv.as_deref().or(clip.plain_text.as_deref())?;
        (text.lines().filter(|l| l.contains('\t')).count() >= 2)
            .then(|| loom_clipboard::tsv_to_table(text))
    })?;
    loom_objects::Chart::from_table("clipboard-chart", &table)
}

pub fn set_active_table_cell(element_id: &str, row: usize, col: usize) {
    ACTIVE_TABLE_CELL.with(|cell| {
        *cell.borrow_mut() = Some((element_id.to_string(), row, col));
    });
}

pub fn format_table_a11y(element: &SlideElement) -> Option<String> {
    let table = element.table()?;
    let (r, c) = get_active_table_cell(&element.id);
    let col_letter = (b'A' + (c as u8).min(25)) as char;
    let cell_content = table
        .cell(r, c)
        .map(|cell| cell.content.text())
        .unwrap_or_default();
    Some(format!(
        "Table {} by {}, cell {}{}: {}",
        table.row_count(),
        table.col_count(),
        col_letter,
        r + 1,
        cell_content
    ))
}

pub fn handle_table_key(app: &PresentApp, state: &GuiState, key: &str, shift: bool) -> bool {
    let selected_ids = state.session.borrow().selected_elements.clone();
    if selected_ids.is_empty() {
        return false;
    }

    let tab_key: SharedString = slint::platform::Key::Tab.into();
    let backtab_key: SharedString = slint::platform::Key::Backtab.into();
    let left_arrow: SharedString = slint::platform::Key::LeftArrow.into();
    let right_arrow: SharedString = slint::platform::Key::RightArrow.into();
    let up_arrow: SharedString = slint::platform::Key::UpArrow.into();
    let down_arrow: SharedString = slint::platform::Key::DownArrow.into();
    let enter_key: SharedString = slint::platform::Key::Return.into();
    let esc_key: SharedString = slint::platform::Key::Escape.into();

    let is_tab = key == tab_key.as_str() || key == backtab_key.as_str();
    let is_arrow = key == left_arrow.as_str()
        || key == right_arrow.as_str()
        || key == up_arrow.as_str()
        || key == down_arrow.as_str();
    let is_enter = key == enter_key.as_str();
    let is_esc = key == esc_key.as_str();

    if !is_tab && !is_arrow && !is_enter && !is_esc {
        return false;
    }

    let mut session = state.session.borrow_mut();
    let Some(slide) = session.document.active_slide_mut() else {
        return false;
    };
    let Some(elem) = slide
        .elements
        .iter_mut()
        .find(|e| selected_ids.iter().any(|id| id == &e.id))
    else {
        return false;
    };
    if elem.element_type != loom_present_core::ElementType::Table {
        return false;
    }

    if is_enter {
        set_table_editing_cell(true);
        drop(session);
        refresh(app, state);
        return true;
    }

    if is_esc {
        if is_table_editing_cell() {
            set_table_editing_cell(false);
            drop(session);
            refresh(app, state);
            return true;
        } else {
            session.selected_elements.clear();
            drop(session);
            refresh(app, state);
            return true;
        }
    }

    let (curr_r, curr_c) = get_active_table_cell(&elem.id);

    if is_tab {
        let is_shift = shift || key == backtab_key.as_str();
        let (new_r, new_c) = elem.tab_navigate_table(curr_r, curr_c, is_shift);
        set_active_table_cell(&elem.id, new_r, new_c);
        drop(session);
        refresh(app, state);
        return true;
    }

    let dir = if key == left_arrow.as_str() {
        TableDirection::Left
    } else if key == right_arrow.as_str() {
        TableDirection::Right
    } else if key == up_arrow.as_str() {
        TableDirection::Up
    } else {
        TableDirection::Down
    };

    let (new_r, new_c) = elem.arrow_navigate_table(curr_r, curr_c, dir);
    set_active_table_cell(&elem.id, new_r, new_c);
    drop(session);
    refresh(app, state);
    true
}

pub fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_add_table(move || {
            if let Some(app) = app_ref.upgrade() {
                let res = state
                    .session
                    .borrow_mut()
                    .insert_table(default_slide_table(3, 3));
                match res {
                    Ok(_) => {
                        refresh(&app, &state);
                        set_status(&app, "Inserted table");
                    }
                    Err(err) => set_status(&app, format!("Failed to insert table: {err}")),
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_insert_table_size(move |rows, cols| {
            if let Some(app) = app_ref.upgrade() {
                let r = (rows as usize).clamp(1, 50);
                let c = (cols as usize).clamp(1, 20);
                let res = state
                    .session
                    .borrow_mut()
                    .insert_table(default_slide_table(r, c));
                match res {
                    Ok(_) => {
                        refresh(&app, &state);
                        set_status(&app, format!("Inserted {r}x{c} table"));
                    }
                    Err(err) => set_status(&app, format!("Failed to insert table: {err}")),
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_add_chart(move || {
            if let Some(app) = app_ref.upgrade() {
                // The chart comes from what the user copied (a chart, or a range
                // of numbers with a header row and a label column); nothing is
                // invented.
                let Some(chart) = chart_from_clipboard() else {
                    set_status(
                        &app,
                        "Copy a chart, or a range of numbers with a header row and a label column, from Sheets first",
                    );
                    return;
                };
                let res = state.session.borrow_mut().insert_chart(chart);
                match res {
                    Ok(_) => {
                        refresh(&app, &state);
                        set_status(&app, "Inserted chart");
                    }
                    Err(err) => set_status(&app, format!("Failed to insert chart: {err}")),
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_set_table_style(move |style_id| {
            if let Some(app) = app_ref.upgrade() {
                let selected_ids = state.session.borrow().selected_elements.clone();
                {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(slide) = session.document.active_slide_mut() {
                        if let Some(elem) = slide
                            .elements
                            .iter_mut()
                            .find(|e| selected_ids.iter().any(|id| id == &e.id))
                        {
                            if let Some(mut table) = elem.table() {
                                table.style_id = style_id.as_str().to_string();
                                elem.set_table(&table);
                            }
                        }
                    }
                }
                refresh(&app, &state);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_toggle_table_option(move |opt| {
            if let Some(app) = app_ref.upgrade() {
                let selected_ids = state.session.borrow().selected_elements.clone();
                {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(slide) = session.document.active_slide_mut() {
                        if let Some(elem) = slide
                            .elements
                            .iter_mut()
                            .find(|e| selected_ids.iter().any(|id| id == &e.id))
                        {
                            if let Some(mut table) = elem.table() {
                                match opt.as_str() {
                                    "header-row" | "header_row" => {
                                        table.header_row = !table.header_row
                                    }
                                    "banded-rows" | "banded_rows" => {
                                        table.banded_rows = !table.banded_rows
                                    }
                                    "first-column" | "first_column" => {
                                        table.first_column = !table.first_column
                                    }
                                    _ => {}
                                }
                                elem.set_table(&table);
                            }
                        }
                    }
                }
                refresh(&app, &state);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_table_action(move |action| {
            if let Some(app) = app_ref.upgrade() {
                let selected_ids = state.session.borrow().selected_elements.clone();
                {
                    let mut session = state.session.borrow_mut();
                    session.checkpoint();
                    if let Some(slide) = session.document.active_slide_mut() {
                        if let Some(elem) = slide
                            .elements
                            .iter_mut()
                            .find(|e| selected_ids.iter().any(|id| id == &e.id))
                        {
                            let (curr_r, curr_c) = get_active_table_cell(&elem.id);
                            match action.as_str() {
                                "insert-row-above" | "add_row_above" => {
                                    elem.insert_table_row(curr_r, false);
                                }
                                "insert-row-below" | "add_row_below" => {
                                    elem.insert_table_row(curr_r, true);
                                    set_active_table_cell(&elem.id, curr_r + 1, curr_c);
                                }
                                "delete-row" | "del_row" => {
                                    elem.delete_table_row(curr_r);
                                    set_active_table_cell(
                                        &elem.id,
                                        curr_r.saturating_sub(1),
                                        curr_c,
                                    );
                                }
                                "insert-col-left" | "add_col_left" => {
                                    elem.insert_table_column(curr_c, false);
                                }
                                "insert-col-right" | "add_col_right" => {
                                    elem.insert_table_column(curr_c, true);
                                    set_active_table_cell(&elem.id, curr_r, curr_c + 1);
                                }
                                "delete-col" | "del_col" => {
                                    elem.delete_table_column(curr_c);
                                    set_active_table_cell(
                                        &elem.id,
                                        curr_r,
                                        curr_c.saturating_sub(1),
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                }
                refresh(&app, &state);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_pptx_import_continue(move || {
            if let Some(app) = app_ref.upgrade() {
                let Some(mut pending) = PENDING_PPTX_IMPORT.with(|slot| slot.borrow_mut().take())
                else {
                    app.set_pptx_import_warning_open(false);
                    crate::recovery_deferred::resume_after_import_warning(&app);
                    return;
                };
                match crate::accept_pending_pptx_import(&app, &state, &mut pending) {
                    Ok(status) => {
                        app.set_pptx_import_warning_open(false);
                        set_status(&app, status);
                        crate::recovery_deferred::resume_after_import_warning(&app);
                    }
                    Err(error) => {
                        let status = format!("Import not completed: {error}");
                        report_import_warning_error(&app, &pending, status);
                        PENDING_PPTX_IMPORT.with(|slot| *slot.borrow_mut() = Some(pending));
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_pptx_import_cancel(move || {
            if let Some(app) = app_ref.upgrade() {
                let Some(mut pending) = PENDING_PPTX_IMPORT.with(|slot| slot.borrow_mut().take())
                else {
                    app.set_pptx_import_warning_open(false);
                    set_status(&app, "Open cancelled");
                    crate::recovery_deferred::resume_after_import_warning(&app);
                    return;
                };
                let preserved = if let Some(kept) = pending.kept.as_ref() {
                    startup_recovery::kept_drafts_directory().and_then(|directory| {
                        startup_recovery::ensure_kept_draft_is_preserved(
                            &directory,
                            kept,
                            &mut pending.preserved_path,
                        )
                        .map(Some)
                    })
                } else {
                    Ok(None)
                };
                match preserved {
                    Ok(path) => {
                        let recovery = if pending.cancel_must_checkpoint_visible_session {
                            let payload = {
                                let session = state.session.borrow();
                                let source = state.save_path.borrow();
                                crate::recovery_draft::payload(&session, source.as_deref())
                            };
                            payload.and_then(crate::checkpoint_required_recovery)
                        } else {
                            crate::ensure_recovery_service()
                        };
                        match recovery {
                            Ok(()) => {
                                if pending.cancel_must_checkpoint_visible_session {
                                    crate::recovery_deferred::invalidate();
                                }
                                app.set_pptx_import_warning_open(false);
                                set_status(
                                    &app,
                                    path.map_or_else(
                                        || "Open cancelled".to_string(),
                                        |path| {
                                            format!(
                                                "Open cancelled. Recovery draft kept at {}",
                                                path.display()
                                            )
                                        },
                                    ),
                                );
                                if !pending.cancel_must_checkpoint_visible_session {
                                    crate::recovery_deferred::resume_after_import_warning(&app);
                                }
                            }
                            Err(error) => {
                                let status = format!(
                                    "Cancel not completed: current presentation recovery could not be secured: {error}"
                                );
                                report_import_warning_error(&app, &pending, status);
                                PENDING_PPTX_IMPORT
                                    .with(|slot| *slot.borrow_mut() = Some(pending));
                            }
                        }
                    }
                    Err(error) => {
                        let status = format!("Cancel not completed: {error}");
                        report_import_warning_error(&app, &pending, status);
                        PENDING_PPTX_IMPORT.with(|slot| *slot.borrow_mut() = Some(pending));
                    }
                }
            }
        });
    }
}
