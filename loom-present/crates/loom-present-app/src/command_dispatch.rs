//! Centralized command and keyboard-shortcut dispatch for Loom Present.

use crate::{
    appearance, element_style_ui, find_replace_ui, set_status, slide_order, start_hub, view_state,
    welcome_guide, window_chrome, ElementInk, PresentApp,
};
use loom_desktop::{CommandAction, DesktopError};
use slint::ComponentHandle;

/// Platform primary modifier label for shortcut hints.
pub(crate) const PRIMARY_MOD: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

/// Return the human-readable shortcut hint for a command ID, or `""` if none.
pub(crate) fn shortcut_for_id(id: &str) -> &'static str {
    if PRIMARY_MOD == "Cmd" {
        match id {
            "app.palette" => "Cmd+K",
            "help.welcome_guide" => "Cmd+Shift+/",
            "file.start_hub" => "Cmd+Shift+O",
            "file.new" => "Cmd+N",
            "file.open" => "Cmd+O",
            "file.save" => "Cmd+S",
            "file.save_as" => "Cmd+Shift+S",
            "file.print_preview" => "Cmd+P",
            "file.export_pdf" => "Cmd+E",
            "file.export_pptx" => "Cmd+Shift+E",
            "edit.undo" => "Cmd+Z",
            "edit.redo" => "Cmd+Shift+Z",
            "edit.cut" => "Cmd+X",
            "edit.copy" => "Cmd+C",
            "edit.paste" => "Cmd+V",
            "edit.duplicate" => "Cmd+D",
            "edit.select_all" => "Cmd+A",
            "edit.find" => "Cmd+F",
            "edit.replace" => "Cmd+H",
            "slide.new" => "Cmd+Shift+N",
            "slide.duplicate" => "Cmd+Shift+D",
            "slide.move_up" => "Cmd+Opt+Up",
            "slide.move_down" => "Cmd+Opt+Down",
            "format.bold" => "Cmd+B",
            "format.italic" => "Cmd+I",
            "format.underline" => "Cmd+U",
            "format.strikethrough" => "Cmd+Shift+X",
            "format.font_increase" => "Cmd+]",
            "format.font_decrease" => "Cmd+[",
            "view.zoom_in" => "Cmd+=",
            "view.zoom_out" => "Cmd+-",
            "view.zoom_reset" => "Cmd+0",
            "view.preview" => "F5",
            "view.presenter" => "Shift+F5",
            _ => "",
        }
    } else {
        match id {
            "app.palette" => "Ctrl+K",
            "help.welcome_guide" => "Ctrl+Shift+/",
            "file.start_hub" => "Ctrl+Shift+O",
            "file.new" => "Ctrl+N",
            "file.open" => "Ctrl+O",
            "file.save" => "Ctrl+S",
            "file.save_as" => "Ctrl+Shift+S",
            "file.print_preview" => "Ctrl+P",
            "file.export_pdf" => "Ctrl+E",
            "file.export_pptx" => "Ctrl+Shift+E",
            "edit.undo" => "Ctrl+Z",
            "edit.redo" => "Ctrl+Y",
            "edit.cut" => "Ctrl+X",
            "edit.copy" => "Ctrl+C",
            "edit.paste" => "Ctrl+V",
            "edit.duplicate" => "Ctrl+D",
            "edit.select_all" => "Ctrl+A",
            "edit.find" => "Ctrl+F",
            "edit.replace" => "Ctrl+H",
            "slide.new" => "Ctrl+Shift+N",
            "slide.duplicate" => "Ctrl+Shift+D",
            "slide.move_up" => "Ctrl+Alt+Up",
            "slide.move_down" => "Ctrl+Alt+Down",
            "format.bold" => "Ctrl+B",
            "format.italic" => "Ctrl+I",
            "format.underline" => "Ctrl+U",
            "format.strikethrough" => "Ctrl+Shift+X",
            "format.font_increase" => "Ctrl+]",
            "format.font_decrease" => "Ctrl+[",
            "view.zoom_in" => "Ctrl+=",
            "view.zoom_out" => "Ctrl+-",
            "view.zoom_reset" => "Ctrl+0",
            "view.preview" => "F5",
            "view.presenter" => "Shift+F5",
            _ => "",
        }
    }
}

