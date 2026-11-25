//! Shared command dispatch and keyboard shortcut routing for Loom Writer.
//!
//! Every user-facing surface (toolbar, toolbar menus, native/in-window menu bar,
//! Command Palette, keyboard shortcuts, and Welcome Guide) dispatches through
//! [`dispatch_command`] and derives platform shortcut hints from
//! [`shortcut_for_id`].

use slint::ComponentHandle;

use crate::{
    appearance, formatting_actions, list_level_actions, local_menu, picture_actions,
    toolbar_commands, view_state, window_chrome, FindBar, PrintPreviewState, WordCountState,
    WriterApp, WriterFootnoteState, WriterHeaderFooterState, WriterReviewState,
};

pub(crate) const PRIMARY_MOD: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

const fn sc(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

/// Platform-native shortcut label for a canonical command identifier.
pub(crate) fn shortcut_for_id(id: &str) -> &'static str {
    match id {
        "file.new" | "writer.new" => sc("Cmd+N", "Ctrl+N"),
        "file.start_hub" => sc("Cmd+Shift+O", "Ctrl+Shift+O"),
        "file.open" | "writer.open" => sc("Cmd+O", "Ctrl+O"),
        "file.save" | "writer.save" => sc("Cmd+S", "Ctrl+S"),
        "file.save_as" | "writer.save-as" => sc("Cmd+Shift+S", "Ctrl+Shift+S"),
        "file.print_preview" => sc("Cmd+P", "Ctrl+P"),
        "file.export_pdf" | "writer.export-pdf" => sc("Cmd+E", "Ctrl+E"),
        "file.export_docx" | "writer.export-docx" => sc("Cmd+Shift+D", "Ctrl+Shift+D"),
        "app.quit" | "loom writer.quit" => sc("Cmd+Q", "Ctrl+Q"),
        "edit.undo" | "writer.undo" => sc("Cmd+Z", "Ctrl+Z"),
        "edit.redo" | "writer.redo" => sc("Cmd+Shift+Z", "Ctrl+Shift+Z"),
        "edit.cut" | "writer.cut" => sc("Cmd+X", "Ctrl+X"),
        "edit.copy" | "writer.copy" => sc("Cmd+C", "Ctrl+C"),
        "edit.paste" | "writer.paste" => sc("Cmd+V", "Ctrl+V"),
        "edit.find" => sc("Cmd+F", "Ctrl+F"),
        "edit.replace" => sc("Cmd+H", "Ctrl+H"),
        "app.palette" => sc("Cmd+K", "Ctrl+K"),
        "help.shortcuts" => sc("Cmd+Shift+P", "Ctrl+Shift+P"),
        "help.welcome" => sc("Cmd+Shift+/", "Ctrl+Shift+/"),
        "view.inspector" => sc("Cmd+Opt+I", "Ctrl+Alt+I"),
        "view.navigator" => sc("Cmd+Opt+O", "Ctrl+Alt+O"),
        "view.zoom_in" => sc("Cmd+=", "Ctrl+="),
        "view.zoom_out" => sc("Cmd+-", "Ctrl+-"),
        "view.zoom_actual" => sc("Cmd+0", "Ctrl+0"),
        "format.bold" | "writer.style.bold" | "writer.style.bold-all" => sc("Cmd+B", "Ctrl+B"),
        "format.italic" | "writer.style.italic" | "writer.style.italic-all" => {
            sc("Cmd+I", "Ctrl+I")
        }
        "format.copy_style" => sc("Cmd+Opt+C", "Ctrl+Alt+C"),
        "format.apply_style" => sc("Cmd+Opt+V", "Ctrl+Alt+V"),
        "insert.hyperlink" => sc("Cmd+Shift+K", "Ctrl+Shift+K"),
        "format.underline" | "writer.style.underline" | "writer.style.underline-all" => {
            sc("Cmd+U", "Ctrl+U")
        }
        "format.strikethrough" => sc("Cmd+Shift+X", "Ctrl+Shift+X"),
        "writer.style.superscript" => sc("Cmd+Shift+.", "Ctrl+Shift+."),
        "writer.style.subscript" => sc("Cmd+Shift+,", "Ctrl+Shift+,"),
        "format.align_left" | "writer.align.left" | "writer.align.left-all" => {
            sc("Cmd+Shift+L", "Ctrl+Shift+L")
        }
        "format.align_center" | "writer.align.center" | "writer.align.center-all" => {
            sc("Cmd+Shift+E", "Ctrl+Shift+E")
        }
        "format.align_right" | "writer.align.right" | "writer.align.right-all" => {
            sc("Cmd+Shift+R", "Ctrl+Shift+R")
        }
        "format.align_justify" | "writer.align.justify" | "writer.align.justify-all" => {
            sc("Cmd+Shift+J", "Ctrl+Shift+J")
        }
        "paragraph.body" | "writer.heading.body" | "writer.style.body-all" => {
            sc("Cmd+Opt+0", "Ctrl+Alt+0")
        }
        "paragraph.h1" | "writer.heading.h1" | "writer.style.heading-1-all" => {
            sc("Cmd+Opt+1", "Ctrl+Alt+1")
        }
        "paragraph.h2" | "writer.heading.h2" | "writer.style.heading-2-all" => {
            sc("Cmd+Opt+2", "Ctrl+Alt+2")
        }
        "paragraph.h3" | "writer.heading.h3" | "writer.style.heading-3-all" => {
            sc("Cmd+Opt+3", "Ctrl+Alt+3")
        }
        "list.numbered" => sc("Cmd+Shift+7", "Ctrl+Shift+7"),
        "list.bulleted" => sc("Cmd+Shift+8", "Ctrl+Shift+8"),
        "insert.page_break" => sc("Cmd+Enter", "Ctrl+Enter"),
        "insert.line_break" => "Shift+Enter",
        "insert.header_footer" => sc("Cmd+Opt+H", "Ctrl+Alt+H"),
        "writer.footnotes" => sc("Cmd+Opt+F", "Ctrl+Alt+F"),
        "list.level.increase" => "Tab",
        "list.level.decrease" => "Shift+Tab",
        "table.insert_row_above" | "writer.table.insert_row_above" => {
            sc("Cmd+Opt+Up", "Ctrl+Alt+Up")
        }
        "table.insert_row_below" | "writer.table.insert_row_below" => {
            sc("Cmd+Opt+Down", "Ctrl+Alt+Down")
        }
        "table.insert_col_left" | "writer.table.insert_col_left" => {
            sc("Cmd+Opt+Left", "Ctrl+Alt+Left")
        }
        "table.insert_col_right" | "writer.table.insert_col_right" => {
            sc("Cmd+Opt+Right", "Ctrl+Alt+Right")
        }
        "table.delete_row" | "writer.table.delete_row" => {
            sc("Cmd+Opt+Backspace", "Ctrl+Alt+Backspace")
        }
        "table.delete_table" | "writer.table.delete_table" => {
            sc("Cmd+Opt+Shift+Backspace", "Ctrl+Alt+Shift+Backspace")
        }
        "table.merge_cells" | "writer.table.merge_cells" => sc("Cmd+Opt+M", "Ctrl+Alt+M"),
        "table.split_cell" | "writer.table.split_cell" => sc("Cmd+Opt+S", "Ctrl+Alt+S"),
        "writer.review.open" => sc("Cmd+Shift+T", "Ctrl+Shift+T"),
        "review.word_count" => sc("Cmd+Shift+C", "Ctrl+Shift+C"),
        _ => "",
    }
}

