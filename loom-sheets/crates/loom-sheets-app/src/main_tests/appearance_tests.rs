//! The in-app Appearance setting: System, Light, Dark and High Contrast chosen
//! from the View menu, the toolbar, the command palette or the keyboard,
//! applied to the whole window, remembered per user, and overridden by
//! `--theme`. Pixels, theme tokens, menu rows and the settings file are read
//! back, not assumed.

use super::keyboard_flow_tests::{launched as launched_session, Session};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_desktop::{Appearance, AppearanceStore};
use slint::platform::{Key, WindowEvent};
use std::sync::atomic::{AtomicU32, Ordering};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

fn temp_directory(tag: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "loom-sheets-appearance-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

struct Harness {
    session: Session,
    directory: PathBuf,
}

impl Harness {
    fn app(&self) -> &SheetsApp {
        &self.session.app
    }

    fn store(&self) -> AppearanceStore {
        AppearanceStore::at(self.directory.join("settings.toml"))
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// The window as `run_gui` builds it, with the settings file in a private
/// directory so a test never reads or writes the user's real setting.
fn launched(tag: &str, flag: Option<&str>) -> Harness {
    launched_in(temp_directory(tag), flag)
}

fn launched_in(directory: PathBuf, flag: Option<&str>) -> Harness {
    let session = launched_session(&[("A1", "Item"), ("B1", "Cost"), ("A2", "Tea"), ("B2", "3")]);
    let app = &session.app;
    // Activating a row runs the same dispatcher the menu sink schedules.
    let weak = app.as_weak();
    app.on_local_menu_action(move |id| {
        if let Some(app) = weak.upgrade() {
            crate::command_dispatch::dispatch_command(&app, id.as_str());
        }
    });
    {
        let menu = session.menu.clone();
        let state = session.state.clone();
        appearance::on_change(move |app| sync_menu_state(&menu, app, &state));
    }
    appearance::start_with_store(
        app,
        AppearanceStore::at(directory.join("settings.toml")),
        flag,
    );
    local_menu::sync(app, &session.menu).expect("project menu");
    let harness = Harness { session, directory };
    settle(harness.app());
    harness
}

/// Lets Slint run its pending property-change handlers, then renders once.
fn settle(app: &SheetsApp) {
    slint::platform::update_timers_and_animations();
    let _ = snapshot_component(app, WIDTH, HEIGHT, 1.0).expect("render");
}

struct Shot {
    width: u32,
    pixels: Vec<[u8; 4]>,
}

impl Shot {
    fn take(app: &SheetsApp) -> Shot {
        slint::platform::update_timers_and_animations();
        let image = snapshot_component(app, WIDTH, HEIGHT, 1.0).expect("render");
        Shot {
            width: image.width(),
            pixels: image.pixels().map(|pixel| pixel.0).collect(),
        }
    }

    /// Fraction of pixels in the rectangle that differ from `other`.
    fn difference(&self, other: &Shot, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
        let mut different = 0usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y * self.width + x) as usize;
                if self.pixels[index] != other.pixels[index] {
                    different += 1;
                }
            }
        }
        different as f64 / f64::from((x1 - x0) * (y1 - y0))
    }
}

fn theme_name(app: &SheetsApp) -> String {
    slint::platform::update_timers_and_animations();
    Theme::get(app).get_active_theme().to_string()
}

fn key(app: &SheetsApp, text: impl Into<SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    settle(app);
}

fn chord_alt(app: &SheetsApp, letter: &str) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Alt.into(),
    });
    key(app, letter);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Alt.into(),
    });
}

fn toolbar_button(app: &SheetsApp, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(app, label)
        .find(|e| {
            e.accessible_role() == Some(AccessibleRole::Button)
                && e.size().height >= 44.0
                && e.absolute_position().y < 100.0
        })
        .unwrap_or_else(|| panic!("toolbar button named {label:?}"))
}

fn checked_rows(app: &SheetsApp) -> Vec<String> {
    app.get_local_menu_items()
        .iter()
        .filter(|row| row.checked && row.command_id.starts_with("view.appearance."))
        .map(|row| row.command_id.to_string())
        .collect()
}

#[test]
fn a_fresh_install_follows_the_system_and_falls_back_to_light() {
    let h = launched("fresh", None);
    assert_eq!(appearance::current(h.app()), Appearance::System);
    assert_eq!(
        theme_name(h.app()),
        "light",
        "a platform that reports no preference is light"
    );
    assert_eq!(checked_rows(h.app()), ["view.appearance.system"]);
}

