//! Loom Present desktop presentation application.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{
    build_standard_menu_bar, CommandAction, CommandStateProjection, DesktopError,
    FileDialogService, FileFilter, Menu, MenuBar, MenuBarService, MenuItem, MenuShortcut,
    NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_present_core::{
    calculate_smart_snapping, export_pdf, load_presentation_session, normalize_angle_degrees,
    save_presentation_session, ElementType, PresentationDocument, PresentationSession,
    SlideElement, SnapGuide, TransitionKind,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::PaletteProbe;
use slint::{
    private_unstable_api::re_exports::EventResult, ComponentHandle, Model, ModelRc, PhysicalSize,
    SharedString, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "presentation.loomdeck";
const EXPORT_FILENAME: &str = "presentation.pdf";

loom_production::define_snapshot_recovery!(PRESENT_RECOVERY, "org.loom.present", "loom.present/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    rtl: bool,
    open: Option<String>,
    theme_chooser: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        journey: None,
        size: DEFAULT_SIZE,
        theme: "dark".into(),
        rtl: false,
        open: None,
        theme_chooser: false,
    };
    let mut iterator = std::env::args().skip(1);
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--screenshot" => {
                args.screenshot = Some(iterator.next().ok_or("--screenshot needs a path")?)
            }
            "--smoke" => args.smoke = true,
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(
                    iterator
                        .next()
                        .ok_or("--journey needs an output directory")?,
                );
            }
            "--size" => {
                let value = iterator.next().ok_or("--size needs WxH")?;
                let (width, height) = value.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    width.parse().map_err(|_| "bad width")?,
                    height.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => args.theme = iterator.next().ok_or("--theme needs a name")?,
            "--rtl" => args.rtl = true,
            "--theme-chooser" => args.theme_chooser = true,
            "--open" => args.open = Some(iterator.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn text_element(
    id: &str,
    kind: ElementType,
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> SlideElement {
    SlideElement {
        id: id.into(),
        element_type: kind,
        content: content.into(),
        x,
        y,
        width,
        height,
        rotation_deg: 0.0,
        action: None,
    }
}

fn sample_session() -> PresentationSession {
    let mut document = PresentationDocument::new("deck-sample", "Loom for Local Creators");
    if let Some(slide) = document.active_slide_mut() {
        slide.title = "Create without compromise".into();
        slide.elements.clear();
        slide.add_element(text_element(
            "cover-title",
            ElementType::Title,
            "Create without compromise",
            90.0,
            90.0,
            820.0,
            110.0,
        ));
        slide.add_element(text_element(
            "cover-body",
            ElementType::BodyText,
            "A private, native creative studio designed for Linux.",
            92.0,
            230.0,
            700.0,
            120.0,
        ));
    }
    document.add_slide("The creative system", "content");
    if let Some(slide) = document.active_slide_mut() {
        slide.add_element(text_element(
            "system-title",
            ElementType::Title,
            "The creative system",
            80.0,
            70.0,
            820.0,
            90.0,
        ));
        slide.add_element(text_element("system-body", ElementType::BodyText, "Writer, Sheets, Present, Photo, Motion, Video, Studio and Encode share one local-first foundation.", 82.0, 190.0, 760.0, 170.0));
    }
    document.add_slide("Built around ownership", "two-column");
    if let Some(slide) = document.active_slide_mut() {
        slide.add_element(text_element(
            "ownership-title",
            ElementType::Title,
            "Built around ownership",
            80.0,
            70.0,
            820.0,
            90.0,
        ));
        slide.add_element(text_element("ownership-body", ElementType::BodyText, "No required account. No hidden upload. Open formats, local models, and files that remain yours.", 82.0, 190.0, 760.0, 170.0));
    }
    document.active_index = 0;
    let mut session = PresentationSession::new(document);
    let first = session.document.slides[0].id.clone();
    session.set_transition(&first, TransitionKind::Dissolve);
    session
}

fn empty_session() -> PresentationSession {
    let mut document = PresentationDocument::new("untitled-deck", "Untitled Presentation");
    if let Some(slide) = document.active_slide_mut() {
        slide.title = "Untitled Slide".into();
        for element in &mut slide.elements {
            element.content.clear();
        }
    }
    PresentationSession::new(document)
}

fn load_session(path: &Path) -> Result<PresentationSession, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read presentation '{}': {error}", path.display()))?;
    load_presentation_session(&bytes)
        .map_err(|error| format!("failed to load presentation '{}': {error}", path.display()))
}

fn initial_session(args: &Args) -> Result<PresentationSession, String> {
    match args.open.as_deref() {
        Some(path) => load_session(Path::new(path)),
        None => Ok(sample_session()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum HandleKind {
    #[default]
    Move,
    ResizeNorthWest,
    ResizeNorthEast,
    ResizeSouthWest,
    ResizeSouthEast,
    Rotate,
    Marquee,
}

#[derive(Debug, Clone)]
struct DragElement {
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation_deg: f32,
}

type TransformTarget = (String, f32, f32, f32, f32);

impl DragElement {
    fn transformed_bounds(&self) -> (f32, f32, f32, f32) {
        let radians = self.rotation_deg.to_radians();
        let (sin, cos) = radians.sin_cos();
        let cx = self.x + self.width / 2.0;
        let cy = self.y + self.height / 2.0;
        let corners = [
            (self.x - cx, self.y - cy),
            (self.x + self.width - cx, self.y - cy),
            (self.x + self.width - cx, self.y + self.height - cy),
            (self.x - cx, self.y + self.height - cy),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (x, y) in corners {
            let rotated_x = cx + x * cos - y * sin;
            let rotated_y = cy + x * sin + y * cos;
            min_x = min_x.min(rotated_x);
            min_y = min_y.min(rotated_y);
            max_x = max_x.max(rotated_x);
            max_y = max_y.max(rotated_y);
        }
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

#[derive(Default, Clone)]
struct DragState {
    mode: Option<HandleKind>,
    start_mouse_x: f32,
    start_mouse_y: f32,
    elements: Vec<DragElement>,
    target_id: Option<String>,
    checkpointed: bool,
    before_digest: Option<u64>,
    before_selection: Vec<String>,
    before_selected_element: usize,
    marquee_additive: bool,
    guides: Vec<SnapGuide>,
    marquee_x: f32,
    marquee_y: f32,
    marquee_width: f32,
    marquee_height: f32,
}

impl DragState {
    fn begin(&mut self, session: &PresentationSession, selected_element: usize) {
        self.target_id = None;
        self.checkpointed = false;
        self.before_digest = Some(session.document.integrity_digest());
        self.before_selection = session.selected_elements.clone();
        self.before_selected_element = selected_element;
        self.marquee_additive = false;
        self.guides.clear();
        self.elements.clear();
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

fn finish_drag(session: &mut PresentationSession, drag: &mut DragState) {
    if drag.checkpointed && drag.before_digest == Some(session.document.integrity_digest()) {
        let _ = session.cancel_checkpoint();
    }
    drag.reset();
}

fn cancel_drag(
    session: &mut PresentationSession,
    drag: &mut DragState,
    selected_element: &Cell<usize>,
) {
    if drag.checkpointed {
        let _ = session.cancel_checkpoint();
    }
    session.selected_elements = drag.before_selection.clone();
    selected_element.set(drag.before_selected_element);
    drag.reset();
}

struct GuiState {
    session: RefCell<PresentationSession>,
    selected_element: Cell<usize>,
    inspector_available: Cell<bool>,
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    deck_filter: FileFilter,
    pdf_filter: FileFilter,
    menu_service: Option<Rc<NativeMenuBar>>,
    drag_state: RefCell<DragState>,
}

fn active_body(session: &PresentationSession) -> String {
    session
        .document
        .active_slide()
        .and_then(|slide| {
            slide
                .elements
                .iter()
                .find(|element| element.element_type == ElementType::BodyText)
        })
        .map(|element| element.content.clone())
        .unwrap_or_else(|| "Add supporting content from the toolbar.".into())
}

/// Stable scene-type ids consumed by the Slint canvas projection. Keep this
/// mapping local to the view model so the domain enum remains independent of
/// UI rendering details.
fn element_type_index(element_type: &ElementType) -> i32 {
    match element_type {
        ElementType::Title => 0,
        ElementType::Subtitle => 1,
        ElementType::BodyText => 2,
        ElementType::ShapeRectangle => 3,
        ElementType::ShapeCircle => 4,
        ElementType::StatCard => 5,
    }
}

fn handle_kind(value: &str) -> HandleKind {
    match value {
        "nw" => HandleKind::ResizeNorthWest,
        "ne" => HandleKind::ResizeNorthEast,
        "sw" => HandleKind::ResizeSouthWest,
        "se" => HandleKind::ResizeSouthEast,
        "rotate" => HandleKind::Rotate,
        _ => HandleKind::Move,
    }
}

fn drag_snapshots(session: &PresentationSession) -> Vec<DragElement> {
    let Some(slide) = session.document.active_slide() else {
        return Vec::new();
    };
    slide
        .elements
        .iter()
        .filter(|element| session.selected_elements.iter().any(|id| id == &element.id))
        .map(|element| DragElement {
            id: element.id.clone(),
            x: element.x,
            y: element.y,
            width: element.width,
            height: element.height,
            rotation_deg: element.rotation_deg,
        })
        .collect()
}

fn reference_bounds(
    session: &PresentationSession,
    selected: &[DragElement],
) -> Vec<(f32, f32, f32, f32)> {
    let Some(slide) = session.document.active_slide() else {
        return Vec::new();
    };
    slide
        .elements
        .iter()
        .filter(|element| !selected.iter().any(|item| item.id == element.id))
        .map(SlideElement::transformed_bounds)
        .collect()
}

fn move_targets(
    session: &PresentationSession,
    drag: &[DragElement],
    dx: f32,
    dy: f32,
) -> (Vec<TransformTarget>, Vec<SnapGuide>) {
    if drag.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for element in drag {
        let (x, y, width, height) = element.transformed_bounds();
        min_x = min_x.min(x + dx);
        min_y = min_y.min(y + dy);
        max_x = max_x.max(x + width + dx);
        max_y = max_y.max(y + height + dy);
    }
    let snap = calculate_smart_snapping(
        (min_x, min_y, max_x - min_x, max_y - min_y),
        &reference_bounds(session, drag),
        8.0,
    );
    let correction_x = snap.snapped_x - min_x;
    let correction_y = snap.snapped_y - min_y;
    (
        drag.iter()
            .map(|element| {
                (
                    element.id.clone(),
                    element.x + dx + correction_x,
                    element.y + dy + correction_y,
                    element.width,
                    element.height,
                )
            })
            .collect(),
        snap.guides,
    )
}

fn resize_target(
    element: &DragElement,
    handle: HandleKind,
    dx: f32,
    dy: f32,
) -> (f32, f32, f32, f32) {
    let right = element.x + element.width;
    let bottom = element.y + element.height;
    let mut x = element.x;
    let mut y = element.y;
    let mut width = element.width;
    let mut height = element.height;
    if matches!(
        handle,
        HandleKind::ResizeNorthWest | HandleKind::ResizeSouthWest
    ) {
        x = (element.x + dx).min(right - 1.0);
        width = right - x;
    }
    if matches!(
        handle,
        HandleKind::ResizeNorthEast | HandleKind::ResizeSouthEast
    ) {
        width = (element.width + dx).max(1.0);
    }
    if matches!(
        handle,
        HandleKind::ResizeNorthWest | HandleKind::ResizeNorthEast
    ) {
        y = (element.y + dy).min(bottom - 1.0);
        height = bottom - y;
    }
    if matches!(
        handle,
        HandleKind::ResizeSouthWest | HandleKind::ResizeSouthEast
    ) {
        height = (element.height + dy).max(1.0);
    }
    (x, y, width.max(1.0), height.max(1.0))
}

fn resize_targets(
    session: &PresentationSession,
    element: &DragElement,
    handle: HandleKind,
    dx: f32,
    dy: f32,
) -> ((f32, f32, f32, f32), Vec<SnapGuide>) {
    let (x, y, width, height) = resize_target(element, handle, dx, dy);
    let snap = calculate_smart_snapping(
        (x, y, width, height),
        &reference_bounds(session, std::slice::from_ref(element)),
        8.0,
    );
    ((snap.snapped_x, snap.snapped_y, width, height), snap.guides)
}

fn nudge_selected(session: &mut PresentationSession, dx: f32, dy: f32) -> bool {
    let selected = drag_snapshots(session);
    let (targets, _) = move_targets(session, &selected, dx, dy);
    let changed = targets.iter().any(|(id, x, y, width, height)| {
        session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|element| element.id == *id))
            .map(|element| {
                (element.x - *x).abs() > f32::EPSILON
                    || (element.y - *y).abs() > f32::EPSILON
                    || (element.width - *width).abs() > f32::EPSILON
                    || (element.height - *height).abs() > f32::EPSILON
            })
            .unwrap_or(false)
    });
    if !changed {
        return false;
    }
    session.checkpoint();
    for (id, x, y, width, height) in targets {
        session.transform_element_no_checkpoint(&id, x, y, width, height);
    }
    true
}

fn refresh(app: &PresentApp, state: &GuiState) {
    refresh_with_recovery(app, state, true);
}

fn refresh_without_recovery(app: &PresentApp, state: &GuiState) {
    refresh_with_recovery(app, state, false);
}

fn refresh_with_recovery(app: &PresentApp, state: &GuiState, recover: bool) {
    let session = state.session.borrow();
    let document = &session.document;
    app.set_deck_title(document.title.as_str().into());
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    app.set_slide_count_text(SharedString::from(format!("{} slides", document.len())));
    app.set_slide_titles(ModelRc::new(VecModel::from(
        document
            .slides
            .iter()
            .map(|slide| SharedString::from(slide.title.as_str()))
            .collect::<Vec<_>>(),
    )));
    app.set_active_slide_index(document.active_index as i32);
    if let Some(slide) = document.active_slide() {
        app.set_slide_title(slide.title.as_str().into());
        app.set_slide_body(active_body(&session).into());
        app.set_slide_notes(slide.speaker_notes.as_str().into());
        let labels = slide
            .elements
            .iter()
            .map(|element| {
                let selected = session.selected_elements.iter().any(|id| id == &element.id);
                SharedString::from(if selected {
                    format!("Selected {:?} · {}", element.element_type, element.content)
                } else {
                    format!("{:?} · {}", element.element_type, element.content)
                })
            })
            .collect::<Vec<_>>();
        app.set_element_labels(ModelRc::new(VecModel::from(labels)));
        app.set_element_contents(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| SharedString::from(element.content.as_str()))
                .collect::<Vec<_>>(),
        )));
        app.set_element_xs(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| element.x)
                .collect::<Vec<_>>(),
        )));
        app.set_element_ys(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| element.y)
                .collect::<Vec<_>>(),
        )));
        app.set_element_widths(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| element.width)
                .collect::<Vec<_>>(),
        )));
        app.set_element_heights(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| element.height)
                .collect::<Vec<_>>(),
        )));
        app.set_element_rotations(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| element.rotation_deg)
                .collect::<Vec<_>>(),
        )));
        app.set_element_types(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| element_type_index(&element.element_type))
                .collect::<Vec<_>>(),
        )));
        let selected_ids = &session.selected_elements;
        app.set_element_selected(ModelRc::new(VecModel::from(
            slide
                .elements
                .iter()
                .map(|element| selected_ids.contains(&element.id))
                .collect::<Vec<_>>(),
        )));
        let selected_indices = slide
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                session
                    .selected_elements
                    .iter()
                    .any(|id| id == &element.id)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let selected_count = selected_indices.len();
        let selected = selected_indices.first().copied().unwrap_or(0);
        state.selected_element.set(selected);
        app.set_selection_count(selected_count as i32);
        app.set_active_element_index(selected as i32);
        if selected_count > 0 {
            let element = slide
                .elements
                .get(selected)
                .expect("selected element index comes from active slide");
            app.set_active_element_label(format!("{:?}", element.element_type).into());
            app.set_active_element_content(element.content.as_str().into());
            app.set_element_x(element.x);
            app.set_element_y(element.y);
            app.set_element_width(element.width);
            app.set_element_height(element.height);
            app.set_element_x_text(format!("{:.0} pt", element.x).into());
            app.set_element_y_text(format!("{:.0} pt", element.y).into());
            app.set_element_width_text(format!("{:.0} pt", element.width).into());
            app.set_element_height_text(format!("{:.0} pt", element.height).into());
            app.set_element_rotation_text(format!("{:.0}°", element.rotation_deg).into());
        } else {
            app.set_active_element_label("No element selected".into());
            app.set_active_element_content("".into());
            app.set_element_x_text("—".into());
            app.set_element_y_text("—".into());
            app.set_element_width_text("—".into());
            app.set_element_height_text("—".into());
            app.set_element_rotation_text("—".into());
        }
        app.set_transition_index(match session.transition_for(&slide.id) {
            TransitionKind::None => 0,
            TransitionKind::Dissolve => 1,
            TransitionKind::Push => 2,
            TransitionKind::Morph => 3,
        });
    }
    let issue_count = session.validate().len();
    app.set_status_left(SharedString::from(format!(
        "{} slides · {} validation issues · undo {}",
        document.len(),
        issue_count,
        if session.can_undo() {
            "available"
        } else {
            "clean"
        }
    )));
    app.set_status_right("Local deck engine".into());
    let drag = state.drag_state.borrow();
    app.set_snap_guides_x(ModelRc::new(VecModel::from(
        drag.guides
            .iter()
            .filter(|guide| guide.is_vertical)
            .map(|guide| guide.position)
            .collect::<Vec<_>>(),
    )));
    app.set_snap_guides_y(ModelRc::new(VecModel::from(
        drag.guides
            .iter()
            .filter(|guide| !guide.is_vertical)
            .map(|guide| guide.position)
            .collect::<Vec<_>>(),
    )));
    app.set_marquee_x(drag.marquee_x);
    app.set_marquee_y(drag.marquee_y);
    app.set_marquee_width(drag.marquee_width);
    app.set_marquee_height(drag.marquee_height);
    app.set_marquee_visible(drag.mode == Some(HandleKind::Marquee));
    if recover {
        if let Ok(bytes) = save_presentation_session(&session) {
            let _ = record_snapshot_recovery("presentation state", bytes);
        }
    }
    if let Some(menu_service) = &state.menu_service {
        sync_menu_state(menu_service, app, state);
    }
}

