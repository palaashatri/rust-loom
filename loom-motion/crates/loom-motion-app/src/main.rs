//! Loom Motion desktop application.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use loom_desktop::{
    build_standard_menu_bar, FileDialogService, FileFilter, Menu, MenuBarService, MenuItem,
    MenuShortcut, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_motion_core::{
    load_motion, save_motion, CompositionClock, CompositionDocument, MotionLayer,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "comp.loommotion";
const EXPORT_FILENAME: &str = "composition-frame.svg";
const HISTORY_LIMIT: usize = 128;

loom_production::define_snapshot_recovery!(MOTION_RECOVERY, "org.loom.motion", "loom.motion/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    rtl: bool,
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
        rtl: false,
        open: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => {
                args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?);
            }
            "--smoke" => {
                args.smoke = true;
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
            "--rtl" => args.rtl = true,
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
        if !sample.visible {
            continue;
        }
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

fn write_svg_frame(
    doc: &CompositionDocument,
    path: impl AsRef<Path>,
    time_secs: f32,
) -> Result<(), String> {
    let svg = export_svg_frame(doc, time_secs);
    loom_storage::atomic_write(path.as_ref(), svg.as_bytes()).map_err(|error| error.to_string())
}

fn format_timecode(frame: u64, fps: f64) -> String {
    let frames_per_second = if fps.is_finite() && fps > 0.0 {
        fps.round().max(1.0) as u64
    } else {
        60
    };
    let total_seconds = frame / frames_per_second;
    let frame_in_second = frame % frames_per_second;
    let seconds = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3_600;
    format!("{hours:02}:{minutes:02}:{seconds:02}:{frame_in_second:02}")
}

#[allow(dead_code)]
fn clock_for_document(doc: &CompositionDocument) -> CompositionClock {
    CompositionClock::new(doc.frame_rate as f64, doc.duration_frames())
}

fn document_duration_secs(doc: &CompositionDocument) -> f32 {
    if doc.duration_secs.is_finite() {
        doc.duration_secs.max(0.0)
    } else {
        0.0
    }
}

fn quantized_frame_time(doc: &CompositionDocument, frame: u64) -> f32 {
    let fps = doc.frame_rate;
    if !fps.is_finite() || fps <= 0.0 {
        return 0.0;
    }
    let last_frame = doc.duration_frames().saturating_sub(1);
    frame.min(last_frame) as f32 / fps
}

/// Inserts or replaces one transform key at a document frame. MotionLayer
/// tracks use layer-local time, so the composition frame is rebased by the
/// layer's start time before storage.
fn edit_transform_at_frame(
    doc: &mut CompositionDocument,
    frame: u64,
    property: &str,
    value: f32,
) -> bool {
    if !value.is_finite() || !matches!(property, "x" | "y" | "scale" | "rotation" | "opacity") {
        return false;
    }
    let active = doc.active_layer_index;
    let frame_time = quantized_frame_time(doc, frame);
    let Some(layer) = doc.layers.get_mut(active) else {
        return false;
    };
    let start_time = layer.start_time;
    let layer_duration = layer.duration;
    if !start_time.is_finite()
        || start_time < 0.0
        || !layer_duration.is_finite()
        || layer_duration < 0.0
    {
        return false;
    }
    let local_time = frame_time - layer.start_time;
    if !local_time.is_finite() || local_time < 0.0 || local_time > layer_duration {
        return false;
    }
    layer.add_keyframe(property, local_time, value);
    true
}

fn selected_layer_keyframe_markers(doc: &CompositionDocument) -> Vec<(f32, String)> {
    let Some(layer) = doc.layers.get(doc.active_layer_index) else {
        return Vec::new();
    };
    let duration = document_duration_secs(doc);
    let start = layer.start_time;
    let layer_duration = layer.duration;
    if !start.is_finite() || start < 0.0 || !layer_duration.is_finite() || layer_duration < 0.0 {
        return Vec::new();
    }
    let mut markers = Vec::new();
    for (property, keys) in [
        ("x", &layer.position_x_keys),
        ("y", &layer.position_y_keys),
        ("scale", &layer.scale_keys),
        ("rotation", &layer.rotation_keys),
        ("opacity", &layer.opacity_keys),
    ] {
        for key in keys {
            let time = start + key.time_secs;
            if key.time_secs.is_finite()
                && key.time_secs >= 0.0
                && key.time_secs <= layer_duration
                && time.is_finite()
                && time >= 0.0
                && time <= duration
            {
                markers.push((time, property.to_string()));
            }
        }
    }
    markers.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    markers
}

#[allow(dead_code)]
fn selected_layer_keyframes(doc: &CompositionDocument) -> Vec<f32> {
    let mut times = selected_layer_keyframe_markers(doc)
        .into_iter()
        .map(|(time, _)| time)
        .collect::<Vec<_>>();
    times.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
    times
}

fn keyframe_exists_at(doc: &CompositionDocument, selected: &SelectedKeyframe) -> bool {
    if doc.layers.get(doc.active_layer_index).is_none() {
        return false;
    }
    let Some((time, _)) =
        selected_layer_keyframe_markers(doc)
            .into_iter()
            .find(|(time, property)| {
                property == &selected.property && (*time - selected.time_secs).abs() <= f32::EPSILON
            })
    else {
        return false;
    };
    time.is_finite()
}

type SampledLayerArrays = (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<bool>);

fn sampled_layer_arrays(doc: &CompositionDocument, time_secs: f32) -> SampledLayerArrays {
    let mut x = Vec::with_capacity(doc.layers.len());
    let mut y = Vec::with_capacity(doc.layers.len());
    let mut scale = Vec::with_capacity(doc.layers.len());
    let mut rotation = Vec::with_capacity(doc.layers.len());
    let mut opacity = Vec::with_capacity(doc.layers.len());
    let mut visible = Vec::with_capacity(doc.layers.len());
    for layer in &doc.layers {
        let sample = layer.sample(time_secs);
        x.push(if sample.x.is_finite() { sample.x } else { 0.0 });
        y.push(if sample.y.is_finite() { sample.y } else { 0.0 });
        scale.push(if sample.scale.is_finite() {
            sample.scale.max(0.0)
        } else {
            1.0
        });
        rotation.push(if sample.rotation.is_finite() {
            sample.rotation
        } else {
            0.0
        });
        opacity.push(if sample.opacity.is_finite() {
            sample.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        });
        visible.push(sample.visible);
    }
    (x, y, scale, rotation, opacity, visible)
}

fn move_keyframe_at_time(
    doc: &mut CompositionDocument,
    property: &str,
    from_absolute: f32,
    to_absolute: f32,
) -> bool {
    if !from_absolute.is_finite() || !to_absolute.is_finite() {
        return false;
    }
    let document_duration = document_duration_secs(doc);
    if to_absolute < 0.0 || to_absolute > document_duration {
        return false;
    }
    let target_absolute = to_absolute;
    let Some(layer) = doc.layers.get_mut(doc.active_layer_index) else {
        return false;
    };
    if !layer.start_time.is_finite()
        || layer.start_time < 0.0
        || !layer.duration.is_finite()
        || layer.duration < 0.0
    {
        return false;
    }
    let target_local = target_absolute - layer.start_time;
    if target_local < 0.0 || target_local > layer.duration {
        return false;
    }
    let from_local = from_absolute - layer.start_time;
    if from_local < 0.0 || from_local > layer.duration {
        return false;
    }
    layer.move_keyframe(property, from_local, target_local)
}

fn layer_timing(doc: &CompositionDocument) -> (Vec<f32>, Vec<f32>) {
    doc.layers
        .iter()
        .map(|layer| {
            let start = if layer.start_time.is_finite() {
                layer.start_time.max(0.0)
            } else {
                0.0
            };
            let duration = if layer.duration.is_finite() {
                layer.duration.max(0.0)
            } else {
                0.0
            };
            (start, duration)
        })
        .unzip()
}

fn apply_motion_at(app: &MotionApp, doc: &CompositionDocument, time_secs: f32, frame: u64) {
    let duration = document_duration_secs(doc);
    let time_secs = if time_secs.is_finite() {
        time_secs.clamp(0.0, duration)
    } else {
        0.0
    };
    let (layer_starts, layer_durations) = layer_timing(doc);
    let (layer_pos_x, layer_pos_y, layer_scale, layer_rotation, layer_opacity, layer_visible) =
        sampled_layer_arrays(doc, time_secs);
    let fps = if doc.frame_rate.is_finite() && doc.frame_rate > 0.0 {
        doc.frame_rate
    } else {
        60.0
    };
    app.set_comp_name(doc.name.as_str().into());
    app.set_timecode_text(SharedString::from(format!(
        "00:00:00:00 ({} fps • {:.0}s)",
        fps, duration
    )));
    app.set_timecode_display(format_timecode(frame, fps as f64).into());
    app.set_layer_start_secs(ModelRc::new(VecModel::from(layer_starts)));
    app.set_layer_duration_secs(ModelRc::new(VecModel::from(layer_durations)));
    app.set_layer_pos_x(ModelRc::new(VecModel::from(layer_pos_x)));
    app.set_layer_pos_y(ModelRc::new(VecModel::from(layer_pos_y)));
    app.set_layer_scale(ModelRc::new(VecModel::from(layer_scale)));
    app.set_layer_rotation(ModelRc::new(VecModel::from(layer_rotation)));
    app.set_layer_opacity(ModelRc::new(VecModel::from(layer_opacity)));
    app.set_layer_visible(ModelRc::new(VecModel::from(layer_visible)));
    app.set_frame_rate(fps);
    let layer_labels: Vec<SharedString> = doc
        .layers
        .iter()
        .map(|layer| SharedString::from(format!("{} ({})", layer.name, layer.layer_type)))
        .collect();
    let layer_types: Vec<SharedString> = doc
        .layers
        .iter()
        .map(|layer| SharedString::from(layer.layer_type.clone()))
        .collect();
    app.set_layer_labels(ModelRc::new(VecModel::from(layer_labels)));
    app.set_layer_types(ModelRc::new(VecModel::from(layer_types)));
    let selected = doc
        .layers
        .get(doc.active_layer_index)
        .map(|layer| layer.name.as_str())
        .unwrap_or("No layer selected");
    app.set_active_layer_index(doc.active_layer_index as i32);
    if let Some(layer) = doc.layers.get(doc.active_layer_index) {
        let sample = layer.sample(time_secs);
        app.set_pos_x(sample.x);
        app.set_pos_y(sample.y);
        app.set_scale_val(sample.scale * 100.0);
        app.set_rotation_val(sample.rotation);
        app.set_opacity_val(sample.opacity * 100.0);
    } else {
        app.set_pos_x(960.0);
        app.set_pos_y(540.0);
        app.set_scale_val(100.0);
        app.set_rotation_val(0.0);
        app.set_opacity_val(100.0);
    }
    let markers = selected_layer_keyframe_markers(doc);
    app.set_active_layer_keyframes(ModelRc::new(VecModel::from(
        markers.iter().map(|(time, _)| *time).collect::<Vec<_>>(),
    )));
    app.set_active_keyframe_properties(ModelRc::new(VecModel::from(
        markers
            .iter()
            .map(|(_, property)| SharedString::from(property.as_str()))
            .collect::<Vec<_>>(),
    )));
    app.set_duration_secs(duration);
    app.set_current_time_secs(time_secs);
    app.set_status_left(SharedString::from(format!(
        "{} motion layers • Selected: {selected}",
        doc.len()
    )));
    app.set_status_right("Offline".into());
}

fn apply_motion(app: &MotionApp, doc: &CompositionDocument) {
    apply_motion_at(app, doc, 0.0, 0);
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
    Zoom,
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
            action: PaletteAction::Zoom,
            id: "motion.zoom",
            label: "Zoom",
            shortcut: "",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResponsiveToolbarState {
    icon_only: bool,
    overflow: bool,
    labeled: bool,
}

fn responsive_toolbar_state(app: &MotionApp, width: u32) -> ResponsiveToolbarState {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32;
    ResponsiveToolbarState {
        icon_only: width < policy.get_priority_1_icon_only_below(),
        overflow: width < policy.get_priority_2_overflow_below(),
        labeled: width >= policy.get_priority_2_overflow_below(),
    }
}

#[cfg(test)]
fn compact_layout_for_width(app: &MotionApp, width: u32) -> bool {
    responsive_toolbar_state(app, width).icon_only
}

fn configure_responsive_layout(app: &MotionApp, width: u32) {
    let state = responsive_toolbar_state(app, width);
    app.set_compact_layout(state.icon_only);
    app.set_icon_only_toolbar(state.icon_only);
    app.set_overflow_toolbar(state.overflow);
    app.set_labeled_toolbar(state.labeled);
}

fn configure_direction(app: &MotionApp, rtl: bool) {
    app.set_rtl(rtl);
}

fn wire_responsive_layout(app: &MotionApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_layout(&app, width.max(0.0) as u32);
        }
    });
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
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

fn capture_motion_journey_step(
    app: &MotionApp,
    state: &GuiState,
    args: &Args,
    out_dir: &Path,
    index: usize,
    name: &str,
) -> Result<serde_json::Value, String> {
    let image = snapshot_component(app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let screenshot = format!("motion-journey-{index:02}-{name}.png");
    loom_test_support::png::save_png(&out_dir.join(&screenshot), &image)
        .map_err(|error| format!("save {screenshot}: {error}"))?;
    let clock = state.clock.borrow();
    let selected = state.selected_keyframe.borrow().clone();
    Ok(serde_json::json!({
        "name": name,
        "frame": clock.current_frame,
        "time_secs": clock.current_time_seconds(),
        "is_playing": clock.is_playing,
        "selected_keyframe": selected.map(|keyframe| serde_json::json!({
            "property": keyframe.property,
            "time_secs": keyframe.time_secs,
        })),
        "layers": state.current.borrow().len(),
        "screenshot": screenshot,
    }))
}

/// Record a deterministic controller-backed in-app journey.  The scripted
/// desktop service supplies save/open/cancel/failure responses while every
/// screenshot is rendered from the real MotionApp component.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir).map_err(|error| format!("create journey output: {error}"))?;
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let save_path = std::env::temp_dir().join(format!(
        "loom-motion-journey-{}.loommotion",
        std::process::id()
    ));
    let export_path =
        std::env::temp_dir().join(format!("loom-motion-journey-{}.svg", std::process::id()));
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(save_path.clone()), None],
        [Some(save_path.clone()), Some(std::env::temp_dir())],
    ));
    let mut document = initial_motion(args)?;
    if let Some(layer) = document.layers.first_mut() {
        layer.add_keyframe("x", 1.0, 1_040.0);
    }
    let initial_clock = clock_for_document(&document);
    let state = GuiState {
        current: RefCell::new(document),
        clock: RefCell::new(initial_clock),
        history: RefCell::new(MotionHistory::default()),
        selected_keyframe: RefCell::new(None),
        transform_gesture_active: RefCell::new(false),
        transform_gesture_checkpointed: RefCell::new(false),
        save_path: RefCell::new(None),
        dialogs,
        composition_filter: FileFilter::new("Loom Motion composition", ["loommotion"])
            .map_err(|error| error.to_string())?,
        svg_filter: FileFilter::new("SVG image", ["svg"]).map_err(|error| error.to_string())?,
    };
    // Keep transport and sampled state aligned with the edited document.
    *state.clock.borrow_mut() = clock_for_document(&state.current.borrow());
    apply_motion(&app, &state.current.borrow());
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let mut steps = Vec::new();
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 0, "initial",
    )?);

    let selected = selected_layer_keyframe_markers(&state.current.borrow())
        .into_iter()
        .find(|(_, property)| property == "x")
        .ok_or("journey fixture has no x keyframe")?;
    *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
        property: selected.1,
        time_secs: selected.0,
    });
    state.clock.borrow_mut().seek_seconds(selected.0 as f64);
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        1,
        "select-keyframe",
    )?);

    state.checkpoint("journey-keyframe-time");
    let moved_time = selected.0 + 0.5;
    if !move_keyframe_at_time(&mut state.current.borrow_mut(), "x", selected.0, moved_time) {
        return Err("journey keyframe timing edit was rejected".into());
    }
    *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
        property: "x".into(),
        time_secs: moved_time,
    });
    state.clock.borrow_mut().seek_seconds(moved_time as f64);
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        2,
        "edit-keyframe",
    )?);

    state.clock.borrow_mut().set_playing(true);
    state.clock.borrow_mut().advance_seconds(0.5);
    state.clock.borrow_mut().set_playing(false);
    state.clock.borrow_mut().seek_seconds(2.0);
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        3,
        "play-seek",
    )?);

    // Exercise the same validated mutation path used by the production
    // `on_transform_changed` callback. Calling the generated callback here
    // would only dispatch to listeners registered on the app component; the
    // journey owns a standalone GuiState, so invoke the controller operation
    // directly and assert that it changed the sampled document.
    let (transform_frame, transform_time, active_layer, previous_transform_y) = {
        let clock = state.clock.borrow();
        let current = state.current.borrow();
        let active_layer = current.active_layer_index;
        let transform_time = clock.current_time_seconds() as f32;
        let previous_transform_y = current
            .layers
            .get(active_layer)
            .map(|layer| layer.sample(transform_time).y)
            .ok_or("journey transform fixture has no active layer")?;
        (
            clock.current_frame,
            transform_time,
            active_layer,
            previous_transform_y,
        )
    };
    if !previous_transform_y.is_finite() || (previous_transform_y - 250.0).abs() <= f32::EPSILON {
        return Err("journey transform fixture has no distinct pre-edit y value".into());
    }
    apply_transform_edit(&state, transform_frame, "y", 250.0)?;
    let edited_transform_y = state
        .current
        .borrow()
        .layers
        .get(active_layer)
        .map(|layer| layer.sample(transform_time).y)
        .ok_or("journey transform edit removed the active layer")?;
    if !edited_transform_y.is_finite() || (edited_transform_y - 250.0).abs() > 0.001 {
        return Err(format!(
            "journey transform edit did not mutate y (sampled {edited_transform_y:.3})"
        ));
    }
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        4,
        "edit-transform",
    )?);

    if !state
        .history
        .borrow_mut()
        .undo(&mut state.current.borrow_mut())
    {
        return Err("journey undo did not revert the transform edit".into());
    }
    let undone_transform_y = state
        .current
        .borrow()
        .layers
        .get(active_layer)
        .map(|layer| layer.sample(transform_time).y)
        .ok_or("journey transform undo removed the active layer")?;
    if !undone_transform_y.is_finite() || (undone_transform_y - previous_transform_y).abs() > 0.001
    {
        return Err(format!(
            "journey transform undo did not restore y (sampled {undone_transform_y:.3}, expected {previous_transform_y:.3})"
        ));
    }
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        5,
        "undo-transform",
    )?);

    if !state
        .history
        .borrow_mut()
        .undo(&mut state.current.borrow_mut())
    {
        return Err("journey undo did not revert the timing edit".into());
    }
    *state.selected_keyframe.borrow_mut() = None;
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 6, "undo",
    )?);
    if !state
        .history
        .borrow_mut()
        .redo(&mut state.current.borrow_mut())
    {
        return Err("journey redo did not restore the timing edit".into());
    }
    *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
        property: "x".into(),
        time_secs: moved_time,
    });
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 7, "redo",
    )?);

    if !state
        .history
        .borrow_mut()
        .redo(&mut state.current.borrow_mut())
    {
        return Err("journey redo did not restore the transform edit".into());
    }
    let redone_transform_y = state
        .current
        .borrow()
        .layers
        .get(active_layer)
        .map(|layer| layer.sample(transform_time).y)
        .ok_or("journey transform redo removed the active layer")?;
    if !redone_transform_y.is_finite() || (redone_transform_y - 250.0).abs() > 0.001 {
        return Err(format!(
            "journey transform redo did not restore y (sampled {redone_transform_y:.3})"
        ));
    }
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        8,
        "redo-transform",
    )?);

    let saved =
        persist_current_motion(&state, false)?.ok_or("journey save unexpectedly cancelled")?;
    let saved_document = state.current.borrow().clone();
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 9, "save",
    )?);

    let (opened_path, reopened) =
        selected_motion_to_open(&state)?.ok_or("journey reopen unexpectedly cancelled")?;
    if reopened != saved_document || opened_path != saved.0 {
        return Err("journey save/reopen did not preserve the document".into());
    }
    state.replace(reopened)?;
    *state.save_path.borrow_mut() = Some(opened_path);
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 10, "reopen",
    )?);

    write_svg_frame(
        &state.current.borrow(),
        &export_path,
        state.clock.borrow().current_time_seconds() as f32,
    )?;
    let exported = std::fs::read_to_string(&export_path)
        .map_err(|error| format!("read journey export: {error}"))?;
    if !exported.contains("<svg") {
        return Err("journey export did not produce SVG".into());
    }
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 11, "export",
    )?);

    let cancelled = selected_motion_to_open(&state)?.is_none();
    if !cancelled {
        return Err("journey cancellation response was not observed".into());
    }
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 12, "cancel",
    )?);

    let failure = persist_current_motion(&state, true).expect_err("journey failure was accepted");
    if !failure.contains("failed to atomic write") {
        return Err(format!("journey failure was not actionable: {failure}"));
    }
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 13, "failure",
    )?);

    let report = serde_json::json!({
        "app": "motion",
        "journey": "keyframe-selection-edit-play-seek-transform-undo-redo-save-reopen-export-cancel-failure",
        "passed": true,
        "notes": ["Controller-backed deterministic journey with scripted desktop dialogs; screenshots are rendered from the real MotionApp component."],
        "steps": steps,
    });
    std::fs::write(
        out_dir.join("motion.json"),
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write journey report: {error}"))?;
    let _ = std::fs::remove_file(save_path);
    let _ = std::fs::remove_file(export_path);
    println!("motion journey: PASS ({})", out_dir.display());
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
    clock: RefCell<CompositionClock>,
    history: RefCell<MotionHistory>,
    selected_keyframe: RefCell<Option<SelectedKeyframe>>,
    transform_gesture_active: RefCell<bool>,
    transform_gesture_checkpointed: RefCell<bool>,
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    composition_filter: FileFilter,
    svg_filter: FileFilter,
}

