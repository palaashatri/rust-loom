//! Loom Motion desktop application.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{
    FileDialogService, FileFilter, NativeFileDialogs, OpenFileRequest, SaveFileRequest,
};
use loom_motion_core::{load_motion, save_motion, CompositionDocument, MotionLayer};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "comp.loommotion";
const EXPORT_FILENAME: &str = "composition-frame.svg";
const HISTORY_LIMIT: usize = 128;

loom_production::define_snapshot_recovery!(MOTION_RECOVERY, "org.loom.motion", "loom.motion/1");

struct Args {
    #[cfg_attr(not(feature = "visual-qa"), allow(dead_code))]
    screenshot: Option<String>,
    #[cfg_attr(not(feature = "visual-qa"), allow(dead_code))]
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        journey: None,
        size: DEFAULT_SIZE,
        theme: "dark".to_string(),
        open: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => {
                #[cfg(feature = "visual-qa")]
                {
                    args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?);
                }
                #[cfg(not(feature = "visual-qa"))]
                {
                    return Err("--screenshot requires a visual-qa build".into());
                }
            }
            "--smoke" => {
                #[cfg(feature = "visual-qa")]
                {
                    args.smoke = true;
                }
                #[cfg(not(feature = "visual-qa"))]
                {
                    return Err("--smoke requires a visual-qa build".into());
                }
            }
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(it.next().ok_or("--journey needs an output directory")?);
            }
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => {
                args.theme = it.next().ok_or("--theme needs a name")?;
            }
            "--open" => {
                args.open = Some(it.next().ok_or("--open needs a path")?);
            }
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn sample_motion() -> CompositionDocument {
    let mut doc = CompositionDocument::new("comp-sample", "Kinetic Typography Intro");
    let mut title = MotionLayer::new("layer-title", "Animated Title", "Text");
    title.add_keyframe("opacity", 0.0, 1.0);
    title.add_keyframe("opacity", 2.5, 1.0);
    title.add_keyframe("opacity", 4.0, 0.35);
    doc.add_layer(title);
    doc.add_layer(MotionLayer::new("l-sub", "Subtitle Motion", "Text"));
    for layer in &mut doc.layers {
        layer.add_keyframe("x", 0.0, 960.0);
        layer.add_keyframe("y", 0.0, 540.0);
        layer.add_keyframe("scale", 0.0, 1.0);
        layer.add_keyframe("rotation", 0.0, 0.0);
        if layer.opacity_keys.is_empty() {
            layer.add_keyframe("opacity", 0.0, 1.0);
        }
    }
    doc
}

fn empty_motion() -> CompositionDocument {
    CompositionDocument::new("untitled-composition", "Untitled Composition")
}

fn load_motion_path(path: &Path) -> Result<CompositionDocument, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read motion composition '{}': {error}",
            path.display()
        )
    })?;
    load_motion(&bytes).map_err(|error| {
        format!(
            "failed to load motion composition '{}': {error}",
            path.display()
        )
    })
}

