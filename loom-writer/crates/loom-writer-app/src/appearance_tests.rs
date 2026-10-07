//! The in-app Appearance setting: System, Light, Dark and High Contrast chosen
//! from the View menu, the toolbar, the command palette or the keyboard,
//! applied to the whole window, remembered per user, and overridden by
//! `--theme`. Pixels, theme tokens, menu rows and the settings file are read
//! back, not assumed.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_desktop::{Appearance, AppearanceStore, MenuBarService};
use slint::platform::{Key, WindowEvent};
use std::sync::atomic::{AtomicU32, Ordering};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

fn temp_directory(tag: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "loom-writer-appearance-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

struct Harness {
    app: WriterApp,
    menu: Arc<NativeMenuBar>,
    directory: PathBuf,
}

impl Harness {
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
    let (app, state) = test_state(
        text_document("Hello world"),
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], vec![])),
    );
    window_chrome::install(&app);
    let menu = Arc::new(NativeMenuBar::new());
    wire_writer_shared_callbacks(&app, &state, Some(menu.clone()));
    wire_writer_inspector_toggle(&app, &state, Some(menu.clone()));
    menu.install_menu_bar(&local_menu::writer_menu_bar())
        .expect("install menu bar");
    local_menu::wire_keyboard(&app);
    // Activating a row runs the same dispatcher the menu sink schedules.
    let weak = app.as_weak();
    app.on_local_menu_action(move |id| {
        if let Some(app) = weak.upgrade() {
            dispatch_command(&app, id.as_str());
        }
    });
    wire_palette(&app);
    palette_wiring::wire(&app, &state);
    appearance::on_change(|app| app.invoke_view_state_changed());
    appearance::start_with_store(
        &app,
        AppearanceStore::at(directory.join("settings.toml")),
        flag,
    );
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app, 1280);
    apply_state(&app, &state);
    local_menu::sync(&app, &menu).expect("project menu");
    let harness = Harness {
        app,
        menu,
        directory,
    };
    settle(&harness.app);
    harness
}

/// Lets Slint run its pending property-change handlers, then renders once.
fn settle(app: &WriterApp) {
    slint::platform::update_timers_and_animations();
    let _ = snapshot_component(app, WIDTH, HEIGHT, 1.0).expect("render");
}

struct Shot {
    width: u32,
    pixels: Vec<[u8; 4]>,
}

impl Shot {
    fn take(app: &WriterApp) -> Shot {
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

fn theme_name(app: &WriterApp) -> String {
    slint::platform::update_timers_and_animations();
    Theme::get(app).get_active_theme().to_string()
}

fn key(app: &WriterApp, text: impl Into<SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    settle(app);
}

fn chord_alt(app: &WriterApp, letter: &str) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Alt.into(),
    });
    key(app, letter);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Alt.into(),
    });
}

fn press_named(app: &WriterApp, role: AccessibleRole, label: &str) {
    settle(app);
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| element.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} named {label:?}"))
        .invoke_accessible_default_action();
    settle(app);
}

fn checked_rows(app: &WriterApp) -> Vec<String> {
    app.get_local_menu_items()
        .iter()
        .filter(|row| row.checked && row.command_id.starts_with("view.appearance."))
        .map(|row| row.command_id.to_string())
        .collect()
}

#[test]
fn a_fresh_install_follows_the_system_and_falls_back_to_light() {
    let h = launched("fresh", None);
    assert_eq!(appearance::current(&h.app), Appearance::System);
    assert_eq!(
        theme_name(&h.app),
        "light",
        "a platform that reports no preference is light"
    );
    assert_eq!(checked_rows(&h.app), ["view.appearance.system"]);
}

#[test]
fn choosing_an_appearance_changes_the_chrome_pixels_but_not_the_document() {
    let h = launched("pixels", Some("light"));
    let light = Shot::take(&h.app);
    appearance::choose(&h.app, Appearance::Dark);
    assert_eq!(theme_name(&h.app), "dark");
    assert_eq!(
        Theme::get(&h.app).get_tokens().palette.canvas,
        slint::Color::from_argb_encoded(0xff1c1c1e)
    );
    let dark = Shot::take(&h.app);

    // The toolbar and menu rows at the top are recoloured...
    assert!(
        light.difference(&dark, 0, 0, 1280, 88) > 0.5,
        "the title and toolbar rows changed"
    );
    // ...and so is the inspector beside the page...
    assert!(light.difference(&dark, 970, 120, 1270, 500) > 0.5);
    // ...but the page and its text are the document, drawn the same.
    assert_eq!(
        light.difference(&dark, 190, 120, 770, 400),
        0.0,
        "the page artwork must not follow the UI theme"
    );

    appearance::choose(&h.app, Appearance::HighContrast);
    assert_eq!(theme_name(&h.app), "high-contrast");
    let contrast = Shot::take(&h.app);
    assert!(dark.difference(&contrast, 0, 0, 1280, 88) > 0.3);
    // High contrast keeps the paper white; only its ink is darker, which is
    // the design contract's own `paper-ink` token for that theme.
    assert_eq!(dark.difference(&contrast, 190, 420, 770, 760), 0.0);
}

