use super::*;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const CHILD_MODE: &str = "LOOM_WRITER_RECOVERY_TEST_CHILD";
const OPEN_PATH: &str = "LOOM_WRITER_RECOVERY_TEST_OPEN_PATH";
const SAVE_PATH: &str = "LOOM_WRITER_RECOVERY_TEST_SAVE_PATH";
const CRASH_MARKER: &str = "UNSAVED_RECOVERY_SENTINEL_2026";

#[cfg(windows)]
const STATE_HOME_ENV: &str = "LOCALAPPDATA";
#[cfg(target_os = "macos")]
const STATE_HOME_ENV: &str = "HOME";
#[cfg(not(any(windows, target_os = "macos")))]
const STATE_HOME_ENV: &str = "XDG_STATE_HOME";

pub(super) fn isolated_root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "loom-writer-recovery-{}-{sequence}",
        std::process::id()
    ))
}

pub(super) fn run_child(
    test_name: &str,
    mode: &str,
    root: &Path,
    open: Option<&Path>,
    save: Option<&Path>,
) -> ExitStatus {
    let state_home = root.join("state");
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args(["--exact", test_name, "--nocapture"])
        .env(CHILD_MODE, mode)
        .env(STATE_HOME_ENV, &state_home);
    if let Some(open) = open {
        command.env(OPEN_PATH, open);
    }
    if let Some(save) = save {
        command.env(SAVE_PATH, save);
    }
    command.status().expect("run isolated Writer process")
}

pub(super) fn make_test_state(
    document: WriterDocument,
    save_path: Option<PathBuf>,
) -> (WriterApp, Rc<GuiState>) {
    set_platform();
    let app = WriterApp::new().expect("create Writer UI for recovery test");
    let mut registry = build_writer_registry();
    let history = EditorHistory::new();
    sync_writer_registry_enablement(&mut registry, &document, &history);
    let last_saved = match &save_path {
        Some(path) => load_file(path).unwrap_or_else(|_| document.clone()),
        None => WriterDocument::new("placeholder", "Placeholder"),
    };
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(last_saved),
        current: RefCell::new(document),
        viewport: RefCell::new(PageViewport::default()),
        pointer_anchor: Cell::new(None),
        pointer_active: Cell::new(false),
        save_path: RefCell::new(save_path),
        compatibility_save: Cell::new(false),
        history: RefCell::new(history),
        history_clock: Cell::new(Instant::now()),
        syncing_editor: Cell::new(false),
        pending_replacement: Cell::new(None),
        dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        document_filter: FileFilter::new("Writer", ["loomdoc"]).expect("document filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("PDF filter"),
        registry: Arc::new(Mutex::new(registry)),
        menu_service: RefCell::new(None),
    });
    (app, state)
}

pub(super) fn child_mode() -> Option<String> {
    std::env::var(CHILD_MODE).ok()
}

fn saved_fixture(root: &Path) -> PathBuf {
    let path = root.join("saved-test.docx");
    let mut document = WriterDocument::new("recovery-fixture", "Recovery fixture");
    document.replace_paragraphs("Saved before the test crash.");
    let bytes = loom_writer_core::export_docx(&document)
        .expect("export saved recovery fixture")
        .bytes;
    std::fs::write(&path, bytes).expect("write saved fixture");
    path
}

const RESTART_TEST: &str =
    "recovery_tests::opened_session_recovers_unsaved_edit_after_process_restart";

