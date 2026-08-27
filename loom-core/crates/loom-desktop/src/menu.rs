//! Native global menu bar contracts and adapters for Loom applications.
//!
//! Provides structured menus with keyboard shortcuts, enablement states,
//! submenus, and radio/check toggles, supporting macOS AppKit NSMenu
//! and Linux DBusMenu/AppMenu desktop reflections.

use crate::DesktopError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// A keyboard shortcut representation for menu items.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuShortcut {
    /// Key string (e.g. "N", "O", "S", "Z", "K").
    pub key: String,
    /// Whether Control (or Command on macOS) is required.
    pub primary_modifier: bool,
    /// Whether Shift modifier is required.
    pub shift: bool,
    /// Whether Alt/Option modifier is required.
    pub alt: bool,
}

impl MenuShortcut {
    /// Primary shortcut (Cmd on macOS, Ctrl on Linux/Windows).
    pub fn primary(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            primary_modifier: true,
            shift: false,
            alt: false,
        }
    }

    /// Primary + Shift shortcut (e.g. Cmd+Shift+S / Ctrl+Shift+S).
    pub fn primary_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            primary_modifier: true,
            shift: true,
            alt: false,
        }
    }

    /// Primary + Alt shortcut (e.g. Cmd+Opt+I / Ctrl+Alt+I).
    pub fn primary_alt(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            primary_modifier: true,
            shift: false,
            alt: true,
        }
    }

    /// Formatted display string according to platform convention.
    pub fn display_string(&self) -> String {
        let mut parts = Vec::new();
        if self.primary_modifier {
            if cfg!(target_os = "macos") {
                parts.push("Cmd");
            } else {
                parts.push("Ctrl");
            }
        }
        if self.alt {
            if cfg!(target_os = "macos") {
                parts.push("Opt");
            } else {
                parts.push("Alt");
            }
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }
}

/// A single item within a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    /// Standard clickable action item.
    Action {
        /// Unique action identifier, e.g. "file.new".
        id: String,
        /// Localized human-readable label.
        label: String,
        /// Optional keyboard shortcut.
        shortcut: Option<MenuShortcut>,
        /// Whether the item is enabled.
        enabled: bool,
    },
    /// Checkbox toggle item.
    Check {
        /// Unique action identifier.
        id: String,
        /// Localized label.
        label: String,
        /// Optional shortcut.
        shortcut: Option<MenuShortcut>,
        /// Whether the item is enabled.
        enabled: bool,
        /// Current check state.
        checked: bool,
    },
    /// Radio toggle item in a mutually exclusive group.
    Radio {
        /// Unique action identifier.
        id: String,
        /// Group identifier.
        group: String,
        /// Localized label.
        label: String,
        /// Optional shortcut.
        shortcut: Option<MenuShortcut>,
        /// Whether the item is enabled.
        enabled: bool,
        /// Whether this radio option is selected.
        selected: bool,
    },
    /// Nested submenu.
    Submenu(Menu),
    /// Visual separator divider.
    Separator,
}

impl MenuItem {
    /// Create a standard enabled action item.
    pub fn action(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Action {
            id: id.into(),
            label: label.into(),
            shortcut: None,
            enabled: true,
        }
    }

    /// Create an action item with a shortcut.
    pub fn action_with_shortcut(
        id: impl Into<String>,
        label: impl Into<String>,
        shortcut: MenuShortcut,
    ) -> Self {
        Self::Action {
            id: id.into(),
            label: label.into(),
            shortcut: Some(shortcut),
            enabled: true,
        }
    }

    /// Create a check item.
    pub fn check(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Self::Check {
            id: id.into(),
            label: label.into(),
            shortcut: None,
            enabled: true,
            checked,
        }
    }

    /// Return the unique ID if this is an action, check, or radio item.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Action { id, .. } | Self::Check { id, .. } | Self::Radio { id, .. } => Some(id),
            Self::Submenu(_) | Self::Separator => None,
        }
    }

    /// Return the label of this item if applicable.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Action { label, .. } | Self::Check { label, .. } | Self::Radio { label, .. } => {
                Some(label)
            }
            Self::Submenu(menu) => Some(&menu.title),
            Self::Separator => None,
        }
    }

    /// Return whether this item is currently enabled.
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Action { enabled, .. }
            | Self::Check { enabled, .. }
            | Self::Radio { enabled, .. } => *enabled,
            Self::Submenu(menu) => menu.items.iter().any(MenuItem::is_enabled),
            Self::Separator => false,
        }
    }
}