#[test]
fn choosing_an_appearance_recolors_the_window_exactly_as_the_theme_flag_does() {
    let h = launched("pixels", Some("light"));
    let light = Shot::take(h.app());
    appearance::choose(h.app(), Appearance::Dark);
    assert_eq!(theme_name(h.app()), "dark");
    assert_eq!(
        Theme::get(h.app()).get_tokens().palette.canvas,
        slint::Color::from_argb_encoded(0xff1c1c1e)
    );
    let chosen = Shot::take(h.app());
    // The title row, toolbar and inspector are recoloured...
    assert!(light.difference(&chosen, 0, 0, 1280, 88) > 0.5);
    assert!(light.difference(&chosen, 1000, 120, 1270, 500) > 0.5);

    // ...and the result is the very picture `--theme dark` has always drawn.
    apply_theme(h.app(), "dark");
    assert_eq!(
        chosen.difference(&Shot::take(h.app()), 0, 0, 1280, 800),
        0.0,
        "menu choice and --theme render identically"
    );

    appearance::choose(h.app(), Appearance::HighContrast);
    assert_eq!(theme_name(h.app()), "high-contrast");
    assert!(chosen.difference(&Shot::take(h.app()), 0, 0, 1280, 88) > 0.3);
}

#[test]
fn the_choice_is_remembered_by_the_next_app_instance() {
    let first = launched("persist", None);
    appearance::choose(first.app(), Appearance::Dark);
    assert_eq!(first.store().load(), Appearance::Dark);
    let directory = first.directory.clone();

    // A new window in a new session reads the same settings directory.
    let second = launched_in(directory, None);
    assert_eq!(appearance::current(second.app()), Appearance::Dark);
    assert_eq!(theme_name(second.app()), "dark");
    assert_eq!(checked_rows(second.app()), ["view.appearance.dark"]);
    std::mem::forget(first); // `second` removes the shared directory
}

#[test]
fn a_corrupt_settings_file_falls_back_to_system() {
    let directory = temp_directory("corrupt");
    std::fs::create_dir_all(&directory).expect("create directory");
    std::fs::write(
        directory.join("settings.toml"),
        b"\xff\xfe appearance = ???\x00",
    )
    .expect("write corrupt settings");
    let h = launched_in(directory, None);
    assert_eq!(appearance::current(h.app()), Appearance::System);
    assert_eq!(theme_name(h.app()), "light");
    // The next choice repairs the file.
    appearance::choose(h.app(), Appearance::Dark);
    assert_eq!(h.store().load(), Appearance::Dark);
}

#[test]
fn the_theme_flag_overrides_the_saved_choice_and_does_not_overwrite_it() {
    let directory = temp_directory("flag");
    std::fs::create_dir_all(&directory).expect("create directory");
    AppearanceStore::at(directory.join("settings.toml"))
        .save(Appearance::Dark)
        .expect("save");
    let h = launched_in(directory, Some("high-contrast"));
    assert_eq!(appearance::current(h.app()), Appearance::HighContrast);
    assert_eq!(theme_name(h.app()), "high-contrast");
    assert_eq!(checked_rows(h.app()), ["view.appearance.high_contrast"]);
    assert_eq!(
        h.store().load(),
        Appearance::Dark,
        "the file was not touched"
    );

    // The flag is parsed as explicit only when it is given.
    assert!(
        cli::parse_args_from(["--theme", "dark"])
            .expect("args")
            .theme_explicit
    );
    assert!(
        !cli::parse_args_from(["--smoke"])
            .expect("args")
            .theme_explicit
    );
    assert!(cli::parse_args_from(["--theme", "system"]).is_ok());
    assert!(cli::parse_args_from(["--theme", "sepia"]).is_err());
}

#[test]
fn menu_check_marks_follow_the_setting_and_state_is_announced_to_assistive_technology() {
    let h = launched("menu", Some("light"));
    for choice in Appearance::ALL {
        appearance::choose(h.app(), choice);
        let projection = menu_projection(&h.session.menu, h.app()).expect("project");
        for other in Appearance::ALL {
            assert_eq!(
                projection.get(other.command_id()).expect("state").checked,
                Some(other == choice),
                "{other:?} while {choice:?} is chosen"
            );
        }
        assert_eq!(
            checked_rows(h.app()),
            [choice.command_id()],
            "in-window rows"
        );
        let installed = h.session.menu.installed_menu_bar().expect("installed");
        assert!(
            matches!(
                installed.find_item(choice.command_id()),
                Some(loom_desktop::MenuItem::Radio { selected: true, .. })
            ),
            "the native menu's radio is selected for {choice:?}"
        );
    }

    // The open View menu names each row and says whether it is the chosen one.
    appearance::choose(h.app(), Appearance::Dark);
    chord_alt(h.app(), "v");
    settle(h.app());
    let row = |label: &str| {
        ElementHandle::find_by_accessible_label(h.app(), label)
            .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .unwrap_or_else(|| panic!("row {label:?}"))
    };
    let dark = row("Appearance: Dark");
    assert_eq!(dark.accessible_checkable(), Some(true));
    assert_eq!(dark.accessible_checked(), Some(true));
    let light = row("Appearance: Light");
    assert_eq!(light.accessible_checkable(), Some(true));
    assert_eq!(light.accessible_checked(), Some(false));
    key(h.app(), Key::Escape);

    // The change is announced through the status line, a polite live region.
    assert_eq!(h.app().get_status_left(), "Appearance: Dark");
    appearance::choose(h.app(), Appearance::System);
    assert_eq!(
        h.app().get_status_left(),
        "Appearance: System (following the system: light)"
    );
}

