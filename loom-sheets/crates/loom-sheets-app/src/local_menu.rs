//! The in-window menu for Sheets. The projection, keyboard navigation, and
//! dispatch logic are shared (`loom_desktop::local_menu_bindings!`); this file
//! only says which commands Sheets handles.

use crate::SheetsApp;

/// Canonical Quit / Exit command id installed in the platform menu bar.
#[cfg(test)]
pub(crate) const QUIT_COMMAND_ID: &str = if cfg!(target_os = "macos") {
    "loom sheets.quit"
} else {
    "app.quit"
};

pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "app.quit",
    "loom sheets.quit",
    "file.new",
    "file.new_template",
    "file.start_hub",
    "file.open",
    "file.save",
    "file.save_as",
    "file.export_csv",
    "file.export_xlsx",
    "file.export_pdf",
    "file.print",
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.paste_values",
    "edit.paste_formats",
    "edit.select_all",
    "edit.find",
    "edit.replace",
    "data.toggle_filter",
    "data.filter_column",
    "data.clear_filter",
    "data.named_ranges",
    "data.validation",
    "app.palette",
    "help.shortcuts",
    "help.welcome",
    "view.inspector",
    "view.zoom_in",
    "view.zoom_out",
    "view.zoom_actual",
    "view.appearance.system",
    "view.appearance.light",
    "view.appearance.dark",
    "view.appearance.high_contrast",
    "view.window-frame.system",
    "view.window-frame.loom",
    "table.add_row",
    "table.delete_row",
    "table.add_col",
    "table.delete_col",
    "table.insert_table",
    "table.toggle_title",
    "table.toggle_alternating_rows",
    "table.cycle_style",
    "table.delete_table",
    "table.sort_asc",
    "table.sort_desc",
    "table.freeze_header",
    "table.unfreeze_panes",
    "table.pivot_sum",
    "table.pivot_count",
    "table.pivot_average",
    "table.pivot_builder",
    "table.adjust_decimals_increase",
    "table.adjust_decimals_decrease",
    "format.bold",
    "format.italic",
    "format.underline",
    "format.strikethrough",
    "format.wrap_text",
    "format.text_color_cycle",
    "format.text_color_auto",
    "format.font_increase",
    "format.font_decrease",
    "format.align_left",
    "format.align_center",
    "format.align_right",
    "format.general",
    "format.number",
    "format.currency",
    "format.percentage",
    "format.borders",
    "format.borders_all",
    "format.borders_outer",
    "format.borders_top",
    "format.borders_bottom",
    "format.borders_left",
    "format.borders_right",
    "format.borders_none",
    "format.fill_cycle",
    "format.merge",
    "format.unmerge",
    "format.conditional_formatting",
    "sheets.add_sheet",
    "sheets.insert_chart",
    "sheets.insert_shape",
    "sheets.insert_image",
    "sheets.delete_sheet",
    "arrange.bring_forward",
    "arrange.send_backward",
    "arrange.bring_to_front",
    "arrange.send_to_back",
];

loom_desktop::local_menu_bindings!(SheetsApp, SUPPORTED_COMMANDS, set_status_left);

