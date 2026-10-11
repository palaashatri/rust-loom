//! The font family control in the inspector: what its list holds, how a
//! choice reaches the document, and how the scan result reaches the window.
//!
//! The list has three sections, in this order: the families used recently,
//! the families this document uses (with a marker for any that are not
//! installed here), and every installed or bundled family. Typing filters it;
//! typing a name that is not in the list offers to use that name anyway, as a
//! document may name a font this machine lacks. A choice applies to the
//! selection as one undoable edit, or to the text typed next when nothing is
//! selected, and the family is remembered for the next run.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use loom_writer_core::fonts::{self, FamilyResolution, FontCatalog, DEFAULT_FAMILY};
use loom_writer_core::WriterDocument;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::document_formatting::{
    selection_font_family, set_selection_font_family, DocumentSelection, FontChoice,
};
use crate::font_store::{self, FontStore};
use crate::{apply_with_history, typing_style, FontPicker, FontPickerLine, GuiState, WriterApp};

/// Lines of the list shown at once (headings count).
pub(crate) const WINDOW_LINES: usize = 9;
/// Longest name a typed row accepts, in characters.
const MAX_TYPED_CHARS: usize = 128;

pub(crate) const TAG_MISSING: &str = "Not installed";
pub(crate) const TAG_IN_USE: &str = "In use";

/// Where in the list a row sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Recent,
    InDocument,
    AllFonts,
    /// "Use <typed name>".
    Typed,
}

impl Section {
    fn heading(self) -> Option<&'static str> {
        match self {
            Self::Recent => Some("Recently used"),
            Self::InDocument => Some("In this document"),
            Self::AllFonts => Some("All fonts"),
            Self::Typed => None,
        }
    }
}

/// One font the list offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// What choosing it stores in the document (empty: the document font).
    pub value: String,
    pub label: String,
    pub tag: &'static str,
    /// The family the name is previewed in: the one that is really drawn.
    pub preview: String,
    pub section: Section,
}

/// What the list is built from.
pub(crate) struct Inputs<'a> {
    pub recents: &'a [String],
    pub document: &'a [String],
    pub installed: &'a [String],
    pub query: &'a str,
}

