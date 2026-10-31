//! Loom Present desktop presentation application.
#![cfg_attr(
    all(windows, not(test), not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(test)]
use menu_models::menu_projection;
use menu_models::sync_menu_state;
use model_sync::synced;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{
    build_standard_menu_bar, CommandAction, DesktopError, FileDialogService, FileFilter, Menu,
    MenuBar, MenuBarService, MenuItem, MenuShortcut, NativeFileDialogs, NativeMenuBar,
    OpenFileRequest, SaveFileRequest,
};
use loom_present_core::{
    calculate_smart_snapping, export_pdf, export_pptx, load_presentation_session, lock_aspect,
    normalize_angle_degrees, save_presentation_session, ElementType, PresentationDocument,
    PresentationSession, SlideElement, SnapGuide, TransitionKind,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::PaletteProbe;
pub(crate) use pptx_import::load_session;
use slint::{
    private_unstable_api::re_exports::EventResult, ComponentHandle, Model, PhysicalSize,
    SharedString,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "presentation.loomdeck";
const EXPORT_FILENAME: &str = "presentation.pdf";
const PPTX_EXPORT_FILENAME: &str = "presentation.pptx";

loom_production::define_snapshot_recovery!(PRESENT_RECOVERY, "org.loom.present", "loom.present/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    /// True when `--theme` was given: it overrides the saved appearance.
    theme_explicit: bool,
    rtl: bool,
    text_scale: f32,
    scale_factor: f32,
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
        theme: "light".into(),
        theme_explicit: false,
        rtl: false,
        text_scale: 1.0,
        scale_factor: 1.0,
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
            "--theme" => {
                args.theme = iterator.next().ok_or("--theme needs a name")?;
                args.theme_explicit = true;
            }
            "--rtl" => args.rtl = true,
            "--text-scale" => {
                let scale: f32 = iterator
                    .next()
                    .ok_or("--text-scale needs a factor")?
                    .parse()
                    .map_err(|_| "bad --text-scale factor")?;
                if !(1.0..=2.0).contains(&scale) {
                    return Err("--text-scale must be between 1.0 and 2.0".to_string());
                }
                args.text_scale = scale;
            }
            "--scale-factor" => {
                let factor: f32 = iterator
                    .next()
                    .ok_or("--scale-factor needs a factor")?
                    .parse()
                    .map_err(|_| "bad --scale-factor factor")?;
                if !(1.0..=4.0).contains(&factor) {
                    return Err("--scale-factor must be between 1.0 and 4.0".to_string());
                }
                args.scale_factor = factor;
            }
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

/// Where a newly added object goes: a short cascade so repeated adds never land
/// exactly on top of each other, kept inside the 1000 x 562.5 slide.
fn added_object_origin(
    session: &PresentationSession,
    base: (f32, f32),
    size: (f32, f32),
) -> (f32, f32) {
    let count = session
        .document
        .active_slide()
        .map_or(0, |slide| slide.elements.len());
    let step = (count % 6) as f32 * 24.0;
    (
        (base.0 + step).min(1000.0 - size.0 - 40.0),
        (base.1 + step).min(562.5 - size.1 - 40.0),
    )
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
        style: Default::default(),
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
            "A private, native creative studio on your desktop.",
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
        slide.title = slide_layouts::UNTITLED_SLIDE.into();
        for element in &mut slide.elements {
            element.content.clear();
        }
    }
    PresentationSession::new(document)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingReplacement {
    NewDeck,
    NewSampleDeck,
    /// A new deck from the template chooser; the number is the card picked.
    NewFromTemplate(i32),
    OpenDeck,
    /// Closing the window with unsaved changes.
    CloseWindow,
}

struct GuiState {
    session: RefCell<PresentationSession>,
    selected_element: Cell<usize>,
    inspector_available: Cell<bool>,
    save_path: RefCell<Option<PathBuf>>,
    last_saved: RefCell<PresentationDocument>,
    last_saved_transitions: RefCell<std::collections::BTreeMap<String, TransitionKind>>,
    pending_replacement: Cell<Option<PendingReplacement>>,
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
pub(crate) fn element_type_index(element_type: &ElementType) -> i32 {
    match element_type {
        ElementType::Title => 0,
        ElementType::Subtitle => 1,
        ElementType::BodyText => 2,
        ElementType::ShapeRectangle => 3,
        ElementType::ShapeCircle => 4,
        ElementType::StatCard => 5,
        ElementType::Picture => 6,
        ElementType::Table => 7,
        ElementType::Chart => 8,
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

/// Plain-language name for an element kind, for labels people read.
fn element_type_name(kind: &ElementType) -> &'static str {
    match kind {
        ElementType::Title => "Title",
        ElementType::Subtitle => "Subtitle",
        ElementType::BodyText => "Body text",
        ElementType::ShapeRectangle => "Rectangle",
        ElementType::ShapeCircle => "Circle",
        ElementType::StatCard => "Stat card",
        ElementType::Picture => "Picture",
        ElementType::Table => "Table",
        ElementType::Chart => "Chart",
    }
}

/// The faint on-canvas prompt for an empty text element; empty when none applies.
fn placeholder_prompt(element: &SlideElement) -> &'static str {
    if !element.content.trim().is_empty() {
        return "";
    }
    match element.element_type {
        ElementType::Title => "Click to add title",
        ElementType::Subtitle | ElementType::BodyText => "Click to add text",
        _ => "",
    }
}

/// The text a slide element shows or is named by: its text, or a picture's file name.
fn element_text(document: &PresentationDocument, element: &SlideElement) -> String {
    if element.element_type == ElementType::Picture {
        picture_view::picture_name(document, element)
    } else if element.element_type == ElementType::Table {
        if let Some(a11y) = table_actions::format_table_a11y(element) {
            a11y
        } else if let Some(table) = element.table() {
            format!("{}x{} Table", table.row_count(), table.col_count())
        } else {
            "Table".to_string()
        }
    } else if element.element_type == ElementType::Chart {
        if let Some(chart) = element.chart() {
            chart.title.unwrap_or_else(|| "Chart".to_string())
        } else {
            "Chart".to_string()
        }
    } else {
        element.content.clone()
    }
}

/// "Edited" while there are changes since the last save, "Saved" once the deck
/// has been written or opened from a file and nothing changed since.
fn deck_status_text(state: &GuiState) -> &'static str {
    if deck_is_dirty(state) {
        "Edited"
    } else if state.save_path.borrow().is_some() {
        "Saved"
    } else {
        ""
    }
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
    let slide_changed =
        slide_focus::active_slide_changed(document.active_slide().map(|slide| slide.id.as_str()));
    presenter::sync(&session);
    file_title::sync(app, state);
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    slide_order::sync(app, &session);
    app.set_slide_count_text(SharedString::from(format!("{} slides", document.len())));
    app.set_slide_titles(synced(
        app.get_slide_titles(),
        document
            .slides
            .iter()
            .map(|slide| SharedString::from(slide.title.as_str()))
            .collect::<Vec<_>>(),
    ));
    app.set_active_slide_index(document.active_index as i32);
    if let Some(slide) = document.active_slide() {
        // The layout control names the slide's own layout, not a default.
        app.set_active_template_index(slide_layouts::choice_for_layout(&slide.layout));
        app.set_slide_title(slide.title.as_str().into());
        app.set_slide_body(active_body(&session).into());
        app.set_slide_notes(slide.speaker_notes.as_str().into());
        let labels = slide
            .elements
            .iter()
            .map(|element| {
                let selected = session.selected_elements.iter().any(|id| id == &element.id);
                SharedString::from(element_names::label(
                    element_type_name(&element.element_type),
                    &element_text(document, element),
                    selected,
                ))
            })
            .collect::<Vec<_>>();
        app.set_element_labels(synced(app.get_element_labels(), labels));
        app.set_element_placeholders(synced(
            app.get_element_placeholders(),
            slide
                .elements
                .iter()
                .map(|element| SharedString::from(placeholder_prompt(element)))
                .collect::<Vec<_>>(),
        ));
        app.set_element_contents(synced(
            app.get_element_contents(),
            slide
                .elements
                .iter()
                .map(|element| SharedString::from(element_text(document, element)))
                .collect::<Vec<_>>(),
        ));
        app.set_element_xs(synced(
            app.get_element_xs(),
            slide
                .elements
                .iter()
                .map(|element| element.x)
                .collect::<Vec<_>>(),
        ));
        app.set_element_ys(synced(
            app.get_element_ys(),
            slide
                .elements
                .iter()
                .map(|element| element.y)
                .collect::<Vec<_>>(),
        ));
        app.set_element_widths(synced(
            app.get_element_widths(),
            slide
                .elements
                .iter()
                .map(|element| element.width)
                .collect::<Vec<_>>(),
        ));
        app.set_element_heights(synced(
            app.get_element_heights(),
            slide
                .elements
                .iter()
                .map(|element| element.height)
                .collect::<Vec<_>>(),
        ));
        app.set_element_rotations(synced(
            app.get_element_rotations(),
            slide
                .elements
                .iter()
                .map(|element| element.rotation_deg)
                .collect::<Vec<_>>(),
        ));
        app.set_element_types(synced(
            app.get_element_types(),
            slide
                .elements
                .iter()
                .map(|element| element_type_index(&element.element_type))
                .collect::<Vec<_>>(),
        ));
        let selected_ids = &session.selected_elements;
        app.set_element_selected(synced(
            app.get_element_selected(),
            slide
                .elements
                .iter()
                .map(|element| selected_ids.contains(&element.id))
                .collect::<Vec<_>>(),
        ));
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
            app.set_active_element_label(element_type_name(&element.element_type).into());
            app.set_active_element_content(element_text(document, element).into());
            app.set_active_element_content_editable(element.element_type != ElementType::Picture);
            app.set_element_x(element.x);
            app.set_element_y(element.y);
            app.set_element_width(element.width);
            app.set_element_height(element.height);
            app.set_element_x_text(format!("{:.0} pt", element.x).into());
            app.set_element_y_text(format!("{:.0} pt", element.y).into());
            app.set_element_width_text(format!("{:.0} pt", element.width).into());
            app.set_element_height_text(format!("{:.0} pt", element.height).into());
            app.set_element_rotation_text(format!("{:.0}°", element.rotation_deg).into());
            element_style_ui::publish(app, element);
            element_style_ui::publish_geometry(app, element);
            if element.element_type == ElementType::Table {
                app.set_inspector_is_table(true);
                if let Some(table) = element.table() {
                    app.set_inspector_table_style_id(table.style_id.clone().into());
                    app.set_inspector_table_header_row(table.header_row);
                    app.set_inspector_table_banded_rows(table.banded_rows);
                    app.set_inspector_table_first_column(table.first_column);
                }
            } else {
                app.set_inspector_is_table(false);
            }
        } else {
            app.set_inspector_is_table(false);
            element_style_ui::clear(app);
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
    let slides = if document.len() == 1 {
        "slide"
    } else {
        "slides"
    };
    let status = match issue_count {
        0 => format!("{} {slides}", document.len()),
        1 => format!("{} {slides} · 1 issue to fix", document.len()),
        n => format!("{} {slides} · {n} issues to fix", document.len()),
    };
    recovery_deferred::project_status(app, SharedString::from(status));
    app.set_status_right(deck_status_text(state).into());
    app.set_deck_dirty(deck_is_dirty(state));
    picture_view::sync(app, document);
    let drag = state.drag_state.borrow();
    app.set_snap_guides_x(synced(
        app.get_snap_guides_x(),
        drag.guides
            .iter()
            .filter(|guide| guide.is_vertical)
            .map(|guide| guide.position)
            .collect::<Vec<_>>(),
    ));
    app.set_snap_guides_y(synced(
        app.get_snap_guides_y(),
        drag.guides
            .iter()
            .filter(|guide| !guide.is_vertical)
            .map(|guide| guide.position)
            .collect::<Vec<_>>(),
    ));
    app.set_marquee_x(drag.marquee_x);
    app.set_marquee_y(drag.marquee_y);
    app.set_marquee_width(drag.marquee_width);
    app.set_marquee_height(drag.marquee_height);
    app.set_marquee_visible(drag.mode == Some(HandleKind::Marquee));
    if recover {
        recovery_deferred::note_edit(app);
    }
    // The borrows end before focus moves: focusing an object selects it, which
    // borrows the session again.
    drop(drag);
    drop(session);
    if let Some(menu_service) = &state.menu_service {
        sync_menu_state(menu_service, app, state);
    }
    if slide_changed {
        app.invoke_focus_slide_after_change();
    }
}

fn apply_theme(app: &PresentApp, theme: &str) {
    appearance::apply_id(app, theme);
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
    let width = width as f32 / Theme::get(app).get_text_scale().max(1.0);
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
    app.set_inspector_available(inspector_available);
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
    window_chrome::install(&app);
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    Theme::get(&app).set_text_scale(args.text_scale);
    appearance::apply_ui_font(&app, true);
    let inspector_available = configure_responsive_layout(&app, args.size);
    let initial = initial_session(args)?;
    let state = GuiState {
        last_saved: RefCell::new(initial.document.clone()),
        last_saved_transitions: RefCell::new(initial.transitions.clone()),
        session: RefCell::new(initial),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(inspector_available),
        save_path: RefCell::new(args.open.as_ref().map(PathBuf::from)),
        pending_replacement: Cell::new(None),
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
    let image = snapshot_component(
        &app,
        args.size.0 as f32,
        args.size.1 as f32,
        args.scale_factor,
    )
    .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn set_status(app: &PresentApp, value: impl Into<SharedString>) {
    recovery_deferred::project_status(app, value);
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn open_request(state: &GuiState) -> OpenFileRequest {
    let mut filters = vec![state.deck_filter.clone()];
    if let Ok(filter) = FileFilter::new("PowerPoint presentation", ["pptx"]) {
        filters.push(filter);
    }
    OpenFileRequest {
        title: "Open Loom Present Deck".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters,
    }
}

fn save_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow().clone();
    let suggested_name = path
        .as_deref()
        .map(|path| {
            if is_loom_deck_path(path) {
                path.to_path_buf()
            } else {
                path.with_extension("loomdeck")
            }
        })
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
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

fn export_pptx_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Loom Present to PowerPoint".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(PPTX_EXPORT_FILENAME.to_string()),
        filters: vec![FileFilter::new("PowerPoint presentation", ["pptx"])
            .expect("static pptx filter is valid")],
    }
}

fn replace_opened_deck(
    app: &PresentApp,
    state: &GuiState,
    path: PathBuf,
    session: PresentationSession,
) {
    *state.last_saved.borrow_mut() = session.document.clone();
    *state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    *state.session.borrow_mut() = session;
    *state.save_path.borrow_mut() = Some(path);
    state.selected_element.set(0);
    refresh(app, state);
}

/// Persists any draft being quarantined, then synchronously covers an accepted
/// PowerPoint candidate in recovery before it can replace the visible deck.
pub(crate) fn accept_pending_pptx_import(
    app: &PresentApp,
    state: &GuiState,
    pending: &mut table_actions::PendingPptxImport,
) -> Result<String, String> {
    let mut kept_path = None;
    if let Some(kept) = pending.kept.as_ref() {
        let directory = startup_recovery::kept_drafts_directory()?;
        let preserved_path = startup_recovery::ensure_kept_draft_is_preserved(
            &directory,
            kept,
            &mut pending.preserved_path,
        )?;
        kept_path = Some(preserved_path.clone());
    }

    let payload = recovery_draft::payload(&pending.session, Some(&pending.path))?;
    checkpoint_pending_import_recovery(pending, payload)
        .map_err(|error| format!("could not checkpoint imported presentation recovery: {error}"))?;
    recovery_deferred::invalidate();

    let path = pending.path.clone();
    let session = pending.session.clone();
    replace_opened_deck(app, state, path.clone(), session);
    let mut status = format!(
        "Opened {}. Import completed with losses: {}",
        path.display(),
        pending.warnings.join("; ")
    );
    if let Some(kept_path) = kept_path {
        status.push_str(&format!(". Unsaved draft kept at {}", kept_path.display()));
    }
    Ok(status)
}

/// Marks that Cancel must restore the visible session before a candidate checkpoint can mutate recovery.
fn checkpoint_pending_import_recovery(
    pending: &mut table_actions::PendingPptxImport,
    payload: Vec<u8>,
) -> Result<(), String> {
    PRESENT_RECOVERY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let recovery = slot
            .as_mut()
            .ok_or_else(|| "recovery service is unavailable".to_string())?;
        pending.cancel_must_checkpoint_visible_session = true;
        recovery
            .checkpoint("loom.present/1", payload)
            .map_err(|error| error.to_string())
    })
}

/// Confirms an active recovery slot exists and its store can be opened now; later writes can still fail.
pub(crate) fn ensure_recovery_service() -> Result<(), String> {
    let directory = PRESENT_RECOVERY.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|recovery| recovery.directory().to_path_buf())
            .ok_or_else(|| "recovery service is unavailable".to_string())
    })?;
    loom_production::snapshot::SnapshotRecovery::open_at(directory)
        .map(|_| ())
        .map_err(|error| format!("recovery service is not usable: {error}"))
}

