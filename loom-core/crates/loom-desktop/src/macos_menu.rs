//! macOS AppKit NSMenu bridge for Loom desktop applications using muda.
//!
//! Provides native macOS global menu bar integration (`NSApp.mainMenu`),
//! linking structured [`MenuBar`] specifications to AppKit menus, native keyboard
//! shortcuts, dynamic enablement states, and event dispatch.

#![allow(clippy::needless_borrow)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem,
    Submenu,
};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;

use crate::menu::{
    CommandState, MenuBar, MenuItem as LoomMenuItem, MenuShortcut, NativeMenuInstallationStatus,
};
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
    installation_id: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeQuitItemKind {
    System,
    Command,
}

fn native_quit_item_kind(
    item_id: &str,
    route_quit_through_command: bool,
) -> Option<NativeQuitItemKind> {
    item_id
        .ends_with(".quit")
        .then_some(if route_quit_through_command {
            NativeQuitItemKind::Command
        } else {
            NativeQuitItemKind::System
        })
}

fn status_for_appkit_root_pointers(
    expected_root: Option<*const c_void>,
    appkit_root: Option<*const c_void>,
) -> NativeMenuInstallationStatus {
    match (expected_root, appkit_root) {
        (Some(expected), Some(actual)) if !expected.is_null() && !actual.is_null() => {
            if expected == actual {
                NativeMenuInstallationStatus::Installed
            } else {
                NativeMenuInstallationStatus::AppKitRootMismatch
            }
        }
        _ => NativeMenuInstallationStatus::VerificationUnavailable,
    }
}

/// Checks whether AppKit's current main menu matches the root installed by Muda.
fn appkit_root_status(root: &Menu) -> NativeMenuInstallationStatus {
    let Some(main_thread) = MainThreadMarker::new() else {
        return NativeMenuInstallationStatus::MainThreadRequired;
    };

    let appkit_root = NSApplication::sharedApplication(main_thread).mainMenu();
    let Some(appkit_root) = appkit_root else {
        return NativeMenuInstallationStatus::VerificationUnavailable;
    };

    let native_root = ContextMenu::ns_menu(root);
    let expected_root = (!native_root.is_null()).then_some(native_root as *const c_void);
    let appkit_root = std::ptr::from_ref(&*appkit_root).cast::<c_void>();
    status_for_appkit_root_pointers(expected_root, Some(appkit_root))
}

/// Confirm whether the current AppKit main menu still matches this installation.
pub fn native_menu_installation_status(
    installation_id: u64,
    last_status: NativeMenuInstallationStatus,
) -> NativeMenuInstallationStatus {
    if MainThreadMarker::new().is_none() {
        return NativeMenuInstallationStatus::MainThreadRequired;
    }

    ACTIVE_MACOS_MENU.with(|menu_ref| match menu_ref.borrow().as_ref() {
        Some(state) if state.installation_id == installation_id => appkit_root_status(&state._root),
        Some(_) | None if last_status == NativeMenuInstallationStatus::Installed => {
            NativeMenuInstallationStatus::AppKitRootMismatch
        }
        _ => last_status,
    })
}

fn populate_submenu(
    parent: &Submenu,
    items: &[LoomMenuItem],
    items_map: &mut BTreeMap<String, MenuItemKind>,
    route_quit_through_command: bool,
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
                } else if native_quit_item_kind(id, route_quit_through_command)
                    == Some(NativeQuitItemKind::System)
                {
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
                populate_submenu(&sub_menu, &sub.items, items_map, route_quit_through_command)?;
                parent
                    .append(&sub_menu)
                    .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;
            }
        }
    }
    Ok(())
}

/// Install the native macOS menu bar and report whether AppKit accepted its root.
pub fn install_native_macos_menu(
    menu_bar: &MenuBar,
    dispatcher: DispatcherFn,
    route_quit_through_command: bool,
    installation_id: u64,
) -> Result<NativeMenuInstallationStatus, DesktopError> {
    if MainThreadMarker::new().is_none() {
        // Keep the legacy model install usable on worker threads, but report
        // that this native menu was not installed because AppKit requires main.
        return Ok(NativeMenuInstallationStatus::MainThreadRequired);
    }

    let root = Menu::new();
    let mut items_map = BTreeMap::new();

    for menu in &menu_bar.menus {
        let submenu = Submenu::new(&menu.title, true);
        populate_submenu(
            &submenu,
            &menu.items,
            &mut items_map,
            route_quit_through_command,
        )?;
        root.append(&submenu)
            .map_err(|e| DesktopError::InvalidRequest(e.to_string()))?;

        if menu.title.eq_ignore_ascii_case("Window") {
            submenu.set_as_windows_menu_for_nsapp();
        } else if menu.title.eq_ignore_ascii_case("Help") {
            submenu.set_as_help_menu_for_nsapp();
        }
    }

    root.init_for_nsapp();

    let status = appkit_root_status(&root);
    if status == NativeMenuInstallationStatus::AppKitRootMismatch {
        return Ok(status);
    }

    *GLOBAL_DISPATCHER.lock().unwrap() = Some(dispatcher);
    ensure_event_handler_installed();

    ACTIVE_MACOS_MENU.with(|menu_ref| {
        *menu_ref.borrow_mut() = Some(MacosMenuState {
            _root: root,
            items: items_map,
            installation_id,
        });
    });

    Ok(status)
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
    fn native_quit_keeps_system_default_and_supports_command_routing() {
        assert_eq!(
            native_quit_item_kind("writer.quit", false),
            Some(NativeQuitItemKind::System)
        );
        assert_eq!(
            native_quit_item_kind("writer.quit", true),
            Some(NativeQuitItemKind::Command)
        );
        assert_eq!(native_quit_item_kind("writer.exit", true), None);
    }

    #[test]
    fn appkit_installation_status_requires_the_same_root_pointer() {
        let root = 1_u8;
        let other_root = 2_u8;
        let expected = std::ptr::from_ref(&root).cast::<c_void>();
        let actual_other = std::ptr::from_ref(&other_root).cast::<c_void>();

        assert_eq!(
            status_for_appkit_root_pointers(Some(expected), Some(expected)),
            NativeMenuInstallationStatus::Installed
        );
        assert_eq!(
            status_for_appkit_root_pointers(Some(expected), Some(actual_other)),
            NativeMenuInstallationStatus::AppKitRootMismatch
        );
        assert_eq!(
            status_for_appkit_root_pointers(Some(expected), None),
            NativeMenuInstallationStatus::VerificationUnavailable
        );
    }

    #[test]
    fn test_worker_thread_safety_without_appkit_panic() {
        use crate::menu::NativeMenuBar;

        let status = std::thread::spawn(|| {
            let menu_bar = build_standard_menu_bar("Loom Writer", vec![], vec![], vec![], vec![]);
            let service = NativeMenuBar::new();
            let status = service
                .install_menu_bar_with_status(&menu_bar)
                .expect("worker-thread install reports its status");

            assert_eq!(status, NativeMenuInstallationStatus::MainThreadRequired);
            assert_eq!(
                service.installation_status(),
                NativeMenuInstallationStatus::MainThreadRequired
            );
            assert!(!service.is_installed());

            // Native update operations must remain safe no-ops off the AppKit thread.
            update_native_item("file.save", true, None);
            let cmd_state = CommandState::action("file.save", "Save");
            update_native_command_state(&cmd_state);
            status
        })
        .join()
        .expect("worker thread");

        assert_eq!(status, NativeMenuInstallationStatus::MainThreadRequired);
    }
}
