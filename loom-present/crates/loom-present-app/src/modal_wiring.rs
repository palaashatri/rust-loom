//! Wires Welcome Guide, Start Hub, Print Preview, Find & Replace, Font Picker,
//! and extended shape/selection callbacks to `GuiState`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use loom_present_core::find_replace::TextMatch;
use loom_present_core::print_layout::{
    export_pdf_with_options, preview_print_pages, PaperSize, PrintLayout, PrintOptions,
    PrintOrientation,
};
use loom_present_core::ElementType;
use slint::ComponentHandle;

use crate::{
    added_object_origin, element_style_ui, export_request, find_replace_ui, font_picker,
    print_preview, refresh, set_status, start_hub, text_element, welcome_guide, FindReplaceState,
    GuiState, PresentApp, PrintPreviewState, StartHubState,
};

fn build_print_options(app: &PresentApp) -> PrintOptions {
    let preview = app.global::<PrintPreviewState>();
    let layout = match preview.get_layout_index() {
        1 => PrintLayout::Handout2,
        2 => PrintLayout::Handout4,
        3 => PrintLayout::Handout6,
        4 => PrintLayout::NotesPage,
        _ => PrintLayout::SingleSlide,
    };
    let paper_size = match preview.get_paper_size_index() {
        1 => PaperSize::A4,
        2 => PaperSize::Legal,
        _ => PaperSize::Letter,
    };
    let orientation = if preview.get_landscape() {
        PrintOrientation::Landscape
    } else {
        PrintOrientation::Portrait
    };
    PrintOptions {
        layout,
        paper_size,
        orientation,
        include_slide_numbers: preview.get_show_slide_numbers(),
    }
}

pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_shape_kind(move |kind| {
            let Some(app) = app_ref.upgrade() else { return };
            let (elem_type, label) = match kind.as_str() {
                "circle" => (ElementType::ShapeCircle, "Circle"),
                "rounded-rect" => (ElementType::ShapeRoundedRect, "Rounded"),
                "arrow" => (ElementType::ShapeArrowRight, "Arrow"),
                "line" => (ElementType::ShapeLine, ""),
                "callout" => (ElementType::ShapeCallout, "Callout"),
                _ => (ElementType::ShapeRectangle, "Shape"),
            };
            let mut session = state.session.borrow_mut();
            let count = session
                .document
                .active_slide()
                .map(|s| s.elements.len() + 1)
                .unwrap_or(1);
            let (x, y) = added_object_origin(&session, (120.0, 260.0), (260.0, 120.0));
            session.add_element(text_element(
                &format!("shape-{count}"),
                elem_type,
                label,
                x,
                y,
                260.0,
                120.0,
            ));
            if let Some(id) = session
                .document
                .active_slide()
                .and_then(|s| s.elements.last())
                .map(|e| e.id.clone())
            {
                session.select_element(&id, false);
            }
            state.selected_element.set(count - 1);
            drop(session);
            refresh(&app, &state);
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_duplicate_selection(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let duplicated = state.session.borrow_mut().duplicate_selected();
            if !duplicated.is_empty() {
                refresh(&app, &state);
                set_status(&app, "Duplicated selection");
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_all_elements(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let mut session = state.session.borrow_mut();
            let ids: Vec<String> = session
                .document
                .active_slide()
                .map(|s| s.elements.iter().map(|e| e.id.clone()).collect())
                .unwrap_or_default();
            if !ids.is_empty() {
                session.selected_elements = ids;
                drop(session);
                refresh(&app, &state);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_print_preview(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let session = state.session.borrow();
            let count = session.document.slides.len();
            let (title, notes) = session
                .document
                .active_slide()
                .map(|s| (s.title.clone(), s.speaker_notes.clone()))
                .unwrap_or_default();
            drop(session);
            print_preview::open(&app, count, &title, &notes);
        });
    }

    // Welcome Guide
    let wg_state = state.clone();
    welcome_guide::wire(
        app,
        Rc::new(move |app, action| {
            match action {
                "templates" => {
                    app.set_theme_selected(0);
                    app.set_theme_category(0);
                    app.set_theme_chooser_open(true);
                }
                "sample-deck" => app.invoke_new_sample_deck(),
                "format-inspector" => app.invoke_show_inspector_tab(0),
                "insert-table" => app.invoke_add_table(),
                "add-shape" => app.invoke_add_shape(),
                "presenter-view" => app.invoke_open_presenter(),
                "palette" => app.invoke_open_palette(),
                _ => app.invoke_focus_editor(),
            }
            let _ = &wg_state;
        }),
    );

    // Start Hub
    let sh_state = state.clone();
    start_hub::wire(
        app,
        start_hub::StartHubCallbacks {
            on_open_file: Rc::new(|app| {
                app.invoke_open_deck();
            }),
            on_new_presentation: Rc::new(|app| {
                app.invoke_create_theme(0);
            }),
            on_open_template: Rc::new(|app, id| {
                let idx = loom_present_core::templates::template_catalog()
                    .iter()
                    .position(|t| t.id == id)
                    .unwrap_or(0) as i32;
                app.invoke_create_theme(idx);
            }),
            on_open_deck: Rc::new(move |app, path| {
                if app.get_deck_dirty() {
                    set_status(app, "Save or discard current changes before opening");
                    app.global::<StartHubState>().set_open(false);
                    app.invoke_open_deck();
                    return;
                }
                let p = PathBuf::from(path);
                if p.exists() {
                    if let Ok(loaded) = crate::load_session(&p) {
                        *sh_state.session.borrow_mut() = loaded;
                        *sh_state.save_path.borrow_mut() = Some(p.clone());
                        start_hub::record_recent(&p);
                        app.global::<StartHubState>().set_open(false);
                        refresh(app, &sh_state);
                    }
                }
            }),
        },
    );

    // Print Preview
    let pp_export_state = state.clone();
    let pp_refresh_state = state.clone();
    print_preview::wire(
        app,
        Rc::new(move |app| {
            let options = build_print_options(app);
            match pp_export_state
                .dialogs
                .save_file(&export_request(&pp_export_state))
            {
                Ok(Some(path)) => {
                    let pdf_bytes = export_pdf_with_options(
                        &pp_export_state.session.borrow().document,
                        &options,
                    );
                    match loom_storage::atomic_write(&path, &pdf_bytes) {
                        Ok(()) => set_status(app, format!("Exported {}", path.display())),
                        Err(err) => set_status(app, format!("Export failed: {err}")),
                    }
                }
                Ok(None) => set_status(app, "Export cancelled"),
                Err(err) => set_status(app, format!("Export dialog failed: {err}")),
            }
        }),
        Rc::new(move |app| {
            let options = build_print_options(app);
            let session = pp_refresh_state.session.borrow();
            let pages = preview_print_pages(&session.document, &options);
            let total = pages.len().max(1) as i32;
            let preview = app.global::<PrintPreviewState>();
            preview.set_total_pages(total);
            if preview.get_current_page() > total {
                preview.set_current_page(total);
            }
        }),
    );

    // Find & Replace
    let matches_cache: Rc<RefCell<Vec<TextMatch>>> = Rc::new(RefCell::new(Vec::new()));
    let match_cursor: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    let fr_query_state = state.clone();
    let fr_query_matches = matches_cache.clone();
    let fr_query_cursor = match_cursor.clone();

    let fr_prev_state = state.clone();
    let fr_prev_matches = matches_cache.clone();
    let fr_prev_cursor = match_cursor.clone();

    let fr_next_state = state.clone();
    let fr_next_matches = matches_cache.clone();
    let fr_next_cursor = match_cursor.clone();

    let fr_rep_one_state = state.clone();
    let fr_rep_one_matches = matches_cache.clone();
    let fr_rep_one_cursor = match_cursor.clone();

    let fr_rep_all_state = state.clone();
    let fr_rep_all_matches = matches_cache.clone();
    let fr_rep_all_cursor = match_cursor.clone();

    find_replace_ui::wire(
        app,
        find_replace_ui::FindReplaceCallbacks {
            on_query_changed: Rc::new(move |app, query, case_sensitive, include_notes| {
                let session = fr_query_state.session.borrow();
                let found = loom_present_core::find_replace::find_matches_with_notes(
                    &session.document,
                    query,
                    case_sensitive,
                    include_notes,
                );
                let total = found.len();
                *fr_query_matches.borrow_mut() = found;
                *fr_query_cursor.borrow_mut() = 0;
                find_replace_ui::update_match_counts(app, if total > 0 { 1 } else { 0 }, total);
            }),
            on_find_prev: Rc::new(move |app| {
                let list = fr_prev_matches.borrow();
                if list.is_empty() {
                    return;
                }
                let mut cur = fr_prev_cursor.borrow_mut();
                *cur = if *cur == 0 { list.len() - 1 } else { *cur - 1 };
                let m = &list[*cur];
                {
                    let mut session = fr_prev_state.session.borrow_mut();
                    session.document.select_slide(m.slide_index);
                    if !m.is_notes {
                        session.select_element(&m.element_id, false);
                    }
                }
                find_replace_ui::update_match_counts(app, *cur + 1, list.len());
                refresh(app, &fr_prev_state);
            }),
            on_find_next: Rc::new(move |app| {
                let list = fr_next_matches.borrow();
                if list.is_empty() {
                    return;
                }
                let mut cur = fr_next_cursor.borrow_mut();
                *cur = (*cur + 1) % list.len();
                let m = &list[*cur];
                {
                    let mut session = fr_next_state.session.borrow_mut();
                    session.document.select_slide(m.slide_index);
                    if !m.is_notes {
                        session.select_element(&m.element_id, false);
                    }
                }
                find_replace_ui::update_match_counts(app, *cur + 1, list.len());
                refresh(app, &fr_next_state);
            }),
            on_replace_one: Rc::new(move |app, replacement| {
                let list = fr_rep_one_matches.borrow().clone();
                if list.is_empty() {
                    return;
                }
                let cur = (*fr_rep_one_cursor.borrow()).min(list.len() - 1);
                let m = &list[cur];
                let replaced = loom_present_core::find_replace::replace_match(
                    &mut fr_rep_one_state.session.borrow_mut(),
                    m,
                    replacement,
                );
                if replaced.is_ok() {
                    let s = app.global::<FindReplaceState>();
                    let found = loom_present_core::find_replace::find_matches_with_notes(
                        &fr_rep_one_state.session.borrow().document,
                        s.get_query().as_str(),
                        s.get_case_sensitive(),
                        s.get_include_notes(),
                    );
                    let total = found.len();
                    *fr_rep_one_matches.borrow_mut() = found;
                    *fr_rep_one_cursor.borrow_mut() = 0;
                    find_replace_ui::update_match_counts(app, if total > 0 { 1 } else { 0 }, total);
                    refresh(app, &fr_rep_one_state);
                }
            }),
            on_replace_all: Rc::new(move |app, replacement| {
                let s = app.global::<FindReplaceState>();
                let count = loom_present_core::find_replace::replace_all_with_notes(
                    &mut fr_rep_all_state.session.borrow_mut(),
                    s.get_query().as_str(),
                    replacement,
                    s.get_case_sensitive(),
                    s.get_include_notes(),
                );
                fr_rep_all_matches.borrow_mut().clear();
                *fr_rep_all_cursor.borrow_mut() = 0;
                find_replace_ui::update_match_counts(app, 0, 0);
                refresh(app, &fr_rep_all_state);
                set_status(app, format!("Replaced {count} occurrence(s)"));
            }),
        },
    );

    // Font Picker
    let fp_state = state.clone();
    let fp_app = app.as_weak();
    font_picker::wire(app, move |family| {
        let Some(app) = fp_app.upgrade() else { return };
        let msg = match element_style_ui::apply_font(&fp_state, family) {
            Ok(Some(words)) => words,
            Ok(None) => return,
            Err(err) => err,
        };
        refresh(&app, &fp_state);
        set_status(&app, &msg);
    });
}