fn replace_with_empty_deck(app: &PresentApp, state: &GuiState) {
    if !recovery_replacement::clear_recovery_before_replacement(app, state) {
        return;
    }
    let session = empty_session();
    *state.last_saved.borrow_mut() = session.document.clone();
    *state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    *state.session.borrow_mut() = session;
    *state.save_path.borrow_mut() = None;
    state.selected_element.set(0);
    refresh(app, state);
    set_status(app, "Created unsaved presentation");
}

fn replace_with_template(app: &PresentApp, state: &GuiState, choice: i32) {
    if !recovery_replacement::clear_recovery_before_replacement(app, state) {
        return;
    }
    let session = slide_layouts::template_session(empty_session(), choice);
    *state.last_saved.borrow_mut() = session.document.clone();
    *state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    *state.session.borrow_mut() = session;
    *state.save_path.borrow_mut() = None;
    state.selected_element.set(0);
    refresh(app, state);
    let name = slide_layouts::TEMPLATE_NAMES
        .get(choice.max(0) as usize)
        .unwrap_or(&slide_layouts::TEMPLATE_NAMES[0]);
    set_status(
        app,
        format!("Created unsaved presentation from the {name} template"),
    );
}

fn replace_with_sample_deck(app: &PresentApp, state: &GuiState) {
    if !recovery_replacement::clear_recovery_before_replacement(app, state) {
        return;
    }
    let session = sample_session();
    *state.last_saved.borrow_mut() = session.document.clone();
    *state.last_saved_transitions.borrow_mut() = session.transitions.clone();
    *state.session.borrow_mut() = session;
    *state.save_path.borrow_mut() = None;
    state.selected_element.set(0);
    refresh(app, state);
    set_status(app, "Created unsaved presentation from the sample deck");
}

