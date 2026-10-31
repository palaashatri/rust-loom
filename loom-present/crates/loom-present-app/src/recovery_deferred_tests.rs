//! Behavioral tests for asynchronous recovery writes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use i_slint_backend_testing::{AccessibleLiveness, AccessibleRole, ElementHandle, ElementRoot};
use loom_desktop::appearance::Appearance;
use loom_present_core::PresentationSession;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{LogicalPosition, PhysicalSize};

use crate::recovery_deferred::test_support::{Delivery, Harness};
use crate::*;

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(1);
const PRIOR_RECOVERY_TITLE: &str = "Recoverable prior deck";

struct RecoveryScratch {
    previous: Option<loom_production::snapshot::SnapshotRecovery>,
    directories: Vec<PathBuf>,
    current: Option<PathBuf>,
}

impl RecoveryScratch {
    fn without_service() -> Self {
        let previous = PRESENT_RECOVERY.with(|slot| slot.borrow_mut().take());
        Self {
            previous,
            directories: Vec::new(),
            current: None,
        }
    }

    fn with_service(payload: &[u8]) -> Self {
        let mut scratch = Self::without_service();
        scratch.install_service(payload);
        scratch
    }

    fn install_service(&mut self, payload: &[u8]) -> &std::path::Path {
        let active = PRESENT_RECOVERY.with(|slot| slot.borrow_mut().take());
        drop(active);
        let directory = std::env::temp_dir().join(format!(
            "loom-present-deferred-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&directory).expect("create recovery test directory");
        let mut recovery = loom_production::snapshot::SnapshotRecovery::open_at(&directory)
            .expect("open recovery test store");
        recovery
            .record("test seed", payload.to_vec())
            .expect("seed recovery test store");
        drop(recovery);
        let recovery = loom_production::snapshot::SnapshotRecovery::open_at(&directory)
            .expect("reopen recovery test store");
        PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(recovery));
        self.directories.push(directory.clone());
        self.current = Some(directory);
        self.current
            .as_deref()
            .expect("installed recovery directory")
    }

    fn directory(&self) -> &std::path::Path {
        self.current
            .as_deref()
            .expect("recovery service is installed")
    }

    fn reopen_latest(&mut self) -> Option<Vec<u8>> {
        let directory = self.current.clone()?;
        let active = PRESENT_RECOVERY.with(|slot| slot.borrow_mut().take());
        drop(active);
        let recovery = loom_production::snapshot::SnapshotRecovery::open_at(&directory)
            .expect("independently reopen recovery test store");
        let latest = recovery.restored_payload().map(ToOwned::to_owned);
        PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(recovery));
        latest
    }
}

impl Drop for RecoveryScratch {
    fn drop(&mut self) {
        recovery_deferred::invalidate();
        recovery_replacement::forget_recovery_directory_for_test();
        let active = PRESENT_RECOVERY.with(|slot| slot.borrow_mut().take());
        drop(active);
        if let Some(previous) = self.previous.take() {
            PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(previous));
        }
        for directory in &self.directories {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

struct AppendFault(PathBuf);

impl AppendFault {
    fn once(directory: &std::path::Path) -> Self {
        Self::times(directory, 1)
    }

    fn times(directory: &std::path::Path, times: u32) -> Self {
        loom_production::fault_injection::inject(
            directory,
            loom_production::fault_injection::FaultStep::JournalAppend,
            0,
            times,
        );
        Self(directory.to_path_buf())
    }

    fn hits(&self) -> u32 {
        loom_production::fault_injection::hits(
            &self.0,
            loom_production::fault_injection::FaultStep::JournalAppend,
        )
    }
}

impl Drop for AppendFault {
    fn drop(&mut self) {
        loom_production::fault_injection::clear(&self.0);
    }
}

struct CheckpointPayloadFault(PathBuf);

impl CheckpointPayloadFault {
    fn once(directory: &std::path::Path) -> Self {
        loom_production::fault_injection::inject(
            directory,
            loom_production::fault_injection::FaultStep::CheckpointPayload,
            0,
            1,
        );
        Self(directory.to_path_buf())
    }

    fn hits(&self) -> u32 {
        loom_production::fault_injection::hits(
            &self.0,
            loom_production::fault_injection::FaultStep::CheckpointPayload,
        )
    }
}

impl Drop for CheckpointPayloadFault {
    fn drop(&mut self) {
        loom_production::fault_injection::clear(&self.0);
    }
}

fn app_with_session(session: PresentationSession) -> (PresentApp, std::rc::Rc<GuiState>) {
    let (app, state) = frame_bench_tests::setup(session.clone());
    *state.last_saved.borrow_mut() = session.document.clone();
    *state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    (app, state)
}

fn app_with_open_results(
    session: PresentationSession,
    open_results: Vec<Option<PathBuf>>,
) -> (PresentApp, std::rc::Rc<GuiState>) {
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new(open_results, []));
    let launched = keyboard_flow_tests::launched_with(dialogs);
    *launched.state.session.borrow_mut() = session.clone();
    *launched.state.last_saved.borrow_mut() = session.document.clone();
    *launched.state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    recovery_deferred::invalidate();
    refresh_without_recovery(&launched.app, &launched.state);
    (launched.app, launched.state)
}

fn app_with_save_result(
    session: PresentationSession,
    save_result: Option<PathBuf>,
) -> (PresentApp, std::rc::Rc<GuiState>) {
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [save_result]));
    let launched = keyboard_flow_tests::launched_with(dialogs);
    *launched.state.session.borrow_mut() = session.clone();
    *launched.state.last_saved.borrow_mut() = session.document.clone();
    *launched.state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    recovery_deferred::invalidate();
    refresh_without_recovery(&launched.app, &launched.state);
    (launched.app, launched.state)
}

struct DeckFileFixture {
    directory: PathBuf,
    path: PathBuf,
}

impl DeckFileFixture {
    fn new(session: &PresentationSession) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "loom-present-deferred-open-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&directory).expect("create Open fixture directory");
        let path = directory.join("candidate.loomdeck");
        let bytes = save_presentation_session(session).expect("serialize Open fixture deck");
        std::fs::write(&path, bytes).expect("write Open fixture deck");
        Self { directory, path }
    }

    fn new_pptx(session: &PresentationSession) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "loom-present-deferred-open-pptx-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&directory).expect("create Open PPTX fixture directory");
        let path = directory.join("candidate.pptx");
        let bytes = export_pptx(session).expect("export warning-free Open fixture PPTX");
        std::fs::write(&path, bytes).expect("write Open PPTX fixture");
        let candidate = pptx_import::prepare(&path).expect("prepare Open PPTX fixture");
        assert!(
            candidate.warnings.is_empty(),
            "the fixture must exercise ordinary warning-free PPTX Open"
        );
        Self { directory, path }
    }
}

