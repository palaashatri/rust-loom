//! macOS AppKit NSMenu bridge for Loom desktop applications using muda.
//!
//! Provides native macOS global menu bar integration (`NSApp.mainMenu`),
//! linking structured [`MenuBar`] specifications to AppKit menus, native keyboard
//! shortcuts, dynamic enablement states, and event dispatch.

#![allow(clippy::needless_borrow)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    CheckMenuItem, Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use objc2::MainThreadMarker;

use crate::menu::{CommandState, MenuBar, MenuItem as LoomMenuItem, MenuShortcut};
use crate::DesktopError;

type DispatcherFn = Arc<dyn Fn(&str) -> Result<(), DesktopError> + Send + Sync + 'static>;

thread_local! {
    static ACTIVE_MACOS_MENU: RefCell<Option<MacosMenuState>> = const { RefCell::new(None) };
}

static EVENT_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);
static GLOBAL_DISPATCHER: Mutex<Option<DispatcherFn>> = Mutex::new(None);

/// Internal state holding references to the active native macOS menu and items.
pub struct MacosMenuState {
    _root: Menu,
    items: BTreeMap<String, MenuItemKind>,
}

impl MacosMenuState {
    /// Update enablement and checked state for a specific item.
    pub fn update_item(&mut self, item_id: &str, enabled: bool, checked: Option<bool>) {
        if let Some(item_kind) = self.items.get(item_id) {
            match item_kind {
                MenuItemKind::MenuItem(item) => {
                    item.set_enabled(enabled);
                }
                MenuItemKind::Check(item) => {
                    item.set_enabled(enabled);
                    if let Some(val) = checked {
                        item.set_checked(val);
                    }
                }
                _ => {}
            }
        }
    }

    /// Apply complete command projection state to a native menu item.
    pub fn update_command_state(&mut self, state: &CommandState) {
        if let Some(item_kind) = self.items.get(&state.id) {
            match item_kind {
                MenuItemKind::MenuItem(item) => {
                    item.set_text(&state.label);
                    item.set_enabled(state.enabled);
                    if let Some(ref sc) = state.shortcut {
                        let _ = item.set_accelerator(convert_shortcut(sc));
                    }
                }
                MenuItemKind::Check(item) => {
                    item.set_text(&state.label);
                    item.set_enabled(state.enabled);
                    if let Some(val) = state.checked {
                        item.set_checked(val);
                    }
                    if let Some(ref sc) = state.shortcut {
                        let _ = item.set_accelerator(convert_shortcut(sc));
                    }
                }
                _ => {}
            }
        }
    }
}

/// Convert a Loom [`MenuShortcut`] into a native muda [`Accelerator`].
pub fn convert_shortcut(shortcut: &MenuShortcut) -> Option<Accelerator> {
    let mut modifiers = Modifiers::empty();
    if shortcut.primary_modifier {
        modifiers |= Modifiers::SUPER; // Command on macOS
    }
    if shortcut.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if shortcut.alt {
        modifiers |= Modifiers::ALT; // Option on macOS
    }

    let code = match shortcut.key.to_uppercase().as_str() {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "," => Code::Comma,
        "." => Code::Period,
        "-" => Code::Minus,
        "=" => Code::Equal,
        ";" => Code::Semicolon,
        "'" => Code::Quote,
        "[" => Code::BracketLeft,
        "]" => Code::BracketRight,
        "/" => Code::Slash,
        "\\" => Code::Backslash,
        "`" => Code::Backquote,
        _ => return shortcut.display_string().parse::<Accelerator>().ok(),
    };

    Some(Accelerator::new(Some(modifiers), code))
}

fn ensure_event_handler_installed() {
    if !EVENT_HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let dispatcher = GLOBAL_DISPATCHER.lock().unwrap().clone();
            if let Some(dispatch) = dispatcher {
                let _ = dispatch(event.id().as_ref());
            }
        }));
    }
}

fn populate_submenu(
    parent: &Submenu,
    items: &[LoomMenuItem],
    items_map: &mut BTreeMap<String, MenuItemKind>,
) -> Result<(), DesktopError> {
    for item in items {
        match item {
            LoomMenuItem::Separator => {
                let sep = PredefinedMenuItem::separator();
                parent
                    .append(&sep)
                    .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
            }
            LoomMenuItem::Action {
                id,
                label,
                shortcut,
                enabled,
            } => {
                if id.ends_with(".hide") {
                    let p = PredefinedMenuItem::hide(Some(label));
                    parent
                        .append(&p)
                        .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                    items_map.insert(id.clone(), MenuItemKind::Predefined(p));
                } else if id.ends_with(".hide_others") {
                    let p = PredefinedMenuItem::hide_others(Some(label));
                    parent
                        .append(&p)
                        .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                    items_map.insert(id.clone(), MenuItemKind::Predefined(p));
                } else if id.ends_with(".quit") {
                    let p = PredefinedMenuItem::quit(Some(label));
                    parent
                        .append(&p)
                        .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                    items_map.insert(id.clone(), MenuItemKind::Predefined(p));
                } else if id == "window.minimize" {
                    let p = PredefinedMenuItem::minimize(Some(label));
                    parent
                        .append(&p)
                        .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                    items_map.insert(id.clone(), MenuItemKind::Predefined(p));
                } else if id == "window.bring_all_to_front" {
                    let p = PredefinedMenuItem::bring_all_to_front(Some(label));
                    parent
                        .append(&p)
                        .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                    items_map.insert(id.clone(), MenuItemKind::Predefined(p));
                } else {
                    let accel = shortcut.as_ref().and_then(convert_shortcut);
                    let m_item = MenuItem::with_id(id, label, *enabled, accel);
                    parent
                        .append(&m_item)
                        .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                    items_map.insert(id.clone(), MenuItemKind::MenuItem(m_item));
                }
            }
            LoomMenuItem::Check {
                id,
                label,
                shortcut,
                enabled,
                checked,
            } => {
                let accel = shortcut.as_ref().and_then(convert_shortcut);
                let c_item = CheckMenuItem::with_id(id, label, *enabled, *checked, accel);
                parent
                    .append(&c_item)
                    .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                items_map.insert(id.clone(), MenuItemKind::Check(c_item));
            }
            LoomMenuItem::Radio {
                id,
                label,
                shortcut,
                enabled,
                selected,
                ..
            } => {
                let accel = shortcut.as_ref().and_then(convert_shortcut);
                let c_item = CheckMenuItem::with_id(id, label, *enabled, *selected, accel);
                parent
                    .append(&c_item)
                    .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
                items_map.insert(id.clone(), MenuItemKind::Check(c_item));
            }
            LoomMenuItem::Submenu(sub) => {
                let sub_menu = Submenu::new(&sub.title, true);
                populate_submenu(&sub_menu, &sub.items, items_map)?;
                parent
                    .append(&sub_menu)
                    .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
            }
        }
    }
    Ok(())
}

