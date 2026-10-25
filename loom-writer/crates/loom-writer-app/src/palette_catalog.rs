//! The command palette's entries. Each entry names a palette action and the
//! command id it dispatches through the same callbacks as the toolbar and menus.

use crate::{dispatch_command, WriterApp};
use loom_command::{CommandRegistry, CommandSpec};

/// Other words a command answers to, besides its label. A query that equals one
/// of them lifts the command above commands that only share the word in a label
/// (a query for "markdown" should find "Export Markdown (.md)" first).
const PALETTE_ALIASES: &[(&str, &[&str])] = &[("file.export_md", &["markdown", "md", ".md"])];

/// Score added to a command for answering to the query exactly. Registry scores
/// top out at 1000 for an exact label, so an alias wins a tie within a tier.
const ALIAS_BONUS: u32 = 100;

/// The registry's matches for `query`, best first. The registry's own order
/// breaks ties, and aliases add [`ALIAS_BONUS`]. The palette lists these rows
/// and Enter runs the first of them, so every search path uses this one order.
pub(crate) fn ranked_matches<'a>(
    registry: &'a CommandRegistry,
    query: &str,
) -> Vec<&'a CommandSpec> {
    let query = query.trim().to_lowercase();
    let mut matches: Vec<(u32, &CommandSpec)> = registry
        .search(&query)
        .into_iter()
        .map(|(spec, score)| (score + alias_bonus(spec.id.as_str(), &query), spec))
        .collect();
    matches.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.order.cmp(&b.1.order))
            .then(a.1.id.cmp(&b.1.id))
    });
    matches.into_iter().map(|(_, spec)| spec).collect()
}

fn alias_bonus(id: &str, query: &str) -> u32 {
    let answers_to_query = PALETTE_ALIASES
        .iter()
        .any(|(alias_id, words)| *alias_id == id && words.contains(&query));
    if answers_to_query {
        ALIAS_BONUS
    } else {
        0
    }
}

/// Commands exposed through the command palette. Each palette entry maps to
/// one of the application callbacks, so palette invocation and toolbar clicks
/// share a single dispatch path.
#[derive(Debug, Clone)]
pub(crate) enum PaletteAction {
    NewDoc,
    NewFromSample,
    OpenDoc,
    SaveDoc,
    SaveAsDoc,
    ExportPdf,
    ExportDocx,
    ExportMarkdown,
    Undo,
    Redo,
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    SetHeading(i32),
    SetAlignment(i32),
    /// A command that is run by its identifier, as the menus run it.
    Command(&'static str),
    InsertTable,
    InsertTableRowAbove,
    InsertTableRowBelow,
    InsertTableColLeft,
    InsertTableColRight,
    DeleteTableRow,
    DeleteTableColumn,
    MergeTableCells,
    SplitTableCell,
    InsertChart,
    InsertPicture,
    SetAppearance(loom_desktop::Appearance),
}

/// Dispatch a palette command through [`dispatch_command`] where the action
/// has a direct command ID.  Parameterized formatting commands still use the
/// same callback endpoints but carry their value explicitly.
pub(crate) fn dispatch_palette_action(app: &WriterApp, action: PaletteAction) -> bool {
    match action {
        PaletteAction::SetHeading(level) => {
            app.invoke_select_heading(level);
            true
        }
        PaletteAction::SetAlignment(index) => {
            app.invoke_select_alignment(index);
            true
        }
        PaletteAction::Command(id) => dispatch_command(app, id),
        PaletteAction::InsertTable => dispatch_command(app, "writer.table.insert"),
        PaletteAction::InsertTableRowAbove => dispatch_command(app, "table.insert_row_above"),
        PaletteAction::InsertTableRowBelow => dispatch_command(app, "table.insert_row_below"),
        PaletteAction::InsertTableColLeft => dispatch_command(app, "table.insert_col_left"),
        PaletteAction::InsertTableColRight => dispatch_command(app, "table.insert_col_right"),
        PaletteAction::DeleteTableRow => dispatch_command(app, "table.delete_row"),
        PaletteAction::DeleteTableColumn => dispatch_command(app, "table.delete_column"),
        PaletteAction::MergeTableCells => dispatch_command(app, "table.merge_cells"),
        PaletteAction::SplitTableCell => dispatch_command(app, "table.split_cell"),
        PaletteAction::InsertChart => dispatch_command(app, "writer.chart.insert"),
        PaletteAction::InsertPicture => dispatch_command(app, "writer.image.insert"),
        PaletteAction::SetAppearance(choice) => dispatch_command(app, choice.command_id()),
        PaletteAction::NewDoc => dispatch_command(app, "writer.new"),
        // Needs the document state; handled where the palette is wired.
        PaletteAction::NewFromSample => false,
        PaletteAction::OpenDoc => dispatch_command(app, "writer.open"),
        PaletteAction::SaveDoc => dispatch_command(app, "writer.save"),
        PaletteAction::SaveAsDoc => dispatch_command(app, "writer.save-as"),
        PaletteAction::ExportPdf => dispatch_command(app, "writer.export-pdf"),
        PaletteAction::ExportDocx => dispatch_command(app, "writer.export-docx"),
        PaletteAction::ExportMarkdown => dispatch_command(app, "file.export_md"),
        PaletteAction::Undo => dispatch_command(app, "writer.undo"),
        PaletteAction::Redo => dispatch_command(app, "writer.redo"),
        PaletteAction::ToggleBold => dispatch_command(app, "writer.style.bold-all"),
        PaletteAction::ToggleItalic => dispatch_command(app, "writer.style.italic-all"),
        PaletteAction::ToggleUnderline => dispatch_command(app, "writer.style.underline-all"),
    }
}

pub(crate) struct PaletteCommand {
    pub(crate) action: PaletteAction,
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) shortcut: &'static str,
}

