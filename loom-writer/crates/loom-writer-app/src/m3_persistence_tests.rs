use crate::history::{EditorHistory, HistoryKind};
use crate::recovery_draft::{self, RecoveredDraft};
use loom_text::{CharacterStyle, ParagraphDirection, StyleRun};
use loom_writer_core::{RichBlock, TextSelection, WriterDocument};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const CHILD_MODE: &str = "LOOM_WRITER_M3_PERSISTENCE_CHILD";
const OPEN_PATH: &str = "LOOM_WRITER_M3_PERSISTENCE_OPEN_PATH";

#[cfg(windows)]
const STATE_HOME_ENV: &str = "LOCALAPPDATA";
#[cfg(target_os = "macos")]
const STATE_HOME_ENV: &str = "HOME";
#[cfg(not(any(windows, target_os = "macos")))]
const STATE_HOME_ENV: &str = "XDG_STATE_HOME";

fn document_with_font_family(font_family: &str) -> WriterDocument {
    let mut document = WriterDocument::new("history-style", "History style");
    let mut block = RichBlock::new(1, "paragraph", "x");
    block.runs.push(StyleRun {
        start: 0,
        end: 1,
        style: CharacterStyle {
            font_family: font_family.to_owned(),
            ..CharacterStyle::default()
        },
    });
    document.push(block);
    document
}

fn document_with_table_font_family(font_family: &str) -> WriterDocument {
    use loom_objects::table::{Table, TextRun};

    let mut table = Table::new(1, 1);
    let mut run = TextRun::plain("x");
    run.font_family = Some(font_family.to_owned());
    table.set_cell_rich_text(0, 0, vec![run]);
    let mut document = WriterDocument::new("history-table", "History table");
    document.push(RichBlock::table_block(1, table));
    document
}

#[test]
fn history_retained_bytes_include_character_style_string_storage() {
    let family = "F".repeat(8 * 1024);
    let before = document_with_font_family("Small");
    let after = document_with_font_family(&family);

    let retained = crate::history::retained_bytes(&before, &after);

    assert!(
        retained >= family.capacity(),
        "history charge {retained} must include the {}-byte font-family allocation",
        family.capacity()
    );
}

#[test]
fn history_counts_a_shared_typed_table_once_and_distinct_tables_twice() {
    let family = "T".repeat(8 * 1024);
    let before = document_with_table_font_family(&family);
    let after_shared = before.clone();
    let after_distinct = document_with_table_font_family(&family);

    let shared = crate::history::retained_bytes(&before, &after_shared);
    let distinct = crate::history::retained_bytes(&before, &after_distinct);

    assert!(
        distinct > shared + family.len() / 2,
        "a separate table allocation must be charged independently: shared={shared}, distinct={distinct}"
    );
}

#[test]
fn history_byte_budget_keeps_the_newest_styled_undo_step() {
    let budget = 4 * 1024;
    let mut history = EditorHistory::with_budget(8, budget);
    let mut document = document_with_font_family("Base");
    let mut expected_undo = document.clone();

    for index in 0..3 {
        expected_undo = document.clone();
        let mut next = document.clone();
        next.blocks[0].runs[0].style.font_family =
            format!("Family-{index}-{}", "x".repeat(4 * 1024));
        history.record(
            document,
            next.clone(),
            HistoryKind::DocumentAction,
            index as u64,
        );
        document = next;
    }

    assert_eq!(
        history.undo_len(),
        1,
        "older entries exceed the byte budget"
    );
    assert!(
        history.total_bytes() > budget,
        "the newest entry stays available even when it alone exceeds the budget"
    );
    assert_eq!(history.undo(), Some(expected_undo));
}

#[test]
fn source_less_recovery_only_treats_default_blank_documents_as_clean() {
    let blank = WriterDocument::new("blank", "Untitled");
    assert!(draft(blank.clone()).is_clean_against_source());

    let mut empty_paragraph = blank.clone();
    empty_paragraph.push(RichBlock::new(1, "paragraph", ""));
    assert!(draft(empty_paragraph.clone()).is_clean_against_source());

    crate::paragraph_alignment::set_selection_direction(
        &mut empty_paragraph,
        TextSelection::caret(0),
        ParagraphDirection::RightToLeft,
    );
    assert!(
        !draft(empty_paragraph).is_clean_against_source(),
        "a direction-only empty paragraph is meaningful recovered content"
    );

    let mut page_only = blank;
    page_only.page.margins = loom_writer_core::PageMarginsPreset::Narrow;
    assert!(
        !draft(page_only).is_clean_against_source(),
        "page setup changes must keep an otherwise empty draft"
    );
}

