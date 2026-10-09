//! Opt-in PERF-01 measurements on million-cell workbooks, run in release mode.
//! Each test returns at once unless its variable is set, so the default test
//! run is unchanged. Run one test at a time with `--release --test-threads=1`.
//!
//! - `LOOM_PERF_FIXTURE_DIR=<dir>`: write `chain-10k.loomtable` and
//!   `unique-1m.loomtable`, the native packages the app itself saves.
//! - `LOOM_PERF_UI=1`: UI-thread cost of whole-workbook edits on 1,000,000 cells.
//! - `LOOM_PERF_EXPORT=1`: CSV and XLSX export of 1,000,000 cells.
//! - `LOOM_PERF_FRAME=1`: scroll callback and software render at 1,000,000 cells.
//! - `LOOM_PERF_MICRO=1`: cost of the individual full-workbook steps.
//!
//! `LOOM_PERF_REPS` overrides the repetition count of the running test.

use super::pseudorandom_address;
use crate::export_operations::ExportCompletion;
use crate::system_clipboard::set_external_text_for_test;
use crate::*;
use loom_sheets_core::{CellRef, Sheet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SIDE: u32 = 2_048;
const UNIQUE_CELLS: u32 = 1_000_000;

fn env_reps(default: usize) -> usize {
    std::env::var("LOOM_PERF_REPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|count: &usize| *count > 0)
        .unwrap_or(default)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn timed(callback: impl FnOnce()) -> Duration {
    let started = Instant::now();
    callback();
    started.elapsed()
}

/// Median, 95th percentile and maximum (nearest rank over sorted samples).
fn summary(mut values: Vec<f64>) -> (f64, f64, f64) {
    values.sort_by(f64::total_cmp);
    let at = |fraction: f64| values[((values.len() - 1) as f64 * fraction).round() as usize];
    (at(0.5), at(0.95), values[values.len() - 1])
}

/// 500 rows of 20 chained formulas: 10,000 formulas, the shape of
/// `loom-sheets-core/tests/perf_measure.rs`, stored as a workbook.
fn chain_sheet() -> Sheet {
    const COLUMNS: [&str; 21] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U",
    ];
    let mut sheet = Sheet::new("Chain 10k");
    for row in 1..=500u32 {
        sheet.set_str(&format!("A{row}"), &row.to_string());
        for column in 1..=20usize {
            sheet.set_str(
                &format!("{}{row}", COLUMNS[column]),
                &format!("={}{row}+1", COLUMNS[column - 1]),
            );
        }
    }
    sheet
}

/// One million numeric cells at pseudorandom, unique addresses in a 2,048 by
/// 2,048 sheet: the workload used by `perf_tests.rs`.
fn unique_sheet() -> Sheet {
    let mut sheet = Sheet::new("Unique 1M");
    for index in 0..UNIQUE_CELLS {
        let address = pseudorandom_address(index);
        sheet.set_raw(
            CellRef {
                row: address / SIDE,
                col: address % SIDE,
            },
            "42",
        );
    }
    assert_eq!(sheet.cells.len(), UNIQUE_CELLS as usize);
    sheet
}

fn filters() -> [FileFilter; 4] {
    [
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ]
}

#[test]
fn write_native_fixtures_for_rss_runs() {
    let Some(dir) = std::env::var_os("LOOM_PERF_FIXTURE_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    fs::create_dir_all(&dir).expect("create fixture directory");
    for (name, sheet) in [("chain-10k", chain_sheet()), ("unique-1m", unique_sheet())] {
        let bytes = crate::workbook_package_bytes(std::slice::from_ref(&sheet), 0)
            .expect("package workbook");
        let path = dir.join(format!("{name}.loomtable"));
        fs::write(&path, &bytes).expect("write fixture");
        eprintln!(
            "PERF_FIXTURE file={} bytes={} cells={}",
            path.display(),
            bytes.len(),
            sheet.cells.len()
        );
    }
}

/// A window wired like the app: the real callbacks, plus a worker that owns
/// the workbook and writes recovery checkpoints.
struct Session {
    app: SheetsApp,
    state: Rc<GuiState>,
    recovery_dir: PathBuf,
}

impl Session {
    fn open(sheet: Sheet, tag: &str, save_paths: Vec<PathBuf>) -> Self {
        set_platform();
        set_external_text_for_test(None);
        let app = SheetsApp::new().expect("create SheetsApp");
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
            [],
            save_paths.into_iter().map(Some),
        ));
        let [workbook, csv, csv_import, xlsx] = filters();
        let state = Rc::new(GuiState::new(
            sheet, None, dialogs, workbook, csv, csv_import, xlsx,
        ));
        let menu = Arc::new(NativeMenuBar::new());
        register_sheet_actions(&app, &state, &menu);
        register_history_actions(&app, &state, &menu);
        wire_export_callbacks(&app, &state);

        let recovery_dir =
            std::env::temp_dir().join(format!("loom-perf-{tag}-{}", std::process::id()));
        crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
        fs::create_dir_all(&recovery_dir).expect("create recovery directory");
        let save_completions = state.save_operations.borrow().sender();
        let export_completions = state.export_operations.borrow().sender();
        let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_file_completions(
            recovery_dir.clone(),
            "loom.sheets/1",
            save_completions,
            export_completions,
        )
        .expect("start workbook worker");
        assert!(
            startup.recovery_error.is_none(),
            "recovery startup failed: {:?}",
            startup.recovery_error
        );
        let revision = state.next_worker_revision();
        let (sheets, active) = workbook_sheets(&state);
        let model = worker
            .initialize_workbook(revision, active, sheets)
            .expect("initialize workbook");
        state.last_queued_worker_revision.set(revision);
        state.install_workbook(model.sheets, model.active_sheet);
        state.mark_saved();
        let result = worker
            .wait_for_result_timeout(revision, Duration::from_secs(900))
            .expect("initial calculation and recovery checkpoint");
        assert!(apply_workbook_worker_result(&app, &state, result));
        *state.workbook_worker.borrow_mut() = Some(worker);
        Self {
            app,
            state,
            recovery_dir,
        }
    }

    fn select(&self, from: &str, to: &str) {
        let values = values_for_projection(&self.state);
        let sheet = self.state.current.borrow();
        update_selection_range(
            &self.app,
            &sheet,
            &values,
            GridSelection::new(
                CellRef::parse(from).expect("selection start"),
                CellRef::parse(to).expect("selection end"),
            ),
        );
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.state.workbook_worker.borrow_mut().take();
        crate::cell_edit_recovery::remove_test_recovery_data(&self.recovery_dir);
    }
}

/// Worker-side costs and the UI-thread time spent applying its result.
struct Settled {
    eval: Duration,
    package: Duration,
    write: Duration,
    apply: Duration,
}

/// Wait for the worker to finish the revision the last callback queued, then
/// apply its result on the UI thread the way the app's timer does.
fn settle(session: &Session) -> Settled {
    let revision = session.state.worker_revision.get();
    let result = {
        let worker = session.state.workbook_worker.borrow();
        worker
            .as_ref()
            .expect("worker is attached")
            .wait_for_result_timeout(revision, Duration::from_secs(900))
    }
    .expect("worker result for the queued revision");
    assert_eq!(result.revision, revision);
    let (eval, package, write) = (
        result.evaluation_duration,
        result.recovery_package_duration,
        result.recovery_journal_duration,
    );
    let started = Instant::now();
    assert!(
        apply_workbook_worker_result(&session.app, &session.state, result),
        "worker result was not applied"
    );
    Settled {
        eval,
        package,
        write,
        apply: started.elapsed(),
    }
}

#[derive(Default)]
struct Series {
    callback: Vec<f64>,
    apply: Vec<f64>,
    eval: Vec<f64>,
    package: Vec<f64>,
    write: Vec<f64>,
}

impl Series {
    fn record(&mut self, callback: Duration, settled: &Settled) {
        self.callback.push(ms(callback));
        self.apply.push(ms(settled.apply));
        self.eval.push(ms(settled.eval));
        self.package.push(ms(settled.package));
        self.write.push(ms(settled.write));
    }

    fn print(&self, step: &str, cells: usize) {
        let (c50, c95, cmax) = summary(self.callback.clone());
        let (a50, _, amax) = summary(self.apply.clone());
        let (e50, _, _) = summary(self.eval.clone());
        let (p50, _, _) = summary(self.package.clone());
        let (w50, _, _) = summary(self.write.clone());
        eprintln!(
            "PERF_UI step={step} cells={cells} n={} callback_ms p50={c50:.2} p95={c95:.2} max={cmax:.2} \
             ui_apply_result_ms p50={a50:.2} max={amax:.2} \
             worker_ms eval_p50={e50:.1} package_p50={p50:.1} write_p50={w50:.1}",
            self.callback.len()
        );
    }
}

/// UI-thread time of format, row insert, sort, paste and undo/redo on a
/// million-cell workbook, each followed by the worker round trip.
#[test]
fn ui_thread_cost_of_whole_workbook_edits_on_one_million_cells() {
    if std::env::var_os("LOOM_PERF_UI").is_none() {
        return;
    }
    let reps = env_reps(10);
    let session = Session::open(unique_sheet(), "ui", Vec::new());
    let (mut bold, mut bold_undo, mut bold_redo) =
        (Series::default(), Series::default(), Series::default());
    let (mut add_row, mut sort, mut sort_undo, mut paste) = (
        Series::default(),
        Series::default(),
        Series::default(),
        Series::default(),
    );
    for _ in 0..reps {
        session.select("A1", "A1");
        let callback = timed(|| session.app.invoke_toggle_bold());
        bold.record(callback, &settle(&session));
        let callback = timed(|| session.app.invoke_undo());
        bold_undo.record(callback, &settle(&session));
        let callback = timed(|| session.app.invoke_redo());
        bold_redo.record(callback, &settle(&session));

        let callback = timed(|| session.app.invoke_add_row());
        add_row.record(callback, &settle(&session));

        session.select("A1", "A1");
        let callback = timed(|| session.app.invoke_sort_ascending());
        sort.record(callback, &settle(&session));
        let callback = timed(|| session.app.invoke_undo());
        sort_undo.record(callback, &settle(&session));

        session.select("A1", "A1");
        session.app.invoke_copy_selection();
        session.select("E5", "E5");
        let callback = timed(|| session.app.invoke_paste_selection());
        paste.record(callback, &settle(&session));
    }
    let cells = session.state.current.borrow().cells.len();
    bold.print("format_bold", cells);
    bold_undo.print("undo_format", cells);
    bold_redo.print("redo_format", cells);
    add_row.print("row_insert_add_row", cells);
    sort.print("sort_ascending", cells);
    sort_undo.print("undo_sort", cells);
    paste.print("paste_one_cell", cells);
}

fn wait_for_export(state: &GuiState) -> ExportCompletion {
    let deadline = Instant::now() + Duration::from_secs(1_800);
    loop {
        if let Some((completion, accepted)) = state.export_operations.borrow_mut().drain().pop() {
            assert!(accepted, "export completion had no accepted request");
            return completion;
        }
        assert!(Instant::now() < deadline, "export did not complete");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// CSV and XLSX export of a million-cell workbook: UI callback time, and the
/// worker time reported with the completion (file write included).
#[test]
fn export_cost_on_one_million_cells() {
    if std::env::var_os("LOOM_PERF_EXPORT").is_none() {
        return;
    }
    let reps = env_reps(3);
    let out = std::env::temp_dir().join(format!("loom-perf-export-{}", std::process::id()));
    fs::create_dir_all(&out).expect("create export directory");
    let mut paths = Vec::new();
    for rep in 0..reps {
        paths.push(out.join(format!("export-{rep}.csv")));
        paths.push(out.join(format!("export-{rep}.xlsx")));
    }
    let session = Session::open(unique_sheet(), "export", paths.clone());
    for rep in 0..reps {
        for (format, path) in [("csv", &paths[2 * rep]), ("xlsx", &paths[2 * rep + 1])] {
            let callback = if format == "csv" {
                timed(|| session.app.invoke_export_csv())
            } else {
                timed(|| session.app.invoke_export_xlsx())
            };
            let completion = wait_for_export(&session.state);
            assert!(
                completion.result.is_ok(),
                "{format} export failed: {:?}",
                completion.result
            );
            let bytes = fs::metadata(path).map_or(0, |metadata| metadata.len());
            eprintln!(
                "PERF_EXPORT format={format} cells=1000000 rep={rep} ui_callback_ms={:.3} worker_ms={:.1} file_bytes={bytes}",
                ms(callback),
                ms(completion.worker_duration),
            );
        }
    }
}

fn frame_bench(label: &str, sheet: Sheet, frames: usize) {
    set_platform();
    let app = SheetsApp::new().expect("create window");
    app.set_local_menu_visible(true);
    let (w, h) = (1280u32, 800u32);
    app.window().set_size(PhysicalSize::new(w, h));
    apply_layout_breakpoints(&app, w);
    apply_headless_viewport_size(&app, w, h);
    let cells = sheet.cells.len();
    let [workbook, csv, csv_import, xlsx] = filters();
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        workbook,
        csv,
        csv_import,
        xlsx,
    ));
    project_current(&app, &state);
    snapshot_component(&app, w as f32, h as f32, 1.0).expect("first frame");
    let mut callback = Vec::new();
    let mut render = Vec::new();
    for frame in 0..frames {
        // The same alternating wheel steps as frame_bench_tests.rs.
        let (dx, dy) = if frame % 3 == 2 {
            (-40.0, 0.0)
        } else {
            (0.0, -72.0)
        };
        app.set_grid_scroll_x(app.get_grid_scroll_x() + dx);
        app.set_grid_scroll_y(app.get_grid_scroll_y() + dy);
        let started = Instant::now();
        scroll_projection::project_scroll(&app, &state);
        callback.push(ms(started.elapsed()));
        let started = Instant::now();
        snapshot_component(&app, w as f32, h as f32, 1.0).expect("frame");
        render.push(ms(started.elapsed()));
    }
    let (c50, c95, cmax) = summary(callback);
    let (r50, r95, rmax) = summary(render);
    eprintln!(
        "PERF_FRAME label={label} cells={cells} frames={frames} os={} scroll_callback_ms p50={c50:.2} p95={c95:.2} max={cmax:.2} software_render_ms p50={r50:.2} p95={r95:.2} max={rmax:.2}",
        std::env::consts::OS
    );
}

#[test]
fn frame_cost_on_one_million_cells() {
    if std::env::var_os("LOOM_PERF_FRAME").is_none() {
        return;
    }
    frame_bench(
        "unique-1m-pseudorandom-2048x2048",
        unique_sheet(),
        env_reps(90),
    );
}

/// Cost of each full-workbook step on its own, to show where the UI-thread
/// time of an edit goes.
#[test]
fn full_workbook_step_costs_on_one_million_cells() {
    if std::env::var_os("LOOM_PERF_MICRO").is_none() {
        return;
    }
    let reps = env_reps(5);
    let sheet = unique_sheet();
    let [workbook, csv, csv_import, xlsx] = filters();
    let state = GuiState::new(
        sheet.clone(),
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        workbook,
        csv,
        csv_import,
        xlsx,
    );
    let (mut clone, mut copies, mut dims, mut scan, mut json, mut eval) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    for _ in 0..reps {
        clone.push(ms(timed(|| {
            std::hint::black_box(sheet.clone());
        })));
        copies.push(ms(timed(|| {
            std::hint::black_box(workbook_sheets(&state));
        })));
        dims.push(ms(timed(|| {
            std::hint::black_box(sheet.dimensions());
        })));
        scan.push(ms(timed(|| {
            std::hint::black_box(
                sheet
                    .cells
                    .values()
                    .filter(|cell| cell.raw.trim_start().starts_with('='))
                    .count(),
            );
        })));
        json.push(ms(timed(|| {
            std::hint::black_box(loom_sheets_core::workbook_to_json(
                std::slice::from_ref(&sheet),
                0,
            ));
        })));
        eval.push(ms(timed(|| {
            std::hint::black_box(loom_sheets_core::evaluate(&sheet));
        })));
    }
    for (step, samples) in [
        ("sheet_clone", clone),
        ("workbook_sheets_copy_x2", copies),
        ("sheet_dimensions_scan", dims),
        ("formula_count_scan", scan),
        ("workbook_to_json", json),
        ("evaluate_no_formulas", eval),
    ] {
        let (p50, p95, max) = summary(samples);
        eprintln!(
            "PERF_MICRO step={step} cells=1000000 n={reps} p50_ms={p50:.2} p95_ms={p95:.2} max_ms={max:.2}"
        );
    }
}

fn resident_kib() -> i64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0)
}