impl Drop for DeckFileFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn edit_by_adding_slide(app: &PresentApp, state: &GuiState) -> PresentationSession {
    app.invoke_add_slide();
    state.session.borrow().clone()
}

fn persisted_session(scratch: &mut RecoveryScratch) -> PresentationSession {
    let bytes = scratch.reopen_latest().expect("recovery payload exists");
    recovery_draft::read(&bytes)
        .expect("decode persisted recovery draft")
        .1
}

fn prior_source_less_recovery_payload() -> Vec<u8> {
    let mut session = sample_session();
    session.document.title = PRIOR_RECOVERY_TITLE.into();
    recovery_draft::payload(&session, None).expect("serialize prior source-less recovery draft")
}

fn assert_prior_source_less_draft_is_restorable(recovery: &mut RecoveryScratch) {
    let bytes = recovery
        .reopen_latest()
        .expect("prior source-less draft is available for restart");
    let (draft, session) = recovery_draft::read(&bytes).expect("prior draft is valid");
    assert_eq!(draft.source, None, "the prior draft has no saved source");
    assert_eq!(
        session.document.title, PRIOR_RECOVERY_TITLE,
        "the seeded prior draft is the deck that a crash could restore"
    );
}

fn run_production_completion(delivery: Delivery) {
    assert_eq!(
        delivery.serialization_error(),
        None,
        "the worker must serialize the deck before dispatch"
    );
    assert!(
        delivery.invoke_error().is_some(),
        "the deterministic harness must simulate event-loop rejection"
    );
    delivery.run_after_invoke_failure();
}

fn accessible_label_containing(app: &PresentApp, needle: &str) -> Option<String> {
    app.root_element()
        .query_descendants()
        .match_predicate(|_| true)
        .find_all()
        .into_iter()
        .filter_map(|element| element.accessible_label().map(|label| label.to_string()))
        .find(|label| label.contains(needle))
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

fn assert_notice_controls_fit(
    app: &PresentApp,
    width: f32,
    height: f32,
    scale_factor: f32,
    text_scale: f32,
) {
    Theme::get(app).set_text_scale(text_scale);
    app.window()
        .set_size(PhysicalSize::new(width as u32, height as u32));
    configure_responsive_layout(app, (width as u32, height as u32));
    let _ = snapshot_component(app, width, height, scale_factor).expect("render recovery notice");
    let notice = ElementHandle::find_by_accessible_label(app, "Recovery failure")
        .next()
        .expect("the recovery notice is accessible");
    let notice_position = notice.absolute_position();
    let notice_size = notice.size();
    assert!(
        notice_position.x >= 0.0
            && notice_position.y >= 0.0
            && notice_position.x + notice_size.width <= width
            && notice_position.y + notice_size.height <= height,
        "the full notice must fit at {width}x{height} with text scale {text_scale}: position={notice_position:?}, size={notice_size:?}"
    );
    if text_scale >= 2.0 {
        assert!(
            notice_size.height >= 160.0,
            "the notice must grow to fit scaled text and controls: size={notice_size:?}"
        );
    }
    for label in ["Retry", "Save As"] {
        let item = ElementHandle::find_by_accessible_label(app, label)
            .next()
            .unwrap_or_else(|| panic!("{label} must remain exposed to accessibility"));
        let position = item.absolute_position();
        let size = item.size();
        assert!(
            position.x >= 0.0
                && position.y >= 0.0
                && position.x + size.width <= width
                && position.y + size.height <= height,
            "{label} must stay visible at {width}x{height} ({scale_factor}x): position={position:?}, size={size:?}"
        );
        assert!(
            position.y >= notice_position.y
                && position.y + size.height <= notice_position.y + notice_size.height,
            "{label} must remain inside the recovery notice: button={position:?}/{size:?}, notice={notice_position:?}/{notice_size:?}"
        );
        if text_scale >= 2.0 {
            assert!(
                size.height >= 40.0,
                "{label} must grow with 2x text: size={size:?}"
            );
        }
    }
    let error = accessible_label_containing(app, "no space left on device")
        .expect("full recovery cause is exposed to accessibility");
    let item = ElementHandle::find_by_accessible_label(app, &error)
        .next()
        .expect("recovery cause text is present");
    assert_eq!(item.accessible_role(), Some(AccessibleRole::Text));
    assert_eq!(
        item.accessible_live_region(),
        Some(AccessibleLiveness::Polite),
        "the full error should be announced as a polite live update"
    );
}

fn assert_same_content(actual: &PresentationSession, expected: &PresentationSession) {
    assert!(
        same_content(actual, expected),
        "persisted deck content must match the expected session"
    );
}

fn same_content(actual: &PresentationSession, expected: &PresentationSession) -> bool {
    deck_guard::content_matches(&actual.document, &expected.document)
        && actual.transitions == expected.transitions
}

#[test]
fn reordered_completions_cannot_publish_an_older_snapshot() {
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);

    let older = edit_by_adding_slide(&app, &state);
    let older_epoch = recovery_deferred::current_epoch();
    let older_bytes = recovery_draft::payload(&older, None).expect("older draft");
    recovery_deferred::prepare_delivery_for_test(older_epoch);
    let app_weak = app.as_weak();
    let older_delivery =
        Delivery::completion_for_test(1, older_epoch, older_bytes.clone(), app_weak.clone());

    // A save/replacement checkpoint supersedes the pending older delivery.
    // The next edit can now have its own serialized delivery in flight.
    recovery_deferred::invalidate();
    let newer = edit_by_adding_slide(&app, &state);
    let newer_epoch = recovery_deferred::current_epoch();
    let newer_bytes = recovery_draft::payload(&newer, None).expect("newer draft");
    assert_ne!(
        older_bytes, newer_bytes,
        "the edits must produce distinct snapshots"
    );
    recovery_deferred::prepare_delivery_for_test(newer_epoch);

    let newer_delivery =
        Delivery::completion_for_test(2, newer_epoch, newer_bytes.clone(), app_weak);
    newer_delivery.run_after_invoke_failure();
    older_delivery.run_after_invoke_failure();

    assert!(
        recovery.reopen_latest().as_deref() == Some(newer_bytes.as_slice()),
        "a delayed older serialization must not overwrite the newer recovery snapshot"
    );
}

