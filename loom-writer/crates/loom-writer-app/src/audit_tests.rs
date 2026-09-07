use super::*;
use loom_desktop::CommandState;
use loom_text::{CharacterStyle, FontWeight, StyleRun};
use loom_writer_core::{
    load_document, save_document, PageStyle, PageViewport, RichBlock, TextSelection, WriterDocument,
};

fn test_audit_doc() -> WriterDocument {
    let mut doc = WriterDocument::new("audit-doc", "Writer Deep Audit");
    doc.push(RichBlock::new(1, "heading1", "Executive Summary"));
    let mut p1 = RichBlock::new(
        2,
        "paragraph",
        "Loom Writer is a professional creative word processor.",
    );
    p1.runs.push(StyleRun {
        start: 0,
        end: 11,
        style: CharacterStyle {
            weight: FontWeight::Bold,
            ..CharacterStyle::default()
        },
    });
    p1.runs.push(StyleRun {
        start: 17,
        end: 29,
        style: CharacterStyle {
            italic: true,
            ..CharacterStyle::default()
        },
    });
    doc.push(p1);
    doc.push(RichBlock::new(3, "heading2", "Core Architecture"));
    doc.push(RichBlock::new(
        4,
        "paragraph",
        "It supports local-first document editing with multi-page flow and rich text styles.",
    ));
    doc
}

#[test]
fn test_deep_audit_document_model_and_layout() {
    let doc = test_audit_doc();
    assert_eq!(doc.blocks.len(), 4);
    assert_eq!(doc.blocks[0].kind, "heading1");
    assert_eq!(doc.blocks[0].text.as_str(), "Executive Summary");
    assert_eq!(doc.blocks[1].runs.len(), 2);

    let style = PageStyle::default();
    let viewport = PageViewport::default();
    let layout = doc.layout(&style, viewport).expect("document layout");

    assert!(!layout.page_bounds.is_empty());
    assert!(!layout.fragments.is_empty());
    assert!(layout.page_bounds[0].height > 0.0);

    // Verify pagination estimates
    let pagination = doc.estimate_pagination();
    assert!(pagination.total_pages >= 1);
    assert!(pagination.words > 0);
}

#[test]
fn test_deep_audit_selection_and_grapheme_navigation() {
    let mut doc = test_audit_doc();
    let full_text = doc.editor_text();

    // Caret at start
    doc.set_selection(TextSelection::caret(0));
    assert!(doc.selection.is_collapsed());
    assert_eq!(doc.selection.anchor, 0);

    // Grapheme-aware selection
    let graphemes = grapheme_boundaries(&full_text);
    assert!(!graphemes.is_empty());

    let target_end = graphemes[graphemes.len().min(12)];
    doc.set_selection(TextSelection::range(0, target_end));
    assert_eq!(doc.selected_text(), &full_text[..target_end]);

    // Grapheme count announcement
    let count = grapheme_count(&doc.selected_text());
    let announcement = format!("Selected {count} characters");
    assert!(announcement.starts_with("Selected"));
}

#[test]
fn test_deep_audit_undo_redo_isolation_and_coalescing() {
    let mut history = EditorHistory::with_budget(32, usize::MAX);
    let mut doc0 = WriterDocument::new("undo-test", "Undo Test");
    doc0.replace_paragraphs("Initial");

    let mut doc1 = doc0.clone();
    doc1.replace_paragraphs("Initial edit");

    let mut doc2 = doc1.clone();
    doc2.replace_paragraphs("Initial edit 2");

    // Coalescing typing edits
    history.record(doc0.clone(), doc1.clone(), HistoryKind::Typing, 100);
    history.record(doc1.clone(), doc2.clone(), HistoryKind::Typing, 200);
    assert_eq!(history.undo_len(), 1); // coalesced

    // Discrete action breaks coalescing
    let mut doc3 = doc2.clone();
    doc3.blocks[0].runs.push(StyleRun {
        start: 0,
        end: 7,
        style: CharacterStyle {
            weight: FontWeight::Bold,
            ..CharacterStyle::default()
        },
    });
    history.record(doc2.clone(), doc3.clone(), HistoryKind::DocumentAction, 300);
    assert_eq!(history.undo_len(), 2);

    // Undo formatting
    let undone = history.undo().expect("undo formatting");
    assert_eq!(undone, doc2);

    // Undo typing
    let undone_typing = history.undo().expect("undo typing");
    assert_eq!(undone_typing, doc0);

    // Redo both
    let redone_typing = history.redo().expect("redo typing");
    assert_eq!(redone_typing, doc2);
    let redone_formatting = history.redo().expect("redo formatting");
    assert_eq!(redone_formatting, doc3);
}

