//! Command palette logic for Loom Sheets.

use std::rc::Rc;

use slint::{ComponentHandle, Model, SharedString, VecModel};

use crate::{dispatch_command, CommandPaletteItem, SheetsApp};

/// Commands exposed through the command palette. Invocation dispatches
/// through the same application callbacks as the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    NewSheet,
    NewFromTemplate,
    OpenSheet,
    SaveSheet,
    SaveAsSheet,
    ExportCsv,
    ExportXlsx,
    Cut,
    Copy,
    Paste,
    SelectAll,
    AddRow,
    DeleteRow,
    AddCol,
    DeleteCol,
    SortAsc,
    SortDesc,
    FreezeHeader,
    UnfreezePanes,
    DeleteSheet,
    Bold,
    Italic,
    Underline,
    AlignLeft,
    AlignCenter,
    AlignRight,
    FormatCurrency,
    FormatPercent,
    FormatGeneral,
    FormatNumber,
    DecimalsIncrease,
    DecimalsDecrease,
    Organize,
    FillDown,
    InsertChart,
    InsertShape,
    InsertImage,
    CycleChartKind,
    Borders,
    FillCycle,
    FontIncrease,
    FontDecrease,
    PivotSum,
    PivotCount,
    PivotAverage,
    PivotMin,
    PivotMax,
    Undo,
    Redo,
}

/// Route a palette action through the canonical command dispatcher.
pub fn dispatch_palette_action(app: &SheetsApp, action: PaletteAction) -> bool {
    match action {
        PaletteAction::NewSheet => dispatch_command(app, "sheets.new"),
        PaletteAction::NewFromTemplate => dispatch_command(app, "sheets.new-template"),
        PaletteAction::OpenSheet => dispatch_command(app, "sheets.open"),
        PaletteAction::SaveSheet => dispatch_command(app, "sheets.save"),
        PaletteAction::SaveAsSheet => dispatch_command(app, "sheets.save-as"),
        PaletteAction::ExportCsv => dispatch_command(app, "sheets.export-csv"),
        PaletteAction::ExportXlsx => dispatch_command(app, "sheets.export-xlsx"),
        PaletteAction::Cut => dispatch_command(app, "sheets.cut"),
        PaletteAction::Copy => dispatch_command(app, "sheets.copy"),
        PaletteAction::Paste => dispatch_command(app, "sheets.paste"),
        PaletteAction::SelectAll => dispatch_command(app, "sheets.select-all"),
        PaletteAction::AddRow => dispatch_command(app, "table.add_row"),
        PaletteAction::DeleteRow => dispatch_command(app, "table.delete_row"),
        PaletteAction::AddCol => dispatch_command(app, "table.add_col"),
        PaletteAction::DeleteCol => dispatch_command(app, "table.delete_col"),
        PaletteAction::SortAsc => dispatch_command(app, "table.sort_asc"),
        PaletteAction::SortDesc => dispatch_command(app, "table.sort_desc"),
        PaletteAction::FreezeHeader => dispatch_command(app, "table.freeze_header"),
        PaletteAction::UnfreezePanes => dispatch_command(app, "table.unfreeze_panes"),
        PaletteAction::DeleteSheet => dispatch_command(app, "sheets.delete_sheet"),
        PaletteAction::Bold => dispatch_command(app, "sheets.bold"),
        PaletteAction::Italic => dispatch_command(app, "sheets.italic"),
        PaletteAction::Underline => dispatch_command(app, "sheets.underline"),
        PaletteAction::AlignLeft => dispatch_command(app, "sheets.align-left"),
        PaletteAction::AlignCenter => dispatch_command(app, "sheets.align-center"),
        PaletteAction::AlignRight => dispatch_command(app, "sheets.align-right"),
        PaletteAction::FormatCurrency => dispatch_command(app, "format.currency"),
        PaletteAction::FormatPercent => dispatch_command(app, "format.percentage"),
        PaletteAction::FormatGeneral => dispatch_command(app, "format.general"),
        PaletteAction::FormatNumber => dispatch_command(app, "format.number"),
        PaletteAction::DecimalsIncrease => dispatch_command(app, "table.adjust_decimals_increase"),
        PaletteAction::DecimalsDecrease => dispatch_command(app, "table.adjust_decimals_decrease"),
        PaletteAction::Organize => dispatch_command(app, "sheets.organize"),
        PaletteAction::FillDown => dispatch_command(app, "table.fill_down"),
        PaletteAction::InsertChart => dispatch_command(app, "sheets.insert_chart"),
        PaletteAction::InsertShape => dispatch_command(app, "sheets.insert_shape"),
        PaletteAction::InsertImage => dispatch_command(app, "sheets.insert_image"),
        PaletteAction::CycleChartKind => dispatch_command(app, "sheets.cycle_chart_kind"),
        PaletteAction::Borders => dispatch_command(app, "format.borders"),
        PaletteAction::FillCycle => dispatch_command(app, "format.fill_cycle"),
        PaletteAction::FontIncrease => dispatch_command(app, "format.font_increase"),
        PaletteAction::FontDecrease => dispatch_command(app, "format.font_decrease"),
        PaletteAction::PivotSum => dispatch_command(app, "table.pivot_sum"),
        PaletteAction::PivotCount => dispatch_command(app, "table.pivot_count"),
        PaletteAction::PivotAverage => dispatch_command(app, "table.pivot_average"),
        PaletteAction::PivotMin => dispatch_command(app, "table.pivot_min"),
        PaletteAction::PivotMax => dispatch_command(app, "table.pivot_max"),
        PaletteAction::Undo if app.get_can_undo() => dispatch_command(app, "sheets.undo"),
        PaletteAction::Redo if app.get_can_redo() => dispatch_command(app, "sheets.redo"),
        PaletteAction::Undo | PaletteAction::Redo => false,
    }
}

