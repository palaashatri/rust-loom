//! Loom Photo desktop application.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{
    build_standard_menu_bar, CommandAction, CommandStateProjection, DesktopError,
    FileDialogService, FileFilter, Menu, MenuBar, MenuBarService, MenuItem, MenuShortcut,
    NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
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
    let mut document = PhotoDocument::new("imported-photo", name, image.width, image.height);
    document.dpi = 144;
    let mut canvas = PhotoCanvas::new(document)?;
    canvas.set_layer_image("layer-bg", image)?;
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
    let last_layer_index = document.layers.len().saturating_sub(1);
    app.set_can_move_up(document.active_layer_index < last_layer_index);
    app.set_can_move_down(document.active_layer_index > 0);
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
                BlendMode::Darken => "Darken",
                BlendMode::Lighten => "Lighten",
                BlendMode::Difference => "Difference",
                BlendMode::HardLight => "Hard Light",
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

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = PhotoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    configure_responsive_layout(&app, args.size);
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
        app.on_import_image(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&import_image_request(&state)) {
                    Ok(Some(path)) => match load_raster_canvas(&path) {
                        Ok(canvas) => {
                            *state.session.borrow_mut() = PhotoSession::new(canvas);
                            *state.save_path.borrow_mut() = None;
                            if let Err(error) = refresh_photo_with_state(&app, &state) {
                                set_status(&app, format!("Import preview failed: {error}"));
                            } else {
                                set_status(
                                    &app,
                                    format!(
                                        "Imported {}; save as a Loom Photo project to preserve edits",
                                        path.display()
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
