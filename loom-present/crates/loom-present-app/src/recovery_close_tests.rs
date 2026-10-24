//! Close behavior when recovery cleanup fails.

use std::rc::Rc;

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::LogicalPosition;

use crate::recovery_deferred::test_support::Harness;
use crate::*;

struct SavedDeckScratch(std::path::PathBuf);

impl Drop for SavedDeckScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct RecoveryReplacementScratch;

impl Drop for RecoveryReplacementScratch {
    fn drop(&mut self) {
        recovery_replacement::forget_recovery_directory_for_test();
    }
}

fn click_accessible(app: &PresentApp, label: &str) {
    let item = ElementHandle::find_by_accessible_label(app, label)
        .next()
        .unwrap_or_else(|| panic!("missing accessible control {label:?}"));
    assert_eq!(item.accessible_role(), Some(AccessibleRole::Button));
    let position = item.absolute_position();
    let size = item.size();
    let position = LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    let button = PointerEventButton::Left;
    app.window()
        .dispatch_event(WindowEvent::PointerPressed { position, button });
    app.window()
        .dispatch_event(WindowEvent::PointerReleased { position, button });
    keyboard_flow_tests::render(app);
}

fn latest_recovery_payload(directory: &std::path::Path) -> Option<Vec<u8>> {
    let active = PRESENT_RECOVERY.with(|slot| slot.borrow_mut().take());
    drop(active);
    let recovery = loom_production::snapshot::SnapshotRecovery::open_at(directory)
        .expect("open recovery after Retry");
    let payload = recovery.restored_payload().map(ToOwned::to_owned);
    PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(recovery));
    payload
}

fn saved_deck_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "loom-present-close-recovery-{}-{}.loomdeck",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos()
    ))
}

#[test]
fn close_discard_stays_open_and_retry_recovers_after_clear_and_reopen_fail() {
    let mut harness = Harness::install();
    let save_path = saved_deck_path();
    let _saved_deck = SavedDeckScratch(save_path.clone());
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(save_path.clone())],
    ));
    let launched = keyboard_flow_tests::launched_with(dialogs);
    launched.app.show().expect("show test app");
    let baseline = launched.state.session.borrow().clone();
    let prior_recovery = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let _replacement_state = RecoveryReplacementScratch;
    let recovery = keyboard_flow_tests::RecoveryScratch::new_checkpoint(&prior_recovery);

    launched.app.invoke_add_slide();
    let visible = launched.state.session.borrow().clone();
    launched
        .app
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert!(launched.app.get_save_changes_open());

    let clear_failure = recovery_replacement::inject_recovery_clear_failure_for_test(
        "injected recovery clear failure",
    );
    let reopen_failure = recovery_replacement::inject_recovery_reopen_failure_for_test(
        "injected recovery reopen failure",
    );
    launched.app.invoke_save_changes_discard();

    assert!(
        launched.app.window().is_visible(),
        "a failed recovery clear must keep the application window open"
    );
    assert!(!launched.app.get_save_changes_open());
    let message = launched.app.get_recovery_failure_message().to_string();
    assert!(
        message.contains("injected recovery clear failure"),
        "{message}"
    );
    assert!(
        message.contains("injected recovery reopen failure"),
        "{message}"
    );
    assert!(message.contains("window remains open"), "{message}");
    assert!(message.contains("Retry or Save As"), "{message}");
    for label in ["Retry", "Save As"] {
        assert!(
            ElementHandle::find_by_accessible_label(&launched.app, label)
                .next()
                .is_some(),
            "{label} remains available after a close cleanup failure"
        );
    }
    assert!(
        recovery_deferred::is_stale(),
        "Retry must be enabled to persist the still-visible presentation"
    );

    drop(clear_failure);
    drop(reopen_failure);
    let save_as_reopen_failure = recovery_replacement::inject_recovery_reopen_failure_for_test(
        "injected Save As recovery reopen failure",
    );
    click_accessible(&launched.app, "Save As");
    assert!(
        save_path.is_file(),
        "Save As writes the visible presentation"
    );
    assert!(launched
        .app
        .get_status_left()
        .to_string()
        .contains("Deck saved; recovery checkpoint failed"));
    assert!(launched
        .app
        .get_recovery_failure_message()
        .to_string()
        .contains("injected Save As recovery reopen failure"));
    drop(save_as_reopen_failure);

    click_accessible(&launched.app, "Retry");
    assert_eq!(harness.launch_requests(), 1);
    let started = harness.receive_started();
    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.serialization_error(), None);
    assert!(delivery.invoke_error().is_some());
    delivery.run_after_invoke_failure();

    let payload = latest_recovery_payload(recovery.directory()).expect("Retry records the deck");
    let (_, restored) = recovery_draft::read(&payload).expect("Retry payload decodes");
    assert!(deck_guard::content_matches(
        &restored.document,
        &visible.document
    ));
    assert_eq!(restored.transitions, visible.transitions);
    assert!(!recovery_deferred::is_stale());
    assert!(launched.app.window().is_visible());
}

#[test]
fn close_failure_save_as_writes_visible_deck_and_allows_close() {
    let save_path = saved_deck_path();
    let _saved_deck = SavedDeckScratch(save_path.clone());
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(save_path.clone())],
    ));
    let launched = keyboard_flow_tests::launched_with(dialogs);
    launched.app.show().expect("show test app");
    let baseline = launched.state.session.borrow().clone();
    let prior_recovery = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let _replacement_state = RecoveryReplacementScratch;
    let _recovery = keyboard_flow_tests::RecoveryScratch::new_checkpoint(&prior_recovery);

    launched.app.invoke_add_slide();
    let visible = launched.state.session.borrow().clone();
    launched
        .app
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert!(launched.app.get_save_changes_open());
    let clear_failure = recovery_replacement::inject_recovery_clear_failure_for_test(
        "injected recovery clear failure",
    );
    let reopen_failure = recovery_replacement::inject_recovery_reopen_failure_for_test(
        "injected recovery reopen failure",
    );
    launched.app.invoke_save_changes_discard();
    assert!(launched.app.window().is_visible());
    assert!(launched
        .app
        .get_recovery_failure_message()
        .to_string()
        .contains("Retry or Save As"));

    drop(clear_failure);
    drop(reopen_failure);
    click_accessible(&launched.app, "Save As");
    assert!(
        save_path.is_file(),
        "Save As writes the visible presentation"
    );
    let saved = load_session(&save_path).expect("read the Save As result");
    assert!(deck_guard::content_matches(
        &saved.document,
        &visible.document
    ));
    assert_eq!(saved.transitions, visible.transitions);
    assert_eq!(
        launched.app.get_status_left().to_string(),
        format!("Saved {}", save_path.display()),
        "status confirms the file save without claiming a recovery checkpoint"
    );
    assert!(launched
        .app
        .get_recovery_failure_message()
        .to_string()
        .is_empty());

    launched
        .app
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert!(
        !launched.app.window().is_visible(),
        "after Save As preserves the deck, a later close can finish"
    );
}
