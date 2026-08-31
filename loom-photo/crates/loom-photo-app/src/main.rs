//! Loom Photo desktop application.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{
    build_standard_menu_bar, CommandAction, CommandStateProjection, DesktopError,
    FileDialogService, FileFilter, Menu, MenuBar, MenuBarService, MenuItem, MenuShortcut,
    NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest, ScriptedFileDialogs,
};
use loom_photo_core::{
    decode_raster, encode_jpeg, encode_png, load_photo, load_photo_canvas, save_photo_canvas,
    AffineTransform2D, BlendMode, Layer, PhotoCanvas, PhotoDocument, PhotoSession, Rect, RgbaImage,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{
    ComponentHandle, Image, Model, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    SharedString, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "untitled-photo.loomphoto";
const PNG_EXPORT_FILENAME: &str = "loom-photo-export.png";
const JPEG_EXPORT_FILENAME: &str = "loom-photo-export.jpg";

loom_production::define_snapshot_recovery!(PHOTO_RECOVERY, "org.loom.photo", "loom.photo/1");

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
            "--rtl" => args.rtl = true,
            "--open" => args.open = Some(it.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn blank_canvas() -> Result<PhotoCanvas, String> {
    let width = 1920;
    let height = 1080;
    let mut document = PhotoDocument::new("untitled-photo", "Untitled Photo", width, height);
    document.dpi = 144;
    let mut canvas = PhotoCanvas::new(document)?;
    canvas.set_layer_image("layer-bg", RgbaImage::transparent(width, height)?)?;
    Ok(canvas)
}

fn sample_canvas() -> Result<PhotoCanvas, String> {
    let source = sample_raster_payload()?;
    let image = decode_raster(&source)?;
    let width = image.width;
    let height = image.height;
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
    canvas.set_layer_image("layer-bg", image)?;
    Ok(canvas)
}

/// Encodes the deterministic sample as an actual PNG payload, then exercises the same decode
/// path as an imported file. The payload is a small still-life card rather than a calibration
/// gradient, so the primary journey always renders real raster content.
fn sample_raster_payload() -> Result<Vec<u8>, String> {
    let width = 480;
    let height = 300;
    let mut image = RgbaImage::solid(width, height, [24, 29, 39, 255])?;
    for y in 34..266 {
        for x in 28..452 {
            let edge = !(40..440).contains(&x) || !(46..254).contains(&y);
            let band = ((x / 28 + y / 24) % 2) == 0;
            let color = if edge {
                [194, 113, 66, 255]
            } else if band {
                [53, 67, 79, 255]
            } else {
                [39, 49, 61, 255]
            };
            image.set_pixel(x, y, color);
        }
    }
    for y in 84..216 {
        for x in 148..332 {
            let dx = x as i32 - 240;
            let dy = y as i32 - 150;
            if dx * dx + dy * dy <= 66 * 66 {
                image.set_pixel(x, y, [224, 150, 72, 255]);
            }
        }
    }
    encode_png(&image)
}

fn load_project(path: &Path) -> Result<PhotoCanvas, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read photo project '{}': {error}", path.display()))?;
    load_photo_canvas(&bytes).or_else(|_| {
        let document = load_photo(&bytes).map_err(|error| {
            format!("failed to load photo project '{}': {error}", path.display())
        })?;
        PhotoCanvas::new(document)
    })
}

fn load_raster_canvas(path: &Path) -> Result<PhotoCanvas, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read image '{}': {error}", path.display()))?;
    let image = decode_raster(&bytes)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Imported Image");
    let digest = image.pixel_digest();
    let layer_id = format!("layer-imported-{digest:016x}");
    let mut document = PhotoDocument::new("imported-photo", name, image.width, image.height);
    document.dpi = 144;
    document.layers[0] = Layer::new_pixel(layer_id.clone(), format!("Imported · {name}"));
    document.layers[0].source_digest = Some(digest);
    let mut canvas = PhotoCanvas::new(document)?;
    canvas.set_layer_image(&layer_id, image)?;
    Ok(canvas)
}

fn initial_canvas(args: &Args) -> Result<PhotoCanvas, String> {
    match args.open.as_deref() {
        Some(path) => load_project(Path::new(path)),
        None => sample_canvas(),
    }
}

fn adjustment_value(document: &PhotoDocument, kind: &str) -> f32 {
    document
        .layers
        .iter()
        .find(|layer| layer.adjustment_type.as_deref() == Some(kind))
        .map(|layer| layer.adjustment_value)
        .unwrap_or(0.0)
}

fn transform_components(transform: AffineTransform2D) -> (f32, f32, f32, f32, f32) {
    let scale_x = (transform.a * transform.a + transform.b * transform.b)
        .sqrt()
        .max(0.01);
    let scale_y = (transform.c * transform.c + transform.d * transform.d)
        .sqrt()
        .max(0.01);
    let rotation = transform.b.atan2(transform.a).to_degrees();
    (
        transform.tx,
        transform.ty,
        scale_x * 100.0,
        scale_y * 100.0,
        rotation,
    )
}

fn transform_from_components(
    x: f32,
    y: f32,
    scale_x_percent: f32,
    scale_y_percent: f32,
    rotation_degrees: f32,
) -> AffineTransform2D {
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let scale_x = (scale_x_percent / 100.0).clamp(0.01, 8.0);
    let scale_y = (scale_y_percent / 100.0).clamp(0.01, 8.0);
    AffineTransform2D {
        a: cos * scale_x,
        b: sin * scale_x,
        c: -sin * scale_y,
        d: cos * scale_y,
        tx: x,
        ty: y,
    }
}

fn format_rect(rect: Option<Rect>) -> String {
    rect.map(|rect| {
        format!(
            "x {:.1}, y {:.1}, {:.1} × {:.1}",
            rect.x, rect.y, rect.width, rect.height
        )
    })
    .unwrap_or_else(|| "None".to_string())
}

fn layer_kind_label(layer: &Layer) -> &'static str {
    match layer.kind {
        loom_photo_core::LayerKind::Pixel => "Pixel",
        loom_photo_core::LayerKind::Adjustment => "Adjustment",
        loom_photo_core::LayerKind::Text => "Text",
        loom_photo_core::LayerKind::Vector => "Vector",
    }
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
    app.set_document_width(document.width as f32);
    app.set_document_height(document.height as f32);
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
    let last_layer_index = document.layers.len().saturating_sub(1);
    app.set_can_move_up(document.active_layer_index < last_layer_index);
    app.set_can_move_down(document.active_layer_index > 0);
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    app.set_brightness_value(adjustment_value(document, "brightness") * 100.0);
    app.set_contrast_value(adjustment_value(document, "contrast") * 100.0);
    app.set_saturation_value(adjustment_value(document, "saturation") * 100.0);

    app.set_selection_geometry(SharedString::from(format_rect(document.selection)));
    app.set_crop_geometry(SharedString::from(format_rect(document.crop)));
    app.set_has_selection(document.selection.is_some());
    app.set_has_crop(document.crop.is_some());

    if let Some(active) = document.layers.get(document.active_layer_index) {
        let (x, y, scale_x, scale_y, rotation) = transform_components(active.transform);
        app.set_active_layer_id(active.id.as_str().into());
        app.set_active_layer_kind(layer_kind_label(active).into());
        app.set_active_layer_transform_enabled(matches!(
            active.kind,
            loom_photo_core::LayerKind::Pixel
        ));
        app.set_active_layer_x(x);
        app.set_active_layer_y(y);
        app.set_active_layer_scale_x(scale_x);
        app.set_active_layer_scale_y(scale_y);
        app.set_active_layer_rotation(rotation);
        let bounds = session.canvas.active_layer_bounds();
        app.set_active_layer_bounds(SharedString::from(format_rect(bounds)));
        if let Some(bounds) = bounds {
            let width = document.width.max(1) as f32;
            let height = document.height.max(1) as f32;
            app.set_selected_layer_bounds_x(bounds.x / width);
            app.set_selected_layer_bounds_y(bounds.y / height);
            app.set_selected_layer_bounds_width(bounds.width / width);
            app.set_selected_layer_bounds_height(bounds.height / height);
        } else {
            app.set_selected_layer_bounds_x(0.0);
            app.set_selected_layer_bounds_y(0.0);
            app.set_selected_layer_bounds_width(1.0);
            app.set_selected_layer_bounds_height(1.0);
        }
        app.set_active_blend_mode(
            match active.blend_mode {
                BlendMode::Normal => "Normal",
                BlendMode::Multiply => "Multiply",
                BlendMode::Screen => "Screen",
                BlendMode::Overlay => "Overlay",
                BlendMode::Darken => "Darken",
                BlendMode::Lighten => "Lighten",
                BlendMode::Difference => "Difference",
                BlendMode::HardLight => "Hard Light",
            }
            .into(),
        );
        app.set_active_layer_opacity(active.opacity * 100.0);
    } else {
        app.set_active_layer_id("".into());
        app.set_active_layer_kind("None".into());
        app.set_active_layer_transform_enabled(false);
        app.set_active_layer_bounds("—".into());
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
    app.set_has_preview(session.canvas.pixel_payload_count() > 0);
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

fn configure_responsive_layout(app: &PhotoApp, size: (u32, u32)) {
    configure_responsive_width(app, size.0);
}

fn configure_responsive_width(app: &PhotoApp, width: u32) {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32;
    app.set_inspector_available(width >= policy.get_priority_1_icon_only_below());
    app.set_labeled_export(width >= policy.get_priority_2_overflow_below());
}

fn configure_direction(app: &PhotoApp, rtl: bool) {
    app.set_rtl(rtl);
}

/// Resolve a tool command into the independent paint/select index and viewport
/// pan mode. Pan must not reuse the Brush index: doing so makes a palette Pan
/// invocation silently change the active toolbar tool.
fn photo_tool_state(tool: &str) -> (i32, bool) {
    match tool.trim().to_ascii_lowercase().as_str() {
        "pan" => (0, true),
        "brush" => (1, false),
        "wand" => (2, false),
        _ => (0, false),
    }
}

fn wire_responsive_layout(app: &PhotoApp, state: Rc<GuiState>) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_width(&app, width.max(0.0) as u32);
            if let Some(menu_service) = &state.menu_service {
                sync_menu_state(menu_service, &app, &state);
            }
        }
    });
}

