//! Glue between the shared window chrome and menu components in `loom-ui` and
//! an application's own Slint window.
//!
//! Every Slint window generates its own Rust types, so this glue cannot be an
//! ordinary generic function. `local_menu_bindings!` and `window_chrome_bindings!`
//! expand inside an application crate, against that application's generated
//! window type, and contain the only copy of the logic. The projection from a
//! `MenuBar` to menu rows is plain Rust and lives here, tested once.

use crate::{DesktopError, MenuBar, MenuItem};

/// One row of the in-window menu, as plain data. Applications convert it into
/// their generated `LocalMenuEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMenuLine {
    /// Index of the top-level menu the row belongs to.
    pub menu_index: i32,
    /// Visible label; empty for separators.
    pub label: String,
    /// Command identifier dispatched on activation; empty for separators.
    pub command_id: String,
    /// Display form of the keyboard shortcut, if any.
    pub shortcut: String,
    /// Whether the row can be activated.
    pub enabled: bool,
    /// Whether a check or radio row is on.
    pub checked: bool,
    /// Whether the row is a check or radio row, so it has an on/off state
    /// even while it is off.
    pub checkable: bool,
    /// Whether this row is a separator rule.
    pub separator: bool,
}

/// Project an installed menu bar into in-window menus.
///
/// Only commands the application actually handles (`supported`) appear, and a
/// top-level menu is shown only if at least one of its supported commands is
/// enabled, so users never see a menu of placeholders.
pub fn project_local_menu(
    menu_bar: &MenuBar,
    supported: &[&str],
) -> Result<(Vec<String>, Vec<LocalMenuLine>), DesktopError> {
    let visible_menus: Vec<_> = menu_bar
        .menus
        .iter()
        .filter(|menu| {
            menu.items.iter().any(|item| {
                item.id()
                    .is_some_and(|id| supported.contains(&id) && item.is_enabled())
            })
        })
        .collect();
    let labels = visible_menus
        .iter()
        .map(|menu| menu.title.clone())
        .collect();
    let mut entries = Vec::new();

    for (menu_index, menu) in visible_menus.iter().enumerate() {
        let mut separator_pending = false;
        let mut menu_entries: Vec<LocalMenuLine> = Vec::new();
        for item in &menu.items {
            if item.id().is_some_and(|id| !supported.contains(&id)) {
                continue;
            }
            let entry = match item {
                MenuItem::Action {
                    id,
                    label,
                    shortcut,
                    enabled,
                } => Some((id, label, shortcut, *enabled, false, false)),
                MenuItem::Check {
                    id,
                    label,
                    shortcut,
                    enabled,
                    checked,
                } => Some((id, label, shortcut, *enabled, *checked, true)),
                MenuItem::Radio {
                    id,
                    label,
                    shortcut,
                    enabled,
                    selected,
                    ..
                } => Some((id, label, shortcut, *enabled, *selected, true)),
                MenuItem::Separator => {
                    separator_pending = !menu_entries.is_empty();
                    None
                }
                MenuItem::Submenu(submenu) => {
                    return Err(DesktopError::InvalidRequest(format!(
                        "the in-window menu does not support nested menu {}",
                        submenu.title
                    )));
                }
            };
            if let Some((id, label, shortcut, enabled, checked, checkable)) = entry {
                if separator_pending {
                    menu_entries.push(LocalMenuLine {
                        menu_index: menu_index as i32,
                        label: String::new(),
                        command_id: String::new(),
                        shortcut: String::new(),
                        enabled: false,
                        checked: false,
                        checkable: false,
                        separator: true,
                    });
                    separator_pending = false;
                }
                menu_entries.push(LocalMenuLine {
                    menu_index: menu_index as i32,
                    label: label.clone(),
                    command_id: id.clone(),
                    shortcut: shortcut
                        .as_ref()
                        .map(|shortcut| shortcut.display_string())
                        .unwrap_or_default(),
                    enabled,
                    checked,
                    checkable,
                    separator: false,
                });
            }
        }
        entries.extend(menu_entries);
    }

    Ok((labels, entries))
}

