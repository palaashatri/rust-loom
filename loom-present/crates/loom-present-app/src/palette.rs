//! Command palette management and wiring for Loom Present.

use std::rc::Rc;

use slint::{ComponentHandle, Model, SharedString, VecModel};

use crate::palette_rank;
use crate::{appearance, CommandPaletteItem, PresentApp};

/// Commands exposed through the command palette. Dispatch reuses the same
/// application callbacks as the toolbar and menus.
#[derive(Debug, Clone)]
pub enum PaletteAction {
    NewDeck,
    OpenDeck,
    SaveDeck,
    SaveAsDeck,
    AddSlide,
    DuplicateSlide,
    DeleteSlide,
    MoveSlideUp,
    MoveSlideDown,
    Undo,
    Redo,
    AddText,
    StyleBold,
    StyleItalic,
    TextLarger,
    TextSmaller,
    AddPicture,
    AddTable,
    DeleteTableRow,
    DeleteTableColumn,
    AddChart,
    NewSampleDeck,
    NewFromTemplate,
    ExportPdf,
    ExportPptx,
    TogglePreview,
    OpenPresenter,
    PrevSlide,
    NextSlide,
    ApplyTemplate(i32),
    SetAppearance(loom_desktop::Appearance),
    SetTransition(i32),
}

pub struct PaletteCommand {
    pub action: PaletteAction,
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
}

pub fn master_palette(app: &PresentApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewDeck,
            id: "present.new",
            label: "New Deck",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenDeck,
            id: "present.open",
            label: "Open Deck",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveDeck,
            id: "present.save",
            label: "Save Deck",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDeck,
            id: "present.save-as",
            label: "Save Deck As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::AddSlide,
            id: "present.add-slide",
            label: "Add Slide",
            shortcut: "Ctrl+Shift+A",
        },
        PaletteCommand {
            action: PaletteAction::DuplicateSlide,
            id: "present.duplicate-slide",
            label: "Duplicate Slide",
            shortcut: "Ctrl+D",
        },
        PaletteCommand {
            action: PaletteAction::DeleteSlide,
            id: "present.delete-slide",
            label: "Delete Slide",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::MoveSlideUp,
            id: "present.move-slide-up",
            label: "Move Slide Up",
            shortcut: "Ctrl+Alt+Up",
        },
        PaletteCommand {
            action: PaletteAction::MoveSlideDown,
            id: "present.move-slide-down",
            label: "Move Slide Down",
            shortcut: "Ctrl+Alt+Down",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "present.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "present.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::AddText,
            id: "present.add-text",
            label: "Add Text",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::StyleBold,
            id: "present.text-bold",
            label: "Bold",
            shortcut: "Ctrl+B",
        },
        PaletteCommand {
            action: PaletteAction::StyleItalic,
            id: "present.text-italic",
            label: "Italic",
            shortcut: "Ctrl+I",
        },
        PaletteCommand {
            action: PaletteAction::TextLarger,
            id: "present.text-larger",
            label: "Increase Text Size",
            shortcut: "Ctrl+]",
        },
        PaletteCommand {
            action: PaletteAction::TextSmaller,
            id: "present.text-smaller",
            label: "Decrease Text Size",
            shortcut: "Ctrl+[",
        },
        PaletteCommand {
            action: PaletteAction::AddPicture,
            id: "present.insert-image",
            label: "Insert Image...",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::AddTable,
            id: "present.insert-table",
            label: "Insert Table",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::DeleteTableRow,
            id: "table.delete_row",
            label: "Delete Table Row",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::DeleteTableColumn,
            id: "table.delete_column",
            label: "Delete Table Column",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::AddChart,
            id: "present.insert-chart",
            label: "Insert Chart",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::NewSampleDeck,
            id: "present.new-sample",
            label: "New from Sample Deck",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::NewFromTemplate,
            id: "present.new-template",
            label: "New from Template...",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::TogglePreview,
            id: "present.preview",
            label: "Start or Exit Slideshow",
            shortcut: "F5",
        },
        PaletteCommand {
            action: PaletteAction::OpenPresenter,
            id: "present.presenter-view",
            label: "Open Presenter View",
            shortcut: "P",
        },
        PaletteCommand {
            action: PaletteAction::PrevSlide,
            id: "present.prev",
            label: "Previous Slide",
            shortcut: "PageUp",
        },
        PaletteCommand {
            action: PaletteAction::NextSlide,
            id: "present.next",
            label: "Next Slide",
            shortcut: "PageDown",
        },
        PaletteCommand {
            action: PaletteAction::ExportPdf,
            id: "present.export-pdf",
            label: "Export PDF",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::ExportPptx,
            id: "present.export-pptx",
            label: "Export PowerPoint (.pptx)",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(0),
            id: "present.template.title",
            label: "Template: Title",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(1),
            id: "present.template.content",
            label: "Template: Content",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(2),
            id: "present.template.2col",
            label: "Template: 2 Column",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(3),
            id: "present.template.image",
            label: "Template: Image",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(1),
            id: "present.transition.dissolve",
            label: "Transition: Dissolve",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(2),
            id: "present.transition.push",
            label: "Transition: Push",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(3),
            id: "present.transition.morph",
            label: "Transition: Morph",
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
        PaletteAction::MoveSlideUp => app.get_can_move_slide_up(),
        PaletteAction::MoveSlideDown => app.get_can_move_slide_down(),
        _ => true,
    })
    .collect()
}