fn initial_motion(args: &Args) -> Result<CompositionDocument, String> {
    match args.open.as_deref() {
        Some(path) => load_motion_path(Path::new(path)),
        None => Ok(sample_motion()),
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn export_svg_frame(doc: &CompositionDocument, time_secs: f32) -> String {
    let mut svg = String::from(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
  <rect width="1920" height="1080" fill="#101217"/>
"##,
    );
    for (index, layer) in doc.layers.iter().enumerate() {
        let sample = layer.sample(time_secs);
        let opacity = sample.opacity.clamp(0.0, 1.0);
        let scale = sample.scale.max(0.001);
        let name = xml_escape(&layer.name);
        let transform = format!(
            "translate({:.3} {:.3}) rotate({:.3}) scale({:.5})",
            sample.x, sample.y, sample.rotation, scale
        );
        match layer.layer_type.as_str() {
            "Text" => svg.push_str(&format!(
                r##"  <text transform="{transform}" opacity="{opacity:.5}" text-anchor="middle" fill="#f5f2eb" font-family="sans-serif" font-size="72">{name}</text>
"##
            )),
            "VectorShape" => svg.push_str(&format!(
                r##"  <rect transform="{transform}" opacity="{opacity:.5}" x="-180" y="-100" width="360" height="200" rx="24" fill="#b86f4b"/>
"##
            )),
            _ => svg.push_str(&format!(
                r##"  <g transform="{transform}" opacity="{opacity:.5}"><rect x="-160" y="-90" width="320" height="180" rx="16" fill="#303744" stroke="#b86f4b"/><text y="8" text-anchor="middle" fill="#f5f2eb" font-family="sans-serif" font-size="28">{name} {}</text></g>
"##,
                index + 1
            )),
        }
    }
    svg.push_str("</svg>\n");
    svg
}

fn write_svg_frame(doc: &CompositionDocument, path: impl AsRef<Path>) -> Result<(), String> {
    let svg = export_svg_frame(doc, 0.0);
    loom_storage::atomic_write(path.as_ref(), svg.as_bytes()).map_err(|error| error.to_string())
}

fn apply_motion(app: &MotionApp, doc: &CompositionDocument) {
    app.set_comp_name(doc.name.as_str().into());
    app.set_timecode_text(SharedString::from(format!(
        "00:00:00:00 ({} fps • {:.0}s)",
        doc.frame_rate, doc.duration_secs
    )));
    app.set_timecode_display("00:00:00:00".into());
    let layer_labels: Vec<SharedString> = doc
        .layers
        .iter()
        .map(|layer| SharedString::from(format!("{} ({})", layer.name, layer.layer_type)))
        .collect();
    app.set_layer_labels(ModelRc::new(VecModel::from(layer_labels)));
    let selected = doc
        .layers
        .get(doc.active_layer_index)
        .map(|layer| layer.name.as_str())
        .unwrap_or("No layer selected");
    app.set_active_layer_index(doc.active_layer_index as i32);
    if let Some(layer) = doc.layers.get(doc.active_layer_index) {
        let sample = layer.sample(0.0);
        app.set_pos_x(sample.x);
        app.set_pos_y(sample.y);
        app.set_scale_val(sample.scale * 100.0);
        app.set_rotation_val(sample.rotation);
        app.set_opacity_val(sample.opacity * 100.0);
    }
    app.set_status_left(SharedString::from(format!(
        "{} motion layers • Selected: {selected}",
        doc.len()
    )));
    app.set_status_right("Offline".into());
    if let Ok(bytes) = save_motion(doc) {
        let _ = record_snapshot_recovery("motion state", bytes);
    }
}

fn apply_theme(app: &MotionApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

/// Commands exposed through the command palette. Each palette entry maps to
/// one of the application callbacks, so palette invocation and toolbar clicks
/// share a single dispatch path.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewComp,
    OpenComp,
    SaveComp,
    SaveAsComp,
    Undo,
    Redo,
    ExportFrame,
    AddLayer,
    PlayPause,
    StepBack,
    StepForward,
    ToggleLoop,
    ToggleCurveDrawer,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &MotionApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewComp,
            id: "motion.new",
            label: "New Composition",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenComp,
            id: "motion.open",
            label: "Open Composition",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveComp,
            id: "motion.save",
            label: "Save Composition",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsComp,
            id: "motion.save-as",
            label: "Save Composition As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "motion.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "motion.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::ExportFrame,
            id: "motion.export-frame",
            label: "Export SVG Frame",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::AddLayer,
            id: "motion.layer.add",
            label: "Add Motion Layer",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::PlayPause,
            id: "motion.play",
            label: "Play / Pause Preview",
            shortcut: "Space",
        },
        PaletteCommand {
            action: PaletteAction::StepBack,
            id: "motion.step-back",
            label: "Step Back One Frame",
            shortcut: "Left",
        },
        PaletteCommand {
            action: PaletteAction::StepForward,
            id: "motion.step-forward",
            label: "Step Forward One Frame",
            shortcut: "Right",
        },
        PaletteCommand {
            action: PaletteAction::ToggleLoop,
            id: "motion.loop",
            label: "Toggle Loop",
            shortcut: "L",
        },
        PaletteCommand {
            action: PaletteAction::ToggleCurveDrawer,
            id: "motion.curves",
            label: "Toggle Keyframe Graph",
            shortcut: "C",
        },
    ]
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &MotionApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette(app)
        .into_iter()
        .filter(|c| {
            query_lower.is_empty()
                || c.label.to_lowercase().contains(&query_lower)
                || c.id.to_lowercase().contains(&query_lower)
        })
        .map(|c| CommandPaletteItem {
            id: c.id.into(),
            label: c.label.into(),
            shortcut: c.shortcut.into(),
            enabled: true,
        })
        .collect();
    app.set_palette_commands(Rc::new(VecModel::from(items)).into());
    let count = app.get_palette_commands().row_count() as i32;
    let selected = app.get_palette_selected();
    if selected >= count && count > 0 {
        app.set_palette_selected(count - 1);
    } else if count == 0 {
        app.set_palette_selected(0);
    }
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    app.set_compact_layout(args.size.0 < 1180);
    apply_theme(&app, &args.theme);
    let doc = initial_motion(args)?;
    apply_motion(&app, &doc);
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    app.set_compact_layout(args.size.0 < 1180);
    apply_theme(&app, &args.theme);
    let doc = initial_motion(args)?;
    apply_motion(&app, &doc);
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "motion", Path::new(out_dir), "frame")
        .map_err(|e| format!("journey failed: {e}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    Ok(())
}

impl PaletteProbe for MotionApp {
    fn palette_open(&self) -> bool {
        self.get_palette_open()
    }

    fn palette_commands(&self) -> usize {
        self.get_palette_commands().row_count()
    }

    fn palette_selected(&self) -> i32 {
        self.get_palette_selected()
    }

    fn palette_query(&self) -> String {
        self.get_palette_query().to_string()
    }

    fn open_palette(&self) {
        self.invoke_open_palette();
    }
}

#[derive(Default)]
struct MotionHistory {
    undo: Vec<CompositionDocument>,
    redo: Vec<CompositionDocument>,
    coalescing_key: Option<String>,
}

impl MotionHistory {
    fn checkpoint(&mut self, current: &CompositionDocument, key: impl Into<String>) {
        let key = key.into();
        if self.coalescing_key.as_deref() != Some(key.as_str()) {
            self.undo.push(current.clone());
            if self.undo.len() > HISTORY_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.coalescing_key = Some(key);
    }

    fn break_coalescing(&mut self) {
        self.coalescing_key = None;
    }

    fn undo(&mut self, current: &mut CompositionDocument) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(current.clone());
        *current = previous;
        self.break_coalescing();
        true
    }

    fn redo(&mut self, current: &mut CompositionDocument) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(current.clone());
        *current = next;
        self.break_coalescing();
        true
    }
}

