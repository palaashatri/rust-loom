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
    RetryRecovery,
    ExportCsv,
    ExportXlsx,
    ExportPdf,
    BringForward,
    SendBackward,
    BringToFront,
    SendToBack,
    Cut,
    Copy,
    Paste,
    PasteValues,
    PasteFormats,
    SelectAll,
    Find,
    Replace,
    ToggleAutoFilter,
    FilterColumn,
    ClearAutoFilter,
    NamedRanges,
    DataValidation,
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
    CopyChart,
    WorksheetObjects,
    Borders,
    BordersAll,
    BordersOuter,
    BordersTop,
    BordersBottom,
    BordersLeft,
    BordersRight,
    BordersNone,
    Strikethrough,
    WrapText,
    TextColorCycle,
    TextColorAuto,
    MergeCells,
    UnmergeCells,
    ConditionalFormatting,
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
    WelcomeGuide,
    Appearance(loom_desktop::Appearance),
    WindowFrame(loom_desktop::window_frame::WindowFrame),
}

/// Route a palette action through the canonical command dispatcher.
pub fn dispatch_palette_action(app: &SheetsApp, action: PaletteAction) -> bool {
    match action {
        PaletteAction::NewSheet => dispatch_command(app, "sheets.new"),
        PaletteAction::NewFromTemplate => dispatch_command(app, "sheets.new-template"),
        PaletteAction::OpenSheet => dispatch_command(app, "sheets.open"),
        PaletteAction::SaveSheet => dispatch_command(app, "sheets.save"),
        PaletteAction::SaveAsSheet => dispatch_command(app, "sheets.save-as"),
        PaletteAction::RetryRecovery => dispatch_command(app, "sheets.retry-recovery"),
        PaletteAction::ExportCsv => dispatch_command(app, "sheets.export-csv"),
        PaletteAction::ExportXlsx => dispatch_command(app, "sheets.export-xlsx"),
        PaletteAction::ExportPdf => dispatch_command(app, "file.export_pdf"),
        PaletteAction::BringForward => dispatch_command(app, "arrange.bring_forward"),
        PaletteAction::SendBackward => dispatch_command(app, "arrange.send_backward"),
        PaletteAction::BringToFront => dispatch_command(app, "arrange.bring_to_front"),
        PaletteAction::SendToBack => dispatch_command(app, "arrange.send_to_back"),
        PaletteAction::Cut => dispatch_command(app, "sheets.cut"),
        PaletteAction::Copy => dispatch_command(app, "sheets.copy"),
        PaletteAction::Paste => dispatch_command(app, "sheets.paste"),
        PaletteAction::PasteValues => dispatch_command(app, "edit.paste_values"),
        PaletteAction::PasteFormats => dispatch_command(app, "edit.paste_formats"),
        PaletteAction::SelectAll => dispatch_command(app, "sheets.select-all"),
        PaletteAction::Find => dispatch_command(app, "edit.find"),
        PaletteAction::Replace => dispatch_command(app, "edit.replace"),
        PaletteAction::ToggleAutoFilter => dispatch_command(app, "data.toggle_filter"),
        PaletteAction::FilterColumn => dispatch_command(app, "data.filter_column"),
        PaletteAction::ClearAutoFilter => dispatch_command(app, "data.clear_filter"),
        PaletteAction::NamedRanges => dispatch_command(app, "data.named_ranges"),
        PaletteAction::DataValidation => dispatch_command(app, "data.validation"),
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
        PaletteAction::CopyChart => dispatch_command(app, "sheets.copy_chart"),
        PaletteAction::WorksheetObjects => dispatch_command(app, "sheets.worksheet-objects"),
        PaletteAction::Borders => dispatch_command(app, "format.borders"),
        PaletteAction::BordersAll => dispatch_command(app, "format.borders_all"),
        PaletteAction::BordersOuter => dispatch_command(app, "format.borders_outer"),
        PaletteAction::BordersTop => dispatch_command(app, "format.borders_top"),
        PaletteAction::BordersBottom => dispatch_command(app, "format.borders_bottom"),
        PaletteAction::BordersLeft => dispatch_command(app, "format.borders_left"),
        PaletteAction::BordersRight => dispatch_command(app, "format.borders_right"),
        PaletteAction::BordersNone => dispatch_command(app, "format.borders_none"),
        PaletteAction::Strikethrough => dispatch_command(app, "format.strikethrough"),
        PaletteAction::WrapText => dispatch_command(app, "format.wrap_text"),
        PaletteAction::TextColorCycle => dispatch_command(app, "format.text_color_cycle"),
        PaletteAction::TextColorAuto => dispatch_command(app, "format.text_color_auto"),
        PaletteAction::MergeCells => dispatch_command(app, "format.merge"),
        PaletteAction::UnmergeCells => dispatch_command(app, "format.unmerge"),
        PaletteAction::ConditionalFormatting => {
            dispatch_command(app, "format.conditional_formatting")
        }
        PaletteAction::FillCycle => dispatch_command(app, "format.fill_cycle"),
        PaletteAction::FontIncrease => dispatch_command(app, "format.font_increase"),
        PaletteAction::FontDecrease => dispatch_command(app, "format.font_decrease"),
        PaletteAction::PivotSum => dispatch_command(app, "table.pivot_sum"),
        PaletteAction::PivotCount => dispatch_command(app, "table.pivot_count"),
        PaletteAction::PivotAverage => dispatch_command(app, "table.pivot_average"),
        PaletteAction::PivotMin => dispatch_command(app, "table.pivot_min"),
        PaletteAction::PivotMax => dispatch_command(app, "table.pivot_max"),
        PaletteAction::Appearance(choice) => dispatch_command(app, choice.command_id()),
        PaletteAction::WindowFrame(choice) => dispatch_command(app, choice.command_id()),
        PaletteAction::WelcomeGuide => dispatch_command(app, "sheets.welcome"),
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
        "sheets.retry-recovery" => Some(PaletteAction::RetryRecovery),
        "sheets.export-csv" => Some(PaletteAction::ExportCsv),
        "sheets.export-xlsx" => Some(PaletteAction::ExportXlsx),
        "file.export_pdf" | "sheets.export-pdf" => Some(PaletteAction::ExportPdf),
        "arrange.bring_forward" => Some(PaletteAction::BringForward),
        "arrange.send_backward" => Some(PaletteAction::SendBackward),
        "arrange.bring_to_front" => Some(PaletteAction::BringToFront),
        "arrange.send_to_back" => Some(PaletteAction::SendToBack),
        "sheets.cut" => Some(PaletteAction::Cut),
        "sheets.copy" => Some(PaletteAction::Copy),
        "sheets.paste" => Some(PaletteAction::Paste),
        "edit.paste_values" => Some(PaletteAction::PasteValues),
        "edit.paste_formats" => Some(PaletteAction::PasteFormats),
        "sheets.select-all" => Some(PaletteAction::SelectAll),
        "edit.find" => Some(PaletteAction::Find),
        "edit.replace" => Some(PaletteAction::Replace),
        "data.toggle_filter" => Some(PaletteAction::ToggleAutoFilter),
        "data.filter_column" => Some(PaletteAction::FilterColumn),
        "data.clear_filter" => Some(PaletteAction::ClearAutoFilter),
        "data.named_ranges" => Some(PaletteAction::NamedRanges),
        "data.validation" => Some(PaletteAction::DataValidation),
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
        "sheets.copy_chart" => Some(PaletteAction::CopyChart),
        "sheets.worksheet-objects" => Some(PaletteAction::WorksheetObjects),
        "format.borders" => Some(PaletteAction::Borders),
        "format.borders_all" => Some(PaletteAction::BordersAll),
        "format.borders_outer" => Some(PaletteAction::BordersOuter),
        "format.borders_top" => Some(PaletteAction::BordersTop),
        "format.borders_bottom" => Some(PaletteAction::BordersBottom),
        "format.borders_left" => Some(PaletteAction::BordersLeft),
        "format.borders_right" => Some(PaletteAction::BordersRight),
        "format.borders_none" => Some(PaletteAction::BordersNone),
        "format.strikethrough" => Some(PaletteAction::Strikethrough),
        "format.wrap_text" => Some(PaletteAction::WrapText),
        "format.text_color_cycle" => Some(PaletteAction::TextColorCycle),
        "format.text_color_auto" => Some(PaletteAction::TextColorAuto),
        "format.merge" => Some(PaletteAction::MergeCells),
        "format.unmerge" => Some(PaletteAction::UnmergeCells),
        "format.conditional_formatting" => Some(PaletteAction::ConditionalFormatting),
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
        "sheets.welcome" => Some(PaletteAction::WelcomeGuide),
        other => loom_desktop::Appearance::from_command_id(other)
            .map(PaletteAction::Appearance)
            .or_else(|| {
                loom_desktop::window_frame::WindowFrame::from_command_id(other)
                    .map(PaletteAction::WindowFrame)
            }),
    }
}

