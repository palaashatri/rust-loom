use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::settings_file;
use slint::{ComponentHandle, SharedString};

use crate::{PresentApp, WelcomeGuideState};

const WELCOME_SEEN_KEY: &str = "welcome_guide_seen";

fn settings_path() -> PathBuf {
    settings_file::path_for_application(crate::appearance::APPLICATION_ID)
}

fn should_show_at(path: &Path) -> bool {
    settings_file::read(path, WELCOME_SEEN_KEY).as_deref() != Some("true")
}

fn mark_seen_at(path: &Path) -> io::Result<()> {
    settings_file::write(path, WELCOME_SEEN_KEY, "true")
}

fn mark_seen() {
    if cfg!(test) {
        return;
    }
    let _ = mark_seen_at(&settings_path());
}

/// Platform modifier key prefix with trailing `+` preserved (`"Cmd+"` or `"Ctrl+"`).
pub(crate) fn platform_mod_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+"
    } else {
        "Ctrl+"
    }
}

/// Returns true until the user dismisses or acts on the first-run welcome guide.
pub(crate) fn should_show_on_first_run() -> bool {
    should_show_at(&settings_path())
}

/// Opens the 7-step Loom Present Welcome Guide at Step 1 and syncs platform shortcut labels.
pub(crate) fn open(app: &PresentApp) {
    let guide = app.global::<WelcomeGuideState>();
    guide.set_mod_key(SharedString::from(platform_mod_key()));
    guide.set_step(0);
    guide.set_open(true);
    let toggled_focus = !guide.get_focus_request();
    guide.set_focus_request(toggled_focus);
    app.set_status_left("Opened Welcome Guide".into());
}

fn close_without_refocus(app: &PresentApp) {
    let guide = app.global::<WelcomeGuideState>();
    guide.set_open(false);
    mark_seen();
}

pub(crate) type TryActionCb = Rc<dyn Fn(&PresentApp, &str)>;

/// Connects Welcome Guide close and interactive Try callbacks to Present application handlers.
pub(crate) fn wire(app: &PresentApp, on_try_action: TryActionCb) {
    let weak = app.as_weak();
    app.global::<WelcomeGuideState>().on_close_guide(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        close_without_refocus(&app);
        app.set_status_left("Closed Welcome Guide".into());
    });

    let weak = app.as_weak();
    app.global::<WelcomeGuideState>()
        .on_try_action(move |action| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            close_without_refocus(&app);
            on_try_action(&app, action.as_str());
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_guide_seen_persists_via_settings_file() {
        let dir =
            std::env::temp_dir().join(format!("loom-welcome-guide-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.conf");
        assert!(should_show_at(&path));
        mark_seen_at(&path).expect("mark_seen_at");
        assert!(!should_show_at(&path));
    }

    #[test]
    fn mod_key_preserves_trailing_plus() {
        let key = platform_mod_key();
        assert!(key == "Cmd+" || key == "Ctrl+");
    }
}
