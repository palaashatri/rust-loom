//! Startup with a recovered draft. The draft is restored as unsaved work, kept in
//! a file of its own, or the file opens clean when the two match. Nothing is
//! dropped silently. Each case was seen failing before the planner changed.

use std::path::{Path, PathBuf};

use loom_present_core::{
    load_presentation_session, save_presentation_session, PresentationSession,
};

use crate::recovery_draft;
use crate::sample_session;
use crate::startup_recovery::{
    kept_drafts_directory, startup_sessions, write_kept_draft, KeptDraft,
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
fn kept_drafts_sit_beside_the_recovery_store_so_clearing_it_cannot_remove_them() {
    let store = loom_production::snapshot::application_state_directory("org.loom.present")
        .expect("recovery directory");
    let kept = kept_drafts_directory().expect("kept drafts directory");

    assert!(
        !kept.starts_with(&store),
        "clearing the store removes its whole folder"
    );
    assert_eq!(kept.parent(), store.parent());
}