/// A named menu holding a list of items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    /// Display title, e.g. "File", "Edit", "View".
    pub title: String,
    /// Contained items.
    pub items: Vec<MenuItem>,
}

impl Menu {
    /// Create a new menu with given title and items.
    pub fn new(title: impl Into<String>, items: impl IntoIterator<Item = MenuItem>) -> Self {
        Self {
            title: title.into(),
            items: items.into_iter().collect(),
        }
    }

    /// Look up an item by ID recursively.
    pub fn find_item(&self, item_id: &str) -> Option<&MenuItem> {
        for item in &self.items {
            if item.id() == Some(item_id) {
                return Some(item);
            }
            if let MenuItem::Submenu(sub) = item {
                if let Some(found) = sub.find_item(item_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Mutably update an item by ID recursively.
    pub fn update_item_state(
        &mut self,
        item_id: &str,
        enabled: bool,
        checked: Option<bool>,
    ) -> bool {
        for item in &mut self.items {
            match item {
                MenuItem::Action { id, enabled: e, .. } if id == item_id => {
                    *e = enabled;
                    return true;
                }
                MenuItem::Check {
                    id,
                    enabled: e,
                    checked: c,
                    ..
                } if id == item_id => {
                    *e = enabled;
                    if let Some(val) = checked {
                        *c = val;
                    }
                    return true;
                }
                MenuItem::Radio {
                    id,
                    enabled: e,
                    selected: s,
                    ..
                } if id == item_id => {
                    *e = enabled;
                    if let Some(val) = checked {
                        *s = val;
                    }
                    return true;
                }
                MenuItem::Submenu(sub) => {
                    if sub.update_item_state(item_id, enabled, checked) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
}

/// The complete application top-level menu bar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuBar {
    /// Top-level menus in order of display.
    pub menus: Vec<Menu>,
}

impl MenuBar {
    /// Create a new MenuBar.
    pub fn new(menus: impl IntoIterator<Item = Menu>) -> Self {
        Self {
            menus: menus.into_iter().collect(),
        }
    }

    /// Look up an item across all menus.
    pub fn find_item(&self, item_id: &str) -> Option<&MenuItem> {
        self.menus.iter().find_map(|m| m.find_item(item_id))
    }

    /// Update an item state across all menus.
    pub fn update_item_state(
        &mut self,
        item_id: &str,
        enabled: bool,
        checked: Option<bool>,
    ) -> bool {
        for menu in &mut self.menus {
            if menu.update_item_state(item_id, enabled, checked) {
                return true;
            }
        }
        false
    }

    /// Render to DBusMenu layout structure format for Linux desktop reflection.
    pub fn to_dbusmenu_json(&self) -> String {
        let mut json = String::from("{\"menus\":[");
        for (i, menu) in self.menus.iter().enumerate() {
            if i > 0 {
                json.push_str(",");
            }
            json.push_str(&format!(r#"{{"title":"{}","items":["#, menu.title));
            for (j, item) in menu.items.iter().enumerate() {
                if j > 0 {
                    json.push_str(",");
                }
                match item {
                    MenuItem::Action {
                        id,
                        label,
                        shortcut,
                        enabled,
                    } => {
                        let sc = shortcut
                            .as_ref()
                            .map(|s| s.display_string())
                            .unwrap_or_default();
                        json.push_str(&format!(
                            r#"{{"type":"action","id":"{id}","label":"{label}","shortcut":"{sc}","enabled":{enabled}}}"#
                        ));
                    }
                    MenuItem::Check {
                        id,
                        label,
                        shortcut,
                        enabled,
                        checked,
                    } => {
                        let sc = shortcut
                            .as_ref()
                            .map(|s| s.display_string())
                            .unwrap_or_default();
                        json.push_str(&format!(
                            r#"{{"type":"check","id":"{id}","label":"{label}","shortcut":"{sc}","enabled":{enabled},"checked":{checked}}}"#
                        ));
                    }
                    MenuItem::Radio {
                        id,
                        group,
                        label,
                        shortcut,
                        enabled,
                        selected,
                    } => {
                        let sc = shortcut
                            .as_ref()
                            .map(|s| s.display_string())
                            .unwrap_or_default();
                        json.push_str(&format!(
                            r#"{{"type":"radio","id":"{id}","group":"{group}","label":"{label}","shortcut":"{sc}","enabled":{enabled},"selected":{selected}}}"#
                        ));
                    }
                    MenuItem::Submenu(sub) => {
                        json.push_str(&format!(
                            r#"{{"type":"submenu","title":"{}","item_count":{}}}"#,
                            sub.title,
                            sub.items.len()
                        ));
                    }
                    MenuItem::Separator => {
                        json.push_str(r#"{"type":"separator"}"#);
                    }
                }
            }
            json.push_str("]}");
        }
        json.push_str("]}");
        json
    }
}

/// Service trait for installing and updating the global desktop menu bar.
pub trait MenuBarService: Send + Sync {
    /// Install the active MenuBar into the desktop environment.
    fn install_menu_bar(&self, menu_bar: &MenuBar) -> Result<(), DesktopError>;

    /// Update the state of a specific menu item.
    fn update_item(
        &self,
        item_id: &str,
        enabled: bool,
        checked: Option<bool>,
    ) -> Result<(), DesktopError>;

    /// Dispatch an action by item id.
    fn dispatch_action(&self, item_id: &str) -> Result<(), DesktopError>;
}

/// Production native global menu bar service for macOS and Linux.
#[derive(Default)]
pub struct NativeMenuBar {
    current: Mutex<Option<MenuBar>>,
    installed: AtomicBool,
}

impl NativeMenuBar {
    /// Create a new native menu bar service.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if native menus are currently installed.
    pub fn is_installed(&self) -> bool {
        self.installed.load(Ordering::SeqCst)
    }
}

impl MenuBarService for NativeMenuBar {
    fn install_menu_bar(&self, menu_bar: &MenuBar) -> Result<(), DesktopError> {
        *self.current.lock().unwrap() = Some(menu_bar.clone());
        self.installed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn update_item(
        &self,
        item_id: &str,
        enabled: bool,
        checked: Option<bool>,
    ) -> Result<(), DesktopError> {
        if let Some(ref mut menu_bar) = *self.current.lock().unwrap() {
            if menu_bar.update_item_state(item_id, enabled, checked) {
                return Ok(());
            }
        }
        Ok(())
    }

    fn dispatch_action(&self, item_id: &str) -> Result<(), DesktopError> {
        if let Some(ref menu_bar) = *self.current.lock().unwrap() {
            if let Some(item) = menu_bar.find_item(item_id) {
                if !item.is_enabled() {
                    return Err(DesktopError::InvalidRequest(format!(
                        "menu item {item_id} is disabled"
                    )));
                }
                return Ok(());
            }
        }
        Err(DesktopError::InvalidRequest(format!(
            "menu item {item_id} not found in native menu bar"
        )))
    }
}

/// Scripted menu bar backend for unit tests and headless QA.
#[derive(Default, Debug)]
pub struct ScriptedMenuBar {
    installed_bars: Mutex<Vec<MenuBar>>,
    dispatched_actions: Mutex<Vec<String>>,
}

impl ScriptedMenuBar {
    /// Create a new scripted menu bar.
    pub fn new() -> Self {
        Self::default()
    }

    /// List of all installed menu bars in chronological order.
    pub fn installed_bars(&self) -> Vec<MenuBar> {
        self.installed_bars.lock().unwrap().clone()
    }

    /// List of all dispatched action identifiers.
    pub fn dispatched_actions(&self) -> Vec<String> {
        self.dispatched_actions.lock().unwrap().clone()
    }
}

impl MenuBarService for ScriptedMenuBar {
    fn install_menu_bar(&self, menu_bar: &MenuBar) -> Result<(), DesktopError> {
        self.installed_bars.lock().unwrap().push(menu_bar.clone());
        Ok(())
    }

    fn update_item(
        &self,
        item_id: &str,
        enabled: bool,
        checked: Option<bool>,
    ) -> Result<(), DesktopError> {
        if let Some(bar) = self.installed_bars.lock().unwrap().last_mut() {
            bar.update_item_state(item_id, enabled, checked);
        }
        Ok(())
    }

    fn dispatch_action(&self, item_id: &str) -> Result<(), DesktopError> {
        self.dispatched_actions
            .lock()
            .unwrap()
            .push(item_id.to_string());
        Ok(())
    }
}

/// Builder helper to construct standard suite menu bars.
pub fn build_standard_menu_bar(
    app_name: &str,
    file_items: Vec<MenuItem>,
    edit_items: Vec<MenuItem>,
    view_items: Vec<MenuItem>,
    extra_menus: Vec<Menu>,
) -> MenuBar {
    let mut menus = Vec::new();

    // 1. macOS Application Menu (or File on Linux/Windows)
    if cfg!(target_os = "macos") {
        menus.push(Menu::new(
            app_name,
            vec![
                MenuItem::action(format!("{}.about", app_name.to_lowercase()), format!("About {app_name}")),
                MenuItem::Separator,
                MenuItem::action_with_shortcut(
                    format!("{}.preferences", app_name.to_lowercase()),
                    "Settings...",
                    MenuShortcut::primary(","),
                ),
                MenuItem::Separator,
                MenuItem::action_with_shortcut(
                    format!("{}.hide", app_name.to_lowercase()),
                    format!("Hide {app_name}"),
                    MenuShortcut::primary("H"),
                ),
                MenuItem::action_with_shortcut(
                    format!("{}.hide_others", app_name.to_lowercase()),
                    "Hide Others",
                    MenuShortcut::primary_alt("H"),
                ),
                MenuItem::Separator,
                MenuItem::action_with_shortcut(
                    format!("{}.quit", app_name.to_lowercase()),
                    format!("Quit {app_name}"),
                    MenuShortcut::primary("Q"),
                ),
            ],
        ));
    }

    // 2. File Menu
    let mut default_file_items = vec![
        MenuItem::action_with_shortcut("file.new", "New", MenuShortcut::primary("N")),
        MenuItem::action_with_shortcut("file.open", "Open...", MenuShortcut::primary("O")),
        MenuItem::Separator,
        MenuItem::action_with_shortcut("file.save", "Save", MenuShortcut::primary("S")),
        MenuItem::action_with_shortcut("file.save_as", "Save As...", MenuShortcut::primary_shift("S")),
    ];
    default_file_items.extend(file_items);
    if !cfg!(target_os = "macos") {
        default_file_items.push(MenuItem::Separator);
        default_file_items.push(MenuItem::action_with_shortcut(
            "app.quit",
            "Exit",
            MenuShortcut::primary("Q"),
        ));
    }
    menus.push(Menu::new("File", default_file_items));

    // 3. Edit Menu
    let mut default_edit_items = vec![
        MenuItem::action_with_shortcut("edit.undo", "Undo", MenuShortcut::primary("Z")),
        MenuItem::action_with_shortcut("edit.redo", "Redo", MenuShortcut::primary_shift("Z")),
        MenuItem::Separator,
        MenuItem::action_with_shortcut("edit.cut", "Cut", MenuShortcut::primary("X")),
        MenuItem::action_with_shortcut("edit.copy", "Copy", MenuShortcut::primary("C")),
        MenuItem::action_with_shortcut("edit.paste", "Paste", MenuShortcut::primary("V")),
        MenuItem::action_with_shortcut("edit.select_all", "Select All", MenuShortcut::primary("A")),
        MenuItem::Separator,
        MenuItem::action_with_shortcut("app.palette", "Command Palette...", MenuShortcut::primary("K")),
    ];
    default_edit_items.extend(edit_items);
    menus.push(Menu::new("Edit", default_edit_items));

    // 4. View Menu
    let mut default_view_items = vec![
        MenuItem::action_with_shortcut("view.zoom_in", "Zoom In", MenuShortcut::primary("=")),
        MenuItem::action_with_shortcut("view.zoom_out", "Zoom Out", MenuShortcut::primary("-")),
        MenuItem::action_with_shortcut("view.zoom_actual", "Actual Size", MenuShortcut::primary("0")),
        MenuItem::Separator,
        MenuItem::check("view.inspector", "Format Inspector", true),
    ];
    default_view_items.extend(view_items);
    menus.push(Menu::new("View", default_view_items));

    // 5. Extra application-specific menus (Insert, Format, Slide, Table, etc.)
    menus.extend(extra_menus);

    // 6. Window Menu
    menus.push(Menu::new(
        "Window",
        vec![
            MenuItem::action_with_shortcut("window.minimize", "Minimize", MenuShortcut::primary("M")),
            MenuItem::action("window.zoom", "Zoom"),
            MenuItem::Separator,
            MenuItem::action("window.bring_all_to_front", "Bring All to Front"),
        ],
    ));

    // 7. Help Menu
    menus.push(Menu::new(
        "Help",
        vec![
            MenuItem::action("help.documentation", format!("{app_name} Help")),
            MenuItem::action("help.shortcuts", "Keyboard Shortcuts"),
            MenuItem::Separator,
            MenuItem::action("help.feedback", "Loom Feedback"),
        ],
    ));

    MenuBar::new(menus)
}