fn apply_theme(app: &PresentApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn configure_direction(app: &PresentApp, rtl: bool) {
    app.set_rtl(rtl);
}

fn configure_responsive_layout(app: &PresentApp, size: (u32, u32)) -> bool {
    configure_responsive_width(app, size.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResponsiveToolbarState {
    icon_only: bool,
    overflow: bool,
    labeled: bool,
}

fn responsive_toolbar_state(app: &PresentApp, width: u32) -> ResponsiveToolbarState {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32;
    ResponsiveToolbarState {
        icon_only: width < policy.get_priority_1_icon_only_below(),
        overflow: width < policy.get_priority_2_overflow_below(),
        labeled: width >= policy.get_priority_2_overflow_below(),
    }
}

fn configure_responsive_width(app: &PresentApp, width: u32) -> bool {
    let state = responsive_toolbar_state(app, width);
    let inspector_available = !state.icon_only;
    app.set_icon_only_toolbar(state.icon_only);
    app.set_labeled_toolbar(state.labeled);
    app.set_show_inspector(inspector_available);
    app.set_labeled_export(state.labeled);
    if !state.overflow && app.get_toolbar_overflow_open() {
        app.invoke_close_toolbar_overflow();
    }
    app.set_overflow_toolbar(state.overflow);
    if !state.overflow {
        app.set_toolbar_overflow_open(false);
    }
    inspector_available
}

#[cfg(test)]
fn wire_responsive_layout(app: &PresentApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_width(&app, width.max(0.0) as u32);
        }
    });
}