fn draft(document: WriterDocument) -> RecoveredDraft {
    RecoveredDraft {
        document,
        source: None,
    }
}

#[test]
fn recovery_replacement_quarantines_direction_only_blank_and_drops_pristine_blank() {
    let child_mode = std::env::var(CHILD_MODE).ok();
    match child_mode.as_deref() {
        Some("record-blank") | Some("record-direction") => {
            let startup = crate::recovery::initialize_editing_session(None).expect("initialize");
            assert!(startup.draft.is_none());
            let document = if child_mode.as_deref() == Some("record-direction") {
                let mut document = WriterDocument::new("direction-only", "Untitled");
                document.push(RichBlock::new(1, "paragraph", ""));
                crate::paragraph_alignment::set_selection_direction(
                    &mut document,
                    TextSelection::caret(0),
                    ParagraphDirection::RightToLeft,
                );
                document
            } else {
                WriterDocument::new("untouched", "Untitled")
            };
            crate::recovery::record_document(&document, None).expect("record the draft");
            std::process::exit(0);
        }
        Some("replace-blank") | Some("replace-direction") => {
            let open_path = PathBuf::from(
                std::env::var_os(OPEN_PATH).expect("the replacement file path is set"),
            );
            let startup = crate::recovery::initialize_editing_session(Some(&open_path))
                .expect("open another document through recovery startup");
            assert!(startup.draft.is_none());
            let root = recovery_draft::quarantine_root().expect("quarantine root");
            let entries = recovery_draft::kept_entries(&root);
            if child_mode.as_deref() == Some("replace-blank") {
                assert!(
                    entries.is_empty(),
                    "pristine blank drafts remain discardable"
                );
                assert!(startup.notice.is_none());
            } else {
                assert_eq!(entries.len(), 1, "the changed draft is quarantined");
                let payload = recovery_draft::read_kept(&entries[0])
                    .expect("read kept draft")
                    .expect("kept payload");
                let kept = RecoveredDraft::from_payload(&payload).expect("decode kept draft");
                assert!(kept.source.is_none());
                assert_eq!(
                    kept.document.blocks[0].style.direction,
                    ParagraphDirection::RightToLeft
                );
                assert!(startup.notice.is_some());
            }
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown persistence child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated state root");
    let other_path = root.join("other.loomdoc");
    let mut other = WriterDocument::new("other", "Other document");
    other.push(RichBlock::new(1, "paragraph", "saved file"));
    std::fs::write(
        &other_path,
        loom_writer_core::save_document(&other).expect("save replacement fixture"),
    )
    .expect("write replacement fixture");

    for (record_mode, replace_mode) in [
        ("record-blank", "replace-blank"),
        ("record-direction", "replace-direction"),
    ] {
        let status = run_child(record_mode, &root, &other_path);
        assert!(status.success(), "record child failed for {record_mode}");
        let status = run_child(replace_mode, &root, &other_path);
        assert!(
            status.success(),
            "replacement child failed for {replace_mode}"
        );
    }

    std::fs::remove_dir_all(root).expect("remove isolated test state");
}

fn isolated_root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "loom-writer-m3-persistence-{}-{sequence}",
        std::process::id()
    ))
}

fn run_child(mode: &str, root: &Path, open_path: &Path) -> ExitStatus {
    const TEST_NAME: &str = "recovery_draft::m3_persistence_tests::recovery_replacement_quarantines_direction_only_blank_and_drops_pristine_blank";
    let state_home = root.join("state");
    Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", TEST_NAME])
        .env(CHILD_MODE, mode)
        .env(STATE_HOME_ENV, state_home)
        .env(OPEN_PATH, open_path)
        .status()
        .expect("run isolated recovery process")
}