/// Open and validate a candidate deck before replacing the live session.
/// Cancelled or invalid opens leave the current deck untouched.
fn open_deck_from_picker(app: &PresentApp, state: &GuiState) {
    match state.dialogs.open_file(&open_request(state)) {
        Ok(Some(path)) => {
            if pptx_import::is_pptx_path(&path) {
                match pptx_import::prepare(&path) {
                    Ok(candidate) if !candidate.warnings.is_empty() => {
                        app.set_pptx_import_warning_message(
                            pptx_import::warning_message(&candidate.warnings).into(),
                        );
                        table_actions::PENDING_PPTX_IMPORT.with(|slot| {
                            *slot.borrow_mut() = Some(table_actions::PendingPptxImport {
                                path: candidate.path,
                                session: candidate.session,
                                warnings: candidate.warnings,
                                kept: None,
                                preserved_path: None,
                                cancel_must_checkpoint_visible_session: false,
                            });
                        });
                        recovery_deferred::hold_for_import_warning();
                        app.set_pptx_import_warning_open(true);
                        set_status(
                            app,
                            "Review the PowerPoint import warning before replacing this deck",
                        );
                    }
                    Ok(candidate) => {
                        if recovery_replacement::clear_recovery_before_replacement(app, state) {
                            replace_opened_deck(
                                app,
                                state,
                                candidate.path.clone(),
                                candidate.session,
                            );
                            set_status(app, format!("Opened {}", candidate.path.display()));
                        }
                    }
                    Err(error) => set_status(app, format!("Open failed: {error}")),
                }
            } else {
                match load_session(&path) {
                    Ok(session) => {
                        if recovery_replacement::clear_recovery_before_replacement(app, state) {
                            replace_opened_deck(app, state, path.clone(), session);
                            set_status(app, format!("Opened {}", path.display()));
                        }
                    }
                    Err(error) => set_status(app, format!("Open failed: {error}")),
                }
            }
        }
        Ok(None) => set_status(app, "Open cancelled"),
        Err(error) => set_status(app, format!("Open dialog failed: {error}")),
    }
}

