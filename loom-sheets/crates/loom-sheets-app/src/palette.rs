//! Command palette logic for Loom Sheets.

use std::rc::Rc;

use slint::{ComponentHandle, Model, SharedString, VecModel};

use crate::{dispatch_command, CommandPaletteItem, SheetsApp};

/// Commands exposed through the command palette. Invocation dispatches
/// through the same application callbacks as the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    NewSheet,
    OpenSheet,
    SaveSheet,
    SaveAsSheet,
    ExportCsv,
    Undo,
    Redo,
}

/// Route a palette action through the canonical command dispatcher.
pub fn dispatch_palette_action(app: &SheetsApp, action: PaletteAction) -> bool {
    match action {
        PaletteAction::NewSheet => dispatch_command(app, "sheets.new"),
        PaletteAction::OpenSheet => dispatch_command(app, "sheets.open"),
        PaletteAction::SaveSheet => dispatch_command(app, "sheets.save"),
        PaletteAction::SaveAsSheet => dispatch_command(app, "sheets.save-as"),
        PaletteAction::ExportCsv => dispatch_command(app, "sheets.export-csv"),
        PaletteAction::Undo if app.get_can_undo() => dispatch_command(app, "sheets.undo"),
        PaletteAction::Redo if app.get_can_redo() => dispatch_command(app, "sheets.redo"),
        PaletteAction::Undo | PaletteAction::Redo => false,
    }
}

/// Resolve a rendered palette row back to the canonical action.
pub fn palette_action_for_id(id: &str) -> Option<PaletteAction> {
    match id {
        "sheets.new" => Some(PaletteAction::NewSheet),
        "sheets.open" => Some(PaletteAction::OpenSheet),
        "sheets.save" => Some(PaletteAction::SaveSheet),
        "sheets.save-as" => Some(PaletteAction::SaveAsSheet),
        "sheets.export-csv" => Some(PaletteAction::ExportCsv),
        "sheets.undo" => Some(PaletteAction::Undo),
        "sheets.redo" => Some(PaletteAction::Redo),
        _ => None,
    }
}

pub struct PaletteCommand {
    pub action: PaletteAction,
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
}

pub fn master_palette(app: &SheetsApp) -> Vec<PaletteCommand> {
    [
        (PaletteAction::NewSheet, "sheets.new", "New Sheet", "Ctrl+N"),
        (
            PaletteAction::OpenSheet,
            "sheets.open",
            "Open Sheet",
            "Ctrl+O",
        ),
        (
            PaletteAction::SaveSheet,
            "sheets.save",
            "Save Sheet",
            "Ctrl+S",
        ),
        (
            PaletteAction::SaveAsSheet,
            "sheets.save-as",
            "Save Sheet As",
            "Ctrl+Shift+S",
        ),
        (
            PaletteAction::ExportCsv,
            "sheets.export-csv",
            "Export CSV",
            "Ctrl+E",
        ),
        (PaletteAction::Undo, "sheets.undo", "Undo", "Ctrl+Z"),
        (PaletteAction::Redo, "sheets.redo", "Redo", "Ctrl+Shift+Z"),
    ]
    .into_iter()
    .map(|(action, id, label, shortcut)| PaletteCommand {
        action,
        id,
        label,
        shortcut,
    })
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

pub fn rebuild_palette(app: &SheetsApp, query: &str) {
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

pub fn wire_palette(app: &SheetsApp) {
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
                let Some(action) = palette_action_for_id(item.id.as_str()) else {
                    return;
                };
                if dispatch_palette_action(&app, action) {
                    app.set_palette_open(false);
                }
            }
        });
    }
}