pub struct PaletteCommand {
    pub action: PaletteAction,
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
    /// Other words people type for this command, such as file format names.
    pub keywords: &'static str,
}

/// Search words for commands whose label does not name the way people look
/// for them.
fn keywords_for(id: &str) -> &'static str {
    match id {
        "sheets.export-csv" => "csv comma separated values spreadsheet",
        "sheets.export-xlsx" => "xlsx excel spreadsheet office",
        "file.export_pdf" | "sheets.export-pdf" => "pdf print export portable document",
        "arrange.bring_forward"
        | "arrange.send_backward"
        | "arrange.bring_to_front"
        | "arrange.send_to_back" => "arrange z-order layer object shape image",
        "sheets.open" => "xlsx csv excel file",
        "sheets.welcome" => "welcome guide tour tutorial help onboarding getting started",
        id if loom_desktop::window_frame::WindowFrame::from_command_id(id).is_some() => {
            "window frame title bar chrome border"
        }
        _ => "",
    }
}

/// True when the query matches the command: the whole phrase appears in one of
/// its fields, or every word of the query appears in some field.
pub fn command_matches(command: &PaletteCommand, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let fields = [
        command.label,
        command.id,
        command.keywords,
        command.shortcut,
    ]
    .map(|field| field.to_lowercase());
    fields.iter().any(|field| field.contains(&query))
        || query
            .split_whitespace()
            .all(|word| fields.iter().any(|field| field.contains(word)))
}

