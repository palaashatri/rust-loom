//! Native global menu bar contracts and adapters for Loom applications.
//!
//! Provides structured menus with keyboard shortcuts, enablement states,
//! submenus, and radio/check toggles, supporting macOS AppKit NSMenu
//! and Linux DBusMenu/AppMenu desktop reflections.

use crate::DesktopError;
use std::collections::{BTreeMap, BTreeSet};
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

/// Surface from which a command action was requested.
///
/// Menu items, toolbar controls, keyboard shortcuts, and accessibility APIs
/// all carry the same command identifier.  The source is retained so the
/// controller can record or announce the interaction without maintaining a
/// second per-surface command table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandSource {
    /// A native or in-app menu item.
    Menu,
    /// A toolbar button.
    Toolbar,
    /// A keyboard shortcut.
    Keyboard,
    /// An accessibility default action.
    Accessibility,
}

/// A typed command action shared by desktop surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandAction {
    /// Stable command identifier dispatched to the application controller.
    pub id: String,
    /// Surface that requested the action.
    pub source: CommandSource,
}

impl CommandAction {
    /// Creates an action for `id` from `source`.
    pub fn new(id: impl Into<String>, source: CommandSource) -> Self {
        Self {
            id: id.into(),
            source,
        }
    }
}

/// The state projected to every desktop command surface.
///
/// This is deliberately UI-agnostic.  A menu adapter turns it into a
/// [`MenuItem`], while a toolbar or accessibility adapter consumes the same
/// state and invokes [`CommandStateProjection::dispatch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandState {
    /// Stable command identifier.
    pub id: String,
    /// Visible, localized command label.
    pub label: String,
    /// Whether the command may currently execute.
    pub enabled: bool,
    /// Check state for a toggle command. `None` means a regular action.
    pub checked: Option<bool>,
    /// Keyboard shortcut shown by menu and toolbar adapters.
    pub shortcut: Option<MenuShortcut>,
}

impl CommandState {
    /// Creates an enabled, unchecked action command.
    pub fn action(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            checked: None,
            shortcut: None,
        }
    }

    /// Creates an enabled check command.
    pub fn check(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            checked: Some(checked),
            shortcut: None,
        }
    }

    /// Attaches a keyboard shortcut to the command.
    pub fn with_shortcut(mut self, shortcut: MenuShortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Sets whether the command may execute.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Returns the action used for a keyboard invocation.
    pub fn keyboard_action(&self) -> Option<CommandAction> {
        (self.enabled && self.shortcut.is_some())
            .then(|| CommandAction::new(self.id.clone(), CommandSource::Keyboard))
    }

    /// Returns the default action exposed to assistive technology.
    pub fn accessibility_default_action(&self) -> Option<CommandAction> {
        self.enabled
            .then(|| CommandAction::new(self.id.clone(), CommandSource::Accessibility))
    }

    /// Returns the equivalent toolbar state.
    pub fn toolbar_state(&self) -> Self {
        self.clone()
    }

    /// Converts this state into the menu representation consumed by native
    /// and in-app menu adapters.
    pub fn menu_item(&self) -> MenuItem {
        match self.checked {
            Some(checked) => MenuItem::Check {
                id: self.id.clone(),
                label: self.label.clone(),
                shortcut: self.shortcut.clone(),
                enabled: self.enabled,
                checked,
            },
            None => MenuItem::Action {
                id: self.id.clone(),
                label: self.label.clone(),
                shortcut: self.shortcut.clone(),
                enabled: self.enabled,
            },
        }
    }
}

/// One authoritative projection of command state for all desktop surfaces.
///
/// Applications update this projection after document or selection changes,
/// then apply it to their menu service and use the same entries for toolbar,
/// keyboard, command-palette, and accessibility dispatch.  Dispatch is
/// guarded by `enabled`, so a stale or disabled surface cannot mutate the
/// document by bypassing the projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandStateProjection {
    states: BTreeMap<String, CommandState>,
}

impl CommandStateProjection {
    /// Creates a projection from command states. Duplicate IDs are replaced
    /// by the last state, keeping one canonical entry per command.
    pub fn new(states: impl IntoIterator<Item = CommandState>) -> Self {
        let mut projection = Self::default();
        for state in states {
            projection.insert(state);
        }
        projection
    }

    /// Inserts or replaces a command state.
    pub fn insert(&mut self, state: CommandState) {
        self.states.insert(state.id.clone(), state);
    }

    /// Returns the state for a command identifier.
    pub fn get(&self, id: &str) -> Option<&CommandState> {
        self.states.get(id)
    }