#[test]
fn append_failure_keeps_the_latest_draft_pending_and_the_reason_visible() {
    let mut harness = Harness::install();
    let (app, state) = app_with_session(sample_session());
    let original = state.session.borrow().clone();
    let original_bytes = recovery_draft::payload(&original, None).expect("original draft");
    let mut recovery = RecoveryScratch::with_service(&original_bytes);
    let fault = AppendFault::times(recovery.directory(), 2);
    let latest = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let started_id = started.id;
    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, started_id);
    run_production_completion(delivery);

    let fault_hits = fault.hits();
    assert_eq!(fault_hits, 1, "the real journal append fault was reached");
    assert_eq!(
        recovery.reopen_latest().as_deref(),
        Some(original_bytes.as_slice())
    );
    assert!(
        recovery_deferred::is_stale(),
        "the latest draft must remain pending after its append fails"
    );
    assert!(
        recovery_deferred::retry_is_armed_for_test(),
        "an append failure must schedule its bounded automatic retry"
    );
    let reason = "injected disk-full failure at JournalAppend";
    let status = app.get_status_left().to_string();
    assert!(
        !status.contains(reason) && status.len() < 64,
        "the status stays short while the full cause is presented separately: {status:?}"
    );
    let full_error = accessible_label_containing(&app, "no space left on device")
        .expect("the recovery notice exposes the full append error");
    assert!(full_error.contains(reason));
    assert_notice_controls_fit(&app, 1280.0, 800.0, 1.0, 1.0);
    assert_notice_controls_fit(&app, 1024.0, 720.0, 2.0, 2.0);

    refresh(&app, &state);
    assert!(!app.get_status_left().to_string().contains(reason));
    assert_eq!(
        accessible_label_containing(&app, "no space left on device").as_deref(),
        Some(full_error.as_str()),
        "normal refresh keeps the full cause in the notice"
    );
    set_status(&app, "Saved presentation. Recovery checkpoint failed.");
    assert!(!app.get_status_left().to_string().contains(reason));
    assert_eq!(
        accessible_label_containing(&app, "no space left on device").as_deref(),
        Some(full_error.as_str()),
        "ordinary status updates keep the full cause in the notice"
    );
    assert_same_content(&latest, &state.session.borrow());

    click_accessible(&app, "Retry");
    assert!(
        !recovery_deferred::retry_is_armed_for_test(),
        "explicit Retry supersedes the pending automatic timer"
    );
    let retry_started = harness.receive_started();
    retry_started.release();
    run_production_completion(harness.receive_delivery());
    assert_eq!(
        fault.hits(),
        2,
        "the explicit retry reaches the append fault"
    );
    assert!(recovery_deferred::is_stale());
    assert!(
        accessible_label_containing(&app, "no space left on device").is_some(),
        "a failed explicit retry keeps the full error actionable"
    );
    assert!(ElementHandle::find_by_accessible_label(&app, "Retry")
        .next()
        .is_some());
    assert!(ElementHandle::find_by_accessible_label(&app, "Save As")
        .next()
        .is_some());
    assert_eq!(harness.launch_requests(), 2);
    assert_eq!(
        recovery.reopen_latest().as_deref(),
        Some(original_bytes.as_slice()),
        "failed explicit Retry must not claim the latest draft was persisted"
    );

    loom_production::fault_injection::clear(recovery.directory());
    click_accessible(&app, "Retry");
    assert_eq!(harness.launch_requests(), 3);
    let retry_started = harness.receive_started();
    let retry_started_id = retry_started.id;
    retry_started.release();
    let retry_delivery = harness.receive_delivery();
    assert_eq!(retry_delivery.id, retry_started_id);
    run_production_completion(retry_delivery);

    assert!(
        !recovery_deferred::is_stale(),
        "a successful retry consumes the pending latest draft"
    );
    assert_same_content(&persisted_session(&mut recovery), &latest);
    assert!(
        accessible_label_containing(&app, "no space left on device").is_none(),
        "a successful durable retry clears the recovery notice"
    );
}

#[test]
fn save_as_notice_action_reuses_save_flow_and_retry_persists_the_latest_deck() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let save_file = DeckFileFixture::new(&baseline);
    let save_path = save_file.path.clone();
    let (app, state) = app_with_save_result(baseline, Some(save_path.clone()));
    let append_fault = AppendFault::times(recovery.directory(), 2);
    let latest = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    started.release();
    run_production_completion(harness.receive_delivery());
    assert_eq!(
        append_fault.hits(),
        1,
        "the real append error created the notice"
    );
    assert!(accessible_label_containing(&app, "no space left on device").is_some());

    recovery_deferred::fire_retry_callback_for_test(&app);
    let automatic_retry_started = harness.receive_started();
    automatic_retry_started.release();
    run_production_completion(harness.receive_delivery());
    assert_eq!(
        append_fault.hits(),
        2,
        "the scheduled one-shot retry also reaches the append fault"
    );
    assert!(recovery_deferred::is_stale());
    assert!(accessible_label_containing(&app, "no space left on device").is_some());

    let checkpoint_fault = CheckpointPayloadFault::once(recovery.directory());
    click_accessible(&app, "Save As");
    assert_eq!(
        append_fault.hits(),
        3,
        "Save As appends the latest deck before the checkpoint payload fault"
    );

    assert_eq!(
        checkpoint_fault.hits(),
        1,
        "Save As reached checkpoint storage"
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(save_path.as_path())
    );
    assert!(
        save_path.is_file(),
        "the deck file was saved before checkpoint failure"
    );
    assert_eq!(
        app.get_status_left().to_string(),
        "Deck saved; recovery checkpoint failed",
        "status must tell the truth without eliding the cause"
    );
    let checkpoint_error = accessible_label_containing(&app, "CheckpointPayload")
        .expect("the notice retains the actual checkpoint failure");
    assert!(checkpoint_error.contains("no space left on device"));
    assert!(recovery_deferred::is_stale());
    assert!(ElementHandle::find_by_accessible_label(&app, "Retry")
        .next()
        .is_some());
    assert!(ElementHandle::find_by_accessible_label(&app, "Save As")
        .next()
        .is_some());
    assert!(recovery_deferred::retry_is_armed_for_test());
    loom_production::fault_injection::clear(recovery.directory());
    click_accessible(&app, "Retry");
    let retry_started = harness.receive_started();
    retry_started.release();
    run_production_completion(harness.receive_delivery());

    let recovered_bytes = recovery
        .reopen_latest()
        .expect("retry wrote a recovery payload");
    let (draft, recovered) = recovery_draft::read(&recovered_bytes).expect("decode recovery draft");
    assert_same_content(&recovered, &latest);
    assert_eq!(draft.source.as_deref(), Some(save_path.as_path()));
    let saved = load_session(&save_path).expect("reopen saved deck");
    assert_same_content(&saved, &latest);
    assert!(!recovery_deferred::is_stale());
    assert_eq!(
        app.get_status_left().to_string(),
        "Deck saved; recovery is current",
        "a successful retry must clear the stale checkpoint-failure status"
    );
    assert!(accessible_label_containing(&app, "no space left on device").is_none());
}