/// Undo-history memory: the bytes `push_history` accounts (capped at 8 MiB)
/// against the resident memory the retained snapshots really occupy.
#[test]
fn undo_history_memory_after_repeated_sorts() {
    if std::env::var_os("LOOM_PERF_HISTORY").is_none() {
        return;
    }
    let session = Session::open(unique_sheet(), "history", Vec::new());
    let baseline = resident_kib();
    for sort in 1..=3 {
        session.select("A1", "A1");
        session.app.invoke_sort_ascending();
        settle(&session);
        let stack = session.state.undo_stack.borrow();
        let accounted = crate::history_bytes(&stack);
        eprintln!(
            "PERF_HISTORY sorts={sort} undo_entries={} accounted_mib={:.1} resident_growth_mib={:.1}",
            stack.len(),
            accounted as f64 / 1_048_576.0,
            (resident_kib() - baseline) as f64 / 1_024.0,
        );
    }
}

/// Where the UI-thread time of an edit goes, timed one step at a time on the
/// live session state of a million-cell workbook.
#[test]
fn attribution_of_ui_thread_steps_on_one_million_cells() {
    if std::env::var_os("LOOM_PERF_ATTRIBUTION").is_none() {
        return;
    }
    let reps = env_reps(5);
    let session = Session::open(unique_sheet(), "attribution", Vec::new());
    session.select("A1", "A1");
    let (mut projection, mut copy, mut dirty, mut sort) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for _ in 0..reps {
        projection.push(ms(timed(|| project_current(&session.app, &session.state))));
        copy.push(ms(timed(|| {
            std::hint::black_box(workbook_sheets(&session.state));
        })));
        dirty.push(ms(timed(|| session.state.recompute_dirty_from_saved())));
        let mut sheet = session.state.current.borrow().clone();
        let (mut undo, mut redo) = (Vec::new(), Vec::new());
        sort.push(ms(timed(|| {
            std::hint::black_box(crate::actions::sort_table(
                &mut sheet, &mut undo, &mut redo, 0, true,
            ));
        })));
    }
    for (step, samples) in [
        ("project_current", projection),
        ("workbook_sheets_copy", copy),
        ("recompute_dirty_from_saved", dirty),
        ("sort_table_on_sheet_copy", sort),
    ] {
        let (p50, p95, max) = summary(samples);
        eprintln!(
            "PERF_ATTRIBUTION step={step} cells=1000000 n={reps} p50_ms={p50:.2} p95_ms={p95:.2} max_ms={max:.2}"
        );
    }
}