fn row(
    value: &str,
    section: Section,
    in_use: bool,
    resolve: &dyn Fn(&str) -> FamilyResolution,
) -> Row {
    let resolution = resolve(value);
    let label = if value.trim().is_empty() {
        format!("Default ({DEFAULT_FAMILY})")
    } else {
        value.trim().to_owned()
    };
    let tag = if resolution.standing.is_missing() {
        TAG_MISSING
    } else if in_use {
        TAG_IN_USE
    } else {
        ""
    };
    Row {
        value: value.trim().to_owned(),
        label,
        tag,
        preview: resolution.drawn,
        section,
    }
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The rows of the list for `inputs`. `resolve` says how a name stands on this
/// machine (tests pass their own; the window passes the process catalogue).
pub(crate) fn build_rows(
    inputs: &Inputs<'_>,
    resolve: &dyn Fn(&str) -> FamilyResolution,
) -> Vec<Row> {
    let in_document = |name: &str| {
        inputs
            .document
            .iter()
            .any(|used| fonts::same_family(used, name))
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut add = |rows: &mut Vec<Row>, value: &str, section: Section, in_use: bool| {
        if seen.contains(&key(value)) {
            return;
        }
        seen.push(key(value));
        rows.push(row(value, section, in_use, resolve));
    };
    for name in inputs.recents {
        add(&mut rows, name, Section::Recent, in_document(name));
    }
    add(&mut rows, "", Section::InDocument, true);
    for name in inputs.document {
        add(&mut rows, name, Section::InDocument, true);
    }
    for name in inputs.installed {
        add(&mut rows, name, Section::AllFonts, in_document(name));
    }

    let query = key(inputs.query);
    if !query.is_empty() {
        rows.retain(|row| key(&row.label).contains(&query) || key(&row.value).contains(&query));
        // Names that start with what was typed come first within a section.
        rows.sort_by_key(|row| {
            (
                row.section as u8,
                !(key(&row.label).starts_with(&query) || key(&row.value).starts_with(&query)),
            )
        });
        let typed = inputs.query.trim();
        let exists = rows.iter().any(|row| key(&row.value) == query);
        if !exists && typed.chars().count() <= MAX_TYPED_CHARS {
            let mut offered = row(typed, Section::Typed, false, resolve);
            offered.label = format!("Use \"{typed}\"");
            rows.push(offered);
        }
    }
    rows
}

/// A heading or a font, in list order.
enum Line {
    Heading(&'static str),
    Item(usize),
}

fn layout(rows: &[Row]) -> Vec<Line> {
    let mut lines = Vec::with_capacity(rows.len() + 3);
    let mut previous: Option<Section> = None;
    for (index, row) in rows.iter().enumerate() {
        if previous != Some(row.section) {
            if let Some(heading) = row.section.heading() {
                lines.push(Line::Heading(heading));
            }
            previous = Some(row.section);
        }
        lines.push(Line::Item(index));
    }
    lines
}

/// The first line of a window of `size` lines that keeps `selected` in view,
/// moving `first` as little as possible (and showing the heading above the
/// first font of a section).
fn scrolled(lines: &[Line], selected: usize, first: usize, size: usize) -> usize {
    let Some(at) = lines
        .iter()
        .position(|line| matches!(line, Line::Item(index) if *index == selected))
    else {
        return 0;
    };
    let top = if at > 0 && matches!(lines[at - 1], Line::Heading(_)) {
        at - 1
    } else {
        at
    };
    let mut first = first.min(lines.len().saturating_sub(size));
    if top < first {
        first = top;
    } else if at >= first + size {
        first = at + 1 - size;
    }
    first
}

#[derive(Default)]
struct PickerState {
    open: bool,
    query: String,
    rows: Vec<Row>,
    selected: usize,
    first: usize,
    recents: Vec<String>,
    store: Option<FontStore>,
    loading: bool,
}

thread_local! {
    static PICKER: RefCell<PickerState> = RefCell::new(PickerState::default());
    static LIVE: RefCell<Option<(slint::Weak<WriterApp>, Rc<GuiState>)>> = const { RefCell::new(None) };
}

fn rebuild(document: &WriterDocument) {
    let installed: Vec<String> = fonts::font_catalog().families().to_vec();
    let used = fonts::document_families(document);
    PICKER.with(|picker| {
        let mut picker = picker.borrow_mut();
        let rows = build_rows(
            &Inputs {
                recents: &picker.recents,
                document: &used,
                installed: &installed,
                query: &picker.query,
            },
            &fonts::resolve_family,
        );
        picker.rows = rows;
    });
}

/// Highlight the row that holds `family`, or the first row.
fn select_current(family: &str) {
    PICKER.with(|picker| {
        let mut picker = picker.borrow_mut();
        picker.selected = picker
            .rows
            .iter()
            .position(|row| key(&row.value) == key(family))
            .unwrap_or(0);
        picker.first = 0;
    });
}

/// Send the list's state to the window.
fn publish(app: &WriterApp) {
    PICKER.with(|picker| {
        let mut picker = picker.borrow_mut();
        let lines = layout(&picker.rows);
        picker.first = scrolled(&lines, picker.selected, picker.first, WINDOW_LINES);
        let shown: Vec<FontPickerLine> = lines
            .iter()
            .skip(picker.first)
            .take(WINDOW_LINES)
            .map(|line| match line {
                Line::Heading(text) => FontPickerLine {
                    index: -1,
                    label: SharedString::from(*text),
                    tag: SharedString::new(),
                    family: SharedString::new(),
                    heading: true,
                },
                Line::Item(index) => {
                    let row = &picker.rows[*index];
                    FontPickerLine {
                        index: *index as i32,
                        label: SharedString::from(row.label.as_str()),
                        tag: SharedString::from(row.tag),
                        family: SharedString::from(row.preview.as_str()),
                        heading: false,
                    }
                }
            })
            .collect();
        let global = app.global::<FontPicker>();
        global.set_lines(ModelRc::new(VecModel::from(shown)));
        global.set_selected(if picker.rows.is_empty() {
            -1
        } else {
            picker.selected as i32
        });
        global.set_match_count(picker.rows.len() as i32);
        global.set_loading(picker.loading);
        global.set_open(picker.open);
    });
}

/// Update the closed field from the selection, or the pending family at the
/// caret. Called whenever the window's formatting state refreshes.
pub(crate) fn publish_current(app: &WriterApp, document: &WriterDocument) {
    let selection = document.selection();
    let pending = if selection.is_collapsed() {
        typing_style::at(selection.focus).family
    } else {
        None
    };
    let choice = pending.map_or_else(
        || {
            selection_font_family(
                document,
                DocumentSelection::range(selection.anchor, selection.focus),
            )
        },
        FontChoice::Single,
    );
    let global = app.global::<FontPicker>();
    match choice {
        FontChoice::Mixed => {
            global.set_current("Mixed".into());
            global.set_current_note(SharedString::new());
            global.set_current_family(SharedString::new());
        }
        FontChoice::Single(name) => {
            let resolution = fonts::resolve_family(&name);
            let label = if fonts::is_document_font(&name) {
                DEFAULT_FAMILY.to_owned()
            } else {
                name.trim().to_owned()
            };
            global.set_current(SharedString::from(label));
            global.set_current_note(if resolution.standing.is_missing() {
                TAG_MISSING.into()
            } else {
                SharedString::new()
            });
            global.set_current_family(SharedString::from(resolution.drawn));
        }
    }
}

/// The family the field shows for `document`'s selection, as stored.
fn current_family(document: &WriterDocument) -> String {
    let selection = document.selection();
    if selection.is_collapsed() {
        if let Some(family) = typing_style::at(selection.focus).family {
            return family;
        }
    }
    match selection_font_family(
        document,
        DocumentSelection::range(selection.anchor, selection.focus),
    ) {
        FontChoice::Single(name) => name,
        FontChoice::Mixed => String::new(),
    }
}

fn open(app: &WriterApp, state: &GuiState, query: &str) {
    let current = current_family(&state.current.borrow());
    PICKER.with(|picker| {
        let mut picker = picker.borrow_mut();
        picker.open = true;
        picker.query = query.to_owned();
    });
    rebuild(&state.current.borrow());
    if query.is_empty() {
        select_current(&current);
    } else {
        PICKER.with(|picker| {
            let mut picker = picker.borrow_mut();
            picker.selected = 0;
            picker.first = 0;
        });
    }
    app.global::<FontPicker>().set_query(query.into());
    publish(app);
    let global = app.global::<FontPicker>();
    global.set_search_focus_tick(global.get_search_focus_tick() + 1);
}

fn close(app: &WriterApp, refocus_page: bool) {
    PICKER.with(|picker| picker.borrow_mut().open = false);
    publish(app);
    if refocus_page {
        app.invoke_focus_page();
    } else {
        let global = app.global::<FontPicker>();
        global.set_field_focus_tick(global.get_field_focus_tick() + 1);
    }
}

/// True for a key that types a visible character into the search.
fn is_printable(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => {
            !ch.is_control() && !('\u{E000}'..='\u{F8FF}').contains(&ch) && !ch.is_whitespace()
        }
        _ => false,
    }
}

/// Apply `family` to the selection as one undoable edit, or make it the
/// family of the text typed next when nothing is selected.
pub(crate) fn apply_family(app: &WriterApp, state: &Rc<GuiState>, family: &str) {
    let family = family.trim();
    let selection = state.current.borrow().selection();
    if selection.is_collapsed() {
        typing_style::set_family(selection.focus, family);
        let name = if family.is_empty() {
            DEFAULT_FAMILY
        } else {
            family
        };
        app.set_status_right(SharedString::from(format!("{name} for new text")));
    } else {
        let mut next = state.current.borrow().clone();
        set_selection_font_family(
            &mut next,
            DocumentSelection::range(selection.anchor, selection.focus),
            family,
        );
        next.set_selection(selection);
        if next != *state.current.borrow() {
            apply_with_history(app, state, next, crate::HistoryKind::DocumentAction);
        }
        let name = if family.is_empty() {
            DEFAULT_FAMILY
        } else {
            family
        };
        app.set_status_right(SharedString::from(format!("Font: {name}")));
    }
    remember(family);
    publish_current(app, &state.current.borrow());
}

fn remember(family: &str) {
    PICKER.with(|picker| {
        let mut picker = picker.borrow_mut();
        picker.recents = font_store::with_recent(&picker.recents, family);
        if let Some(store) = &picker.store {
            // Recents are a convenience; failing to save them is not an error
            // worth interrupting the user for.
            let _ = store.save_recents(&picker.recents);
        }
    });
}

/// Connect the font field and list to the controller.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let picker = app.global::<FontPicker>();
    {
        let (weak, state) = (app.as_weak(), state.clone());
        picker.on_open_requested(move || {
            if let Some(app) = weak.upgrade() {
                open(&app, &state, "");
            }
        });
    }
    {
        let weak = app.as_weak();
        picker.on_close_requested(move || {
            if let Some(app) = weak.upgrade() {
                close(&app, false);
            }
        });
    }
    {
        let (weak, state) = (app.as_weak(), state.clone());
        picker.on_query_edited(move |text| {
            let Some(app) = weak.upgrade() else { return };
            PICKER.with(|picker| {
                let mut picker = picker.borrow_mut();
                picker.query = text.to_string();
                picker.selected = 0;
                picker.first = 0;
            });
            rebuild(&state.current.borrow());
            publish(&app);
        });
    }
    {
        let weak = app.as_weak();
        picker.on_move(move |delta| {
            let Some(app) = weak.upgrade() else { return };
            PICKER.with(|picker| {
                let mut picker = picker.borrow_mut();
                let count = picker.rows.len();
                if count > 0 {
                    let moved =
                        (picker.selected as i64 + i64::from(delta)).clamp(0, count as i64 - 1);
                    picker.selected = moved as usize;
                }
            });
            publish(&app);
        });
    }
    {
        let (weak, state) = (app.as_weak(), state.clone());
        picker.on_choose(move |index| {
            let Some(app) = weak.upgrade() else { return };
            let value = PICKER.with(|picker| {
                picker
                    .borrow()
                    .rows
                    .get(usize::try_from(index).unwrap_or(usize::MAX))
                    .map(|row| row.value.clone())
            });
            if let Some(value) = value {
                close(&app, true);
                apply_family(&app, &state, &value);
            }
        });
    }
    {
        let (weak, state) = (app.as_weak(), state.clone());
        picker.on_type_ahead(move |text| {
            let Some(app) = weak.upgrade() else { return };
            if is_printable(text.as_str()) {
                open(&app, &state, text.as_str());
            }
        });
    }
}

/// Live-window setup: open the per-user store, load the recent families and
/// start the system font scan on its own thread. The bundled families are
/// listed until it finishes; its result then replaces the catalogue, and the
/// page and the list are refreshed.
pub(crate) fn start_live(app: &WriterApp, state: &Rc<GuiState>, application_id: &str) {
    let store = FontStore::for_application(application_id);
    PICKER.with(|picker| {
        let mut picker = picker.borrow_mut();
        picker.recents = store.load_recents();
        picker.store = Some(store.clone());
        picker.loading = true;
    });
    LIVE.with(|live| *live.borrow_mut() = Some((app.as_weak(), state.clone())));
    font_store::scan_in_background(
        font_store::system_scan_config(),
        Some(store),
        |catalog: Arc<FontCatalog>| {
            // Back on the interface thread, where the window's state lives.
            let _ = slint::invoke_from_event_loop(move || catalogue_ready(catalog));
        },
    );
}

/// Lets a test deliver a catalogue as the scan would, without scanning.
#[cfg(test)]
pub(crate) fn attach_for_test(app: &WriterApp, state: &Rc<GuiState>) {
    LIVE.with(|live| *live.borrow_mut() = Some((app.as_weak(), state.clone())));
}

/// The scan finished (or a test supplied a catalogue): install it and refresh
/// everything that measured or listed fonts.
pub(crate) fn catalogue_ready(catalog: Arc<FontCatalog>) {
    fonts::install_font_catalog(catalog);
    PICKER.with(|picker| picker.borrow_mut().loading = false);
    let live = LIVE.with(|live| live.borrow().clone());
    let Some((weak, state)) = live else { return };
    let Some(app) = weak.upgrade() else { return };
    // The page re-measures with the installed faces.
    crate::apply_state(&app, &state);
    if PICKER.with(|picker| picker.borrow().open) {
        rebuild(&state.current.borrow());
    }
    publish(&app);
}

#[cfg(test)]
#[path = "font_picker_tests.rs"]
mod tests;
