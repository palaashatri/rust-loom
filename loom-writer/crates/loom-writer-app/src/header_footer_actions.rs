use std::rc::Rc;

use loom_writer_core::{HeaderFooterConfig, PageNumberFormat};
use slint::{ComponentHandle, SharedString};

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp, WriterHeaderFooterState};

pub(crate) fn register(app: &WriterApp, state: &Rc<GuiState>) {
    let global = app.global::<WriterHeaderFooterState>();
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_open_editor(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let config = state.current.borrow().header_footer.clone();
            let global = app.global::<WriterHeaderFooterState>();
            let alignment = alignment_index(config.effective_alignment());
            global.set_header_text(config.header_text.into());
            global.set_footer_text(config.footer_text.into());
            global.set_alignment(alignment);
            global.set_page_number_format(number_format_index(config.page_number_format));
            global.set_different_first_page(config.different_first_page);
            global.set_error(SharedString::new());
            global.set_dialog_open(true);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_apply(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let global = app.global::<WriterHeaderFooterState>();
            let config = match dialog_config(&global) {
                Ok(config) => config,
                Err(error) => {
                    global.set_error(error.into());
                    return;
                }
            };
            let mut next = state.current.borrow().clone();
            if let Err(error) = config.validate_for_page_style(&next.page.page_style()) {
                global.set_error(error.into());
                return;
            }
            next.header_footer = config;
            if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
                global.set_error(app.get_status_left());
                return;
            }
            global.set_error(SharedString::new());
            global.set_dialog_open(false);
            app.set_status_right("Headers and footers updated".into());
        });
    }
    {
        let app_ref = app.as_weak();
        global.on_cancel(move || {
            if let Some(app) = app_ref.upgrade() {
                let global = app.global::<WriterHeaderFooterState>();
                global.set_error(SharedString::new());
                global.set_dialog_open(false);
            }
        });
    }
}

fn dialog_config(global: &WriterHeaderFooterState) -> Result<HeaderFooterConfig, String> {
    let alignment = match global.get_alignment() {
        0 => "",
        1 => "center",
        2 => "right",
        _ => return Err("Choose left, center, or right alignment".into()),
    };
    let page_number_format = match global.get_page_number_format() {
        0 => PageNumberFormat::Arabic,
        1 => PageNumberFormat::RomanUpper,
        2 => PageNumberFormat::RomanLower,
        3 => PageNumberFormat::Alphabetical,
        _ => return Err("Choose a page number format".into()),
    };
    let config = HeaderFooterConfig {
        header_text: global.get_header_text().to_string(),
        footer_text: global.get_footer_text().to_string(),
        alignment: alignment.into(),
        page_number_format,
        different_first_page: global.get_different_first_page(),
    };
    config.validate().map_err(str::to_owned)?;
    Ok(config)
}

fn alignment_index(alignment: &str) -> i32 {
    match alignment {
        "center" => 1,
        "right" => 2,
        _ => 0,
    }
}

fn number_format_index(format: PageNumberFormat) -> i32 {
    match format {
        PageNumberFormat::Arabic => 0,
        PageNumberFormat::RomanUpper => 1,
        PageNumberFormat::RomanLower => 2,
        PageNumberFormat::Alphabetical => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use loom_desktop::ScriptedFileDialogs;
    use slint::ComponentHandle;

    use crate::{
        actions_tests::test_state, header_footer_actions, WriterDocument, WriterHeaderFooterState,
    };
    use loom_writer_core::PageNumberFormat;

    fn launched() -> (crate::WriterApp, Rc<crate::GuiState>) {
        test_state(
            WriterDocument::new("header-footer", "Headers"),
            Rc::new(ScriptedFileDialogs::new([], [])),
        )
    }

    #[test]
    fn cancel_discards_header_footer_draft_without_history() {
        let (app, state) = launched();
        header_footer_actions::register(&app, &state);
        let global = app.global::<WriterHeaderFooterState>();
        global.invoke_open_editor();
        global.set_header_text("Draft {page}".into());
        global.set_footer_text("Draft {total}".into());
        global.invoke_cancel();

        assert!(state.current.borrow().header_footer.header_text.is_empty());
        assert!(state.current.borrow().header_footer.footer_text.is_empty());
        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(!global.get_dialog_open());
    }

    #[test]
    fn applying_unchanged_defaults_does_not_dirty_the_document() {
        let (app, state) = launched();
        header_footer_actions::register(&app, &state);
        let global = app.global::<WriterHeaderFooterState>();
        global.invoke_open_editor();
        global.invoke_apply();

        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(!global.get_dialog_open());
        assert!(state.current.borrow().header_footer.header_text.is_empty());
    }

    #[test]
    fn apply_and_undo_restore_document_owned_header_footer_settings() {
        let (app, state) = launched();
        header_footer_actions::register(&app, &state);
        let global = app.global::<WriterHeaderFooterState>();
        global.invoke_open_editor();
        global.set_header_text("Page {page} of {total}".into());
        global.set_footer_text("Loom".into());
        global.set_alignment(1);
        global.set_page_number_format(1);
        global.set_different_first_page(true);
        global.invoke_apply();

        assert_eq!(
            state.current.borrow().header_footer.header_text,
            "Page {page} of {total}"
        );
        assert_eq!(state.current.borrow().header_footer.alignment, "center");
        assert_eq!(
            state.current.borrow().header_footer.page_number_format,
            PageNumberFormat::RomanUpper
        );
        assert!(state.current.borrow().header_footer.different_first_page);
        assert_eq!(state.history.borrow().undo_len(), 1);
        let previous = state
            .history
            .borrow_mut()
            .undo()
            .expect("undo applied header/footer");
        assert!(previous.header_footer.header_text.is_empty());
        assert!(previous.header_footer.footer_text.is_empty());
    }

    #[test]
    fn apply_refuses_header_when_top_margin_cannot_separate_it_from_body() {
        let (app, state) = launched();
        state
            .current
            .borrow_mut()
            .page
            .set_custom_geometry_points(612.0, 792.0, 23.9, 72.0, 72.0, 72.0)
            .expect("valid custom page geometry");
        header_footer_actions::register(&app, &state);
        let global = app.global::<WriterHeaderFooterState>();
        global.invoke_open_editor();
        global.set_header_text("Header".into());
        global.invoke_apply();

        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(global.get_dialog_open());
        assert!(global.get_error().contains("24 pt"));
    }

    #[test]
    fn custom_geometry_cannot_reduce_a_configured_header_margin() {
        let (app, state) = launched();
        state.current.borrow_mut().header_footer.header_text = "Header".into();
        let original_margin = state.current.borrow().page.page_style().margin_top_pt;
        crate::page_setup_actions::register(&app, &state);

        assert!(!app.invoke_edit_page_geometry("top".into(), "6.0".into()));
        assert_eq!(state.current.borrow().header_footer.header_text, "Header");
        assert_eq!(
            state.current.borrow().page.page_style().margin_top_pt,
            original_margin
        );
        assert_eq!(state.history.borrow().undo_len(), 0);
    }

    #[test]
    fn apply_rejects_header_line_breaks_without_mutating_the_document() {
        let (app, state) = launched();
        header_footer_actions::register(&app, &state);
        let global = app.global::<WriterHeaderFooterState>();
        global.invoke_open_editor();
        global.set_header_text("first\nsecond".into());
        global.invoke_apply();

        assert!(state.current.borrow().header_footer.header_text.is_empty());
        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(global.get_dialog_open());
        assert!(global.get_error().contains("single line"));
    }
}