/// Which in-window menu a key opens: Alt plus a letter, or `"F10"` for the
/// first menu. Each menu takes the first letter of its label that an earlier
/// menu has not already claimed (File and Format become F and O), so every
/// menu has a distinct key. Returns -1 when the key opens nothing.
pub fn menu_key_index<S: AsRef<str>>(labels: &[S], key: &str) -> i32 {
    if key == "F10" {
        return if labels.is_empty() { -1 } else { 0 };
    }
    let mut wanted = key.chars();
    let (Some(letter), None) = (wanted.next(), wanted.next()) else {
        return -1;
    };
    let letter = letter.to_ascii_lowercase();
    let mut claimed: Vec<char> = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        let mnemonic = label
            .as_ref()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .find(|c| !claimed.contains(c));
        if let Some(mnemonic) = mnemonic {
            if mnemonic == letter {
                return index as i32;
            }
            claimed.push(mnemonic);
        }
    }
    -1
}

/// Expand the in-window menu bindings for one application window.
///
/// Invoke inside a module of the application crate:
/// `loom_desktop::local_menu_bindings!(MyApp, SUPPORTED_COMMANDS, set_status_left);`
/// The window must declare the `local-menu-*` properties and callbacks that
/// `loom-ui`'s `LocalMenuLabels` and `LocalMenuPopup` are wired to, and import
/// `LocalMenuEntry` at the crate root. Generates `sync`, `wire_keyboard`, and
/// `wire_action`.
#[macro_export]
// The macro deliberately names the invoking crate's generated `LocalMenuEntry`.
#[allow(clippy::crate_in_macro_def)]
macro_rules! local_menu_bindings {
    ($app:ty, $supported:expr, $status_setter:ident) => {
        /// Rebuild the in-window menu rows from the installed menu bar.
        pub(crate) fn sync(
            app: &$app,
            menu_service: &$crate::NativeMenuBar,
        ) -> Result<(), $crate::DesktopError> {
            use $crate::MenuBarService as _;
            let menu_bar = menu_service.installed_menu_bar().ok_or_else(|| {
                $crate::DesktopError::InvalidRequest("the menu bar is not installed".into())
            })?;
            let (labels, lines) = $crate::project_local_menu(&menu_bar, $supported)?;
            let labels: ::std::vec::Vec<::slint::SharedString> = labels
                .into_iter()
                .map(::slint::SharedString::from)
                .collect();
            let items: ::std::vec::Vec<crate::LocalMenuEntry> = lines
                .into_iter()
                .map(|line| crate::LocalMenuEntry {
                    menu_index: line.menu_index,
                    label: line.label.into(),
                    command_id: line.command_id.into(),
                    shortcut: line.shortcut.into(),
                    enabled: line.enabled,
                    checked: line.checked,
                    checkable: line.checkable,
                    separator: line.separator,
                })
                .collect();
            app.set_local_menu_labels(::slint::ModelRc::new(::slint::VecModel::from(labels)));
            app.set_local_menu_items(::slint::ModelRc::new(::slint::VecModel::from(items)));
            Ok(())
        }

        /// Keep the highlighted row valid as menus open and arrow keys move it.
        pub(crate) fn wire_keyboard(app: &$app) {
            use ::slint::{ComponentHandle as _, Model as _};
            let app_ref = app.as_weak();
            app.global::<crate::LocalMenu>().on_key_index(move |key| {
                app_ref.upgrade().map_or(-1, |app| {
                    let labels: ::std::vec::Vec<::std::string::String> = app
                        .get_local_menu_labels()
                        .iter()
                        .map(|label| label.to_string())
                        .collect();
                    $crate::menu_key_index(&labels, key.as_str())
                })
            });
            let app_ref = app.as_weak();
            app.on_local_menu_opened(move |menu_index| {
                if let Some(app) = app_ref.upgrade() {
                    let items = app.get_local_menu_items();
                    let popup_items = (0..items.row_count())
                        .filter_map(|index| items.row_data(index))
                        .filter(|item| item.menu_index == menu_index)
                        .collect::<::std::vec::Vec<_>>();
                    let first = (0..items.row_count()).find(|index| {
                        items.row_data(*index).is_some_and(|item| {
                            item.menu_index == menu_index && item.enabled && !item.separator
                        })
                    });
                    app.set_local_menu_selected_index(first.map_or(-1, |index| index as i32));
                    app.set_local_menu_popup_selected_index(first.map_or(-1, |index| {
                        (0..=index)
                            .filter(|position| {
                                items
                                    .row_data(*position)
                                    .is_some_and(|item| item.menu_index == menu_index)
                            })
                            .count() as i32
                            - 1
                    }));
                    app.set_local_menu_popup_items(::slint::ModelRc::new(::slint::VecModel::from(
                        popup_items,
                    )));
                }
            });

            let app_ref = app.as_weak();
            app.on_local_menu_move(move |direction| {
                if let Some(app) = app_ref.upgrade() {
                    let items = app.get_local_menu_items();
                    let count = items.row_count();
                    if count == 0 {
                        app.set_local_menu_selected_index(-1);
                        return;
                    }
                    let menu_index = app.get_local_menu_open_index();
                    let current = app.get_local_menu_selected_index();
                    let delta: isize = if direction < 0 { -1 } else { 1 };
                    for offset in 1..=count {
                        let index = if current < 0 {
                            if delta < 0 {
                                count - offset
                            } else {
                                offset - 1
                            }
                        } else {
                            (current as isize + delta * offset as isize).rem_euclid(count as isize)
                                as usize
                        };
                        if items.row_data(index).is_some_and(|item| {
                            item.menu_index == menu_index && item.enabled && !item.separator
                        }) {
                            app.set_local_menu_selected_index(index as i32);
                            let popup_index = (0..=index)
                                .filter(|position| {
                                    items
                                        .row_data(*position)
                                        .is_some_and(|item| item.menu_index == menu_index)
                                })
                                .count() as i32
                                - 1;
                            app.set_local_menu_popup_selected_index(popup_index);
                            return;
                        }
                    }
                    app.set_local_menu_selected_index(-1);
                }
            });
        }

        /// Route activated rows through the installed menu's guard and sink, so
        /// disabled commands can never run from the in-window menu either.
        pub(crate) fn wire_action<M>(app: &$app, menu_service: M)
        where
            M: ::std::ops::Deref<Target = $crate::NativeMenuBar> + 'static,
        {
            use ::slint::ComponentHandle as _;
            use $crate::MenuBarService as _;
            wire_keyboard(app);
            let app_ref = app.as_weak();
            app.on_local_menu_action(move |id| {
                if let Err(error) = menu_service.dispatch_action(id.as_str()) {
                    if let Some(app) = app_ref.upgrade() {
                        app.$status_setter(::slint::SharedString::from(format!(
                            "Menu action failed: {error}"
                        )));
                    }
                }
            });
        }
    };
}