/// Resolve a rendered palette row back to the canonical action.
pub fn palette_action_for_id(id: &str) -> Option<PaletteAction> {
    match id {
        "sheets.new" => Some(PaletteAction::NewSheet),
        "sheets.new-template" => Some(PaletteAction::NewFromTemplate),
        "sheets.open" => Some(PaletteAction::OpenSheet),
        "sheets.save" => Some(PaletteAction::SaveSheet),
        "sheets.save-as" => Some(PaletteAction::SaveAsSheet),
        "sheets.export-csv" => Some(PaletteAction::ExportCsv),
        "sheets.export-xlsx" => Some(PaletteAction::ExportXlsx),
        "sheets.cut" => Some(PaletteAction::Cut),
        "sheets.copy" => Some(PaletteAction::Copy),
        "sheets.paste" => Some(PaletteAction::Paste),
        "sheets.select-all" => Some(PaletteAction::SelectAll),
        "table.add_row" => Some(PaletteAction::AddRow),
        "table.delete_row" => Some(PaletteAction::DeleteRow),
        "table.add_col" => Some(PaletteAction::AddCol),
        "table.delete_col" => Some(PaletteAction::DeleteCol),
        "table.sort_asc" => Some(PaletteAction::SortAsc),
        "table.sort_desc" => Some(PaletteAction::SortDesc),
        "table.freeze_header" => Some(PaletteAction::FreezeHeader),
        "table.unfreeze_panes" => Some(PaletteAction::UnfreezePanes),
        "sheets.delete_sheet" => Some(PaletteAction::DeleteSheet),
        "sheets.bold" => Some(PaletteAction::Bold),
        "sheets.italic" => Some(PaletteAction::Italic),
        "sheets.underline" => Some(PaletteAction::Underline),
        "sheets.align-left" => Some(PaletteAction::AlignLeft),
        "sheets.align-center" => Some(PaletteAction::AlignCenter),
        "sheets.align-right" => Some(PaletteAction::AlignRight),
        "format.currency" => Some(PaletteAction::FormatCurrency),
        "format.percentage" => Some(PaletteAction::FormatPercent),
        "format.general" => Some(PaletteAction::FormatGeneral),
        "format.number" => Some(PaletteAction::FormatNumber),
        "table.adjust_decimals_increase" => Some(PaletteAction::DecimalsIncrease),
        "table.adjust_decimals_decrease" => Some(PaletteAction::DecimalsDecrease),
        "sheets.organize" => Some(PaletteAction::Organize),
        "table.fill_down" => Some(PaletteAction::FillDown),
        "sheets.insert_chart" => Some(PaletteAction::InsertChart),
        "sheets.insert_shape" => Some(PaletteAction::InsertShape),
        "sheets.insert_image" => Some(PaletteAction::InsertImage),
        "sheets.cycle_chart_kind" => Some(PaletteAction::CycleChartKind),
        "format.borders" => Some(PaletteAction::Borders),
        "format.fill_cycle" => Some(PaletteAction::FillCycle),
        "format.font_increase" => Some(PaletteAction::FontIncrease),
        "format.font_decrease" => Some(PaletteAction::FontDecrease),
        "table.pivot_sum" => Some(PaletteAction::PivotSum),
        "table.pivot_count" => Some(PaletteAction::PivotCount),
        "table.pivot_average" => Some(PaletteAction::PivotAverage),
        "table.pivot_min" => Some(PaletteAction::PivotMin),
        "table.pivot_max" => Some(PaletteAction::PivotMax),
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
        (
            PaletteAction::NewSheet,
            "sheets.new",
            "New Workbook",
            "Ctrl+N",
        ),
        (
            PaletteAction::NewFromTemplate,
            "sheets.new-template",
            "New from Template...",
            "",
        ),
        (
            PaletteAction::OpenSheet,
            "sheets.open",
            "Open Workbook",
            "Ctrl+O",
        ),
        (
            PaletteAction::SaveSheet,
            "sheets.save",
            "Save Workbook",
            "Ctrl+S",
        ),
        (
            PaletteAction::SaveAsSheet,
            "sheets.save-as",
            "Save Workbook As",
            "Ctrl+Shift+S",
        ),
        (
            PaletteAction::ExportCsv,
            "sheets.export-csv",
            "Export CSV",
            "Ctrl+E",
        ),
        (
            PaletteAction::ExportXlsx,
            "sheets.export-xlsx",
            "Export Excel (.xlsx)",
            "",
        ),
        (PaletteAction::Cut, "sheets.cut", "Cut", "Ctrl+X"),
        (PaletteAction::Copy, "sheets.copy", "Copy", "Ctrl+C"),
        (PaletteAction::Paste, "sheets.paste", "Paste", "Ctrl+V"),
        (
            PaletteAction::SelectAll,
            "sheets.select-all",
            "Select All",
            "Ctrl+A",
        ),
        (PaletteAction::AddRow, "table.add_row", "Add Row", ""),
        (
            PaletteAction::DeleteRow,
            "table.delete_row",
            "Delete Row",
            "",
        ),
        (PaletteAction::AddCol, "table.add_col", "Add Column", ""),
        (
            PaletteAction::DeleteCol,
            "table.delete_col",
            "Delete Column",
            "",
        ),
        (
            PaletteAction::SortAsc,
            "table.sort_asc",
            "Sort Ascending",
            "",
        ),
        (
            PaletteAction::SortDesc,
            "table.sort_desc",
            "Sort Descending",
            "",
        ),
        (
            PaletteAction::FreezeHeader,
            "table.freeze_header",
            "Freeze Header Row",
            "",
        ),
        (
            PaletteAction::UnfreezePanes,
            "table.unfreeze_panes",
            "Unfreeze Panes",
            "",
        ),
        (
            PaletteAction::DeleteSheet,
            "sheets.delete_sheet",
            "Delete Sheet",
            "",
        ),
        (PaletteAction::Bold, "sheets.bold", "Bold", "Ctrl+B"),
        (PaletteAction::Italic, "sheets.italic", "Italic", "Ctrl+I"),
        (
            PaletteAction::Underline,
            "sheets.underline",
            "Underline",
            "Ctrl+U",
        ),
        (
            PaletteAction::AlignLeft,
            "sheets.align-left",
            "Align Left",
            "",
        ),
        (
            PaletteAction::AlignCenter,
            "sheets.align-center",
            "Align Center",
            "",
        ),
        (
            PaletteAction::AlignRight,
            "sheets.align-right",
            "Align Right",
            "",
        ),
        (
            PaletteAction::FormatCurrency,
            "format.currency",
            "Format as Currency ($)",
            "",
        ),
        (
            PaletteAction::FormatPercent,
            "format.percentage",
            "Format as Percentage (%)",
            "",
        ),
        (
            PaletteAction::FormatGeneral,
            "format.general",
            "Format as General",
            "",
        ),
        (
            PaletteAction::FormatNumber,
            "format.number",
            "Format as Number (1.23)",
            "",
        ),
        (
            PaletteAction::DecimalsIncrease,
            "table.adjust_decimals_increase",
            "Decimals: Increase",
            "",
        ),
        (
            PaletteAction::DecimalsDecrease,
            "table.adjust_decimals_decrease",
            "Decimals: Decrease",
            "",
        ),
        (
            PaletteAction::Organize,
            "sheets.organize",
            "Sort by This Column (A→Z)",
            "",
        ),
        (PaletteAction::FillDown, "table.fill_down", "Fill Down", ""),
        (
            PaletteAction::InsertChart,
            "sheets.insert_chart",
            "Insert Chart",
            "",
        ),
        (
            PaletteAction::InsertShape,
            "sheets.insert_shape",
            "Insert Shape",
            "",
        ),
        (
            PaletteAction::InsertImage,
            "sheets.insert_image",
            "Insert Image",
            "",
        ),
        (
            PaletteAction::CycleChartKind,
            "sheets.cycle_chart_kind",
            "Cycle Chart Kind",
            "",
        ),
        (
            PaletteAction::Borders,
            "format.borders",
            "Borders: Toggle",
            "",
        ),
        (
            PaletteAction::FillCycle,
            "format.fill_cycle",
            "Fill: Cycle Color",
            "",
        ),
        (
            PaletteAction::FontIncrease,
            "format.font_increase",
            "Font Size: Increase",
            "",
        ),
        (
            PaletteAction::FontDecrease,
            "format.font_decrease",
            "Font Size: Decrease",
            "",
        ),
        (
            PaletteAction::PivotSum,
            "table.pivot_sum",
            "Pivot Summary: Sum",
            "",
        ),
        (
            PaletteAction::PivotCount,
            "table.pivot_count",
            "Pivot Summary: Count",
            "",
        ),
        (
            PaletteAction::PivotAverage,
            "table.pivot_average",
            "Pivot Summary: Average",
            "",
        ),
        (
            PaletteAction::PivotMin,
            "table.pivot_min",
            "Pivot Summary: Min",
            "",
        ),
        (
            PaletteAction::PivotMax,
            "table.pivot_max",
            "Pivot Summary: Max",
            "",
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