struct GuiState {
    current: RefCell<CompositionDocument>,
    history: RefCell<MotionHistory>,
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    composition_filter: FileFilter,
    svg_filter: FileFilter,
}

impl GuiState {
    fn checkpoint(&self, key: impl Into<String>) {
        let current = self.current.borrow();
        self.history.borrow_mut().checkpoint(&current, key);
    }

    fn replace(&self, document: CompositionDocument) {
        *self.current.borrow_mut() = document;
        *self.history.borrow_mut() = MotionHistory::default();
    }
}

fn refresh_motion(app: &MotionApp, state: &GuiState) {
    apply_motion(app, &state.current.borrow());
    let history = state.history.borrow();
    app.set_can_undo(!history.undo.is_empty());
    app.set_can_redo(!history.redo.is_empty());
}

fn set_status(app: &MotionApp, value: impl Into<SharedString>) {
    app.set_status_left(value.into());
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn open_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Loom Motion Composition".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.composition_filter.clone()],
    }
}

fn save_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow().clone();
    let suggested_name = path
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| SAVE_FILENAME.to_string());
    SaveFileRequest {
        title: "Save Loom Motion Composition".into(),
        initial_directory: initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: vec![state.composition_filter.clone()],
    }
}

fn export_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Loom Motion SVG Frame".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(EXPORT_FILENAME.to_string()),
        filters: vec![state.svg_filter.clone()],
    }
}

