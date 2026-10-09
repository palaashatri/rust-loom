//! The command palette's entries. Each entry names a palette action and the
//! command id it dispatches through the same callbacks as the toolbar and menus.

use crate::{dispatch_command, WriterApp};

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
    InsertTable,
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
        PaletteAction::InsertTable => dispatch_command(app, "writer.table.insert"),
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
            label: "Insert Markdown Table (text)",
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
