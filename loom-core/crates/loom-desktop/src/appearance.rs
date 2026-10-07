//! The user's appearance choice (System, Light, Dark, High Contrast) and where
//! it is remembered.
//!
//! This module is plain Rust. It knows the four choices, the command ids and
//! labels every surface shares, how a choice resolves to a theme name, the
//! radio rows of the View menu, and a small settings file under the user's
//! configuration directory. The Slint side (the `Theme` global in `loom-ui`)
//! and the glue that applies a choice to a window live in
//! [`appearance_bindings!`](crate::appearance_bindings).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::{CommandStateProjection, MenuItem};

/// Largest settings file that is read. A bigger file is treated as corrupt.
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
/// Settings file key that holds the appearance.
const KEY: &str = "appearance";
/// Name of the radio group the four menu rows share.
const MENU_GROUP: &str = "appearance";

/// How the application chooses its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Appearance {
    /// Follow the operating system's light or dark preference. Light when the
    /// platform does not report one.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
    /// Always high contrast.
    HighContrast,
}

impl Appearance {
    /// Every choice, in menu order.
    pub const ALL: [Appearance; 4] = [
        Appearance::System,
        Appearance::Light,
        Appearance::Dark,
        Appearance::HighContrast,
    ];

    /// The stable name used in the settings file and by `--theme`.
    pub const fn id(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
            Appearance::HighContrast => "high-contrast",
        }
    }

    /// Reads a stable name back; `None` for anything unknown.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.id() == id)
    }

    /// The command id shared by the menu, the palette and the toolbar.
    pub const fn command_id(self) -> &'static str {
        match self {
            Appearance::System => "view.appearance.system",
            Appearance::Light => "view.appearance.light",
            Appearance::Dark => "view.appearance.dark",
            Appearance::HighContrast => "view.appearance.high_contrast",
        }
    }

    /// The choice a command id names, if any.
    pub fn from_command_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.command_id() == id)
    }

    /// The visible name of the choice, used by the menu and the palette.
    pub const fn label(self) -> &'static str {
        match self {
            Appearance::System => "Appearance: System",
            Appearance::Light => "Appearance: Light",
            Appearance::Dark => "Appearance: Dark",
            Appearance::HighContrast => "Appearance: High Contrast",
        }
    }

    /// The theme that is drawn: `"light"`, `"dark"` or `"high-contrast"`.
    /// `System` follows the operating system.
    pub const fn resolve(self, system_is_dark: bool) -> &'static str {
        match self {
            Appearance::System if system_is_dark => "dark",
            Appearance::System | Appearance::Light => "light",
            Appearance::Dark => "dark",
            Appearance::HighContrast => "high-contrast",
        }
    }

    /// The sentence announced to assistive technology after a change.
    pub fn announcement(self, system_is_dark: bool) -> String {
        match self {
            Appearance::System => format!(
                "{} (following the system: {})",
                self.label(),
                if system_is_dark { "dark" } else { "light" }
            ),
            _ => self.label().to_string(),
        }
    }
}

/// The separator and four radio rows that make up the appearance part of a
/// View menu. `System` starts selected; the live choice is applied with
/// [`project_checks`].
pub fn menu_items() -> Vec<MenuItem> {
    let mut items = vec![MenuItem::Separator];
    items.extend(Appearance::ALL.into_iter().map(|choice| MenuItem::Radio {
        id: choice.command_id().to_string(),
        group: MENU_GROUP.to_string(),
        label: choice.label().to_string(),
        shortcut: None,
        enabled: true,
        selected: choice == Appearance::System,
    }));
    items
}

/// Marks exactly the current choice as selected in a command projection.
pub fn project_checks(projection: &mut CommandStateProjection, current: Appearance) {
    for choice in Appearance::ALL {
        if let Some(mut state) = projection.get(choice.command_id()).cloned() {
            state.checked = Some(choice == current);
            projection.insert(state);
        }
    }
}

/// Where one application remembers its appearance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppearanceStore {
    path: PathBuf,
}

