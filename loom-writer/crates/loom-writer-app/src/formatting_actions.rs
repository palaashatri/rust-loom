//! Commands and selection-derived state for Writer's paragraph and inline
//! formatting controls.

use std::rc::Rc;

use loom_command::{CommandInvocation, CommandSpec, InvocationSource};
use loom_text::ParagraphDirection;
use loom_writer_core::{TextSelection, WriterDocument};
use slint::{ComponentHandle, SharedString};

use crate::document_formatting::{formatting_state_for_selection, DocumentSelection};
use crate::{
    apply_with_history, font_picker, inline_formatting, paragraph_alignment, table_actions,
    typing_style, GuiState, HistoryKind, WriterApp, WriterFormatState,
};

pub(crate) fn set_selection_strikethrough(
    document: &mut WriterDocument,
    selection: TextSelection,
    enabled: bool,
) {
    inline_formatting::set_selection_strikethrough(
        document,
        DocumentSelection::range(selection.anchor, selection.focus),
        enabled,
    );
    table_actions::table_format_actions::set_selection_emphasis(
        document,
        selection,
        table_actions::table_format_actions::Emphasis::Strikethrough,
        enabled,
    );
}

/// Register formatting actions on the selected-format state global.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    wire_direction(app, state);
    wire_logical_alignment(app, state);
    wire_script_toggle(app, state, true);
    wire_script_toggle(app, state, false);
    wire_color_dialog(app, state);
    wire_strikethrough(app, state);
}

fn wire_strikethrough(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_toggle_strikethrough(move || {
        let Some(app) = app_ref.upgrade() else { return };
        let enabled = app.get_is_strikethrough();
        let selection = crate::selection_from_app(&app);
        if selection.is_collapsed() {
            app.set_status_right("Select text to apply strikethrough".into());
            return;
        }
        let mut next = state.current.borrow().clone();
        set_selection_strikethrough(&mut next, selection.clone(), !enabled);
        next.set_selection(selection);
        if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
            return;
        }
        announce(
            &app,
            if enabled {
                "Strikethrough removed"
            } else {
                "Strikethrough applied"
            }
            .to_owned(),
        );
    });
}

fn wire_direction(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.global::<WriterFormatState>()
        .on_select_direction(move |index| {
            let Some(app) = app_ref.upgrade() else { return };
            let Some(direction) = direction_for_index(index) else {
                app.set_status_right("Unsupported paragraph direction".into());
                return;
            };
            let id = direction_command_id(direction);
            if !command_allowed(&app, &state, id) {
                return;
            }
            let selection = state.current.borrow().selection();
            let mut next = state.current.borrow().clone();
            let result = paragraph_alignment::set_selection_direction(
                &mut next,
                selection.clone(),
                direction,
            );
            next.set_selection(selection);
            if next != *state.current.borrow() {
                if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
                    return;
                }
            }
            announce(
                &app,
                paragraph_alignment::format_result_announcement(
                    paragraph_alignment::direction_name(index),
                    result,
                ),
            );
            sync_menu(&app, &state);
        });
}

fn wire_logical_alignment(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.global::<WriterFormatState>()
        .on_select_logical_alignment(move |index| {
            let Some(app) = app_ref.upgrade() else { return };
            let Some(command_id) = logical_alignment_command_id(index) else {
                app.set_status_right("Unsupported logical alignment".into());
                return;
            };
            if !command_allowed(&app, &state, command_id) {
                return;
            }
            let selection = state.current.borrow().selection();
            let mut next = state.current.borrow().clone();
            let Some((result, alignment)) = paragraph_alignment::set_selection_logical_alignment(
                &mut next,
                selection.clone(),
                index,
            ) else {
                return;
            };
            next.set_selection(selection);
            if next != *state.current.borrow() {
                if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
                    return;
                }
            }
            announce(
                &app,
                paragraph_alignment::format_result_announcement(
                    match alignment {
                        loom_text::Alignment::Start => "Start alignment",
                        _ => "End alignment",
                    },
                    result,
                ),
            );
            sync_menu(&app, &state);
        });
}