#[test]
fn the_choice_is_remembered_by_the_next_app_instance() {
    let first = launched("persist", None);
    appearance::choose(&first.app, Appearance::Dark);
    assert_eq!(first.store().load(), Appearance::Dark);
    let directory = first.directory.clone();

    // A new window in a new session reads the same settings directory.
    let second = launched_in(directory.clone(), None);
    assert_eq!(appearance::current(&second.app), Appearance::Dark);
    assert_eq!(theme_name(&second.app), "dark");
    assert_eq!(checked_rows(&second.app), ["view.appearance.dark"]);
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
    assert_eq!(appearance::current(&h.app), Appearance::System);
    assert_eq!(theme_name(&h.app), "light");
    // The next choice repairs the file.
    appearance::choose(&h.app, Appearance::Dark);
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
    assert_eq!(appearance::current(&h.app), Appearance::HighContrast);
    assert_eq!(theme_name(&h.app), "high-contrast");
    assert_eq!(checked_rows(&h.app), ["view.appearance.high_contrast"]);
    assert_eq!(
        h.store().load(),
        Appearance::Dark,
        "the file was not touched"
    );

    // The flag is parsed as explicit only when it is given.
    assert!(
        parse_args_from(["--theme", "dark"])
            .expect("args")
            .theme_explicit
    );
    assert!(!parse_args_from(["--smoke"]).expect("args").theme_explicit);
    assert!(parse_args_from(["--theme", "system"]).is_ok());
    assert!(parse_args_from(["--theme", "sepia"]).is_err());
}

#[test]
fn menu_check_marks_follow_the_setting_and_state_is_announced_to_assistive_technology() {
    let h = launched("menu", Some("light"));
    for choice in Appearance::ALL {
        appearance::choose(&h.app, choice);
        let projection = menu_projection(&h.menu, &h.app).expect("project");
        for other in Appearance::ALL {
            assert_eq!(
                projection.get(other.command_id()).expect("state").checked,
                Some(other == choice),
                "{other:?} while {choice:?} is chosen"
            );
        }
        assert_eq!(
            checked_rows(&h.app),
            [choice.command_id()],
            "in-window rows"
        );
        let installed = h.menu.installed_menu_bar().expect("installed");
        assert!(
            matches!(
                installed.find_item(choice.command_id()),
                Some(loom_desktop::MenuItem::Radio { selected: true, .. })
            ),
            "the native menu's radio is selected for {choice:?}"
        );
    }

    // The open View menu names each row and says whether it is the chosen one.
    appearance::choose(&h.app, Appearance::Dark);
    chord_alt(&h.app, "v");
    settle(&h.app);
    let names = |checked: bool| {
        ElementHandle::find_by_accessible_label(&h.app, "Appearance: Dark")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .any(|e| {
                e.accessible_checkable() == Some(true) && e.accessible_checked() == Some(checked)
            })
    };
    assert!(
        names(true),
        "the chosen row is a checked, checkable list item"
    );
    let light = ElementHandle::find_by_accessible_label(&h.app, "Appearance: Light")
        .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
        .expect("Light row");
    assert_eq!(light.accessible_checkable(), Some(true));
    assert_eq!(light.accessible_checked(), Some(false));
    key(&h.app, Key::Escape);

    // The change is announced through the status line, a polite live region.
    assert_eq!(h.app.get_status_left(), "Appearance: Dark");
    appearance::choose(&h.app, Appearance::System);
    assert_eq!(
        h.app.get_status_left(),
        "Appearance: System (following the system: light)"
    );
}

#[test]
fn the_view_menu_works_from_the_keyboard() {
    let h = launched("keyboard", Some("light"));
    chord_alt(&h.app, "v");
    let labels: Vec<String> = h
        .app
        .get_local_menu_labels()
        .iter()
        .map(|label| label.to_string())
        .collect();
    assert_eq!(labels[h.app.get_local_menu_open_index() as usize], "View");
    let selected = |app: &WriterApp| {
        app.get_local_menu_popup_items()
            .row_data(app.get_local_menu_popup_selected_index() as usize)
            .map(|row| row.label.to_string())
            .unwrap_or_default()
    };
    for _ in 0..30 {
        if selected(&h.app) == "Appearance: Dark" {
            break;
        }
        key(&h.app, Key::DownArrow);
    }
    assert_eq!(selected(&h.app), "Appearance: Dark", "Down reaches the row");
    key(&h.app, Key::Return);
    assert_eq!(
        h.app.get_local_menu_open_index(),
        -1,
        "Enter closes the menu"
    );
    assert_eq!(appearance::current(&h.app), Appearance::Dark);
    assert_eq!(theme_name(&h.app), "dark");
    assert_eq!(h.store().load(), Appearance::Dark);
}

