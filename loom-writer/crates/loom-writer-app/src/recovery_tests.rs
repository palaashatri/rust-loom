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

fn isolated_root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "loom-writer-recovery-{}-{sequence}",
        std::process::id()
    ))
}

fn run_child(
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

fn make_test_state(
    document: WriterDocument,
    save_path: Option<PathBuf>,
) -> (WriterApp, Rc<GuiState>) {
    set_platform();
    let app = WriterApp::new().expect("create Writer UI for recovery test");
    let mut registry = build_writer_registry();
    let history = EditorHistory::new();
    sync_writer_registry_enablement(&mut registry, &document, &history);
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(document.clone()),
        current: RefCell::new(document),
        viewport: RefCell::new(PageViewport::default()),
        pointer_anchor: Cell::new(None),
        pointer_active: Cell::new(false),
        save_path: RefCell::new(save_path),
        history: RefCell::new(history),
        history_clock: Instant::now(),
        syncing_editor: Cell::new(false),
        pending_replacement: Cell::new(None),
        dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        document_filter: FileFilter::new("Writer", ["loomdoc"]).expect("document filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("PDF filter"),
        registry: Arc::new(Mutex::new(registry)),
    });
    (app, state)
}

fn child_mode() -> Option<String> {
    std::env::var(CHILD_MODE).ok()
}

fn saved_fixture(root: &Path) -> PathBuf {
    let path = root.join("saved-test.loomdoc");
    let mut document = WriterDocument::new("recovery-fixture", "Recovery fixture");
    document.replace_paragraphs("Saved before the test crash.");
    std::fs::write(
        &path,
        loom_writer_core::save_document(&document).expect("serialize saved fixture"),
    )
    .expect("write saved fixture");
    path
}

const RESTART_TEST: &str =
    "recovery_tests::opened_session_recovers_unsaved_edit_after_process_restart";

#[test]
fn opened_session_recovers_unsaved_edit_after_process_restart() {
    match child_mode().as_deref() {
        Some("record") => {
            let open_path = PathBuf::from(std::env::var_os(OPEN_PATH).expect("fixture path"));
            assert!(recovery::initialize_editing_session(true)
                .expect("initialize recovery for --open")
                .is_none());
            let mut document = load_file(&open_path).expect("open saved fixture");
            let edited = format!("{}\n{CRASH_MARKER}", document.editor_text());
            document
                .replace_editor_text(&edited)
                .expect("apply simulated user edit");
            let (app, state) = make_test_state(document, None);
            apply_state(&app, &state);
            // Exit without Rust destructors, like the app being killed before Save.
            std::process::exit(0);
        }
        Some("restore") => {
            let document = recovery::initialize_editing_session(false)
                .expect("initialize ordinary launch")
                .expect("recover the unsaved document");
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
            recovery::initialize_editing_session(true).expect("initialize recovery");
            let directory =
                loom_production::snapshot::application_state_directory("org.loom.writer")
                    .expect("resolve recovery directory");
            let mut permissions = std::fs::metadata(&directory).unwrap().permissions();
            use std::os::unix::fs::PermissionsExt;
            let original_permissions = permissions.clone();
            permissions.set_mode(0o500);
            std::fs::set_permissions(&directory, permissions).unwrap();

            let mut document = WriterDocument::new("write-failure", "Write failure");
            document.replace_paragraphs("An edit whose recovery write fails.");
            let save_path = PathBuf::from(std::env::var_os(SAVE_PATH).expect("save path"));
            let (app, state) = make_test_state(document, Some(save_path));
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

    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated test root");
    let open_path = root.join("opened.loomdoc");
    let save_path = root.join("saved-after-error.loomdoc");
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
            assert!(initialize_recovery_for_gui(&app, false).is_none());
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
                theme: "light".into(),
                rtl: false,
                open: Some(open_path.to_string_lossy().into_owned()),
                template: None,
                template_chooser: false,
                inspector: false,
                comment: None,
                table: false,
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
