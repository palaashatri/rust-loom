//! Test-only write-fault injection for recovery storage.
//!
//! Compiled only with the `fault-injection` feature, which application test
//! builds enable as a dev-dependency feature. A rule names one recovery
//! directory and one write step; the next matching writes fail with an I/O
//! error shaped like a full disk. Rules are keyed by directory so parallel
//! tests never disturb each other.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A recovery write that a test can make fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultStep {
    /// Appending one record to the operation journal.
    JournalAppend,
    /// Writing a checkpoint generation payload.
    CheckpointPayload,
    /// Writing a checkpoint generation metadata file.
    CheckpointMetadata,
    /// Publishing the checkpoint pointer or commit record.
    CheckpointPointer,
    /// Rewriting the journal (compaction or torn-tail repair).
    JournalRewrite,
}

struct Rule {
    directory: PathBuf,
    step: FaultStep,
    skip: u32,
    remaining: u32,
    hits: u32,
}

static RULES: Mutex<Vec<Rule>> = Mutex::new(Vec::new());

/// Skip `skip` matching writes in `directory`, then fail the next `count`.
pub fn inject(directory: &Path, step: FaultStep, skip: u32, count: u32) {
    if let Ok(mut rules) = RULES.lock() {
        rules.push(Rule {
            directory: directory.to_path_buf(),
            step,
            skip,
            remaining: count,
            hits: 0,
        });
    }
}

/// Remove every rule for `directory`.
pub fn clear(directory: &Path) {
    if let Ok(mut rules) = RULES.lock() {
        rules.retain(|rule| rule.directory != directory);
    }
}

/// How many writes of `step` in `directory` reached an injection point.
pub fn hits(directory: &Path, step: FaultStep) -> u32 {
    RULES.lock().map_or(0, |rules| {
        rules
            .iter()
            .filter(|rule| rule.directory == directory && rule.step == step)
            .map(|rule| rule.hits)
            .sum()
    })
}

/// Classify an atomic-write target by its file name.
pub(crate) fn classify(path: &Path) -> Option<FaultStep> {
    let name = path.file_name()?.to_str()?;
    if name == "operations.jsonl" {
        Some(FaultStep::JournalRewrite)
    } else if name.starts_with("checkpoint-generation-") && name.ends_with(".bin") {
        Some(FaultStep::CheckpointPayload)
    } else if name.starts_with("checkpoint-generation-") && name.ends_with(".json") {
        Some(FaultStep::CheckpointMetadata)
    } else if name == "checkpoint.json"
        || (name.starts_with("checkpoint-commit-") && name.ends_with(".json"))
    {
        Some(FaultStep::CheckpointPointer)
    } else {
        None
    }
}

/// Fail `path` when a rule matches its step.
pub(crate) fn check(path: &Path, step: FaultStep) -> io::Result<()> {
    let Some(directory) = path.parent() else {
        return Ok(());
    };
    let Ok(mut rules) = RULES.lock() else {
        return Ok(());
    };
    for rule in rules
        .iter_mut()
        .filter(|rule| rule.step == step && rule.directory == directory)
    {
        rule.hits += 1;
        if rule.skip > 0 {
            rule.skip -= 1;
        } else if rule.remaining > 0 {
            rule.remaining -= 1;
            return Err(io::Error::other(format!(
                "injected disk-full failure at {step:?}: no space left on device"
            )));
        }
    }
    Ok(())
}
