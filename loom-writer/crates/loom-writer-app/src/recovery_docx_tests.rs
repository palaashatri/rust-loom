use super::RecoveredDraft;
use loom_writer_core::{RichBlock, WriterDocument};
use std::path::PathBuf;

fn temp_dir() -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("loom-writer-recovery-docx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create test directory");
    dir
}

fn document(text: &str) -> WriterDocument {
    let mut document = WriterDocument::new("recovery-docx", "Recovery DOCX");
    document.blocks.push(RichBlock::new(1, "paragraph", text));
    document
}

#[test]
fn a_docx_recovery_draft_is_compared_with_the_imported_word_source() {
    let dir = temp_dir();
    let source = dir.join("source.docx");
    let exported =
        loom_writer_core::export_docx(&document("Saved text")).expect("export source DOCX");
    std::fs::write(&source, exported.bytes).expect("write source DOCX");
    let imported = crate::save_flow::load_file(&source).expect("import source DOCX");
    let draft = RecoveredDraft {
        document: imported,
        source: Some(source),
    };

    assert!(draft.is_clean_against_source());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_docx_recovery_draft_with_edits_remains_available() {
    let dir = temp_dir();
    let source = dir.join("source.docx");
    let exported =
        loom_writer_core::export_docx(&document("Saved text")).expect("export source DOCX");
    std::fs::write(&source, exported.bytes).expect("write source DOCX");
    let mut changed = crate::save_flow::load_file(&source).expect("import source DOCX");
    changed.blocks[0].text = loom_writer_core::Text::from_str("Recovered edit");
    let draft = RecoveredDraft {
        document: changed,
        source: Some(source),
    };

    assert!(!draft.is_clean_against_source());
    let _ = std::fs::remove_dir_all(dir);
}