fn wire_responsive_layout_with_state(app: &PresentApp, state: Rc<GuiState>) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            let inspector_available = configure_responsive_width(&app, width.max(0.0) as u32);
            state.inspector_available.set(inspector_available);
            if let Some(menu_service) = &state.menu_service {
                sync_menu_state(menu_service, &app, &state);
            }
        }
    });
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = PresentApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    let inspector_available = configure_responsive_layout(&app, args.size);
    let state = GuiState {
        session: RefCell::new(initial_session(args)?),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(inspector_available),
        save_path: RefCell::new(args.open.as_ref().map(PathBuf::from)),
        dialogs: Rc::new(NativeFileDialogs),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"])
            .map_err(|error| error.to_string())?,
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).map_err(|error| error.to_string())?,
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };
    refresh(&app, &state);
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    if args.theme_chooser {
        app.set_theme_chooser_open(true);
    }
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn set_status(app: &PresentApp, value: impl Into<SharedString>) {
    app.set_status_left(value.into());
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn open_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Loom Present Deck".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.deck_filter.clone()],
    }
}

fn save_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow().clone();
    let suggested_name = path
        .as_deref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| SAVE_FILENAME.to_string());
    SaveFileRequest {
        title: "Save Loom Present Deck".into(),
        initial_directory: initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: vec![state.deck_filter.clone()],
    }
}

fn export_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Loom Present PDF".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(EXPORT_FILENAME.to_string()),
        filters: vec![state.pdf_filter.clone()],
    }
}

fn replace_opened_deck(
    app: &PresentApp,
    state: &GuiState,
    path: PathBuf,
    session: PresentationSession,
) {
    *state.session.borrow_mut() = session;
    *state.save_path.borrow_mut() = Some(path);
    state.selected_element.set(0);
    refresh(app, state);
}

fn save_current_deck(
    app: &PresentApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
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
        set_status(app, "Save cancelled");
        return Ok(false);
    };

    let bytes = save_presentation_session(&state.session.borrow())?;
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

fn capture_present_journey_step(
    app: &PresentApp,
    args: &Args,
    out_dir: &Path,
    name: &str,
) -> Result<String, String> {
    let image = snapshot_component(app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let file_name = format!("present-manipulation-{name}.png");
    let path = out_dir.join(&file_name);
    loom_test_support::png::save_png(&path, &image)
        .map_err(|error| format!("save {file_name}: {error}"))?;
    let decoded = loom_test_support::png::load_png(&path)
        .map_err(|error| format!("validate {file_name}: {error}"))?;
    if decoded.dimensions() != (args.size.0, args.size.1) {
        return Err(format!(
            "invalid {file_name} dimensions: {:?}",
            decoded.dimensions()
        ));
    }
    Ok(file_name)
}

fn presentation_documents_match(left: &PresentationDocument, right: &PresentationDocument) -> bool {
    left.id == right.id
        && left.title == right.title
        && left.author == right.author
        && left.theme == right.theme
        && left.active_index == right.active_index
        && left.slides.len() == right.slides.len()
        && left.slides.iter().zip(&right.slides).all(|(left, right)| {
            left.id == right.id
                && left.title == right.title
                && left.layout == right.layout
                && left.elements == right.elements
                && left.speaker_notes == right.speaker_notes
                && left.bg_color == right.bg_color
        })
}

/// Record the controller-backed direct manipulation journey with per-step
/// screenshots and serialized/reopened/exported artifacts.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("create journey output '{}': {error}", out_dir.display()))?;
    let app = PresentApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    let inspector_available = configure_responsive_layout(&app, args.size);
    let save_path = out_dir.join("present-manipulation.loomdeck");
    let export_path = out_dir.join("present-manipulation.pdf");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(save_path.clone())],
        [Some(save_path.clone()), Some(export_path.clone())],
    ));
    let state = Rc::new(GuiState {
        session: RefCell::new(initial_session(args)?),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(inspector_available),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"])
            .map_err(|error| error.to_string())?,
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).map_err(|error| error.to_string())?,
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });

    refresh(&app, &state);
    wire_app_callbacks(&app, &state);
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let mut screenshots = Vec::new();
    screenshots.push(capture_present_journey_step(
        &app, args, out_dir, "initial",
    )?);

    // Add a real shape through the same callback used by the toolbar and
    // palette, then select it through the public selection callback.
    app.invoke_add_shape();
    let shape_index = {
        let session = state.session.borrow();
        let slide = session
            .document
            .active_slide()
            .ok_or("journey has no active slide after adding a shape")?;
        slide
            .elements
            .len()
            .checked_sub(1)
            .ok_or("shape was not added")?
    };
    app.invoke_select_element(shape_index as i32);
    let shape_id = {
        let session = state.session.borrow();
        let slide = session
            .document
            .active_slide()
            .ok_or("journey slide disappeared")?;
        let element = slide
            .elements
            .get(shape_index)
            .ok_or("journey shape index is invalid")?;
        if !session.selected_elements.contains(&element.id) {
            return Err("journey shape was not selected".into());
        }
        element.id.clone()
    };
    let baseline_document = state.session.borrow().document.clone();
    screenshots.push(capture_present_journey_step(
        &app,
        args,
        out_dir,
        "add-select",
    )?);

    // Move near the title/body edges. The controller applies the correction
    // and keeps the guides visible until pointer release.
    app.invoke_element_pressed(shape_index as i32, false);
    app.invoke_element_moved(shape_index as i32, -27.0, -29.0);
    if state.drag_state.borrow().guides.is_empty() {
        return Err("journey move did not produce snap guides".into());
    }
    screenshots.push(capture_present_journey_step(
        &app,
        args,
        out_dir,
        "move-snap-guides",
    )?);
    app.invoke_element_released(shape_index as i32);
    let moved = {
        let session = state.session.borrow();
        session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|element| element.id == shape_id))
            .cloned()
            .ok_or("journey moved shape disappeared")?
    };
    if (moved.x - 90.0).abs() > 0.001 || (moved.y - 230.0).abs() > 0.001 {
        return Err(format!(
            "journey snap landed at ({:.3}, {:.3}), expected (90, 230)",
            moved.x, moved.y
        ));
    }

    // Resize from the lower-right handle, retaining one undo transaction for
    // the entire gesture and capturing the visible guides before release.
    app.invoke_handle_pressed(shape_index as i32, "se".into(), false);
    app.invoke_handle_moved(shape_index as i32, "se".into(), 60.0, 40.0);
    screenshots.push(capture_present_journey_step(&app, args, out_dir, "resize")?);
    app.invoke_handle_released(shape_index as i32, "se".into());
    let resized = {
        let session = state.session.borrow();
        session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|element| element.id == shape_id))
            .cloned()
            .ok_or("journey resized shape disappeared")?
    };
    if (resized.width - 360.0).abs() > 0.001 || (resized.height - 180.0).abs() > 0.001 {
        return Err(format!(
            "journey resize landed at {:.3}x{:.3}, expected 360x180",
            resized.width, resized.height
        ));
    }

    app.invoke_handle_pressed(shape_index as i32, "rotate".into(), false);
    app.invoke_handle_moved(shape_index as i32, "rotate".into(), 60.0, 0.0);
    screenshots.push(capture_present_journey_step(&app, args, out_dir, "rotate")?);
    app.invoke_handle_released(shape_index as i32, "rotate".into());
    let rotated = {
        let session = state.session.borrow();
        session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|element| element.id == shape_id))
            .cloned()
            .ok_or("journey rotated shape disappeared")?
    };
    if (rotated.rotation_deg - 30.0).abs() > 0.001 {
        return Err(format!(
            "journey rotation landed at {:.3}, expected 30",
            rotated.rotation_deg
        ));
    }

    // Undo is routed through the same controller callback as the menu and
    // keyboard shortcut. It should revert only the latest rotation gesture.
    app.invoke_undo();
    let undone = {
        let session = state.session.borrow();
        session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|element| element.id == shape_id))
            .cloned()
            .ok_or("journey undo removed the shape")?
    };
    if (undone.x - resized.x).abs() > 0.001
        || (undone.y - resized.y).abs() > 0.001
        || (undone.width - resized.width).abs() > 0.001
        || (undone.height - resized.height).abs() > 0.001
        || undone.rotation_deg.abs() > 0.001
    {
        return Err(format!(
            "journey undo did not restore resized geometry: ({:.3}, {:.3}, {:.3}, {:.3}, {:.3})",
            undone.x, undone.y, undone.width, undone.height, undone.rotation_deg
        ));
    }
    screenshots.push(capture_present_journey_step(&app, args, out_dir, "undo")?);

    let saved_document = state.session.borrow().document.clone();
    app.invoke_save_deck();
    if !save_path.is_file() {
        return Err(format!(
            "journey save did not create {}",
            save_path.display()
        ));
    }
    let saved_bytes = std::fs::read(&save_path)
        .map_err(|error| format!("read journey deck '{}': {error}", save_path.display()))?;
    let reopened_from_bytes = load_presentation_session(&saved_bytes)
        .map_err(|error| format!("load journey deck '{}': {error}", save_path.display()))?;
    if !presentation_documents_match(&reopened_from_bytes.document, &saved_document) {
        return Err("journey package bytes did not preserve the saved document".into());
    }
    screenshots.push(capture_present_journey_step(&app, args, out_dir, "save")?);

    app.invoke_open_deck();
    if !presentation_documents_match(&state.session.borrow().document, &saved_document) {
        return Err("journey save/reopen did not preserve document geometry".into());
    }
    screenshots.push(capture_present_journey_step(&app, args, out_dir, "reopen")?);

    app.invoke_toggle_preview_mode();
    if !app.get_is_preview_mode() {
        return Err("journey Present mode toggle was not applied".into());
    }
    screenshots.push(capture_present_journey_step(
        &app,
        args,
        out_dir,
        "present-mode",
    )?);

    app.invoke_export_pdf();
    if !export_path.is_file() {
        return Err(format!(
            "journey export did not create {}",
            export_path.display()
        ));
    }
    let pdf_bytes = std::fs::read(&export_path)
        .map_err(|error| format!("read journey PDF '{}': {error}", export_path.display()))?;
    if !pdf_bytes.starts_with(b"%PDF") {
        return Err("journey export did not produce a PDF payload".into());
    }
    let baseline_pdf = export_pdf(&baseline_document);
    if pdf_bytes == baseline_pdf {
        return Err("journey PDF did not encode edited geometry or rotation".into());
    }
    screenshots.push(capture_present_journey_step(
        &app,
        args,
        out_dir,
        "export-pdf",
    )?);

    // Each capture validates its own dimensions; repeat the check over the
    // output directory so stale or extra PNGs cannot hide an invalid artifact.
    for entry in std::fs::read_dir(out_dir)
        .map_err(|error| format!("read journey output '{}': {error}", out_dir.display()))?
    {
        let path = entry
            .map_err(|error| format!("read journey output entry: {error}"))?
            .path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("png") {
            let image = loom_test_support::png::load_png(&path)
                .map_err(|error| format!("validate journey PNG '{}': {error}", path.display()))?;
            if image.dimensions() != args.size {
                return Err(format!(
                    "journey PNG '{}' has dimensions {:?}, expected {:?}",
                    path.display(),
                    image.dimensions(),
                    args.size
                ));
            }
        }
    }

    let step_json = screenshots
        .iter()
        .map(|screenshot| format!("{{\"screenshot\":\"{screenshot}\"}}"))
        .collect::<Vec<_>>()
        .join(",");
    let transcript = format!(
        "{{\n  \"app\": \"present\",\n  \"journey\": \"add-shape-select-move-snap-resize-rotate-undo-save-reopen-present-export\",\n  \"passed\": true,\n  \"size\": [ {}, {} ],\n  \"saved_package\": \"{}\",\n  \"exported_pdf\": \"{}\",\n  \"steps\": [ {} ]\n}}\n",
        args.size.0,
        args.size.1,
        save_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("present-manipulation.loomdeck"),
        export_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("present-manipulation.pdf"),
        step_json
    );
    std::fs::write(out_dir.join("present.json"), transcript)
        .map_err(|error| format!("write journey transcript: {error}"))?;
    println!("present journey: PASS ({})", out_dir.display());
    Ok(())
}