/// Expand the custom-title-bar bindings for one application window.
///
/// The window must declare `custom-chrome`, `no-frame: root.custom-chrome`, and
/// the `window-drag`, `window-close`, and `window-resize(int)` callbacks that
/// `loom-ui`'s `LoomTitleBar` and `LoomWindowResizeEdges` are wired to, and the
/// application crate must depend on `i-slint-backend-winit`. Generates `install`
/// and the `USES_CUSTOM_CHROME` constant. macOS keeps its native frame and menu.
#[macro_export]
macro_rules! window_chrome_bindings {
    ($app:ty) => {
        /// Whether this platform draws Loom's own title bar.
        pub(crate) const USES_CUSTOM_CHROME: bool =
            cfg!(any(target_os = "windows", target_os = "linux"));

        /// Turn on custom chrome and connect its callbacks. Safe in headless
        /// renders: window operations then find no winit window and do nothing.
        pub(crate) fn install(app: &$app) {
            app.set_custom_chrome(USES_CUSTOM_CHROME);
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            wire_window_chrome(app);
        }

        #[cfg(any(target_os = "windows", target_os = "linux"))]
        fn wire_window_chrome(app: &$app) {
            use ::i_slint_backend_winit::winit::window::ResizeDirection;
            use ::i_slint_backend_winit::WinitWindowAccessor as _;
            use ::slint::ComponentHandle as _;
            let weak = app.as_weak();
            app.on_window_drag({
                let weak = weak.clone();
                move || {
                    if let Some(app) = weak.upgrade() {
                        app.window().with_winit_window(|window| {
                            let _ = window.drag_window();
                        });
                    }
                }
            });
            // Same close request the native button raises, so unsaved-changes
            // handling and drain-before-close logic still run.
            app.on_window_close({
                let weak = weak.clone();
                move || {
                    if let Some(app) = weak.upgrade() {
                        app.window()
                            .dispatch_event(::slint::platform::WindowEvent::CloseRequested);
                    }
                }
            });
            app.on_window_resize(move |direction| {
                let Some(app) = weak.upgrade() else { return };
                let direction = match direction {
                    0 => ResizeDirection::North,
                    1 => ResizeDirection::South,
                    2 => ResizeDirection::East,
                    3 => ResizeDirection::West,
                    4 => ResizeDirection::NorthEast,
                    5 => ResizeDirection::NorthWest,
                    6 => ResizeDirection::SouthEast,
                    _ => ResizeDirection::SouthWest,
                };
                app.window().with_winit_window(|window| {
                    let _ = window.drag_resize_window(direction);
                });
            });
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_standard_menu_bar;

    #[test]
    fn menu_keys_are_distinct_mnemonics_and_f10_opens_the_first_menu() {
        let labels = ["File", "Edit", "View", "Format"];
        assert_eq!(menu_key_index(&labels, "f"), 0);
        assert_eq!(menu_key_index(&labels, "E"), 1);
        assert_eq!(menu_key_index(&labels, "v"), 2);
        assert_eq!(menu_key_index(&labels, "o"), 3, "Format yields F to File");
        assert_eq!(menu_key_index(&labels, "x"), -1);
        assert_eq!(menu_key_index(&labels, "ab"), -1);
        assert_eq!(menu_key_index(&labels, "F10"), 0);
        assert_eq!(menu_key_index::<&str>(&[], "F10"), -1);
    }

    #[test]
    fn projection_shows_only_menus_with_working_commands() {
        let mut menu = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        menu.disable_items_except(["file.open", "app.palette", "view.zoom_in", "help.shortcuts"]);
        let supported = [
            "file.open",
            "edit.undo",
            "app.palette",
            "view.zoom_in",
            "help.shortcuts",
        ];

        let (labels, entries) = project_local_menu(&menu, &supported).expect("project menus");
        assert_eq!(labels, ["File", "Edit", "View", "Help"]);
        assert!(entries.iter().any(|entry| {
            entry.command_id == "file.open" && entry.label == "Open..." && entry.enabled
        }));
        assert!(entries
            .iter()
            .any(|entry| entry.command_id == "edit.undo" && !entry.enabled));
        assert!(!entries.iter().any(|entry| {
            entry.command_id == "help.documentation" || entry.command_id == "window.minimize"
        }));
    }

    #[test]
    fn check_and_radio_rows_are_checkable_even_while_off() {
        let menu = build_standard_menu_bar(
            "Loom",
            vec![],
            vec![],
            crate::appearance::menu_items(),
            vec![],
        );
        let supported: Vec<&str> = crate::Appearance::ALL
            .iter()
            .map(|choice| choice.command_id())
            .chain(["file.open"])
            .collect();
        let (_, entries) = project_local_menu(&menu, &supported).expect("project menus");
        let open = entries
            .iter()
            .find(|e| e.command_id == "file.open")
            .expect("open");
        assert!(!open.checkable && !open.checked);
        let dark = entries
            .iter()
            .find(|e| e.command_id == "view.appearance.dark")
            .expect("dark row");
        assert!(
            dark.checkable && !dark.checked,
            "an unchecked radio row is still checkable"
        );
        let system = entries
            .iter()
            .find(|e| e.command_id == "view.appearance.system")
            .expect("system row");
        assert!(system.checkable && system.checked);
    }

    #[test]
    fn unsupported_commands_never_appear_even_when_enabled() {
        let menu = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        let (_, entries) = project_local_menu(&menu, &["file.open"]).expect("project menus");
        assert!(entries
            .iter()
            .all(|entry| entry.separator || entry.command_id == "file.open"));
    }
}