pub fn master_palette(app: &SheetsApp) -> Vec<PaletteCommand> {
    [
        (PaletteAction::NewSheet, "sheets.new", "New Workbook"),
        (
            PaletteAction::NewFromTemplate,
            "sheets.new-template",
            "New from Template...",
        ),
        (PaletteAction::OpenSheet, "sheets.open", "Open Workbook"),
        (PaletteAction::SaveSheet, "sheets.save", "Save Workbook"),
        (
            PaletteAction::SaveAsSheet,
            "sheets.save-as",
            "Save Workbook As",
        ),
        (
            PaletteAction::RetryRecovery,
            "sheets.retry-recovery",
            "Retry Recovery",
        ),
        (PaletteAction::ExportCsv, "sheets.export-csv", "Export CSV"),
        (
            PaletteAction::ExportXlsx,
            "sheets.export-xlsx",
            "Export Excel (.xlsx)",
        ),
        (
            PaletteAction::ExportPdf,
            "file.export_pdf",
            "Export to PDF / Print",
        ),
        (
            PaletteAction::BringForward,
            "arrange.bring_forward",
            "Arrange: Bring Forward",
        ),
        (
            PaletteAction::SendBackward,
            "arrange.send_backward",
            "Arrange: Send Backward",
        ),
        (
            PaletteAction::BringToFront,
            "arrange.bring_to_front",
            "Arrange: Bring to Front",
        ),
        (
            PaletteAction::SendToBack,
            "arrange.send_to_back",
            "Arrange: Send to Back",
        ),
        (PaletteAction::Cut, "sheets.cut", "Cut"),
        (PaletteAction::Copy, "sheets.copy", "Copy"),
        (PaletteAction::Paste, "sheets.paste", "Paste"),
        (PaletteAction::SelectAll, "sheets.select-all", "Select All"),
        (PaletteAction::Find, "edit.find", "Find..."),
        (PaletteAction::Replace, "edit.replace", "Find & Replace..."),
        (
            PaletteAction::ToggleAutoFilter,
            "data.toggle_filter",
            "Toggle AutoFilter",
        ),
        (
            PaletteAction::FilterColumn,
            "data.filter_column",
            "Filter Column by Value...",
        ),
        (
            PaletteAction::ClearAutoFilter,
            "data.clear_filter",
            "Clear All Column Filters",
        ),
        (
            PaletteAction::NamedRanges,
            "data.named_ranges",
            "Manage Named Ranges...",
        ),
        (
            PaletteAction::DataValidation,
            "data.validation",
            "Cell Data Validation...",
        ),
        (PaletteAction::AddRow, "table.add_row", "Add Row"),
        (PaletteAction::DeleteRow, "table.delete_row", "Delete Row"),
        (PaletteAction::AddCol, "table.add_col", "Add Column"),
        (
            PaletteAction::DeleteCol,
            "table.delete_col",
            "Delete Column",
        ),
        (PaletteAction::SortAsc, "table.sort_asc", "Sort Ascending"),
        (
            PaletteAction::SortDesc,
            "table.sort_desc",
            "Sort Descending",
        ),
        (
            PaletteAction::FreezeHeader,
            "table.freeze_header",
            "Freeze Header Row",
        ),
        (
            PaletteAction::UnfreezePanes,
            "table.unfreeze_panes",
            "Unfreeze Panes",
        ),
        (
            PaletteAction::DeleteSheet,
            "sheets.delete_sheet",
            "Delete Sheet",
        ),
        (PaletteAction::Bold, "sheets.bold", "Bold"),
        (PaletteAction::Italic, "sheets.italic", "Italic"),
        (PaletteAction::Underline, "sheets.underline", "Underline"),
        (PaletteAction::AlignLeft, "sheets.align-left", "Align Left"),
        (
            PaletteAction::AlignCenter,
            "sheets.align-center",
            "Align Center",
        ),
        (
            PaletteAction::AlignRight,
            "sheets.align-right",
            "Align Right",
        ),
        (
            PaletteAction::FormatCurrency,
            "format.currency",
            "Format as Currency ($)",
        ),
        (
            PaletteAction::FormatPercent,
            "format.percentage",
            "Format as Percentage (%)",
        ),
        (
            PaletteAction::FormatGeneral,
            "format.general",
            "Format as General",
        ),
        (
            PaletteAction::FormatNumber,
            "format.number",
            "Format as Number (1.23)",
        ),
        (
            PaletteAction::DecimalsIncrease,
            "table.adjust_decimals_increase",
            "Decimals: Increase",
        ),
        (
            PaletteAction::DecimalsDecrease,
            "table.adjust_decimals_decrease",
            "Decimals: Decrease",
        ),
        (
            PaletteAction::Organize,
            "sheets.organize",
            "Sort by This Column (A→Z)",
        ),
        (PaletteAction::FillDown, "table.fill_down", "Fill Down"),
        (
            PaletteAction::InsertChart,
            "sheets.insert_chart",
            if !app.get_chart_exists() {
                "Insert Chart"
            } else {
                "Change Chart Source"
            },
        ),
        (
            PaletteAction::InsertShape,
            "sheets.insert_shape",
            "Insert Shape",
        ),
        (
            PaletteAction::InsertImage,
            "sheets.insert_image",
            "Insert Image",
        ),
        (
            PaletteAction::CycleChartKind,
            "sheets.cycle_chart_kind",
            "Cycle Chart Kind",
        ),
        (PaletteAction::CopyChart, "sheets.copy_chart", "Copy Chart"),
        (
            PaletteAction::WorksheetObjects,
            "sheets.worksheet-objects",
            "Select Worksheet Objects",
        ),
        (PaletteAction::Borders, "format.borders", "Borders: Toggle"),
        (
            PaletteAction::BordersAll,
            "format.borders_all",
            "Borders: All",
        ),
        (
            PaletteAction::BordersOuter,
            "format.borders_outer",
            "Borders: Outer",
        ),
        (
            PaletteAction::BordersTop,
            "format.borders_top",
            "Borders: Top",
        ),
        (
            PaletteAction::BordersBottom,
            "format.borders_bottom",
            "Borders: Bottom",
        ),
        (
            PaletteAction::BordersLeft,
            "format.borders_left",
            "Borders: Left",
        ),
        (
            PaletteAction::BordersRight,
            "format.borders_right",
            "Borders: Right",
        ),
        (
            PaletteAction::BordersNone,
            "format.borders_none",
            "Borders: Clear",
        ),
        (
            PaletteAction::Strikethrough,
            "format.strikethrough",
            "Format: Strikethrough",
        ),
        (
            PaletteAction::WrapText,
            "format.wrap_text",
            "Format: Wrap Text",
        ),
        (
            PaletteAction::TextColorCycle,
            "format.text_color_cycle",
            "Format: Cycle Text Color",
        ),
        (
            PaletteAction::TextColorAuto,
            "format.text_color_auto",
            "Format: Automatic Text Color",
        ),
        (
            PaletteAction::MergeCells,
            "format.merge",
            "Format: Merge Cells",
        ),
        (
            PaletteAction::UnmergeCells,
            "format.unmerge",
            "Format: Unmerge Cells",
        ),
        (
            PaletteAction::ConditionalFormatting,
            "format.conditional_formatting",
            "Format: Conditional Formatting...",
        ),
        (
            PaletteAction::PasteValues,
            "edit.paste_values",
            "Paste Special: Values Only",
        ),
        (
            PaletteAction::PasteFormats,
            "edit.paste_formats",
            "Paste Special: Formats Only",
        ),
        (
            PaletteAction::FillCycle,
            "format.fill_cycle",
            "Fill: Cycle Color",
        ),
        (
            PaletteAction::FontIncrease,
            "format.font_increase",
            "Font Size: Increase",
        ),
        (
            PaletteAction::FontDecrease,
            "format.font_decrease",
            "Font Size: Decrease",
        ),
        (
            PaletteAction::PivotSum,
            "table.pivot_sum",
            "Pivot Summary: Sum",
        ),
        (
            PaletteAction::PivotCount,
            "table.pivot_count",
            "Pivot Summary: Count",
        ),
        (
            PaletteAction::PivotAverage,
            "table.pivot_average",
            "Pivot Summary: Average",
        ),
        (
            PaletteAction::PivotMin,
            "table.pivot_min",
            "Pivot Summary: Min",
        ),
        (
            PaletteAction::PivotMax,
            "table.pivot_max",
            "Pivot Summary: Max",
        ),
    ]
    .into_iter()
    .chain(loom_desktop::Appearance::ALL.into_iter().map(|choice| {
        (
            PaletteAction::Appearance(choice),
            choice.command_id(),
            choice.label(),
        )
    }))
    .chain(
        loom_desktop::window_frame::WindowFrame::ALL
            .into_iter()
            .map(|choice| {
                (
                    PaletteAction::WindowFrame(choice),
                    choice.command_id(),
                    choice.label(),
                )
            }),
    )
    .chain([
        (
            PaletteAction::WelcomeGuide,
            "sheets.welcome",
            "Welcome Guide",
        ),
        (PaletteAction::Undo, "sheets.undo", "Undo"),
        (PaletteAction::Redo, "sheets.redo", "Redo"),
    ])
    .map(|(action, id, label)| PaletteCommand {
        action,
        id,
        label,
        shortcut: crate::command_dispatch::shortcut_for_id(id),
        keywords: keywords_for(id),
    })
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        PaletteAction::WorksheetObjects => app.get_object_views().row_count() > 0,
        PaletteAction::RetryRecovery => app.get_recovery_paused(),
        PaletteAction::WindowFrame(loom_desktop::window_frame::WindowFrame::Loom) => {
            loom_desktop::window_frame::SUPPORTED
        }
        _ => true,
    })
    .collect()
}