#[test]
fn serialization_failure_is_reported_and_kept_pending_for_retry() {
    let _harness = Harness::install();
    let (app, state) = app_with_session(sample_session());
    edit_by_adding_slide(&app, &state);
    let epoch = recovery_deferred::current_epoch();
    recovery_deferred::prepare_delivery_for_test(epoch);

    let failure = Delivery::serialization_error_for_test(
        1,
        epoch,
        "injected serializer failure".into(),
        app.as_weak(),
    );
    failure.run_after_invoke_failure();

    assert!(recovery_deferred::is_stale());
    assert!(accessible_label_containing(
        &app,
        "snapshot serialization failed: injected serializer failure"
    )
    .is_some());
    assert!(!app
        .get_status_left()
        .to_string()
        .contains("snapshot serialization failed"));
    assert!(
        recovery_deferred::retry_is_armed_for_test(),
        "serialization failure must use the same bounded retry path"
    );
}

#[test]
fn a_persistent_recovery_failure_gets_one_retry_then_waits_for_a_new_edit() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let recovery = RecoveryScratch::with_service(&baseline_bytes);
    let fault = AppendFault::times(recovery.directory(), 2);
    let (app, state) = app_with_session(baseline);
    edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let first_started = harness.receive_started();
    first_started.release();
    run_production_completion(harness.receive_delivery());
    assert!(recovery_deferred::retry_is_armed_for_test());

    recovery_deferred::fire_retry_callback_for_test(&app);
    let retry_started = harness.receive_started();
    retry_started.release();
    run_production_completion(harness.receive_delivery());

    assert_eq!(fault.hits(), 2, "both append attempts reached the journal");
    assert!(recovery_deferred::is_stale());
    assert!(
        !recovery_deferred::retry_is_armed_for_test(),
        "a persistent error must not create a tight automatic retry loop"
    );
    assert_eq!(harness.launch_requests(), 2);

    edit_by_adding_slide(&app, &state);
    assert!(recovery_deferred::edit_timer_is_armed_for_test());
    assert!(
        !recovery_deferred::retry_is_armed_for_test(),
        "a new edit uses the normal debounce timer, not the failed retry timer"
    );
}

#[test]
fn accepted_replacement_arms_new_recovery_work_after_cancelling_the_old_retry() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let recovery = RecoveryScratch::with_service(&baseline_bytes);
    let fault = AppendFault::times(recovery.directory(), 2);
    let (app, state) = app_with_session(baseline);
    edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let first_started = harness.receive_started();
    first_started.release();
    run_production_completion(harness.receive_delivery());
    assert_eq!(fault.hits(), 1, "the initial append failure was injected");
    assert!(recovery_deferred::retry_is_armed_for_test());

    app.invoke_new_deck();
    assert!(app.get_save_changes_open(), "New requires a decision");
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert!(app
        .get_status_left()
        .to_string()
        .contains("Created unsaved presentation"));
    assert!(
        !recovery_deferred::retry_is_armed_for_test(),
        "clearing the discarded deck cancels its retry"
    );
    assert!(
        recovery_deferred::edit_timer_is_armed_for_test(),
        "the replacement schedules its own recovery snapshot"
    );

    // Drive the same recovery-flush callback that the 600ms edit timer invokes.
    app.invoke_recovery_flush();
    let replacement_started = harness.receive_started();
    replacement_started.release();
    run_production_completion(harness.receive_delivery());

    assert_eq!(
        fault.hits(),
        2,
        "the replacement append failure was injected"
    );
    assert!(recovery_deferred::is_stale());
    assert!(
        recovery_deferred::retry_is_armed_for_test(),
        "a failed replacement append must retain its bounded retry even after status projection"
    );
    assert!(
        accessible_label_containing(&app, "injected disk-full failure at JournalAppend").is_some(),
        "the replacement's append failure remains visible after status projection"
    );
}

#[test]
fn appearance_choice_does_not_hide_a_recovery_append_error() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let recovery = RecoveryScratch::with_service(&baseline_bytes);
    let fault = AppendFault::once(recovery.directory());
    let (app, state) = app_with_session(baseline);
    edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    started.release();
    run_production_completion(harness.receive_delivery());
    assert_eq!(
        fault.hits(),
        1,
        "the append fault must reach the real journal"
    );
    let error = "injected disk-full failure at JournalAppend";
    assert!(accessible_label_containing(&app, error).is_some());

    appearance::choose(&app, Appearance::Dark);

    let status = app.get_status_left().to_string();
    assert!(status.contains("Appearance: Dark"));
    assert!(status.contains("Recovery pending"));
    assert!(
        accessible_label_containing(&app, error).is_some(),
        "appearance status refresh must retain the unresolved full recovery error"
    );
}

#[test]
fn invalidation_during_held_serialization_cannot_overwrite_a_new_checkpoint() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);
    let older = edit_by_adding_slide(&app, &state);
    let older_bytes = recovery_draft::payload(&older, None).expect("older draft");

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    assert!(recovery_deferred::serialization_is_active_for_test());

    let checkpoint = edit_by_adding_slide(&app, &state);
    let checkpoint_bytes = recovery_draft::payload(&checkpoint, None).expect("checkpoint draft");
    assert_ne!(older_bytes, checkpoint_bytes);
    PRESENT_RECOVERY.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("recovery service")
            .checkpoint("loom.present/1", checkpoint_bytes.clone())
            .expect("publish newer checkpoint");
    });
    assert_eq!(
        recovery.reopen_latest().as_deref(),
        Some(checkpoint_bytes.as_slice())
    );

    recovery_deferred::invalidate();
    started.release();
    run_production_completion(harness.receive_delivery());

    assert_eq!(
        recovery.reopen_latest().as_deref(),
        Some(checkpoint_bytes.as_slice()),
        "the old held serializer must not append after invalidation"
    );
    assert!(!recovery_deferred::is_stale());
}