pub(crate) fn master_palette(app: &WriterApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewDoc,
            id: "writer.new",
            label: "New Document",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::NewFromSample,
            id: "writer.new-sample",
            label: "New from Quick Start sample",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::OpenDoc,
            id: "writer.open",
            label: "Open Document",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveDoc,
            id: "writer.save",
            label: "Save Document",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDoc,
            id: "writer.save-as",
            label: "Save Document As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::ExportPdf,
            id: "writer.export-pdf",
            label: "Export PDF",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::ExportDocx,
            id: "writer.export-docx",
            label: "Export Word Document (.docx)",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ExportMarkdown,
            id: "file.export_md",
            label: "Export Markdown (.md)",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "writer.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "writer.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::ToggleBold,
            id: "writer.style.bold-all",
            label: "Document Style: Bold",
            shortcut: "Ctrl+B",
        },
        PaletteCommand {
            action: PaletteAction::ToggleItalic,
            id: "writer.style.italic-all",
            label: "Document Style: Italic",
            shortcut: "Ctrl+I",
        },
        PaletteCommand {
            action: PaletteAction::ToggleUnderline,
            id: "writer.style.underline-all",
            label: "Document Style: Underline",
            shortcut: "Ctrl+U",
        },
        PaletteCommand {
            action: PaletteAction::InsertTable,
            id: "writer.table.insert",
            label: "Insert Table",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::InsertTableRowAbove,
            id: "table.insert_row_above",
            label: "Table: Insert Row Above",
            shortcut: "Ctrl+Alt+Up",
        },
        PaletteCommand {
            action: PaletteAction::InsertTableRowBelow,
            id: "table.insert_row_below",
            label: "Table: Insert Row Below",
            shortcut: "Ctrl+Alt+Down",
        },
        PaletteCommand {
            action: PaletteAction::InsertTableColLeft,
            id: "table.insert_col_left",
            label: "Table: Insert Column Left",
            shortcut: "Ctrl+Alt+Left",
        },
        PaletteCommand {
            action: PaletteAction::InsertTableColRight,
            id: "table.insert_col_right",
            label: "Table: Insert Column Right",
            shortcut: "Ctrl+Alt+Right",
        },
        PaletteCommand {
            action: PaletteAction::DeleteTableRow,
            id: "table.delete_row",
            label: "Table: Delete Row",
            shortcut: "Ctrl+Alt+Backspace",
        },
        PaletteCommand {
            action: PaletteAction::DeleteTableColumn,
            id: "table.delete_column",
            label: "Table: Delete Column",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::MergeTableCells,
            id: "table.merge_cells",
            label: "Table: Merge Cells",
            shortcut: "Ctrl+Alt+M",
        },
        PaletteCommand {
            action: PaletteAction::SplitTableCell,
            id: "table.split_cell",
            label: "Table: Split Cell",
            shortcut: "Ctrl+Alt+S",
        },
        PaletteCommand {
            action: PaletteAction::InsertPicture,
            id: "writer.image.insert",
            label: "Insert Picture...",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::InsertChart,
            id: "writer.chart.insert",
            label: "Insert Chart (from Clipboard)",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(1),
            id: "writer.style.heading-1-all",
            label: "All Paragraphs: Heading 1",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(2),
            id: "writer.style.heading-2-all",
            label: "All Paragraphs: Heading 2",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(3),
            id: "writer.style.heading-3-all",
            label: "All Paragraphs: Heading 3",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(0),
            id: "writer.style.body-all",
            label: "All Paragraphs: Body Text",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(0),
            id: "writer.align.left-all",
            label: "Align All Left",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(1),
            id: "writer.align.center-all",
            label: "Align All Center",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(2),
            id: "writer.align.right-all",
            label: "Align All Right",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(3),
            id: "writer.align.justify-all",
            label: "Align All Justify",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("insert.page_break"),
            id: "insert.page_break",
            label: "Insert Page Break",
            shortcut: "Ctrl+Enter",
        },
        PaletteCommand {
            action: PaletteAction::Command("insert.line_break"),
            id: "insert.line_break",
            label: "Insert Line Break",
            shortcut: "Shift+Enter",
        },
        PaletteCommand {
            action: PaletteAction::Command("list.level.increase"),
            id: "list.level.increase",
            label: "Increase List Level",
            shortcut: "Tab",
        },
        PaletteCommand {
            action: PaletteAction::Command("list.level.decrease"),
            id: "list.level.decrease",
            label: "Decrease List Level",
            shortcut: "Shift+Tab",
        },
    ]
    .into_iter()
    .chain(
        loom_desktop::Appearance::ALL
            .into_iter()
            .map(|choice| PaletteCommand {
                action: PaletteAction::SetAppearance(choice),
                id: choice.command_id(),
                label: choice.label(),
                shortcut: "",
            }),
    )
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}
