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
            let doc = &session.document;
            let count = doc.slides.len();
            let (title, notes) = doc
                .active_slide()
                .map(|s| (s.title.clone(), s.speaker_notes.clone()))
                .unwrap_or_default();
            print_preview::open(&app, count, &title, &notes);
            sync_print_preview(&app, doc);
        });
    }

    // Welcome Guide
    let wg_state = state.clone();
    welcome_guide::wire(
        app,
        Rc::new(move |app, action| {
            match action {
                "templates" | "themes" => {
                    app.set_theme_selected(0);
                    app.set_theme_category(0);
                    app.set_theme_chooser_open(true);
                }
                "sample-deck" => app.invoke_new_sample_deck(),
                "format-inspector" => app.invoke_show_inspector_tab(0),
                "insert-table" => app.invoke_add_table(),
                "add-shape" => app.invoke_add_shape(),
                "speaker-notes" => app.set_show_notes_drawer(true),
                "print-preview" => app.invoke_open_print_preview(),
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
                let p = PathBuf::from(path);
                if !p.exists() {
                    set_status(app, format!("File not found: {}", p.display()));
                    return;
                }
                if crate::deck_guard::deck_is_dirty(&sh_state) {
                    crate::PENDING_RECENT_PATH.with(|slot| *slot.borrow_mut() = Some(p));
                    app.global::<StartHubState>().set_open(false);
                    let _ = crate::request_deck_replacement(
                        app,
                        &sh_state,
                        crate::PendingReplacement::OpenRecentDeck,
                    );
                    return;
                }
                if !matches!(
                    crate::open_deck_at_path(app, &sh_state, p),
                    crate::OpenPathOutcome::Failed
                ) {
                    app.global::<StartHubState>().set_open(false);
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
            let session = pp_refresh_state.session.borrow();
            sync_print_preview(app, &session.document);
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
                let found = {
                    let session = fr_query_state.session.borrow();
                    loom_present_core::find_replace::find_matches_with_notes(
                        &session.document,
                        query,
                        case_sensitive,
                        include_notes,
                    )
                };
                let total = found.len();
                if let Some(first) = found.first() {
                    let mut session = fr_query_state.session.borrow_mut();
                    session.document.select_slide(first.slide_index);
                    if !first.is_notes {
                        session.select_element(&first.element_id, false);
                    }
                    drop(session);
                    refresh(app, &fr_query_state);
                }
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
                let s = app.global::<FindReplaceState>();
                let fresh = loom_present_core::find_replace::find_matches_with_notes(
                    &fr_rep_one_state.session.borrow().document,
                    s.get_query().as_str(),
                    s.get_case_sensitive(),
                    s.get_include_notes(),
                );
                if fresh.is_empty() {
                    fr_rep_one_matches.borrow_mut().clear();
                    *fr_rep_one_cursor.borrow_mut() = 0;
                    find_replace_ui::update_match_counts(app, 0, 0);
                    return;
                }
                let cached = fr_rep_one_matches.borrow().clone();
                let cur = *fr_rep_one_cursor.borrow();
                let target = cached
                    .get(cur)
                    .filter(|m| fresh.contains(m))
                    .cloned()
                    .unwrap_or_else(|| fresh[cur.min(fresh.len() - 1)].clone());
                let replaced = loom_present_core::find_replace::replace_match(
                    &mut fr_rep_one_state.session.borrow_mut(),
                    &target,
                    replacement,
                );
                if replaced.is_ok() {
                    let found = loom_present_core::find_replace::find_matches_with_notes(
                        &fr_rep_one_state.session.borrow().document,
                        s.get_query().as_str(),
                        s.get_case_sensitive(),
                        s.get_include_notes(),
                    );
                    let total = found.len();
                    let next_idx = {
                        let session = fr_rep_one_state.session.borrow();
                        find_successor_after_replace(
                            &session.document,
                            &found,
                            &target,
                            replacement.len(),
                        )
                    };
                    if let Some(next_m) = found.get(next_idx) {
                        let mut session = fr_rep_one_state.session.borrow_mut();
                        session.document.select_slide(next_m.slide_index);
                        if !next_m.is_notes {
                            session.select_element(&next_m.element_id, false);
                        }
                    }
                    *fr_rep_one_matches.borrow_mut() = found;
                    *fr_rep_one_cursor.borrow_mut() = next_idx;
                    find_replace_ui::update_match_counts(
                        app,
                        if total > 0 { next_idx + 1 } else { 0 },
                        total,
                    );
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
                let remaining = loom_present_core::find_replace::find_matches_with_notes(
                    &fr_rep_all_state.session.borrow().document,
                    s.get_query().as_str(),
                    s.get_case_sensitive(),
                    s.get_include_notes(),
                );
                let total = remaining.len();
                *fr_rep_all_matches.borrow_mut() = remaining;
                *fr_rep_all_cursor.borrow_mut() = 0;
                find_replace_ui::update_match_counts(app, if total > 0 { 1 } else { 0 }, total);
                refresh(app, &fr_rep_all_state);
                set_status(app, format!("Replaced {count} occurrence(s)"));
            }),
        },
    );

    // Font Picker
    let fp_deck_state = state.clone();
    let fp_state = state.clone();
    let fp_app = app.as_weak();
    font_picker::wire(
        app,
        move || {
            let session = fp_deck_state.session.borrow();
            let mut families = Vec::new();
            for slide in &session.document.slides {
                for elem in &slide.elements {
                    if let Some(ff) = &elem.style.font_family {
                        families.push(ff.clone());
                    }
                    for run in &elem.runs {
                        if let Some(ff) = &run.font_family {
                            families.push(ff.clone());
                        }
                    }
                }
            }
            families
        },
        move |family, is_reset| {
            let Some(app) = fp_app.upgrade() else { return };
            let msg = match element_style_ui::apply_font_with_reset(&fp_state, family, is_reset) {
                Ok(Some(words)) => words,
                Ok(None) => return,
                Err(err) => err,
            };
            refresh(&app, &fp_state);
            set_status(&app, &msg);
        },
    );
}

fn sync_print_preview(app: &PresentApp, doc: &loom_present_core::PresentationDocument) {
    let options = build_print_options(app);
    let pages = preview_print_pages(doc, &options);
    let total = pages.len().max(1) as i32;
    let preview = app.global::<PrintPreviewState>();
    preview.set_aspect_ratio_wide(doc.aspect_ratio_wide);
    preview.set_total_pages(total);
    let cur = preview.get_current_page().clamp(1, total);
    preview.set_current_page(cur);
    if let Some(page) = pages.get((cur - 1) as usize) {
        let rows: Vec<crate::ThumbRows> = page
            .slide_indices
            .iter()
            .filter_map(|&si| doc.slides.get(si))
            .map(|slide| crate::picture_view::rows_for(doc, Some(slide)))
            .collect();
        preview.set_page_slides(slint::ModelRc::new(slint::VecModel::from(rows)));
        if let Some(first_slide) = page
            .slide_indices
            .first()
            .and_then(|&si| doc.slides.get(si))
        {
            preview.set_current_slide_title(slint::SharedString::from(first_slide.title.as_str()));
            preview.set_current_slide_notes(slint::SharedString::from(
                first_slide.speaker_notes.as_str(),
            ));
        }
    }
}

fn match_doc_rank(
    doc: &loom_present_core::PresentationDocument,
    m: &TextMatch,
) -> (usize, u8, usize, usize) {
    let elem_idx = if m.is_notes {
        usize::MAX
    } else {
        doc.slides
            .get(m.slide_index)
            .and_then(|s| s.elements.iter().position(|e| e.id == m.element_id))
            .unwrap_or(usize::MAX)
    };
    (m.slide_index, u8::from(m.is_notes), elem_idx, m.byte_start)
}

fn find_successor_after_replace(
    doc: &loom_present_core::PresentationDocument,
    found: &[TextMatch],
    target: &TextMatch,
    replacement_len: usize,
) -> usize {
    let mut target_rank = match_doc_rank(doc, target);
    target_rank.3 = target.byte_start.saturating_add(replacement_len);
    found
        .iter()
        .position(|m| match_doc_rank(doc, m) >= target_rank)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::find_successor_after_replace;
    use loom_present_core::find_replace::find_matches_with_notes;
    use loom_present_core::{ElementStyle, ElementType, PresentationDocument, Slide, SlideElement};

    #[test]
    fn replace_one_successor_follows_slide_element_notes_offset_order() {
        let mut doc = PresentationDocument::new("fr-order", "FR");
        doc.slides.clear();
        let make_box = |id: &str, text: &str| SlideElement {
            id: id.into(),
            element_type: ElementType::BodyText,
            content: text.into(),
            x: 40.0,
            y: 40.0,
            width: 400.0,
            height: 80.0,
            rotation_deg: 0.0,
            action: None,
            style: ElementStyle::default(),
            runs: Vec::new(),
            effects: None,
            group_id: None,
            locked: false,
            flip_h: false,
            flip_v: false,
            paragraphs: Vec::new(),
        };
        let mut s0 = Slide::new("s0", "S0", "blank");
        s0.elements
            .extend([make_box("e1", "foo and foo"), make_box("e2", "next foo")]);
        s0.speaker_notes = "note foo".into();
        let mut s1 = Slide::new("s1", "S1", "blank");
        s1.elements.push(make_box("e3", "later foo"));
        doc.slides.extend([s0, s1]);

        let matches = find_matches_with_notes(&doc, "foo", true, true);
        assert_eq!(matches.len(), 5);
        // After replacing second match in e1 (matches[1]), next is e2 on same slide (not s1).
        doc.slides[0].elements[0].content = "foo and bar".into();
        let post = find_matches_with_notes(&doc, "foo", true, true);
        let idx = find_successor_after_replace(&doc, &post, &matches[1], "bar".len());
        assert_eq!(post[idx].element_id, "e2");
        assert_eq!(post[idx].slide_index, 0);

        // After replacing e2 match (matches[2]), next is speaker notes on same slide (not s1).
        doc.slides[0].elements[1].content = "next bar".into();
        let post2 = find_matches_with_notes(&doc, "foo", true, true);
        let idx2 = find_successor_after_replace(&doc, &post2, &matches[2], "bar".len());
        assert!(post2[idx2].is_notes);
        assert_eq!(post2[idx2].slide_index, 0);
    }
}