#[test]
fn rejected_event_loop_dispatch_keeps_recovery_pending_and_retryable() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);
    edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    started.release();
    let delivery = harness.receive_delivery();
    assert!(
        delivery.invoke_error().is_some(),
        "the test platform must exercise event-loop rejection"
    );
    delivery.run_dispatch_failure_for_test();

    assert!(
        recovery_deferred::is_stale(),
        "a rejected event-loop dispatch must leave the latest draft pending"
    );
    set_status(&app, "Presentation edited");
    assert!(
        accessible_label_containing(&app, "event loop rejected recovery delivery").is_some(),
        "the dispatch failure reason must remain in the notice after a status update"
    );

    app.invoke_recovery_flush();
    assert_eq!(
        harness.launch_requests(),
        2,
        "the retained pending draft must be eligible for a retry"
    );
    assert_eq!(
        recovery.reopen_latest().as_deref(),
        Some(baseline_bytes.as_slice()),
        "the failed dispatch must not claim to have appended the draft"
    );
}

#[test]
fn missing_recovery_service_is_reported_as_a_write_failure() {
    let mut harness = Harness::install();
    let (app, state) = app_with_session(sample_session());
    let mut recovery = RecoveryScratch::without_service();
    edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let started_id = started.id;
    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, started_id);
    run_production_completion(delivery);

    assert!(
        recovery_deferred::is_stale(),
        "a missing service must not consume the pending latest draft"
    );
    assert!(accessible_label_containing(&app, "recovery service is unavailable").is_some());
    set_status(&app, "Presentation edited");
    assert!(accessible_label_containing(&app, "recovery service is unavailable").is_some());
    refresh(&app, &state);
    assert!(accessible_label_containing(&app, "recovery service is unavailable").is_some());
    assert!(recovery.reopen_latest().is_none());
}

#[test]
fn deferred_writes_are_single_flight_and_recover_the_newest_edit() {
    let mut harness = Harness::install();
    let (app, state) = app_with_session(sample_session());
    let original = state.session.borrow().clone();
    let original_bytes = recovery_draft::payload(&original, None).expect("original draft");
    let mut recovery = RecoveryScratch::with_service(&original_bytes);
    let first = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let first_started = harness.receive_started();
    let first_id = first_started.id;
    let second = edit_by_adding_slide(&app, &state);
    app.invoke_recovery_flush();
    let launches_while_first_is_serializing = harness.launch_requests();

    if launches_while_first_is_serializing > 1 {
        let second_started = harness.receive_started();
        let second_id = second_started.id;
        second_started.release();
        let second_delivery = harness.receive_delivery();
        assert_eq!(second_delivery.id, second_id);

        first_started.release();
        let first_delivery = harness.receive_delivery();
        assert_eq!(first_delivery.id, first_id);

        // The current implementation permits the newer worker to append first
        // and the older worker to overwrite it afterward.
        run_production_completion(second_delivery);
        run_production_completion(first_delivery);
    } else {
        first_started.release();
        let first_delivery = harness.receive_delivery();
        assert_eq!(first_delivery.id, first_id);
        run_production_completion(first_delivery);

        let second_started = harness.receive_started();
        let second_id = second_started.id;
        second_started.release();
        let second_delivery = harness.receive_delivery();
        assert_eq!(second_delivery.id, second_id);
        run_production_completion(second_delivery);
    }

    let max_active_workers = harness.max_active_workers();
    let latest_payload_persisted = same_content(&persisted_session(&mut recovery), &second);
    assert!(
        launches_while_first_is_serializing == 1
            && max_active_workers == 1
            && latest_payload_persisted,
        "deferred write gate: launches={launches_while_first_is_serializing}, \
         max_active_workers={max_active_workers}, latest_payload_persisted={latest_payload_persisted}"
    );
    assert_ne!(
        recovery_draft::payload(&first, None).expect("first draft"),
        recovery_draft::payload(&second, None).expect("second draft"),
        "the fixture must distinguish older and newer edits"
    );
}

#[test]
fn a_late_completion_after_accepted_discard_cannot_restore_the_discarded_deck() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let mut recovery = RecoveryScratch::with_service(&prior_source_less_recovery_payload());
    assert_prior_source_less_draft_is_restorable(&mut recovery);
    let (app, state) = app_with_session(baseline);
    let discarded = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let job_id = started.id;
    assert!(
        recovery_deferred::serialization_is_active_for_test(),
        "Discard is accepted while the old serializer is held"
    );

    app.invoke_new_deck();
    assert!(app.get_save_changes_open(), "the edit requires a decision");
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert_ne!(
        state.session.borrow().document.slides.len(),
        discarded.document.slides.len(),
        "Discard accepted a different visible deck"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "accepted New must clear the old draft before publishing the replacement"
    );

    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, job_id);
    run_production_completion(delivery);

    assert!(
        recovery.reopen_latest().is_none(),
        "an invalidated completion must not restore the previous recovery payload"
    );

    let replacement_started = harness.receive_started();
    assert_ne!(replacement_started.id, job_id);
    replacement_started.release();
    run_production_completion(harness.receive_delivery());

    let replacement = recovery.reopen_latest().expect("New deferred snapshot");
    let (draft, replacement_session) =
        recovery_draft::read(&replacement).expect("read New deferred snapshot");
    assert_eq!(draft.source, None, "New remains an unsaved presentation");
    assert_eq!(
        replacement_session.document.title, "Untitled Presentation",
        "the post-Discard snapshot represents the accepted New deck"
    );
}

