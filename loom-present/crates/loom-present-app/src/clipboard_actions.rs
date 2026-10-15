//! Multi-format clipboard actions for Present (W2 interop).

use loom_clipboard::{read_clipboard, tsv_to_table, write_clipboard, ClipboardData};
use loom_present_core::ElementType;
use slint::ComponentHandle;
use std::rc::Rc;

use crate::{GuiState, PresentApp};

pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_copy(move || {
            if let Some(app) = app_ref.upgrade() {
                copy_action(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cut(move || {
            if let Some(app) = app_ref.upgrade() {
                cut_action(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_paste(move || {
            if let Some(app) = app_ref.upgrade() {
                paste_action(&app, &state);
            }
        });
    }
}

pub(crate) fn copy_action(app: &PresentApp, state: &GuiState) {
    let session = state.session.borrow();
    let Some(slide) = session.document.active_slide() else {
        return;
    };

    if let Some(id) = session.selected_elements.first() {
        if let Some(elem) = slide.elements.iter().find(|e| &e.id == id) {
            match elem.element_type {
                ElementType::Table => {
                    if let Some(table) = elem.table() {
                        let clip = ClipboardData::from_table(&table);
                        if write_clipboard(&clip).is_ok() {
                            crate::set_status(
                                app,
                                format!(
                                    "Copied table ({}x{})",
                                    table.row_count(),
                                    table.col_count()
                                ),
                            );
                        }
                    }
                }
                ElementType::Chart => {
                    if let Some(chart) = elem.chart() {
                        let clip = ClipboardData::from_chart(&chart);
                        if write_clipboard(&clip).is_ok() {
                            crate::set_status(app, "Copied chart");
                        }
                    }
                }
                _ => {
                    if !elem.content.is_empty() {
                        let clip = ClipboardData {
                            plain_text: Some(elem.content.clone()),
                            ..Default::default()
                        };
                        if write_clipboard(&clip).is_ok() {
                            crate::set_status(app, "Copied text");
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn cut_action(app: &PresentApp, state: &GuiState) {
    copy_action(app, state);
    let mut session = state.session.borrow_mut();
    if let Some(id) = session.selected_elements.first().cloned() {
        session.checkpoint();
        if let Some(slide) = session.document.active_slide_mut() {
            slide.elements.retain(|e| e.id != id);
        }
        session.selected_elements.clear();
        state.selected_element.set(0);
        drop(session);
        crate::refresh(app, state);
        crate::set_status(app, "Cut element");
    }
}

pub(crate) fn paste_action(app: &PresentApp, state: &GuiState) {
    let Ok(clip) = read_clipboard() else {
        crate::set_status(app, "Clipboard unavailable");
        return;
    };

    // 1. Chart payload
    if let Some(chart) = clip.to_chart() {
        let res = state.session.borrow_mut().insert_chart(chart);
        match res {
            Ok(_) => {
                crate::refresh(app, state);
                crate::set_status(app, "Pasted chart");
                return;
            }
            Err(e) => {
                crate::set_status(app, format!("Paste chart failed: {e}"));
                return;
            }
        }
    }

    // 2. Table payload
    if let Some(table) = clip.to_table() {
        let res = state.session.borrow_mut().insert_table(table);
        match res {
            Ok(_) => {
                crate::refresh(app, state);
                let status = match clip.truncation_note() {
                    Some(note) => format!("Pasted table - {note}"),
                    None => "Pasted table".to_string(),
                };
                crate::set_status(app, status);
                return;
            }
            Err(e) => {
                crate::set_status(app, format!("Paste table failed: {e}"));
                return;
            }
        }
    }

    // 3. TSV or plain text with >= 2 tab rows
    if let Some(text) = clip.tsv.or(clip.plain_text) {
        let tab_rows = text.lines().filter(|l| l.contains('\t')).count();
        if tab_rows >= 2 {
            let table = tsv_to_table(&text);
            let res = state.session.borrow_mut().insert_table(table);
            match res {
                Ok(_) => {
                    crate::refresh(app, state);
                    crate::set_status(app, "Pasted table");
                }
                Err(e) => {
                    crate::set_status(app, format!("Paste table failed: {e}"));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{empty_session, set_platform, wire_app_callbacks, DragState, FileFilter};
    use loom_clipboard::set_mock_clipboard_enabled;
    use loom_desktop::ScriptedFileDialogs;
    use loom_objects::chart::{Chart, ChartSeries, ChartType};
    use loom_objects::Table;
    use std::cell::{Cell, RefCell};

    fn make_test_app() -> (PresentApp, Rc<GuiState>) {
        set_platform();
        let app = PresentApp::new().expect("create app");
        let session = empty_session();
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
        wire(&app, &state);
        (app, state)
    }

    #[test]
    fn test_present_paste_sheets_range_inserts_table_element() {
        set_mock_clipboard_enabled(true);
        let (app, state) = make_test_app();

        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Col1");
        table.set_cell_text(0, 1, "Col2");
        table.set_cell_text(1, 0, "Val1");
        table.set_cell_text(1, 1, "Val2");

        let clip = ClipboardData::from_table(&table);
        write_clipboard(&clip).expect("write mock");

        app.invoke_paste();

        let session = state.session.borrow();
        let slide = session.document.active_slide().expect("active slide");
        let table_elem = slide
            .elements
            .iter()
            .find(|e| e.element_type == ElementType::Table)
            .expect("must contain Table element");
        let restored_tbl = table_elem.table().expect("table must parse");
        assert_eq!(restored_tbl.row_count(), 2);
        assert_eq!(restored_tbl.cell_text(0, 0).as_deref(), Some("Col1"));

        set_mock_clipboard_enabled(false);
    }

    #[test]
    fn test_present_paste_chart_inserts_chart_element() {
        set_mock_clipboard_enabled(true);
        let (app, state) = make_test_app();

        let chart = Chart::from_data(
            "chart-revenue",
            Some("Revenue Growth".into()),
            ChartType::Bar,
            vec!["2024".into(), "2025".into()],
            vec![ChartSeries {
                name: "Revenue".into(),
                values: vec![100.0, 150.0],
                color: None,
            }],
        );

        let clip = ClipboardData::from_chart(&chart);
        write_clipboard(&clip).expect("write mock");

        app.invoke_paste();

        let session = state.session.borrow();
        let slide = session.document.active_slide().expect("active slide");
        let chart_elem = slide
            .elements
            .iter()
            .find(|e| e.element_type == ElementType::Chart)
            .expect("must contain Chart element");
        let restored_chart = chart_elem.chart().expect("chart must parse");
        assert_eq!(restored_chart.title.as_deref(), Some("Revenue Growth"));
        assert_eq!(restored_chart.series.len(), 1);

        set_mock_clipboard_enabled(false);
    }

    #[test]
    fn test_present_copy_table_writes_multiformat_for_sheets() {
        set_mock_clipboard_enabled(true);
        let (app, state) = make_test_app();

        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "PresentA");
        table.set_cell_text(1, 1, "PresentB");

        let elem_id = state
            .session
            .borrow_mut()
            .insert_table(table)
            .expect("inserted");
        state.session.borrow_mut().selected_elements = vec![elem_id];

        app.invoke_copy();

        let clip = read_clipboard().expect("read clipboard");
        assert!(
            clip.loom_objects_json.is_some(),
            "must write private format"
        );
        assert!(clip.html.is_some(), "must write HTML table");
        assert!(clip.tsv.is_some(), "must write TSV");

        let sheets_tbl = clip.to_table().expect("parseable by sheets");
        assert_eq!(sheets_tbl.cell_text(0, 0).as_deref(), Some("PresentA"));
        assert_eq!(sheets_tbl.cell_text(1, 1).as_deref(), Some("PresentB"));

        set_mock_clipboard_enabled(false);
    }
}