pub fn rebuild_palette(app: &SheetsApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette(app)
        .into_iter()
        .filter(|c| command_matches(c, &query_lower))
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

#[cfg(test)]
mod search_tests {
    use super::*;

    fn command(id: &'static str, label: &'static str) -> PaletteCommand {
        PaletteCommand {
            action: PaletteAction::SaveSheet,
            id,
            label,
            shortcut: "",
            keywords: keywords_for(id),
        }
    }

    #[test]
    fn export_xlsx_is_found_by_its_name_and_by_the_words_people_type() {
        let xlsx = command("sheets.export-xlsx", "Export Excel (.xlsx)");
        assert!(command_matches(&xlsx, "Export XLSX"));
        assert!(command_matches(&xlsx, "xlsx"));
        assert!(command_matches(&xlsx, "excel"));
        assert!(command_matches(&xlsx, "export excel"));
        assert!(command_matches(&xlsx, "office"));
        assert!(!command_matches(&xlsx, "csv"));
        assert!(!command_matches(&xlsx, "export pdf"));
    }

    #[test]
    fn csv_export_keeps_its_own_words_and_the_phrases_that_already_matched() {
        let csv = command("sheets.export-csv", "Export CSV");
        assert!(command_matches(&csv, "export csv"));
        assert!(command_matches(&csv, "comma"));
        assert!(!command_matches(&csv, "xlsx"));
        let save_as = command("sheets.save-as", "Save Workbook As");
        assert!(command_matches(&save_as, "save workbook"));
        assert!(
            command_matches(&save_as, "   "),
            "a blank query lists every command"
        );
    }

    #[test]
    fn a_query_containing_a_word_no_field_has_matches_nothing() {
        let csv = command("sheets.export-csv", "Export CSV");
        assert!(!command_matches(&csv, "zzz"));
        assert!(!command_matches(&csv, "export zzz"));
    }

    #[test]
    fn window_frame_choices_match_title_bar_and_window_frame_queries() {
        for choice in loom_desktop::window_frame::WindowFrame::ALL {
            let entry = PaletteCommand {
                action: PaletteAction::WindowFrame(choice),
                id: choice.command_id(),
                label: choice.label(),
                shortcut: "",
                keywords: keywords_for(choice.command_id()),
            };
            assert!(
                command_matches(&entry, "title bar"),
                "{} must match 'title bar'",
                choice.command_id()
            );
            assert!(
                command_matches(&entry, "window frame"),
                "{} must match 'window frame'",
                choice.command_id()
            );
            assert!(
                command_matches(&entry, "chrome"),
                "{} must match 'chrome'",
                choice.command_id()
            );
            assert!(
                command_matches(&entry, "border"),
                "{} must match 'border'",
                choice.command_id()
            );
            assert_eq!(
                palette_action_for_id(choice.command_id()),
                Some(PaletteAction::WindowFrame(choice)),
                "palette_action_for_id must round-trip {}",
                choice.command_id()
            );
        }
    }

    #[test]
    fn master_palette_includes_supported_window_frame_choices_and_dispatches_them() {
        crate::set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let commands = master_palette(&app);
        let has_system = commands.iter().any(|c| {
            c.action == PaletteAction::WindowFrame(loom_desktop::window_frame::WindowFrame::System)
        });
        let has_loom = commands.iter().any(|c| {
            c.action == PaletteAction::WindowFrame(loom_desktop::window_frame::WindowFrame::Loom)
        });
        assert!(has_system, "master_palette must include System title bar");
        assert_eq!(
            has_loom,
            loom_desktop::window_frame::SUPPORTED,
            "Loom title bar presence in master_palette must match SUPPORTED"
        );

        rebuild_palette(&app, "window frame");
        let expected_rows = 1 + usize::from(loom_desktop::window_frame::SUPPORTED);
        assert_eq!(app.get_palette_commands().row_count(), expected_rows);

        assert!(dispatch_palette_action(
            &app,
            PaletteAction::WindowFrame(loom_desktop::window_frame::WindowFrame::System),
        ));
    }
}
