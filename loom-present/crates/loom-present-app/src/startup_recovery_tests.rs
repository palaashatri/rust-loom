//! Startup with a recovered draft. The draft is restored as unsaved work, kept in
//! a file of its own, or the file opens clean when the two match. Nothing is
//! dropped silently. Each case was seen failing before the planner changed.

use std::path::{Path, PathBuf};

use loom_present_core::{
    export_pptx, inspect_pptx_unsupported, load_presentation_session, save_presentation_session,
    PresentationSession,
};

use crate::recovery_draft;
use crate::sample_session;
use crate::startup_recovery::{
    ensure_kept_draft_is_preserved, kept_drafts_directory, startup_sessions, write_kept_draft,
    KeptDraft,
};

/// A fresh scratch folder for one test. The caller removes it.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "loom-present-startup-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// The sample deck plus one slide: an edit that was never saved.
fn edited_sample() -> PresentationSession {
    let mut session = sample_session();
    session
        .document
        .add_slide("Added before the crash", "content");
    session
}

/// Writes `session` as a saved deck and returns its path.
fn save_to(dir: &Path, file: &str, session: &PresentationSession) -> PathBuf {
    let path = dir.join(file);
    let bytes = save_presentation_session(session).expect("package the deck");
    std::fs::write(&path, bytes).expect("write the saved deck");
    path
}

fn slide_count(session: &PresentationSession) -> usize {
    session.document.slides.len()
}

/// Writes an exported deck with a SmartArt part, which Present warns about and
/// skips during import.
fn save_unsupported_pptx(dir: &Path) -> PathBuf {
    let exported = export_pptx(&sample_session()).expect("export fixture PPTX");
    let archive = loom_package::zip::PackageArchive::from_bytes(&exported).expect("open PPTX");
    let mut unsupported = loom_package::zip::PackageArchive::new();
    for path in archive.paths() {
        if let Some(bytes) = archive.get(path) {
            unsupported
                .add(path, bytes.to_vec())
                .expect("copy PPTX part");
        }
    }
    unsupported
        .add("ppt/diagrams/data1.xml", b"<xml></xml>".to_vec())
        .expect("add unsupported SmartArt part");
    let bytes = unsupported.to_bytes().expect("package PPTX");
    assert!(
        inspect_pptx_unsupported(&bytes)
            .expect("inspect PPTX")
            .iter()
            .any(|warning| warning.contains("SmartArt")),
        "fixture must trigger the import warning"
    );
    let path = dir.join("unsupported.pptx");
    std::fs::write(&path, bytes).expect("write PPTX");
    if let Some(output) = std::env::var_os("LOOM_PRESENT_LOSSY_PPTX_OUT") {
        let output = PathBuf::from(output);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).expect("create QA fixture directory");
        }
        std::fs::copy(&path, output).expect("copy verified lossy PPTX fixture");
    }
    path
}