#[derive(Debug, Clone, PartialEq)]
struct SelectedKeyframe {
    property: String,
    time_secs: f32,
}

impl GuiState {
    fn checkpoint(&self, key: impl Into<String>) {
        let current = self.current.borrow();
        self.history.borrow_mut().checkpoint(&current, key);
    }

    fn record_recovery(&self) -> Result<(), String> {
        let current = self.current.borrow();
        let bytes = save_motion(&current)?;
        record_snapshot_recovery("motion state", bytes)
    }

    fn replace(&self, document: CompositionDocument) -> Result<(), String> {
        *self.current.borrow_mut() = document;
        *self.clock.borrow_mut() = clock_for_document(&self.current.borrow());
        *self.history.borrow_mut() = MotionHistory::default();
        *self.selected_keyframe.borrow_mut() = None;
        *self.transform_gesture_active.borrow_mut() = false;
        *self.transform_gesture_checkpointed.borrow_mut() = false;
        self.record_recovery()
    }
}

/// Applies a timing edit only after a cloned document proves the move is
/// valid. The history checkpoint is recorded against the untouched current
/// document, after validation/mutation on the clone, so rejected edits cannot
/// leave an undo entry behind.
fn apply_keyframe_time_edit(
    state: &GuiState,
    selected: &SelectedKeyframe,
    property: &str,
    new_time: f32,
) -> Result<f32, String> {
    if selected.property != property || !new_time.is_finite() {
        return Err("Keyframe timing edit is invalid".into());
    }
    let (target, updated) = {
        let current = state.current.borrow();
        let target = new_time.clamp(0.0, document_duration_secs(&current));
        if !keyframe_exists_at(&current, selected) {
            return Err("Selected keyframe is no longer available".into());
        }
        let mut updated = current.clone();
        if !move_keyframe_at_time(
            &mut updated,
            selected.property.as_str(),
            selected.time_secs,
            target,
        ) {
            return Err("Keyframe time must stay inside the layer interval".into());
        }
        (target, updated)
    };

    state.checkpoint(format!(
        "keyframe-time:{}:{}",
        state.current.borrow().active_layer_index,
        selected.property
    ));
    *state.current.borrow_mut() = updated;
    Ok(target)
}

