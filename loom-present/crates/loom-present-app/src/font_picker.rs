//! Font picker controller for the Present inspector.
//!
//! Uses `loom_fonts::FontCatalog::picker_families()` (which filters `.`-prefixed
//! hidden macOS system fonts), supports `"Theme font"` reset, `"Mixed"` display
//! when selected elements disagree, search filtering, and keyboard navigation.

use std::cell::RefCell;
use std::rc::Rc;

use loom_fonts::FontCatalog;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{FontPicker, FontPickerLine, PresentApp};

pub(crate) const THEME_FONT_LABEL: &str = "Theme font";
pub(crate) const MIXED_LABEL: &str = "Mixed";
const MAX_TYPED_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Theme,
    InDeck,
    AllFonts,
    Typed,
}

impl Section {
    fn heading(self) -> Option<&'static str> {
        match self {
            Self::Theme => None,
            Self::InDeck => Some("In this deck"),
            Self::AllFonts => Some("All fonts"),
            Self::Typed => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// Stored value (`""` for Theme font reset).
    pub value: String,
    pub label: String,
    pub tag: &'static str,
    pub preview: String,
    pub section: Section,
}

pub(crate) struct Inputs<'a> {
    pub in_deck: &'a [String],
    pub installed: &'a [String],
    pub query: &'a str,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

pub(crate) fn build_rows(inputs: &Inputs<'_>) -> Vec<Row> {
    let q = key(inputs.query);
    let mut rows = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    if q.is_empty() || THEME_FONT_LABEL.to_lowercase().contains(&q) {
        rows.push(Row {
            value: String::new(),
            label: THEME_FONT_LABEL.to_owned(),
            tag: "Reset",
            preview: String::new(),
            section: Section::Theme,
        });
    }

    for name in inputs.in_deck {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.starts_with('.') {
            continue;
        }
        let k = key(trimmed);
        if seen.contains(&k) {
            continue;
        }
        if !q.is_empty() && !k.contains(&q) {
            continue;
        }
        seen.push(k);
        rows.push(Row {
            value: trimmed.to_owned(),
            label: trimmed.to_owned(),
            tag: "In deck",
            preview: trimmed.to_owned(),
            section: Section::InDeck,
        });
    }

    for name in inputs.installed {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.starts_with('.') {
            continue;
        }
        let k = key(trimmed);
        if seen.contains(&k) {
            continue;
        }
        if !q.is_empty() && !k.contains(&q) {
            continue;
        }
        seen.push(k);
        rows.push(Row {
            value: trimmed.to_owned(),
            label: trimmed.to_owned(),
            tag: "",
            preview: trimmed.to_owned(),
            section: Section::AllFonts,
        });
    }

    let typed = inputs.query.trim();
    if !typed.is_empty() && typed.chars().count() <= MAX_TYPED_CHARS {
        let k = key(typed);
        if !seen.contains(&k) {
            rows.push(Row {
                value: typed.to_owned(),
                label: format!("Use \"{typed}\""),
                tag: "Custom",
                preview: typed.to_owned(),
                section: Section::Typed,
            });
        }
    }

    rows
}

pub(crate) fn to_lines(rows: &[Row]) -> Vec<FontPickerLine> {
    let mut out = Vec::with_capacity(rows.len() + 3);
    let mut last_section: Option<Section> = None;
    for (idx, row) in rows.iter().enumerate() {
        if last_section != Some(row.section) {
            if let Some(heading) = row.section.heading() {
                out.push(FontPickerLine {
                    index: -1,
                    label: SharedString::from(heading),
                    tag: SharedString::default(),
                    family: SharedString::default(),
                    heading: true,
                });
            }
            last_section = Some(row.section);
        }
        out.push(FontPickerLine {
            index: idx as i32,
            label: SharedString::from(row.label.as_str()),
            tag: SharedString::from(row.tag),
            family: SharedString::from(row.preview.as_str()),
            heading: false,
        });
    }
    out
}

pub(crate) fn line_for_row(lines: &[FontPickerLine], row_index: i32) -> i32 {
    lines
        .iter()
        .position(|line| !line.heading && line.index == row_index)
        .map_or(-1, |pos| pos as i32)
}

#[derive(Default)]
pub(crate) struct PickerState {
    pub installed: Vec<String>,
    pub in_deck: Vec<String>,
    pub rows: Vec<Row>,
    pub lines: Vec<FontPickerLine>,
    pub selected: i32,
}

impl PickerState {
    pub fn load_system_fonts(&mut self) {
        if self.installed.is_empty() {
            self.installed = FontCatalog::system().picker_families();
        }
    }