impl PaletteProbe for PresentApp {
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

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(output) = &args.screenshot {
        return render_headless(&args, output);
    }
    if args.smoke {
        let output =
            std::env::temp_dir().join(format!("loom-present-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }
    run_gui_with_dialogs(&args, Rc::new(NativeFileDialogs))
}

fn run_gui_with_dialogs(args: &Args, dialogs: Rc<dyn FileDialogService>) -> Result<(), String> {
    let app = PresentApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let inspector_available = configure_responsive_layout(&app, args.size);
    let recovered = initialize_snapshot_recovery()?;
    let initial_path = args.open.as_ref().map(PathBuf::from);
    let initial = if let Some(path) = initial_path.as_deref() {
        load_session(path)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_presentation_session(bytes).ok())
            .unwrap_or_else(sample_session)
    };
    let deck_filter =
        FileFilter::new("Loom Present deck", ["loomdeck"]).map_err(|error| error.to_string())?;
    let pdf_filter = FileFilter::new("PDF document", ["pdf"]).map_err(|error| error.to_string())?;
    let menu_service = Rc::new(NativeMenuBar::new());
    let state = Rc::new(GuiState {
        session: RefCell::new(initial),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(inspector_available),
        save_path: RefCell::new(initial_path),
        dialogs,
        deck_filter,
        pdf_filter,
        menu_service: Some(menu_service.clone()),
        drag_state: RefCell::new(DragState::default()),
    });

    wire_app_callbacks(&app, &state);
    wire_responsive_layout_with_state(&app, state.clone());

    let menu_bar = build_present_menu_bar();
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
    refresh(&app, &state);
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

fn build_present_menu_bar() -> MenuBar {
    let mut menu_bar = build_standard_menu_bar(
        "Loom Present",
        vec![MenuItem::action_with_shortcut(
            "file.export_pdf",
            "Export to PDF...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Format Inspector", false)],
        vec![Menu::new(
            "Slide",
            vec![
                MenuItem::action_with_shortcut(
                    "slide.new",
                    "New Slide",
                    MenuShortcut::primary_shift("N"),
                ),
                MenuItem::action("slide.duplicate", "Duplicate Slide"),
                MenuItem::action("slide.delete", "Delete Slide"),
                MenuItem::Separator,
                MenuItem::action("slide.prev", "Previous Slide"),
                MenuItem::action("slide.next", "Next Slide"),
            ],
        )],
    );
    menu_bar.disable_items_except([
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.export_pdf",
        "edit.undo",
        "edit.redo",
        "slide.new",
        "slide.duplicate",
        "slide.delete",
        "slide.prev",
        "slide.next",
        "view.inspector",
        "app.palette",
    ]);
    menu_bar
}

fn menu_projection(
    menu_service: &NativeMenuBar,
    app: &PresentApp,
    state: &GuiState,
) -> Result<CommandStateProjection, DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu bar is not installed".into()))?;
    let mut projection = menu_bar.command_state_projection();

    let session = state.session.borrow();
    let can_undo = session.can_undo();
    let can_redo = session.can_redo();
    let deck_len = session.document.len();
    let active_index = session.document.active_index;

    let mut undo = projection
        .get("edit.undo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing edit.undo".into()))?;
    undo.enabled = can_undo;
    projection.insert(undo);

    let mut redo = projection
        .get("edit.redo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing edit.redo".into()))?;
    redo.enabled = can_redo;
    projection.insert(redo);

    let mut inspector = projection.get("view.inspector").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Present menu is missing view.inspector".into())
    })?;
    inspector.enabled = state.inspector_available.get();
    inspector.checked = Some(app.get_show_inspector());
    projection.insert(inspector);

    let mut slide_delete = projection.get("slide.delete").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Present menu is missing slide.delete".into())
    })?;
    slide_delete.enabled = deck_len > 1;
    projection.insert(slide_delete);

    let mut slide_prev = projection
        .get("slide.prev")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing slide.prev".into()))?;
    slide_prev.enabled = active_index > 0;
    projection.insert(slide_prev);

    let mut slide_next = projection
        .get("slide.next")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing slide.next".into()))?;
    slide_next.enabled = active_index < deck_len.saturating_sub(1);
    projection.insert(slide_next);

    Ok(projection)
}

fn sync_menu_state_result(
    menu_service: &NativeMenuBar,
    app: &PresentApp,
    state: &GuiState,
) -> Result<(), DesktopError> {
    rebuild_palette(app, app.get_palette_query().as_str());
    let projection = menu_projection(menu_service, app, state)?;
    menu_service.sync_command_states(&projection)
}

fn sync_menu_state(menu_service: &NativeMenuBar, app: &PresentApp, state: &GuiState) {
    if let Err(error) = sync_menu_state_result(menu_service, app, state) {
        set_status(app, format!("Menu update failed: {error}"));
    }
}

fn dispatch_command(app: &PresentApp, id: &str) -> bool {
    match id {
        "file.new" => app.invoke_new_deck(),
        "file.open" => app.invoke_open_deck(),
        "file.save" => app.invoke_save_deck(),
        "file.save_as" => app.invoke_save_as_deck(),
        "file.export_pdf" => app.invoke_export_pdf(),
        "edit.undo" => app.invoke_undo(),
        "edit.redo" => app.invoke_redo(),
        "slide.new" => app.invoke_add_slide(),
        "slide.duplicate" => app.invoke_duplicate_slide(),
        "slide.delete" => app.invoke_delete_slide(),
        "slide.prev" => app.invoke_prev_slide(),
        "slide.next" => app.invoke_next_slide(),
        "view.inspector" => app.invoke_toggle_inspector(),
        "app.palette" => app.invoke_open_palette(),
        _ => return false,
    }
    true
}

fn is_present_menu_command(id: &str) -> bool {
    matches!(
        id,
        "file.new"
            | "file.open"
            | "file.save"
            | "file.save_as"
            | "file.export_pdf"
            | "edit.undo"
            | "edit.redo"
            | "slide.new"
            | "slide.duplicate"
            | "slide.delete"
            | "slide.prev"
            | "slide.next"
            | "view.inspector"
            | "app.palette"
    )
}

