//! The in-window menu for Writer (Windows and Linux). Projection, keyboard
//! navigation, and dispatch are shared (`loom_desktop::local_menu_bindings!`);
//! this file only says which commands Writer handles.

use crate::WriterApp;

/// Commands Writer's menu bar enables; the same list gates the native menu.
pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "file.new",
    "file.open",
    "file.save",
    "file.save_as",
    "file.export_pdf",
    "file.export_docx",
    "file.export_md",
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "app.palette",
    "view.inspector",
    "view.navigator",
    "view.appearance.system",
    "view.appearance.light",
    "view.appearance.dark",
    "view.appearance.high_contrast",
    "format.bold",
    "format.italic",
    "format.underline",
    "insert.table",
    "insert.chart",
    "table.insert_row_above",
    "table.insert_row_below",
    "table.insert_col_left",
    "table.insert_col_right",
    "table.delete_row",
    "table.delete_column",
    "table.merge_cells",
    "table.split_cell",
];

loom_desktop::local_menu_bindings!(WriterApp, SUPPORTED_COMMANDS, set_status_right);

/// Writer's menu bar with only the commands Writer actually handles enabled.
pub(crate) fn writer_menu_bar() -> loom_desktop::MenuBar {
    use loom_desktop::{build_standard_menu_bar, Menu, MenuItem, MenuShortcut};
    let mut menu_bar = build_standard_menu_bar(
        "Loom Writer",
        vec![
            MenuItem::action_with_shortcut(
                "file.export_pdf",
                "Export to PDF...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action("file.export_docx", "Export to Word..."),
            MenuItem::action("file.export_md", "Export to Markdown..."),
        ],
        vec![],
        [
            vec![
                MenuItem::check("view.inspector", "Format Inspector", false),
                MenuItem::check("view.navigator", "Outline Navigator", false),
            ],
            loom_desktop::appearance::menu_items(),
        ]
        .concat(),
        vec![
            Menu::new(
                "Format",
                vec![
                    MenuItem::action_with_shortcut(
                        "format.bold",
                        "Bold",
                        MenuShortcut::primary("B"),
                    ),
                    MenuItem::action_with_shortcut(
                        "format.italic",
                        "Italic",
                        MenuShortcut::primary("I"),
                    ),
                    MenuItem::action_with_shortcut(
                        "format.underline",
                        "Underline",
                        MenuShortcut::primary("U"),
                    ),
                ],
            ),
            Menu::new(
                "Table",
                vec![
                    MenuItem::action("insert.table", "Insert Table..."),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "table.insert_row_above",
                        "Insert Row Above",
                        MenuShortcut::primary_alt("Up"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.insert_row_below",
                        "Insert Row Below",
                        MenuShortcut::primary_alt("Down"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.insert_col_left",
                        "Insert Column Left",
                        MenuShortcut::primary_alt("Left"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.insert_col_right",
                        "Insert Column Right",
                        MenuShortcut::primary_alt("Right"),
                    ),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "table.delete_row",
                        "Delete Row",
                        MenuShortcut::primary_alt("Backspace"),
                    ),
                    MenuItem::action("table.delete_column", "Delete Column"),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "table.merge_cells",
                        "Merge Cells",
                        MenuShortcut::primary_alt("M"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.split_cell",
                        "Split Cell",
                        MenuShortcut::primary_alt("S"),
                    ),
                ],
            ),
        ],
    );
    // Application/window/help entries stay disabled until a real host bridge
    // handles them.
    menu_bar.disable_items_except(SUPPORTED_COMMANDS);
    menu_bar
}