impl AppearanceStore {
    /// A store backed by an explicit file.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The per-user store for one application id such as `org.loom.sheets`.
    ///
    /// The directory is `LOOM_CONFIG_DIR` when set; otherwise `%APPDATA%` on
    /// Windows, `~/Library/Application Support` on macOS, and
    /// `$XDG_CONFIG_HOME` or `~/.config` elsewhere, with `loom/<id>` below it.
    pub fn for_application(application_id: &str) -> Self {
        Self::at(settings_directory(application_id).join("settings.toml"))
    }

    /// The settings file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved choice. A missing, unreadable, oversized or corrupt file, or
    /// an unknown value, is `System`.
    pub fn load(&self) -> Appearance {
        self.read_text()
            .as_deref()
            .and_then(parse_appearance)
            .unwrap_or_default()
    }

    /// Remembers a choice. The file is replaced in one step, so a crash never
    /// leaves half a file. Other `key = value` lines already in the file are
    /// kept.
    pub fn save(&self, appearance: Appearance) -> io::Result<()> {
        let mut text = String::from("# Loom settings\n");
        text.push_str(&format!("{KEY} = \"{}\"\n", appearance.id()));
        if let Some(existing) = self.read_text() {
            for line in existing.lines() {
                let trimmed = line.trim();
                let is_other_setting = trimmed
                    .split_once('=')
                    .is_some_and(|(key, _)| !trimmed.starts_with('#') && key.trim() != KEY);
                if is_other_setting {
                    text.push_str(trimmed);
                    text.push('\n');
                }
            }
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut temporary = self.path.clone().into_os_string();
        temporary.push(format!(".tmp{}", std::process::id()));
        let temporary = PathBuf::from(temporary);
        let result = (|| {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn read_text(&self) -> Option<String> {
        let metadata = fs::metadata(&self.path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_SETTINGS_BYTES {
            return None;
        }
        fs::read_to_string(&self.path).ok()
    }
}

/// The appearance to start with. An explicit `--theme` value wins and is never
/// written back; otherwise the saved choice is used.
pub fn startup_appearance(store: &AppearanceStore, flag: Option<&str>) -> Appearance {
    match flag {
        Some(name) => Appearance::from_id(name).unwrap_or(Appearance::Light),
        None => store.load(),
    }
}

fn parse_appearance(text: &str) -> Option<Appearance> {
    text.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        if key.trim() != KEY {
            return None;
        }
        Appearance::from_id(value.trim().trim_matches('"'))
    })
}

fn settings_directory(application_id: &str) -> PathBuf {
    if let Some(directory) = std::env::var_os("LOOM_CONFIG_DIR") {
        return PathBuf::from(directory).join(application_id);
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        })
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }
    .unwrap_or_else(|| PathBuf::from(".loom-config"));
    base.join("loom").join(application_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandState, Menu, MenuBar};

    fn temporary_store(name: &str) -> (PathBuf, AppearanceStore) {
        let directory = std::env::temp_dir().join(format!(
            "loom-appearance-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        let store = AppearanceStore::at(directory.join("nested").join("settings.toml"));
        (directory, store)
    }

    #[test]
    fn ids_and_command_ids_round_trip_and_are_distinct() {
        for choice in Appearance::ALL {
            assert_eq!(Appearance::from_id(choice.id()), Some(choice));
            assert_eq!(
                Appearance::from_command_id(choice.command_id()),
                Some(choice)
            );
            assert!(choice.label().starts_with("Appearance: "));
        }
        assert_eq!(Appearance::from_id("sepia"), None);
        assert_eq!(Appearance::from_command_id("view.inspector"), None);
    }

    #[test]
    fn system_follows_the_platform_and_falls_back_to_light() {
        assert_eq!(Appearance::System.resolve(false), "light");
        assert_eq!(Appearance::System.resolve(true), "dark");
        assert_eq!(Appearance::Light.resolve(true), "light");
        assert_eq!(Appearance::Dark.resolve(false), "dark");
        assert_eq!(Appearance::HighContrast.resolve(false), "high-contrast");
        assert_eq!(Appearance::HighContrast.resolve(true), "high-contrast");
    }

    #[test]
    fn announcements_name_the_choice_and_what_system_resolved_to() {
        assert_eq!(Appearance::Dark.announcement(false), "Appearance: Dark");
        assert_eq!(
            Appearance::System.announcement(true),
            "Appearance: System (following the system: dark)"
        );
    }

    #[test]
    fn a_choice_survives_a_new_store_on_the_same_file() {
        let (directory, store) = temporary_store("persist");
        assert_eq!(store.load(), Appearance::System, "no file yet");
        store.save(Appearance::Dark).expect("save");
        assert_eq!(AppearanceStore::at(store.path()).load(), Appearance::Dark);
        store.save(Appearance::HighContrast).expect("save again");
        assert_eq!(
            AppearanceStore::at(store.path()).load(),
            Appearance::HighContrast
        );
        assert!(
            fs::read_dir(store.path().parent().expect("parent"))
                .expect("list")
                .all(|entry| !entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp")),
            "no temporary file is left behind"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn corrupt_oversized_or_unknown_settings_fall_back_to_system() {
        let (directory, store) = temporary_store("corrupt");
        fs::create_dir_all(store.path().parent().expect("parent")).expect("create");
        for bytes in [
            &b"\xff\xfe\x00garbage"[..],
            b"appearance",
            b"appearance = \"sepia\"",
            b"= = =",
            b"",
        ] {
            fs::write(store.path(), bytes).expect("write");
            assert_eq!(store.load(), Appearance::System, "{bytes:?}");
        }
        fs::write(
            store.path(),
            format!("appearance = \"dark\"\n{}", "#".repeat(70_000)),
        )
        .expect("write large");
        assert_eq!(store.load(), Appearance::System, "oversized file");
        // A corrupt file is repaired by the next save.
        store.save(Appearance::Light).expect("save over corrupt");
        assert_eq!(store.load(), Appearance::Light);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn saving_keeps_other_settings() {
        let (directory, store) = temporary_store("others");
        fs::create_dir_all(store.path().parent().expect("parent")).expect("create");
        fs::write(store.path(), "zoom = 1.5\nappearance = \"dark\"\n# note\n").expect("write");
        store.save(Appearance::Light).expect("save");
        let text = fs::read_to_string(store.path()).expect("read");
        assert!(text.contains("zoom = 1.5"), "{text}");
        assert!(text.contains("appearance = \"light\""), "{text}");
        assert_eq!(text.matches("appearance").count(), 1, "{text}");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn an_explicit_flag_overrides_the_saved_choice_and_never_writes() {
        let (directory, store) = temporary_store("flag");
        store.save(Appearance::Dark).expect("save");
        assert_eq!(startup_appearance(&store, Some("light")), Appearance::Light);
        assert_eq!(
            startup_appearance(&store, Some("system")),
            Appearance::System
        );
        assert_eq!(
            startup_appearance(&store, Some("nonsense")),
            Appearance::Light
        );
        assert_eq!(startup_appearance(&store, None), Appearance::Dark);
        assert_eq!(
            store.load(),
            Appearance::Dark,
            "flag did not overwrite the setting"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn the_menu_rows_are_four_radios_and_checks_follow_the_choice() {
        let items = menu_items();
        assert!(matches!(items[0], MenuItem::Separator));
        assert_eq!(items.len(), 5);
        let bar = MenuBar::new([Menu::new("View", items)]);
        let mut projection = bar.command_state_projection();
        for current in Appearance::ALL {
            project_checks(&mut projection, current);
            for choice in Appearance::ALL {
                let state: &CommandState = projection.get(choice.command_id()).expect("state");
                assert_eq!(
                    state.checked,
                    Some(choice == current),
                    "{choice:?} under {current:?}"
                );
                assert_eq!(state.label, choice.label());
            }
        }
    }

    #[test]
    fn the_configuration_directory_can_be_redirected_for_isolation() {
        // `settings_directory` reads the environment, so only the pure shape is
        // checked here: the application id is always the last path component.
        let store = AppearanceStore::for_application("org.loom.example");
        let path = store.path();
        assert!(
            path.ends_with("org.loom.example/settings.toml")
                || path.ends_with("org.loom.example\\settings.toml")
        );
    }
}