    /// Returns all states in stable identifier order.
    pub fn iter(&self) -> impl Iterator<Item = &CommandState> {
        self.states.values()
    }

    /// Number of projected commands.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Whether no commands are projected.
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Returns the menu item projection for one command.
    pub fn menu_item(&self, id: &str) -> Option<MenuItem> {
        self.get(id).map(CommandState::menu_item)
    }

    /// Returns toolbar state for one command.
    pub fn toolbar_state(&self, id: &str) -> Option<CommandState> {
        self.get(id).map(CommandState::toolbar_state)
    }

    /// Returns menu items for the requested IDs, preserving the supplied
    /// order so application menu layout remains explicit.
    pub fn menu_items<'a>(&'a self, ids: impl IntoIterator<Item = &'a str>) -> Vec<MenuItem> {
        ids.into_iter()
            .filter_map(|id| self.menu_item(id))
            .collect()
    }

    /// Returns the keyboard action for one command.
    pub fn keyboard_action(&self, id: &str) -> Option<CommandAction> {
        self.get(id).and_then(CommandState::keyboard_action)
    }

    /// Returns the accessibility default action for one command.
    pub fn accessibility_default_action(&self, id: &str) -> Option<CommandAction> {
        self.get(id)
            .and_then(CommandState::accessibility_default_action)
    }

    /// Validates and creates a command action for one surface.
    pub fn dispatch(&self, id: &str, source: CommandSource) -> Result<CommandAction, DesktopError> {
        let state = self.get(id).ok_or_else(|| {
            DesktopError::InvalidRequest(format!("command {id} is not projected"))
        })?;
        if !state.enabled {
            return Err(DesktopError::InvalidRequest(format!(
                "command {id} is disabled"
            )));
        }
        if source == CommandSource::Keyboard && state.shortcut.is_none() {
            return Err(DesktopError::InvalidRequest(format!(
                "command {id} has no keyboard shortcut"
            )));
        }
        Ok(CommandAction::new(id, source))
    }

    /// Applies every projected state to an existing menu bar. Unknown IDs are
    /// ignored because application-specific toolbar-only commands need not be
    /// present in the global menu.
    pub fn apply_to_menu_bar(&self, menu_bar: &mut MenuBar) {
        for state in self.iter() {
            menu_bar.apply_command_state(state);
        }
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

    /// Returns the command action associated with this item for `source`.
    /// Separators and submenus do not dispatch commands.
    pub fn command_action(&self, source: CommandSource) -> Option<CommandAction> {
        self.is_enabled()
            .then(|| self.id().map(|id| CommandAction::new(id, source)))
            .flatten()
    }

    /// Returns whether this item has a keyboard shortcut binding.
    pub fn has_keyboard_shortcut(&self) -> bool {
        match self {
            Self::Action { shortcut, .. }
            | Self::Check { shortcut, .. }
            | Self::Radio { shortcut, .. } => shortcut.is_some(),
            Self::Submenu(menu) => menu.items.iter().any(MenuItem::has_keyboard_shortcut),
            Self::Separator => false,
        }
    }

    /// Returns the keyboard action represented by this item, if any.
    pub fn keyboard_action(&self) -> Option<CommandAction> {
        (self.is_enabled() && self.has_keyboard_shortcut())
            .then(|| {
                self.id()
                    .map(|id| CommandAction::new(id, CommandSource::Keyboard))
            })
            .flatten()
    }

    /// Returns the accessibility default action represented by this item, if
    /// any.
    pub fn accessibility_default_action(&self) -> Option<CommandAction> {
        self.command_action(CommandSource::Accessibility)
    }

    /// Returns whether this item or any nested submenu contains `item_id`.
    pub fn contains_id(&self, item_id: &str) -> bool {
        self.id() == Some(item_id)
            || matches!(self, Self::Submenu(menu) if menu.contains_id(item_id))
    }

    /// Applies the complete projected state to this item.
    ///
    /// Labels and shortcuts are updated along with enablement/check state so
    /// dynamic command labels such as “Undo Typing” cannot drift between the
    /// menu and toolbar surfaces.
    pub fn apply_command_state(&mut self, state: &CommandState) -> bool {
        match self {
            Self::Action { id, .. } if id == &state.id && state.checked.is_some() => {
                *self = state.menu_item();
                true
            }
            Self::Action {
                id,
                label,
                shortcut,
                enabled,
            } if id == &state.id && state.checked.is_none() => {
                *label = state.label.clone();
                *shortcut = state.shortcut.clone();
                *enabled = state.enabled;
                true
            }
            Self::Check { id, .. } if id == &state.id && state.checked.is_none() => {
                *self = state.menu_item();
                true
            }
            Self::Check {
                id,
                label,
                shortcut,
                enabled,
                checked,
            } if id == &state.id => {
                *label = state.label.clone();
                *shortcut = state.shortcut.clone();
                *enabled = state.enabled;
                if let Some(value) = state.checked {
                    *checked = value;
                }
                true
            }
            Self::Radio { id, .. } if id == &state.id && state.checked.is_none() => {
                *self = state.menu_item();
                true
            }
            Self::Radio {
                id,
                label,
                shortcut,
                enabled,
                selected,
                ..
            } if id == &state.id => {
                *label = state.label.clone();
                *shortcut = state.shortcut.clone();
                *enabled = state.enabled;
                if let Some(value) = state.checked {
                    *selected = value;
                }
                true
            }
            Self::Submenu(menu) => menu.update_item_from_state(state),
            _ => false,
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

    /// Returns whether this menu or any nested submenu contains `item_id`.
    pub fn contains_id(&self, item_id: &str) -> bool {
        self.items.iter().any(|item| item.contains_id(item_id))
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

    /// Applies a complete command projection to an item by ID.
    pub fn update_item_from_state(&mut self, state: &CommandState) -> bool {
        let mut updated = false;
        for item in &mut self.items {
            updated |= item.apply_command_state(state);
        }
        updated
    }

    fn retain_unique_command_ids(&mut self, seen: &mut BTreeSet<String>) {
        self.items.retain_mut(|item| match item {
            MenuItem::Action { id, .. }
            | MenuItem::Check { id, .. }
            | MenuItem::Radio { id, .. } => seen.insert(id.clone()),
            MenuItem::Submenu(submenu) => {
                submenu.retain_unique_command_ids(seen);
                true
            }
            MenuItem::Separator => true,
        });
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

    /// Applies a complete command projection to an item by ID.
    pub fn apply_command_state(&mut self, state: &CommandState) -> bool {
        let mut updated = false;
        for menu in &mut self.menus {
            updated |= menu.update_item_from_state(state);
        }
        updated
    }

    /// Removes duplicate command identifiers, retaining the first menu entry
    /// in display order. Unique command IDs are required so `find_item`, menu
    /// dispatch, and toolbar projection cannot disagree about which state is
    /// authoritative.
    pub fn deduplicate_command_ids(&mut self) {
        let mut seen = BTreeSet::new();
        for menu in &mut self.menus {
            menu.retain_unique_command_ids(&mut seen);
        }
    }

    /// Returns a command action after checking that the item exists and is
    /// currently enabled.
    pub fn dispatch_action(
        &self,
        item_id: &str,
        source: CommandSource,
    ) -> Result<CommandAction, DesktopError> {
        let item = self.find_item(item_id).ok_or_else(|| {
            DesktopError::InvalidRequest(format!("menu item {item_id} not found"))
        })?;
        if !item.is_enabled() {
            return Err(DesktopError::InvalidRequest(format!(
                "menu item {item_id} is disabled"
            )));
        }
        if source == CommandSource::Keyboard && item.keyboard_action().is_none() {
            return Err(DesktopError::InvalidRequest(format!(
                "menu item {item_id} has no keyboard shortcut"
            )));
        }
        item.command_action(source).ok_or_else(|| {
            DesktopError::InvalidRequest(format!("menu item {item_id} is not actionable"))
        })
    }

    /// Creates a command-state projection from actionable menu entries.
    ///
    /// This is useful for toolbar and accessibility adapters that need to
    /// consume the same state as a native menu. Submenus are traversed
    /// recursively; separators are omitted.
    pub fn command_state_projection(&self) -> CommandStateProjection {
        fn collect(menu: &Menu, states: &mut Vec<CommandState>, seen: &mut BTreeSet<String>) {
            for item in &menu.items {
                match item {
                    MenuItem::Action {
                        id,
                        label,
                        shortcut,
                        enabled,
                    } if seen.insert(id.clone()) => states.push(CommandState {
                        id: id.clone(),
                        label: label.clone(),
                        enabled: *enabled,
                        checked: None,
                        shortcut: shortcut.clone(),
                    }),
                    MenuItem::Check {
                        id,
                        label,
                        shortcut,
                        enabled,
                        checked,
                    } if seen.insert(id.clone()) => states.push(CommandState {
                        id: id.clone(),
                        label: label.clone(),
                        enabled: *enabled,
                        checked: Some(*checked),
                        shortcut: shortcut.clone(),
                    }),
                    MenuItem::Radio {
                        id,
                        label,
                        shortcut,
                        enabled,
                        selected,
                        ..
                    } if seen.insert(id.clone()) => states.push(CommandState {
                        id: id.clone(),
                        label: label.clone(),
                        enabled: *enabled,
                        checked: Some(*selected),
                        shortcut: shortcut.clone(),
                    }),
                    MenuItem::Submenu(submenu) => collect(submenu, states, seen),
                    MenuItem::Separator => {}
                    _ => {}
                }
            }
        }

        let mut states = Vec::new();
        let mut seen = BTreeSet::new();
        for menu in &self.menus {
            collect(menu, &mut states, &mut seen);
        }
        CommandStateProjection::new(states)
    }

    /// Render to DBusMenu layout structure format for Linux desktop reflection.
    pub fn to_dbusmenu_json(&self) -> String {
        let mut json = String::from("{\"menus\":[");
        for (i, menu) in self.menus.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(r#"{{"title":"{}","items":["#, menu.title));
            for (j, item) in menu.items.iter().enumerate() {
                if j > 0 {
                    json.push(',');
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

    /// Applies a complete command state, including dynamic label and
    /// keyboard shortcut, to the installed menu adapter.
    ///
    /// Implementations that only expose legacy `update_item` can retain the
    /// default, while native/scripted adapters override it to keep every
    /// projected field synchronized.
    fn update_command_state(&self, state: &CommandState) -> Result<(), DesktopError> {
        self.update_item(&state.id, state.enabled, state.checked)
    }

    /// Synchronize the menu state from the shared command projection.
    ///
    /// Toolbar and accessibility adapters can consume the same projection
    /// directly; this default implementation keeps native menu state in sync
    /// without requiring each application to duplicate the command list.
    fn sync_command_states(&self, projection: &CommandStateProjection) -> Result<(), DesktopError> {
        for state in projection.iter() {
            self.update_command_state(state)?;
        }
        Ok(())
    }

    /// Dispatch an action by item id.
    fn dispatch_action(&self, item_id: &str) -> Result<(), DesktopError>;

    /// Dispatches a command while retaining the originating surface.
    ///
    /// Implementations must apply the same enablement guard as
    /// [`Self::dispatch_action`]. The default delegates to that method so
    /// existing adapters remain source-compatible.
    fn dispatch_action_from(
        &self,
        item_id: &str,
        source: CommandSource,
    ) -> Result<CommandAction, DesktopError> {
        self.dispatch_action(item_id)?;
        Ok(CommandAction::new(item_id, source))
    }
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

    fn update_command_state(&self, state: &CommandState) -> Result<(), DesktopError> {
        let mut current = self.current.lock().unwrap();
        let menu_bar = current.as_mut().ok_or_else(|| {
            DesktopError::InvalidRequest("native menu bar is not installed".into())
        })?;
        if menu_bar.apply_command_state(state) {
            Ok(())
        } else {
            Err(DesktopError::InvalidRequest(format!(
                "command {} is not present in native menu bar",
                state.id
            )))
        }
    }

    fn dispatch_action(&self, item_id: &str) -> Result<(), DesktopError> {
        self.dispatch_action_from(item_id, CommandSource::Menu)
            .map(|_| ())
    }

    fn dispatch_action_from(
        &self,
        item_id: &str,
        source: CommandSource,
    ) -> Result<CommandAction, DesktopError> {
        let current = self.current.lock().unwrap();
        let menu_bar = current.as_ref().ok_or_else(|| {
            DesktopError::InvalidRequest("native menu bar is not installed".into())
        })?;
        menu_bar.dispatch_action(item_id, source)
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

    fn update_command_state(&self, state: &CommandState) -> Result<(), DesktopError> {
        let mut bars = self.installed_bars.lock().unwrap();
        let bar = bars.last_mut().ok_or_else(|| {
            DesktopError::InvalidRequest("scripted menu bar is not installed".into())
        })?;
        if bar.apply_command_state(state) {
            Ok(())
        } else {
            Err(DesktopError::InvalidRequest(format!(
                "command {} is not present in scripted menu bar",
                state.id
            )))
        }
    }

    fn dispatch_action(&self, item_id: &str) -> Result<(), DesktopError> {
        self.dispatch_action_from(item_id, CommandSource::Menu)
            .map(|_| ())
    }

    fn dispatch_action_from(
        &self,
        item_id: &str,
        source: CommandSource,
    ) -> Result<CommandAction, DesktopError> {
        let bars = self.installed_bars.lock().unwrap();
        let bar = bars.last().ok_or_else(|| {
            DesktopError::InvalidRequest("scripted menu bar is not installed".into())
        })?;
        let action = bar.dispatch_action(item_id, source)?;
        self.dispatched_actions
            .lock()
            .unwrap()
            .push(item_id.to_string());
        Ok(action)
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
    // Keep the standard document commands in one projection.  The menu is
    // only one adapter; callers can derive toolbar, keyboard, and
    // accessibility state from this same source via `MenuBar::command_state_projection`.
    let standard_commands = CommandStateProjection::new([
        CommandState::action("file.new", "New").with_shortcut(MenuShortcut::primary("N")),
        CommandState::action("file.open", "Open...").with_shortcut(MenuShortcut::primary("O")),
        CommandState::action("file.save", "Save").with_shortcut(MenuShortcut::primary("S")),
        CommandState::action("file.save_as", "Save As...")
            .with_shortcut(MenuShortcut::primary_shift("S")),
        CommandState::action("edit.undo", "Undo").with_shortcut(MenuShortcut::primary("Z")),
        CommandState::action("edit.redo", "Redo").with_shortcut(MenuShortcut::primary_shift("Z")),
        CommandState::action("edit.cut", "Cut").with_shortcut(MenuShortcut::primary("X")),
        CommandState::action("edit.copy", "Copy").with_shortcut(MenuShortcut::primary("C")),
        CommandState::action("edit.paste", "Paste").with_shortcut(MenuShortcut::primary("V")),
        CommandState::action("edit.select_all", "Select All")
            .with_shortcut(MenuShortcut::primary("A")),
        CommandState::action("app.palette", "Command Palette...")
            .with_shortcut(MenuShortcut::primary("K")),
        CommandState::action("view.zoom_in", "Zoom In").with_shortcut(MenuShortcut::primary("=")),
        CommandState::action("view.zoom_out", "Zoom Out").with_shortcut(MenuShortcut::primary("-")),
        CommandState::action("view.zoom_actual", "Actual Size")
            .with_shortcut(MenuShortcut::primary("0")),
        // Inspectors are opt-in contextual chrome.  Applications may replace
        // this entry with a live state when their inspector is available.
        CommandState::check("view.inspector", "Format Inspector", false),
    ]);

    // 1. macOS Application Menu (or File on Linux/Windows)
    if cfg!(target_os = "macos") {
        menus.push(Menu::new(
            app_name,
            vec![
                MenuItem::action(
                    format!("{}.about", app_name.to_lowercase()),
                    format!("About {app_name}"),
                ),
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
        standard_commands
            .menu_item("file.new")
            .expect("standard command"),
        standard_commands
            .menu_item("file.open")
            .expect("standard command"),
        MenuItem::Separator,
        standard_commands
            .menu_item("file.save")
            .expect("standard command"),
        standard_commands
            .menu_item("file.save_as")
            .expect("standard command"),
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
        standard_commands
            .menu_item("edit.undo")
            .expect("standard command"),
        standard_commands
            .menu_item("edit.redo")
            .expect("standard command"),
        MenuItem::Separator,
        standard_commands
            .menu_item("edit.cut")
            .expect("standard command"),
        standard_commands
            .menu_item("edit.copy")
            .expect("standard command"),
        standard_commands
            .menu_item("edit.paste")
            .expect("standard command"),
        standard_commands
            .menu_item("edit.select_all")
            .expect("standard command"),
        MenuItem::Separator,
        standard_commands
            .menu_item("app.palette")
            .expect("standard command"),
    ];
    default_edit_items.extend(edit_items);
    menus.push(Menu::new("Edit", default_edit_items));

    // 4. View Menu
    let custom_inspector = view_items
        .iter()
        .any(|item| item.contains_id("view.inspector"));
    let mut default_view_items = vec![
        standard_commands
            .menu_item("view.zoom_in")
            .expect("standard command"),
        standard_commands
            .menu_item("view.zoom_out")
            .expect("standard command"),
        standard_commands
            .menu_item("view.zoom_actual")
            .expect("standard command"),
        MenuItem::Separator,
    ];
    if !custom_inspector {
        default_view_items.push(
            standard_commands
                .menu_item("view.inspector")
                .expect("standard command"),
        );
    }
    default_view_items.extend(view_items);
    menus.push(Menu::new("View", default_view_items));

    // 5. Extra application-specific menus (Insert, Format, Slide, Table, etc.)
    menus.extend(extra_menus);

    // 6. Window Menu
    menus.push(Menu::new(
        "Window",
        vec![
            MenuItem::action_with_shortcut(
                "window.minimize",
                "Minimize",
                MenuShortcut::primary("M"),
            ),
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

    let mut menu_bar = MenuBar::new(menus);
    menu_bar.deduplicate_command_ids();
    menu_bar
}
