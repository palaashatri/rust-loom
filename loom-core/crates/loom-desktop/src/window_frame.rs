//! Whether Loom draws its own title bar or uses the platform's window frame.
//!
//! `System` is the default: the window manager or operating system draws the
//! title bar and borders. `Loom` hides the platform frame and draws Loom's own
//! title bar (`LoomTitleBar`), with the same close and unsaved-changes path.
//! The choice is remembered in the same settings file as the appearance. macOS
//! always keeps its native frame and menu bar, so its `Loom` row is disabled
//! there.
//!
//! This module is plain Rust: the choices, their command ids and labels, the
//! View-menu rows, and the settings key. The Slint side and the glue that
//! applies a choice to a window live in
//! [`window_chrome_bindings!`](crate::window_chrome_bindings).

use std::io;
use std::path::{Path, PathBuf};

use crate::settings_file;
use crate::{CommandStateProjection, MenuItem};

/// Settings file key that holds the window frame.
const KEY: &str = "window_frame";
/// Name of the radio group the two menu rows share.
const MENU_GROUP: &str = "window-frame";
/// Environment variable from the first custom-chrome build. `1` forces Loom's
/// title bar for one launch and is never written back.
pub const ENVIRONMENT_OVERRIDE: &str = "LOOM_CUSTOM_CHROME";

/// Whether this platform can draw Loom's own title bar. Windows and Linux can
/// hide the platform frame and wire the window operations; macOS keeps its own.
pub const SUPPORTED: bool = cfg!(any(target_os = "windows", target_os = "linux"));

/// Which window frame the application draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WindowFrame {
    /// The platform's own title bar and borders.
    #[default]
    System,
    /// Loom's title bar, with the platform frame hidden.
    Loom,
}

impl WindowFrame {
    /// Every choice, in menu order.
    pub const ALL: [WindowFrame; 2] = [WindowFrame::System, WindowFrame::Loom];

    /// The stable word stored in the settings file.
    pub const fn id(self) -> &'static str {
        match self {
            WindowFrame::System => "system",
            WindowFrame::Loom => "loom",
        }
    }

    /// The choice for a stored word, or `None` for anything unknown.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.id() == id)
    }

    /// The command id a View-menu row, the palette or a toolbar sends.
    pub const fn command_id(self) -> &'static str {
        match self {
            WindowFrame::System => "view.window-frame.system",
            WindowFrame::Loom => "view.window-frame.loom",
        }
    }

    /// The choice for a command id, or `None` for any other id.
    pub fn from_command_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.command_id() == id)
    }

    /// The name shown in the menu.
    pub const fn label(self) -> &'static str {
        match self {
            WindowFrame::System => "System title bar",
            WindowFrame::Loom => "Loom title bar",
        }
    }

    /// Whether this choice hides the platform frame and draws Loom's title bar.
    pub const fn draws_own_title_bar(self) -> bool {
        matches!(self, WindowFrame::Loom)
    }
}

/// The separator and two radio rows that make up the window-frame part of a
/// View menu. `System` starts selected; the live choice is applied with
/// [`project_checks`]. Where the platform cannot draw Loom's title bar, the
/// `Loom` row is listed but disabled.
pub fn menu_items() -> Vec<MenuItem> {
    menu_items_for(SUPPORTED)
}

fn menu_items_for(supported: bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::Separator];
    items.extend(WindowFrame::ALL.into_iter().map(|choice| MenuItem::Radio {
        id: choice.command_id().to_string(),
        group: MENU_GROUP.to_string(),
        label: choice.label().to_string(),
        shortcut: None,
        enabled: supported || choice == WindowFrame::System,
        selected: choice == WindowFrame::System,
    }));
    items
}

/// Marks exactly the current choice as selected in a command projection.
pub fn project_checks(projection: &mut CommandStateProjection, current: WindowFrame) {
    for choice in WindowFrame::ALL {
        if let Some(mut state) = projection.get(choice.command_id()).cloned() {
            state.checked = Some(choice == current);
            projection.insert(state);
        }
    }
}

/// Where one application remembers its window frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFrameStore {
    path: PathBuf,
}

impl WindowFrameStore {
    /// A store backed by an explicit file.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The per-user store for one application id. It shares the settings file
    /// that holds the appearance.
    pub fn for_application(application_id: &str) -> Self {
        Self::at(settings_file::path_for_application(application_id))
    }

    /// The settings file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved choice. A missing, unreadable, oversized or corrupt file, or
    /// an unknown value, is `System`.
    pub fn load(&self) -> WindowFrame {
        settings_file::read(&self.path, KEY)
            .as_deref()
            .and_then(WindowFrame::from_id)
            .unwrap_or_default()
    }

    /// Remembers a choice. Other settings already in the file are kept.
    pub fn save(&self, frame: WindowFrame) -> io::Result<()> {
        settings_file::write(&self.path, KEY, frame.id())
    }
}

