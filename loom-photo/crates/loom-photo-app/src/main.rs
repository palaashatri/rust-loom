//! Loom Photo desktop application.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_photo_core::{
    decode_raster, encode_jpeg, encode_png, load_photo, load_photo_canvas, save_photo_canvas,
    BlendMode, Layer, PhotoCanvas, PhotoDocument, PhotoSession, RgbaImage,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{
    ComponentHandle, Image, Model, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    SharedString, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "image.loomphoto";
const DEFAULT_EXPORT_FILENAME: &str = "loom-photo-export.png";

loom_production::define_snapshot_recovery!(PHOTO_RECOVERY, "org.loom.photo", "loom.photo/1");

struct Args {
    screenshot: Option<String>,
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
    while let Some(argument) = it.next() {
        match argument.as_str() {
            "--screenshot" => args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?),
            "--smoke" => args.smoke = true,
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(it.next().ok_or("--journey needs an output directory")?);
            }
            "--size" => {
                let value = it.next().ok_or("--size needs WxH")?;
                let (width, height) = value.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    width.parse().map_err(|_| "bad width")?,
                    height.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => args.theme = it.next().ok_or("--theme needs a name")?,
            "--open" => args.open = Some(it.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn sample_canvas() -> Result<PhotoCanvas, String> {
    let width = 960;
    let height = 540;
    let mut document = PhotoDocument::new("photo-sample", "Copper Light Study", width, height);
    document.dpi = 144;
    document.add_layer(Layer::new_adjustment(
        "adjust-brightness",
        "Brightness",
        "brightness",
        0.0,
    ));
    document.add_layer(Layer::new_adjustment(
        "adjust-contrast",
        "Contrast",
        "contrast",
        0.0,
    ));
    document.add_layer(Layer::new_adjustment(
        "adjust-saturation",
        "Saturation",
        "saturation",
        0.0,
    ));
    document.active_layer_index = 0;
    let mut canvas = PhotoCanvas::new(document)?;
    let mut image = RgbaImage::transparent(width, height)?;
    for y in 0..height {
        for x in 0..width {
            let nx = x as f32 / width as f32;
            let ny = y as f32 / height as f32;
            let glow = ((1.0 - ((nx - 0.72).powi(2) + (ny - 0.34).powi(2)).sqrt()).clamp(0.0, 1.0)
                * 110.0) as u8;
            let copper = ((nx * 70.0 + glow as f32 * 0.55).clamp(0.0, 255.0)) as u8;
            let blue = ((36.0 + (1.0 - ny) * 46.0).clamp(0.0, 255.0)) as u8;
            image.set_pixel(
                x,
                y,
                [
                    24u8.saturating_add(copper),
                    30u8.saturating_add(glow / 3),
                    blue,
                    255,
                ],
            );
        }
    }
    canvas.set_layer_image("layer-bg", image)?;
    Ok(canvas)
}

fn initial_canvas(args: &Args) -> Result<PhotoCanvas, String> {
    let Some(path) = args.open.as_deref() else {
        return sample_canvas();
    };
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read photo project '{path}': {error}"))?;
    load_photo_canvas(&bytes).or_else(|_| {
        let document = load_photo(&bytes)
            .map_err(|error| format!("failed to load photo project '{path}': {error}"))?;
        PhotoCanvas::new(document)
    })
}

fn adjustment_value(document: &PhotoDocument, kind: &str) -> f32 {
    document
        .layers
        .iter()
        .find(|layer| layer.adjustment_type.as_deref() == Some(kind))
        .map(|layer| layer.adjustment_value)
        .unwrap_or(0.0)
}

fn slint_image(image: &RgbaImage) -> Image {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        image.pixels.as_slice(),
        image.width,
        image.height,
    );
    Image::from_rgba8(buffer)
}

fn refresh_photo(app: &PhotoApp, session: &PhotoSession) -> Result<(), String> {
    let document = &session.canvas.document;
    app.set_project_name(document.name.as_str().into());
    app.set_dimensions_text(SharedString::from(format!(
        "{} × {} · {} DPI · {}",
        document.width, document.height, document.dpi, document.color_space
    )));
    let labels: Vec<SharedString> = document
        .layers
        .iter()
        .map(|layer| SharedString::from(format!("{} · {:?}", layer.name, layer.kind)))
        .collect();
    let visibilities: Vec<bool> = document.layers.iter().map(|layer| layer.visible).collect();
    app.set_layer_labels(ModelRc::new(VecModel::from(labels)));
    app.set_layer_visibilities(ModelRc::new(VecModel::from(visibilities)));
    app.set_active_layer_index(document.active_layer_index as i32);
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    app.set_brightness_value(adjustment_value(document, "brightness") * 100.0);
    app.set_contrast_value(adjustment_value(document, "contrast") * 100.0);
    app.set_saturation_value(adjustment_value(document, "saturation") * 100.0);

    if let Some(active) = document.layers.get(document.active_layer_index) {
        app.set_active_blend_mode(
            match active.blend_mode {
                BlendMode::Normal => "Normal",
                BlendMode::Multiply => "Multiply",
                BlendMode::Screen => "Screen",
                BlendMode::Overlay => "Overlay",
            }
            .into(),
        );
        app.set_active_layer_opacity(active.opacity * 100.0);
    }

    let composite = session.canvas.composite()?;
    let preview = if composite.width > 960 || composite.height > 640 {
        let scale = (960.0 / composite.width as f32)
            .min(640.0 / composite.height as f32)
            .min(1.0);
        composite.resize_nearest(
            (composite.width as f32 * scale).max(1.0).round() as u32,
            (composite.height as f32 * scale).max(1.0).round() as u32,
        )?
    } else {
        composite
    };
    app.set_preview_image(slint_image(&preview));
    app.set_has_preview(true);
    app.set_status_left(SharedString::from(format!(
        "{} layers · {} pixel payloads · nondestructive preview",
        document.layers.len(),
        session.canvas.pixel_payload_count()
    )));
    app.set_status_right("Local compositor".into());
    if let Ok(bytes) = save_photo_canvas(&session.canvas) {
        let _ = record_snapshot_recovery("photo state", bytes);
    }
    Ok(())
}

fn apply_theme(app: &PhotoApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

/// Commands exposed through the command palette. Each palette entry maps to
/// one of the application callbacks, so palette invocation and toolbar clicks
/// share a single dispatch path.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewProject,
    OpenProject,
    SaveProject,
    Undo,
    Redo,
    AddLayer,
    AddAdjustment,
    RemoveLayer,
    MoveLayer(i32),
    SelectTool(&'static str),
    ExportPng,
    ExportJpeg,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &PhotoApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewProject,
            id: "photo.new",
            label: "New Project",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenProject,
            id: "photo.open",
            label: "Open Project",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveProject,
            id: "photo.save",
            label: "Save Project",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "photo.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "photo.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::AddLayer,
            id: "photo.layer.add",
            label: "Add Pixel Layer",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::AddAdjustment,
            id: "photo.layer.adjustment",
            label: "Add Adjustment Layer",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::RemoveLayer,
            id: "photo.layer.remove",
            label: "Remove Selected Layer",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::MoveLayer(1),
            id: "photo.layer.move-up",
            label: "Move Layer Up",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::MoveLayer(-1),
            id: "photo.layer.move-down",
            label: "Move Layer Down",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SelectTool("Select"),
            id: "photo.tool.select",
            label: "Tool: Select",
            shortcut: "V",
        },
        PaletteCommand {
            action: PaletteAction::SelectTool("Pan"),
            id: "photo.tool.pan",
            label: "Tool: Pan",
            shortcut: "H",
        },
        PaletteCommand {
            action: PaletteAction::ExportPng,
            id: "photo.export-png",
            label: "Export PNG",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::ExportJpeg,
            id: "photo.export-jpeg",
            label: "Export JPEG",
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

fn rebuild_palette(app: &PhotoApp, query: &str) {
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

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = PhotoApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let session = PhotoSession::new(initial_canvas(args)?);
    refresh_photo(&app, &session)?;
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

struct GuiState {
    session: RefCell<PhotoSession>,
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = PhotoApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let session = PhotoSession::new(initial_canvas(args)?);
    refresh_photo(&app, &session)?;
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "photo", Path::new(out_dir), "layer")
        .map_err(|error| format!("journey failed: {error}"))?;
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

impl PaletteProbe for PhotoApp {
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

fn set_status(app: &PhotoApp, message: impl Into<SharedString>) {
    app.set_status_left(message.into());
}

fn normalize_export_path(value: &str, extension: &str) -> PathBuf {
    let value = value.trim();
    let mut path = if value.is_empty() {
        PathBuf::from(DEFAULT_EXPORT_FILENAME)
    } else {
        PathBuf::from(value)
    };
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

fn update_adjustment(state: &GuiState, app: &PhotoApp, kind: &str, display_name: &str, value: f32) {
    let mut session = state.session.borrow_mut();
    let current = adjustment_value(&session.canvas.document, kind);
    let normalized = (value / 100.0).clamp(-1.0, 1.0);
    if (current - normalized).abs() < 0.0001 {
        return;
    }
    session.checkpoint();
    if let Some(layer) = session
        .canvas
        .document
        .layers
        .iter_mut()
        .find(|layer| layer.adjustment_type.as_deref() == Some(kind))
    {
        layer.adjustment_value = normalized;
    } else {
        let id = format!("adjust-{kind}");
        session.canvas.document.add_layer(Layer::new_adjustment(
            id,
            display_name,
            kind,
            normalized,
        ));
    }
    if let Err(error) = refresh_photo(app, &session) {
        set_status(app, format!("Preview failed: {error}"));
    }
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(output) = &args.screenshot {
        return render_headless(&args, output);
    }
    if args.smoke {
        let output =
            std::env::temp_dir().join(format!("loom-photo-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }

    let app = PhotoApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_canvas(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_photo_canvas(bytes).ok())
            .unwrap_or(initial_canvas(&args)?)
    };
    let state = Rc::new(GuiState {
        session: RefCell::new(PhotoSession::new(initial)),
    });

    macro_rules! mutate_and_refresh {
        ($callback:ident, $body:expr) => {{
            let state = state.clone();
            let app_ref = app.as_weak();
            app.$callback(move || {
                if let Some(app) = app_ref.upgrade() {
                    let mut session = state.session.borrow_mut();
                    ($body)(&mut session);
                    if let Err(error) = refresh_photo(&app, &session) {
                        set_status(&app, format!("Operation failed: {error}"));
                    }
                }
            });
        }};
    }

    mutate_and_refresh!(on_new_project, |session: &mut PhotoSession| {
        if let Ok(canvas) = sample_canvas() {
            *session = PhotoSession::new(canvas);
        }
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_project(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| load_photo_canvas(&bytes))
                {
                    Ok(canvas) => {
                        *state.session.borrow_mut() = PhotoSession::new(canvas);
                        if let Err(error) = refresh_photo(&app, &state.session.borrow()) {
                            set_status(&app, format!("Open preview failed: {error}"));
                        } else {
                            set_status(&app, format!("Opened {SAVE_FILENAME}"));
                        }
                    }
                    Err(error) => set_status(&app, format!("Open failed: {error}")),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                match save_photo_canvas(&state.session.borrow().canvas) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => set_status(&app, format!("Saved {SAVE_FILENAME}")),
                        Err(error) => set_status(&app, format!("Save/checkpoint failed: {error}")),
                    },
                    Err(error) => set_status(&app, format!("Save failed: {error}")),
                }
            }
        });
    }

    mutate_and_refresh!(on_undo, |session: &mut PhotoSession| {
        session.undo();
    });
    mutate_and_refresh!(on_redo, |session: &mut PhotoSession| {
        session.redo();
    });
    mutate_and_refresh!(on_add_layer, |session: &mut PhotoSession| {
        let count = session.canvas.document.layers.len() + 1;
        session.add_pixel_layer(format!("layer-{count}"), format!("Pixel Layer {count}"));
        if let Ok(image) = RgbaImage::transparent(
            session.canvas.document.width,
            session.canvas.document.height,
        ) {
            let _ = session
                .canvas
                .set_layer_image(&format!("layer-{count}"), image);
        }
    });
    mutate_and_refresh!(on_add_adjustment, |session: &mut PhotoSession| {
        let count = session.canvas.document.layers.len() + 1;
        session.add_adjustment(
            format!("adjustment-{count}"),
            format!("Brightness {count}"),
            "brightness",
            0.0,
        );
    });
    mutate_and_refresh!(on_remove_layer, |session: &mut PhotoSession| {
        let index = session.canvas.document.active_layer_index;
        session.remove_layer(index);
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_move_layer(move |direction| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let from = session.canvas.document.active_layer_index;
                let to = if direction < 0 {
                    from.saturating_sub(1)
                } else {
                    (from + 1).min(session.canvas.document.layers.len().saturating_sub(1))
                };
                session.move_layer(from, to);
                let _ = refresh_photo(&app, &session);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_layer(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    state
                        .session
                        .borrow_mut()
                        .canvas
                        .document
                        .select_layer(index as usize);
                    let _ = refresh_photo(&app, &state.session.borrow());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_layer_visibility(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
                    if (index as usize) < session.canvas.document.layers.len() {
                        session.checkpoint();
                        session.canvas.document.layers[index as usize].visible =
                            !session.canvas.document.layers[index as usize].visible;
                    }
                    let _ = refresh_photo(&app, &session);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_opacity_changed(move |value| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let index = session.canvas.document.active_layer_index;
                if index < session.canvas.document.layers.len() {
                    session.checkpoint();
                    session.canvas.document.layers[index].opacity = (value / 100.0).clamp(0.0, 1.0);
                }
                let _ = refresh_photo(&app, &session);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_blend_mode_changed(move |value| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let index = session.canvas.document.active_layer_index;
                if index < session.canvas.document.layers.len() {
                    session.checkpoint();
                    session.canvas.document.layers[index].blend_mode = match value.as_str() {
                        "Multiply" => BlendMode::Multiply,
                        "Screen" => BlendMode::Screen,
                        "Overlay" => BlendMode::Overlay,
                        _ => BlendMode::Normal,
                    };
                }
                let _ = refresh_photo(&app, &session);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_select_tool(move |tool| {
            if let Some(app) = app_ref.upgrade() {
                set_status(&app, format!("{} tool selected", tool.as_str()));
            }
        });
    }

    for (kind, display) in [
        ("brightness", "Brightness"),
        ("contrast", "Contrast"),
        ("saturation", "Saturation"),
    ] {
        let state = state.clone();
        let app_ref = app.as_weak();
        match kind {
            "brightness" => app.on_brightness_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    update_adjustment(&state, &app, kind, display, value);
                }
            }),
            "contrast" => app.on_contrast_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    update_adjustment(&state, &app, kind, display, value);
                }
            }),
            _ => app.on_saturation_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    update_adjustment(&state, &app, kind, display, value);
                }
            }),
        }
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_import_image(move |path| {
            if let Some(app) = app_ref.upgrade() {
                let path = path.trim();
                if path.is_empty() {
                    set_status(&app, "Enter a PNG or JPEG path first");
                    return;
                }
                match std::fs::read(path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| decode_raster(&bytes))
                {
                    Ok(image) => {
                        let mut document = PhotoDocument::new(
                            "imported-photo",
                            Path::new(path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Imported Image"),
                            image.width,
                            image.height,
                        );
                        document.dpi = 144;
                        let mut canvas = match PhotoCanvas::new(document) {
                            Ok(canvas) => canvas,
                            Err(error) => {
                                set_status(&app, format!("Import failed: {error}"));
                                return;
                            }
                        };
                        if let Err(error) = canvas.set_layer_image("layer-bg", image) {
                            set_status(&app, format!("Import failed: {error}"));
                            return;
                        }
                        *state.session.borrow_mut() = PhotoSession::new(canvas);
                        let _ = refresh_photo(&app, &state.session.borrow());
                        set_status(&app, format!("Imported {path}"));
                    }
                    Err(error) => set_status(&app, format!("Import failed: {error}")),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_png(move |path| {
            if let Some(app) = app_ref.upgrade() {
                let target = normalize_export_path(path.as_str(), "png");
                let result = state
                    .session
                    .borrow()
                    .canvas
                    .composite()
                    .and_then(|image| encode_png(&image))
                    .and_then(|bytes| {
                        std::fs::write(&target, bytes).map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => set_status(&app, format!("Exported {}", target.display())),
                    Err(error) => set_status(&app, format!("Export failed: {error}")),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_jpeg(move |path| {
            if let Some(app) = app_ref.upgrade() {
                let target = normalize_export_path(path.as_str(), "jpg");
                let result = state
                    .session
                    .borrow()
                    .canvas
                    .composite()
                    .and_then(|image| encode_jpeg(&image, 92))
                    .and_then(|bytes| {
                        std::fs::write(&target, bytes).map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => set_status(&app, format!("Exported {}", target.display())),
                    Err(error) => set_status(&app, format!("Export failed: {error}")),
                }
            }
        });
    }

    wire_palette(&app);

    refresh_photo(&app, &state.session.borrow())?;
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// Connect the command-palette callbacks. Invocation dispatches through the
/// same application callbacks as the toolbar, so palette and toolbar behave
/// identically, and the query model stays in Rust for testability.
fn wire_palette(app: &PhotoApp) {
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
                        PaletteAction::NewProject => app.invoke_new_project(),
                        PaletteAction::OpenProject => app.invoke_open_project(),
                        PaletteAction::SaveProject => app.invoke_save_project(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddLayer => app.invoke_add_layer(),
                        PaletteAction::AddAdjustment => app.invoke_add_adjustment(),
                        PaletteAction::RemoveLayer => app.invoke_remove_layer(),
                        PaletteAction::MoveLayer(direction) => app.invoke_move_layer(direction),
                        PaletteAction::SelectTool(tool) => {
                            app.invoke_select_tool(SharedString::from(tool))
                        }
                        PaletteAction::ExportPng => {
                            app.invoke_export_png(DEFAULT_EXPORT_FILENAME.into())
                        }
                        PaletteAction::ExportJpeg => {
                            app.invoke_export_jpeg(DEFAULT_EXPORT_FILENAME.into())
                        }
                    }
                }
            }
        });
    }
}