#[test]
fn a_late_completion_after_sample_replacement_cannot_restore_the_old_deck() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let mut recovery = RecoveryScratch::with_service(&prior_source_less_recovery_payload());
    assert_prior_source_less_draft_is_restorable(&mut recovery);
    let (app, state) = app_with_session(baseline.clone());
    let discarded = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let started_id = started.id;
    app.invoke_new_sample_deck();
    assert!(app.get_save_changes_open(), "sample replacement asks first");
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert!(
        same_content(&state.session.borrow(), &baseline),
        "Discard commits the sample deck"
    );
    assert_ne!(
        state.session.borrow().document.slides.len(),
        discarded.document.slides.len(),
        "the sample replacement differs from the discarded draft"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "accepted Sample must clear the old draft before publishing the replacement"
    );

    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, started_id);
    run_production_completion(delivery);

    assert!(
        recovery.reopen_latest().is_none(),
        "an invalidated completion must not restore the previous recovery payload"
    );
}

#[test]
fn a_late_completion_after_template_replacement_cannot_restore_the_old_deck() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let mut recovery = RecoveryScratch::with_service(&prior_source_less_recovery_payload());
    assert_prior_source_less_draft_is_restorable(&mut recovery);
    let (app, state) = app_with_session(baseline);
    let discarded = edit_by_adding_slide(&app, &state);
    let template_choice = 1;
    let expected = slide_layouts::template_session(empty_session(), template_choice);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let started_id = started.id;
    app.invoke_create_theme(template_choice);
    assert!(
        app.get_save_changes_open(),
        "template replacement asks first"
    );
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert!(
        same_content(&state.session.borrow(), &expected),
        "Discard commits the selected template"
    );
    assert_ne!(
        state.session.borrow().document.slides.len(),
        discarded.document.slides.len(),
        "the template replacement differs from the discarded draft"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "accepted Template must clear the old draft before publishing the replacement"
    );

    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, started_id);
    run_production_completion(delivery);

    assert!(
        recovery.reopen_latest().is_none(),
        "an invalidated completion must not restore the previous recovery payload"
    );
}

#[test]
fn a_late_completion_after_accepted_open_cannot_restore_the_old_deck() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let mut recovery = RecoveryScratch::with_service(&prior_source_less_recovery_payload());
    assert_prior_source_less_draft_is_restorable(&mut recovery);
    let mut candidate = sample_session();
    candidate.document.title = "Opened candidate deck".into();
    candidate.document.active_index = 0;
    let fixture = DeckFileFixture::new(&candidate);
    let (app, state) = app_with_open_results(baseline, vec![Some(fixture.path.clone())]);
    let discarded = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let started_id = started.id;
    app.invoke_open_deck();
    assert!(
        app.get_save_changes_open(),
        "Open asks before replacing edits"
    );
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert_eq!(
        state.session.borrow().document.title,
        "Opened candidate deck",
        "the scripted valid deck was accepted"
    );
    assert_ne!(
        state.session.borrow().document.slides.len(),
        discarded.document.slides.len(),
        "the opened deck differs from the discarded draft"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "accepted ordinary Open must clear the old draft before publishing the candidate"
    );

    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, started_id);
    run_production_completion(delivery);

    assert!(
        recovery.reopen_latest().is_none(),
        "an invalidated completion must not restore the previous recovery payload"
    );
}

#[test]
fn warning_free_pptx_open_clears_the_previous_recovery_draft() {
    let mut harness = Harness::install();
    let mut baseline = sample_session();
    // The shared sample deck includes a transition that routes Open through the warning flow.
    baseline.transitions.clear();
    let mut recovery = RecoveryScratch::with_service(&prior_source_less_recovery_payload());
    assert_prior_source_less_draft_is_restorable(&mut recovery);
    let fixture = DeckFileFixture::new_pptx(&baseline);
    let (app, state) = app_with_open_results(baseline, vec![Some(fixture.path.clone())]);
    edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    app.invoke_open_deck();
    assert!(
        app.get_save_changes_open(),
        "Open asks before replacing edits"
    );
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(fixture.path.as_path()),
        "the warning-free PPTX candidate was accepted"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "warning-free PPTX Open must clear the old draft before publishing the candidate"
    );

    started.release();
    run_production_completion(harness.receive_delivery());
    assert!(
        recovery.reopen_latest().is_none(),
        "an invalidated completion must not restore the previous recovery payload"
    );
}

#[test]
fn failed_recovery_clear_keeps_new_retryable_and_preserves_the_current_draft() {
    let mut recovery = RecoveryScratch::with_service(&prior_source_less_recovery_payload());
    assert_prior_source_less_draft_is_restorable(&mut recovery);
    let baseline = sample_session();
    let (app, state) = app_with_session(baseline.clone());

    let clear_failure = recovery_replacement::inject_recovery_clear_failure_for_test(
        "injected recovery clear failure",
    );
    app.invoke_new_deck();

    assert!(
        same_content(&state.session.borrow(), &baseline),
        "a failed recovery clear must leave the current presentation visible"
    );
    assert!(
        app.get_status_left()
            .to_string()
            .contains("injected recovery clear failure"),
        "the reset error must be shown so the user can retry"
    );
    assert!(
        ensure_recovery_service().is_ok(),
        "the live recovery service must be re-opened after clear failure"
    );
    let bytes = recovery
        .reopen_latest()
        .expect("the visible presentation remains recoverable");
    let (draft, recovered) = recovery_draft::read(&bytes).expect("reopened draft decodes");
    assert_eq!(draft.source, None);
    assert!(same_content(&recovered, &baseline));

    drop(clear_failure);
    app.invoke_new_deck();
    assert_eq!(
        state.session.borrow().document.title,
        "Untitled Presentation",
        "reissuing New retries the accepted replacement"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "the retry must run the real clear before publishing the new deck"
    );
}

#[test]
fn failed_clear_reopens_the_latest_dirty_visible_deck_before_retry() {
    let _harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);
    let visible = edit_by_adding_slide(&app, &state);

    app.invoke_new_deck();
    assert!(
        app.get_save_changes_open(),
        "New requires a discard decision"
    );
    let clear_failure = recovery_replacement::inject_recovery_clear_failure_for_test(
        "injected recovery clear failure",
    );
    app.invoke_save_changes_discard();

    assert!(
        same_content(&state.session.borrow(), &visible),
        "failed clear leaves the current visible deck unchanged"
    );
    assert!(
        ensure_recovery_service().is_ok(),
        "failed clear reopens the recovery service for the visible deck"
    );
    let bytes = recovery
        .reopen_latest()
        .expect("the previously seeded recovery draft remains available");
    let (_, restored) = recovery_draft::read(&bytes).expect("reopened draft decodes");
    assert!(
        same_content(&restored, &visible),
        "after a failed clear, recovery must contain the latest visible dirty deck before retry"
    );
    drop(clear_failure);
}