/// Whether `path` names a Loom Present deck (the format Save writes).
fn is_loom_deck_path(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("loomdeck"))
}

fn save_current_deck(
    app: &PresentApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    // A deck opened from a PowerPoint file saves as a Loom deck: writing a Loom
    // deck over the `.pptx` would destroy it.
    let current_path = (!force_picker)
        .then(|| state.save_path.borrow().clone())
        .flatten()
        .filter(|path| is_loom_deck_path(path));
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

    state.session.borrow_mut().end_table_text_edit();
    let bytes = save_presentation_session(&state.session.borrow())?;
    loom_storage::atomic_write(&path, &bytes)
        .map_err(|error| format!("failed to atomic write '{}': {error}", path.display()))?;
    *state.save_path.borrow_mut() = Some(path.clone());
    *state.last_saved.borrow_mut() = state.session.borrow().document.clone();
    *state.last_saved_transitions.borrow_mut() = state.session.borrow().transitions.clone();
    file_title::sync(app, state);
    app.set_status_right(deck_status_text(state).into());
    app.set_deck_dirty(deck_is_dirty(state));
    recovery_deferred::invalidate();
    match recovery_replacement::checkpoint_saved_deck(recovery_draft::wrap(
        Some(path.as_path()),
        &bytes,
    )) {
        Ok(()) => set_status(app, format!("Saved {}", path.display())),
        Err(error) => {
            recovery_deferred::report_checkpoint_failure(app, error);
            set_status(app, "Deck saved; recovery checkpoint failed");
        }
    }
    Ok(true)
}