/// Dispatch a Present command ID to the appropriate app callback or module.
pub(crate) fn dispatch_command(app: &PresentApp, id: &str) -> bool {
    if app.get_pptx_import_warning_open() {
        return is_present_menu_command(id);
    }
    match id {
        "file.new" => app.invoke_new_deck(),
        "file.new_sample" => app.invoke_new_sample_deck(),
        "file.start_hub" => start_hub::open(app),
        "file.templates" => {
            app.set_theme_selected(0);
            app.set_theme_category(0);
            app.set_theme_chooser_open(true);
        }
        "file.open" => app.invoke_open_deck(),
        "file.save" => app.invoke_save_deck(),
        "file.save_as" => app.invoke_save_as_deck(),
        "file.print_preview" => app.invoke_open_print_preview(),
        "file.export_pdf" => app.invoke_export_pdf(),
        "file.export_pptx" => app.invoke_export_pptx(),
        "edit.undo" => app.invoke_undo(),
        "edit.redo" => app.invoke_redo(),
        "edit.cut" => app.invoke_cut(),
        "edit.copy" => app.invoke_copy(),
        "edit.paste" => app.invoke_paste(),
        "edit.duplicate" => app.invoke_duplicate_selection(),
        "edit.select_all" => app.invoke_select_all_elements(),
        "edit.find" | "edit.replace" => find_replace_ui::open(app),
        "insert.text_box" => app.invoke_add_text(),
        "insert.shape_rect" => app.invoke_add_shape(),
        "insert.shape_circle" => app.invoke_add_shape_kind("circle".into()),
        "insert.shape_rounded_rect" => app.invoke_add_shape_kind("rounded-rect".into()),
        "insert.shape_arrow" => app.invoke_add_shape_kind("arrow".into()),
        "insert.shape_line" => app.invoke_add_shape_kind("line".into()),
        "insert.shape_callout" => app.invoke_add_shape_kind("callout".into()),
        "insert.table" | "slide.insert_table" => app.invoke_add_table(),
        "insert.chart" | "slide.insert_chart" => app.invoke_add_chart(),
        "insert.image" | "slide.insert_image" => app.invoke_add_picture(),
        "table.delete_row" | "slide.delete_table_row" => {
            app.invoke_table_action("delete-row".into())
        }
        "table.delete_column" | "slide.delete_table_column" => {
            app.invoke_table_action("delete-col".into())
        }
        "slide.new" => app.invoke_add_slide(),
        "slide.duplicate" => app.invoke_duplicate_slide(),
        "slide.delete" => app.invoke_delete_slide(),
        "slide.prev" => app.invoke_prev_slide(),
        "slide.next" => app.invoke_next_slide(),
        "format.bold" => element_style_ui::request(app, "bold", 0),
        "format.italic" => element_style_ui::request(app, "italic", 0),
        "format.underline" => element_style_ui::request(app, "underline", 0),
        "format.strikethrough" => element_style_ui::request(app, "strikethrough", 0),
        "format.font_increase" => element_style_ui::request(app, "size", 1),
        "format.font_decrease" => element_style_ui::request(app, "size", -1),
        "arrange.bring_front" => app
            .global::<ElementInk>()
            .invoke_arrange_requested("front".into()),
        "arrange.bring_forward" => app
            .global::<ElementInk>()
            .invoke_arrange_requested("forward".into()),
        "arrange.send_backward" => app
            .global::<ElementInk>()
            .invoke_arrange_requested("backward".into()),
        "arrange.send_back" => app
            .global::<ElementInk>()
            .invoke_arrange_requested("back".into()),
        "view.inspector" => app.invoke_toggle_inspector(),
        "view.preview" => app.invoke_toggle_preview_mode(),
        "view.presenter" => app.invoke_open_presenter(),
        "view.slide_sorter" => app.global::<ElementInk>().invoke_toggle_slide_sorter(),
        "format.copy_style" => app.global::<ElementInk>().invoke_copy_format_style(),
        "format.apply_style" => app.global::<ElementInk>().invoke_apply_format_style(),
        "view.zoom_in" => {
            let z = (app.get_zoom_index() - 1).max(0);
            app.set_zoom_index(z);
        }
        "view.zoom_out" => {
            let z = (app.get_zoom_index() + 1).min(2);
            app.set_zoom_index(z);
        }
        "view.zoom_reset" => app.set_zoom_index(0),
        "help.welcome_guide" => welcome_guide::open(app),
        "app.palette" => app.invoke_open_palette(),
        id if view_state::dispatch(app, id) => {}
        id if appearance::dispatch(app, id) => {}
        id if window_chrome::dispatch(app, id) => {}
        id if slide_order::dispatch(app, id) => {}
        _ => return false,
    }
    true
}

