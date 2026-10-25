use super::super::actions_tests::test_state;
use super::super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_writer_core::loom_objects::{
    chart::{Chart, ChartType},
    table::Table,
};
use slint::Model;
use std::sync::Arc;

fn app_with_document(document: WriterDocument) -> (WriterApp, Rc<GuiState>) {
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(document, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    (app, state)
}

fn object_only_document(kind: &str) -> WriterDocument {
    let mut document = WriterDocument::new("justify-object", "Justify object");
    let mut block = match kind {
        "table" => {
            let mut table = Table::new(1, 1);
            table.set_cell_text(0, 0, "cell");
            RichBlock::table_block(1, table)
        }
        "image" => {
            let mut block =
                RichBlock::new(1, loom_writer_core::IMAGE_BLOCK_KIND, "\u{fffc}\u{e000}");
            block.image = Some(
                loom_writer_core::InlineImage::from_bytes("fixture.png", tiny_png())
                    .expect("valid image fixture"),
            );
            block
        }
        "chart" => {
            let chart = Chart::new("justify-chart", ChartType::Column);
            let mut block =
                RichBlock::new(1, loom_writer_core::CHART_BLOCK_KIND, "\u{fffc}\u{e000}");
            block.chart = Some(Arc::new(chart.to_json().expect("chart JSON")));
            block
        }
        _ => panic!("unknown object kind {kind}"),
    };
    block.style.alignment = loom_text::Alignment::Center;
    let end = block.text.as_str().len();
    document.blocks.push(block);
    document.set_selection(TextSelection::range(0, end));
    document
}

fn tiny_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 80, 120, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode picture fixture");
    bytes
}

fn query_live_palette(app: &WriterApp, state: &Rc<GuiState>, query: &str) -> Vec<String> {
    // Match run_gui_with_dialogs: shared-registry wiring follows the headless
    // handlers and owns the actual query and invocation callbacks.
    super::super::wire_palette(app);
    super::super::palette_wiring::wire(app, state);
    app.invoke_open_palette();
    app.set_palette_query(query.into());
    app.invoke_palette_query_changed(query.into());

    let commands = app.get_palette_commands();
    (0..commands.row_count())
        .filter_map(|row| commands.row_data(row))
        .map(|command| command.id.to_string())
        .collect()
}

fn assert_justify_not_offered(app: &WriterApp, state: &Rc<GuiState>, context: &str) {
    {
        let registry = state.registry.lock().expect("command registry");
        for command in ["writer.align.justify", "writer.align.justify-all"] {
            let spec = registry
                .get(&CommandId::new(command))
                .unwrap_or_else(|| panic!("registered command {command}"));
            assert!(!spec.enabled, "{context}: {command} must be disabled");
        }
    }

    let palette_ids = query_live_palette(app, state, "justify");
    assert!(
        !palette_ids
            .iter()
            .any(|id| id == "writer.align.justify" || id == "writer.align.justify-all"),
        "{context}: palette must not offer an unavailable Justify action: {palette_ids:?}"
    );
    let before_palette_accept = state.current.borrow().clone();
    app.invoke_palette_invoked(0);
    app.set_palette_open(false);
    assert_eq!(
        *state.current.borrow(),
        before_palette_accept,
        "{context}: the live palette accept path must not apply an omitted Justify command"
    );
    app.set_show_inspector(true);
    app.set_toolbar_menu(4);
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render alignment surfaces");
    assert_eq!(
        ElementHandle::find_by_accessible_label(app, "Justify").count(),
        0,
        "{context}: neither inspector nor Text menu should expose Justify"
    );
}

#[test]
fn object_only_selection_does_not_offer_or_apply_justify() {
    for kind in ["table", "image", "chart"] {
        let document = object_only_document(kind);
        let (app, state) = app_with_document(document);
        assert_justify_not_offered(&app, &state, kind);

        let before = state.current.borrow().clone();
        app.invoke_select_alignment(3);
        assert_eq!(
            *state.current.borrow(),
            before,
            "Justify must not mutate {kind} block style when no paragraph is selected"
        );
        assert!(
            !app.get_status_right().contains("Justify alignment applied"),
            "Justify must not announce an applied paragraph effect to a {kind}-only selection"
        );
    }
}