/// Install the native macOS menu bar if executing on the main thread.
pub fn install_native_macos_menu(
    menu_bar: &MenuBar,
    dispatcher: DispatcherFn,
) -> Result<(), DesktopError> {
    if MainThreadMarker::new().is_none() {
        // Non-GUI or worker test thread: keep model in memory without touching AppKit.
        return Ok(());
    }

    *GLOBAL_DISPATCHER.lock().unwrap() = Some(dispatcher);
    ensure_event_handler_installed();

    let root = Menu::new();
    let mut items_map = BTreeMap::new();

    for menu in &menu_bar.menus {
        let submenu = Submenu::new(&menu.title, true);
        populate_submenu(&submenu, &menu.items, &mut items_map)?;
        root.append(&submenu)
            .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;

        if menu.title.eq_ignore_ascii_case("Window") {
            submenu.set_as_windows_menu_for_nsapp();
        } else if menu.title.eq_ignore_ascii_case("Help") {
            submenu.set_as_help_menu_for_nsapp();
        }
    }

    root.init_for_nsapp();

    ACTIVE_MACOS_MENU.with(|menu_ref| {
        *menu_ref.borrow_mut() = Some(MacosMenuState {
            _root: root,
            items: items_map,
        });
    });

    Ok(())
}

/// Update a native macOS menu item state if on the main thread.
pub fn update_native_item(item_id: &str, enabled: bool, checked: Option<bool>) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    ACTIVE_MACOS_MENU.with(|menu_ref| {
        if let Some(state) = menu_ref.borrow_mut().as_mut() {
            state.update_item(item_id, enabled, checked);
        }
    });
}

/// Update a native macOS menu item from full command state if on the main thread.
pub fn update_native_command_state(state: &CommandState) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    ACTIVE_MACOS_MENU.with(|menu_ref| {
        if let Some(s) = menu_ref.borrow_mut().as_mut() {
            s.update_command_state(state);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::build_standard_menu_bar;

    #[test]
    fn test_convert_shortcut_modifiers_and_codes() {
        let sc_n = MenuShortcut::primary("N");
        let accel_n = convert_shortcut(&sc_n).expect("accelerator for Cmd+N");
        assert_eq!(accel_n.key(), Code::KeyN);
        assert_eq!(accel_n.modifiers(), Modifiers::SUPER);

        let sc_shift = MenuShortcut::primary_shift("S");
        let accel_shift = convert_shortcut(&sc_shift).expect("accelerator for Cmd+Shift+S");
        assert_eq!(accel_shift.key(), Code::KeyS);
        assert_eq!(accel_shift.modifiers(), Modifiers::SUPER | Modifiers::SHIFT);

        let sc_alt = MenuShortcut::primary_alt("H");
        let accel_alt = convert_shortcut(&sc_alt).expect("accelerator for Cmd+Alt+H");
        assert_eq!(accel_alt.key(), Code::KeyH);
        assert_eq!(accel_alt.modifiers(), Modifiers::SUPER | Modifiers::ALT);

        let sc_comma = MenuShortcut::primary(",");
        let accel_comma = convert_shortcut(&sc_comma).expect("accelerator for Cmd+,");
        assert_eq!(accel_comma.key(), Code::Comma);
        assert_eq!(accel_comma.modifiers(), Modifiers::SUPER);
    }

    #[test]
    fn test_worker_thread_safety_without_appkit_panic() {
        let menu_bar = build_standard_menu_bar("Loom Sheets", vec![], vec![], vec![], vec![]);
        let dispatcher = Arc::new(|_id: &str| Ok(()));

        // When executed on a worker test thread, MainThreadMarker is None
        // so install_native_macos_menu must gracefully return Ok(()) without crashing.
        assert!(install_native_macos_menu(&menu_bar, dispatcher).is_ok());

        // Similarly, update functions must be safe no-ops on worker threads.
        update_native_item("file.save", true, None);
        let cmd_state = CommandState::action("file.save", "Save");
        update_native_command_state(&cmd_state);
    }
}
