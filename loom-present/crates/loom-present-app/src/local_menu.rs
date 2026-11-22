//! The in-window menu for Present (Windows and Linux). Projection, keyboard
//! navigation, and dispatch are shared (`loom_desktop::local_menu_bindings!`);
//! this file only says which commands Present handles.

use crate::PresentApp;

/// Commands Present's menu bar enables; the same list gates the native menu.
pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "file.new",
    "file.new_sample",
    "file.start_hub",
    "file.open",
    "file.save",
    "file.save_as",
    "file.print_preview",
    "file.export_pdf",
    "file.export_pptx",
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.duplicate",
    "edit.select_all",
    "edit.find",
    "edit.replace",
    "insert.text_box",
    "insert.shape_rect",
    "insert.shape_circle",
    "insert.table",
    "insert.chart",
    "insert.image",
    "table.delete_row",
    "table.delete_column",
    "slide.new",
    "slide.insert_table",
    "slide.insert_chart",
    "slide.insert_image",
    "slide.duplicate",
    "slide.delete",
    "slide.move_up",
    "slide.move_down",
    "slide.prev",
    "slide.next",
    "format.bold",
    "format.italic",
    "format.underline",
    "format.strikethrough",
    "format.font_increase",
    "format.font_decrease",
    "arrange.bring_front",
    "arrange.bring_forward",
    "arrange.send_backward",
    "arrange.send_back",
    "view.inspector",
    "view.navigator",
    "view.notes",
    "view.preview",
    "view.presenter",
    "view.zoom_in",
    "view.zoom_out",
    "view.zoom_reset",
    "view.appearance.system",
    "view.appearance.light",
    "view.appearance.dark",
    "view.appearance.high_contrast",
    "view.window-frame.system",
    "view.window-frame.loom",
    "help.welcome_guide",
    "app.palette",
];

loom_desktop::local_menu_bindings!(PresentApp, SUPPORTED_COMMANDS, set_status_right);