#[test]
fn the_view_menu_works_from_the_keyboard() {
    let h = launched("keyboard", Some("light"));
    chord_alt(h.app(), "v");
    let labels: Vec<String> = h
        .app()
        .get_local_menu_labels()
        .iter()
        .map(|label| label.to_string())
        .collect();
    assert_eq!(labels[h.app().get_local_menu_open_index() as usize], "View");
    let selected = |app: &SheetsApp| {
        app.get_local_menu_popup_items()
            .row_data(app.get_local_menu_popup_selected_index() as usize)
            .map(|row| row.label.to_string())
            .unwrap_or_default()
    };
    for _ in 0..30 {
        if selected(h.app()) == "Appearance: Dark" {
            break;
        }
        key(h.app(), Key::DownArrow);
    }
    assert_eq!(
        selected(h.app()),
        "Appearance: Dark",
        "Down reaches the row"
    );
    key(h.app(), Key::Return);
    assert_eq!(
        h.app().get_local_menu_open_index(),
        -1,
        "Enter closes the menu"
    );
    assert_eq!(appearance::current(h.app()), Appearance::Dark);
    assert_eq!(theme_name(h.app()), "dark");
    assert_eq!(h.store().load(), Appearance::Dark);
}

#[test]
fn the_toolbar_view_menu_offers_the_same_choices() {
    let h = launched("toolbar", Some("light"));
    settle(h.app());
    toolbar_button(h.app(), "View").invoke_accessible_default_action();
    settle(h.app());
    assert_eq!(h.app().get_toolbar_menu(), 1);
    let row = ElementHandle::find_by_accessible_label(h.app(), "Appearance: High Contrast")
        .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
        .expect("High Contrast row");
    row.invoke_accessible_default_action();
    settle(h.app());
    assert_eq!(appearance::current(h.app()), Appearance::HighContrast);
    assert_eq!(theme_name(h.app()), "high-contrast");
    assert_eq!(h.store().load(), Appearance::HighContrast);
}

#[test]
fn the_command_palette_lists_and_runs_every_appearance() {
    let h = launched("palette", Some("light"));
    h.app().set_palette_open(true);
    h.app().set_palette_query("appearance".into());
    h.app().invoke_palette_query_changed("appearance".into());
    let labels: Vec<String> = h
        .app()
        .get_palette_commands()
        .iter()
        .map(|item| item.label.to_string())
        .collect();
    for choice in Appearance::ALL {
        assert!(
            labels.iter().any(|label| label == choice.label()),
            "{labels:?}"
        );
    }

    h.app().set_palette_query("Appearance: Dark".into());
    h.app()
        .invoke_palette_query_changed("Appearance: Dark".into());
    let first = h.app().get_palette_commands().row_data(0).expect("a match");
    assert_eq!(first.label, "Appearance: Dark");
    h.app().invoke_palette_invoked(0);
    assert_eq!(appearance::current(h.app()), Appearance::Dark);
    assert_eq!(theme_name(h.app()), "dark");
    assert_eq!(h.store().load(), Appearance::Dark);
}

#[test]
fn system_follows_the_operating_system_while_it_is_chosen() {
    let h = launched("system", None);
    let theme = Theme::get(h.app());
    theme.set_system_scheme("dark".into());
    assert_eq!(theme_name(h.app()), "dark", "System follows a dark OS");
    theme.set_system_scheme("light".into());
    assert_eq!(theme_name(h.app()), "light");

    appearance::choose(h.app(), Appearance::Light);
    theme.set_system_scheme("dark".into());
    assert_eq!(
        theme_name(h.app()),
        "light",
        "an explicit choice ignores the OS"
    );
}

#[test]
fn a_settings_directory_that_cannot_be_written_keeps_the_switch_and_says_so() {
    let directory = temp_directory("readonly");
    std::fs::create_dir_all(&directory).expect("create directory");
    // A file where the settings directory should be makes every save fail.
    let blocker = directory.join("blocked");
    std::fs::write(&blocker, b"not a directory").expect("write blocker");
    let h = launched_in(directory, None);
    appearance::install_store(AppearanceStore::at(blocker.join("settings.toml")));
    appearance::choose(h.app(), Appearance::Dark);
    assert_eq!(theme_name(h.app()), "dark", "the window still switches");
    assert!(
        h.app()
            .get_status_left()
            .contains("could not be remembered"),
        "{}",
        h.app().get_status_left()
    );
}