pub fn rebuild_palette(app: &PresentApp, query: &str) {
    let items: Vec<CommandPaletteItem> = palette_rank::matching(master_palette(app), query)
        .into_iter()
        .map(|c| CommandPaletteItem {
            id: c.id.into(),
            label: c.label.into(),
            shortcut: c.shortcut.into(),
            enabled: true,
        })
        .collect();
    app.set_palette_commands(Rc::new(VecModel::from(items)).into());
    let count = app.get_palette_commands().row_count() as i32;
    let selected = app.get_palette_selected();
    if selected >= count && count > 0 {
        app.set_palette_selected(count - 1);
    } else if count == 0 {
        app.set_palette_selected(0);
    }
}

pub fn wire_palette(app: &PresentApp) {
    {
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_move(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let count = app.get_palette_commands().row_count() as i32;
                if count == 0 {
                    return;
                }
                let next = (app.get_palette_selected() + delta).clamp(0, count - 1);
                app.set_palette_selected(next);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.push_str(text.as_str());
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.pop();
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_palette_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let command =
                    palette_rank::matching(master_palette(&app), &app.get_palette_query())
                        .into_iter()
                        .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewDeck => app.invoke_new_deck(),
                        PaletteAction::OpenDeck => app.invoke_open_deck(),
                        PaletteAction::SaveDeck => app.invoke_save_deck(),
                        PaletteAction::SaveAsDeck => app.invoke_save_as_deck(),
                        PaletteAction::AddSlide => app.invoke_add_slide(),
                        PaletteAction::DuplicateSlide => app.invoke_duplicate_slide(),
                        PaletteAction::DeleteSlide => app.invoke_delete_slide(),
                        PaletteAction::MoveSlideUp => app.invoke_move_slide_up(),
                        PaletteAction::MoveSlideDown => app.invoke_move_slide_down(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddText => app.invoke_add_text(),
                        PaletteAction::StyleBold => {
                            crate::element_style_ui::request(&app, "bold", 0)
                        }
                        PaletteAction::StyleItalic => {
                            crate::element_style_ui::request(&app, "italic", 0)
                        }
                        PaletteAction::TextLarger => {
                            crate::element_style_ui::request(&app, "size", 1)
                        }
                        PaletteAction::TextSmaller => {
                            crate::element_style_ui::request(&app, "size", -1)
                        }
                        PaletteAction::AddPicture => app.invoke_add_picture(),
                        PaletteAction::AddTable => app.invoke_add_table(),
                        PaletteAction::DeleteTableRow => {
                            app.invoke_table_action("delete-row".into())
                        }
                        PaletteAction::DeleteTableColumn => {
                            app.invoke_table_action("delete-col".into())
                        }
                        PaletteAction::AddChart => app.invoke_add_chart(),
                        PaletteAction::NewSampleDeck => app.invoke_new_sample_deck(),
                        PaletteAction::NewFromTemplate => {
                            app.set_theme_selected(0);
                            app.set_theme_category(0);
                            app.set_theme_chooser_open(true);
                        }
                        PaletteAction::TogglePreview => app.invoke_toggle_preview_mode(),
                        PaletteAction::OpenPresenter => app.invoke_open_presenter(),
                        PaletteAction::PrevSlide => app.invoke_prev_slide(),
                        PaletteAction::NextSlide => app.invoke_next_slide(),
                        PaletteAction::ExportPdf => app.invoke_export_pdf(),
                        PaletteAction::ExportPptx => app.invoke_export_pptx(),
                        PaletteAction::ApplyTemplate(index) => app.invoke_apply_template(index),
                        PaletteAction::SetTransition(index) => app.invoke_set_transition(index),
                        PaletteAction::SetAppearance(choice) => appearance::choose(&app, choice),
                    }
                }
            }
        });
    }
}