/// Commands exposed through the command palette. Each palette entry maps to
/// one of the application callbacks, so palette invocation and toolbar clicks
/// share a single dispatch path.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewProject,
    OpenProject,
    SaveProject,
    SaveAsProject,
    ImportImage,
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

fn palette_action_enabled(app: &PhotoApp, action: &PaletteAction) -> bool {
    match action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        PaletteAction::MoveLayer(direction) => {
            if *direction > 0 {
                app.get_can_move_up()
            } else {
                app.get_can_move_down()
            }
        }
        _ => true,
    }
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
            action: PaletteAction::SaveAsProject,
            id: "photo.save-as",
            label: "Save Project As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::ImportImage,
            id: "photo.import-image",
            label: "Import PNG or JPEG",
            shortcut: "Ctrl+I",
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
    .filter(|command| palette_action_enabled(app, &command.action))
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
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    configure_responsive_layout(&app, args.size);
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
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    project_filter: FileFilter,
    raster_filter: FileFilter,
    png_filter: FileFilter,
    jpeg_filter: FileFilter,
    menu_service: Option<Rc<NativeMenuBar>>,
}

fn build_photo_menu_bar(app: &PhotoApp) -> MenuBar {
    let mut menu_bar = build_standard_menu_bar(
        "Loom Photo",
        vec![
            MenuItem::action_with_shortcut(
                "file.import_image",
                "Import Image...",
                MenuShortcut::primary("I"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_png",
                "Export as PNG...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action("file.export_jpeg", "Export as JPEG..."),
        ],
        vec![],
        vec![MenuItem::check(
            "view.inspector",
            "Inspector",
            app.get_show_inspector(),
        )],
        vec![Menu::new(
            "Layer",
            vec![
                MenuItem::action_with_shortcut(
                    "layer.new",
                    "New Layer",
                    MenuShortcut::primary_shift("N"),
                ),
                MenuItem::action("layer.adjustment", "New Adjustment Layer"),
                MenuItem::action("layer.delete", "Delete Layer"),
                MenuItem::Separator,
                MenuItem::action("layer.move_up", "Move Layer Up"),
                MenuItem::action("layer.move_down", "Move Layer Down"),
            ],
        )],
    );
    menu_bar.disable_items_except([
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.import_image",
        "file.export_png",
        "file.export_jpeg",
        "edit.undo",
        "edit.redo",
        "layer.new",
        "layer.adjustment",
        "layer.delete",
        "layer.move_up",
        "layer.move_down",
        "view.inspector",
        "app.palette",
    ]);
    menu_bar
}

fn menu_projection(
    menu_service: &NativeMenuBar,
    app: &PhotoApp,
    state: &GuiState,
) -> Result<CommandStateProjection, DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("Photo menu bar is not installed".into()))?;
    let mut projection = menu_bar.command_state_projection();
    let session = state.session.borrow();
    let layer_count = session.canvas.document.layers.len();
    let active_layer_index = session.canvas.document.active_layer_index;

    let mut undo = projection
        .get("edit.undo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Photo menu is missing edit.undo".into()))?;
    undo.enabled = session.can_undo();
    projection.insert(undo);

    let mut redo = projection
        .get("edit.redo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Photo menu is missing edit.redo".into()))?;
    redo.enabled = session.can_redo();
    projection.insert(redo);

    let mut inspector = projection.get("view.inspector").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Photo menu is missing view.inspector".into())
    })?;
    inspector.enabled = app.get_inspector_available();
    inspector.checked = Some(app.get_show_inspector());
    projection.insert(inspector);

    let mut delete = projection
        .get("layer.delete")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Photo menu is missing layer.delete".into()))?;
    delete.enabled = layer_count > 1;
    projection.insert(delete);

    let mut move_up = projection.get("layer.move_up").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Photo menu is missing layer.move_up".into())
    })?;
    move_up.enabled = active_layer_index < layer_count.saturating_sub(1);
    projection.insert(move_up);

    let mut move_down = projection.get("layer.move_down").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Photo menu is missing layer.move_down".into())
    })?;
    move_down.enabled = active_layer_index > 0;
    projection.insert(move_down);

    Ok(projection)
}

