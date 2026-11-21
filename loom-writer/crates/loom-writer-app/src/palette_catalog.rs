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
    if crate::command_dispatch::is_shortcut_query(&query) {
        let existing: std::collections::HashSet<&str> =
            matches.iter().map(|(_, spec)| spec.id.as_str()).collect();
        for spec in registry.commands() {
            if existing.contains(spec.id.as_str()) {
                continue;
            }
            let platform = crate::command_dispatch::shortcut_for_id(spec.id.as_str());
            let effective = if !platform.is_empty() {
                platform
            } else {
                spec.default_shortcut.as_deref().unwrap_or("")
            };
            if crate::command_dispatch::shortcut_matches_query(effective, &query)
                || (spec.id.as_str() == "help.welcome" && query == "f1")
            {
                matches.push((25, spec));
            }
        }
    }
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
    DeleteTable,
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
        PaletteAction::DeleteTable => dispatch_command(app, "table.delete_table"),
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
    use crate::command_dispatch::shortcut_for_id;
    let cmd_entry = |id: &'static str, label: &'static str| PaletteCommand {
        action: PaletteAction::Command(id),
        id,
        label,
        shortcut: shortcut_for_id(id),
    };
    vec![
        PaletteCommand {
            action: PaletteAction::NewDoc,
            id: "writer.new",
            label: "New Document",
            shortcut: shortcut_for_id("writer.new"),
        },
        cmd_entry("file.start_hub", "Open Start Hub…"),
        cmd_entry("file.templates", "Browse Templates…"),
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
            shortcut: shortcut_for_id("writer.open"),
        },
        PaletteCommand {
            action: PaletteAction::SaveDoc,
            id: "writer.save",
            label: "Save Document",
            shortcut: shortcut_for_id("writer.save"),
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDoc,
            id: "writer.save-as",
            label: "Save Document As",
            shortcut: shortcut_for_id("writer.save-as"),
        },
        cmd_entry("file.print_preview", "Print Preview…"),
        PaletteCommand {
            action: PaletteAction::ExportPdf,
            id: "writer.export-pdf",
            label: "Export PDF",
            shortcut: shortcut_for_id("writer.export-pdf"),
        },
        PaletteCommand {
            action: PaletteAction::ExportDocx,
            id: "writer.export-docx",
            label: "Export Word Document (.docx)",
            shortcut: shortcut_for_id("writer.export-docx"),
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
            shortcut: shortcut_for_id("writer.undo"),
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "writer.redo",
            label: "Redo",
            shortcut: shortcut_for_id("writer.redo"),
        },
        cmd_entry("edit.find", "Find…"),
        cmd_entry("edit.replace", "Find & Replace…"),
        cmd_entry("help.welcome", "Open Welcome Guide"),
        cmd_entry("help.shortcuts", "Keyboard Shortcuts"),
        cmd_entry("view.inspector", "Toggle Format Inspector"),
        cmd_entry("view.navigator", "Toggle Outline Navigator"),
        cmd_entry("view.zoom_in", "Zoom In"),
        cmd_entry("view.zoom_out", "Zoom Out"),
        cmd_entry("view.zoom_actual", "Actual Size (100%)"),
        cmd_entry("view.zoom_fit_width", "Fit Page Width"),
        cmd_entry("view.zoom_fit_page", "Fit Whole Page"),
        cmd_entry("review.word_count", "Word Count & Statistics…"),
        cmd_entry("writer.review.open", "Open Tracked Changes…"),
        cmd_entry("insert.header_footer", "Headers & Footers…"),
        cmd_entry("insert.comment", "Insert Comment"),
        PaletteCommand {
            action: PaletteAction::ToggleBold,
            id: "writer.style.bold-all",
            label: "Document Style: Bold",
            shortcut: shortcut_for_id("writer.style.bold-all"),
        },
        PaletteCommand {
            action: PaletteAction::ToggleItalic,
            id: "writer.style.italic-all",
            label: "Document Style: Italic",
            shortcut: shortcut_for_id("writer.style.italic-all"),
        },
        PaletteCommand {
            action: PaletteAction::ToggleUnderline,
            id: "writer.style.underline-all",
            label: "Document Style: Underline",
            shortcut: shortcut_for_id("writer.style.underline-all"),
        },
        cmd_entry("format.strikethrough", "Strikethrough"),
        PaletteCommand {
            action: PaletteAction::Command("writer.direction.auto"),
            id: "writer.direction.auto",
            label: "Automatic Paragraph Direction",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.direction.ltr"),
            id: "writer.direction.ltr",
            label: "Paragraph Left to Right",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.direction.rtl"),
            id: "writer.direction.rtl",
            label: "Paragraph Right to Left",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.align.start"),
            id: "writer.align.start",
            label: "Align to Start",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.align.end"),
            id: "writer.align.end",
            label: "Align to End",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.style.color"),
            id: "writer.style.color",
            label: "Text Color",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.style.superscript"),
            id: "writer.style.superscript",
            label: "Superscript",
            shortcut: shortcut_for_id("writer.style.superscript"),
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.style.subscript"),
            id: "writer.style.subscript",
            label: "Subscript",
            shortcut: shortcut_for_id("writer.style.subscript"),
        },
        PaletteCommand {
            action: PaletteAction::Command("writer.footnotes"),
            id: "writer.footnotes",
            label: "Footnotes…",
            shortcut: shortcut_for_id("writer.footnotes"),
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
            shortcut: shortcut_for_id("table.insert_row_above"),
        },
        PaletteCommand {
            action: PaletteAction::InsertTableRowBelow,
            id: "table.insert_row_below",
            label: "Table: Insert Row Below",
            shortcut: shortcut_for_id("table.insert_row_below"),
        },
        PaletteCommand {
            action: PaletteAction::InsertTableColLeft,
            id: "table.insert_col_left",
            label: "Table: Insert Column Left",
            shortcut: shortcut_for_id("table.insert_col_left"),
        },
        PaletteCommand {
            action: PaletteAction::InsertTableColRight,
            id: "table.insert_col_right",
            label: "Table: Insert Column Right",
            shortcut: shortcut_for_id("table.insert_col_right"),
        },
        PaletteCommand {
            action: PaletteAction::DeleteTableRow,
            id: "table.delete_row",
            label: "Table: Delete Row",
            shortcut: shortcut_for_id("table.delete_row"),
        },
        PaletteCommand {
            action: PaletteAction::DeleteTableColumn,
            id: "table.delete_column",
            label: "Table: Delete Column",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::DeleteTable,
            id: "table.delete_table",
            label: "Table: Delete Table",
            shortcut: shortcut_for_id("table.delete_table"),
        },
        PaletteCommand {
            action: PaletteAction::MergeTableCells,
            id: "table.merge_cells",
            label: "Table: Merge Cells",
            shortcut: shortcut_for_id("table.merge_cells"),
        },
        PaletteCommand {
            action: PaletteAction::SplitTableCell,
            id: "table.split_cell",
            label: "Table: Split Cell",
            shortcut: shortcut_for_id("table.split_cell"),
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
            shortcut: shortcut_for_id("writer.style.heading-1-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(2),
            id: "writer.style.heading-2-all",
            label: "All Paragraphs: Heading 2",
            shortcut: shortcut_for_id("writer.style.heading-2-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(3),
            id: "writer.style.heading-3-all",
            label: "All Paragraphs: Heading 3",
            shortcut: shortcut_for_id("writer.style.heading-3-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetHeading(0),
            id: "writer.style.body-all",
            label: "All Paragraphs: Body Text",
            shortcut: shortcut_for_id("writer.style.body-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(0),
            id: "writer.align.left-all",
            label: "Align All Left",
            shortcut: shortcut_for_id("writer.align.left-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(1),
            id: "writer.align.center-all",
            label: "Align All Center",
            shortcut: shortcut_for_id("writer.align.center-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(2),
            id: "writer.align.right-all",
            label: "Align All Right",
            shortcut: shortcut_for_id("writer.align.right-all"),
        },
        PaletteCommand {
            action: PaletteAction::SetAlignment(3),
            id: "writer.align.justify-all",
            label: "Align All Justify",
            shortcut: shortcut_for_id("writer.align.justify-all"),
        },
        cmd_entry("list.none", "List: None"),
        cmd_entry("list.bulleted", "List: Bulleted"),
        cmd_entry("list.bullet.circle", "List: Bulleted (Circle ◦)"),
        cmd_entry("list.bullet.square", "List: Bulleted (Square ▪)"),
        cmd_entry("list.bullet.dash", "List: Bulleted (Dash –)"),
        cmd_entry("list.numbered", "List: Numbered"),
        cmd_entry("list.number.lower_alpha", "List: Numbered (a, b, c)"),
        cmd_entry("list.number.upper_alpha", "List: Numbered (A, B, C)"),
        cmd_entry("list.number.lower_roman", "List: Numbered (i, ii, iii)"),
        cmd_entry("list.number.upper_roman", "List: Numbered (I, II, III)"),
        PaletteCommand {
            action: PaletteAction::Command("insert.page_break"),
            id: "insert.page_break",
            label: "Insert Page Break",
            shortcut: shortcut_for_id("insert.page_break"),
        },
        PaletteCommand {
            action: PaletteAction::Command("insert.line_break"),
            id: "insert.line_break",
            label: "Insert Line Break",
            shortcut: shortcut_for_id("insert.line_break"),
        },
        PaletteCommand {
            action: PaletteAction::Command("list.level.increase"),
            id: "list.level.increase",
            label: "Increase List Level",
            shortcut: shortcut_for_id("list.level.increase"),
        },
        PaletteCommand {
            action: PaletteAction::Command("list.level.decrease"),
            id: "list.level.decrease",
            label: "Decrease List Level",
            shortcut: shortcut_for_id("list.level.decrease"),
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