#[test]
fn a_draft_of_the_opened_file_is_restored_as_unsaved_work() {
    let dir = scratch("same-path");
    let file = save_to(&dir, "deck.loomdeck", &sample_session());
    let bytes = recovery_draft::payload(&edited_sample(), Some(&file)).expect("draft");

    let plan = startup_sessions(Some(&bytes), Some(&file)).expect("startup");

    assert_eq!(
        slide_count(&plan.session),
        4,
        "the unsaved draft is what opens, not the file on disk"
    );
    assert_eq!(
        slide_count(&plan.baseline),
        3,
        "the file on disk is what Save and the close prompt compare with"
    );
    assert_eq!(plan.save_path.as_deref(), Some(file.as_path()));
    assert_eq!(plan.restored.as_deref(), Some("deck"));
    assert!(plan.kept.is_none(), "the same file keeps nothing aside");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_restored_draft_keeps_its_file_name_when_launched_without_a_path() {
    let dir = scratch("plain-launch");
    let file = save_to(&dir, "deck.loomdeck", &sample_session());
    let bytes = recovery_draft::payload(&edited_sample(), Some(&file)).expect("draft");

    let plan = startup_sessions(Some(&bytes), None).expect("startup");

    assert_eq!(
        plan.save_path.as_deref(),
        Some(file.as_path()),
        "Ctrl+S saves the draft over its own file"
    );
    assert_eq!(plan.restored.as_deref(), Some("deck"));
    assert_eq!(slide_count(&plan.session), 4);
    assert_eq!(slide_count(&plan.baseline), 3);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_draft_of_another_file_is_kept_while_the_requested_file_opens() {
    let dir = scratch("other-file");
    let requested = save_to(&dir, "requested.loomdeck", &sample_session());
    let other = dir.join("other.loomdeck");
    let bytes = recovery_draft::payload(&edited_sample(), Some(&other)).expect("draft");

    let plan = startup_sessions(Some(&bytes), Some(&requested)).expect("startup");

    assert_eq!(
        slide_count(&plan.session),
        3,
        "the requested file opens as saved"
    );
    assert_eq!(plan.save_path.as_deref(), Some(requested.as_path()));
    let kept = plan
        .kept
        .expect("the other file's draft is kept, not dropped");
    assert!(kept.readable);
    let reopened = load_presentation_session(&kept.bytes).expect("kept draft loads");
    assert_eq!(slide_count(&reopened), 4, "the kept bytes are the draft");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_lossy_startup_pptx_stays_pending_while_another_file_draft_is_current() {
    let dir = scratch("pptx-warning-keeps-recovery");
    let source = save_to(&dir, "recovered.loomdeck", &sample_session());
    let recovered = edited_sample();
    let recovery_bytes = recovery_draft::payload(&recovered, Some(&source)).expect("draft");
    let requested = save_unsupported_pptx(&dir);
    let requested_bytes = std::fs::read(&requested).expect("read PPTX fixture");

    let plan = startup_sessions(Some(&recovery_bytes), Some(&requested)).expect("startup");
    let expected_recovery_package =
        save_presentation_session(&recovered).expect("package recovered draft");

    assert!(
        crate::deck_guard::content_matches(&plan.session.document, &recovered.document),
        "the recovered deck must remain live until the lossy import is accepted"
    );
    assert_eq!(plan.session.transitions, recovered.transitions);
    assert_eq!(plan.save_path.as_deref(), Some(source.as_path()));
    assert_eq!(plan.restored.as_deref(), Some("recovered"));
    let pending = plan
        .pending_import
        .as_ref()
        .expect("lossy startup PowerPoint is held for a choice");
    assert_eq!(pending.path, requested);
    assert!(pending
        .warnings
        .iter()
        .any(|warning| warning.contains("SmartArt")));
    assert_eq!(slide_count(&pending.session), 3);
    assert_eq!(
        plan.kept.as_ref().map(|kept| kept.bytes.as_slice()),
        Some(expected_recovery_package.as_slice()),
        "the full recovered package must be available for quarantine on Continue"
    );
    assert_eq!(
        std::fs::read(&requested).expect("PPTX remains on disk"),
        requested_bytes,
        "planning an import must not mutate the requested PowerPoint file"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_same_source_pptx_draft_keeps_precedence_over_the_startup_file() {
    let dir = scratch("same-pptx-source");
    let requested = save_unsupported_pptx(&dir);
    let recovered = edited_sample();
    let recovery_bytes = recovery_draft::payload(&recovered, Some(&requested)).expect("draft");

    let plan = startup_sessions(Some(&recovery_bytes), Some(&requested)).expect("startup");

    assert!(crate::deck_guard::content_matches(
        &plan.session.document,
        &recovered.document
    ));
    assert_eq!(plan.session.transitions, recovered.transitions);
    assert!(plan.kept.is_none(), "same-source work stays in recovery");
    assert!(
        plan.pending_import.is_none(),
        "the same-source recovered draft remains the startup document"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unreadable_recovery_stays_untouched_while_a_lossy_startup_import_waits() {
    let dir = scratch("unreadable-pptx-warning");
    let requested = save_unsupported_pptx(&dir);
    let unreadable = b"not a readable recovery draft";

    let plan = startup_sessions(Some(unreadable), Some(&requested)).expect("startup");

    assert_eq!(
        slide_count(&plan.session),
        1,
        "the unrecovered deck state remains the blank current session"
    );
    assert_eq!(
        slide_count(&plan.baseline),
        1,
        "the blank session remains the saved baseline until Continue"
    );
    let pending = plan
        .pending_import
        .as_ref()
        .expect("the lossy PowerPoint still requires a choice");
    assert!(pending
        .warnings
        .iter()
        .any(|warning| warning.contains("SmartArt")));
    let kept = plan
        .kept
        .expect("unreadable recovery is deferred for preservation");
    assert!(!kept.readable);
    assert_eq!(kept.bytes.as_slice(), unreadable);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_untitled_draft_is_kept_beside_an_opened_file_and_restored_on_a_plain_launch() {
    let dir = scratch("untitled");
    let requested = save_to(&dir, "requested.loomdeck", &sample_session());
    // A draft recorded before drafts named their file.
    let legacy = save_presentation_session(&edited_sample()).expect("legacy package");

    let kept_plan = startup_sessions(Some(&legacy), Some(&requested)).expect("startup");
    assert!(
        kept_plan.kept.is_some(),
        "untitled work is kept when another file is opened"
    );
    assert_eq!(slide_count(&kept_plan.session), 3);

    let plain = startup_sessions(Some(&legacy), None).expect("startup");
    assert_eq!(slide_count(&plain.session), 4, "a plain launch restores it");
    assert_eq!(plain.save_path, None);
    assert!(plain.restored.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_draft_that_matches_its_file_opens_clean() {
    let dir = scratch("matches");
    let file = save_to(&dir, "deck.loomdeck", &sample_session());
    let bytes = recovery_draft::payload(&sample_session(), Some(&file)).expect("draft");

    let opened = startup_sessions(Some(&bytes), Some(&file)).expect("startup");
    assert_eq!(opened.restored, None, "nothing is unsaved");
    assert!(opened.kept.is_none());
    assert!(crate::deck_guard::content_matches(
        &opened.baseline.document,
        &opened.session.document
    ));

    let launched = startup_sessions(Some(&bytes), None).expect("startup");
    assert_eq!(launched.restored, None, "a plain launch is clean too");
    assert_eq!(launched.save_path.as_deref(), Some(file.as_path()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_draft_of_a_file_that_is_gone_is_restored_for_that_path() {
    let dir = scratch("missing");
    let file = dir.join("deleted.loomdeck");
    let bytes = recovery_draft::payload(&edited_sample(), Some(&file)).expect("draft");

    let plan = startup_sessions(Some(&bytes), Some(&file)).expect("startup");

    assert_eq!(slide_count(&plan.session), 4, "the draft is not lost");
    assert_eq!(plan.save_path.as_deref(), Some(file.as_path()));
    assert!(plan.restored.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unreadable_recovery_data_is_kept_as_it_was_found() {
    let plan = startup_sessions(Some(b"not a deck"), None).expect("startup");

    let kept = plan.kept.expect("unreadable data is kept, not dropped");
    assert!(!kept.readable);
    assert_eq!(kept.bytes, b"not a deck".to_vec());
    assert_eq!(slide_count(&plan.session), 1, "the window opens blank");
}

#[test]
fn kept_drafts_are_written_beside_the_recovery_store_and_never_overwritten() {
    let dir = scratch("written");
    let package = save_presentation_session(&edited_sample()).expect("package");
    let kept = KeptDraft {
        name: "deck".into(),
        bytes: package,
        readable: true,
    };

    let first = write_kept_draft(&dir, &kept, 7).expect("first write");
    let second = write_kept_draft(&dir, &kept, 7).expect("second write");

    assert_ne!(first, second, "a second draft never replaces the first");
    for path in [&first, &second] {
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("loomdeck"));
        let bytes = std::fs::read(path).expect("kept file");
        assert_eq!(
            slide_count(&load_presentation_session(&bytes).expect("loads")),
            4
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unreadable_recovery_is_verified_byte_for_byte_before_a_choice_can_release_it() {
    let dir = scratch("verified-unreadable");
    let bytes = b"unreadable recovery bytes".to_vec();
    let kept = KeptDraft {
        name: "Unreadable recovered draft".into(),
        bytes: bytes.clone(),
        readable: false,
    };
    let mut preserved_path = None;

    let path = ensure_kept_draft_is_preserved(&dir, &kept, &mut preserved_path)
        .expect("quarantine exact recovery bytes");

    assert_eq!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("recovered")
    );
    assert_eq!(preserved_path.as_deref(), Some(path.as_path()));
    assert_eq!(std::fs::read(&path).expect("quarantine bytes"), bytes);

    std::fs::write(&path, b"changed bytes").expect("tamper with quarantine fixture");
    assert!(
        ensure_kept_draft_is_preserved(&dir, &kept, &mut preserved_path).is_err(),
        "a changed quarantine must not be reported as preserved"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn kept_drafts_sit_beside_the_recovery_store_so_clearing_it_cannot_remove_them() {
    let _state_directory = crate::test_environment::lock_state_directory();
    let store = loom_production::snapshot::application_state_directory("org.loom.present")
        .expect("recovery directory");
    let kept = kept_drafts_directory().expect("kept drafts directory");

    assert!(
        !kept.starts_with(&store),
        "clearing the store removes its whole folder"
    );
    assert_eq!(kept.parent(), store.parent());
}

#[test]
fn an_unedited_draft_is_not_kept_as_unsaved_when_another_file_opens() {
    let dir = scratch("unedited-clean");
    let requested = save_to(&dir, "requested.loomdeck", &sample_session());
    let other = save_to(&dir, "other.loomdeck", &sample_session());
    // The other file was in recovery, but was UNEDITED (matches file on disk).
    let bytes = recovery_draft::payload(&sample_session(), Some(&other)).expect("draft");

    let plan = startup_sessions(Some(&bytes), Some(&requested)).expect("startup");
    assert!(
        plan.kept.is_none(),
        "an unedited draft is not kept as an unsaved file when opening another file"
    );
    let _ = std::fs::remove_dir_all(dir);
}