/// Dispatch a keyboard shortcut to the matching Present command.
pub(crate) fn dispatch_shortcut(
    app: &PresentApp,
    key: &str,
    shift: bool,
    primary: bool,
    alt: bool,
) -> bool {
    // F-keys (no primary modifier required)
    if key == "\u{f704}" || key == "F1" {
        return dispatch_command(app, "app.palette");
    }
    if key == "\u{f708}" || key == "F5" {
        return dispatch_command(
            app,
            if shift {
                "view.presenter"
            } else {
                "view.preview"
            },
        );
    }
    if key == "\u{f709}" || key == "F6" {
        app.invoke_cycle_region(1, shift);
        return true;
    }

    if primary && alt {
        if key == "\u{f700}" || key == "UpArrow" {
            return dispatch_command(app, "slide.move_up");
        }
        if key == "\u{f701}" || key == "DownArrow" {
            return dispatch_command(app, "slide.move_down");
        }
        return false;
    }

    if !primary {
        return false;
    }

    let lower = key.to_ascii_lowercase();
    match lower.as_str() {
        "k" => dispatch_command(app, "app.palette"),
        "p" if shift => dispatch_command(app, "app.palette"),
        "p" => dispatch_command(app, "file.print_preview"),
        "/" | "?" if shift => dispatch_command(app, "help.welcome_guide"),
        "o" if shift => dispatch_command(app, "file.start_hub"),
        "o" => dispatch_command(app, "file.open"),
        "n" if shift => dispatch_command(app, "slide.new"),
        "n" => dispatch_command(app, "file.new"),
        "s" if shift => dispatch_command(app, "file.save_as"),
        "s" => dispatch_command(app, "file.save"),
        "e" if shift => dispatch_command(app, "file.export_pptx"),
        "e" => dispatch_command(app, "file.export_pdf"),
        "f" => dispatch_command(app, "edit.find"),
        "h" => dispatch_command(app, "edit.replace"),
        "z" if shift => dispatch_command(app, "edit.redo"),
        "z" => dispatch_command(app, "edit.undo"),
        "y" => dispatch_command(app, "edit.redo"),
        "x" if shift => dispatch_command(app, "format.strikethrough"),
        "x" => dispatch_command(app, "edit.cut"),
        "c" => dispatch_command(app, "edit.copy"),
        "v" => dispatch_command(app, "edit.paste"),
        "d" if shift => dispatch_command(app, "slide.duplicate"),
        "d" => dispatch_command(app, "edit.duplicate"),
        "a" => dispatch_command(app, "edit.select_all"),
        "b" => dispatch_command(app, "format.bold"),
        "i" => dispatch_command(app, "format.italic"),
        "u" => dispatch_command(app, "format.underline"),
        "]" => dispatch_command(app, "format.font_increase"),
        "[" => dispatch_command(app, "format.font_decrease"),
        "=" | "+" => dispatch_command(app, "view.zoom_in"),
        "-" => dispatch_command(app, "view.zoom_out"),
        "0" => dispatch_command(app, "view.zoom_reset"),
        _ => false,
    }
}