/// Returns true when `query` looks like a keyboard shortcut expression rather than plain prose.
pub(crate) fn is_shortcut_query(query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    q.contains('+') || q == "f1" || q == "tab"
}

fn normalize_shortcut_tokens(text: &str) -> String {
    text.trim()
        .to_ascii_lowercase()
        .replace("cmd+", "primary+")
        .replace("ctrl+", "primary+")
        .replace("opt+", "alt+")
}

/// Matches a command shortcut string against a normalized shortcut search query.
pub(crate) fn shortcut_matches_query(shortcut: &str, query: &str) -> bool {
    if shortcut.is_empty() || !is_shortcut_query(query) {
        return false;
    }
    let haystack = normalize_shortcut_tokens(shortcut);
    let needle = normalize_shortcut_tokens(query);
    !needle.is_empty() && haystack.contains(&needle)
}

/// Dispatch one canonical command identifier through the callbacks shared by
/// toolbar, command palette, keyboard, Welcome Guide, and menu surfaces.
pub(crate) fn dispatch_command(app: &WriterApp, id: &str) -> bool {
    match id {
        "app.quit" | "loom writer.quit" => app.invoke_application_quit_requested(),
        "file.new" | "writer.new" => app.invoke_new_doc(),
        "file.start_hub" => crate::start_hub::open_hub(app),
        "file.templates" => app.invoke_new_doc(),
        "file.open" | "writer.open" => app.invoke_open_doc(),
        "file.save" | "writer.save" => app.invoke_save_doc(),
        "file.save_as" | "writer.save-as" => app.invoke_save_as_doc(),
        "file.print_preview" => app.global::<PrintPreviewState>().invoke_open_preview(),
        "file.export_pdf" | "writer.export-pdf" => app.invoke_export_pdf(),
        "file.export_docx" | "writer.export-docx" => app.invoke_export_docx(),
        "file.export_md" => app.invoke_export_markdown(),
        "edit.undo" | "writer.undo" => app.invoke_undo(),
        "edit.redo" | "writer.redo" => app.invoke_redo(),
        "edit.cut" | "writer.cut" => app.invoke_cut(),
        "edit.copy" | "writer.copy" => app.invoke_copy(),
        "edit.paste" | "writer.paste" => app.invoke_paste(),
        "edit.find" => app.global::<FindBar>().invoke_open_requested(false),
        "edit.replace" => app.global::<FindBar>().invoke_open_requested(true),
        "format.copy_style" => app
            .global::<crate::WriterFormatState>()
            .invoke_format_painter_copy(),
        "format.copy_style_locked" => app
            .global::<crate::WriterFormatState>()
            .invoke_format_painter_copy_locked(),
        "format.apply_style" => app
            .global::<crate::WriterFormatState>()
            .invoke_format_painter_apply(),
        "insert.hyperlink" => app
            .global::<crate::WriterFormatState>()
            .invoke_hyperlink_open_requested(),
        "app.palette" | "help.shortcuts" => app.invoke_open_palette(),
        "help.welcome" => crate::welcome_guide::open(app),
        "review.word_count" => app.global::<WordCountState>().invoke_open_dialog(),
        "writer.review.open" => {
            let review = app.global::<WriterReviewState>();
            review.set_panel_open(true);
            review.invoke_navigate(0);
        }
        "insert.header_footer" => app.global::<WriterHeaderFooterState>().invoke_open_editor(),
        "insert.toc" => app.global::<crate::WriterFormatState>().invoke_insert_toc(),
        "view.ruler" => {
            let fmt = app.global::<crate::WriterFormatState>();
            fmt.set_show_ruler(!fmt.get_show_ruler());
        }
        "insert.comment" => app.invoke_open_comment_composer(),
        "writer.footnotes" => app.global::<WriterFootnoteState>().invoke_open_editor(),
        "insert.table" | "writer.table.insert" => app.invoke_insert_table(),
        "insert.chart" | "writer.chart.insert" => app.invoke_insert_chart(),
        "insert.image" | "writer.image.insert" => app.invoke_insert_image(),
        "picture.larger" | "picture.smaller" | "picture.reset_size" | "picture.alt_text" => {
            picture_actions::dispatch(app, id);
        }
        id if list_level_actions::dispatch(app, id) => {}
        id if formatting_actions::dispatch(app, id) => {}
        "format.bold" | "writer.style.bold" | "writer.style.bold-all" => app.invoke_toggle_bold(),
        "format.italic" | "writer.style.italic" | "writer.style.italic-all" => {
            app.invoke_toggle_italic()
        }
        "format.underline" | "writer.style.underline" | "writer.style.underline-all" => {
            app.invoke_toggle_underline()
        }
        "format.strikethrough" => app.invoke_toggle_strikethrough(),
        "format.align_left" => app.invoke_select_alignment(0),
        "format.align_center" => app.invoke_select_alignment(1),
        "format.align_right" => app.invoke_select_alignment(2),
        "format.align_justify" => app.invoke_select_alignment(3),
        "paragraph.body" => app.invoke_select_heading(0),
        "paragraph.h1" => app.invoke_select_heading(1),
        "paragraph.h2" => app.invoke_select_heading(2),
        "paragraph.h3" => app.invoke_select_heading(3),
        "list.none" => app.invoke_select_list_style(0),
        "list.bulleted" | "list.bullet.disc" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(1, 0),
        "list.bullet.circle" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(1, 1),
        "list.bullet.square" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(1, 2),
        "list.bullet.dash" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(1, 3),
        "list.numbered" | "list.number.decimal" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(2, 0),
        "list.number.lower_alpha" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(2, 1),
        "list.number.upper_alpha" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(2, 2),
        "list.number.lower_roman" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(2, 3),
        "list.number.upper_roman" => app
            .global::<crate::WriterFormatState>()
            .invoke_apply_list_preset(2, 4),
        "view.zoom_in" => toolbar_commands::zoom_in(app),
        "view.zoom_out" => toolbar_commands::zoom_out(app),
        "view.zoom_actual" => toolbar_commands::zoom_actual(app),
        "view.zoom_fit_width" => toolbar_commands::fit_width(app),
        "view.zoom_fit_page" => toolbar_commands::fit_page_view(app),
        "table.insert_row_above" | "writer.table.insert_row_above" => {
            app.invoke_table_action("insert-row-above".into())
        }
        "table.insert_row_below" | "writer.table.insert_row_below" => {
            app.invoke_table_action("insert-row-below".into())
        }
        "table.insert_col_left" | "writer.table.insert_col_left" => {
            app.invoke_table_action("insert-col-left".into())
        }
        "table.insert_col_right" | "writer.table.insert_col_right" => {
            app.invoke_table_action("insert-col-right".into())
        }
        "table.delete_row" | "writer.table.delete_row" => {
            app.invoke_table_action("delete-row".into())
        }
        "table.delete_column" | "table.delete_col" | "writer.table.delete_col" => {
            app.invoke_table_action("delete-col".into())
        }
        "table.delete_table" | "writer.table.delete_table" => {
            app.invoke_table_action("delete-table".into())
        }
        "table.merge_cells" | "writer.table.merge_cells" => {
            app.invoke_table_action("merge-cells".into())
        }
        "table.split_cell" | "writer.table.split_cell" => {
            app.invoke_table_action("split-cell".into())
        }
        "view.inspector" => app.invoke_toggle_inspector(),
        id if view_state::dispatch(app, id) => {}
        id if appearance::dispatch(app, id) => {}
        id if window_chrome::dispatch(app, id) => {}
        _ => return false,
    }
    true
}