fn wire_script_toggle(app: &WriterApp, state: &Rc<GuiState>, superscript: bool) {
    let state = state.clone();
    let app_ref = app.as_weak();
    let global = app.global::<WriterFormatState>();
    if superscript {
        global.on_toggle_superscript(move || {
            if let Some(app) = app_ref.upgrade() {
                toggle_script(&app, &state, true);
            }
        });
    } else {
        global.on_toggle_subscript(move || {
            if let Some(app) = app_ref.upgrade() {
                toggle_script(&app, &state, false);
            }
        });
    }
}

fn toggle_script(app: &WriterApp, state: &GuiState, superscript: bool) {
    let command_id = if superscript {
        "writer.style.superscript"
    } else {
        "writer.style.subscript"
    };
    if !command_allowed(app, state, command_id) {
        return;
    }
    let selection = state.current.borrow().selection();
    let selection_range = DocumentSelection::range(selection.anchor, selection.focus);
    let inline =
        inline_formatting::selection_state(&state.current.borrow(), selection_range.clone());
    let currently_enabled = if superscript {
        inline.superscript && !inline.superscript_mixed
    } else {
        inline.subscript && !inline.subscript_mixed
    };
    let enabled = !currently_enabled;

    if selection.is_collapsed() && table_actions::get_table_cell_selection_range().is_none() {
        let pending = if superscript {
            typing_style::toggle_superscript(selection.focus, inline.superscript, inline.subscript)
        } else {
            typing_style::toggle_subscript(selection.focus, inline.subscript, inline.superscript)
        };
        let global = app.global::<WriterFormatState>();
        global.set_superscript_selected(pending.superscript.unwrap_or(inline.superscript));
        global.set_superscript_mixed(false);
        global.set_subscript_selected(pending.subscript.unwrap_or(inline.subscript));
        global.set_subscript_mixed(false);
        announce(
            app,
            format!(
                "{} {} for new text",
                if superscript {
                    "Superscript"
                } else {
                    "Subscript"
                },
                if enabled { "on" } else { "off" }
            ),
        );
        return;
    }

    let mut next = state.current.borrow().clone();
    if superscript {
        inline_formatting::set_selection_superscript(&mut next, selection_range.clone(), enabled);
        table_actions::table_format_actions::set_selection_superscript(
            &mut next,
            selection.clone(),
            enabled,
        );
    } else {
        inline_formatting::set_selection_subscript(&mut next, selection_range, enabled);
        table_actions::table_format_actions::set_selection_subscript(
            &mut next,
            selection.clone(),
            enabled,
        );
    }
    next.set_selection(selection.clone());
    if next != *state.current.borrow() {
        if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
            return;
        }
    }
    let label = if superscript {
        "Superscript"
    } else {
        "Subscript"
    };
    let action = if enabled {
        "applied to"
    } else {
        "removed from"
    };
    announce(
        app,
        with_skipped_items(
            format!("{label} {action} selected text"),
            unsupported_selection_items(&state.current.borrow(), &selection),
        ),
    );
    sync_menu(app, state);
}