pub(crate) fn build_present_menu_bar() -> loom_desktop::MenuBar {
    let mut menu_bar = loom_desktop::build_standard_menu_bar(
        "Loom Present",
        vec![
            loom_desktop::MenuItem::action_with_shortcut(
                "file.start_hub",
                "Start Hub...",
                loom_desktop::MenuShortcut::primary_shift("O"),
            ),
            loom_desktop::MenuItem::action_with_shortcut(
                "file.print_preview",
                "Print Preview...",
                loom_desktop::MenuShortcut::primary("P"),
            ),
            loom_desktop::MenuItem::action_with_shortcut(
                "file.export_pdf",
                "Export to PDF...",
                loom_desktop::MenuShortcut::primary("E"),
            ),
            loom_desktop::MenuItem::action("file.new_sample", "New from Sample Deck"),
            loom_desktop::MenuItem::action_with_shortcut(
                "file.export_pptx",
                "Export to PowerPoint...",
                loom_desktop::MenuShortcut::primary_shift("E"),
            ),
        ],
        vec![
            loom_desktop::MenuItem::action_with_shortcut(
                "edit.duplicate",
                "Duplicate Element",
                loom_desktop::MenuShortcut::primary("D"),
            ),
            loom_desktop::MenuItem::action_with_shortcut(
                "edit.select_all",
                "Select All",
                loom_desktop::MenuShortcut::primary("A"),
            ),
            loom_desktop::MenuItem::Separator,
            loom_desktop::MenuItem::action_with_shortcut(
                "edit.find",
                "Find...",
                loom_desktop::MenuShortcut::primary("F"),
            ),
            loom_desktop::MenuItem::action_with_shortcut(
                "edit.replace",
                "Find & Replace...",
                loom_desktop::MenuShortcut::primary("H"),
            ),
        ],
        vec![
            loom_desktop::MenuItem::check("view.navigator", "Navigator", true),
            loom_desktop::MenuItem::check("view.notes", "Speaker Notes", false),
            loom_desktop::MenuItem::check("view.inspector", "Format Inspector", false),
        ]
        .into_iter()
        .chain(loom_desktop::appearance::menu_items())
        .chain(loom_desktop::window_frame::menu_items())
        .collect(),
        vec![
            loom_desktop::Menu::new(
                "Insert",
                vec![
                    loom_desktop::MenuItem::action("insert.text_box", "Text Box"),
                    loom_desktop::MenuItem::action("insert.shape_rect", "Rectangle"),
                    loom_desktop::MenuItem::action("insert.shape_circle", "Circle"),
                    loom_desktop::MenuItem::Separator,
                    loom_desktop::MenuItem::action("insert.table", "Table..."),
                    loom_desktop::MenuItem::action("insert.chart", "Chart"),
                    loom_desktop::MenuItem::action("insert.image", "Picture..."),
                ],
            ),
            loom_desktop::Menu::new(
                "Slide",
                vec![
                    loom_desktop::MenuItem::action_with_shortcut(
                        "slide.new",
                        "New Slide",
                        loom_desktop::MenuShortcut::primary_shift("N"),
                    ),
                    loom_desktop::MenuItem::action("slide.insert_table", "Insert Table..."),
                    loom_desktop::MenuItem::action(
                        "slide.insert_chart",
                        "Paste Chart from Sheets...",
                    ),
                    loom_desktop::MenuItem::action("slide.insert_image", "Insert Image..."),
                    loom_desktop::MenuItem::action("slide.duplicate", "Duplicate Slide"),
                    loom_desktop::MenuItem::action("slide.delete", "Delete Slide"),
                    loom_desktop::MenuItem::Separator,
                    loom_desktop::MenuItem::action_with_shortcut(
                        crate::slide_order::MOVE_UP,
                        "Move Slide Up",
                        loom_desktop::MenuShortcut::primary_alt("Up"),
                    ),
                    loom_desktop::MenuItem::action_with_shortcut(
                        crate::slide_order::MOVE_DOWN,
                        "Move Slide Down",
                        loom_desktop::MenuShortcut::primary_alt("Down"),
                    ),
                    loom_desktop::MenuItem::Separator,
                    loom_desktop::MenuItem::action("slide.prev", "Previous Slide"),
                    loom_desktop::MenuItem::action("slide.next", "Next Slide"),
                ],
            ),
            loom_desktop::Menu::new(
                "Format",
                vec![
                    loom_desktop::MenuItem::action_with_shortcut(
                        "format.bold",
                        "Bold",
                        loom_desktop::MenuShortcut::primary("B"),
                    ),
                    loom_desktop::MenuItem::action_with_shortcut(
                        "format.italic",
                        "Italic",
                        loom_desktop::MenuShortcut::primary("I"),
                    ),
                    loom_desktop::MenuItem::action_with_shortcut(
                        "format.underline",
                        "Underline",
                        loom_desktop::MenuShortcut::primary("U"),
                    ),
                    loom_desktop::MenuItem::action_with_shortcut(
                        "format.strikethrough",
                        "Strikethrough",
                        loom_desktop::MenuShortcut::primary_shift("X"),
                    ),
                    loom_desktop::MenuItem::Separator,
                    loom_desktop::MenuItem::action_with_shortcut(
                        "format.font_increase",
                        "Increase Font Size",
                        loom_desktop::MenuShortcut::primary("]"),
                    ),
                    loom_desktop::MenuItem::action_with_shortcut(
                        "format.font_decrease",
                        "Decrease Font Size",
                        loom_desktop::MenuShortcut::primary("["),
                    ),
                ],
            ),
            loom_desktop::Menu::new(
                "Arrange",
                vec![
                    loom_desktop::MenuItem::action("arrange.bring_front", "Bring to Front"),
                    loom_desktop::MenuItem::action("arrange.bring_forward", "Bring Forward"),
                    loom_desktop::MenuItem::action("arrange.send_backward", "Send Backward"),
                    loom_desktop::MenuItem::action("arrange.send_back", "Send to Back"),
                ],
            ),
            loom_desktop::Menu::new(
                "Help",
                vec![
                    loom_desktop::MenuItem::action_with_shortcut(
                        "help.welcome_guide",
                        "Welcome Guide",
                        loom_desktop::MenuShortcut::primary_shift("/"),
                    ),
                    loom_desktop::MenuItem::action_with_shortcut(
                        "app.palette",
                        "Command Palette...",
                        loom_desktop::MenuShortcut::primary("K"),
                    ),
                ],
            ),
        ],
    );
    crate::file_menu::group(&mut menu_bar);
    menu_bar.disable_items_except(SUPPORTED_COMMANDS);
    menu_bar
}