/// Dispatch one keyboard chord captured by `WriterShortcutScope` (`F1` with no modifiers
/// or a primary-modifier shortcut chord with optional Shift/Alt).
pub(crate) fn dispatch_shortcut(app: &WriterApp, key: &str, shift: bool, alt: bool) -> bool {
    let lower = key.to_ascii_lowercase();
    let command_id = match (lower.as_str(), shift, alt) {
        ("f1", false, false) | ("/", true, false) | ("?", _, false) => "help.welcome",
        ("k", false, false) | ("p", true, false) => "app.palette",
        ("n", false, false) => "file.new",
        ("o", false, false) => "file.open",
        ("o", true, false) => "file.start_hub",
        ("s", false, false) => "file.save",
        ("s", true, false) => "file.save_as",
        ("p", false, false) => "file.print_preview",
        ("e", false, false) => "file.export_pdf",
        ("d", true, false) => "file.export_docx",
        ("q", false, false) => local_menu::QUIT_COMMAND_ID,
        ("z", false, false) => "edit.undo",
        ("z", true, false) | ("y", false, false) => "edit.redo",
        ("f", false, false) => "edit.find",
        ("h", false, false) => "edit.replace",
        ("b", false, false) => "format.bold",
        ("i", false, false) => "format.italic",
        ("u", false, false) => "format.underline",
        ("x", true, false) => "format.strikethrough",
        (".", true, false) | (">", _, false) => "writer.style.superscript",
        (",", true, false) | ("<", _, false) => "writer.style.subscript",
        ("l", true, false) => "format.align_left",
        ("e", true, false) => "format.align_center",
        ("r", true, false) => "format.align_right",
        ("j", true, false) => "format.align_justify",
        ("7", true, false) | ("&", _, false) => "list.numbered",
        ("8", true, false) | ("*", _, false) => "list.bulleted",
        ("0", false, true) => "paragraph.body",
        ("1", false, true) => "paragraph.h1",
        ("2", false, true) => "paragraph.h2",
        ("3", false, true) => "paragraph.h3",
        ("=", _, false) | ("+", _, false) => "view.zoom_in",
        ("-", false, false) | ("_", _, false) => "view.zoom_out",
        ("0", false, false) => "view.zoom_actual",
        ("h", false, true) => "insert.header_footer",
        ("f", false, true) => "writer.footnotes",
        ("i", false, true) => "view.inspector",
        ("o", false, true) => "view.navigator",
        ("c", true, false) => "review.word_count",
        ("t", true, false) => "writer.review.open",
        _ => return false,
    };
    dispatch_command(app, command_id)
}

/// Connect `WriterApp::shortcut-pressed` to [`dispatch_shortcut`].
pub(crate) fn wire_shortcuts(app: &WriterApp) {
    let app_ref = app.as_weak();
    app.on_shortcut_pressed(move |key, shift, alt| {
        app_ref
            .upgrade()
            .is_some_and(|app| dispatch_shortcut(&app, key.as_str(), shift, alt))
    });
}