/// Applies a transform edit through a validated document clone. History is
/// checkpointed only after the clone accepts the edit, which keeps rejected
/// layer-interval edits out of undo while still coalescing one pointer drag.
fn apply_transform_edit(
    state: &GuiState,
    frame: u64,
    property: &str,
    value: f32,
) -> Result<String, String> {
    if !value.is_finite() || !matches!(property, "x" | "y" | "scale" | "rotation" | "opacity") {
        return Err("Transform value must be finite".into());
    }
    let active = state.current.borrow().active_layer_index;
    let (updated, layer_name) = {
        let current = state.current.borrow();
        let mut updated = current.clone();
        if !edit_transform_at_frame(&mut updated, frame, property, value) {
            return Err("Transform edit must fall inside the selected layer interval".into());
        }
        let layer_name = updated
            .layers
            .get(active)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "Layer".to_string());
        (updated, layer_name)
    };

    let gesture_active = *state.transform_gesture_active.borrow();
    let needs_checkpoint = !gesture_active || !*state.transform_gesture_checkpointed.borrow();
    if needs_checkpoint {
        let history_key = if gesture_active {
            format!("transform-gesture:{active}")
        } else {
            format!("transform:{active}:{property}")
        };
        state.checkpoint(history_key);
        if gesture_active {
            *state.transform_gesture_checkpointed.borrow_mut() = true;
        }
    }
    *state.current.borrow_mut() = updated;
    Ok(layer_name)
}