/// The frame to start with.
///
/// Where the platform cannot draw Loom's title bar, the answer is always
/// `System`. An environment value of `1` forces `Loom` for this launch and is
/// never written back. Otherwise the saved choice is used.
pub fn startup_frame(
    store: &WindowFrameStore,
    supported: bool,
    environment: Option<&str>,
) -> WindowFrame {
    if !supported {
        WindowFrame::System
    } else if environment == Some("1") {
        WindowFrame::Loom
    } else {
        store.load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandState, Menu, MenuBar};

    fn temporary_store(name: &str) -> WindowFrameStore {
        let directory =
            std::env::temp_dir().join(format!("loom-frame-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temporary directory");
        WindowFrameStore::at(directory.join("settings.toml"))
    }

    #[test]
    fn ids_and_command_ids_round_trip_and_are_distinct() {
        for choice in WindowFrame::ALL {
            assert_eq!(WindowFrame::from_id(choice.id()), Some(choice));
            assert_eq!(
                WindowFrame::from_command_id(choice.command_id()),
                Some(choice)
            );
        }
        assert_ne!(WindowFrame::System.id(), WindowFrame::Loom.id());
        assert_eq!(WindowFrame::from_id("native-csd"), None);
        assert_eq!(WindowFrame::from_command_id("view.appearance.dark"), None);
    }

    #[test]
    fn only_the_loom_choice_draws_its_own_title_bar() {
        assert!(!WindowFrame::System.draws_own_title_bar());
        assert!(WindowFrame::Loom.draws_own_title_bar());
        assert_eq!(WindowFrame::default(), WindowFrame::System);
    }

    #[test]
    fn menu_rows_are_two_radios_and_loom_is_disabled_where_unsupported() {
        let supported = menu_items_for(true);
        assert!(matches!(supported[0], MenuItem::Separator));
        assert_eq!(supported.len(), 3);
        let loom_enabled = supported.iter().find_map(|item| match item {
            MenuItem::Radio {
                id,
                enabled,
                selected,
                ..
            } if id == WindowFrame::Loom.command_id() => Some((*enabled, *selected)),
            _ => None,
        });
        assert_eq!(loom_enabled, Some((true, false)));

        let unsupported = menu_items_for(false);
        let loom_disabled = unsupported.iter().find_map(|item| match item {
            MenuItem::Radio { id, enabled, .. } if id == WindowFrame::Loom.command_id() => {
                Some(*enabled)
            }
            _ => None,
        });
        assert_eq!(loom_disabled, Some(false));
    }

    #[test]
    fn checks_follow_the_current_choice() {
        let bar = MenuBar::new([Menu::new("View", menu_items())]);
        let mut projection = bar.command_state_projection();
        for current in WindowFrame::ALL {
            project_checks(&mut projection, current);
            for choice in WindowFrame::ALL {
                let state: &CommandState = projection.get(choice.command_id()).expect("state");
                assert_eq!(state.checked, Some(choice == current));
                assert_eq!(state.label, choice.label());
            }
        }
    }

    #[test]
    fn a_saved_choice_survives_a_new_store_and_keeps_the_appearance() {
        let store = temporary_store("survives");
        settings_file::write(store.path(), "appearance", "dark").expect("appearance");
        assert_eq!(store.load(), WindowFrame::System);
        store.save(WindowFrame::Loom).expect("save frame");
        let reopened = WindowFrameStore::at(store.path());
        assert_eq!(reopened.load(), WindowFrame::Loom);
        assert_eq!(
            settings_file::read(store.path(), "appearance").as_deref(),
            Some("dark"),
            "saving the window frame must keep the appearance"
        );
    }

    #[test]
    fn corrupt_or_unknown_saved_values_fall_back_to_system() {
        let store = temporary_store("corrupt");
        settings_file::write(store.path(), KEY, "csd-maybe").expect("write");
        assert_eq!(store.load(), WindowFrame::System);
    }

    #[test]
    fn startup_uses_the_saved_choice_and_an_environment_override_wins_without_saving() {
        let store = temporary_store("startup");
        store.save(WindowFrame::Loom).expect("save");
        assert_eq!(startup_frame(&store, true, None), WindowFrame::Loom);
        assert_eq!(startup_frame(&store, true, Some("0")), WindowFrame::Loom);

        let system_store = temporary_store("startup-system");
        assert_eq!(
            startup_frame(&system_store, true, Some("1")),
            WindowFrame::Loom,
            "the override forces Loom for one launch"
        );
        assert_eq!(
            system_store.load(),
            WindowFrame::System,
            "the override is never written back"
        );
        assert_eq!(
            startup_frame(&store, false, Some("1")),
            WindowFrame::System,
            "unsupported platforms always keep the platform frame"
        );
    }
}
