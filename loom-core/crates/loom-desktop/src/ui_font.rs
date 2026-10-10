//! The face that window chrome is drawn in, chosen once per run.
//!
//! Menus, toolbars, inspectors, dialogs, status bars and tabs use the platform's
//! own interface face, so Loom reads as a native application on each desktop.
//! Document content (page text, cell text, slide text) keeps the bundled Inter
//! on every platform, so layout, pagination and exported artwork do not depend
//! on which fonts the user's machine happens to install.
//!
//! Slint takes one family name per text element and has no fallback list, so
//! the family for the running platform is chosen here from the candidates the
//! platform reports. The Slint side is the `ui-font-family` property of the
//! shared `Theme` global; [`ui_font_bindings!`](crate::ui_font_bindings) applies
//! the choice to one application window.
//!
//! `LOOM_UI_FONT` overrides the choice: `system` selects the platform face, and
//! any other non-empty value is used as a family name. Headless captures
//! (`--screenshot`, `--smoke`, journeys) use the bundled face unless the variable
//! is set, so their pixels do not depend on the machine that renders them.

use std::path::{Path, PathBuf};

/// The bundled interface face, used by captures and whenever nothing else is installed.
pub const BUNDLED_FAMILY: &str = "Inter";

/// Environment variable that overrides the chrome face.
pub const ENV_VAR: &str = "LOOM_UI_FONT";

/// The operating systems that have a platform interface face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Microsoft Windows.
    Windows,
    /// Apple macOS.
    MacOs,
    /// Linux desktops that use fontconfig.
    Linux,
    /// Anything else: the bundled face is always used.
    Other,
}

impl Platform {
    /// The platform this build runs on.
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

/// The families to try for `platform`, most preferred first.
pub const fn candidates(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Windows => &["Segoe UI Variable Text", "Segoe UI"],
        Platform::MacOs => &[".AppleSystemUIFont", "SF Pro Text", "Helvetica Neue"],
        Platform::Linux => &["Cantarell", "Noto Sans", "Ubuntu", "DejaVu Sans"],
        Platform::Other => &[],
    }
}

/// The first of `families` that `is_installed` accepts, or the bundled face when
/// none is. The bundled face is always available, so the result is always usable.
pub fn first_installed(families: &[&str], is_installed: impl Fn(&str) -> bool) -> String {
    families
        .iter()
        .copied()
        .find(|family| is_installed(family))
        .unwrap_or(BUNDLED_FAMILY)
        .to_string()
}

/// The chrome face for one run.
///
/// `setting` is the value of `LOOM_UI_FONT`, when it is set. A blank setting
/// counts as unset. Unset means the bundled face for a headless capture and the
/// platform face for a window.
pub fn resolve(
    setting: Option<&str>,
    headless: bool,
    platform: Platform,
    is_installed: impl Fn(&str) -> bool,
) -> String {
    match setting.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.eq_ignore_ascii_case("system") => {
            first_installed(candidates(platform), is_installed)
        }
        Some(family) => family.to_string(),
        None if headless => BUNDLED_FAMILY.to_string(),
        None => first_installed(candidates(platform), is_installed),
    }
}

/// The chrome face for this process: reads `LOOM_UI_FONT` and checks the real
/// machine for the platform candidates.
pub fn for_this_run(headless: bool) -> String {
    let setting = std::env::var(ENV_VAR).ok();
    let platform = Platform::current();
    resolve(setting.as_deref(), headless, platform, |family| {
        installed_on(platform, family)
    })
}

/// Whether `platform` can show `family` as an interface face.
///
/// macOS cannot be asked which families exist from here, so its first candidate
/// (the system interface face) is always accepted. That path is unverified on a
/// real Mac.
pub fn installed_on(platform: Platform, family: &str) -> bool {
    match platform {
        Platform::Windows => windows_font_installed(&windows_fonts_dir(), family),
        Platform::Linux => linux_font_installed(family),
        Platform::MacOs => true,
        Platform::Other => false,
    }
}

/// The Windows font file that carries each candidate family.
fn windows_font_file(family: &str) -> Option<&'static str> {
    match family {
        "Segoe UI Variable Text" => Some("SegUIVar.ttf"),
        "Segoe UI" => Some("segoeui.ttf"),
        _ => None,
    }
}

/// Whether the font file for `family` exists in `fonts_dir`.
pub fn windows_font_installed(fonts_dir: &Path, family: &str) -> bool {
    windows_font_file(family).is_some_and(|file| fonts_dir.join(file).is_file())
}

fn windows_fonts_dir() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

/// Whether fontconfig's best match for `family` is `family` itself.
///
/// fontconfig substitutes a default face for a missing family, so the first
/// family name it reports must be the one that was asked for.
pub fn fontconfig_reports(output: &str, family: &str) -> bool {
    output
        .lines()
        .next()
        .and_then(|line| line.split(',').next())
        .is_some_and(|first| first.trim().eq_ignore_ascii_case(family))
}