#[test]
fn mixed_text_and_table_selection_justifies_only_the_text_paragraph() {
    let mut document = WriterDocument::new("justify-mixed", "Justify mixed selection");
    let paragraph_text =
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ".repeat(24);
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", &paragraph_text));
    let mut table = Table::new(1, 1);
    table.set_cell_text(0, 0, "Keep this table centered");
    let mut table_block = RichBlock::table_block(2, table);
    table_block.style.alignment = loom_text::Alignment::Center;
    document.blocks.push(table_block);
    let end = document.editor_text().len();
    document.set_selection(TextSelection::range(0, end));

    let (app, state) = app_with_document(document);
    assert!(
        state
            .registry
            .lock()
            .expect("command registry")
            .get(&CommandId::new("writer.align.justify"))
            .is_some_and(|spec| spec.enabled),
        "a selected LTR paragraph makes Justify available even when an object is also selected"
    );
    let palette_ids = query_live_palette(&app, &state, "justify");
    let justify_index = palette_ids
        .iter()
        .position(|id| id == "writer.align.justify-all")
        .expect("the live palette includes the eligible Align All Justify command");
    app.invoke_palette_invoked(justify_index as i32);
    app.set_palette_open(false);
    app.set_toolbar_menu(4);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render Text menu");
    assert_eq!(
        ElementHandle::find_by_accessible_label(&app, "Justify")
            .filter(|element| element.accessible_role() == Some(AccessibleRole::ListItem))
            .count(),
        1,
        "the open Text menu exposes Justify as a menu list item"
    );
    let current = state.current.borrow();
    assert_eq!(
        current.blocks[0].style.alignment,
        loom_text::Alignment::Justify
    );
    assert_eq!(
        current.blocks[1].style.alignment,
        loom_text::Alignment::Center,
        "the object's stored horizontal alignment is preserved"
    );
    assert_eq!(
        app.get_status_right().as_str(),
        "Justify alignment applied to 1 paragraph; skipped 1 unsupported item"
    );
}

#[test]
fn rtl_only_selection_does_not_offer_or_apply_justify_before_bidi_support() {
    let text = "مرحبا بالعالم في مستند قابل للتحرير مع كلمات كثيرة لاختبار الالتفاف ".repeat(8);
    let mut document = WriterDocument::new("justify-rtl", "Justify RTL");
    document.blocks.push(RichBlock::new(1, "paragraph", &text));
    document.blocks[0].style.alignment = loom_text::Alignment::Justify;
    document.set_selection(TextSelection::range(0, text.len()));

    let (app, state) = app_with_document(document);
    assert!(
        writer_render_projection(&state.current.borrow(), PageViewport::default())
            .0
            .iter()
            .any(|row| row.unsupported_label == "Justify unavailable"),
        "the imported/current RTL paragraph is one whose M2 geometry defers justification"
    );
    assert_justify_not_offered(&app, &state, "RTL paragraph");

    let before = state.current.borrow().clone();
    app.invoke_select_alignment(3);
    assert_eq!(*state.current.borrow(), before);
    assert!(
        !app.get_status_right().contains("Justify alignment applied"),
        "the app must not announce a visual effect that M2 cannot render for this RTL paragraph"
    );
}

#[test]
fn mixed_ltr_rtl_selection_reports_applied_and_skipped_paragraph_counts() {
    let mut document = WriterDocument::new("justify-mixed-directions", "Mixed directions");
    document.blocks.push(RichBlock::new(
        1,
        "paragraph",
        "The left-to-right paragraph is selected with this one.",
    ));
    let mut rtl = RichBlock::new(2, "paragraph", "مرحبا بالعالم في المستند المحدد");
    rtl.style.alignment = loom_text::Alignment::Center;
    document.blocks.push(rtl);
    document.set_selection(TextSelection::range(0, document.editor_text().len()));

    let (app, state) = app_with_document(document);
    let mut candidate = state.current.borrow().clone();
    let selection = candidate.selection();
    let result = super::justify_selected_paragraphs(&mut candidate, selection);
    assert_eq!(
        result,
        super::JustificationResult {
            applied_paragraphs: 1,
            skipped_unsupported_items: 1,
        }
    );
    app.invoke_select_alignment(3);

    let current = state.current.borrow();
    assert_eq!(
        current.blocks[0].style.alignment,
        loom_text::Alignment::Justify
    );
    assert_eq!(
        current.blocks[1].style.alignment,
        loom_text::Alignment::Center
    );
    assert_eq!(
        app.get_status_right().as_str(),
        "Justify alignment applied to 1 paragraph; skipped 1 unsupported item"
    );
}

#[test]
fn imported_rtl_justify_is_preserved_but_not_offered_without_bidi_geometry() {
    let text = "مرحبا بالعالم في مستند قابل للتحرير مع كلمات كثيرة لاختبار الالتفاف ".repeat(8);
    let mut source = WriterDocument::new("docx-rtl-justify", "DOCX RTL Justify");
    let mut paragraph = RichBlock::new(1, "paragraph", &text);
    paragraph.style.alignment = loom_text::Alignment::Justify;
    source.blocks.push(paragraph);

    let exported = loom_writer_core::docx::export_docx(&source).expect("create DOCX bytes");
    let imported = loom_writer_core::import_docx(&exported.bytes).expect("import DOCX bytes");
    assert_eq!(
        imported.blocks[0].style.alignment,
        loom_text::Alignment::Justify
    );
    assert!(imported.blocks[0].justification_deferred_for_bidi());

    let (app, state) = app_with_document(imported);
    assert_eq!(
        state.current.borrow().blocks[0].style.alignment,
        loom_text::Alignment::Justify,
        "app projection must preserve the imported paragraph style"
    );
    assert!(
        writer_render_projection(&state.current.borrow(), PageViewport::default())
            .0
            .iter()
            .any(|row| row.unsupported_label == "Justify unavailable"),
        "an imported bidi paragraph must keep Justify but show the honest unavailable status"
    );
    assert_justify_not_offered(&app, &state, "imported RTL DOCX paragraph");
}
