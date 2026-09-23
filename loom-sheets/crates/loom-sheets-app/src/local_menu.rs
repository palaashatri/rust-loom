use std::sync::Arc;

use loom_desktop::{DesktopError, MenuBar, MenuBarService, MenuItem, NativeMenuBar};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{LocalMenuEntry, SheetsApp};

pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "file.new",
    "file.new_template",
    "file.open",
    "file.save",
    "file.save_as",
    "file.export_csv",
    "file.export_xlsx",
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.select_all",
    "app.palette",
    "help.shortcuts",
    "view.inspector",
    "view.zoom_in",
    "view.zoom_out",
    "view.zoom_actual",
    "table.add_row",
    "table.delete_row",
    "table.add_col",
    "table.delete_col",
    "table.sort_asc",
    "table.sort_desc",
    "table.freeze_header",
    "table.unfreeze_panes",
    "table.pivot_sum",
    "sheets.insert_shape",
    "sheets.insert_image",
    "sheets.delete_sheet",
];

fn project(menu_bar: &MenuBar) -> Result<(Vec<SharedString>, Vec<LocalMenuEntry>), DesktopError> {
    let visible_menus: Vec<_> = menu_bar
        .menus
        .iter()
        .filter(|menu| {
            menu.items.iter().any(|item| {
                item.id()
                    .is_some_and(|id| SUPPORTED_COMMANDS.contains(&id) && item.is_enabled())
            })
        })
        .collect();
    let labels = visible_menus
        .iter()
        .map(|menu| SharedString::from(menu.title.as_str()))
        .collect();
    let mut entries = Vec::new();

    for (menu_index, menu) in visible_menus.iter().enumerate() {
        let mut separator_pending = false;
        let mut menu_entries = Vec::new();
        for item in &menu.items {
            if item
                .id()
                .is_some_and(|id| !SUPPORTED_COMMANDS.contains(&id))
            {
                continue;
            }
            let entry = match item {
                MenuItem::Action {
                    id,
                    label,
                    shortcut,
                    enabled,
                } => Some((id, label, shortcut, *enabled, false)),
                MenuItem::Check {
                    id,
                    label,
                    shortcut,
                    enabled,
                    checked,
                } => Some((id, label, shortcut, *enabled, *checked)),
                MenuItem::Radio {
                    id,
                    label,
                    shortcut,
                    enabled,
                    selected,
                    ..
                } => Some((id, label, shortcut, *enabled, *selected)),
                MenuItem::Separator => {
                    separator_pending = !menu_entries.is_empty();
                    None
                }
                MenuItem::Submenu(submenu) => {
                    return Err(DesktopError::InvalidRequest(format!(
                        "local Sheets menu does not support nested menu {}",
                        submenu.title
                    )));
                }
            };
            if let Some((id, label, shortcut, enabled, checked)) = entry {
                if separator_pending {
                    menu_entries.push(LocalMenuEntry {
                        menu_index: menu_index as i32,
                        label: SharedString::from(""),
                        command_id: SharedString::from(""),
                        shortcut: SharedString::from(""),
                        enabled: false,
                        checked: false,
                        separator: true,
                    });
                    separator_pending = false;
                }
                menu_entries.push(LocalMenuEntry {
                    menu_index: menu_index as i32,
                    label: SharedString::from(label.as_str()),
                    command_id: SharedString::from(id.as_str()),
                    shortcut: SharedString::from(
                        shortcut
                            .as_ref()
                            .map(|shortcut| shortcut.display_string())
                            .unwrap_or_default(),
                    ),
                    enabled,
                    checked,
                    separator: false,
                });
            }
        }
        entries.extend(menu_entries);
    }

    Ok((labels, entries))
}

pub(crate) fn sync(app: &SheetsApp, menu_service: &NativeMenuBar) -> Result<(), DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("Sheets menu bar is not installed".into()))?;
    let (labels, items) = project(&menu_bar)?;
    app.set_local_menu_labels(ModelRc::new(VecModel::from(labels)));
    app.set_local_menu_items(ModelRc::new(VecModel::from(items)));
    Ok(())
}

pub(crate) fn wire_action(app: &SheetsApp, menu_service: Arc<NativeMenuBar>) {
    let app_ref = app.as_weak();
    app.on_local_menu_action(move |id| {
        if let Err(error) = menu_service.dispatch_action(id.as_str()) {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Menu action failed: {error}")));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use loom_desktop::{build_standard_menu_bar, MenuBarService};

    use super::*;

    #[test]
    fn projection_shows_only_menus_with_working_commands() {
        let mut menu = build_standard_menu_bar("Loom Sheets", vec![], vec![], vec![], vec![]);
        menu.disable_items_except(["file.open", "app.palette", "view.zoom_in", "help.shortcuts"]);

        let (labels, entries) = project(&menu).expect("project local menus");
        let labels: Vec<_> = labels.iter().map(ToString::to_string).collect();
        assert_eq!(labels, ["File", "Edit", "View", "Help"]);
        assert!(entries.iter().any(|entry| {
            entry.command_id == "file.open" && entry.label == "Open..." && entry.enabled
        }));
        assert!(entries
            .iter()
            .any(|entry| { entry.command_id == "edit.undo" && !entry.enabled }));
        assert!(!entries.iter().any(|entry| {
            entry.command_id == "help.documentation" || entry.command_id == "help.feedback"
        }));
        assert!(!entries
            .iter()
            .any(|entry| entry.command_id == "window.minimize"));
    }

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
