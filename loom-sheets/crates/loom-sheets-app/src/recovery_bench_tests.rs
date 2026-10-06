//! Opt-in recovery durability measurements (`LOOM_REC_BENCH=1`).
//!
//! These print `REC-BENCH` lines; with `LOOM_REC_BENCH=enforce` they also fail
//! when the approved 250 ms p95 edit-durability target is missed. Run them in
//! an optimized test profile (`CARGO_PROFILE_TEST_OPT_LEVEL=3`) for meaningful
//! numbers.

use super::*;
use crate::cell_edit_recovery::{versioned_directory_for, CellEditRecovery};
use crate::workbook_io::workbook_package_bytes;
use std::fs;
use std::path::Path;
use std::time::Duration;

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn mode() -> Option<bool> {
    match std::env::var("LOOM_REC_BENCH").ok().as_deref() {
        Some("enforce") => Some(true),
        Some(_) => Some(false),
        None => None,
    }
}

fn directory_bytes(directory: &Path) -> u64 {
    fs::read_dir(directory).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .map(|entry| entry.metadata().map_or(0, |metadata| metadata.len()))
            .sum()
    })
}

fn percentile(sorted: &[Duration], fraction: f64) -> f64 {
    let index = ((sorted.len() as f64 * fraction).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[index].as_secs_f64() * 1_000.0
}

fn sheet_with_cells(count: u32) -> Vec<Sheet> {
    let mut sheet = Sheet::new("Bench");
    for index in 0..count {
        sheet.set_raw(
            CellRef {
                row: index / 100,
                col: index % 100,
            },
            format!("{index}"),
        );
    }
    vec![sheet]
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-bench-{tag}-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("bench scratch");
        Self(path)
    }

    fn base(&self) -> PathBuf {
        self.0.join("recovery")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn bench_journal_durability_after_one_ten_and_one_hundred_unsaved_edits() {
    let Some(enforce) = mode() else {
        return;
    };
    const TRIALS: usize = 25;
    for workbook_cells in [100_u32, 100_000] {
        for unsaved in [1_usize, 10, 100] {
            let mut latencies = Vec::with_capacity(TRIALS);
            let (mut journal_bytes, mut directory_total) = (0, 0);
            for trial in 0..TRIALS {
                let scratch = Scratch::new("durability");
                let mut sheets = sheet_with_cells(workbook_cells);
                let (mut recovery, _) = CellEditRecovery::open_at(scratch.base()).expect("open");
                recovery
                    .checkpoint_package(workbook_package_bytes(&sheets, 0).unwrap(), false)
                    .expect("baseline checkpoint");
                for edit in 0..unsaved {
                    let cell = CellRef {
                        row: 5_000 + edit as u32,
                        col: trial as u32,
                    };
                    let raw = format!("edit {trial}-{edit}");
                    sheets[0].set_raw(cell, raw.clone());
                    let started = Instant::now();
                    recovery
                        .record_cells(0, [(0, cell, Some(raw))], || {
                            workbook_package_bytes(&sheets, 0)
                        })
                        .expect("durable edit");
                    if edit + 1 == unsaved {
                        latencies.push(started.elapsed());
                    }
                }
                if trial == 0 {
                    let versioned = versioned_directory_for(&scratch.base()).unwrap();
                    journal_bytes = fs::metadata(versioned.join("operations.jsonl"))
                        .map_or(0, |metadata| metadata.len());
                    directory_total = directory_bytes(&versioned);
                }
            }
            latencies.sort_unstable();
            let p95 = percentile(&latencies, 0.95);
            eprintln!(
                "REC-BENCH durability workbook_cells={workbook_cells} unsaved_edits={unsaved} trials={TRIALS} p50_ms={:.2} p95_ms={p95:.2} max_ms={:.2} journal_bytes={journal_bytes} recovery_dir_file_bytes={directory_total}",
                percentile(&latencies, 0.5),
                percentile(&latencies, 1.0),
            );
            if enforce {
                assert!(p95 < 250.0, "p95 {p95:.2} ms exceeds the 250 ms target");
            }
        }
    }
}

#[test]
fn bench_worker_journal_latency_across_one_hundred_consecutive_edits() {
    let Some(enforce) = mode() else {
        return;
    };
    let scratch = Scratch::new("worker");
    let (worker, startup) = WorkbookWorker::start_at(scratch.base(), "loom.sheets/1").unwrap();
    assert!(startup.recovery_error.is_none());
    worker
        .initialize_workbook(1, 0, sheet_with_cells(10_000))
        .unwrap();
    worker
        .wait_for_result_timeout(1, Duration::from_secs(60))
        .expect("initial result");
    let mut journal_times = Vec::new();
    let versioned = versioned_directory_for(&scratch.base()).unwrap();
    for edit in 0..100_u64 {
        let revision = edit + 2;
        worker
            .submit_cell(CellUpdate {
                revision,
                active_sheet: 0,
                sheet: 0,
                cell: CellRef {
                    row: 9_000 + edit as u32,
                    col: 0,
                },
                raw: Some(format!("e{edit}")),
            })
            .unwrap();
        let result = worker
            .wait_for_result_timeout(revision, Duration::from_secs(60))
            .expect("edit result");
        assert!(result.recovery_error.is_none());
        journal_times.push(result.recovery_journal_duration);
        if matches!(edit, 0 | 9 | 99) {
            eprintln!(
                "REC-BENCH bytes_kept unsaved_edits={} journal_bytes={} recovery_dir_file_bytes={}",
                edit + 1,
                fs::metadata(versioned.join("operations.jsonl")).map_or(0, |m| m.len()),
                directory_bytes(&versioned)
            );
        }
    }
    journal_times.sort_unstable();
    let p95 = percentile(&journal_times, 0.95);
    eprintln!(
        "REC-BENCH worker_journal edits=100 p50_ms={:.2} p95_ms={p95:.2} max_ms={:.2}",
        percentile(&journal_times, 0.5),
        percentile(&journal_times, 1.0)
    );
    if enforce {
        assert!(p95 < 250.0, "worker journal p95 {p95:.2} ms exceeds 250 ms");
    }
}