fn sync_menu_state_result(
    menu_service: &NativeMenuBar,
    app: &PhotoApp,
    state: &GuiState,
) -> Result<(), DesktopError> {
    rebuild_palette(app, app.get_palette_query().as_str());
    let projection = menu_projection(menu_service, app, state)?;
    menu_service.sync_command_states(&projection)
}

fn sync_menu_state(menu_service: &NativeMenuBar, app: &PhotoApp, state: &GuiState) {
    if let Err(error) = sync_menu_state_result(menu_service, app, state) {
        set_status(app, format!("Menu update failed: {error}"));
    }
}

fn refresh_photo_with_state(app: &PhotoApp, state: &GuiState) -> Result<(), String> {
    let refresh_result = {
        let session = state.session.borrow();
        refresh_photo(app, &session)
    };
    refresh_result?;
    if let Some(menu_service) = &state.menu_service {
        sync_menu_state(menu_service, app, state);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ExportKind {
    Png,
    Jpeg,
}

impl ExportKind {
    fn title(self) -> &'static str {
        match self {
            Self::Png => "Export Loom Photo PNG",
            Self::Jpeg => "Export Loom Photo JPEG",
        }
    }

    fn suggested_name(self) -> &'static str {
        match self {
            Self::Png => PNG_EXPORT_FILENAME,
            Self::Jpeg => JPEG_EXPORT_FILENAME,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

fn new_gui_state(
    session: PhotoSession,
    save_path: Option<PathBuf>,
    dialogs: Rc<dyn FileDialogService>,
) -> Result<GuiState, String> {
    Ok(GuiState {
        session: RefCell::new(session),
        save_path: RefCell::new(save_path),
        dialogs,
        project_filter: FileFilter::new("Loom Photo project", ["loomphoto"])
            .map_err(|error| error.to_string())?,
        raster_filter: FileFilter::new("PNG or JPEG image", ["png", "jpg", "jpeg"])
            .map_err(|error| error.to_string())?,
        png_filter: FileFilter::new("PNG image", ["png"]).map_err(|error| error.to_string())?,
        jpeg_filter: FileFilter::new("JPEG image", ["jpg", "jpeg"])
            .map_err(|error| error.to_string())?,
        menu_service: None,
    })
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn open_project_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Loom Photo Project".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.project_filter.clone()],
    }
}

fn import_image_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Import PNG or JPEG Image".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.raster_filter.clone()],
    }
}

fn save_project_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow().clone();
    let suggested_name = path
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| SAVE_FILENAME.to_string());
    SaveFileRequest {
        title: "Save Loom Photo Project".into(),
        initial_directory: initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: vec![state.project_filter.clone()],
    }
}

fn export_request(state: &GuiState, kind: ExportKind) -> SaveFileRequest {
    SaveFileRequest {
        title: kind.title().into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(kind.suggested_name().to_string()),
        filters: vec![match kind {
            ExportKind::Png => state.png_filter.clone(),
            ExportKind::Jpeg => state.jpeg_filter.clone(),
        }],
    }
}

fn is_native_project(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("loomphoto"))
}

fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

fn save_current_project(
    app: &PhotoApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    let existing = (!force_picker)
        .then(|| state.save_path.borrow().clone())
        .flatten();
    let path = match existing {
        Some(path) => Some(path),
        None => state
            .dialogs
            .save_file(&save_project_request(state))
            .map_err(|error| error.to_string())?,
    };
    let Some(path) = path else {
        set_status(app, "Save cancelled");
        return Ok(false);
    };
    let path = ensure_extension(path, "loomphoto");
    let bytes = save_photo_canvas(&state.session.borrow().canvas)?;
    loom_storage::atomic_write(&path, &bytes)
        .map_err(|error| format!("failed to atomic write '{}': {error}", path.display()))?;
    *state.save_path.borrow_mut() = Some(path.clone());
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

fn export_current_image(
    app: &PhotoApp,
    state: &GuiState,
    kind: ExportKind,
) -> Result<bool, String> {
    let path = state
        .dialogs
        .save_file(&export_request(state, kind))
        .map_err(|error| error.to_string())?;
    let Some(path) = path else {
        set_status(app, "Export cancelled");
        return Ok(false);
    };
    let path = ensure_extension(path, kind.extension());
    let image = state.session.borrow().canvas.composite()?;
    let bytes = match kind {
        ExportKind::Png => encode_png(&image)?,
        ExportKind::Jpeg => encode_jpeg(&image, 92)?,
    };
    loom_storage::atomic_write(&path, &bytes)
        .map_err(|error| format!("failed to atomic write '{}': {error}", path.display()))?;
    set_status(app, format!("Exported {}", path.display()));
    Ok(true)
}

fn dispatch_command(app: &PhotoApp, id: &str) -> bool {
    match id {
        "file.new" => app.invoke_new_project(),
        "file.open" => app.invoke_open_project(),
        "file.save" => app.invoke_save_project(),
        "file.save_as" => app.invoke_save_as_project(),
        "file.import_image" => app.invoke_import_image(),
        "file.export_png" => app.invoke_export_png(),
        "file.export_jpeg" => app.invoke_export_jpeg(),
        "edit.undo" => app.invoke_undo(),
        "edit.redo" => app.invoke_redo(),
        "layer.new" => app.invoke_add_layer(),
        "layer.adjustment" => app.invoke_add_adjustment(),
        "layer.delete" => app.invoke_remove_layer(),
        "layer.move_up" => app.invoke_move_layer(1),
        "layer.move_down" => app.invoke_move_layer(-1),
        "view.inspector" => app.invoke_toggle_inspector(),
        "app.palette" => app.invoke_open_palette(),
        _ => return false,
    }
    true
}

fn is_photo_menu_command(id: &str) -> bool {
    matches!(
        id,
        "file.new"
            | "file.open"
            | "file.save"
            | "file.save_as"
            | "file.import_image"
            | "file.export_png"
            | "file.export_jpeg"
            | "edit.undo"
            | "edit.redo"
            | "layer.new"
            | "layer.adjustment"
            | "layer.delete"
            | "layer.move_up"
            | "layer.move_down"
            | "view.inspector"
            | "app.palette"
    )
}

fn schedule_menu_action(
    app_ref: &slint::Weak<PhotoApp>,
    action: CommandAction,
) -> Result<(), DesktopError> {
    if !is_photo_menu_command(&action.id) {
        return Err(DesktopError::InvalidRequest(format!(
            "unsupported Photo menu command {}",
            action.id
        )));
    }
    let id = action.id;
    let error_id = id.clone();
    app_ref
        .upgrade_in_event_loop(move |app| {
            if !dispatch_command(&app, &id) {
                set_status(&app, format!("Unsupported menu command: {id}"));
            }
        })
        .map_err(|error| {
            DesktopError::InvalidRequest(format!(
                "failed to schedule Photo menu command {error_id}: {error}"
            ))
        })
}

fn wire_add_layer_callback(app: &PhotoApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_add_layer(move || {
        if let Some(app) = app_ref.upgrade() {
            let mut session = state.session.borrow_mut();
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
            drop(session);
            if let Err(error) = refresh_photo_with_state(&app, &state) {
                set_status(&app, format!("Operation failed: {error}"));
            }
        }
    });
}

fn wire_move_layer_callback(app: &PhotoApp, state: &Rc<GuiState>) {
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
            drop(session);
            let _ = refresh_photo_with_state(&app, &state);
        }
    });
}

