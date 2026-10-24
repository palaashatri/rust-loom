//! The in-window menu for Writer (Windows and Linux). Projection, keyboard
//! navigation, and dispatch are shared (`loom_desktop::local_menu_bindings!`);
//! this file only says which commands Writer handles.

use crate::{appearance, view_state, WriterApp};
use loom_desktop::{CommandStateProjection, DesktopError, NativeMenuBar};

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
    "list.level.increase",
    "list.level.decrease",
    "insert.page_break",
    "insert.line_break",
    "insert.table",
    "insert.chart",
    "insert.image",
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

/// State changes that can alter the installed menu's command availability.
pub(crate) fn enablement_key(app: &WriterApp) -> (bool, bool, bool) {
    (
        app.get_is_table_selected(),
        app.get_can_undo(),
        app.get_can_redo(),
    )
}

const TABLE_STRUCTURAL_COMMANDS: &[&str] = &[
    "table.insert_row_above",
    "table.insert_row_below",
    "table.insert_col_left",
    "table.insert_col_right",
    "table.delete_row",
    "table.delete_column",
    "table.merge_cells",
    "table.split_cell",
];

/// Build the live state consumed by every standard menu surface, including
/// contextual structural table-command availability.
pub(crate) fn menu_projection(
    menu_service: &NativeMenuBar,
    app: &WriterApp,
) -> Result<CommandStateProjection, DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("Writer menu bar is not installed".into()))?;
    let mut projection = menu_bar.command_state_projection();

    let mut undo = projection
        .get("edit.undo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Writer menu is missing edit.undo".into()))?;
    undo.enabled = app.get_can_undo();
    projection.insert(undo);

    let mut redo = projection
        .get("edit.redo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Writer menu is missing edit.redo".into()))?;
    redo.enabled = app.get_can_redo();
    projection.insert(redo);

    let mut inspector = projection.get("view.inspector").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Writer menu is missing view.inspector".into())
    })?;
    inspector.enabled = app.get_inspector_available();
    inspector.checked = Some(app.get_show_inspector());
    projection.insert(inspector);

    let table_selected = app.get_is_table_selected();
    for command_id in TABLE_STRUCTURAL_COMMANDS {
        if let Some(mut command) = projection.get(command_id).cloned() {
            command.enabled = table_selected;
            projection.insert(command);
        }
    }

    view_state::project(&mut projection, app);
    loom_desktop::appearance::project_checks(&mut projection, appearance::current(app));

    Ok(projection)
}

/// A shortcut for a bare key, with or without Shift (Tab, Shift+Tab, Shift+Enter).
fn key_shortcut(key: &str, shift: bool) -> loom_desktop::MenuShortcut {
    loom_desktop::MenuShortcut {
        key: key.to_string(),
        primary_modifier: false,
        shift,
        alt: false,
    }
}

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
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "list.level.increase",
                        "Increase List Level",
                        key_shortcut("Tab", false),
                    ),
                    MenuItem::action_with_shortcut(
                        "list.level.decrease",
                        "Decrease List Level",
                        key_shortcut("Tab", true),
                    ),
                ],
            ),
            Menu::new(
                "Insert",
                vec![
                    MenuItem::action_with_shortcut(
                        "insert.page_break",
                        "Page Break",
                        MenuShortcut::primary("Enter"),
                    ),
                    MenuItem::action_with_shortcut(
                        "insert.line_break",
                        "Line Break",
                        key_shortcut("Enter", true),
                    ),
                ],
            ),
            Menu::new(
                "Table",
                vec![
                    MenuItem::action("insert.table", "Insert Table"),
                    MenuItem::action("insert.image", "Insert Picture..."),
                    MenuItem::action("picture.alt_text", "Picture Description..."),
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