#[test]
fn failed_reset_and_visible_checkpoint_keep_the_combined_error_retryable() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);
    let visible = edit_by_adding_slide(&app, &state);

    app.invoke_new_deck();
    assert!(app.get_save_changes_open());
    let clear_failure = recovery_replacement::inject_recovery_clear_failure_for_test(
        "injected recovery clear failure",
    );
    let checkpoint_fault = CheckpointPayloadFault::once(recovery.directory());
    app.invoke_save_changes_discard();

    assert_eq!(
        checkpoint_fault.hits(),
        1,
        "visible-draft checkpoint failed"
    );
    assert!(
        same_content(&state.session.borrow(), &visible),
        "both recovery failures leave the visible deck unchanged"
    );
    assert!(
        !app.get_status_left()
            .to_string()
            .contains("Created unsaved presentation"),
        "a failed reset cannot publish the requested New deck"
    );

    refresh(&app, &state);
    let notice = accessible_label_containing(&app, "CheckpointPayload")
        .expect("the combined clear and preservation failures remain in the recovery notice");
    assert!(notice.contains("injected recovery clear failure"));
    assert!(notice.contains("Use Retry or Save As to preserve it, then reissue the replacement."));
    assert!(recovery_deferred::is_stale());
    assert!(
        recovery_deferred::edit_timer_is_armed_for_test(),
        "refresh keeps the current dirty deck queued for the normal recovery debounce"
    );
    assert!(ElementHandle::find_by_accessible_label(&app, "Retry")
        .next()
        .is_some());

    drop(clear_failure);
    drop(checkpoint_fault);
    click_accessible(&app, "Retry");
    let started = harness.receive_started();
    started.release();
    run_production_completion(harness.receive_delivery());

    assert!(!recovery_deferred::is_stale());
    let bytes = recovery
        .reopen_latest()
        .expect("Retry writes the current visible draft");
    let (_, restored) = recovery_draft::read(&bytes).expect("retry draft decodes");
    assert!(same_content(&restored, &visible));

    app.invoke_new_deck();
    assert!(app.get_save_changes_open());
    app.invoke_save_changes_discard();
    assert_eq!(
        state.session.borrow().document.title,
        "Untitled Presentation"
    );
    assert!(
        recovery.reopen_latest().is_none(),
        "reissuing New clears the recovered old session before publishing"
    );
}

#[test]
fn explicit_retry_reports_latest_reopen_error_and_keeps_retry_actionable() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);
    let visible = edit_by_adding_slide(&app, &state);

    app.invoke_new_deck();
    assert!(app.get_save_changes_open());
    let reopen_failure = recovery_replacement::inject_recovery_reopen_failure_for_test(
        "injected recovery reopen failure",
    );
    app.invoke_save_changes_discard();

    assert!(
        same_content(&state.session.borrow(), &visible),
        "a failed reopen must leave the current presentation visible"
    );
    assert!(
        PRESENT_RECOVERY.with(|slot| slot.borrow().is_none()),
        "clear succeeds before the injected reopen failure, leaving no live service"
    );
    let empty_store = loom_production::snapshot::SnapshotRecovery::open_at(recovery.directory())
        .expect("independently open the cleared recovery store");
    assert!(
        empty_store.restored_payload().is_none(),
        "the successful clear leaves the on-disk recovery store empty"
    );
    drop(empty_store);

    drop(reopen_failure);
    accessible_label_containing(&app, "injected recovery reopen failure")
        .expect("the initial reopen failure remains visible");
    assert!(recovery_deferred::retry_is_armed_for_test());

    for attempt in 1..=2 {
        let latest_error = format!("injected retry reopen failure {attempt}");
        let retry_reopen_failure =
            recovery_replacement::inject_recovery_reopen_failure_for_test(latest_error.clone());
        click_accessible(&app, "Retry");

        assert_eq!(
            harness.launch_requests(),
            0,
            "a failed reconnect must not serialize as if recovery were available"
        );
        assert!(recovery_deferred::is_stale());
        assert!(PRESENT_RECOVERY.with(|slot| slot.borrow().is_none()));
        let retry_store =
            loom_production::snapshot::SnapshotRecovery::open_at(recovery.directory())
                .expect("independently open the recovery store after failed retry");
        assert!(
            retry_store.restored_payload().is_none(),
            "failed Retry must leave the cleared recovery store empty"
        );
        drop(retry_store);
        let latest_notice = accessible_label_containing(&app, &latest_error)
            .expect("the recovery notice reports the latest reconnect failure");
        assert!(
            latest_notice.contains("recovery service could not be reopened"),
            "the latest reconnect cause stays visible in context: {latest_notice}"
        );
        assert!(
            app.get_status_left()
                .to_string()
                .contains("Recovery pending"),
            "the status continues to show that recovery is pending"
        );
        assert!(ElementHandle::find_by_accessible_label(&app, "Retry")
            .next()
            .is_some());
        assert!(ElementHandle::find_by_accessible_label(&app, "Save As")
            .next()
            .is_some());
        assert!(!recovery_deferred::retry_is_armed_for_test());
        drop(retry_reopen_failure);
    }

    click_accessible(&app, "Retry");
    assert_eq!(harness.launch_requests(), 1);
    let started = harness.receive_started();
    started.release();
    run_production_completion(harness.receive_delivery());

    let bytes = recovery
        .reopen_latest()
        .expect("Retry must persist the visible deck after reopening recovery");
    let (_, restored) = recovery_draft::read(&bytes).expect("reopened draft decodes");
    assert!(same_content(&restored, &visible));
    assert!(
        !recovery_deferred::is_stale(),
        "Retry must clear the pending recovery error only after the write succeeds"
    );
    let status = app.get_status_left().to_string();
    assert!(
        !status.contains("The current presentation could not be saved to recovery"),
        "successful Retry must clear the obsolete preservation failure: {status}"
    );
    assert!(
        status.contains("Could not reset recovery data"),
        "successful Retry must retain the truthful replacement-reset failure: {status}"
    );
}

