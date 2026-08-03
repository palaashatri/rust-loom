//! Loom Motion desktop application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_motion_core::{load_motion, save_motion, CompositionDocument, MotionLayer};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "comp.loommotion";
const EXPORT_FILENAME: &str = "composition-frame.svg";
const HISTORY_LIMIT: usize = 128;

loom_production::define_snapshot_recovery!(MOTION_RECOVERY, "org.loom.motion", "loom.motion/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        size: DEFAULT_SIZE,
        theme: "dark".to_string(),
        open: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => {
                args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?);
            }
            "--smoke" => args.smoke = true,
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

fn initial_motion(args: &Args) -> Result<CompositionDocument, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read motion composition '{path}': {e}"))?;
            load_motion(&bytes)
                .map_err(|e| format!("failed to load motion composition '{path}': {e}"))
        }
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
    std::fs::write(path, export_svg_frame(doc, 0.0)).map_err(|error| error.to_string())
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

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = initial_motion(args)?;
    apply_motion(&app, &doc);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
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

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-motion-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }

    let app = MotionApp::new().map_err(|e| e.to_string())?;
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
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                state.replace(sample_motion());
                refresh_motion(&app, &state);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|e| format!("failed to read {SAVE_FILENAME}: {e}"))
                    .and_then(|bytes| load_motion(&bytes))
                {
                    Ok(doc) => {
                        state.replace(doc);
                        refresh_motion(&app, &state);
                        app.set_status_left(SharedString::from(format!("Opened {SAVE_FILENAME}")));
                    }
                    Err(err) => {
                        app.set_status_left(SharedString::from(format!("Open failed: {err}")))
                    }
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
                match write_svg_frame(&state.current.borrow(), EXPORT_FILENAME) {
                    Ok(()) => app
                        .set_status_left(format!("Exported SVG frame to {EXPORT_FILENAME}").into()),
                    Err(error) => {
                        app.set_status_left(format!("SVG frame export failed: {error}").into())
                    }
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
                match save_motion(&state.current.borrow()) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => app
                            .set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}"))),
                        Err(error) => app.set_status_left(SharedString::from(format!(
                            "Save/checkpoint failed: {error}"
                        ))),
                    },
                    Err(error) => {
                        app.set_status_left(SharedString::from(format!("Save failed: {error}")))
                    }
                }
            }
        });
    }

    refresh_motion(&app, &state);
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod product_tests {
    use super::*;

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