#[test]
fn test_deep_audit_native_loomdoc_persistence_and_recovery() {
    let doc = test_audit_doc();

    // Native package save
    let package_bytes = save_document(&doc).expect("save loomdoc package");
    assert!(!package_bytes.is_empty());

    // Native package reload
    let loaded = load_document(&package_bytes).expect("load loomdoc package");
    assert_eq!(loaded.id, doc.id);
    assert_eq!(loaded.title, doc.title);
    assert_eq!(loaded.blocks.len(), doc.blocks.len());
    for (orig, loaded_blk) in doc.blocks.iter().zip(loaded.blocks.iter()) {
        assert_eq!(orig.kind, loaded_blk.kind);
        assert_eq!(orig.text, loaded_blk.text);
        assert_eq!(orig.runs, loaded_blk.runs);
    }

    // Corrupted content rejection
    let corrupted = b"not a valid loomdoc package";
    assert!(load_document(corrupted).is_err());
}

#[test]
fn test_deep_audit_document_statistics_and_toc() {
    let doc = test_audit_doc();

    let stats = doc.statistics();
    assert!(stats.word_count > 10);

    let toc = doc.generate_toc();
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Executive Summary");
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[1].title, "Core Architecture");
    assert_eq!(toc[1].level, 2);
}

#[test]
fn test_deep_audit_export_pipelines_pdf_and_markdown() {
    let doc = test_audit_doc();

    // Markdown export
    let md = doc.to_markdown();
    assert!(md.contains("# Executive Summary"));
    assert!(md.contains("## Core Architecture"));
    assert!(md.contains("Loom Writer is a professional"));

    // PDF export
    let pdf_bytes = loom_writer_core::export_pdf(&doc);
    assert!(!pdf_bytes.is_empty());
    assert!(pdf_bytes.starts_with(b"%PDF-"));
}

#[test]
fn test_deep_audit_native_macos_global_menu_bar() {
    let menu = NativeMenuBar::new();
    let mut bar = build_standard_menu_bar(
        "Loom Writer",
        vec![
            MenuItem::action_with_shortcut(
                "file.export_pdf",
                "Export PDF...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_md",
                "Export Markdown...",
                MenuShortcut::primary_shift("M"),
            ),
        ],
        vec![MenuItem::action_with_shortcut(
            "edit.select_all",
            "Select All",
            MenuShortcut::primary("A"),
        )],
        vec![MenuItem::check("view.inspector", "Format Inspector", true)],
        vec![Menu::new(
            "Format",
            vec![
                MenuItem::action_with_shortcut("format.bold", "Bold", MenuShortcut::primary("B")),
                MenuItem::action_with_shortcut(
                    "format.italic",
                    "Italic",
                    MenuShortcut::primary("I"),
                ),
                MenuItem::action_with_shortcut(
                    "format.underline",
                    "Underline",
                    MenuShortcut::primary("U"),
                ),
            ],
        )],
    );

    bar.disable_items_except([
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.export_pdf",
        "file.export_md",
        "edit.undo",
        "edit.redo",
        "edit.select_all",
        "view.inspector",
        "format.bold",
        "format.italic",
        "format.underline",
    ]);

    menu.install_menu_bar(&bar).expect("install menu bar");
    assert!(menu.is_installed());

    let installed = menu.installed_menu_bar().expect("installed menu bar");
    assert!(installed.find_item("file.export_pdf").is_some());
    assert!(installed.find_item("format.bold").is_some());
    assert!(installed.find_item("format.italic").is_some());
    assert!(installed.find_item("format.underline").is_some());

    // Update command states dynamically
    menu.update_item("format.bold", true, Some(true))
        .expect("update bold");
    let state = CommandState::check("view.inspector", "Format Inspector", false);
    menu.update_command_state(&state).expect("update state");
}