fn wire_inspector_callback(app: &PhotoApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_toggle_inspector(move || {
        if let Some(app) = app_ref.upgrade() {
            if app.get_inspector_available() {
                app.set_show_inspector(!app.get_show_inspector());
                if let Some(menu_service) = &state.menu_service {
                    sync_menu_state(menu_service, &app, &state);
                }
            }
        }
    });
}

fn capture_photo_journey_step(
    app: &PhotoApp,
    state: &GuiState,
    args: &Args,
    out_dir: &Path,
    index: usize,
    name: &str,
) -> Result<String, String> {
    let image = snapshot_component(app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    let file_name = format!("photo-vertical-{index:02}-{name}.png");
    let path = out_dir.join(&file_name);
    loom_test_support::png::save_png(&path, &image).map_err(|error| error.to_string())?;
    let session = state.session.borrow();
    let active = session.canvas.document.active_layer();
    Ok(format!(
        "{index:02} {name} status={:?} active={} transform={:?} selection={} crop={} undo={} pixels={}",
        app.get_status_left().as_str(),
        active.map(|layer| layer.id.as_str()).unwrap_or("none"),
        active.map(|layer| layer.transform),
        format_rect(session.canvas.document.selection),
        format_rect(session.canvas.document.crop),
        session.can_undo(),
        session.canvas.pixel_payload_count(),
    ))
}

/// Record the controller-backed Photo editing journey with per-step screenshots. The existing
/// keyboard palette recorder remains a separate regression at the end of this journey.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("create journey directory: {error}"))?;

    let source_path = out_dir.join("photo-import-source.png");
    let invalid_path = out_dir.join("photo-import-invalid.png");
    std::fs::write(&source_path, sample_raster_payload()?)
        .map_err(|error| format!("write journey source: {error}"))?;
    std::fs::write(&invalid_path, b"not a raster image")
        .map_err(|error| format!("write invalid journey source: {error}"))?;
    let save_path = out_dir.join("photo-vertical.loomphoto");
    let export_path = out_dir.join("photo-vertical.png");
    let failing_export_path = out_dir.join("photo-export-failure.png");
    if failing_export_path.exists() {
        std::fs::remove_dir_all(&failing_export_path)
            .map_err(|error| format!("remove stale failure target: {error}"))?;
    }
    std::fs::create_dir(&failing_export_path)
        .map_err(|error| format!("create failure target: {error}"))?;

    let app = PhotoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    configure_responsive_layout(&app, args.size);
    let dialogs = Rc::new(ScriptedFileDialogs::new(
        [Some(source_path.clone()), Some(invalid_path.clone()), None],
        [
            Some(save_path.clone()),
            Some(export_path.clone()),
            Some(failing_export_path.clone()),
        ],
    ));
    let state = Rc::new(new_gui_state(
        PhotoSession::new(initial_canvas(args)?),
        None,
        dialogs,
    )?);
    wire_transform_callback(&app, &state);
    wire_selection_callbacks(&app, &state);
    wire_adjustment_callbacks(&app, &state);
    wire_import_callback(&app, &state);
    wire_export_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state)?;
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let mut steps = Vec::new();
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 0, "initial",
    )?);

    app.invoke_import_image();
    let imported_id = state
        .session
        .borrow()
        .canvas
        .document
        .active_layer()
        .map(|layer| layer.id.clone())
        .ok_or("journey import has no layer")?;
    if !imported_id.starts_with("layer-imported-")
        || state.session.borrow().canvas.pixel_payload_count() != 1
    {
        return Err("journey import did not preserve a real identified payload".into());
    }
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 1, "imported",
    )?);

    state.session.borrow_mut().canvas.document.select_layer(0);
    set_status(&app, "Selected imported layer");
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 2, "selected",
    )?);

    app.invoke_layer_transform_changed(36.0, 20.0, 118.0, 96.0, 8.0);
    set_status(&app, "Transformed selected layer");
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        3,
        "transformed",
    )?);
    if state.session.borrow().canvas.document.layers[0]
        .transform
        .tx
        .abs()
        < f32::EPSILON
    {
        return Err("journey transform did not mutate the imported layer".into());
    }

    app.invoke_select_layer_bounds();
    set_status(&app, "Selected transformed layer bounds");
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        4,
        "selection",
    )?);
    app.invoke_crop_to_selection();
    set_status(&app, "Cropped preview to selection");
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 5, "cropped",
    )?);
    app.invoke_brightness_changed(35.0);
    set_status(&app, "Adjusted brightness");
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 6, "adjusted",
    )?);
    if (adjustment_value(&state.session.borrow().canvas.document, "brightness") - 0.35).abs()
        > 0.001
    {
        return Err("journey adjustment did not mutate document state".into());
    }

    if !state.session.borrow_mut().undo() {
        return Err("journey adjustment undo was unavailable".into());
    }
    set_status(&app, "Undid brightness adjustment");
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        7,
        "undo-adjustment",
    )?);
    if adjustment_value(&state.session.borrow().canvas.document, "brightness").abs() > 0.001 {
        return Err("journey undo did not restore brightness".into());
    }

    save_current_project(&app, &state, false)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 8, "saved",
    )?);
    let saved_bytes =
        std::fs::read(&save_path).map_err(|error| format!("read saved project: {error}"))?;
    let saved_canvas = load_photo_canvas(&saved_bytes)?;
    let saved_digest = saved_canvas.document.metadata_digest();
    *state.session.borrow_mut() = PhotoSession::new(saved_canvas);
    *state.save_path.borrow_mut() = Some(save_path.clone());
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 9, "reopened",
    )?);
    if state.session.borrow().canvas.document.metadata_digest() != saved_digest {
        return Err("journey save/reopen changed persisted metadata".into());
    }

    app.invoke_export_png();
    let exported =
        std::fs::read(&export_path).map_err(|error| format!("read exported PNG: {error}"))?;
    let decoded_export = decode_raster(&exported)?;
    if (decoded_export.width, decoded_export.height)
        != (
            state.session.borrow().canvas.document.width,
            state.session.borrow().canvas.document.height,
        )
    {
        return Err("journey PNG export dimensions did not match the document".into());
    }
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 10, "exported",
    )?);

    app.invoke_import_image();
    if !app.get_status_left().as_str().contains("Import failed") {
        return Err("journey invalid import did not produce actionable feedback".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        11,
        "import-failure",
    )?);

    app.invoke_export_png();
    if !app.get_status_left().as_str().contains("PNG export failed") {
        return Err("journey invalid export did not produce actionable feedback".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        12,
        "export-failure",
    )?);

    app.invoke_import_image();
    if !app.get_status_left().as_str().contains("Import cancelled") {
        return Err("journey import cancellation response was not observed".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        13,
        "import-cancel",
    )?);

    wire_palette(&app);
    rebuild_palette(&app, "");
    let report = record_keyboard_palette_journey(&app, "photo", out_dir, "layer")
        .map_err(|error| format!("journey failed: {error}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir.display()
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    let log = format!(
        "Photo vertical journey: PASS\njourney=import-select-transform-selection-crop-adjust-undo-save-reopen-export-failures\n{}\n",
        steps.join("\n")
    );
    std::fs::write(out_dir.join("photo-vertical.log"), log)
        .map_err(|error| format!("write journey log: {error}"))?;
    println!("photo journey: PASS ({})", out_dir.display());
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
    drop(session);
    if let Err(error) = refresh_photo_with_state(app, state) {
        set_status(app, format!("Preview failed: {error}"));
    }
}

fn apply_layer_transform(
    state: &GuiState,
    app: &PhotoApp,
    x: f32,
    y: f32,
    scale_x: f32,
    scale_y: f32,
    rotation: f32,
) -> Result<bool, String> {
    let transform = transform_from_components(x, y, scale_x, scale_y, rotation);
    let changed = {
        let mut session = state.session.borrow_mut();
        let index = session.canvas.document.active_layer_index;
        session.set_layer_transform(index, transform)?
    };
    if changed {
        refresh_photo_with_state(app, state)?;
    }
    Ok(changed)
}

fn clamp_rect_to_canvas(rect: Rect, width: u32, height: u32) -> Option<Rect> {
    let x0 = rect.x.max(0.0);
    let y0 = rect.y.max(0.0);
    let x1 = rect.right().min(width as f32);
    let y1 = rect.bottom().min(height as f32);
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

fn select_active_layer_bounds(state: &GuiState, app: &PhotoApp) -> Result<bool, String> {
    let selection = {
        let session = state.session.borrow();
        let bounds = session.canvas.active_layer_bounds();
        bounds.and_then(|bounds| {
            clamp_rect_to_canvas(
                bounds,
                session.canvas.document.width,
                session.canvas.document.height,
            )
        })
    };
    let Some(selection) = selection else {
        return Err("selected layer has no visible bounds".into());
    };
    let changed = state.session.borrow_mut().set_selection(Some(selection))?;
    if changed {
        refresh_photo_with_state(app, state)?;
    }
    Ok(changed)
}

fn crop_to_selection(state: &GuiState, app: &PhotoApp) -> Result<bool, String> {
    let selection = state.session.borrow().canvas.document.selection;
    let Some(selection) = selection else {
        return Err("select a layer region before cropping".into());
    };
    let changed = state.session.borrow_mut().set_crop(Some(selection))?;
    if changed {
        refresh_photo_with_state(app, state)?;
    }
    Ok(changed)
}

fn clear_selection(state: &GuiState, app: &PhotoApp) -> Result<bool, String> {
    let changed = state.session.borrow_mut().set_selection(None)?;
    if changed {
        refresh_photo_with_state(app, state)?;
    }
    Ok(changed)
}

fn clear_crop(state: &GuiState, app: &PhotoApp) -> Result<bool, String> {
    let changed = state.session.borrow_mut().set_crop(None)?;
    if changed {
        refresh_photo_with_state(app, state)?;
    }
    Ok(changed)
}

fn wire_transform_callback(app: &PhotoApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_layer_transform_changed(move |x, y, scale_x, scale_y, rotation| {
        if let Some(app) = app_ref.upgrade() {
            if let Err(error) =
                apply_layer_transform(&state, &app, x, y, scale_x, scale_y, rotation)
            {
                set_status(&app, format!("Transform failed: {error}"));
            }
        }
    });
}

fn wire_selection_callbacks(app: &PhotoApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_layer_bounds(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = select_active_layer_bounds(&state, &app) {
                    set_status(&app, format!("Selection failed: {error}"));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_crop_to_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = crop_to_selection(&state, &app) {
                    set_status(&app, format!("Crop failed: {error}"));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_clear_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = clear_selection(&state, &app) {
                    set_status(&app, format!("Clear selection failed: {error}"));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_clear_crop(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = clear_crop(&state, &app) {
                    set_status(&app, format!("Clear crop failed: {error}"));
                }
            }
        });
    }
}

fn wire_adjustment_callbacks(app: &PhotoApp, state: &Rc<GuiState>) {
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
}

fn wire_import_callback(app: &PhotoApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_import_image(move || {
        if let Some(app) = app_ref.upgrade() {
            match state.dialogs.open_file(&import_image_request(&state)) {
                Ok(Some(path)) => match load_raster_canvas(&path) {
                    Ok(canvas) => {
                        let layer_id = canvas
                            .document
                            .active_layer()
                            .map(|layer| layer.id.clone())
                            .unwrap_or_else(|| "unknown".into());
                        *state.session.borrow_mut() = PhotoSession::new(canvas);
                        *state.save_path.borrow_mut() = None;
                        if let Err(error) = refresh_photo_with_state(&app, &state) {
                            set_status(&app, format!("Import preview failed: {error}"));
                        } else {
                            set_status(
                                &app,
                                format!(
                                    "Imported real raster layer {layer_id}; save as a Loom Photo project to preserve edits"
                                ),
                            );
                        }
                    }
                    Err(error) => set_status(&app, format!("Import failed: {error}")),
                },
                Ok(None) => set_status(&app, "Import cancelled"),
                Err(error) => set_status(&app, format!("Import dialog failed: {error}")),
            }
        }
    });
}

fn wire_export_callbacks(app: &PhotoApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_png(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = export_current_image(&app, &state, ExportKind::Png) {
                    set_status(&app, format!("PNG export failed: {error}"));
                }
                if let Some(menu_service) = &state.menu_service {
                    sync_menu_state(menu_service, &app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_jpeg(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = export_current_image(&app, &state, ExportKind::Jpeg) {
                    set_status(&app, format!("JPEG export failed: {error}"));
                }
                if let Some(menu_service) = &state.menu_service {
                    sync_menu_state(menu_service, &app, &state);
                }
            }
        });
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
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    configure_responsive_layout(&app, args.size);
    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_canvas(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_photo_canvas(bytes).ok())
            .unwrap_or(initial_canvas(&args)?)
    };
    let initial_path = args
        .open
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| is_native_project(path));
    let menu_service = Rc::new(NativeMenuBar::new());
    let mut gui_state = new_gui_state(
        PhotoSession::new(initial),
        initial_path,
        Rc::new(NativeFileDialogs),
    )?;
    gui_state.menu_service = Some(menu_service.clone());
    let state = Rc::new(gui_state);

    macro_rules! mutate_and_refresh {
        ($callback:ident, $body:expr) => {{
            let state = state.clone();
            let app_ref = app.as_weak();
            app.$callback(move || {
                if let Some(app) = app_ref.upgrade() {
                    let mut session = state.session.borrow_mut();
                    ($body)(&mut session);
                    drop(session);
                    if let Err(error) = refresh_photo_with_state(&app, &state) {
                        set_status(&app, format!("Operation failed: {error}"));
                    }
                }
            });
        }};
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                match blank_canvas() {
                    Ok(canvas) => {
                        *state.session.borrow_mut() = PhotoSession::new(canvas);
                        *state.save_path.borrow_mut() = None;
                        if let Err(error) = refresh_photo_with_state(&app, &state) {
                            set_status(&app, format!("New project failed: {error}"));
                        } else {
                            set_status(&app, "Created unsaved photo project");
                        }
                    }
                    Err(error) => set_status(&app, format!("New project failed: {error}")),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_project(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&open_project_request(&state)) {
                    Ok(Some(path)) => match load_project(&path) {
                        Ok(canvas) => {
                            *state.session.borrow_mut() = PhotoSession::new(canvas);
                            *state.save_path.borrow_mut() = Some(path.clone());
                            if let Err(error) = refresh_photo_with_state(&app, &state) {
                                set_status(&app, format!("Open preview failed: {error}"));
                            } else {
                                set_status(&app, format!("Opened {}", path.display()));
                            }
                        }
                        Err(error) => set_status(&app, format!("Open failed: {error}")),
                    },
                    Ok(None) => set_status(&app, "Open cancelled"),
                    Err(error) => set_status(&app, format!("Open dialog failed: {error}")),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_project(&app, &state, false) {
                    set_status(&app, format!("Save failed: {error}"));
                }
                if let Some(menu_service) = &state.menu_service {
                    sync_menu_state(menu_service, &app, &state);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_project(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_project(&app, &state, true) {
                    set_status(&app, format!("Save As failed: {error}"));
                }
                if let Some(menu_service) = &state.menu_service {
                    sync_menu_state(menu_service, &app, &state);
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
        app.on_select_layer(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    state
                        .session
                        .borrow_mut()
                        .canvas
                        .document
                        .select_layer(index as usize);
                    let _ = refresh_photo_with_state(&app, &state);
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
                    drop(session);
                    let _ = refresh_photo_with_state(&app, &state);
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
                drop(session);
                let _ = refresh_photo_with_state(&app, &state);
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
                        "Darken" => BlendMode::Darken,
                        "Lighten" => BlendMode::Lighten,
                        "Difference" => BlendMode::Difference,
                        "Hard Light" => BlendMode::HardLight,
                        _ => BlendMode::Normal,
                    };
                }
                drop(session);
                let _ = refresh_photo_with_state(&app, &state);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_select_tool(move |tool| {
            if let Some(app) = app_ref.upgrade() {
                let (active_tool, pan_mode) = photo_tool_state(tool.as_str());
                app.set_active_tool(active_tool);
                app.set_pan_mode(pan_mode);
                set_status(&app, format!("{} tool selected", tool.as_str()));
            }
        });
    }

    wire_transform_callback(&app, &state);
    wire_selection_callbacks(&app, &state);
    wire_adjustment_callbacks(&app, &state);
    wire_import_callback(&app, &state);
    wire_export_callbacks(&app, &state);

    wire_responsive_layout(&app, state.clone());
    wire_add_layer_callback(&app, &state);
    wire_move_layer_callback(&app, &state);
    wire_inspector_callback(&app, &state);
    let menu_bar = build_photo_menu_bar(&app);
    menu_service
        .install_menu_bar(&menu_bar)
        .map_err(|error| error.to_string())?;
    let app_ref = app.as_weak();
    menu_service
        .register_action_sink(std::sync::Arc::new(move |action: CommandAction| {
            schedule_menu_action(&app_ref, action)
        }))
        .map_err(|error| error.to_string())?;
    wire_palette(&app);

    refresh_photo_with_state(&app, &state)?;
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
                let Some(item) = app.get_palette_commands().row_data(index as usize) else {
                    return;
                };
                if !item.enabled {
                    return;
                }
                let command = master_palette(&app)
                    .into_iter()
                    .find(|command| command.id == item.id.as_str())
                    .filter(|command| palette_action_enabled(&app, &command.action));
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewProject => app.invoke_new_project(),
                        PaletteAction::OpenProject => app.invoke_open_project(),
                        PaletteAction::SaveProject => app.invoke_save_project(),
                        PaletteAction::SaveAsProject => app.invoke_save_as_project(),
                        PaletteAction::ImportImage => app.invoke_import_image(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddLayer => app.invoke_add_layer(),
                        PaletteAction::AddAdjustment => app.invoke_add_adjustment(),
                        PaletteAction::RemoveLayer => app.invoke_remove_layer(),
                        PaletteAction::MoveLayer(direction) => app.invoke_move_layer(direction),
                        PaletteAction::SelectTool(tool) => {
                            app.invoke_select_tool(SharedString::from(tool))
                        }
                        PaletteAction::ExportPng => app.invoke_export_png(),
                        PaletteAction::ExportJpeg => app.invoke_export_jpeg(),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::ScriptedFileDialogs;

    fn scripted_state() -> GuiState {
        new_gui_state(
            PhotoSession::new(blank_canvas().expect("blank canvas")),
            None,
            Rc::new(ScriptedFileDialogs::new(
                [
                    Some(PathBuf::from("opened.loomphoto")),
                    Some(PathBuf::from("source.png")),
                ],
                [
                    Some(PathBuf::from("saved")),
                    Some(PathBuf::from("exported")),
                    Some(PathBuf::from("exported-jpeg")),
                ],
            )),
        )
        .expect("state")
    }

    #[test]
    fn dialog_requests_keep_projects_imports_and_exports_separate() {
        let state = scripted_state();
        let project = open_project_request(&state);
        let import = import_image_request(&state);
        let save = save_project_request(&state);
        let png = export_request(&state, ExportKind::Png);
        let jpeg = export_request(&state, ExportKind::Jpeg);

        assert_eq!(project.filters[0].extensions, vec!["loomphoto".to_string()]);
        assert_eq!(
            import.filters[0].extensions,
            vec!["png".to_string(), "jpg".to_string(), "jpeg".to_string()]
        );
        assert_eq!(save.suggested_name.as_deref(), Some(SAVE_FILENAME));
        assert_eq!(png.filters[0].extensions, vec!["png".to_string()]);
        assert_eq!(
            jpeg.filters[0].extensions,
            vec!["jpg".to_string(), "jpeg".to_string()]
        );
    }

    #[test]
    fn scripted_dialog_backend_drives_all_photo_file_operations() {
        let state = scripted_state();
        assert_eq!(
            state
                .dialogs
                .open_file(&open_project_request(&state))
                .expect("project picker"),
            Some(PathBuf::from("opened.loomphoto"))
        );
        assert_eq!(
            state
                .dialogs
                .open_file(&import_image_request(&state))
                .expect("import picker"),
            Some(PathBuf::from("source.png"))
        );
        assert_eq!(
            state
                .dialogs
                .save_file(&save_project_request(&state))
                .expect("save picker"),
            Some(PathBuf::from("saved"))
        );
        assert_eq!(
            state
                .dialogs
                .save_file(&export_request(&state, ExportKind::Png))
                .expect("png picker"),
            Some(PathBuf::from("exported"))
        );
        assert_eq!(
            state
                .dialogs
                .save_file(&export_request(&state, ExportKind::Jpeg))
                .expect("jpeg picker"),
            Some(PathBuf::from("exported-jpeg"))
        );
    }

    #[test]
    fn imported_rasters_never_become_project_save_paths() {
        assert!(is_native_project(Path::new("project.loomphoto")));
        assert!(!is_native_project(Path::new("source.png")));
        assert_eq!(
            ensure_extension(PathBuf::from("project"), "loomphoto"),
            PathBuf::from("project.loomphoto")
        );
        assert_eq!(
            ensure_extension(PathBuf::from("already.jpeg"), "jpg"),
            PathBuf::from("already.jpeg")
        );
    }

    #[test]
    fn pan_tool_is_a_viewport_mode_separate_from_brush() {
        assert_eq!(photo_tool_state("Pan"), (0, true));
        assert_eq!(photo_tool_state("brush"), (1, false));
        assert_eq!(photo_tool_state("WAND"), (2, false));
        assert_eq!(photo_tool_state("unknown"), (0, false));
    }

    #[test]
    fn imported_raster_canvas_preserves_payload_identity_through_reopen() {
        let path =
            std::env::temp_dir().join(format!("loom-photo-import-test-{}.png", std::process::id()));
        let source = sample_raster_payload().expect("sample payload");
        std::fs::write(&path, source).expect("write sample payload");
        let canvas = load_raster_canvas(&path).expect("decode imported raster");
        let layer = canvas.document.active_layer().expect("imported layer");
        assert!(layer.id.starts_with("layer-imported-"));
        assert_eq!(
            layer.source_digest,
            canvas.layer_image(&layer.id).map(RgbaImage::pixel_digest)
        );
        assert_eq!(canvas.pixel_payload_count(), 1);

        let bytes = save_photo_canvas(&canvas).expect("save imported canvas");
        let reopened = load_photo_canvas(&bytes).expect("reopen imported canvas");
        let reopened_layer = reopened.document.active_layer().expect("reopened layer");
        assert_eq!(reopened_layer.id, layer.id);
        assert_eq!(reopened_layer.source_digest, layer.source_digest);
        assert_eq!(reopened.pixel_payload_count(), 1);
        assert_eq!(
            reopened
                .layer_image(&reopened_layer.id)
                .map(RgbaImage::pixel_digest),
            canvas.layer_image(&layer.id).map(RgbaImage::pixel_digest)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn photo_edit_callbacks_mutate_selection_transform_crop_and_adjustment() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        let state = Rc::new(scripted_state());
        wire_transform_callback(&app, &state);
        wire_selection_callbacks(&app, &state);
        wire_adjustment_callbacks(&app, &state);
        refresh_photo_with_state(&app, &state).expect("initial refresh");

        app.invoke_layer_transform_changed(12.0, 8.0, 125.0, 100.0, 4.0);
        let transformed = state.session.borrow().canvas.document.layers[0].transform;
        assert_eq!(transformed.tx, 12.0);
        assert_eq!(transformed.ty, 8.0);
        assert!(state.session.borrow().can_undo());

        app.invoke_select_layer_bounds();
        assert!(state.session.borrow().canvas.document.selection.is_some());
        app.invoke_crop_to_selection();
        assert!(state.session.borrow().canvas.document.crop.is_some());
        app.invoke_brightness_changed(25.0);
        assert!(
            (adjustment_value(&state.session.borrow().canvas.document, "brightness") - 0.25).abs()
                < 0.001
        );

        assert!(state.session.borrow_mut().undo());
        assert!(
            adjustment_value(&state.session.borrow().canvas.document, "brightness").abs() < 0.001
        );
        let bytes = save_photo_canvas(&state.session.borrow().canvas).expect("save edits");
        let reopened = load_photo_canvas(&bytes).expect("reopen edits");
        assert_eq!(
            reopened.document.layers[0].transform,
            state.session.borrow().canvas.document.layers[0].transform
        );
        assert_eq!(
            reopened.document.selection,
            state.session.borrow().canvas.document.selection
        );
        assert_eq!(
            reopened.document.crop,
            state.session.borrow().canvas.document.crop
        );
    }

    #[test]
    fn photo_menu_uses_canonical_commands_and_disables_unhandled_items() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        let menu = build_photo_menu_bar(&app);

        for id in [
            "file.new",
            "file.open",
            "file.save",
            "file.save_as",
            "file.import_image",
            "file.export_png",
            "file.export_jpeg",
            "edit.undo",
            "edit.redo",
            "layer.new",
            "layer.adjustment",
            "layer.delete",
            "layer.move_up",
            "layer.move_down",
            "view.inspector",
            "app.palette",
        ] {
            assert!(
                menu.find_item(id).is_some(),
                "missing Photo menu command {id}"
            );
        }
        for id in [
            "edit.cut",
            "edit.copy",
            "edit.paste",
            "edit.select_all",
            "view.zoom_in",
            "view.zoom_out",
            "view.zoom_actual",
            "layer.duplicate",
        ] {
            if let Some(item) = menu.find_item(id) {
                assert!(
                    !item.is_enabled(),
                    "unhandled Photo command {id} must be disabled"
                );
            }
        }
    }

    #[test]
    fn photo_menu_projection_tracks_history_layer_boundaries_and_inspector() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        configure_responsive_layout(&app, (1280, 800));
        let state = scripted_state();
        let menu = NativeMenuBar::new();
        menu.install_menu_bar(&build_photo_menu_bar(&app))
            .expect("install menu");

        sync_menu_state(&menu, &app, &state);
        let installed = menu.installed_menu_bar().expect("installed menu");
        assert!(matches!(
            installed.find_item("edit.undo"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("edit.redo"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("layer.delete"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("layer.move_up"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("layer.move_down"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("view.inspector"),
            Some(MenuItem::Check {
                checked,
                enabled: true,
                ..
            }) if *checked == app.get_show_inspector()
        ));

        state
            .session
            .borrow_mut()
            .add_pixel_layer("layer-2", "Pixel Layer 2");
        sync_menu_state(&menu, &app, &state);
        let installed = menu.installed_menu_bar().expect("installed menu");
        assert!(matches!(
            installed.find_item("layer.delete"),
            Some(MenuItem::Action { enabled: true, .. })
        ));
        assert!(matches!(
            installed.find_item("layer.move_up"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("layer.move_down"),
            Some(MenuItem::Action { enabled: true, .. })
        ));

        state.session.borrow_mut().canvas.document.select_layer(0);
        sync_menu_state(&menu, &app, &state);
        let installed = menu.installed_menu_bar().expect("installed menu");
        assert!(matches!(
            installed.find_item("layer.move_up"),
            Some(MenuItem::Action { enabled: true, .. })
        ));
        assert!(matches!(
            installed.find_item("layer.move_down"),
            Some(MenuItem::Action { enabled: false, .. })
        ));

        app.set_show_inspector(false);
        sync_menu_state(&menu, &app, &state);
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                checked: false,
                enabled: true,
                ..
            })
        ));
    }

    #[test]
    fn photo_menu_sink_preserves_source_mutates_once_and_guards_boundaries() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        let state = Rc::new(scripted_state());
        let menu = Rc::new(NativeMenuBar::new());
        menu.install_menu_bar(&build_photo_menu_bar(&app))
            .expect("install menu");

        wire_add_layer_callback(&app, &state);
        wire_move_layer_callback(&app, &state);
        let app_ref = app.as_weak();
        let observed_source = std::sync::Arc::new(std::sync::Mutex::new(None));
        let observed_source_ref = observed_source.clone();
        menu.register_action_sink(std::sync::Arc::new(
            move |action: loom_desktop::CommandAction| {
                *observed_source_ref.lock().expect("source lock") = Some(action.source);
                let app = app_ref.upgrade().ok_or_else(|| {
                    loom_desktop::DesktopError::InvalidRequest("Photo app was dropped".into())
                })?;
                if dispatch_command(&app, &action.id) {
                    Ok(())
                } else {
                    Err(loom_desktop::DesktopError::InvalidRequest(format!(
                        "unsupported Photo menu command {}",
                        action.id
                    )))
                }
            },
        ))
        .expect("register sink");

        sync_menu_state(&menu, &app, &state);
        let before = state.session.borrow().canvas.document.layers.len();
        menu.dispatch_action_from("layer.new", loom_desktop::CommandSource::Menu)
            .expect("enabled layer.new");
        assert_eq!(
            state.session.borrow().canvas.document.layers.len(),
            before + 1
        );
        assert_eq!(
            *observed_source.lock().expect("source lock"),
            Some(loom_desktop::CommandSource::Menu)
        );

        state.session.borrow_mut().canvas.document.select_layer(0);
        sync_menu_state(&menu, &app, &state);
        let order_before = state
            .session
            .borrow()
            .canvas
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(order_before, vec!["layer-bg", "layer-2"]);
        menu.dispatch_action_from("layer.move_up", loom_desktop::CommandSource::Menu)
            .expect("enabled layer.move_up");
        let session = state.session.borrow();
        assert_eq!(session.canvas.document.active_layer_index, 1);
        assert_eq!(
            session
                .canvas
                .document
                .layers
                .iter()
                .map(|layer| layer.id.as_str())
                .collect::<Vec<_>>(),
            vec!["layer-2", "layer-bg"]
        );
        drop(session);

        *state.session.borrow_mut() = PhotoSession::new(blank_canvas().expect("blank canvas"));
        sync_menu_state(&menu, &app, &state);
        let before = state.session.borrow().canvas.document.layers.len();
        assert!(menu
            .dispatch_action_from("layer.delete", loom_desktop::CommandSource::Menu)
            .is_err());
        assert_eq!(
            state.session.borrow().canvas.document.layers.len(),
            before,
            "one-layer delete stays unchanged"
        );
        assert!(menu
            .dispatch_action_from("layer.move_up", loom_desktop::CommandSource::Menu)
            .is_err());
        assert!(menu
            .dispatch_action_from("layer.move_down", loom_desktop::CommandSource::Menu)
            .is_err());
        assert_eq!(
            state.session.borrow().canvas.document.active_layer_index,
            0,
            "one-layer move boundaries stay unchanged"
        );
    }

    #[test]
    fn photo_toolbar_inspector_toggle_reprojects_checked_state() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        configure_responsive_layout(&app, (1280, 800));
        let menu = Rc::new(NativeMenuBar::new());
        menu.install_menu_bar(&build_photo_menu_bar(&app))
            .expect("install menu");
        let mut gui_state = scripted_state();
        gui_state.menu_service = Some(menu.clone());
        let state = Rc::new(gui_state);
        wire_inspector_callback(&app, &state);
        sync_menu_state(&menu, &app, &state);
        assert!(app.get_show_inspector());

        app.invoke_toggle_inspector();

        assert!(!app.get_show_inspector());
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                checked: false,
                enabled: true,
                ..
            })
        ));
    }

    #[test]
    fn photo_palette_move_commands_follow_layer_boundaries() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        let state = scripted_state();

        let has_command = |id: &str| master_palette(&app).iter().any(|command| command.id == id);

        refresh_photo(&app, &state.session.borrow()).expect("refresh first layer");
        assert!(!app.get_can_move_up());
        assert!(!app.get_can_move_down());
        assert!(!has_command("photo.layer.move-up"));
        assert!(!has_command("photo.layer.move-down"));

        state
            .session
            .borrow_mut()
            .add_pixel_layer("layer-2", "Pixel Layer 2");
        refresh_photo(&app, &state.session.borrow()).expect("refresh last layer");
        assert!(!app.get_can_move_up());
        assert!(app.get_can_move_down());
        assert!(!has_command("photo.layer.move-up"));
        assert!(has_command("photo.layer.move-down"));

        state
            .session
            .borrow_mut()
            .add_pixel_layer("layer-3", "Pixel Layer 3");
        state.session.borrow_mut().canvas.document.select_layer(1);
        refresh_photo(&app, &state.session.borrow()).expect("refresh middle layer");
        assert!(app.get_can_move_up());
        assert!(app.get_can_move_down());
        assert!(has_command("photo.layer.move-up"));
        assert!(has_command("photo.layer.move-down"));

        state.session.borrow_mut().canvas.document.select_layer(0);
        refresh_photo(&app, &state.session.borrow()).expect("refresh first layer");
        assert!(app.get_can_move_up());
        assert!(!app.get_can_move_down());
        assert!(has_command("photo.layer.move-up"));
        assert!(!has_command("photo.layer.move-down"));
    }

    #[test]
    fn photo_palette_invocation_uses_visible_action_after_boundary_change() {
        set_platform();
        let app = PhotoApp::new().expect("create PhotoApp");
        let state = Rc::new(scripted_state());
        wire_move_layer_callback(&app, &state);
        wire_palette(&app);

        state
            .session
            .borrow_mut()
            .add_pixel_layer("layer-2", "Pixel Layer 2");
        refresh_photo(&app, &state.session.borrow()).expect("refresh last layer");
        app.set_palette_query("move layer".into());
        rebuild_palette(&app, "move layer");
        assert_eq!(app.get_palette_commands().row_count(), 1);
        assert_eq!(
            app.get_palette_commands()
                .row_data(0)
                .expect("visible move command")
                .id,
            "photo.layer.move-down"
        );

        state.session.borrow_mut().canvas.document.select_layer(0);
        refresh_photo(&app, &state.session.borrow()).expect("refresh first layer");
        assert!(!app.get_can_move_down());
        app.invoke_palette_invoked(0);
        assert_eq!(
            state.session.borrow().canvas.document.active_layer_index,
            0,
            "stale visible move-down must not dispatch move-up"
        );
    }
}