#[test]
fn automatic_retry_reopens_recovery_service_after_clear_succeeded_but_reopen_failed() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_session(baseline);
    let visible = edit_by_adding_slide(&app, &state);

    app.invoke_new_deck();
    assert!(app.get_save_changes_open());
    let reopen_failure = recovery_replacement::inject_recovery_reopen_failure_for_test(
        "injected recovery reopen failure",
    );
    app.invoke_save_changes_discard();

    assert!(same_content(&state.session.borrow(), &visible));
    assert!(PRESENT_RECOVERY.with(|slot| slot.borrow().is_none()));
    assert!(recovery_deferred::is_stale());
    assert!(recovery_deferred::retry_is_armed_for_test());
    let empty_store = loom_production::snapshot::SnapshotRecovery::open_at(recovery.directory())
        .expect("independently open the cleared recovery store");
    assert!(empty_store.restored_payload().is_none());
    drop(empty_store);
    assert!(PRESENT_RECOVERY.with(|slot| slot.borrow().is_none()));

    drop(reopen_failure);
    recovery_deferred::fire_retry_callback_for_test(&app);
    assert!(!recovery_deferred::retry_is_armed_for_test());
    assert_eq!(harness.launch_requests(), 1);
    let started = harness.receive_started();
    started.release();
    run_production_completion(harness.receive_delivery());

    let bytes = recovery
        .reopen_latest()
        .expect("the bounded automatic retry persists the visible deck");
    let (_, restored) = recovery_draft::read(&bytes).expect("reopened draft decodes");
    assert!(same_content(&restored, &visible));
    assert!(!recovery_deferred::is_stale());
    assert!(!recovery_deferred::retry_is_armed_for_test());
}

fn assert_noncommitting_open_preserves_in_flight_draft(
    open_result: Option<PathBuf>,
    expected_status: &str,
) {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let (app, state) = app_with_open_results(baseline, vec![open_result]);
    let latest = edit_by_adding_slide(&app, &state);
    let latest_bytes = recovery_draft::payload(&latest, None).expect("latest draft");

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    let started_id = started.id;
    app.invoke_open_deck();
    assert!(
        app.get_save_changes_open(),
        "Open asks before replacing edits"
    );
    app.invoke_save_changes_discard();
    assert!(!app.get_save_changes_open());
    assert!(
        same_content(&state.session.borrow(), &latest),
        "a cancelled or failed Open leaves the current deck visible"
    );
    assert!(app.get_status_left().to_string().contains(expected_status));

    started.release();
    let delivery = harness.receive_delivery();
    assert_eq!(delivery.id, started_id);
    run_production_completion(delivery);

    assert_eq!(
        recovery.reopen_latest().as_deref(),
        Some(latest_bytes.as_slice()),
        "a noncommitted Open must not invalidate the current in-flight draft"
    );
}

#[test]
fn cancelling_open_preserves_the_current_in_flight_draft() {
    assert_noncommitting_open_preserves_in_flight_draft(None, "Open cancelled");
}

#[test]
fn failed_open_preserves_the_current_in_flight_draft() {
    let missing_path = std::env::temp_dir()
        .join(format!(
            "loom-present-deferred-missing-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
        ))
        .join("missing.loomdeck");
    assert_noncommitting_open_preserves_in_flight_draft(Some(missing_path), "Open failed:");
}

#[test]
fn a_dropped_apps_completion_cannot_write_into_the_next_apps_recovery_store() {
    let mut harness = Harness::install();
    let baseline_a = sample_session();
    let payload_a = recovery_draft::payload(&baseline_a, None).expect("first baseline");
    let mut recovery = RecoveryScratch::with_service(&payload_a);
    let (app_a, state_a) = app_with_session(baseline_a);
    edit_by_adding_slide(&app_a, &state_a);
    app_a.invoke_recovery_flush();
    let started_a = harness.receive_started();
    started_a.release();
    let completion_a = harness.receive_delivery();
    end_session_recovery();
    drop(state_a);
    drop(app_a);

    let mut baseline_b = sample_session();
    baseline_b.document.title = "Next Present instance".into();
    let payload_b = recovery_draft::payload(&baseline_b, None).expect("second baseline");
    recovery.install_service(&payload_b);
    let (app_b, state_b) = app_with_session(baseline_b);
    let first_b = edit_by_adding_slide(&app_b, &state_b);
    app_b.invoke_recovery_flush();
    let started_b = harness.receive_started();
    let started_b_id = started_b.id;
    let newest_b = edit_by_adding_slide(&app_b, &state_b);
    app_b.invoke_recovery_flush();
    let launches_before_old_completion = harness.launch_requests();

    run_production_completion(completion_a);
    let launches_after_old_completion = harness.launch_requests();
    let store_after_old_completion = recovery.reopen_latest();

    if launches_before_old_completion > 2 {
        let second_b_started = harness.receive_started();
        let second_b_id = second_b_started.id;
        second_b_started.release();
        let second_b_delivery = harness.receive_delivery();
        assert_eq!(second_b_delivery.id, second_b_id);

        started_b.release();
        let first_b_delivery = harness.receive_delivery();
        assert_eq!(first_b_delivery.id, started_b_id);
        run_production_completion(first_b_delivery);
        run_production_completion(second_b_delivery);
    } else {
        started_b.release();
        let first_b_delivery = harness.receive_delivery();
        assert_eq!(first_b_delivery.id, started_b_id);
        run_production_completion(first_b_delivery);

        let second_b_started = harness.receive_started();
        let second_b_id = second_b_started.id;
        second_b_started.release();
        let second_b_delivery = harness.receive_delivery();
        assert_eq!(second_b_delivery.id, second_b_id);
        run_production_completion(second_b_delivery);
    }

    assert_eq!(
        store_after_old_completion.as_deref(),
        Some(payload_b.as_slice()),
        "the dropped app's completion must not target the newly installed recovery store"
    );
    let max_active_workers = harness.max_active_workers();
    let latest_payload_persisted = same_content(&persisted_session(&mut recovery), &newest_b);
    assert!(
        launches_before_old_completion == 2
            && launches_after_old_completion == 2
            && max_active_workers == 1
            && latest_payload_persisted,
        "app/store lifetime gate: launches_before_old={launches_before_old_completion}, \
         launches_after_old={launches_after_old_completion}, max_active_workers={max_active_workers}, \
         latest_payload_persisted={latest_payload_persisted}"
    );
    assert_ne!(
        recovery_draft::payload(&first_b, None).expect("first new-app draft"),
        recovery_draft::payload(&newest_b, None).expect("latest new-app draft")
    );
}

mod recovery_import_retry;