fn selected_motion_to_open(
    state: &GuiState,
) -> Result<Option<(PathBuf, CompositionDocument)>, String> {
    let path = state
        .dialogs
        .open_file(&open_request(state))
        .map_err(|error| error.to_string())?;
    let Some(path) = path else {
        return Ok(None);
    };
    let document = load_motion_path(&path)?;
    Ok(Some((path, document)))
}

fn persist_current_motion(
    state: &GuiState,
    force_picker: bool,
) -> Result<Option<(PathBuf, Vec<u8>)>, String> {
    let current_path = (!force_picker)
        .then(|| state.save_path.borrow().clone())
        .flatten();
    let path = match current_path {
        Some(path) => Some(path),
        None => state
            .dialogs
            .save_file(&save_request(state))
            .map_err(|error| error.to_string())?,
    };
    let Some(path) = path else {
        return Ok(None);
    };

    let bytes = save_motion(&state.current.borrow())?;
    loom_storage::atomic_write(&path, &bytes)
        .map_err(|error| format!("failed to atomic write '{}': {error}", path.display()))?;
    *state.save_path.borrow_mut() = Some(path.clone());
    Ok(Some((path, bytes)))
}

fn save_current_motion(
    app: &MotionApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    let Some((path, bytes)) = persist_current_motion(state, force_picker)? else {
        set_status(app, "Save cancelled");
        return Ok(false);
    };

    match checkpoint_snapshot_recovery(bytes) {
        Ok(()) => set_status(app, format!("Saved {}", path.display())),
        Err(error) => set_status(
            app,
            format!(
                "Saved {}, but recovery checkpoint failed: {error}",
                path.display()
            ),
        ),
    }
    Ok(true)
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    #[cfg(feature = "visual-qa")]
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    #[cfg(feature = "visual-qa")]
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-motion-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }

    let app = MotionApp::new().map_err(|e| e.to_string())?;
    app.set_compact_layout(args.size.0 < 1180);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_motion(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_motion(bytes).ok())
            .unwrap_or(initial_motion(&args)?)
    };
    let state = Rc::new(GuiState {
        current: RefCell::new(initial),
        history: RefCell::new(MotionHistory::default()),
        save_path: RefCell::new(args.open.as_ref().map(PathBuf::from)),
        dialogs: Rc::new(NativeFileDialogs),
        composition_filter: FileFilter::new("Loom Motion composition", ["loommotion"])
            .map_err(|error| error.to_string())?,
        svg_filter: FileFilter::new("SVG image", ["svg"]).map_err(|error| error.to_string())?,
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                state.replace(empty_motion());
                *state.save_path.borrow_mut() = None;
                refresh_motion(&app, &state);
                set_status(&app, "Created a new untitled composition");
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                match selected_motion_to_open(&state) {
                    Ok(Some((path, document))) => {
                        state.replace(document);
                        *state.save_path.borrow_mut() = Some(path.clone());
                        refresh_motion(&app, &state);
                        set_status(&app, format!("Opened {}", path.display()));
                    }
                    Ok(None) => set_status(&app, "Open cancelled"),
                    Err(error) => set_status(&app, format!("Open failed: {error}")),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_layer(move || {
            if let Some(app) = app_ref.upgrade() {
                state.checkpoint("add-layer");
                let mut current = state.current.borrow_mut();
                let count = current.len() + 1;
                let mut layer = MotionLayer::new(
                    format!("layer-{count}"),
                    format!("Motion Layer {count}"),
                    "VectorShape",
                );
                layer.add_keyframe("x", 0.0, 960.0);
                layer.add_keyframe("y", 0.0, 540.0);
                layer.add_keyframe("scale", 0.0, 1.0);
                layer.add_keyframe("rotation", 0.0, 0.0);
                layer.add_keyframe("opacity", 0.0, 1.0);
                current.add_layer(layer);
                current.active_layer_index = current.layers.len().saturating_sub(1);
                drop(current);
                refresh_motion(&app, &state);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_layer(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if current.select_layer(index as usize) {
                    state.history.borrow_mut().break_coalescing();
                    drop(current);
                    refresh_motion(&app, &state);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                let playing = app.get_is_playing();
                app.set_status_left(SharedString::from(if playing {
                    "Playing motion preview..."
                } else {
                    "Paused playback"
                }));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_step_back(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from("Stepped back 1 frame"));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_step_forward(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from("Stepped forward 1 frame"));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_loop(move || {
            if let Some(app) = app_ref.upgrade() {
                let looping = app.get_is_looping();
                app.set_status_left(SharedString::from(format!("Loop mode: {looping}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_transform_changed(move |prop, val| {
            if let Some(app) = app_ref.upgrade() {
                let active = state.current.borrow().active_layer_index;
                let property = match prop.as_str() {
                    "pos-x" => "x",
                    "pos-y" => "y",
                    "scale" => "scale",
                    "rotation" => "rotation",
                    "opacity" => "opacity",
                    _ => {
                        app.set_status_left(SharedString::from(format!(
                            "Unsupported transform property: {prop}"
                        )));
                        return;
                    }
                };
                let stored_value = match property {
                    "scale" | "opacity" => val / 100.0,
                    _ => val,
                };
                state.checkpoint(format!("transform:{active}:{property}"));
                let mut current = state.current.borrow_mut();
                if let Some(layer) = current.layers.get_mut(active) {
                    layer.add_keyframe(property, 0.0, stored_value);
                    let layer_name = layer.name.clone();
                    drop(current);
                    refresh_motion(&app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Updated {layer_name} {property} to {val:.1}"
                    )));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                let changed = {
                    let mut current = state.current.borrow_mut();
                    state.history.borrow_mut().undo(&mut current)
                };
                if changed {
                    refresh_motion(&app, &state);
                    app.set_status_left("Undid composition edit".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                let changed = {
                    let mut current = state.current.borrow_mut();
                    state.history.borrow_mut().redo(&mut current)
                };
                if changed {
                    refresh_motion(&app, &state);
                    app.set_status_left("Redid composition edit".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_frame(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&export_request(&state)) {
                    Ok(Some(path)) => match write_svg_frame(&state.current.borrow(), &path) {
                        Ok(()) => set_status(&app, format!("Exported {}", path.display())),
                        Err(error) => set_status(&app, format!("SVG frame export failed: {error}")),
                    },
                    Ok(None) => set_status(&app, "Export cancelled"),
                    Err(error) => set_status(&app, format!("Export dialog failed: {error}")),
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_curve_drawer(move || {
            if let Some(app) = app_ref.upgrade() {
                let open = app.get_curve_drawer_open();
                app.set_status_left(SharedString::from(format!("Keyframe drawer open: {open}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_motion(&app, &state, false) {
                    set_status(&app, format!("Save failed: {error}"));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_motion(&app, &state, true) {
                    set_status(&app, format!("Save As failed: {error}"));
                }
            }
        });
    }

    wire_palette(&app);

    refresh_motion(&app, &state);
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}

/// Connect the command-palette callbacks. Invocation dispatches through the
/// same application callbacks as the toolbar, so palette and toolbar behave
/// identically, and the query model stays in Rust for testability.
fn wire_palette(app: &MotionApp) {
    {
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_move(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let count = app.get_palette_commands().row_count() as i32;
                if count == 0 {
                    return;
                }
                let next = (app.get_palette_selected() + delta).clamp(0, count - 1);
                app.set_palette_selected(next);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.push_str(text.as_str());
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.pop();
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_palette_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let command = master_palette(&app)
                    .into_iter()
                    .filter(|c| match c.action {
                        PaletteAction::Undo => app.get_can_undo(),
                        PaletteAction::Redo => app.get_can_redo(),
                        _ => true,
                    })
                    .filter(|c| {
                        let q = app.get_palette_query().trim().to_lowercase();
                        q.is_empty()
                            || c.label.to_lowercase().contains(&q)
                            || c.id.to_lowercase().contains(&q)
                    })
                    .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewComp => app.invoke_new_comp(),
                        PaletteAction::OpenComp => app.invoke_open_comp(),
                        PaletteAction::SaveComp => app.invoke_save_comp(),
                        PaletteAction::SaveAsComp => app.invoke_save_as_comp(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::ExportFrame => app.invoke_export_frame(),
                        PaletteAction::AddLayer => app.invoke_add_layer(),
                        PaletteAction::PlayPause => {
                            app.set_is_playing(!app.get_is_playing());
                            app.invoke_play_pause();
                        }
                        PaletteAction::StepBack => app.invoke_step_back(),
                        PaletteAction::StepForward => app.invoke_step_forward(),
                        PaletteAction::ToggleLoop => {
                            app.set_is_looping(!app.get_is_looping());
                            app.invoke_toggle_loop();
                        }
                        PaletteAction::ToggleCurveDrawer => {
                            app.set_curve_drawer_open(!app.get_curve_drawer_open());
                            app.invoke_toggle_curve_drawer();
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod product_tests {
    use super::*;
    use loom_desktop::ScriptedFileDialogs;

    fn test_state_with_dialogs(
        dialogs: Rc<dyn FileDialogService>,
        save_path: Option<PathBuf>,
    ) -> GuiState {
        GuiState {
            current: RefCell::new(empty_motion()),
            history: RefCell::new(MotionHistory::default()),
            save_path: RefCell::new(save_path),
            dialogs,
            composition_filter: FileFilter::new("Loom Motion composition", ["loommotion"])
                .expect("filter"),
            svg_filter: FileFilter::new("SVG image", ["svg"]).expect("filter"),
        }
    }

    fn test_state() -> GuiState {
        test_state_with_dialogs(
            Rc::new(ScriptedFileDialogs::default()),
            Some(PathBuf::from("projects/demo.loommotion")),
        )
    }

    #[test]
    fn new_motion_composition_is_blank() {
        let document = empty_motion();
        assert_eq!(document.name, "Untitled Composition");
        assert!(document.layers.is_empty());
    }

    #[test]
    fn dialog_requests_use_current_directory_and_expected_extensions() {
        let state = test_state();
        let open = open_request(&state);
        let save = save_request(&state);
        let export = export_request(&state);

        assert_eq!(open.initial_directory, Some(PathBuf::from("projects")));
        assert_eq!(open.filters[0].extensions, vec!["loommotion".to_string()]);
        assert_eq!(save.suggested_name.as_deref(), Some("demo.loommotion"));
        assert_eq!(export.suggested_name.as_deref(), Some(EXPORT_FILENAME));
        assert_eq!(export.filters[0].extensions, vec!["svg".to_string()]);
    }

    #[test]
    fn motion_path_round_trip_preserves_composition() {
        let path = std::env::temp_dir().join(format!(
            "loom-motion-roundtrip-{}.loommotion",
            std::process::id()
        ));
        let mut document = empty_motion();
        document.width = 3840;
        document.height = 2160;
        document.frame_rate = 23.976;
        document.duration_secs = 37.5;
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.start_time = 1.25;
        layer.duration = 12.0;
        layer.add_keyframe("x", 0.0, 120.0);
        layer.add_keyframe("x", 2.0, 840.0);
        layer.add_keyframe("rotation", 1.0, 33.0);
        document.add_layer(layer);
        document.active_layer_index = 0;

        let bytes = save_motion(&document).expect("serialize");
        std::fs::write(&path, bytes).expect("write");
        let loaded = load_motion_path(&path).expect("load");
        let loaded_again = load_motion_path(&path).expect("repeated load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded, document);
        assert_eq!(loaded_again, loaded);
    }

    #[test]
    fn save_then_save_as_changes_path_and_preserves_each_version() {
        let first = std::env::temp_dir().join(format!(
            "loom-motion-save-{}-first.loommotion",
            std::process::id()
        ));
        let second = std::env::temp_dir().join(format!(
            "loom-motion-save-{}-second.loommotion",
            std::process::id()
        ));
        let dialogs: Rc<dyn FileDialogService> = Rc::new(ScriptedFileDialogs::new(
            std::iter::empty::<Option<PathBuf>>(),
            [Some(first.clone()), Some(second.clone())],
        ));
        let state = test_state_with_dialogs(dialogs, None);
        state
            .current
            .borrow_mut()
            .add_layer(MotionLayer::new("layer-a", "Layer A", "VectorShape"));
        let first_document = state.current.borrow().clone();

        let first_result = persist_current_motion(&state, false)
            .expect("first save")
            .expect("first save cancelled");
        assert_eq!(first_result.0, first);
        assert_eq!(*state.save_path.borrow(), Some(first.clone()));

        state
            .current
            .borrow_mut()
            .add_layer(MotionLayer::new("layer-b", "Layer B", "Text"));
        let second_document = state.current.borrow().clone();
        let second_result = persist_current_motion(&state, true)
            .expect("Save As")
            .expect("Save As cancelled");
        assert_eq!(second_result.0, second);
        assert_eq!(*state.save_path.borrow(), Some(second.clone()));
        assert_eq!(
            load_motion_path(&first).expect("load first"),
            first_document
        );
        assert_eq!(
            load_motion_path(&second).expect("load second"),
            second_document
        );

        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(second);
    }

    #[test]
    fn cancelled_save_and_open_leave_state_untouched() {
        let dialogs: Rc<dyn FileDialogService> = Rc::new(ScriptedFileDialogs::new([None], [None]));
        let state = test_state_with_dialogs(dialogs, None);
        state
            .current
            .borrow_mut()
            .add_layer(MotionLayer::new("layer", "Layer", "Text"));
        state.checkpoint("before-cancel");
        let before_document = state.current.borrow().clone();
        let before_path = state.save_path.borrow().clone();
        let before_history = {
            let history = state.history.borrow();
            (history.undo.len(), history.redo.len())
        };

        assert!(persist_current_motion(&state, false)
            .expect("cancelled save")
            .is_none());
        assert!(selected_motion_to_open(&state)
            .expect("cancelled open")
            .is_none());
        assert_eq!(*state.current.borrow(), before_document);
        assert_eq!(*state.save_path.borrow(), before_path);
        let after_history = state.history.borrow();
        assert_eq!(
            (after_history.undo.len(), after_history.redo.len()),
            before_history
        );
    }

    #[test]
    fn open_replaces_document_and_resets_history_boundary() {
        let path = std::env::temp_dir().join(format!(
            "loom-motion-open-{}.loommotion",
            std::process::id()
        ));
        let mut opened = empty_motion();
        opened.name = "Opened Composition".into();
        opened.add_layer(MotionLayer::new("opened-layer", "Opened Layer", "Text"));
        std::fs::write(
            &path,
            save_motion(&opened).expect("serialize opened document"),
        )
        .expect("write opened document");

        let dialogs: Rc<dyn FileDialogService> = Rc::new(ScriptedFileDialogs::new(
            [Some(path.clone())],
            std::iter::empty::<Option<PathBuf>>(),
        ));
        let state = test_state_with_dialogs(dialogs, None);
        state.checkpoint("old-document");
        state
            .current
            .borrow_mut()
            .add_layer(MotionLayer::new("stale", "Stale", "VectorShape"));

        let (opened_path, document) = selected_motion_to_open(&state)
            .expect("open selection")
            .expect("open cancelled");
        state.replace(document);
        *state.save_path.borrow_mut() = Some(opened_path.clone());

        assert_eq!(*state.current.borrow(), opened);
        assert_eq!(*state.save_path.borrow(), Some(path.clone()));
        let history = state.history.borrow();
        assert!(history.undo.is_empty());
        assert!(history.redo.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_and_malformed_projects_return_actionable_errors() {
        let missing = std::env::temp_dir().join(format!(
            "loom-motion-missing-{}.loommotion",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing);
        let missing_error = load_motion_path(&missing).expect_err("missing project loaded");
        assert!(missing_error.contains("failed to read motion composition"));

        let malformed = std::env::temp_dir().join(format!(
            "loom-motion-malformed-{}.loommotion",
            std::process::id()
        ));
        std::fs::write(&malformed, b"not a Loom Motion package").expect("write malformed");
        let malformed_error = load_motion_path(&malformed).expect_err("malformed project loaded");
        assert!(!malformed_error.trim().is_empty());
        let _ = std::fs::remove_file(malformed);
    }

    #[test]
    fn read_only_destination_reports_failure_without_changing_active_path() {
        let path = std::env::temp_dir().join(format!(
            "loom-motion-read-only-{}.loommotion",
            std::process::id()
        ));
        std::fs::write(&path, b"existing").expect("create destination");
        let original_permissions = std::fs::metadata(&path)
            .expect("destination metadata")
            .permissions();
        let mut read_only = original_permissions.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&path, read_only).expect("set read-only");

        let state =
            test_state_with_dialogs(Rc::new(ScriptedFileDialogs::default()), Some(path.clone()));
        let result = persist_current_motion(&state, false);

        std::fs::set_permissions(&path, original_permissions).expect("restore permissions");
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_err(),
            "read-only destination unexpectedly accepted"
        );
        assert_eq!(*state.save_path.borrow(), Some(path));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn non_utf8_path_round_trip_is_supported() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let mut name = format!("loom-motion-non-utf8-{}-", std::process::id()).into_bytes();
        name.push(0xff);
        name.extend_from_slice(b".loommotion");
        let path = std::env::temp_dir().join(OsString::from_vec(name));
        let state =
            test_state_with_dialogs(Rc::new(ScriptedFileDialogs::default()), Some(path.clone()));
        state
            .current
            .borrow_mut()
            .add_layer(MotionLayer::new("layer", "Layer", "Text"));
        let expected = state.current.borrow().clone();

        persist_current_motion(&state, false)
            .expect("save non-UTF-8 path")
            .expect("save unexpectedly cancelled");
        assert_eq!(
            load_motion_path(&path).expect("load non-UTF-8 path"),
            expected
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn svg_frame_export_contains_sampled_layers() {
        let svg = export_svg_frame(&sample_motion(), 0.0);
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Subtitle Motion"));
        assert!(svg.ends_with(
            "</svg>
"
        ));
    }

    #[test]
    fn motion_history_undoes_and_redoes_edits() {
        let mut current = sample_motion();
        let original = current.clone();
        let mut history = MotionHistory::default();
        history.checkpoint(&current, "add-layer");
        current.add_layer(MotionLayer::new("extra", "Extra", "VectorShape"));
        assert!(history.undo(&mut current));
        assert_eq!(current.layers.len(), original.layers.len());
        assert!(history.redo(&mut current));
        assert_eq!(current.layers.len(), original.layers.len() + 1);
    }
}