fn linux_font_installed(family: &str) -> bool {
    std::process::Command::new("fc-match")
        .args(["-f", "%{family}\n", family])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && fontconfig_reports(&String::from_utf8_lossy(&output.stdout), family)
        })
}

/// Expands `apply_ui_font(app, headless)` for one application window.
///
/// Invoke inside an application module, naming the window type and the shared
/// `Theme` global the window imports:
/// `loom_desktop::ui_font_bindings!(SheetsApp, crate::Theme);`
#[macro_export]
macro_rules! ui_font_bindings {
    ($app:ty, $theme:ty) => {
        /// Sets the window's chrome face for this run. Document content is not affected.
        pub(crate) fn apply_ui_font(app: &$app, headless: bool) {
            use ::slint::ComponentHandle as _;
            app.global::<$theme>()
                .set_ui_font_family($crate::ui_font::for_this_run(headless).into());
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(list: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |family| list.iter().copied().any(|item| item == family)
    }

    #[test]
    fn each_platform_offers_its_own_interface_face_first() {
        assert_eq!(
            candidates(Platform::Windows),
            ["Segoe UI Variable Text", "Segoe UI"]
        );
        assert_eq!(
            candidates(Platform::MacOs),
            [".AppleSystemUIFont", "SF Pro Text", "Helvetica Neue"]
        );
        assert_eq!(
            candidates(Platform::Linux),
            ["Cantarell", "Noto Sans", "Ubuntu", "DejaVu Sans"]
        );
        assert!(candidates(Platform::Other).is_empty());
    }

    #[test]
    fn the_first_installed_candidate_in_preference_order_wins() {
        let chosen = first_installed(
            candidates(Platform::Linux),
            installed(&["Ubuntu", "DejaVu Sans", "Noto Sans"]),
        );
        assert_eq!(chosen, "Noto Sans");
    }

    #[test]
    fn with_nothing_installed_the_bundled_face_is_used() {
        assert_eq!(
            first_installed(candidates(Platform::Linux), installed(&[])),
            BUNDLED_FAMILY
        );
        assert_eq!(first_installed(&[], |_| true), BUNDLED_FAMILY);
    }

    #[test]
    fn an_interactive_run_picks_the_face_of_the_build_platform() {
        #[cfg(target_os = "windows")]
        assert_eq!(
            resolve(None, false, Platform::current(), |_| true),
            "Segoe UI Variable Text"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            resolve(None, false, Platform::current(), |family| family
                != "Cantarell"),
            "Noto Sans"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            resolve(None, false, Platform::current(), |_| true),
            ".AppleSystemUIFont"
        );
    }

    #[test]
    fn headless_captures_keep_the_bundled_face_unless_system_is_requested() {
        assert_eq!(
            resolve(None, true, Platform::Linux, |_| true),
            BUNDLED_FAMILY
        );
        assert_eq!(
            resolve(
                Some("system"),
                true,
                Platform::Linux,
                installed(&["Ubuntu"])
            ),
            "Ubuntu"
        );
        assert_eq!(
            resolve(Some("System"), true, Platform::Windows, |_| true),
            "Segoe UI Variable Text"
        );
    }

    #[test]
    fn an_explicit_family_name_overrides_the_platform_face() {
        assert_eq!(
            resolve(Some("DejaVu Sans"), false, Platform::Windows, |_| true),
            "DejaVu Sans"
        );
        assert_eq!(
            resolve(Some("Inter"), false, Platform::Linux, |_| true),
            BUNDLED_FAMILY
        );
    }

    #[test]
    fn a_blank_setting_counts_as_unset() {
        assert_eq!(
            resolve(Some("   "), true, Platform::Linux, |_| true),
            BUNDLED_FAMILY
        );
        assert_eq!(
            resolve(Some(""), false, Platform::Linux, installed(&["Ubuntu"])),
            "Ubuntu"
        );
    }

    #[test]
    fn fontconfig_must_report_the_requested_family_first() {
        assert!(!fontconfig_reports("Noto Sans\n", "Cantarell"));
        assert!(fontconfig_reports("Ubuntu\n", "Ubuntu"));
        assert!(fontconfig_reports(
            "Noto Sans,Noto Sans Display\n",
            "noto sans"
        ));
        assert!(!fontconfig_reports("", "Ubuntu"));
    }

    #[test]
    fn windows_font_files_decide_whether_a_candidate_is_installed() {
        let dir = std::env::temp_dir().join(format!("loom-ui-font-windows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create fonts dir");
        std::fs::write(dir.join("segoeui.ttf"), b"font").expect("write font");

        assert!(windows_font_installed(&dir, "Segoe UI"));
        assert!(!windows_font_installed(&dir, "Segoe UI Variable Text"));
        assert!(!windows_font_installed(&dir, "Cantarell"));

        std::fs::remove_dir_all(&dir).expect("remove fonts dir");
    }
}