/// Wire the Slint `dispatch-shortcut` callback on `PresentApp`.
pub(crate) fn wire_shortcuts(app: &PresentApp) {
    let app_ref = app.as_weak();
    app.on_dispatch_shortcut(move |key, shift, primary, alt| {
        let Some(app) = app_ref.upgrade() else {
            return false;
        };
        dispatch_shortcut(&app, key.as_str(), shift, primary, alt)
    });
}

/// Return true when `id` is a supported Present menu command.
pub(crate) fn is_present_menu_command(id: &str) -> bool {
    slide_order::is_command(id)
        || matches!(
            id,
            "file.new"
                | "file.new_sample"
                | "file.start_hub"
                | "file.templates"
                | "file.open"
                | "file.save"
                | "file.save_as"
                | "file.print_preview"
                | "file.export_pdf"
                | "file.export_pptx"
                | "edit.undo"
                | "edit.redo"
                | "edit.cut"
                | "edit.copy"
                | "edit.paste"
                | "edit.duplicate"
                | "edit.select_all"
                | "edit.find"
                | "edit.replace"
                | "insert.text_box"
                | "insert.shape_rect"
                | "insert.shape_circle"
                | "insert.shape_rounded_rect"
                | "insert.shape_arrow"
                | "insert.shape_line"
                | "insert.shape_callout"
                | "insert.table"
                | "insert.chart"
                | "insert.image"
                | "slide.insert_table"
                | "slide.insert_chart"
                | "slide.insert_image"
                | "table.delete_row"
                | "table.delete_column"
                | "slide.delete_table_row"
                | "slide.delete_table_column"
                | "slide.new"
                | "slide.duplicate"
                | "slide.delete"
                | "slide.prev"
                | "slide.next"
                | "format.bold"
                | "format.italic"
                | "format.underline"
                | "format.strikethrough"
                | "format.font_increase"
                | "format.font_decrease"
                | "arrange.bring_front"
                | "arrange.bring_forward"
                | "arrange.send_backward"
                | "arrange.send_back"
                | "view.inspector"
                | "view.navigator"
                | "view.notes"
                | "view.slide_sorter"
                | "view.preview"
                | "view.presenter"
                | "view.zoom_in"
                | "view.zoom_out"
                | "view.zoom_reset"
                | "view.appearance.system"
                | "view.appearance.light"
                | "view.appearance.dark"
                | "view.appearance.high_contrast"
                | "view.window-frame.system"
                | "view.window-frame.loom"
                | "help.welcome_guide"
                | "app.palette"
        )
}

pub(crate) fn schedule_menu_action(
    app_ref: &slint::Weak<PresentApp>,
    action: CommandAction,
) -> Result<(), DesktopError> {
    if !is_present_menu_command(&action.id) {
        return Err(DesktopError::InvalidRequest(format!(
            "unsupported Present menu command {}",
            action.id
        )));
    }
    let id = action.id;
    let error_id = id.clone();
    app_ref
        .upgrade_in_event_loop(move |app| {
            dispatch_scheduled_menu_action(&app, &id);
        })
        .map_err(|error| {
            DesktopError::InvalidRequest(format!(
                "failed to schedule Present menu command {error_id}: {error}"
            ))
        })
}

pub(crate) fn dispatch_scheduled_menu_action(app: &PresentApp, id: &str) {
    if !dispatch_command(app, id) {
        set_status(app, format!("Unsupported menu command: {id}"));
    }
}