fn capture_present_journey_step(
    app: &PresentApp,
    args: &Args,
    out_dir: &Path,
    name: &str,
) -> Result<String, String> {
    let image = snapshot_component(
        app,
        args.size.0 as f32,
        args.size.1 as f32,
        args.scale_factor,
    )
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

/// Whether two saved documents are identical, including the slide they open on:
/// used to check that a saved file reopens as the same deck.
fn presentation_documents_match(left: &PresentationDocument, right: &PresentationDocument) -> bool {
    left.active_index == right.active_index && deck_guard::content_matches(left, right)
}

/// Record the controller-backed direct manipulation journey with per-step
/// screenshots and serialized/reopened/exported artifacts.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("create journey output '{}': {error}", out_dir.display()))?;
    let app = PresentApp::new().map_err(|error| error.to_string())?;
    appearance::apply_ui_font(&app, true);
    window_chrome::install(&app);
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    Theme::get(&app).set_text_scale(args.text_scale);
    let inspector_available = configure_responsive_layout(&app, args.size);
    let save_path = out_dir.join("present-manipulation.loomdeck");
    let export_path = out_dir.join("present-manipulation.pdf");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(save_path.clone())],
        [Some(save_path.clone()), Some(export_path.clone())],
    ));
    let initial = initial_session(args)?;
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(initial.document.clone()),
        last_saved_transitions: RefCell::new(initial.transitions.clone()),
        session: RefCell::new(initial),
        pending_replacement: Cell::new(None),
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
    let (shape_id, shape_x, shape_y) = {
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
        (element.id.clone(), element.x, element.y)
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
    // New objects cascade, so derive the drag from the real origin instead of
    // assuming the pre-cascade (120, 260) position. Land three points left and
    // one point above the body edge so the snap guides still engage.
    app.invoke_element_moved(
        shape_index as i32,
        90.0 + 3.0 - shape_x,
        230.0 + 1.0 - shape_y,
    );
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
    // macOS: Slint's winit backend otherwise installs its app-only default
    // menu over Loom's native File/Edit/View/Slide menu. Writer and Sheets
    // disable it the same way before building their shared menu.
    #[cfg(target_os = "macos")]
    {
        let backend = i_slint_backend_winit::Backend::builder()
            .with_default_menu_bar(false)
            .build()
            .map_err(|error| error.to_string())?;
        slint::platform::set_platform(Box::new(backend)).map_err(|error| error.to_string())?;
    }

    let app = PresentApp::new().map_err(|error| error.to_string())?;
    window_chrome::install(&app);
    configure_direction(&app, args.rtl);
    // The saved appearance, unless `--theme` was given.
    appearance::start(
        &app,
        appearance::APPLICATION_ID,
        args.theme_explicit.then_some(args.theme.as_str()),
    );
    appearance::apply_ui_font(&app, false);
    Theme::get(&app).set_text_scale(args.text_scale);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let inspector_available = configure_responsive_layout(&app, args.size);
    let recovered = initialize_snapshot_recovery()?;
    let initial_path = args.open.as_ref().map(PathBuf::from);
    let mut startup =
        startup_recovery::startup_sessions(recovered.as_deref(), initial_path.as_deref())?;
    let pending_import = startup.pending_import.take();
    let deferred_kept = if pending_import.is_some() {
        startup.kept.take()
    } else {
        None
    };
    // A draft of another file is written to a file of its own before recovery is
    // reset, so opening this file never loses it. A warned import waits for the
    // user's choice before either step.
    let kept = match (pending_import.is_none(), startup.kept.take()) {
        (true, Some(kept)) => {
            let directory = startup_recovery::kept_drafts_directory()?;
            let path = startup_recovery::write_kept_draft(
                &directory,
                &kept,
                startup_recovery::unix_seconds(),
            )?;
            recovery_replacement::reset_recovery_store()?;
            Some((kept.name, path))
        }
        _ => None,
    };
    let (initial, saved_baseline) = (startup.session, startup.baseline);
    let deck_filter =
        FileFilter::new("Loom Present deck", ["loomdeck"]).map_err(|error| error.to_string())?;
    let pdf_filter = FileFilter::new("PDF document", ["pdf"]).map_err(|error| error.to_string())?;
    let menu_service = Rc::new(NativeMenuBar::new());
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(saved_baseline.document),
        last_saved_transitions: RefCell::new(saved_baseline.transitions),
        session: RefCell::new(initial),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(inspector_available),
        save_path: RefCell::new(startup.save_path),
        dialogs,
        deck_filter,
        pdf_filter,
        menu_service: Some(menu_service.clone()),
        drag_state: RefCell::new(DragState::default()),
    });

    wire_app_callbacks(&app, &state);
    wire_responsive_layout_with_state(&app, state.clone());
    wire_close_guard(&app, &state);

    let menu_bar = build_present_menu_bar();
    menu_service
        .install_menu_bar(&menu_bar)
        .map_err(|error| error.to_string())?;
    local_menu::wire_action(&app, menu_service.clone());
    // A choice from any surface refreshes the menu check marks and the
    // presenter window.
    appearance::on_change(|app| {
        app.invoke_view_state_changed();
        presenter::mirror_theme(app);
    });
    let _ = local_menu::sync(&app, &menu_service);

    let app_ref = app.as_weak();
    menu_service
        .register_action_sink(std::sync::Arc::new(move |action: CommandAction| {
            schedule_menu_action(&app_ref, action)
        }))
        .map_err(|error| error.to_string())?;

    wire_palette(&app);
    match (kept, startup.restored) {
        (Some((name, path)), _) => set_status(
            &app,
            format!("Unsaved changes to {name} were kept in {}", path.display()),
        ),
        (None, Some(name)) => set_status(&app, format!("Restored unsaved changes to {name}")),
        (None, None) => {}
    }
    if let Some(candidate) = pending_import {
        let warnings = candidate.warnings.clone();
        app.set_pptx_import_warning_message(pptx_import::warning_message(&warnings).into());
        table_actions::PENDING_PPTX_IMPORT.with(|slot| {
            *slot.borrow_mut() = Some(table_actions::PendingPptxImport {
                path: candidate.path,
                session: candidate.session,
                warnings,
                kept: deferred_kept,
                preserved_path: None,
                cancel_must_checkpoint_visible_session: false,
            });
        });
        recovery_deferred::hold_for_import_warning();
        app.set_pptx_import_warning_open(true);
        set_status(
            &app,
            "Review the PowerPoint import warning before replacing this deck",
        );
    }
    refresh(&app, &state);
    if args.theme_chooser {
        app.set_theme_chooser_open(true);
    }
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    app.show().map_err(|error| error.to_string())?;
    focus_editor_at_launch(&app);
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// The slide editor takes keyboard focus once the window exists, unless a
/// startup overlay owns it, so arrows, Delete and Ctrl+Z work without a click.
fn focus_editor_at_launch(app: &PresentApp) {
    if !(app.get_theme_chooser_open()
        || app.get_palette_open()
        || app.get_save_changes_open()
        || app.get_pptx_import_warning_open())
    {
        app.invoke_focus_editor();
    }
}

