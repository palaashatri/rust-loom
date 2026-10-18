//! Save on a document opened from a Word file must not overwrite the Word
//! file with a Loom package.

use crate::actions_tests::test_state;
use crate::*;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-docxsave-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn save_after_opening_a_docx_asks_for_a_loom_file_and_leaves_the_word_file_alone() {
    let dir = temp_dir("keep");
    let word = dir.join("report.docx");
    std::fs::write(&word, b"original word bytes").unwrap();
    let target = dir.join("copy.loomdoc");
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(target.clone())],
    ));
    let mut doc = WriterDocument::new("d", "Report");
    doc.replace_paragraphs("Text from the Word file");
    let (app, state) = test_state(doc.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    *state.save_path.borrow_mut() = Some(word.clone());
    replace_opened_document(&app, &state, word.clone(), doc);

    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(&word).unwrap(),
        b"original word bytes",
        "the Word file must not be overwritten"
    );
    let saved = std::fs::read(&target).expect("the Loom copy was written");
    assert!(saved.starts_with(b"PK"), "a Loom package");
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(target.as_path()),
        "later saves go to the Loom file"
    );
}

#[test]
fn the_suggested_name_for_a_word_file_is_a_loom_name() {
    let dir = temp_dir("name");
    let word = dir.join("Quarterly Report.docx");
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (_app, state) = test_state(WriterDocument::new("d", "T"), dialogs);
    *state.save_path.borrow_mut() = Some(word);
    let request = writer_save_request(&state);
    assert_eq!(
        request.suggested_name.as_deref(),
        Some("Quarterly Report.loomdoc")
    );
}