#[test]
fn opened_session_recovers_unsaved_edit_after_process_restart() {
    match child_mode().as_deref() {
        Some("record") => {
            let open_path = PathBuf::from(std::env::var_os(OPEN_PATH).expect("fixture path"));
            assert!(recovery::initialize_editing_session(Some(&open_path))
                .expect("initialize recovery for --open")
                .draft
                .is_none());
            let mut document = load_file(&open_path).expect("open saved fixture");
            let edited = format!("{}\n{CRASH_MARKER}", document.editor_text());
            document
                .replace_editor_text(&edited)
                .expect("apply simulated user edit");
            let (app, state) = make_test_state(document, Some(open_path.clone()));
            apply_state(&app, &state);
            // Exit without Rust destructors, like the app being killed before Save.
            std::process::exit(0);
        }
        Some("restore") => {
            let document = recovery::initialize_editing_session(None)
                .expect("initialize ordinary launch")
                .draft
                .expect("recover the unsaved document")
                .document;
            assert!(document.editor_text().contains(CRASH_MARKER));
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let open_path = saved_fixture(&root);
    let saved_bytes = std::fs::read(&open_path).expect("read original saved fixture");
    assert!(run_child(RESTART_TEST, "record", &root, Some(&open_path), None).success());
    assert_eq!(std::fs::read(&open_path).unwrap(), saved_bytes);
    assert!(run_child(RESTART_TEST, "restore", &root, None, None).success());
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

#[cfg(unix)]
const WRITE_FAILURE_TEST: &str = "recovery_tests::interactive_recovery_write_failures_are_visible";

#[cfg(unix)]
#[test]
fn interactive_recovery_write_failures_are_visible() {
    match child_mode().as_deref() {
        Some("fail-write") => {
            recovery::initialize_editing_session(Some(Path::new("unused.loomdoc")))
                .expect("initialize recovery");
            let directory =
                loom_production::snapshot::application_state_directory("org.loom.writer")
                    .expect("resolve recovery directory");
            let mut permissions = std::fs::metadata(&directory).unwrap().permissions();
            use std::os::unix::fs::PermissionsExt;
            let original_permissions = permissions.clone();
            permissions.set_mode(0o500);
            std::fs::set_permissions(&directory, permissions).unwrap();

            let document = WriterDocument::new("write-failure", "Write failure");
            let save_path = PathBuf::from(std::env::var_os(SAVE_PATH).expect("save path"));
            let (app, state) = make_test_state(document, Some(save_path));
            state
                .current
                .borrow_mut()
                .replace_paragraphs("An edit whose recovery write fails.");
            apply_state(&app, &state);
            assert_eq!(
                app.get_status_left(),
                "Recovery failed. Save a copy now; autosave may be out of date."
            );

            assert!(save_current_document(&app, &state, false).expect("save document"));
            assert_eq!(
                app.get_status_left(),
                "Saved. Recovery checkpoint failed; keep another copy."
            );

            let state_home = PathBuf::from(std::env::var_os(STATE_HOME_ENV).expect("state home"));
            let screenshot_path = state_home
                .parent()
                .expect("test root")
                .join("recovery-write-failure.png");
            let screenshot =
                snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render recovery error state");
            loom_test_support::png::save_png(&screenshot_path, &screenshot)
                .expect("save recovery error screenshot");

            std::fs::set_permissions(&directory, original_permissions).unwrap();
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let is_root = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false);
    if is_root {
        println!("Skipping chmod 0500 test when running as root (euid == 0); root ignores directory mode 0500");
        return;
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let open_path = root.join("opened.docx");
    let save_path = root.join("saved-after-error.docx");
    assert!(run_child(
        WRITE_FAILURE_TEST,
        "fail-write",
        &root,
        Some(&open_path),
        Some(&save_path)
    )
    .success());
    assert!(
        save_path.is_file(),
        "the recovery warning must not block an explicit save"
    );
    let screenshot_path = root.join("recovery-write-failure.png");
    assert!(
        screenshot_path.is_file(),
        "capture the visible recovery warning"
    );
    let inspected_screenshot = std::env::temp_dir().join("loom-writer-recovery-write-failure.png");
    std::fs::copy(&screenshot_path, &inspected_screenshot).expect("keep inspection screenshot");
    println!(
        "recovery failure screenshot: {}",
        inspected_screenshot.display()
    );
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

#[test]
fn interactive_recovery_write_failures_are_visible_via_injection() {
    recovery::INJECT_FAILURE.with(|f| f.set(true));

    let mut document = WriterDocument::new("write-failure-injected", "Write failure injected");
    document.replace_paragraphs("An edit whose recovery write fails.");
    let temp_save =
        std::env::temp_dir().join(format!("loom-writer-save-{}.docx", std::process::id()));
    let (app, state) = make_test_state(document, Some(temp_save.clone()));
    state
        .current
        .borrow_mut()
        .replace_paragraphs("A dirty edit whose recovery write fails.");
    apply_state(&app, &state);
    assert_eq!(
        app.get_status_left(),
        "Recovery failed. Save a copy now; autosave may be out of date."
    );

    assert!(save_current_document(&app, &state, false).expect("save document"));
    assert_eq!(
        app.get_status_left(),
        "Saved. Recovery checkpoint failed; keep another copy."
    );

    recovery::INJECT_FAILURE.with(|f| f.set(false));
    let _ = std::fs::remove_file(temp_save);
}

const DEFERRED_ORDER_TEST: &str =
    "recovery_tests::deferred_recovery_drops_obsolete_serialization_before_delivery";

#[test]
fn deferred_recovery_drops_obsolete_serialization_before_delivery() {
    match child_mode().as_deref() {
        Some("deferred-order") => {
            set_platform();
            assert!(recovery::initialize_editing_session(None)
                .expect("initialize isolated deferred recovery")
                .draft
                .is_none());
            let app = WriterApp::new().expect("create Writer UI for deferred recovery test");
            let harness = recovery::DeferredWriteTestHarness::install();

            let mut older = WriterDocument::new("deferred-order", "Deferred order");
            older.replace_paragraphs("older snapshot");
            recovery::DEFERRED.with(|state| {
                state.note_edit();
                assert!(state.fire());
            });
            assert_eq!(
                recovery::record_document_deferred(&app, &older, None)
                    .expect("start first real serializer"),
                recovery::DeferredDispatch::Started
            );
            assert_eq!(
                harness
                    .started
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("first serializer starts"),
                0
            );

            let mut latest = WriterDocument::new("deferred-order", "Deferred order");
            latest.replace_paragraphs("newest snapshot");
            recovery::DEFERRED.with(|state| {
                state.note_edit();
                assert!(state.fire());
            });
            let latest_dispatch = recovery::record_document_deferred(&app, &latest, None)
                .expect("submit newer recovery snapshot");
            if latest_dispatch == recovery::DeferredDispatch::Started {
                assert_eq!(
                    harness
                        .started
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .expect("current code starts the newer serializer concurrently"),
                    1
                );
                let newer_completion = harness
                    .completed
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("newer serializer completes before the delayed first one");
                newer_completion();
            } else {
                assert!(matches!(
                    harness.started.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Empty)
                ));
            }

            harness.release_first();
            let older_completion = harness
                .completed
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("delayed first serializer completes");
            older_completion();
            if latest_dispatch == recovery::DeferredDispatch::Queued {
                assert_eq!(
                    harness
                        .started
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .expect("the newest pending serializer starts after the first completes"),
                    1
                );
                let latest_completion = harness
                    .completed
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("newest pending serializer completes");
                latest_completion();
            }

            let reopened = recovery::initialize_editing_session(None)
                .expect("reopen persisted deferred recovery")
                .draft
                .expect("the latest draft is available after reopen");
            assert_eq!(
                (reopened.document.editor_text(), harness.max_in_flight(),),
                ("newest snapshot".to_owned(), 1),
                "recovery keeps the latest edit and runs at most one serializer at a time"
            );
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {
            let root = isolated_root();
            std::fs::create_dir_all(&root).expect("create isolated deferred recovery root");
            let status = run_child(DEFERRED_ORDER_TEST, "deferred-order", &root, None, None);
            let _ = std::fs::remove_dir_all(&root);
            assert!(status.success(), "deferred recovery child failed: {status}");
        }
    }
}

const DEFERRED_RETRY_TEST: &str = "recovery_tests::deferred_recovery_retry_clears_failure_status";

#[test]
fn deferred_recovery_retry_clears_failure_status() {
    match child_mode().as_deref() {
        Some("deferred-retry") => {
            set_platform();
            assert!(recovery::initialize_editing_session(None)
                .expect("initialize isolated deferred recovery")
                .draft
                .is_none());
            let app = WriterApp::new().expect("create Writer UI for deferred retry test");
            let harness = recovery::DeferredWriteTestHarness::install();
            let mut document = WriterDocument::new("deferred-retry", "Deferred retry");
            document.replace_paragraphs("recovery retry snapshot");

            recovery::DEFERRED.with(|state| {
                state.note_edit();
                assert!(state.fire());
            });
            let retry_epoch = recovery::DEFERRED.with(recovery::DeferredWrite::epoch);
            assert_eq!(
                recovery::record_document_deferred(&app, &document, None)
                    .expect("start deferred recovery write"),
                recovery::DeferredDispatch::Started
            );
            assert_eq!(
                harness
                    .started
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("first serializer starts"),
                0
            );
            harness.release_first();
            let failed_delivery = harness
                .completed
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("first serializer completes");
            recovery::INJECT_FAILURE.with(|failure| failure.set(true));
            failed_delivery();
            recovery::INJECT_FAILURE.with(|failure| failure.set(false));
            assert_eq!(
                app.get_status_left(),
                "Recovery failed. Save a copy now; autosave may be out of date."
            );
            assert!(recovery::DEFERRED.with(recovery::DeferredWrite::is_stale));

            assert!(recovery::DEFERRED.with(|state| state.fire()));
            assert_eq!(
                recovery::DEFERRED.with(recovery::DeferredWrite::epoch),
                retry_epoch,
                "retry the failed snapshot without a new edit"
            );
            assert_eq!(
                recovery::record_document_deferred(&app, &document, None)
                    .expect("retry deferred recovery write"),
                recovery::DeferredDispatch::Started
            );
            assert_eq!(
                harness
                    .started
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("retry serializer starts"),
                1
            );
            let successful_delivery = harness
                .completed
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("retry serializer completes");
            successful_delivery();

            assert_eq!(
                recovery::DEFERRED.with(recovery::DeferredWrite::epoch),
                retry_epoch
            );
            assert!(!recovery::DEFERRED.with(recovery::DeferredWrite::is_stale));
            assert_eq!(
                app.get_status_left(),
                "",
                "a successful same-epoch retry clears the stale recovery warning"
            );
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {
            let root = isolated_root();
            std::fs::create_dir_all(&root).expect("create isolated deferred retry root");
            let status = run_child(DEFERRED_RETRY_TEST, "deferred-retry", &root, None, None);
            let _ = std::fs::remove_dir_all(&root);
            assert!(
                status.success(),
                "deferred recovery retry child failed: {status}"
            );
        }
    }
}

const INITIALIZATION_FAILURE_TEST: &str =
    "recovery_tests::initialization_failure_is_visible_on_startup";

#[test]
fn initialization_failure_is_visible_on_startup() {
    match child_mode().as_deref() {
        Some("initialization-failure") => {
            let state_home = PathBuf::from(std::env::var_os(STATE_HOME_ENV).expect("state home"));
            std::fs::write(&state_home, b"block recovery directory creation")
                .expect("block recovery directory creation");
            set_platform();
            let app = WriterApp::new().expect("create Writer UI for startup failure");
            assert!(initialize_recovery_for_gui(&app, None).is_none());
            assert_eq!(
                app.get_status_left(),
                "Recovery unavailable. Save regularly; crash recovery may be out of date."
            );

            let screenshot_path = state_home
                .parent()
                .expect("test root")
                .join("recovery-initialization-failure.png");
            let screenshot = snapshot_component(&app, 1280.0, 800.0, 1.0)
                .expect("render recovery initialization warning");
            loom_test_support::png::save_png(&screenshot_path, &screenshot)
                .expect("save recovery initialization screenshot");
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    assert!(run_child(
        INITIALIZATION_FAILURE_TEST,
        "initialization-failure",
        &root,
        None,
        None
    )
    .success());
    let screenshot_path = root.join("recovery-initialization-failure.png");
    assert!(
        screenshot_path.is_file(),
        "capture the startup recovery warning"
    );
    let inspected_screenshot = std::env::temp_dir().join("loom-writer-recovery-init-failure.png");
    std::fs::copy(&screenshot_path, &inspected_screenshot).expect("keep inspection screenshot");
    println!(
        "recovery initialization screenshot: {}",
        inspected_screenshot.display()
    );
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

const INVALID_OPEN_TEST: &str =
    "recovery_tests::invalid_command_line_open_keeps_startup_errors_clear";

#[test]
fn invalid_command_line_open_keeps_startup_errors_clear() {
    match child_mode().as_deref() {
        Some("invalid-open") => {
            set_platform();
            let open_path = PathBuf::from(std::env::var_os(OPEN_PATH).expect("invalid path"));
            let args = Args {
                screenshot: None,
                smoke: false,
                palette: false,
                journey: None,
                size: DEFAULT_SIZE,
                position: None,
                theme: "light".into(),
                theme_explicit: false,
                rtl: false,
                text_scale: 1.0,
                scale_factor: 1.0,
                open: Some(open_path.to_string_lossy().into_owned()),
                template: None,
                template_chooser: false,
                inspector: false,
                document_tab: false,
                navigator: false,
                comment: None,
                table: false,
                sample: false,
            };
            let error = run_gui_with_dialogs(
                &args,
                Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
            )
            .expect_err("a missing command-line document must fail");
            assert!(!error.is_empty());
            let recovery_directory =
                loom_production::snapshot::application_state_directory("org.loom.writer")
                    .expect("resolve recovery directory");
            assert!(
                recovery_directory.is_dir(),
                "recovery initializes before the file is opened"
            );
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let invalid_path = root.join("does-not-exist.loomdoc");
    assert!(run_child(
        INVALID_OPEN_TEST,
        "invalid-open",
        &root,
        Some(&invalid_path),
        None
    )
    .success());
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

#[test]
fn an_intentional_close_clears_recovery_but_a_crash_does_not() {
    use std::thread;

    let root = isolated_root();
    let previous = std::env::var_os(STATE_HOME_ENV);
    std::env::set_var(STATE_HOME_ENV, root.join("state"));

    let session = |work: fn() -> Option<String>| thread::spawn(work).join().expect("session ran");
    fn first_text(document: &WriterDocument) -> String {
        document
            .blocks
            .first()
            .map(|b| b.text.as_str().to_string())
            .unwrap_or_default()
    }

    // Session 1 types a draft and then dies without closing: the slot is simply
    // dropped, as in a crash.
    let first = session(|| {
        assert!(recovery::initialize_editing_session(None)
            .expect("start")
            .draft
            .is_none());
        let mut draft = WriterDocument::new("draft", "Draft");
        draft.replace_paragraphs("typed before the crash");
        recovery::record_document(&draft, None).expect("record");
        None
    });
    assert!(first.is_none());

    // Session 2 is offered the draft, then the user closes deliberately.
    let second = session(|| {
        let restored = recovery::initialize_editing_session(None).expect("restart");
        recovery::discard_document_recovery().expect("clear on close");
        restored.draft.map(|draft| first_text(&draft.document))
    });
    assert_eq!(
        second.as_deref(),
        Some("typed before the crash"),
        "a crash keeps the draft"
    );

    // Session 3 starts clean: the discarded draft does not come back.
    let third = session(|| {
        recovery::initialize_editing_session(None)
            .expect("start again")
            .draft
            .map(|draft| first_text(&draft.document))
    });
    assert_eq!(
        third, None,
        "an intentional close must not resurrect the draft"
    );

    match previous {
        Some(value) => std::env::set_var(STATE_HOME_ENV, value),
        None => std::env::remove_var(STATE_HOME_ENV),
    }
    let _ = std::fs::remove_dir_all(root);
}

fn path_from_env(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is set")))
}

/// Another saved file, written beside the fixture the child edits.
fn named_fixture(root: &Path, name: &str, text: &str) -> PathBuf {
    let path = root.join(name);
    let mut document = WriterDocument::new(name, name);
    document.replace_paragraphs(text);
    std::fs::write(
        &path,
        loom_writer_core::save_document(&document).expect("serialize fixture"),
    )
    .expect("write fixture");
    path
}

/// Type into the file at `open_path` and die without saving, as a crash does.
fn record_unsaved_edit_for(open_path: &Path) -> ! {
    let startup = recovery::initialize_editing_session(Some(open_path))
        .expect("initialize recovery for --open");
    assert!(
        startup.draft.is_none(),
        "no draft exists before the first edit"
    );
    let mut document = load_file(open_path).expect("open saved fixture");
    let edited = format!("{}\n{CRASH_MARKER}", document.editor_text());
    document
        .replace_editor_text(&edited)
        .expect("apply simulated user edit");
    let (app, state) = make_test_state(document, Some(open_path.to_path_buf()));
    apply_state(&app, &state);
    // Exit without Rust destructors, like the app being killed before Save.
    std::process::exit(0);
}

const SAME_FILE_TEST: &str =
    "recovery_tests::a_draft_for_the_opened_file_restores_as_unsaved_and_saves_back";

/// Double-clicking the file it was typed into must not discard the typing.
#[test]
fn a_draft_for_the_opened_file_restores_as_unsaved_and_saves_back() {
    match child_mode().as_deref() {
        Some("edit") => record_unsaved_edit_for(&path_from_env(OPEN_PATH)),
        Some("relaunch") => {
            let open_path = path_from_env(OPEN_PATH);
            let (app, state) =
                make_test_state(WriterDocument::new("placeholder", "Placeholder"), None);
            let startup =
                initialize_recovery_for_gui(&app, Some(&open_path)).expect("initialize recovery");
            assert_eq!(
                app.get_status_left().as_str(),
                "Restored unsaved changes to saved-test"
            );
            let draft = startup.draft.expect("the draft for this file is restored");
            assert_eq!(draft.source.as_deref(), Some(open_path.as_path()));
            assert!(draft.document.editor_text().contains(CRASH_MARKER));
            let open_text = open_path.to_string_lossy().into_owned();
            let (document, saved) =
                startup_documents(Some(draft), Some(&open_text), None).expect("startup documents");
            assert!(
                !document_content_equal(&document, &saved),
                "a restored draft reads as unsaved"
            );
            *state.current.borrow_mut() = document;
            *state.last_saved.borrow_mut() = saved;
            *state.save_path.borrow_mut() = Some(open_path.clone());
            apply_state(&app, &state);
            assert_eq!(
                app.get_window_title().as_str(),
                "saved-test * - Loom Writer"
            );
            assert!(save_current_document(&app, &state, false).expect("save restored draft"));
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let open_path = saved_fixture(&root);
    assert!(run_child(SAME_FILE_TEST, "edit", &root, Some(&open_path), None).success());
    assert!(run_child(SAME_FILE_TEST, "relaunch", &root, Some(&open_path), None).success());
    let on_disk = load_file(&open_path).expect("the saved file loads");
    assert!(
        on_disk.editor_text().contains(CRASH_MARKER),
        "Save writes the restored draft to the file it came from"
    );
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

const KEEP_DRAFT_TEST: &str =
    "recovery_tests::opening_another_file_keeps_the_draft_for_a_later_launch";

/// Opening a different file must set the draft aside, never overwrite it, and
/// offer it again later; a deliberate discard ends it for good.
#[test]
fn opening_another_file_keeps_the_draft_for_a_later_launch() {
    match child_mode().as_deref() {
        Some("edit") => record_unsaved_edit_for(&path_from_env(OPEN_PATH)),
        Some("open-other") => {
            let other = path_from_env(OPEN_PATH);
            let startup =
                recovery::initialize_editing_session(Some(&other)).expect("open another file");
            assert!(
                startup.draft.is_none(),
                "another file opens as itself, not with the draft"
            );
            std::process::exit(0);
        }
        Some("plain") => {
            let open_path = path_from_env(OPEN_PATH);
            let startup = recovery::initialize_editing_session(None).expect("ordinary launch");
            let draft = startup
                .draft
                .expect("the kept draft is offered on the next launch");
            assert!(draft.document.editor_text().contains(CRASH_MARKER));
            assert_eq!(draft.source.as_deref(), Some(open_path.as_path()));
            recovery::discard_document_recovery().expect("the user discards the draft");
            std::process::exit(0);
        }
        Some("plain-again") => {
            let startup = recovery::initialize_editing_session(None).expect("ordinary launch");
            assert!(
                startup.draft.is_none(),
                "a discarded draft does not come back"
            );
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let open_path = saved_fixture(&root);
    let other_path = named_fixture(&root, "other-file.loomdoc", "A different saved file.");
    assert!(run_child(KEEP_DRAFT_TEST, "edit", &root, Some(&open_path), None).success());
    assert!(run_child(
        KEEP_DRAFT_TEST,
        "open-other",
        &root,
        Some(&other_path),
        None
    )
    .success());
    assert!(run_child(KEEP_DRAFT_TEST, "plain", &root, Some(&open_path), None).success());
    assert!(run_child(KEEP_DRAFT_TEST, "plain-again", &root, None, None).success());
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

const LEGACY_DRAFT_TEST: &str =
    "recovery_tests::drafts_saved_before_paths_were_recorded_still_restore";

/// Drafts written by the previous format are bare packages with no file.
#[test]
fn drafts_saved_before_paths_were_recorded_still_restore() {
    match child_mode().as_deref() {
        Some("legacy") => {
            let mut document = WriterDocument::new("legacy", "Legacy");
            document.replace_paragraphs(&format!("Legacy draft {CRASH_MARKER}"));
            let package =
                loom_writer_core::save_document(&document).expect("serialize legacy draft");
            let mut store = loom_production::snapshot::SnapshotRecovery::open("org.loom.writer")
                .expect("open recovery");
            store
                .record("writer state", package)
                .expect("record legacy draft");
            std::process::exit(0);
        }
        Some("plain") => {
            let startup = recovery::initialize_editing_session(None).expect("ordinary launch");
            let draft = startup.draft.expect("the legacy draft restores");
            assert!(draft.document.editor_text().contains(CRASH_MARKER));
            assert!(draft.source.is_none(), "a legacy draft has no file");
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    assert!(run_child(LEGACY_DRAFT_TEST, "legacy", &root, None, None).success());
    assert!(run_child(LEGACY_DRAFT_TEST, "plain", &root, None, None).success());
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}

/// A path survives the draft container exactly, including spaces, accents and
/// a newline; a bare package still decodes and has no file.
#[test]
fn a_draft_keeps_the_exact_path_it_was_edited_from() {
    let package = b"native package bytes".to_vec();
    let sources = [
        Some(PathBuf::from("/home/writer/Q3 plan (final) é.loomdoc")),
        Some(PathBuf::from("/tmp/odd\nname.loomdoc")),
        None,
    ];
    for source in sources {
        let payload = crate::recovery_draft::encode(source.as_deref(), &package);
        let (decoded, bytes) = crate::recovery_draft::decode(&payload).expect("payload decodes");
        assert_eq!(decoded, source);
        assert_eq!(bytes, package.as_slice());
    }
    let (decoded, bytes) =
        crate::recovery_draft::decode(&package).expect("a legacy payload decodes");
    assert_eq!(decoded, None);
    assert_eq!(bytes, package.as_slice());
}

const FOOTNOTE_RESTART_TEST: &str =
    "recovery_tests::footnotes_survive_a_fresh_recovery_session_after_restart";

#[test]
fn footnotes_survive_a_fresh_recovery_session_after_restart() {
    match child_mode().as_deref() {
        Some("record") => {
            let open_path = PathBuf::from(std::env::var_os(OPEN_PATH).expect("fixture path"));
            assert!(recovery::initialize_editing_session(Some(&open_path))
                .expect("initialize recovery for the source document")
                .draft
                .is_none());
            let mut document = load_file(&open_path).expect("load saved fixture");
            let block = document.blocks.first().expect("fixture paragraph");
            let block_id = block.id;
            let offset = block
                .text
                .as_str()
                .find("test")
                .expect("stable footnote anchor");
            document.footnotes.push(loom_writer_core::FootnoteEntry {
                id: 91_007,
                block_id,
                offset,
                body: "A footnote stored in the recovery draft.".into(),
            });
            let (app, state) = make_test_state(document, Some(open_path));
            apply_state(&app, &state);
            std::process::exit(0);
        }
        Some("restore") => {
            let draft = recovery::initialize_editing_session(None)
                .expect("open a fresh recovery session")
                .draft
                .expect("the unsaved draft survives process restart");
            let document = draft.document;
            let note = document
                .footnotes
                .iter()
                .find(|note| note.id == 91_007)
                .expect("stable note ID survives recovery");
            assert_eq!(note.body, "A footnote stored in the recovery draft.");
            assert_eq!(note.block_id, document.blocks[0].id);
            assert_eq!(
                note.offset,
                document.blocks[0].text.as_str().find("test").unwrap()
            );
            std::process::exit(0);
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {}
    }

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let open_path = saved_fixture(&root);
    let saved_bytes = std::fs::read(&open_path).expect("read saved fixture before recovery");
    assert!(run_child(
        FOOTNOTE_RESTART_TEST,
        "record",
        &root,
        Some(&open_path),
        None
    )
    .success());
    assert_eq!(
        std::fs::read(&open_path).expect("saved file remains unchanged"),
        saved_bytes
    );
    assert!(run_child(FOOTNOTE_RESTART_TEST, "restore", &root, None, None).success());
    std::fs::remove_dir_all(root).expect("remove isolated test data");
}