fn report_recovery(app: &MotionApp, state: &GuiState, success: impl Into<String>) {
    let success = success.into();
    match state.record_recovery() {
        Ok(()) => set_status(app, success),
        Err(error) => set_status(app, format!("{success}; recovery snapshot failed: {error}")),
    }
}

fn refresh_motion(app: &MotionApp, state: &GuiState) {
    let doc = state.current.borrow();
    let clock = state.clock.borrow();
    let time_secs = clock.frame_to_seconds(clock.current_frame) as f32;
    apply_motion_at(app, &doc, time_secs, clock.current_frame);
    let selection_invalid = state
        .selected_keyframe
        .borrow()
        .as_ref()
        .is_some_and(|selected| !keyframe_exists_at(&doc, selected));
    if selection_invalid {
        *state.selected_keyframe.borrow_mut() = None;
    }
    let selected = state.selected_keyframe.borrow();
    if let Some(selected) = selected.as_ref() {
        app.set_selected_keyframe_property(selected.property.clone().into());
        app.set_selected_keyframe_time(selected.time_secs);
    } else {
        app.set_selected_keyframe_property("".into());
        app.set_selected_keyframe_time(0.0);
    }
    let history = state.history.borrow();
    app.set_can_undo(!history.undo.is_empty());
    app.set_can_redo(!history.redo.is_empty());
    app.set_is_playing(clock.is_playing);
    app.set_is_looping(clock.loop_playback);
    app.set_duration_secs(document_duration_secs(&doc));
    app.set_current_time_secs(time_secs);
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
    // Durably capture the exact bytes before touching the user-selected file.
    // A later file-system failure can therefore be recovered without losing
    // the edit, while a checkpoint failure never reports a successful save.
    checkpoint_snapshot_recovery(bytes.clone())
        .map_err(|error| format!("recovery checkpoint failed before save: {error}"))?;
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
    let Some((path, _bytes)) = persist_current_motion(state, force_picker)? else {
        set_status(app, "Save cancelled");
        return Ok(false);
    };

    set_status(app, format!("Saved {}", path.display()));
    Ok(true)
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
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }

    let app = MotionApp::new().map_err(|e| e.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    wire_responsive_layout(&app);

    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_motion(&args)?
    } else if let Some(bytes) = recovered.as_deref() {
        load_motion(bytes).map_err(|error| {
            format!(
                "startup recovery is present but could not be opened; save or remove the recovery data and retry: {error}"
            )
        })?
    } else {
        initial_motion(&args)?
    };
    let initial_clock = clock_for_document(&initial);
    let state = Rc::new(GuiState {
        current: RefCell::new(initial),
        clock: RefCell::new(initial_clock),
        history: RefCell::new(MotionHistory::default()),
        selected_keyframe: RefCell::new(None),
        transform_gesture_active: RefCell::new(false),
        transform_gesture_checkpointed: RefCell::new(false),
        save_path: RefCell::new(args.open.as_ref().map(PathBuf::from)),
        dialogs: Rc::new(NativeFileDialogs),
        composition_filter: FileFilter::new("Loom Motion composition", ["loommotion"])
            .map_err(|error| error.to_string())?,
        svg_filter: FileFilter::new("SVG image", ["svg"]).map_err(|error| error.to_string())?,
    });

    let startup_recovery_error = state.record_recovery().err();

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                let recovery_error = state.replace(empty_motion()).err();
                *state.save_path.borrow_mut() = None;
                refresh_motion(&app, &state);
                if let Some(error) = recovery_error {
                    set_status(
                        &app,
                        format!("New composition created, but recovery failed: {error}"),
                    );
                } else {
                    set_status(&app, "Created a new untitled composition");
                }
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
                        let recovery_error = state.replace(document).err();
                        *state.save_path.borrow_mut() = Some(path.clone());
                        refresh_motion(&app, &state);
                        if let Some(error) = recovery_error {
                            set_status(
                                &app,
                                format!(
                                    "Opened {}, but recovery snapshot failed: {error}",
                                    path.display()
                                ),
                            );
                        } else {
                            set_status(&app, format!("Opened {}", path.display()));
                        }
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
                report_recovery(&app, &state, "Layer added");
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
                    *state.selected_keyframe.borrow_mut() = None;
                    drop(current);
                    refresh_motion(&app, &state);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                let playing = state.clock.borrow_mut().toggle_playing();
                let msg = if playing {
                    "Playing motion preview..."
                } else {
                    "Paused playback"
                };
                refresh_motion(&app, &state);
                app.set_status_left(SharedString::from(msg));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_step_back(move || {
            if let Some(app) = app_ref.upgrade() {
                state.clock.borrow_mut().step_backward();
                refresh_motion(&app, &state);
                app.set_status_left(SharedString::from("Stepped back 1 frame"));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_step_forward(move || {
            if let Some(app) = app_ref.upgrade() {
                state.clock.borrow_mut().step_forward();
                refresh_motion(&app, &state);
                app.set_status_left(SharedString::from("Stepped forward 1 frame"));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_loop(move || {
            if let Some(app) = app_ref.upgrade() {
                let looping = state.clock.borrow_mut().toggle_loop_playback();
                refresh_motion(&app, &state);
                app.set_status_left(SharedString::from(format!("Loop mode: {looping}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_scrub_time(move |time_secs| {
            if let Some(app) = app_ref.upgrade() {
                // Scrubbing changes transport only; it must not merge with or
                // create an editing history entry.
                state.history.borrow_mut().break_coalescing();
                state.clock.borrow_mut().seek_seconds(time_secs as f64);
                refresh_motion(&app, &state);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_keyframe(move |time_secs, property| {
            if let Some(app) = app_ref.upgrade() {
                let property = property.to_string();
                let valid = {
                    let current = state.current.borrow();
                    selected_layer_keyframe_markers(&current)
                        .iter()
                        .any(|(time, candidate)| {
                            candidate == &property && (*time - time_secs).abs() <= f32::EPSILON
                        })
                };
                if !valid {
                    set_status(&app, "Keyframe is no longer available at that time");
                    return;
                }
                *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
                    property,
                    time_secs,
                });
                state.history.borrow_mut().break_coalescing();
                state.clock.borrow_mut().seek_seconds(time_secs as f64);
                refresh_motion(&app, &state);
                set_status(&app, format!("Selected keyframe at {time_secs:.3}s"));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_edit_keyframe_time(move |property, new_time| {
            if let Some(app) = app_ref.upgrade() {
                let Some(selected) = state.selected_keyframe.borrow().clone() else {
                    set_status(&app, "Select a keyframe before editing its time");
                    return;
                };
                match apply_keyframe_time_edit(&state, &selected, property.as_str(), new_time) {
                    Ok(target) => {
                        *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
                            property: selected.property,
                            time_secs: target,
                        });
                        state.clock.borrow_mut().seek_seconds(target as f64);
                        refresh_motion(&app, &state);
                        report_recovery(&app, &state, format!("Moved keyframe to {target:.3}s"));
                    }
                    Err(error) => set_status(&app, error),
                }
            }
        });
    }

    {
        let state = state.clone();
        app.on_transform_begin(move || {
            *state.transform_gesture_active.borrow_mut() = true;
            *state.transform_gesture_checkpointed.borrow_mut() = false;
            state.history.borrow_mut().break_coalescing();
        });
    }

    {
        let state = state.clone();
        app.on_transform_end(move || {
            *state.transform_gesture_active.borrow_mut() = false;
            *state.transform_gesture_checkpointed.borrow_mut() = false;
            state.history.borrow_mut().break_coalescing();
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_transform_changed(move |prop, val| {
            if let Some(app) = app_ref.upgrade() {
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
                let frame = state.clock.borrow().current_frame;
                match apply_transform_edit(&state, frame, property, stored_value) {
                    Ok(layer_name) => {
                        refresh_motion(&app, &state);
                        report_recovery(
                            &app,
                            &state,
                            format!("Updated {layer_name} {property} to {val:.1}"),
                        );
                    }
                    Err(error) => set_status(&app, error),
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
                    *state.selected_keyframe.borrow_mut() = None;
                    refresh_motion(&app, &state);
                    report_recovery(&app, &state, "Undid composition edit");
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
                    *state.selected_keyframe.borrow_mut() = None;
                    refresh_motion(&app, &state);
                    report_recovery(&app, &state, "Redid composition edit");
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
                    Ok(Some(path)) => {
                        let clock_time = state.clock.borrow().current_time_seconds() as f32;
                        match write_svg_frame(&state.current.borrow(), &path, clock_time) {
                            Ok(()) => set_status(&app, format!("Exported {}", path.display())),
                            Err(error) => {
                                set_status(&app, format!("SVG frame export failed: {error}"))
                            }
                        }
                    }
                    Ok(None) => set_status(&app, "Export cancelled"),
                    Err(error) => set_status(&app, format!("Export dialog failed: {error}")),
                }
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

    let menu_bar = build_standard_menu_bar(
        "Loom Motion",
        vec![MenuItem::action_with_shortcut(
            "file.export_frame",
            "Export Frame as SVG...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![Menu::new(
            "Composition",
            vec![
                MenuItem::action_with_shortcut(
                    "comp.new_layer",
                    "New Motion Layer",
                    MenuShortcut::primary_shift("N"),
                ),
                MenuItem::action("comp.duplicate_layer", "Duplicate Layer"),
                MenuItem::action("comp.delete_layer", "Delete Layer"),
            ],
        )],
    );
    let menu_service = NativeMenuBar::new();
    let _ = menu_service.install_menu_bar(&menu_bar);

    wire_palette(&app);

    let timer = slint::Timer::default();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let mut last_tick = Instant::now();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(16),
            move || {
                let now = Instant::now();
                let dt = now.duration_since(last_tick).as_secs_f64();
                last_tick = now;
                if let Some(app) = app_ref.upgrade() {
                    let mut clock = state.clock.borrow_mut();
                    if clock.is_playing {
                        clock.advance_seconds(dt);
                        drop(clock);
                        refresh_motion(&app, &state);
                    }
                }
            },
        );
    }

    refresh_motion(&app, &state);
    if let Some(error) = startup_recovery_error {
        set_status(
            &app,
            format!("Startup recovery snapshot failed; edits may not survive a crash: {error}"),
        );
    }
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
                        PaletteAction::PlayPause => app.invoke_play_pause(),
                        PaletteAction::StepBack => app.invoke_step_back(),
                        PaletteAction::StepForward => app.invoke_step_forward(),
                        PaletteAction::ToggleLoop => app.invoke_toggle_loop(),
                        PaletteAction::Zoom => {
                            let zoom = app.get_zoom_value();
                            app.set_zoom_value(if zoom >= 150.0 { 100.0 } else { zoom + 25.0 });
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
            clock: RefCell::new(clock_for_document(&empty_motion())),
            history: RefCell::new(MotionHistory::default()),
            selected_keyframe: RefCell::new(None),
            transform_gesture_active: RefCell::new(false),
            transform_gesture_checkpointed: RefCell::new(false),
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
    fn responsive_policy_transition_probes_are_exact() {
        set_platform();
        let app = MotionApp::new().expect("create MotionApp");
        let expected = [
            (1179, true, true, false),
            (1180, false, true, false),
            (1279, false, true, false),
            (1280, false, true, false),
            (1319, false, true, false),
            (1320, false, false, true),
        ];
        for (width, icon_only, overflow, labeled) in expected {
            assert_eq!(
                responsive_toolbar_state(&app, width),
                ResponsiveToolbarState {
                    icon_only,
                    overflow,
                    labeled,
                }
            );
            configure_responsive_layout(&app, width);
            assert_eq!(app.get_icon_only_toolbar(), icon_only);
            assert_eq!(app.get_overflow_toolbar(), overflow);
            assert_eq!(app.get_labeled_toolbar(), labeled);
            assert_eq!(compact_layout_for_width(&app, width), icon_only);
        }
    }

    #[test]
    fn new_motion_composition_is_blank() {
        let document = empty_motion();
        assert_eq!(document.name, "Untitled Composition");
        assert!(document.layers.is_empty());
    }

    #[test]
    fn timecode_uses_the_clock_frame_and_frame_rate() {
        assert_eq!(format_timecode(0, 60.0), "00:00:00:00");
        assert_eq!(format_timecode(61, 60.0), "00:00:01:01");
        assert_eq!(format_timecode(3_723, 24.0), "00:02:35:03");
    }

    #[test]
    fn transform_edit_is_quantized_to_current_frame_and_samples_immediately() {
        let mut document = empty_motion();
        document.frame_rate = 24.0;
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.add_keyframe("x", 0.0, 10.0);
        document.add_layer(layer);

        assert!(edit_transform_at_frame(&mut document, 13, "x", 42.0));
        assert!(edit_transform_at_frame(&mut document, 13, "x", 84.0));

        let keys = &document.layers[0].position_x_keys;
        assert_eq!(keys.len(), 2);
        assert!((keys[1].time_secs - 13.0 / 24.0).abs() < f32::EPSILON);
        assert_eq!(keys[1].value, 84.0);
        assert_eq!(document.frame(13).layers[0].x, 84.0);
    }

    #[test]
    fn replacing_document_rebases_transport_clock() {
        let state = test_state();
        state.clock.borrow_mut().seek_frame(120);
        state.clock.borrow_mut().set_playing(true);

        let mut replacement = empty_motion();
        replacement.frame_rate = 24.0;
        replacement.duration_secs = 2.0;
        state.replace(replacement).expect("replace recovery state");

        let clock = state.clock.borrow();
        assert_eq!(clock.current_frame, 0);
        assert_eq!(clock.fps, 24.0);
        assert_eq!(clock.out_frame, 48);
        assert!(!clock.is_playing);
    }

    #[test]
    fn selected_layer_keyframes_are_absolute_and_bounded_to_timeline() {
        let mut document = empty_motion();
        document.duration_secs = 4.0;
        let mut layer = MotionLayer::new("layer", "Layer", "Text");
        layer.start_time = 1.0;
        layer.add_keyframe("x", -2.0, 10.0);
        layer.add_keyframe("y", 1.0, 20.0);
        layer.add_keyframe("opacity", 8.0, 30.0);
        document.add_layer(layer);

        // Keys outside the layer interval or composition timeline are not
        // projected onto a misleading boundary marker.
        assert_eq!(selected_layer_keyframes(&document), vec![2.0]);
    }

    #[test]
    fn transform_edit_rejects_frames_outside_layer_interval() {
        let mut document = empty_motion();
        document.frame_rate = 10.0;
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.start_time = 2.0;
        layer.duration = 3.0;
        layer.add_keyframe("x", 0.0, 10.0);
        document.add_layer(layer);

        assert!(!edit_transform_at_frame(&mut document, 19, "x", 20.0));
        assert!(!edit_transform_at_frame(&mut document, 51, "x", 30.0));
        assert!(edit_transform_at_frame(&mut document, 20, "x", 40.0));
        assert!(edit_transform_at_frame(&mut document, 49, "x", 50.0));
        assert_eq!(selected_layer_keyframes(&document), vec![2.0, 4.9]);
    }

    #[test]
    fn keyframe_time_move_preserves_channel_order_and_interval() {
        let mut document = empty_motion();
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.add_keyframe("x", 1.0, 10.0);
        layer.add_keyframe("x", 3.0, 30.0);
        document.add_layer(layer);

        assert!(move_keyframe_at_time(&mut document, "x", 1.0, 2.0));
        assert!(!move_keyframe_at_time(&mut document, "x", 2.0, -1.0));
        assert_eq!(document.layers[0].position_x_keys[0].time_secs, 2.0);
        assert_eq!(document.layers[0].position_x_keys[0].value, 10.0);
    }

    #[test]
    fn rejected_keyframe_time_edit_does_not_create_history_entry() {
        let state = test_state();
        let mut document = empty_motion();
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.start_time = 2.0;
        layer.duration = 3.0;
        layer.add_keyframe("x", 0.0, 10.0);
        document.add_layer(layer);
        *state.current.borrow_mut() = document.clone();
        let selected = SelectedKeyframe {
            property: "x".into(),
            time_secs: 2.0,
        };

        // Document time 0 is outside this layer's [2s, 5s] interval. The
        // candidate mutation must fail before the current document is
        // checkpointed, leaving history and document state untouched.
        assert!(apply_keyframe_time_edit(&state, &selected, "x", 0.0).is_err());
        assert!(state.history.borrow().undo.is_empty());
        assert_eq!(*state.current.borrow(), document);
    }

    #[test]
    fn rejected_transform_edit_does_not_create_history_entry() {
        let state = test_state();
        let mut document = empty_motion();
        let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
        layer.start_time = 2.0;
        layer.duration = 3.0;
        layer.add_keyframe("x", 0.0, 10.0);
        document.add_layer(layer);
        *state.current.borrow_mut() = document.clone();

        // Frame 0 is outside this layer's active interval. The failed
        // candidate must leave both document and history untouched.
        assert!(apply_transform_edit(&state, 0, "x", 20.0).is_err());
        assert!(state.history.borrow().undo.is_empty());
        assert_eq!(*state.current.borrow(), document);
    }

    #[test]
    fn layer_timing_projection_uses_safe_values_for_timeline_lanes() {
        let mut document = empty_motion();
        let mut valid = MotionLayer::new("valid", "Valid", "Text");
        valid.start_time = 1.5;
        valid.duration = 3.0;
        let mut invalid = MotionLayer::new("invalid", "Invalid", "Text");
        invalid.start_time = f32::NAN;
        invalid.duration = f32::NEG_INFINITY;
        document.add_layer(valid);
        document.add_layer(invalid);

        assert_eq!(layer_timing(&document), (vec![1.5, 0.0], vec![3.0, 0.0]));
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
        state.replace(document).expect("replace recovery state");
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
    fn svg_frame_export_uses_non_zero_clock_time() {
        let mut document = empty_motion();
        let mut layer = MotionLayer::new("layer", "Animated", "VectorShape");
        layer.add_keyframe("x", 0.0, 0.0);
        layer.add_keyframe("x", 1.0, 100.0);
        document.add_layer(layer);

        let at_start = export_svg_frame(&document, 0.0);
        let at_playhead = export_svg_frame(&document, 1.0);
        assert!(at_start.contains("translate(0.000 0.000)"));
        assert!(at_playhead.contains("translate(100.000 0.000)"));
        assert_ne!(at_start, at_playhead);
    }

    #[test]
    fn svg_frame_export_omits_layers_outside_their_active_interval() {
        let mut document = empty_motion();
        let mut hidden = MotionLayer::new("hidden", "Hidden Layer", "Text");
        hidden.start_time = 2.0;
        hidden.duration = 1.0;
        hidden.add_keyframe("x", 0.0, 960.0);
        document.add_layer(hidden);

        let before = export_svg_frame(&document, 0.0);
        let during = export_svg_frame(&document, 2.5);
        assert!(!before.contains("Hidden Layer"));
        assert!(during.contains("Hidden Layer"));
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

    #[test]
    fn motion_overflow_palette_exposes_and_invokes_zoom() {
        set_platform();
        let app = MotionApp::new().expect("create MotionApp");
        configure_responsive_layout(&app, 1180);
        wire_palette(&app);

        app.set_palette_query("zoom".into());
        rebuild_palette(&app, "zoom");
        assert_eq!(app.get_palette_commands().row_count(), 1);
        assert_eq!(
            app.get_palette_commands()
                .row_data(0)
                .expect("zoom command")
                .id,
            "motion.zoom"
        );

        app.set_zoom_value(100.0);
        app.invoke_palette_invoked(0);
        assert_eq!(app.get_zoom_value(), 125.0);

        // Keep the existing toolbar cycle semantics: the 150% step wraps to
        // 100% rather than creating an unsupported out-of-range value.
        app.set_zoom_value(150.0);
        app.set_palette_query("zoom".into());
        rebuild_palette(&app, "zoom");
        app.invoke_palette_invoked(0);
        assert_eq!(app.get_zoom_value(), 100.0);
    }
}
