//! Keyboard journey recorder for the Loom command palette.
//!
//! The recorder drives the real Slint input pipeline: key events are
//! dispatched through [`slint::platform::Window`]'s public event API and are
//! handled by the same `key-pressed` FocusScope handlers a physical keyboard
//! triggers. Every step renders a screenshot and records the palette state,
//! producing a transcript plus pixel evidence for the visual-QA pipeline.
//!
//! One caveat is documented in each transcript: Slint 1.17's public event API
//! does not expose key-modifier injection, so the Ctrl+K *open* step is
//! performed through the host hook that the Ctrl+K FocusScope handler routes
//! to. Every other step — typing the query, arrow navigation, Return
//! invocation, and Escape dismissal — goes through real dispatched key
//! events and the real `key-pressed` handlers.

use std::path::Path;

use slint::{platform::WindowEvent, ComponentHandle};

use crate::capture::snapshot_component;

/// State probe an application implements so the recorder can verify the
/// palette's observable behavior without knowing the application UI.
pub trait PaletteProbe {
    /// Whether the command palette is currently open.
    fn palette_open(&self) -> bool;
    /// Number of commands currently shown in the filtered list.
    fn palette_commands(&self) -> usize;
    /// The selected command index.
    fn palette_selected(&self) -> i32;
    /// The command palette's current filter query text.
    fn palette_query(&self) -> String;
    /// Open the palette exactly as the Ctrl+K FocusScope handler does:
    /// reset the query, rebuild the unfiltered list, select the first row,
    /// and reveal the modal.
    fn open_palette(&self);
}

/// One recorded step of the journey.
#[derive(Debug)]
pub struct JourneyStep {
    pub name: String,
    pub keys: String,
    pub palette_open: bool,
    pub query: String,
    pub commands: usize,
    pub selected: i32,
    pub screenshot: String,
}

/// The complete recorded journey for one application.
#[derive(Debug)]
pub struct JourneyReport {
    pub app: String,
    pub query: String,
    pub passed: bool,
    pub steps: Vec<JourneyStep>,
    pub notes: Vec<String>,
}

fn dispatch_key<H: ComponentHandle>(app: &H, text: impl Into<slint::SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn capture_step<H: ComponentHandle + PaletteProbe>(
    app: &H,
    name: &str,
    keys: &str,
    out_dir: &Path,
    app_name: &str,
) -> Result<JourneyStep, String> {
    let (width, height) = (1280.0f32, 800.0f32);
    let image = snapshot_component(app, width, height, 1.0).map_err(|e| format!("capture: {e}"))?;
    let file_name = format!("{app_name}-journey-{name}.png");
    let path = out_dir.join(&file_name);
    crate::png::save_png(&path, &image).map_err(|e| format!("save {file_name}: {e}"))?;
    Ok(JourneyStep {
        name: name.to_string(),
        keys: keys.to_string(),
        palette_open: app.palette_open(),
        query: app.palette_query(),
        commands: app.palette_commands(),
        selected: app.palette_selected(),
        screenshot: file_name,
    })
}

/// Record the canonical keyboard-only command-palette journey:
/// open → type → filter → navigate → invoke → reopen → dismiss.
///
/// `query` must be a filter that yields at least one command for the
/// application (per-application in the matrix configuration).
pub fn record_keyboard_palette_journey<H: ComponentHandle + PaletteProbe>(
    app: &H,
    app_name: &str,
    out_dir: &Path,
    query: &str,
) -> Result<JourneyReport, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create {out_dir:?}: {e}"))?;
    let mut steps = Vec::new();
    let mut notes = vec![
        "open step uses the Ctrl+K host hook (Slint 1.17 public API cannot inject modifiers); all other steps are real dispatched key events".to_string(),
    ];

    steps.push(capture_step(app, "0-initial", "", out_dir, app_name)?);

    app.open_palette();
    steps.push(capture_step(app, "1-open", "ctrl+k", out_dir, app_name)?);

    for (index, character) in query.chars().enumerate() {
        dispatch_key(app, character.to_string());
        steps.push(capture_step(
            app,
            &format!("2-query-{}", index + 1),
            &character.to_string(),
            out_dir,
            app_name,
        )?);
    }

    dispatch_key(app, slint::platform::Key::DownArrow);
    steps.push(capture_step(app, "3-move-down", "down", out_dir, app_name)?);

    dispatch_key(app, slint::platform::Key::Return);
    steps.push(capture_step(app, "4-invoke", "return", out_dir, app_name)?);

    app.open_palette();
    dispatch_key(app, slint::platform::Key::Escape);
    steps.push(capture_step(app, "5-dismiss", "escape", out_dir, app_name)?);

    let mut passed = true;
    let mut failures = Vec::new();
    let first = steps.get(1).ok_or("missing open step")?;
    if !first.palette_open {
        passed = false;
        failures.push("palette did not open".to_string());
    }
    let last_query = steps
        .iter()
        .rev()
        .find(|step| step.name.starts_with("2-query-"))
        .ok_or("missing query steps")?;
    if last_query.commands == 0 {
        passed = false;
        failures.push("filter query produced an empty command list".to_string());
    }
    if last_query.commands >= steps[1].commands && steps[1].commands > 0 {
        passed = false;
        failures.push("filter query did not narrow the command list".to_string());
    }
    let move_step = steps
        .iter()
        .find(|step| step.name == "3-move-down")
        .ok_or("missing move step")?;
    if move_step.selected <= last_query.selected {
        passed = false;
        failures.push("down-arrow did not move the selection forward".to_string());
    }
    let invoke = steps
        .iter()
        .find(|step| step.name == "4-invoke")
        .ok_or("missing invoke step")?;
    if invoke.palette_open {
        passed = false;
        failures.push("Return did not dismiss the palette after invocation".to_string());
    }
    let dismiss = steps
        .iter()
        .find(|step| step.name == "5-dismiss")
        .ok_or("missing dismiss step")?;
    if dismiss.palette_open {
        passed = false;
        failures.push("Escape did not dismiss the palette".to_string());
    }
    if !failures.is_empty() {
        notes.push(format!("failed invariants: {}", failures.join("; ")));
    }

    let report = JourneyReport {
        app: app_name.to_string(),
        query: query.to_string(),
        passed,
        steps,
        notes,
    };
    let report_path = out_dir.join(format!("{app_name}.json"));
    let steps_json: Vec<serde_json::Value> = report
        .steps
        .iter()
        .map(|step| {
            serde_json::json!({
                "name": step.name,
                "keys": step.keys,
                "palette_open": step.palette_open,
                "query": step.query,
                "commands": step.commands,
                "selected": step.selected,
                "screenshot": step.screenshot,
            })
        })
        .collect();
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "app": report.app,
        "query": report.query,
        "passed": report.passed,
        "notes": report.notes,
        "steps": steps_json,
    }))
    .map_err(|e| e.to_string())?;
    std::fs::write(&report_path, json).map_err(|e| format!("write report: {e}"))?;
    Ok(report)
}