fn schedule_menu_action(
    app_ref: &slint::Weak<PresentApp>,
    action: CommandAction,
) -> Result<(), DesktopError> {
    if !is_present_menu_command(&action.id) {
        return Err(DesktopError::InvalidRequest(format!(
            "unsupported Present menu command {}",
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
                "failed to schedule Present menu command {error_id}: {error}"
            ))
        })
}

fn wire_app_callbacks(app: &PresentApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.session.borrow_mut() = empty_session();
                *state.save_path.borrow_mut() = None;
                state.selected_element.set(0);
                refresh(&app, &state);
                set_status(&app, "Created unsaved presentation");
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&open_request(&state)) {
                    Ok(Some(path)) => match load_session(&path) {
                        Ok(session) => {
                            replace_opened_deck(&app, &state, path.clone(), session);
                            set_status(&app, format!("Opened {}", path.display()));
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
        app.on_save_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_deck(&app, &state, false) {
                    set_status(&app, format!("Save failed: {error}"));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_deck(&app, &state, true) {
                    set_status(&app, format!("Save As failed: {error}"));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                let count = session.document.len() + 1;
                session
                    .document
                    .add_slide(format!("New Slide {count}"), "content");
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.add_element(text_element(
                        &format!("title-{count}"),
                        ElementType::Title,
                        &slide.title,
                        80.0,
                        70.0,
                        820.0,
                        90.0,
                    ));
                    slide.add_element(text_element(
                        &format!("body-{count}"),
                        ElementType::BodyText,
                        "Add your story here.",
                        82.0,
                        190.0,
                        760.0,
                        170.0,
                    ));
                }
                session.clear_selection();
                state.selected_element.set(0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_duplicate_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let index = state.session.borrow().document.active_index;
                let mut session = state.session.borrow_mut();
                if session.duplicate_slide(index) {
                    session.clear_selection();
                }
                state.selected_element.set(0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_delete_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let index = state.session.borrow().document.active_index;
                let mut session = state.session.borrow_mut();
                if session.remove_slide(index) {
                    session.clear_selection();
                }
                state.selected_element.set(0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.undo();
                session.prune_selection();
                state.selected_element.set(0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.redo();
                session.prune_selection();
                state.selected_element.set(0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_slide(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = state.session.borrow_mut();
                    if session.document.select_slide(index as usize) {
                        session.clear_selection();
                    }
                    state.selected_element.set(0);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_element(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut session = state.session.borrow_mut();
                let id = session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.get(index as usize))
                    .map(|element| element.id.clone());
                if let Some(id) = id {
                    session.select_element(&id, false);
                    state.selected_element.set(index as usize);
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    for shape in [false, true] {
        let state = state.clone();
        let app_ref = app.as_weak();
        if shape {
            app.on_add_shape(move || {
                if let Some(app) = app_ref.upgrade() {
                    let mut session = state.session.borrow_mut();
                    let count = session
                        .document
                        .active_slide()
                        .map(|slide| slide.elements.len() + 1)
                        .unwrap_or(1);
                    session.add_element(text_element(
                        &format!("shape-{count}"),
                        ElementType::ShapeRectangle,
                        "Shape",
                        120.0,
                        260.0,
                        300.0,
                        140.0,
                    ));
                    if let Some(id) = session
                        .document
                        .active_slide()
                        .and_then(|slide| slide.elements.last())
                        .map(|element| element.id.clone())
                    {
                        session.select_element(&id, false);
                    }
                    state.selected_element.set(count - 1);
                    drop(session);
                    refresh(&app, &state);
                }
            });
        } else {
            app.on_add_text(move || {
                if let Some(app) = app_ref.upgrade() {
                    let mut session = state.session.borrow_mut();
                    let count = session
                        .document
                        .active_slide()
                        .map(|slide| slide.elements.len() + 1)
                        .unwrap_or(1);
                    session.add_element(text_element(
                        &format!("text-{count}"),
                        ElementType::BodyText,
                        "New text",
                        120.0,
                        220.0,
                        520.0,
                        100.0,
                    ));
                    if let Some(id) = session
                        .document
                        .active_slide()
                        .and_then(|slide| slide.elements.last())
                        .map(|element| element.id.clone())
                    {
                        session.select_element(&id, false);
                    }
                    state.selected_element.set(count - 1);
                    drop(session);
                    refresh(&app, &state);
                }
            });
        }
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_update_element_content(move |content| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let selected = state.selected_element.get();
                if session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.get(selected))
                    .is_some()
                {
                    session.checkpoint();
                    let is_title = if let Some(element) = session
                        .document
                        .active_slide_mut()
                        .and_then(|slide| slide.elements.get_mut(selected))
                    {
                        element.content = content.as_str().to_string();
                        element.element_type == ElementType::Title
                    } else {
                        false
                    };
                    if is_title {
                        if let Some(slide) = session.document.active_slide_mut() {
                            slide.title = content.as_str().to_string();
                        }
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_transform_element(move |id, x, y, width, height| {
            if let Some(app) = app_ref.upgrade() {
                state
                    .session
                    .borrow_mut()
                    .transform_element(id.as_str(), x, y, width, height);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_set_element_rotation(move |id, rot| {
            if let Some(app) = app_ref.upgrade() {
                state
                    .session
                    .borrow_mut()
                    .set_element_rotation(id.as_str(), rot);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_element_pressed(move |index, shift| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() || index < 0 {
                    return;
                }
                let mut drag = state.drag_state.borrow_mut();
                let mut session = state.session.borrow_mut();
                let Some(id) = session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.get(index as usize))
                    .map(|element| element.id.clone())
                else {
                    return;
                };
                drag.begin(&session, state.selected_element.get());
                session.select_element(&id, shift);
                state.selected_element.set(index as usize);
                if session.selected_elements.is_empty() {
                    drag.reset();
                } else {
                    drag.mode = Some(HandleKind::Move);
                    drag.target_id = Some(id);
                    drag.elements = drag_snapshots(&session);
                }
                drop(session);
                drop(drag);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_element_moved(move |_, dx, dy| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() {
                    return;
                }
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode != Some(HandleKind::Move) {
                    return;
                }
                let (targets, guides) = {
                    let session = state.session.borrow();
                    move_targets(&session, &drag.elements, dx, dy)
                };
                let changed = {
                    let session = state.session.borrow();
                    targets.iter().any(|(id, x, y, width, height)| {
                        session
                            .document
                            .active_slide()
                            .and_then(|slide| {
                                slide.elements.iter().find(|element| element.id == *id)
                            })
                            .map(|element| {
                                (element.x - *x).abs() > f32::EPSILON
                                    || (element.y - *y).abs() > f32::EPSILON
                                    || (element.width - *width).abs() > f32::EPSILON
                                    || (element.height - *height).abs() > f32::EPSILON
                            })
                            .unwrap_or(false)
                    })
                };
                if changed {
                    let mut session = state.session.borrow_mut();
                    if !drag.checkpointed {
                        session.checkpoint();
                        drag.checkpointed = true;
                    }
                    for (id, x, y, width, height) in targets {
                        session.transform_element_no_checkpoint(&id, x, y, width, height);
                    }
                    drag.guides = guides;
                }
                drop(drag);
                refresh_without_recovery(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_element_released(move |_| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(HandleKind::Move) {
                    finish_drag(&mut session, &mut drag);
                } else {
                    drag.reset();
                }
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_element_cancelled(move |_| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(HandleKind::Move) {
                    cancel_drag(&mut session, &mut drag, &state.selected_element);
                } else {
                    drag.reset();
                }
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_handle_pressed(move |index, kind, _shift| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() || index < 0 {
                    return;
                }
                let mut drag = state.drag_state.borrow_mut();
                let session = state.session.borrow();
                let Some(id) = session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.get(index as usize))
                    .map(|element| element.id.clone())
                else {
                    return;
                };
                drag.begin(&session, state.selected_element.get());
                // Resize/rotate handles are only meaningful for one selected
                // element. Ignore stale or accessibility-triggered handle
                // events rather than applying them to an arbitrary first
                // element in a multi-selection.
                if session.selected_elements.len() != 1
                    || !session.selected_elements.iter().any(|item| item == &id)
                {
                    drag.reset();
                    drop(session);
                    drop(drag);
                    return;
                }
                state.selected_element.set(index as usize);
                drag.mode = Some(handle_kind(kind.as_str()));
                drag.target_id = Some(id);
                drag.elements = drag_snapshots(&session);
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_handle_moved(move |_, kind, dx, dy| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() {
                    return;
                }
                let mut drag = state.drag_state.borrow_mut();
                let mode = handle_kind(kind.as_str());
                if drag.mode != Some(mode) {
                    return;
                }
                let Some(target_id) = drag.target_id.as_deref() else {
                    return;
                };
                let Some(element) = drag
                    .elements
                    .iter()
                    .find(|element| element.id == target_id)
                    .cloned()
                else {
                    return;
                };
                let mut session = state.session.borrow_mut();
                let (x, y, width, height, changed) = if mode == HandleKind::Rotate {
                    let rotation = normalize_angle_degrees(element.rotation_deg + dx * 0.5);
                    let changed = session
                        .document
                        .active_slide()
                        .and_then(|slide| slide.elements.iter().find(|item| item.id == element.id))
                        .map(|item| (item.rotation_deg - rotation).abs() > f32::EPSILON)
                        .unwrap_or(false);
                    if changed && !drag.checkpointed {
                        session.checkpoint();
                        drag.checkpointed = true;
                    }
                    let changed = session.set_element_rotation_no_checkpoint(&element.id, rotation);
                    (element.x, element.y, element.width, element.height, changed)
                } else {
                    let ((x, y, width, height), guides) =
                        resize_targets(&session, &element, mode, dx, dy);
                    let changed = session
                        .document
                        .active_slide()
                        .and_then(|slide| slide.elements.iter().find(|item| item.id == element.id))
                        .map(|item| {
                            (item.x - x).abs() > f32::EPSILON
                                || (item.y - y).abs() > f32::EPSILON
                                || (item.width - width).abs() > f32::EPSILON
                                || (item.height - height).abs() > f32::EPSILON
                        })
                        .unwrap_or(false);
                    if changed && !drag.checkpointed {
                        session.checkpoint();
                        drag.checkpointed = true;
                    }
                    drag.guides = guides;
                    (x, y, width, height, changed)
                };
                if mode != HandleKind::Rotate && changed {
                    session.transform_element_no_checkpoint(&element.id, x, y, width, height);
                }
                drop(session);
                drop(drag);
                refresh_without_recovery(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_handle_released(move |_, kind| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(handle_kind(kind.as_str())) {
                    finish_drag(&mut session, &mut drag);
                } else {
                    drag.reset();
                }
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_handle_cancelled(move |_, kind| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(handle_kind(kind.as_str())) {
                    cancel_drag(&mut session, &mut drag, &state.selected_element);
                } else {
                    drag.reset();
                }
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_canvas_pressed(move |x, y, shift| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() {
                    return;
                }
                let mut drag = state.drag_state.borrow_mut();
                let mut session = state.session.borrow_mut();
                drag.begin(&session, state.selected_element.get());
                drag.mode = Some(HandleKind::Marquee);
                drag.start_mouse_x = x;
                drag.start_mouse_y = y;
                drag.marquee_x = x;
                drag.marquee_y = y;
                drag.marquee_width = 0.0;
                drag.marquee_height = 0.0;
                drag.marquee_additive = shift;
                if !shift {
                    session.clear_selection();
                }
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_canvas_moved(move |x, y| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() {
                    return;
                }
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(HandleKind::Marquee) {
                    drag.marquee_x = drag.start_mouse_x.min(x);
                    drag.marquee_y = drag.start_mouse_y.min(y);
                    drag.marquee_width = (x - drag.start_mouse_x).abs();
                    drag.marquee_height = (y - drag.start_mouse_y).abs();
                    drop(drag);
                    refresh_without_recovery(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_canvas_released(move |x, y| {
            if let Some(app) = app_ref.upgrade() {
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(HandleKind::Marquee) {
                    let x0 = drag.start_mouse_x;
                    let y0 = drag.start_mouse_y;
                    let width = x - x0;
                    let height = y - y0;
                    let additive = drag.marquee_additive;
                    let mut session = state.session.borrow_mut();
                    session.marquee_select(x0, y0, width, height, additive);
                    if let Some(id) = session.selected_elements.first().cloned() {
                        if let Some(index) = session.document.active_slide().and_then(|slide| {
                            slide.elements.iter().position(|element| element.id == id)
                        }) {
                            state.selected_element.set(index);
                        }
                    }
                    finish_drag(&mut session, &mut drag);
                    drop(drag);
                    drop(session);
                    refresh(&app, &state);
                    return;
                }
                drop(drag);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_canvas_cancelled(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let mut drag = state.drag_state.borrow_mut();
                if drag.mode == Some(HandleKind::Marquee) {
                    cancel_drag(&mut session, &mut drag, &state.selected_element);
                } else {
                    drag.reset();
                }
                drop(drag);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let left_arrow: SharedString = slint::platform::Key::LeftArrow.into();
        let right_arrow: SharedString = slint::platform::Key::RightArrow.into();
        let up_arrow: SharedString = slint::platform::Key::UpArrow.into();
        let down_arrow: SharedString = slint::platform::Key::DownArrow.into();
        app.on_canvas_key_pressed(move |key, _shift, modified| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() || modified {
                    return EventResult::Reject;
                }
                let (dx, dy) = if key == left_arrow {
                    (-10.0, 0.0)
                } else if key == right_arrow {
                    (10.0, 0.0)
                } else if key == up_arrow {
                    (0.0, -10.0)
                } else if key == down_arrow {
                    (0.0, 10.0)
                } else {
                    (0.0, 0.0)
                };
                if (dx, dy) != (0.0, 0.0) {
                    let mut session = state.session.borrow_mut();
                    if nudge_selected(&mut session, dx, dy) {
                        drop(session);
                        refresh(&app, &state);
                        return EventResult::Accept;
                    }
                }
            }
            EventResult::Reject
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_notes_edited(move |notes| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.speaker_notes = notes.as_str().to_string();
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_preview_mode(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_is_preview_mode(!app.get_is_preview_mode());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_apply_template(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let layout = match index {
                    0 => "cover",
                    2 => "two-column",
                    3 => "image-text",
                    _ => "content",
                };
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.layout = layout.into();
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_set_transition(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                if let Some(slide) = session.document.active_slide() {
                    let id = slide.id.clone();
                    session.set_transition(
                        &id,
                        match index {
                            1 => TransitionKind::Dissolve,
                            2 => TransitionKind::Push,
                            3 => TransitionKind::Morph,
                            _ => TransitionKind::None,
                        },
                    );
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_prev_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let index = state.session.borrow().document.active_index;
                if index > 0 {
                    state.session.borrow_mut().document.select_slide(index - 1);
                    state.selected_element.set(0);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_next_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let (index, len) = {
                    let session = state.session.borrow();
                    (session.document.active_index, session.document.len())
                };
                if index + 1 < len {
                    state.session.borrow_mut().document.select_slide(index + 1);
                    state.selected_element.set(0);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&export_request(&state)) {
                    Ok(Some(path)) => {
                        let pdf_bytes = export_pdf(&state.session.borrow().document);
                        match loom_storage::atomic_write(&path, &pdf_bytes) {
                            Ok(()) => set_status(&app, format!("Exported {}", path.display())),
                            Err(error) => set_status(&app, format!("Export failed: {error}")),
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
        app.on_toggle_inspector(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.inspector_available.get() {
                    app.set_show_inspector(!app.get_show_inspector());
                    if let Some(menu_service) = &state.menu_service {
                        sync_menu_state(menu_service, &app, &state);
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_create_theme(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = empty_session();
                session.document.title = match idx {
                    1 => "Black Minimal".into(),
                    2 => "Editorial Presentation".into(),
                    3 => "Dynamic Accent Deck".into(),
                    _ => "Untitled Deck".into(),
                };
                *state.session.borrow_mut() = session;
                *state.save_path.borrow_mut() = None;
                state.selected_element.set(0);
                refresh(&app, &state);
                app.set_theme_chooser_open(false);
                set_status(&app, "Created presentation from theme");
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_cancel_theme(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_theme_chooser_open(false);
            }
        });
    }
}

/// Commands exposed through the command palette. Dispatch reuses the same
/// application callbacks as the toolbar and menus.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewDeck,
    OpenDeck,
    SaveDeck,
    SaveAsDeck,
    AddSlide,
    DuplicateSlide,
    DeleteSlide,
    Undo,
    Redo,
    AddText,
    ExportPdf,
    TogglePreview,
    PrevSlide,
    NextSlide,
    ApplyTemplate(i32),
    SetTransition(i32),
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &PresentApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewDeck,
            id: "present.new",
            label: "New Deck",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenDeck,
            id: "present.open",
            label: "Open Deck",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveDeck,
            id: "present.save",
            label: "Save Deck",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDeck,
            id: "present.save-as",
            label: "Save Deck As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::AddSlide,
            id: "present.add-slide",
            label: "Add Slide",
            shortcut: "Ctrl+Shift+A",
        },
        PaletteCommand {
            action: PaletteAction::DuplicateSlide,
            id: "present.duplicate-slide",
            label: "Duplicate Slide",
            shortcut: "Ctrl+D",
        },
        PaletteCommand {
            action: PaletteAction::DeleteSlide,
            id: "present.delete-slide",
            label: "Delete Slide",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "present.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "present.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::AddText,
            id: "present.add-text",
            label: "Add Text",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::TogglePreview,
            id: "present.preview",
            label: "Toggle Preview Mode",
            shortcut: "F5",
        },
        PaletteCommand {
            action: PaletteAction::PrevSlide,
            id: "present.prev",
            label: "Previous Slide",
            shortcut: "PageUp",
        },
        PaletteCommand {
            action: PaletteAction::NextSlide,
            id: "present.next",
            label: "Next Slide",
            shortcut: "PageDown",
        },
        PaletteCommand {
            action: PaletteAction::ExportPdf,
            id: "present.export-pdf",
            label: "Export PDF",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(0),
            id: "present.template.title",
            label: "Template: Title",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(1),
            id: "present.template.content",
            label: "Template: Content",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(2),
            id: "present.template.2col",
            label: "Template: 2 Column",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(3),
            id: "present.template.image",
            label: "Template: Image",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(1),
            id: "present.transition.dissolve",
            label: "Transition: Dissolve",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(2),
            id: "present.transition.push",
            label: "Transition: Push",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(3),
            id: "present.transition.morph",
            label: "Transition: Morph",
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

fn rebuild_palette(app: &PresentApp, query: &str) {
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

fn wire_palette(app: &PresentApp) {
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
                let query = app.get_palette_query().trim().to_lowercase();
                let command = master_palette(&app)
                    .into_iter()
                    .filter(|c| {
                        query.is_empty()
                            || c.label.to_lowercase().contains(&query)
                            || c.id.to_lowercase().contains(&query)
                    })
                    .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewDeck => app.invoke_new_deck(),
                        PaletteAction::OpenDeck => app.invoke_open_deck(),
                        PaletteAction::SaveDeck => app.invoke_save_deck(),
                        PaletteAction::SaveAsDeck => app.invoke_save_as_deck(),
                        PaletteAction::AddSlide => app.invoke_add_slide(),
                        PaletteAction::DuplicateSlide => app.invoke_duplicate_slide(),
                        PaletteAction::DeleteSlide => app.invoke_delete_slide(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddText => app.invoke_add_text(),
                        PaletteAction::TogglePreview => app.invoke_toggle_preview_mode(),
                        PaletteAction::PrevSlide => app.invoke_prev_slide(),
                        PaletteAction::NextSlide => app.invoke_next_slide(),
                        PaletteAction::ExportPdf => app.invoke_export_pdf(),
                        PaletteAction::ApplyTemplate(index) => app.invoke_apply_template(index),
                        PaletteAction::SetTransition(index) => app.invoke_set_transition(index),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod desktop_tests {
    use super::*;
    use loom_desktop::{CommandSource, ScriptedFileDialogs};

    fn test_state() -> GuiState {
        GuiState {
            session: RefCell::new(empty_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(Some(PathBuf::from("projects/demo.loomdeck"))),
            dialogs: Rc::new(ScriptedFileDialogs::default()),
            deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        }
    }

    #[test]
    fn new_presentation_is_blank_and_single_slide() {
        let session = empty_session();
        assert_eq!(session.document.len(), 1);
        assert_eq!(session.document.title, "Untitled Presentation");
        assert!(session.document.slides[0]
            .elements
            .iter()
            .all(|element| element.content.is_empty()));
    }

    #[test]
    fn optional_inspector_and_notes_drawer_are_closed_by_default() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        assert!(!app.get_show_inspector());
        assert!(!app.get_show_notes_drawer());
    }

    #[test]
    fn refresh_projects_selected_element_into_canvas_and_inspector() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = GuiState {
            session: RefCell::new(sample_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs: Rc::new(ScriptedFileDialogs::default()),
            deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        };

        state
            .session
            .borrow_mut()
            .select_element("cover-title", false);
        refresh(&app, &state);
        assert_eq!(app.get_active_element_label().as_str(), "Title");
        assert_eq!(
            app.get_active_element_content().as_str(),
            "Create without compromise"
        );
        assert_eq!(app.get_element_x_text().as_str(), "90 pt");
        assert_eq!(app.get_element_width_text().as_str(), "820 pt");
        assert_eq!(app.get_element_contents().row_count(), 2);
        assert_eq!(app.get_element_types().row_count(), 2);

        state
            .session
            .borrow_mut()
            .select_element("cover-body", false);
        refresh(&app, &state);
        assert_eq!(app.get_active_element_label().as_str(), "BodyText");
        assert_eq!(app.get_selection_count(), 1);
        assert_eq!(
            app.get_active_element_content().as_str(),
            "A private, native creative studio designed for Linux."
        );
        assert_eq!(app.get_element_y_text().as_str(), "230 pt");
        assert_eq!(app.get_element_height_text().as_str(), "120 pt");
    }

    #[test]
    fn refresh_clears_inspector_when_domain_selection_is_empty() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = GuiState {
            session: RefCell::new(sample_session()),
            selected_element: Cell::new(1),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs: Rc::new(ScriptedFileDialogs::default()),
            deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        };

        state
            .session
            .borrow_mut()
            .select_element("cover-body", false);
        refresh(&app, &state);
        assert_eq!(app.get_active_element_label().as_str(), "BodyText");

        state.session.borrow_mut().clear_selection();
        refresh(&app, &state);
        assert_eq!(
            app.get_active_element_label().as_str(),
            "No element selected"
        );
        assert_eq!(app.get_selection_count(), 0);
        assert_eq!(app.get_active_element_content().as_str(), "");
        assert_eq!(app.get_element_x_text().as_str(), "—");
    }

    #[test]
    fn focused_canvas_arrow_key_nudges_selected_element() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(test_state());
        state.session.borrow_mut().select_element("elem-1", false);
        wire_app_callbacks(&app, &state);

        let before = state
            .session
            .borrow()
            .document
            .active_slide()
            .expect("slide")
            .elements[0]
            .x;
        app.invoke_canvas_key_pressed(slint::platform::Key::RightArrow.into(), false, false);
        let after = state
            .session
            .borrow()
            .document
            .active_slide()
            .expect("slide")
            .elements[0]
            .x;
        assert!((after - before - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn shift_marquee_adds_to_existing_domain_selection() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(test_state());
        {
            let mut session = state.session.borrow_mut();
            session
                .document
                .active_slide_mut()
                .expect("slide")
                .elements
                .push(SlideElement {
                    id: "elem-2".into(),
                    element_type: ElementType::ShapeRectangle,
                    content: "Second".into(),
                    x: 300.0,
                    y: 300.0,
                    width: 50.0,
                    height: 50.0,
                    rotation_deg: 0.0,
                    action: None,
                });
            session.select_element("elem-1", false);
        }
        wire_app_callbacks(&app, &state);

        app.invoke_canvas_pressed(280.0, 280.0, true);
        app.invoke_canvas_moved(380.0, 380.0);
        app.invoke_canvas_released(380.0, 380.0);

        assert_eq!(
            state.session.borrow().selected_elements,
            vec!["elem-1".to_string(), "elem-2".to_string()]
        );
        assert_eq!(app.get_selection_count(), 2);
    }

    #[test]
    fn cancelled_pointer_gestures_restore_geometry_selection_and_history() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(test_state());
        state.session.borrow_mut().select_element("elem-1", false);
        state.selected_element.set(0);
        wire_app_callbacks(&app, &state);

        let before = state.session.borrow().document.clone();
        app.invoke_element_pressed(0, false);
        app.invoke_element_moved(0, 20.0, 30.0);
        assert_ne!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        assert!(state.session.borrow().can_undo());
        app.invoke_element_cancelled(0);
        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        assert_eq!(state.session.borrow().selected_elements, ["elem-1"]);
        assert!(!state.session.borrow().can_undo());

        app.invoke_canvas_pressed(0.0, 0.0, false);
        app.invoke_canvas_moved(80.0, 80.0);
        assert!(state.session.borrow().selected_elements.is_empty());
        app.invoke_canvas_cancelled();
        assert_eq!(state.session.borrow().selected_elements, ["elem-1"]);
        assert!(!state.session.borrow().can_undo());

        let before = state.session.borrow().document.clone();
        app.invoke_handle_pressed(0, "se".into(), false);
        app.invoke_handle_moved(0, "se".into(), 20.0, 15.0);
        assert_ne!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        app.invoke_handle_cancelled(0, "se".into());
        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        assert!(!state.session.borrow().can_undo());

        let before = state.session.borrow().document.clone();
        app.invoke_handle_pressed(0, "rotate".into(), false);
        app.invoke_handle_moved(0, "rotate".into(), 30.0, 0.0);
        assert_ne!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        app.invoke_handle_cancelled(0, "rotate".into());
        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        assert!(!state.session.borrow().can_undo());
    }

    #[test]
    fn no_op_rotation_gesture_does_not_leave_history() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(test_state());
        state.session.borrow_mut().select_element("elem-1", false);
        wire_app_callbacks(&app, &state);

        let before = state.session.borrow().document.clone();
        app.invoke_handle_pressed(0, "rotate".into(), false);
        app.invoke_handle_moved(0, "rotate".into(), 0.0, 0.0);
        app.invoke_handle_released(0, "rotate".into());

        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        assert!(!state.session.borrow().can_undo());
    }

    #[test]
    fn preview_mode_rejects_pointer_edits_and_modified_canvas_arrows() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(test_state());
        state.session.borrow_mut().select_element("elem-1", false);
        wire_app_callbacks(&app, &state);

        let before = state.session.borrow().document.clone();
        let result =
            app.invoke_canvas_key_pressed(slint::platform::Key::RightArrow.into(), false, true);
        assert!(matches!(result, EventResult::Reject));
        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );

        app.set_is_preview_mode(true);
        app.invoke_element_pressed(0, false);
        app.invoke_element_moved(0, 20.0, 30.0);
        app.invoke_handle_pressed(0, "se".into(), false);
        app.invoke_handle_moved(0, "se".into(), 20.0, 15.0);
        app.invoke_canvas_pressed(0.0, 0.0, false);
        app.invoke_canvas_moved(100.0, 100.0);
        app.invoke_canvas_released(100.0, 100.0);
        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
        assert!(!state.session.borrow().can_undo());

        let result =
            app.invoke_canvas_key_pressed(slint::platform::Key::RightArrow.into(), false, false);
        assert!(matches!(result, EventResult::Reject));
        assert_eq!(
            state.session.borrow().document.integrity_digest(),
            before.integrity_digest()
        );
    }

    #[test]
    fn empty_slide_keeps_inspector_truthful() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = test_state();
        state
            .session
            .borrow_mut()
            .document
            .active_slide_mut()
            .expect("empty session slide")
            .elements
            .clear();

        configure_responsive_width(&app, 1280);
        refresh(&app, &state);

        assert_eq!(
            app.get_active_element_label().as_str(),
            "No element selected"
        );
        assert_eq!(app.get_element_contents().row_count(), 0);
        assert_eq!(app.get_element_x_text().as_str(), "—");
        let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render empty slide");
        assert_eq!((image.width(), image.height()), (1280, 800));
    }

    #[test]
    fn compact_stage_render_is_safe_for_short_windows() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = GuiState {
            session: RefCell::new(sample_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs: Rc::new(ScriptedFileDialogs::default()),
            deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        };
        configure_responsive_width(&app, 900);
        refresh(&app, &state);
        let image = snapshot_component(&app, 900.0, 480.0, 1.0).expect("render short window");
        assert_eq!(image.width(), 900);
        assert_eq!(image.height(), 480);
    }

    #[test]
    fn expanding_past_overflow_breakpoint_closes_menu() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        configure_responsive_width(&app, 1024);
        assert!(app.get_overflow_toolbar());
        app.set_toolbar_overflow_open(true);

        configure_responsive_width(&app, 1320);

        assert!(!app.get_overflow_toolbar());
        assert!(!app.get_toolbar_overflow_open());
    }

    #[test]
    fn responsive_policy_transition_probes_are_exact() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
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
            configure_responsive_width(&app, width);
            assert_eq!(app.get_icon_only_toolbar(), icon_only);
            assert_eq!(app.get_overflow_toolbar(), overflow);
            assert_eq!(app.get_labeled_toolbar(), labeled);
        }
    }

    #[test]
    fn widening_window_preserves_palette_focus() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        configure_responsive_width(&app, 1024);
        wire_responsive_layout(&app);
        let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render compact window");

        app.invoke_open_palette();
        let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render open palette");
        let focused_before =
            slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("palette should own focus");

        app.window().set_size(PhysicalSize::new(1280, 800));
        let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render widened window");
        let focused_after =
            slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("palette focus should remain present");

        assert_eq!(focused_after, focused_before);
        assert!(app.get_palette_open());
    }

    #[test]
    fn dialog_requests_use_current_directory_and_expected_extensions() {
        let state = test_state();
        let open = open_request(&state);
        let save = save_request(&state);
        let export = export_request(&state);

        assert_eq!(open.initial_directory, Some(PathBuf::from("projects")));
        assert_eq!(open.filters[0].extensions, vec!["loomdeck".to_string()]);
        assert_eq!(save.suggested_name.as_deref(), Some("demo.loomdeck"));
        assert_eq!(export.suggested_name.as_deref(), Some(EXPORT_FILENAME));
        assert_eq!(export.filters[0].extensions, vec!["pdf".to_string()]);
    }

    #[test]
    fn presentation_path_round_trip_preserves_document() {
        let path = std::env::temp_dir().join(format!(
            "loom-present-roundtrip-{}.loomdeck",
            std::process::id()
        ));
        let session = empty_session();
        let bytes = save_presentation_session(&session).expect("serialize");
        std::fs::write(&path, bytes).expect("write");
        let loaded = load_session(&path).expect("load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.document.title, session.document.title);
        assert_eq!(loaded.document.len(), session.document.len());
    }

    #[test]
    fn present_menu_disables_unhandled_controller_commands() {
        set_platform();
        let menu = build_present_menu_bar();
        for id in [
            "edit.cut",
            "edit.copy",
            "edit.paste",
            "edit.select_all",
            "view.zoom_in",
            "view.zoom_out",
            "view.zoom_actual",
        ] {
            assert!(
                !menu.find_item(id).expect("menu command").is_enabled(),
                "unhandled Present command {id} must be disabled"
            );
        }
    }

    #[test]
    fn present_menu_projection_derives_live_session_and_window_state() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
        let state = GuiState {
            session: RefCell::new(empty_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs,
            deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        };
        let menu = NativeMenuBar::new();
        let bar = build_present_menu_bar();
        menu.install_menu_bar(&bar).expect("install menu");

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
            installed.find_item("slide.delete"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("slide.prev"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("slide.next"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("view.inspector"),
            Some(MenuItem::Check {
                checked: false,
                enabled: true,
                ..
            })
        ));

        state
            .session
            .borrow_mut()
            .document
            .add_slide("Slide 2", "content");
        sync_menu_state(&menu, &app, &state);
        let installed = menu.installed_menu_bar().expect("installed menu");

        assert!(matches!(
            installed.find_item("slide.delete"),
            Some(MenuItem::Action { enabled: true, .. })
        ));
        assert!(matches!(
            installed.find_item("slide.prev"),
            Some(MenuItem::Action { enabled: true, .. })
        ));
        assert!(matches!(
            installed.find_item("slide.next"),
            Some(MenuItem::Action { enabled: false, .. })
        ));

        state.session.borrow_mut().document.select_slide(0);
        sync_menu_state(&menu, &app, &state);
        let installed = menu.installed_menu_bar().expect("installed menu");

        assert!(matches!(
            installed.find_item("slide.prev"),
            Some(MenuItem::Action { enabled: false, .. })
        ));
        assert!(matches!(
            installed.find_item("slide.next"),
            Some(MenuItem::Action { enabled: true, .. })
        ));

        app.set_show_inspector(true);
        sync_menu_state(&menu, &app, &state);
        let installed = menu.installed_menu_bar().expect("installed menu");

        assert!(matches!(
            installed.find_item("view.inspector"),
            Some(MenuItem::Check {
                checked: true,
                enabled: true,
                ..
            })
        ));
    }

    #[test]
    fn present_menu_disables_inspector_when_window_cannot_show_it() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let inspector_available = configure_responsive_width(&app, 900);
        let state = Rc::new(GuiState {
            session: RefCell::new(empty_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(inspector_available),
            save_path: RefCell::new(None),
            dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
            deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        });
        wire_app_callbacks(&app, &state);
        let menu = NativeMenuBar::new();
        menu.install_menu_bar(&build_present_menu_bar())
            .expect("install menu");

        sync_menu_state(&menu, &app, &state);

        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                checked: false,
                enabled: false,
                ..
            })
        ));

        let before = app.get_show_inspector();
        let error = menu
            .dispatch_action_from("view.inspector", CommandSource::Menu)
            .expect_err("compact inspector action must be disabled");
        assert!(error
            .to_string()
            .contains("menu item view.inspector is disabled"));
        assert_eq!(app.get_show_inspector(), before);
    }

    #[test]
    fn present_menu_action_sink_dispatches_to_controller_and_guards_disabled_boundary() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
        let menu_service = Rc::new(NativeMenuBar::new());
        let state = Rc::new(GuiState {
            session: RefCell::new(empty_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs,
            deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
            menu_service: Some(menu_service.clone()),
            drag_state: RefCell::new(DragState::default()),
        });
        let bar = build_present_menu_bar();
        menu_service.install_menu_bar(&bar).expect("install menu");

        wire_app_callbacks(&app, &state);
        let app_ref = app.as_weak();
        menu_service
            .register_action_sink(std::sync::Arc::new(move |action: CommandAction| {
                assert_eq!(action.source, CommandSource::Menu);
                let app = app_ref.upgrade().ok_or_else(|| {
                    DesktopError::InvalidRequest("Present app was dropped".into())
                })?;
                if dispatch_command(&app, &action.id) {
                    Ok(())
                } else {
                    Err(DesktopError::InvalidRequest(format!(
                        "unsupported Present menu command {}",
                        action.id
                    )))
                }
            }))
            .expect("register sink");

        sync_menu_state(&menu_service, &app, &state);
        assert_eq!(state.session.borrow().document.len(), 1);

        let before_unsupported = state.session.borrow().document.len();
        let error = menu_service
            .dispatch_action_from("edit.cut", CommandSource::Menu)
            .expect_err("unsupported menu action must be disabled");
        assert!(error.to_string().contains("menu item edit.cut is disabled"));
        assert_eq!(state.session.borrow().document.len(), before_unsupported);

        menu_service
            .dispatch_action_from("slide.new", CommandSource::Menu)
            .expect("enabled menu action");
        assert_eq!(state.session.borrow().document.len(), 2);

        state.session.borrow_mut().document.select_slide(0);
        sync_menu_state(&menu_service, &app, &state);
        let err = menu_service
            .dispatch_action_from("slide.prev", CommandSource::Menu)
            .expect_err("disabled action");
        assert!(err.to_string().contains("menu item slide.prev is disabled"));
        assert_eq!(state.session.borrow().document.active_index, 0);

        state.session.borrow_mut().remove_slide(1);
        sync_menu_state(&menu_service, &app, &state);
        assert_eq!(state.session.borrow().document.len(), 1);
        let err = menu_service
            .dispatch_action_from("slide.delete", CommandSource::Menu)
            .expect_err("disabled slide.delete");
        assert!(err
            .to_string()
            .contains("menu item slide.delete is disabled"));
        assert_eq!(state.session.borrow().document.len(), 1);
    }

    #[test]
    fn notes_edit_refreshes_undo_menu_state() {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let menu_service = Rc::new(NativeMenuBar::new());
        let state = Rc::new(GuiState {
            session: RefCell::new(empty_session()),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
            deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
            menu_service: Some(menu_service.clone()),
            drag_state: RefCell::new(DragState::default()),
        });
        menu_service
            .install_menu_bar(&build_present_menu_bar())
            .expect("install menu");
        wire_app_callbacks(&app, &state);
        sync_menu_state(&menu_service, &app, &state);

        assert!(matches!(
            menu_service
                .installed_menu_bar()
                .and_then(|bar| bar.find_item("edit.undo").cloned()),
            Some(MenuItem::Action { enabled: false, .. })
        ));

        app.invoke_notes_edited(SharedString::from("Speaker notes"));

        assert_eq!(
            state
                .session
                .borrow()
                .document
                .active_slide()
                .expect("active slide")
                .speaker_notes,
            "Speaker notes"
        );
        assert!(matches!(
            menu_service
                .installed_menu_bar()
                .and_then(|bar| bar.find_item("edit.undo").cloned()),
            Some(MenuItem::Action { enabled: true, .. })
        ));
    }
}