#[test]
fn the_toolbar_view_menu_offers_the_same_choices() {
    let h = launched("toolbar", Some("light"));
    press_named(&h.app, AccessibleRole::Button, "View");
    press_named(
        &h.app,
        AccessibleRole::ListItem,
        "Appearance: High Contrast",
    );
    assert_eq!(appearance::current(&h.app), Appearance::HighContrast);
    assert_eq!(theme_name(&h.app), "high-contrast");
    assert_eq!(h.store().load(), Appearance::HighContrast);
}

#[test]
fn the_command_palette_lists_and_runs_every_appearance() {
    let h = launched("palette", Some("light"));
    h.app.set_palette_open(true);
    h.app.invoke_palette_query_changed("appearance".into());
    let labels: Vec<String> = h
        .app
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

    h.app.set_palette_query("Appearance: Dark".into());
    h.app
        .invoke_palette_query_changed("Appearance: Dark".into());
    let first = h.app.get_palette_commands().row_data(0).expect("a match");
    assert_eq!(first.label, "Appearance: Dark");
    h.app.invoke_palette_invoked(0);
    assert!(!h.app.get_palette_open(), "the palette closes");
    assert_eq!(appearance::current(&h.app), Appearance::Dark);
    assert_eq!(theme_name(&h.app), "dark");
    assert_eq!(h.store().load(), Appearance::Dark);
}

#[test]
fn system_follows_the_operating_system_while_it_is_chosen() {
    let h = launched("system", None);
    let theme = Theme::get(&h.app);
    theme.set_system_scheme("dark".into());
    assert_eq!(theme_name(&h.app), "dark", "System follows a dark OS");
    theme.set_system_scheme("light".into());
    assert_eq!(theme_name(&h.app), "light");

    appearance::choose(&h.app, Appearance::Light);
    theme.set_system_scheme("dark".into());
    assert_eq!(
        theme_name(&h.app),
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
    let h = launched_in(directory.clone(), None);
    appearance::install_store(AppearanceStore::at(blocker.join("settings.toml")));
    appearance::choose(&h.app, Appearance::Dark);
    assert_eq!(theme_name(&h.app), "dark", "the window still switches");
    assert!(
        h.app.get_status_left().contains("could not be remembered"),
        "{}",
        h.app.get_status_left()
    );
}
/// Puts the window into one surface to compare.
type Enter = Box<dyn Fn(&WriterApp)>;

/// Slint's own widget palette follows the operating system's light or dark
/// preference. Writer draws every surface from `Theme`, so a dark operating
/// system must not change a single pixel of the window; this is what lets the
/// application follow the system without pinning that palette to light.
#[test]
fn the_window_does_not_depend_on_the_native_widget_palette() {
    use slint::private_unstable_api::re_exports::ColorScheme;
    let h = launched("palette-independence", Some("light"));
    let states: [(&str, Enter); 6] = [
        ("document", Box::new(|_| {})),
        ("palette", Box::new(|app| app.set_palette_open(true))),
        (
            "template chooser",
            Box::new(|app| app.set_template_chooser_open(true)),
        ),
        (
            "save changes",
            Box::new(|app| app.set_save_changes_open(true)),
        ),
        ("view menu", Box::new(|app| chord_alt(app, "v"))),
        (
            "toolbar menu",
            Box::new(|app| press_named(app, AccessibleRole::Button, "View")),
        ),
    ];
    for theme in ["light", "dark", "high-contrast"] {
        appearance::apply_id(&h.app, theme);
        for (name, enter) in &states {
            enter(&h.app);
            WidgetPalette::get(&h.app).set_color_scheme(ColorScheme::Light);
            let light = Shot::take(&h.app);
            WidgetPalette::get(&h.app).set_color_scheme(ColorScheme::Dark);
            let dark = Shot::take(&h.app);
            assert_eq!(
                light.difference(&dark, 0, 0, 1280, 800),
                0.0,
                "{name} under the {theme} theme depends on the widget palette"
            );
            h.app.set_palette_open(false);
            h.app.set_template_chooser_open(false);
            h.app.set_save_changes_open(false);
            key(&h.app, Key::Escape);
            key(&h.app, Key::Escape);
        }
    }
}