fn wire_color_dialog(app: &WriterApp, state: &Rc<GuiState>) {
    let global = app.global::<WriterFormatState>();
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_open_foreground_color(move || {
            let Some(app) = app_ref.upgrade() else { return };
            if !command_allowed(&app, &state, "writer.style.color") {
                return;
            }
            let document = state.current.borrow();
            let selection = document.selection();
            let inline = inline_formatting::selection_state(
                &document,
                DocumentSelection::range(selection.anchor, selection.focus),
            );
            let global = app.global::<WriterFormatState>();
            global.set_color_error(SharedString::new());
            global.set_color_input(SharedString::from(
                inline
                    .foreground_color
                    .unwrap_or_else(|| "#000000FF".to_string()),
            ));
            global.set_color_dialog_open(true);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_apply_foreground_color(move || {
            let Some(app) = app_ref.upgrade() else { return };
            apply_foreground_color(&app, &state);
        });
    }
    {
        let app_ref = app.as_weak();
        global.on_cancel_foreground_color(move || {
            if let Some(app) = app_ref.upgrade() {
                let global = app.global::<WriterFormatState>();
                global.set_color_dialog_open(false);
                global.set_color_error(SharedString::new());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        global.on_clear_foreground_color_error(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<WriterFormatState>()
                    .set_color_error(SharedString::new());
            }
        });
    }
}

fn apply_foreground_color(app: &WriterApp, state: &GuiState) {
    if !command_allowed(app, state, "writer.style.color") {
        return;
    }
    let global = app.global::<WriterFormatState>();
    let Some(color) = parse_foreground_color(global.get_color_input().as_str()) else {
        let message = "Enter #RRGGBB or #RRGGBBAA using hexadecimal digits";
        global.set_color_error(SharedString::from(message));
        app.set_status_right(SharedString::from(message));
        return;
    };

    let selection = state.current.borrow().selection();
    if selection.is_collapsed() && table_actions::get_table_cell_selection_range().is_none() {
        typing_style::set_foreground_color(selection.focus, &color);
        global.set_foreground_color(SharedString::from(color));
        global.set_foreground_color_mixed(false);
        global.set_color_dialog_open(false);
        global.set_color_error(SharedString::new());
        announce(app, "Foreground color set for new text".to_string());
        return;
    }

    let mut next = state.current.borrow().clone();
    inline_formatting::set_selection_foreground_color(
        &mut next,
        DocumentSelection::range(selection.anchor, selection.focus),
        &color,
    );
    table_actions::table_format_actions::set_selection_foreground_color(
        &mut next,
        selection.clone(),
        &color,
    );
    next.set_selection(selection.clone());
    if next != *state.current.borrow() {
        if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
            return;
        }
    }
    global.set_color_dialog_open(false);
    global.set_color_error(SharedString::new());
    announce(
        app,
        with_skipped_items(
            format!("Foreground color {color} applied to selected text"),
            unsupported_selection_items(&state.current.borrow(), &selection),
        ),
    );
    sync_menu(app, state);
}

fn parse_foreground_color(input: &str) -> Option<String> {
    let hex = input.trim().strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(if hex.len() == 6 {
        format!("#{}FF", hex.to_ascii_uppercase())
    } else {
        format!("#{}", hex.to_ascii_uppercase())
    })
}

/// Publish all selection-derived fields through the same model projection for
/// initial load, edits, undo/redo, and pointer/keyboard selection changes.
pub(crate) fn project_document_controls(app: &WriterApp, document: &WriterDocument) {
    let selection = document.selection();
    let range = DocumentSelection::range(selection.anchor, selection.focus);
    let mut formatting = formatting_state_for_selection(document, range.clone());
    if table_actions::table_format_actions::selection_is_table_only(document, selection.clone()) {
        let table_styles = table_actions::table_format_actions::selected_character_styles(
            document,
            selection.clone(),
        );
        if !table_styles.is_empty() {
            formatting.bold = table_styles
                .iter()
                .all(|style| style.weight == loom_text::FontWeight::Bold);
            formatting.italic = table_styles.iter().all(|style| style.italic);
            formatting.underline = table_styles.iter().all(|style| style.underline);
            formatting.strikethrough = table_styles.iter().all(|style| style.strikethrough);
            formatting.font_size_pt = table_styles[0].font_size;
        }
    }
    let pending = (selection.is_collapsed()
        && table_actions::get_table_cell_selection_range().is_none())
    .then(|| typing_style::at(selection.focus));
    if let Some(pending) = &pending {
        if let Some(value) = pending.bold {
            formatting.bold = value;
        }
        if let Some(value) = pending.italic {
            formatting.italic = value;
        }
        if let Some(value) = pending.underline {
            formatting.underline = value;
        }
        if let Some(value) = pending.strikethrough {
            formatting.strikethrough = value;
        }
    }
    let heading_level = inspector_heading_level(document, &selection);
    let (text_alignment, logical_alignment) =
        paragraph_alignment::alignment_indices(document, selection.clone());
    let mut inline = inline_formatting::selection_state(document, range);
    if let Some(pending) = pending {
        if let Some(value) = pending.superscript {
            inline.superscript = value;
            inline.superscript_mixed = false;
        }
        if let Some(value) = pending.subscript {
            inline.subscript = value;
            inline.subscript_mixed = false;
        }
        if let Some(value) = pending.color {
            inline.foreground_color = Some(value);
            inline.foreground_color_mixed = false;
        }
    }
    let global = app.global::<WriterFormatState>();

    app.set_is_bold(formatting.bold);
    app.set_is_italic(formatting.italic);
    app.set_is_underline(formatting.underline);
    app.set_is_strikethrough(formatting.strikethrough);
    font_picker::publish_current(app, document);
    app.set_font_size_pt(formatting.font_size_pt.round() as i32);
    app.set_font_size_value(SharedString::from(formatting.font_size_pt.to_string()));
    app.set_line_spacing_index(formatting.line_spacing_index);
    app.set_list_style_index(formatting.list_style_index);
    app.set_heading_level(heading_level);
    app.set_text_alignment(text_alignment);

    global.set_logical_alignment(logical_alignment);
    global.set_paragraph_direction(paragraph_alignment::direction_index(
        document,
        selection.clone(),
    ));
    global.set_paragraph_formatting_available(paragraph_alignment::selection_can_change_direction(
        document,
        selection.clone(),
    ));
    let inline_available = inline_formatting::can_format_selection(document, selection.clone())
        || table_actions::table_format_actions::can_format_selection(document, selection.clone());
    global.set_inline_formatting_available(inline_available);
    global.set_color_formatting_available(inline_available);
    global.set_superscript_selected(inline.superscript);
    global.set_superscript_mixed(inline.superscript_mixed);
    global.set_subscript_selected(inline.subscript);
    global.set_subscript_mixed(inline.subscript_mixed);
    global.set_foreground_color(SharedString::from(
        inline.foreground_color.unwrap_or_default(),
    ));
    global.set_foreground_color_mixed(inline.foreground_color_mixed);
    global.set_justify_available(paragraph_alignment::selection_can_be_justified(
        document,
        selection.clone(),
    ));
    table_actions::update_table_selection_properties(app, document, selection.anchor);
}

fn inspector_heading_level(document: &WriterDocument, selection: &TextSelection) -> i32 {
    let indices = loom_writer_core::selected_block_indices(document, selection.clone());
    let Some(first) = indices
        .first()
        .map(|&index| heading_level(&document.blocks[index].kind))
    else {
        return 0;
    };
    if indices
        .iter()
        .any(|&index| heading_level(&document.blocks[index].kind) != first)
    {
        -1
    } else {
        first
    }
}

fn heading_level(kind: &str) -> i32 {
    match kind {
        "heading1" => 1,
        "heading2" => 2,
        "heading3" => 3,
        _ => 0,
    }
}

fn unsupported_selection_items(document: &WriterDocument, selection: &TextSelection) -> usize {
    loom_writer_core::selected_block_indices(document, selection.clone())
        .into_iter()
        .filter(|&index| {
            let block = &document.blocks[index];
            block.kind != loom_writer_core::TABLE_BLOCK_KIND
                && !inline_formatting::is_text_paragraph(block)
        })
        .count()
}

fn with_skipped_items(message: String, skipped: usize) -> String {
    if skipped == 0 {
        return message;
    }
    let noun = if skipped == 1 {
        "unsupported item"
    } else {
        "unsupported items"
    };
    format!("{message}; skipped {skipped} {noun}")
}

pub(crate) fn sync_registry_enablement(
    registry: &mut loom_command::CommandRegistry,
    document: &WriterDocument,
) {
    let selection = document.selection();
    let paragraph_enabled =
        paragraph_alignment::selection_can_change_direction(document, selection.clone());
    for id in [
        "writer.direction.auto",
        "writer.direction.ltr",
        "writer.direction.rtl",
        "writer.align.start",
        "writer.align.end",
    ] {
        registry.set_enabled(&loom_command::CommandId::new(id), paragraph_enabled);
    }
    let inline_enabled = inline_formatting::can_format_selection(document, selection.clone())
        || table_actions::table_format_actions::can_format_selection(document, selection);
    for id in [
        "writer.style.color",
        "writer.style.superscript",
        "writer.style.subscript",
    ] {
        registry.set_enabled(&loom_command::CommandId::new(id), inline_enabled);
    }
}

pub(crate) fn command_specs() -> Vec<CommandSpec> {
    [
        (
            "writer.direction.auto",
            "Automatic Direction",
            "Use automatic paragraph direction",
            75,
        ),
        (
            "writer.direction.ltr",
            "Left to Right",
            "Set paragraph direction left to right",
            76,
        ),
        (
            "writer.direction.rtl",
            "Right to Left",
            "Set paragraph direction right to left",
            77,
        ),
        (
            "writer.style.color",
            "Text Color",
            "Choose foreground color for selected text",
            34,
        ),
        (
            "writer.style.superscript",
            "Superscript",
            "Toggle superscript on selected text",
            35,
        ),
        (
            "writer.style.subscript",
            "Subscript",
            "Toggle subscript on selected text",
            36,
        ),
    ]
    .into_iter()
    .map(|(id, label, description, order)| {
        CommandSpec::new(id, label)
            .with_undo_label(label)
            .with_description(description)
            .with_category("format")
            .with_order(order)
            .with_enabled(false)
    })
    .collect()
}

pub(crate) fn dispatch(app: &WriterApp, id: &str) -> bool {
    let global = app.global::<WriterFormatState>();
    match id {
        "writer.direction.auto" => global.invoke_select_direction(0),
        "writer.direction.ltr" => global.invoke_select_direction(1),
        "writer.direction.rtl" => global.invoke_select_direction(2),
        "writer.align.start" => global.invoke_select_logical_alignment(0),
        "writer.align.end" => global.invoke_select_logical_alignment(1),
        "writer.style.superscript" => global.invoke_toggle_superscript(),
        "writer.style.subscript" => global.invoke_toggle_subscript(),
        "writer.style.color" => global.invoke_open_foreground_color(),
        _ => return false,
    }
    true
}

fn direction_for_index(index: i32) -> Option<ParagraphDirection> {
    match index {
        0 => Some(ParagraphDirection::Auto),
        1 => Some(ParagraphDirection::LeftToRight),
        2 => Some(ParagraphDirection::RightToLeft),
        _ => None,
    }
}

fn direction_command_id(direction: ParagraphDirection) -> &'static str {
    match direction {
        ParagraphDirection::Auto => "writer.direction.auto",
        ParagraphDirection::LeftToRight => "writer.direction.ltr",
        ParagraphDirection::RightToLeft => "writer.direction.rtl",
    }
}

fn logical_alignment_command_id(index: i32) -> Option<&'static str> {
    match index {
        0 => Some("writer.align.start"),
        1 => Some("writer.align.end"),
        _ => None,
    }
}

fn command_allowed(app: &WriterApp, state: &GuiState, id: &str) -> bool {
    let result = state
        .registry
        .lock()
        .unwrap()
        .invoke(&CommandInvocation::new(id, InvocationSource::Toolbar));
    if result.is_err() {
        app.set_status_right(SharedString::from(format!(
            "{id} is not available for this selection"
        )));
        return false;
    }
    true
}

fn announce(app: &WriterApp, message: String) {
    app.set_status_right(SharedString::from(message.clone()));
    app.set_selection_announcement(SharedString::from(message));
}

fn sync_menu(app: &WriterApp, state: &GuiState) {
    if let Some(menu) = state.menu_service.borrow().clone() {
        crate::sync_writer_menu_if_present(&Some(menu), app, state);
    }
}
