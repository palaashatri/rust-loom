#!/usr/bin/env python3
"""Make New create blank projects and keep save/checkpoint status truthful."""

from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def update_writer() -> None:
    path = Path("loom-writer/crates/loom-writer-app/src/main.rs")
    text = path.read_text()
    text = replace_once(
        text,
        '''/// A sample document used by `--smoke`, screenshots, and first launch.
fn sample_document() -> WriterDocument {''',
        '''fn blank_document() -> WriterDocument {
    WriterDocument::new("untitled", "Untitled Document")
}

/// A sample document used by `--smoke`, screenshots, and first launch.
fn sample_document() -> WriterDocument {''',
        "Writer blank-document constructor",
    )
    text = replace_once(
        text,
        '''                *state.current.borrow_mut() = sample_document();
                *state.save_path.borrow_mut() = None;''',
        '''                *state.current.borrow_mut() = blank_document();
                *state.save_path.borrow_mut() = None;''',
        "Writer New action",
    )
    text = replace_once(
        text,
        '''    #[test]
    fn scripted_dialog_request_uses_the_current_document_directory() {''',
        '''    #[test]
    fn new_document_is_blank_and_unsaved_ready() {
        let document = blank_document();
        assert!(document.blocks.is_empty());
        assert_eq!(document.title, "Untitled Document");
    }

    #[test]
    fn scripted_dialog_request_uses_the_current_document_directory() {''',
        "Writer blank-document test",
    )
    path.write_text(text)


def update_sheets() -> None:
    path = Path("loom-sheets/crates/loom-sheets-app/src/main.rs")
    text = path.read_text()
    text = replace_once(
        text,
        '''/// A small budget workbook used by `--smoke`, screenshots, and first launch.
fn sample_sheet() -> Sheet {''',
        '''fn blank_sheet() -> Sheet {
    Sheet::new("Untitled")
}

/// A small budget workbook used by `--smoke`, screenshots, and first launch.
fn sample_sheet() -> Sheet {''',
        "Sheets blank-workbook constructor",
    )
    text = replace_once(
        text,
        '''    checkpoint_snapshot_recovery(sheet_to_json(&state.current.borrow()).into_bytes()).map_err(
        |error| {
            format!(
                "saved {}, but recovery checkpoint failed: {error}",
                path.display()
            )
        },
    )?;
    app.set_status_left(SharedString::from(format!("Saved {}", path.display())));
    Ok(true)''',
        '''    match checkpoint_snapshot_recovery(sheet_to_json(&state.current.borrow()).into_bytes()) {
        Ok(()) => app.set_status_left(SharedString::from(format!("Saved {}", path.display()))),
        Err(error) => app.set_status_left(SharedString::from(format!(
            "Saved {}, but recovery checkpoint failed: {error}",
            path.display()
        ))),
    }
    Ok(true)''',
        "Sheets truthful checkpoint status",
    )
    text = replace_once(
        text,
        '''                *state.current.borrow_mut() = sample_sheet();
                *state.save_path.borrow_mut() = None;''',
        '''                *state.current.borrow_mut() = blank_sheet();
                *state.save_path.borrow_mut() = None;''',
        "Sheets New action",
    )
    text = replace_once(
        text,
        '''    #[test]
    fn scripted_dialog_request_uses_current_workbook_directory() {''',
        '''    #[test]
    fn new_workbook_is_blank_and_named_untitled() {
        let sheet = blank_sheet();
        assert!(sheet.cells.is_empty());
        assert_eq!(sheet.name, "Untitled");
    }

    #[test]
    fn scripted_dialog_request_uses_current_workbook_directory() {''',
        "Sheets blank-workbook test",
    )
    path.write_text(text)


def main() -> None:
    update_writer()
    update_sheets()


if __name__ == "__main__":
    main()