fn build_present_menu_bar() -> MenuBar {
    let mut menu_bar = build_standard_menu_bar(
        "Loom Present",
        vec![
            MenuItem::action_with_shortcut(
                "file.export_pdf",
                "Export to PDF...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action("file.new_sample", "New from Sample Deck"),
            MenuItem::action("file.export_pptx", "Export to PowerPoint..."),
        ],
        vec![],
        vec![
            MenuItem::check("view.navigator", "Navigator", true),
            MenuItem::check("view.notes", "Speaker Notes", false),
            MenuItem::check("view.inspector", "Format Inspector", false),
        ]
        .into_iter()
        .chain(loom_desktop::appearance::menu_items())
        .collect(),
        vec![Menu::new(
            "Slide",
            vec![
                MenuItem::action_with_shortcut(
                    "slide.new",
                    "New Slide",
                    MenuShortcut::primary_shift("N"),
                ),
                MenuItem::action("slide.insert_table", "Insert Table..."),
                MenuItem::action("slide.insert_chart", "Paste Chart from Sheets..."),
                MenuItem::action("slide.insert_image", "Insert Image..."),
                MenuItem::action("slide.duplicate", "Duplicate Slide"),
                MenuItem::action("slide.delete", "Delete Slide"),
                MenuItem::Separator,
                MenuItem::action_with_shortcut(
                    slide_order::MOVE_UP,
                    "Move Slide Up",
                    MenuShortcut::primary_alt("Up"),
                ),
                MenuItem::action_with_shortcut(
                    slide_order::MOVE_DOWN,
                    "Move Slide Down",
                    MenuShortcut::primary_alt("Down"),
                ),
                MenuItem::Separator,
                MenuItem::action("slide.prev", "Previous Slide"),
                MenuItem::action("slide.next", "Next Slide"),
            ],
        )],
    );
    file_menu::group(&mut menu_bar);
    menu_bar.disable_items_except(local_menu::SUPPORTED_COMMANDS);
    menu_bar
}

fn dispatch_command(app: &PresentApp, id: &str) -> bool {
    if app.get_pptx_import_warning_open() {
        return is_present_menu_command(id);
    }
    match id {
        "file.new" => app.invoke_new_deck(),
        "file.new_sample" => app.invoke_new_sample_deck(),
        "slide.insert_table" => app.invoke_add_table(),
        "slide.insert_chart" => app.invoke_add_chart(),
        "slide.insert_image" => app.invoke_add_picture(),
        "table.delete_row" | "slide.delete_table_row" => {
            app.invoke_table_action("delete-row".into())
        }
        "table.delete_column" | "slide.delete_table_column" => {
            app.invoke_table_action("delete-col".into())
        }
        "file.open" => app.invoke_open_deck(),
        "file.save" => app.invoke_save_deck(),
        "file.save_as" => app.invoke_save_as_deck(),
        "file.export_pdf" => app.invoke_export_pdf(),
        "file.export_pptx" => app.invoke_export_pptx(),
        "edit.undo" => app.invoke_undo(),
        "edit.redo" => app.invoke_redo(),
        "edit.cut" => app.invoke_cut(),
        "edit.copy" => app.invoke_copy(),
        "edit.paste" => app.invoke_paste(),
        "slide.new" => app.invoke_add_slide(),
        "slide.duplicate" => app.invoke_duplicate_slide(),
        "slide.delete" => app.invoke_delete_slide(),
        "slide.prev" => app.invoke_prev_slide(),
        "slide.next" => app.invoke_next_slide(),
        "view.inspector" => app.invoke_toggle_inspector(),
        id if view_state::dispatch(app, id) => {}
        id if appearance::dispatch(app, id) => {}
        id if slide_order::dispatch(app, id) => {}
        "app.palette" => app.invoke_open_palette(),
        _ => return false,
    }
    true
}

fn is_present_menu_command(id: &str) -> bool {
    slide_order::is_command(id)
        || matches!(
            id,
            "file.new"
                | "file.new_sample"
                | "slide.insert_table"
                | "slide.insert_chart"
                | "slide.insert_image"
                | "table.delete_row"
                | "table.delete_column"
                | "slide.delete_table_row"
                | "slide.delete_table_column"
                | "file.open"
                | "file.save"
                | "file.save_as"
                | "file.export_pdf"
                | "file.export_pptx"
                | "edit.undo"
                | "edit.redo"
                | "edit.cut"
                | "edit.copy"
                | "edit.paste"
                | "slide.new"
                | "slide.duplicate"
                | "slide.delete"
                | "slide.prev"
                | "slide.next"
                | "view.inspector"
                | "view.navigator"
                | "view.notes"
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
            dispatch_scheduled_menu_action(&app, &id);
        })
        .map_err(|error| {
            DesktopError::InvalidRequest(format!(
                "failed to schedule Present menu command {error_id}: {error}"
            ))
        })
}

fn dispatch_scheduled_menu_action(app: &PresentApp, id: &str) {
    if !dispatch_command(app, id) {
        set_status(app, format!("Unsupported menu command: {id}"));
    }
}

/// Closing with unsaved work asks first, through the same Save / Discard /
/// Cancel dialog New and Open use. The custom title bar's close button raises
/// this same request.
fn wire_close_guard(app: &PresentApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.window().on_close_requested(move || {
        let Some(app) = app_ref.upgrade() else {
            return slint::CloseRequestResponse::HideWindow;
        };
        if close_blocked_by_pending_import_recovery(&app) {
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        if request_deck_replacement(&app, &state, PendingReplacement::CloseWindow) {
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        if !recovery_replacement::prepare_to_close(&app) {
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        // The presenter window must not keep the process alive after the deck closes.
        slint::CloseRequestResponse::HideWindow
    });
}

/// A failed import acceptance may already be present in the recovery journal.
/// Keep the window available until Cancel restores coverage for the visible deck
/// or Continue successfully accepts the candidate.
fn close_blocked_by_pending_import_recovery(app: &PresentApp) -> bool {
    let must_resolve = table_actions::PENDING_PPTX_IMPORT.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|pending| pending.cancel_must_checkpoint_visible_session)
    });
    if must_resolve {
        set_status(
            app,
            "Cannot close yet: import recovery is incomplete. Choose Cancel to keep this presentation or Continue to retry.",
        );
    }
    must_resolve
}

