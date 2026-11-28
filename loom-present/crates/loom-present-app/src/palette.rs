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
    FindReplace,
    PrintPreview,
    StartHub,
    WelcomeGuide,
    StyleUnderline,
    StyleStrikethrough,
    DuplicateSelection,
    SelectAll,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SlideSorter,
    InsertRoundedRect,
    InsertArrow,
    InsertLine,
    InsertCallout,
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
            shortcut: crate::command_dispatch::shortcut_for_id("file.new"),
        },
        PaletteCommand {
            action: PaletteAction::OpenDeck,
            id: "present.open",
            label: "Open Deck",
            shortcut: crate::command_dispatch::shortcut_for_id("file.open"),
        },
        PaletteCommand {
            action: PaletteAction::SaveDeck,
            id: "present.save",
            label: "Save Deck",
            shortcut: crate::command_dispatch::shortcut_for_id("file.save"),
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDeck,
            id: "present.save-as",
            label: "Save Deck As",
            shortcut: crate::command_dispatch::shortcut_for_id("file.save_as"),
        },
        PaletteCommand {
            action: PaletteAction::AddSlide,
            id: "present.add-slide",
            label: "Add Slide",
            shortcut: if cfg!(target_os = "macos") {
                "Cmd+Shift+N"
            } else {
                "Ctrl+Shift+N"
            },
        },
        PaletteCommand {
            action: PaletteAction::DuplicateSlide,
            id: "present.duplicate-slide",
            label: "Duplicate Slide",
            shortcut: crate::command_dispatch::shortcut_for_id("slide.duplicate"),
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
            shortcut: if cfg!(target_os = "macos") {
                "Cmd+Opt+Up"
            } else {
                "Ctrl+Alt+Up"
            },
        },
        PaletteCommand {
            action: PaletteAction::MoveSlideDown,
            id: "present.move-slide-down",
            label: "Move Slide Down",
            shortcut: if cfg!(target_os = "macos") {
                "Cmd+Opt+Down"
            } else {
                "Ctrl+Alt+Down"
            },
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "present.undo",
            label: "Undo",
            shortcut: crate::command_dispatch::shortcut_for_id("edit.undo"),
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "present.redo",
            label: "Redo",
            shortcut: crate::command_dispatch::shortcut_for_id("edit.redo"),
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
            shortcut: crate::command_dispatch::shortcut_for_id("format.bold"),
        },
        PaletteCommand {
            action: PaletteAction::StyleItalic,
            id: "present.text-italic",
            label: "Italic",
            shortcut: crate::command_dispatch::shortcut_for_id("format.italic"),
        },
        PaletteCommand {
            action: PaletteAction::StyleUnderline,
            id: "present.text-underline",
            label: "Underline",
            shortcut: crate::command_dispatch::shortcut_for_id("format.underline"),
        },
        PaletteCommand {
            action: PaletteAction::StyleStrikethrough,
            id: "present.text-strikethrough",
            label: "Strikethrough",
            shortcut: crate::command_dispatch::shortcut_for_id("format.strikethrough"),
        },
        PaletteCommand {
            action: PaletteAction::FindReplace,
            id: "present.find-replace",
            label: "Find and Replace",
            shortcut: crate::command_dispatch::shortcut_for_id("edit.find"),
        },
        PaletteCommand {
            action: PaletteAction::PrintPreview,
            id: "present.print-preview",
            label: "Print Preview",
            shortcut: crate::command_dispatch::shortcut_for_id("file.print_preview"),
        },
        PaletteCommand {
            action: PaletteAction::StartHub,
            id: "present.start-hub",
            label: "Start Hub",
            shortcut: crate::command_dispatch::shortcut_for_id("file.start_hub"),
        },
        PaletteCommand {
            action: PaletteAction::WelcomeGuide,
            id: "present.welcome-guide",
            label: "Welcome Guide",
            shortcut: crate::command_dispatch::shortcut_for_id("help.welcome_guide"),
        },
        PaletteCommand {
            action: PaletteAction::DuplicateSelection,
            id: "present.duplicate-selection",
            label: "Duplicate Selection",
            shortcut: crate::command_dispatch::shortcut_for_id("edit.duplicate"),
        },
        PaletteCommand {
            action: PaletteAction::SelectAll,
            id: "present.select-all",
            label: "Select All",
            shortcut: crate::command_dispatch::shortcut_for_id("edit.select_all"),
        },
        PaletteCommand {
            action: PaletteAction::ZoomIn,
            id: "present.zoom-in",
            label: "Zoom In",
            shortcut: crate::command_dispatch::shortcut_for_id("view.zoom_in"),
        },
        PaletteCommand {
            action: PaletteAction::ZoomOut,
            id: "present.zoom-out",
            label: "Zoom Out",
            shortcut: crate::command_dispatch::shortcut_for_id("view.zoom_out"),
        },
        PaletteCommand {
            action: PaletteAction::ZoomReset,
            id: "present.zoom-reset",
            label: "Reset Zoom",
            shortcut: crate::command_dispatch::shortcut_for_id("view.zoom_reset"),
        },
        PaletteCommand {
            action: PaletteAction::SlideSorter,
            id: "present.slide-sorter",
            label: "Slide Sorter (Light Table)",
            shortcut: crate::command_dispatch::shortcut_for_id("view.slide_sorter"),
        },
        PaletteCommand {
            action: PaletteAction::InsertRoundedRect,
            id: "present.insert-rounded-rect",
            label: "Insert Rounded Rectangle",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::InsertArrow,
            id: "present.insert-arrow",
            label: "Insert Arrow",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::InsertLine,
            id: "present.insert-line",
            label: "Insert Line",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::InsertCallout,
            id: "present.insert-callout",
            label: "Insert Callout",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::TextLarger,
            id: "present.text-larger",
            label: "Increase Text Size",
            shortcut: crate::command_dispatch::shortcut_for_id("format.font_increase"),
        },
        PaletteCommand {
            action: PaletteAction::TextSmaller,
            id: "present.text-smaller",
            label: "Decrease Text Size",
            shortcut: crate::command_dispatch::shortcut_for_id("format.font_decrease"),
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
            label: "Paste Chart from Sheets",
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
            shortcut: crate::command_dispatch::shortcut_for_id("file.export_pdf"),
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
                        PaletteAction::FindReplace => {
                            let _ = crate::command_dispatch::dispatch_command(&app, "edit.find");
                        }
                        PaletteAction::PrintPreview => {
                            let _ = crate::command_dispatch::dispatch_command(
                                &app,
                                "file.print_preview",
                            );
                        }
                        PaletteAction::StartHub => {
                            let _ =
                                crate::command_dispatch::dispatch_command(&app, "file.start_hub");
                        }
                        PaletteAction::WelcomeGuide => {
                            let _ = crate::command_dispatch::dispatch_command(
                                &app,
                                "help.welcome_guide",
                            );
                        }
                        PaletteAction::StyleUnderline => {
                            let _ =
                                crate::command_dispatch::dispatch_command(&app, "format.underline");
                        }
                        PaletteAction::StyleStrikethrough => {
                            let _ = crate::command_dispatch::dispatch_command(
                                &app,
                                "format.strikethrough",
                            );
                        }
                        PaletteAction::DuplicateSelection => {
                            let _ =
                                crate::command_dispatch::dispatch_command(&app, "edit.duplicate");
                        }
                        PaletteAction::SelectAll => {
                            let _ =
                                crate::command_dispatch::dispatch_command(&app, "edit.select_all");
                        }
                        PaletteAction::ZoomIn => {
                            let _ = crate::command_dispatch::dispatch_command(&app, "view.zoom_in");
                        }
                        PaletteAction::ZoomOut => {
                            let _ =
                                crate::command_dispatch::dispatch_command(&app, "view.zoom_out");
                        }
                        PaletteAction::ZoomReset => {
                            let _ =
                                crate::command_dispatch::dispatch_command(&app, "view.zoom_reset");
                        }
                        PaletteAction::SlideSorter => {
                            let _ = crate::command_dispatch::dispatch_command(
                                &app,
                                "view.slide_sorter",
                            );
                        }
                        PaletteAction::InsertRoundedRect => {
                            app.invoke_add_shape_kind("rounded-rect".into());
                        }
                        PaletteAction::InsertArrow => {
                            app.invoke_add_shape_kind("arrow".into());
                        }
                        PaletteAction::InsertLine => {
                            app.invoke_add_shape_kind("line".into());
                        }
                        PaletteAction::InsertCallout => {
                            app.invoke_add_shape_kind("callout".into());
                        }
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
