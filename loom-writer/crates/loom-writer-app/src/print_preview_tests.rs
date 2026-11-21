use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use loom_desktop::ScriptedFileDialogs;
use loom_writer_core::{PageMarginsPreset, PageOrientation, PaperSize, RichBlock, WriterDocument};
use slint::{ComponentHandle, Model};

use crate::actions_tests::{test_state, text_document};
use crate::print_preview::{apply_page_setup, export_pdf, open, wire};
use crate::{wire_writer_shared_callbacks, PrintPreviewState};

fn unique_pdf_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "loom-writer-print-preview-{label}-{}-{nanos}.pdf",
        std::process::id()
    ))
}

#[test]
fn preview_updates_dimensions_for_paper_orientation_and_margins() {
    let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document("Hello print preview"), dialogs);
    wire(&app, &state, &None);
    open(&app, &state);

    let preview = app.global::<PrintPreviewState>();
    assert!(preview.get_open());
    assert_eq!(preview.get_page_width_pt(), 595.0);
    assert_eq!(preview.get_page_height_pt(), 842.0);
    assert!(!preview.get_is_empty());

    preview.set_paper_size(1);
    preview.set_orientation(1);
    preview.set_margin_preset(1);
    preview.invoke_settings_changed();
    assert_eq!(preview.get_page_width_pt(), 792.0);
    assert_eq!(preview.get_page_height_pt(), 612.0);
    assert_eq!(preview.get_margin_left_pt(), 36.0);
}

#[test]
fn preview_paginates_multi_page_documents_with_headers_footers_and_tables() {
    let mut doc = WriterDocument::new("multi", "Multi-Page Report");
    doc.header_footer.header_text = "Report Header".into();
    doc.header_footer.footer_text = "Page {page} of {total}".into();
    for i in 0..85 {
        let id = doc.next_id();
        doc.push(RichBlock::new(
            id,
            "paragraph",
            &format!("Paragraph {i}: Detailed project narrative for pagination verification."),
        ));
    }
    doc.insert_table_block(0, 2, 2).expect("insert table");

    let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(doc, dialogs);
    wire(&app, &state, &None);
    open(&app, &state);

    let preview = app.global::<PrintPreviewState>();
    assert!(preview.get_total_pages() >= 2);
    assert_eq!(preview.get_current_page(), 1);
    assert!(preview.get_boxes().row_count() >= 4);

    let lines_p1: Vec<String> = (0..preview.get_lines().row_count())
        .filter_map(|i| preview.get_lines().row_data(i).map(|l| l.text.to_string()))
        .collect();
    assert!(lines_p1.iter().any(|t| t == "Report Header"));
    assert!(lines_p1.iter().any(|t| t.starts_with("Page 1 of ")));

    preview.invoke_next_page();
    assert_eq!(preview.get_current_page(), 2);
    preview.invoke_prev_page();
    assert_eq!(preview.get_current_page(), 1);
}

#[test]
fn apply_page_setup_mutates_document_in_one_undo_step() {
    let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document("Setup target"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    wire(&app, &state, &None);
    open(&app, &state);

    let preview = app.global::<PrintPreviewState>();
    preview.set_paper_size(1);
    preview.set_orientation(1);
    preview.set_margin_preset(2);
    preview.invoke_apply_page_setup();

    assert_eq!(state.current.borrow().page.paper, PaperSize::Letter);
    assert_eq!(
        state.current.borrow().page.orientation,
        PageOrientation::Landscape
    );
    assert_eq!(state.current.borrow().page.margins, PageMarginsPreset::Wide);
    assert_eq!(state.history.borrow().undo_len(), 1);

    // Unchanged reapply does not record a redundant undo entry.
    assert!(!apply_page_setup(&app, &state, None));
    assert_eq!(state.history.borrow().undo_len(), 1);

    // Undoing reverts all three page setup dimensions together.
    app.invoke_undo();
    assert_eq!(state.current.borrow().page.paper, PaperSize::A4);
    assert_eq!(
        state.current.borrow().page.orientation,
        PageOrientation::Portrait
    );
    assert_eq!(
        state.current.borrow().page.margins,
        PageMarginsPreset::Normal
    );
}

#[test]
fn revision_view_switch_and_export_pdf_produce_valid_pdf_bytes() {
    let mut doc = WriterDocument::new("rev-doc", "Tracked Report");
    let b1 = doc.next_id();
    doc.push(RichBlock::new(
        b1,
        "paragraph",
        "Approved summary paragraph.",
    ));
    doc.insert_footnote(b1, 8, "Footnote text for preview.")
        .expect("insert footnote");

    let out_final = unique_pdf_path("final");
    let out_review = unique_pdf_path("review");
    let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new(
        [],
        [Some(out_final.clone()), Some(out_review.clone())],
    ));
    let (app, state) = test_state(doc, dialogs);
    wire(&app, &state, &None);
    open(&app, &state);

    let preview = app.global::<PrintPreviewState>();
    let exported_final = export_pdf(&app, &state).expect("final pdf export");
    assert_eq!(exported_final.as_ref(), Some(&out_final));
    let final_bytes = std::fs::read(&out_final).expect("read final pdf");
    assert!(final_bytes.starts_with(b"%PDF-"));

    preview.set_revision_view(1);
    preview.invoke_settings_changed();
    let lines_review: Vec<String> = (0..preview.get_lines().row_count())
        .filter_map(|i| preview.get_lines().row_data(i).map(|l| l.text.to_string()))
        .collect();
    assert!(lines_review.iter().any(|t| t.contains("PDF view: Review")));

    let exported_review = export_pdf(&app, &state).expect("review pdf export");
    assert_eq!(exported_review.as_ref(), Some(&out_review));
    let review_bytes = std::fs::read(&out_review).expect("read review pdf");
    assert!(review_bytes.starts_with(b"%PDF-"));

    let _ = std::fs::remove_file(&out_final);
    let _ = std::fs::remove_file(&out_review);
}