#[cfg(test)]
fn end_session_recovery() {
    if let Err(error) = recovery_replacement::reset_for_close() {
        eprintln!("Present could not clear its recovery data: {error}");
    }
}

fn continue_deck_replacement(app: &PresentApp, state: &Rc<GuiState>) {
    match state.pending_replacement.take() {
        Some(PendingReplacement::NewDeck) => replace_with_empty_deck(app, state),
        Some(PendingReplacement::NewSampleDeck) => replace_with_sample_deck(app, state),
        Some(PendingReplacement::NewFromTemplate(choice)) => {
            replace_with_template(app, state, choice)
        }
        Some(PendingReplacement::OpenDeck) => open_deck_from_picker(app, state),
        Some(PendingReplacement::CloseWindow) => {
            if close_blocked_by_pending_import_recovery(app) {
                return;
            }
            if recovery_replacement::prepare_to_close(app) {
                let _ = slint::ComponentHandle::hide(app);
            }
        }
        None => {}
    }
}

fn wire_app_callbacks(app: &PresentApp, state: &Rc<GuiState>) {
    view_state::wire(app, state);
    slide_order::wire(app, state);
    deck_guard::wire(app, state);
    table_actions::wire(app, state);
    element_style_ui::wire(app, state);
    recovery_deferred::wire_callbacks(app, state);
    {
        let state = state.clone();
        app.on_end_table_cell_edit(move || {
            state.session.borrow_mut().end_table_text_edit();
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_strip_window_changed(move |first, count| {
            if let Some(app) = app_ref.upgrade() {
                app.set_strip_first(first);
                app.set_strip_count(count);
                if let Ok(session) = state.session.try_borrow() {
                    picture_view::sync_thumbnails(&app, &session.document);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_sample_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if !request_deck_replacement(&app, &state, PendingReplacement::NewSampleDeck) {
                    replace_with_sample_deck(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_picture(move || {
            if let Some(app) = app_ref.upgrade() {
                picture_view::insert_from_picker(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if !request_deck_replacement(&app, &state, PendingReplacement::NewDeck) {
                    replace_with_empty_deck(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if request_deck_replacement(&app, &state, PendingReplacement::OpenDeck) {
                    return;
                }
                open_deck_from_picker(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_changes_save(move || {
            if let Some(app) = app_ref.upgrade() {
                let pending = state.pending_replacement.take();
                match save_current_deck(&app, &state, false) {
                    Ok(true) => {
                        app.set_save_changes_open(false);
                        state.pending_replacement.set(pending);
                        continue_deck_replacement(&app, &state);
                    }
                    Ok(false) => state.pending_replacement.set(pending),
                    Err(error) => {
                        state.pending_replacement.set(pending);
                        set_status(&app, format!("Save failed: {error}"));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_changes_discard(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_save_changes_open(false);
                continue_deck_replacement(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_changes_cancel(move || {
            if let Some(app) = app_ref.upgrade() {
                state.pending_replacement.set(None);
                app.set_save_changes_open(false);
                set_status(&app, "Replacement cancelled");
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
                slide_layouts::add_blank_slide(&mut session.document);
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
    clipboard_actions::wire(app, state);
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_slide(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    {
                        // Release the mutable borrow before `refresh` reads the session.
                        let mut session = state.session.borrow_mut();
                        session.end_table_text_edit();
                        if session.document.select_slide(index as usize) {
                            session.clear_selection();
                        }
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
                session.end_table_text_edit();
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
                    let (x, y) = added_object_origin(&session, (120.0, 260.0), (300.0, 140.0));
                    session.add_element(text_element(
                        &format!("shape-{count}"),
                        ElementType::ShapeRectangle,
                        "Shape",
                        x,
                        y,
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
                    let (x, y) = added_object_origin(&session, (120.0, 220.0), (520.0, 100.0));
                    session.add_element(text_element(
                        &format!("text-{count}"),
                        ElementType::BodyText,
                        "New text",
                        x,
                        y,
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
                if let Some((element_id, element_type)) = session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.get(selected))
                    .map(|element| (element.id.clone(), element.element_type.clone()))
                {
                    if element_type == ElementType::Picture {
                        session.end_table_text_edit();
                        return;
                    }
                    if element_type == ElementType::Table {
                        let (row, col) = table_actions::get_active_table_cell(&element_id);
                        session.edit_table_cell_text(&element_id, row, col, content.as_str());
                    } else {
                        session.end_table_text_edit();
                        session.checkpoint();
                        let is_title = if let Some(element) = session
                            .document
                            .active_slide_mut()
                            .and_then(|slide| slide.elements.get_mut(selected))
                        {
                            if element.element_type == ElementType::Chart {
                                if let Some(mut chart) = element.chart() {
                                    chart.title = Some(content.as_str().to_string());
                                    element.set_chart(&chart);
                                }
                            } else {
                                element.content = content.as_str().to_string();
                            }
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
                table_actions::select_table_cell_under_press(&app, &state, index as usize);
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
                    let ((mut x, mut y, mut width, mut height), guides) =
                        resize_targets(&session, &element, mode, dx, dy);
                    let is_picture = session
                        .document
                        .active_slide()
                        .and_then(|slide| slide.elements.iter().find(|item| item.id == element.id))
                        .is_some_and(|item| item.element_type == ElementType::Picture);
                    if is_picture {
                        (x, y, width, height) = lock_aspect(
                            (element.x, element.y, element.width, element.height),
                            (x, y, width, height),
                            matches!(
                                mode,
                                HandleKind::ResizeNorthWest | HandleKind::ResizeSouthWest
                            ),
                            matches!(
                                mode,
                                HandleKind::ResizeNorthWest | HandleKind::ResizeNorthEast
                            ),
                        );
                    }
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
        let delete_key: SharedString = slint::platform::Key::Delete.into();
        let backspace_key: SharedString = slint::platform::Key::Backspace.into();
        let page_up_key: SharedString = slint::platform::Key::PageUp.into();
        let page_down_key: SharedString = slint::platform::Key::PageDown.into();
        app.on_canvas_key_pressed(move |key, shift, modified| {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_preview_mode() {
                    return EventResult::Reject;
                }
                // Ctrl+B, Ctrl+I and Ctrl+] / Ctrl+[ style the selected text objects.
                if modified && !shift {
                    let request = match key.as_str() {
                        "b" | "B" => Some(("bold", 0)),
                        "i" | "I" => Some(("italic", 0)),
                        "]" => Some(("size", 1)),
                        "[" => Some(("size", -1)),
                        _ => None,
                    };
                    if let Some((kind, value)) = request {
                        if state.session.borrow().selected_elements.is_empty() {
                            return EventResult::Reject;
                        }
                        element_style_ui::request(&app, kind, value);
                        return EventResult::Accept;
                    }
                }
                if modified {
                    return EventResult::Reject;
                }
                if table_actions::handle_table_key(&app, &state, key.as_str(), shift) {
                    return EventResult::Accept;
                }
                // Page Up and Page Down move between slides without the pointer.
                if key == page_down_key {
                    app.invoke_next_slide();
                    return EventResult::Accept;
                }
                if key == page_up_key {
                    app.invoke_prev_slide();
                    return EventResult::Accept;
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
                if key == delete_key || key == backspace_key {
                    let removed = state.session.borrow_mut().remove_selected_elements();
                    if removed > 0 {
                        state.selected_element.set(0);
                        refresh(&app, &state);
                        return EventResult::Accept;
                    }
                }
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
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_presenter(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = presenter::open(&app, &state.session.borrow()) {
                    recovery_deferred::project_status(
                        &app,
                        format!("Could not open the presenter view: {error}"),
                    );
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_preview_mode(move || {
            if let Some(app) = app_ref.upgrade() {
                let next = !app.get_is_preview_mode();
                app.set_is_preview_mode(next);
                if !next {
                    presenter::close();
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_apply_template(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let layout = slide_layouts::layout_for_choice(index);
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                if let Some(slide) = session.document.active_slide_mut() {
                    slide_layouts::apply_layout(slide, layout);
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
                    let kind = match index {
                        1 => TransitionKind::Dissolve,
                        2 => TransitionKind::Push,
                        3 => TransitionKind::Morph,
                        _ => TransitionKind::None,
                    };
                    if session.transition_for(&id) != kind {
                        session.checkpoint();
                        session.set_transition(&id, kind);
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
        app.on_export_pptx(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&export_pptx_request(&state)) {
                    Ok(Some(path)) => {
                        let result = export_pptx(&state.session.borrow()).and_then(|bytes| {
                            loom_storage::atomic_write(&path, &bytes)
                                .map_err(|error| error.to_string())
                        });
                        match result {
                            Ok(()) => {
                                let morph = state
                                    .session
                                    .borrow()
                                    .transitions
                                    .values()
                                    .any(|kind| kind == &TransitionKind::Morph);
                                let status = if morph {
                                    format!(
                                        "Exported {}; Morph transitions exported as fades",
                                        path.display()
                                    )
                                } else {
                                    format!("Exported {}", path.display())
                                };
                                set_status(&app, status);
                            }
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
                app.set_show_inspector(!app.get_show_inspector());
                if let Some(menu_service) = &state.menu_service {
                    sync_menu_state(menu_service, &app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_create_theme(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                // The chooser closes first; unsaved work is asked about before it is replaced.
                app.set_theme_chooser_open(false);
                if !request_deck_replacement(&app, &state, PendingReplacement::NewFromTemplate(idx))
                {
                    replace_with_template(&app, &state, idx);
                }
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

mod palette;
#[cfg(test)]
pub(crate) use palette::{master_palette, PaletteAction};
pub(crate) use palette::{rebuild_palette, PaletteCommand};

fn wire_palette(app: &PresentApp) {
    palette::wire_palette(app);
}

#[cfg(test)]
mod audit_tests;
#[cfg(test)]
mod desktop_tests;
#[cfg(test)]
mod export_pptx_tests;
#[cfg(test)]
mod picture_tests;
#[cfg(test)]
mod pptx_save_tests;

mod appearance;
#[cfg(test)]
mod appearance_tests;
mod clipboard_actions;
mod deck_guard;
use deck_guard::{deck_is_dirty, request_deck_replacement};
mod element_names;
#[cfg(test)]
mod element_style_tests;
mod element_style_ui;
mod file_menu;
mod file_title;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod keyboard_flow_tests;
mod local_menu;
mod menu_models;
mod model_sync;
mod object_parts;
mod palette_rank;
mod picture_view;
mod pptx_import;
mod presenter;
mod presenter_thumbs;
#[cfg(test)]
mod recovery_close_tests;
mod recovery_deferred;
mod recovery_draft;
mod recovery_replacement;
#[cfg(test)]
mod scaling_tests;
mod slide_focus;
#[cfg(test)]
mod slide_layout_tests;
mod slide_layouts;
mod slide_order;
#[cfg(test)]
mod slide_order_tests;
mod startup_recovery;
#[cfg(test)]
mod startup_recovery_tests;
mod table_actions;
#[cfg(test)]
mod template_chooser_tests;
#[cfg(test)]
mod test_environment;
mod view_state;
mod window_chrome;

#[cfg(test)]
mod dpi_surfaces_tests;
#[cfg(test)]
mod frame_bench_tests;
#[cfg(test)]
mod rtl_tests;
#[cfg(test)]
mod scale_surfaces_tests;
#[cfg(test)]
mod text_scale_tests;
#[cfg(test)]
mod toolbar_tests;
#[cfg(test)]
mod ui_font_tests;

#[cfg(test)]
mod accessibility_tests;
#[cfg(test)]
mod deck_guard_tests;
#[cfg(test)]
mod import_warning_tests;
#[cfg(test)]
mod layout_bounds_tests;
#[cfg(test)]
mod miniature_tests;
#[cfg(test)]
mod table_tests;
#[cfg(test)]
mod visual_defect_tests;