/// Sheets' menu bar with only the commands Sheets handles enabled. Help >
/// Keyboard Shortcuts opens the command palette.
pub(crate) fn sheets_menu_bar() -> loom_desktop::MenuBar {
    use loom_desktop::{build_standard_menu_bar, CommandState, Menu, MenuItem, MenuShortcut};
    let mut menu_bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![
            MenuItem::action_with_shortcut(
                "file.new_template",
                "New from Template...",
                MenuShortcut::primary_shift("N"),
            ),
            MenuItem::action("file.start_hub", "Start Hub..."),
            MenuItem::action_with_shortcut(
                "file.export_csv",
                "Export to CSV...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_xlsx",
                "Export to Excel (.xlsx)...",
                MenuShortcut::primary_shift("E"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_pdf",
                "Export to PDF...",
                MenuShortcut {
                    key: "F".into(),
                    primary_modifier: true,
                    shift: true,
                    alt: true,
                },
            ),
            MenuItem::action_with_shortcut("file.print", "Print...", MenuShortcut::primary("P")),
        ],
        vec![
            MenuItem::action_with_shortcut("edit.find", "Find...", MenuShortcut::primary("F")),
            MenuItem::action_with_shortcut(
                "edit.replace",
                "Find & Replace...",
                MenuShortcut::primary("H"),
            ),
            MenuItem::action_with_shortcut(
                "edit.paste_values",
                "Paste Values Only",
                MenuShortcut::primary_shift("V"),
            ),
            MenuItem::action_with_shortcut(
                "edit.paste_formats",
                "Paste Formats Only",
                MenuShortcut {
                    key: "V".into(),
                    primary_modifier: true,
                    shift: true,
                    alt: true,
                },
            ),
        ],
        [
            vec![
                CommandState::check("view.inspector", "Format Inspector", false)
                    .with_shortcut(MenuShortcut::primary_alt("I"))
                    .menu_item(),
            ],
            loom_desktop::appearance::menu_items(),
            loom_desktop::window_frame::menu_items(),
        ]
        .concat(),
        vec![
            Menu::new(
                "Data",
                vec![
                    MenuItem::action("data.toggle_filter", "Toggle AutoFilter"),
                    MenuItem::action("data.filter_column", "Filter Column..."),
                    MenuItem::action("data.clear_filter", "Clear All Filters"),
                    MenuItem::Separator,
                    MenuItem::action("data.named_ranges", "Named Ranges..."),
                    MenuItem::action("data.validation", "Data Validation..."),
                ],
            ),
            Menu::new(
                "Insert",
                vec![
                    MenuItem::action_with_shortcut(
                        "sheets.add_sheet",
                        "Add Sheet",
                        MenuShortcut::primary_alt("N"),
                    ),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "sheets.insert_chart",
                        "Insert Chart",
                        MenuShortcut::primary_alt("C"),
                    ),
                    MenuItem::action_with_shortcut(
                        "sheets.insert_shape",
                        "Insert Shape",
                        MenuShortcut::primary_alt("J"),
                    ),
                    MenuItem::action_with_shortcut(
                        "sheets.insert_image",
                        "Insert Image",
                        MenuShortcut::primary_alt("M"),
                    ),
                ],
            ),
            Menu::new(
                "Table",
                vec![
                    MenuItem::action_with_shortcut(
                        "table.add_row",
                        "Add Row",
                        MenuShortcut::primary_alt("R"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.delete_row",
                        "Delete Row",
                        MenuShortcut::primary_alt("W"),
                    ),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "table.add_col",
                        "Add Column",
                        MenuShortcut::primary_alt("K"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.delete_col",
                        "Delete Column",
                        MenuShortcut::primary_alt("L"),
                    ),
                    MenuItem::Separator,
                    MenuItem::action("table.insert_table", "Insert Table"),
                    MenuItem::action("table.cycle_style", "Cycle Table Style"),
                    MenuItem::action("table.toggle_title", "Toggle Table Title"),
                    MenuItem::action("table.toggle_alternating_rows", "Toggle Alternating Rows"),
                    MenuItem::action("table.delete_table", "Delete Table"),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "table.freeze_header",
                        "Freeze Header Row",
                        MenuShortcut::primary_alt("F"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.unfreeze_panes",
                        "Unfreeze Panes",
                        MenuShortcut::primary_alt("U"),
                    ),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "sheets.delete_sheet",
                        "Delete Sheet",
                        MenuShortcut::primary_alt("X"),
                    ),
                ],
            ),
            Menu::new(
                "Organize",
                vec![
                    MenuItem::action_with_shortcut(
                        "table.sort_asc",
                        "Sort Ascending",
                        MenuShortcut::primary_alt("S"),
                    ),
                    MenuItem::action_with_shortcut(
                        "table.sort_desc",
                        "Sort Descending",
                        MenuShortcut::primary_alt("D"),
                    ),
                    MenuItem::Separator,
                    MenuItem::action_with_shortcut(
                        "table.pivot_builder",
                        "Create Pivot Table…",
                        MenuShortcut {
                            key: "P".into(),
                            primary_modifier: true,
                            shift: true,
                            alt: true,
                        },
                    ),
                    MenuItem::action_with_shortcut(
                        "table.pivot_sum",
                        "Pivot Summary (Sum)",
                        MenuShortcut::primary_alt("P"),
                    ),
                    MenuItem::action("table.pivot_count", "Pivot Summary (Count)"),
                    MenuItem::action("table.pivot_average", "Pivot Summary (Average)"),
                ],
            ),
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
                    MenuItem::action("format.strikethrough", "Strikethrough"),
                    MenuItem::action("format.wrap_text", "Wrap Text"),
                    MenuItem::action("format.text_color_cycle", "Cycle Text Color"),
                    MenuItem::action("format.text_color_auto", "Automatic Text Color"),
                    MenuItem::Separator,
                    MenuItem::action("format.font_increase", "Bigger Text"),
                    MenuItem::action("format.font_decrease", "Smaller Text"),
                    MenuItem::Separator,
                    MenuItem::action("format.align_left", "Align Left"),
                    MenuItem::action("format.align_center", "Align Center"),
                    MenuItem::action("format.align_right", "Align Right"),
                    MenuItem::Separator,
                    MenuItem::action("format.general", "General Format"),
                    MenuItem::action("format.number", "Number Format"),
                    MenuItem::action("format.currency", "Currency Format"),
                    MenuItem::action("format.percentage", "Percentage Format"),
                    MenuItem::action("table.adjust_decimals_increase", "Increase Decimals"),
                    MenuItem::action("table.adjust_decimals_decrease", "Decrease Decimals"),
                    MenuItem::Separator,
                    MenuItem::action("format.borders", "Toggle Borders"),
                    MenuItem::action("format.borders_all", "All Borders"),
                    MenuItem::action("format.borders_outer", "Outer Borders"),
                    MenuItem::action("format.borders_top", "Top Border"),
                    MenuItem::action("format.borders_bottom", "Bottom Border"),
                    MenuItem::action("format.borders_left", "Left Border"),
                    MenuItem::action("format.borders_right", "Right Border"),
                    MenuItem::action("format.borders_none", "Clear Borders"),
                    MenuItem::action("format.fill_cycle", "Cycle Cell Fill"),
                    MenuItem::Separator,
                    MenuItem::action("format.merge", "Merge Cells"),
                    MenuItem::action("format.unmerge", "Unmerge Cells"),
                    MenuItem::action("format.conditional_formatting", "Conditional Formatting..."),
                ],
            ),
            Menu::new(
                "Arrange",
                vec![
                    MenuItem::action_with_shortcut(
                        "arrange.bring_forward",
                        "Bring Forward",
                        MenuShortcut::primary_alt("]"),
                    ),
                    MenuItem::action_with_shortcut(
                        "arrange.send_backward",
                        "Send Backward",
                        MenuShortcut::primary_alt("["),
                    ),
                    MenuItem::Separator,
                    MenuItem::action("arrange.bring_to_front", "Bring to Front"),
                    MenuItem::action("arrange.send_to_back", "Send to Back"),
                ],
            ),
        ],
    );
    if cfg!(target_os = "macos") {
        // Match Apple Numbers menu bar ordering on macOS:
        // App | File | Edit | Insert | Table | Organize | Format | View | Window | Help
        if let Some(view_pos) = menu_bar.menus.iter().position(|m| m.title == "View") {
            let view_menu = menu_bar.menus.remove(view_pos);
            let insert_at = menu_bar
                .menus
                .iter()
                .position(|m| m.title == "Window")
                .unwrap_or(menu_bar.menus.len());
            menu_bar.menus.insert(insert_at, view_menu);
        }
    }
    menu_bar.apply_command_state(
        &CommandState::action("help.shortcuts", "Keyboard Shortcuts")
            .with_shortcut(MenuShortcut::primary_shift("P")),
    );
    if let Some(help) = menu_bar.menus.iter_mut().find(|menu| menu.title == "Help") {
        help.items.insert(
            0,
            MenuItem::action_with_shortcut(
                "help.welcome",
                "Welcome Guide",
                MenuShortcut::primary_shift("/"),
            ),
        );
    }
    menu_bar.disable_items_except(SUPPORTED_COMMANDS);
    menu_bar
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use loom_desktop::{build_standard_menu_bar, MenuBarService, NativeMenuBar};

    use super::*;

    #[test]
    fn local_menu_action_uses_the_installed_menu_guard_and_sink() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let menu_service = Arc::new(NativeMenuBar::new());
        let mut menu = build_standard_menu_bar("Loom Sheets", vec![], vec![], vec![], vec![]);
        menu.disable_items_except(["file.open"]);
        menu_service.install_menu_bar(&menu).expect("install menu");

        let dispatched = Arc::new(Mutex::new(Vec::new()));
        let dispatched_sink = dispatched.clone();
        menu_service
            .register_action_sink(Arc::new(move |action| {
                dispatched_sink.lock().unwrap().push(action.id);
                Ok(())
            }))
            .expect("register action sink");
        wire_action(&app, menu_service);

        app.invoke_local_menu_action("file.open".into());
        app.invoke_local_menu_action("file.save".into());
        assert_eq!(*dispatched.lock().unwrap(), ["file.open"]);
    }
}