    pub fn rebuild(&mut self, query: &str) {
        self.load_system_fonts();
        self.rows = build_rows(&Inputs {
            in_deck: &self.in_deck,
            installed: &self.installed,
            query,
        });
        self.lines = to_lines(&self.rows);
        self.selected = if self.rows.is_empty() { -1 } else { 0 };
    }
}

pub(crate) fn publish_current(app: &PresentApp, family: Option<&str>, mixed: bool) {
    let picker = app.global::<FontPicker>();
    if mixed {
        picker.set_current(MIXED_LABEL.into());
        picker.set_current_family(SharedString::default());
        picker.set_current_note(SharedString::default());
    } else if let Some(name) = family.map(str::trim).filter(|s| !s.is_empty()) {
        picker.set_current(SharedString::from(name));
        picker.set_current_family(SharedString::from(name));
        picker.set_current_note(SharedString::default());
    } else {
        picker.set_current(THEME_FONT_LABEL.into());
        picker.set_current_family(SharedString::default());
        picker.set_current_note(SharedString::default());
    }
}

pub(crate) fn wire(
    app: &PresentApp,
    on_choose: impl Fn(&str) + 'static,
) {
    let state = Rc::new(RefCell::new(PickerState::default()));
    let on_choose = Rc::new(on_choose);

    {
        let weak = app.as_weak();
        let state = Rc::clone(&state);
        app.global::<FontPicker>().on_open_requested(move || {
            let Some(app) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            st.rebuild("");
            let picker = app.global::<FontPicker>();
            picker.set_query(SharedString::default());
            picker.set_lines(ModelRc::new(VecModel::from(st.lines.clone())));
            picker.set_match_count(st.rows.len() as i32);
            picker.set_selected(st.selected);
            picker.set_selected_line(line_for_row(&st.lines, st.selected));
            picker.set_open(true);
            picker.set_search_focus_tick(picker.get_search_focus_tick() + 1);
        });
    }

    {
        let weak = app.as_weak();
        app.global::<FontPicker>().on_close_requested(move || {
            let Some(app) = weak.upgrade() else { return };
            let picker = app.global::<FontPicker>();
            picker.set_open(false);
            picker.set_field_focus_tick(picker.get_field_focus_tick() + 1);
        });
    }

    {
        let weak = app.as_weak();
        let state = Rc::clone(&state);
        app.global::<FontPicker>().on_query_edited(move |q| {
            let Some(app) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            st.rebuild(q.as_str());
            let picker = app.global::<FontPicker>();
            picker.set_query(q);
            picker.set_lines(ModelRc::new(VecModel::from(st.lines.clone())));
            picker.set_match_count(st.rows.len() as i32);
            picker.set_selected(st.selected);
            picker.set_selected_line(line_for_row(&st.lines, st.selected));
        });
    }

    {
        let weak = app.as_weak();
        let state = Rc::clone(&state);
        app.global::<FontPicker>().on_move(move |delta| {
            let Some(app) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            if st.rows.is_empty() {
                return;
            }
            let len = st.rows.len() as i32;
            let next = (st.selected + delta).clamp(0, len - 1);
            st.selected = next;
            let picker = app.global::<FontPicker>();
            picker.set_selected(next);
            picker.set_selected_line(line_for_row(&st.lines, next));
        });
    }

    {
        let weak = app.as_weak();
        let state = Rc::clone(&state);
        let on_choose = Rc::clone(&on_choose);
        app.global::<FontPicker>().on_choose(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let value = {
                let st = state.borrow();
                st.rows.get(idx as usize).map(|r| r.value.clone())
            };
            if let Some(val) = value {
                let picker = app.global::<FontPicker>();
                picker.set_open(false);
                picker.set_field_focus_tick(picker.get_field_focus_tick() + 1);
                on_choose(&val);
            }
        });
    }

    {
        let weak = app.as_weak();
        let state = Rc::clone(&state);
        app.global::<FontPicker>().on_type_ahead(move |text| {
            let Some(app) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            st.rebuild(text.as_str());
            let picker = app.global::<FontPicker>();
            picker.set_query(text);
            picker.set_lines(ModelRc::new(VecModel::from(st.lines.clone())));
            picker.set_match_count(st.rows.len() as i32);
            picker.set_selected(st.selected);
            picker.set_selected_line(line_for_row(&st.lines, st.selected));
            picker.set_open(true);
            picker.set_search_focus_tick(picker.get_search_focus_tick() + 1);
        });
    }
}
