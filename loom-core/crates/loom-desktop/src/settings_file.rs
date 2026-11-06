//! The per-user `settings.toml` that remembers every desktop choice, one
//! `key = "value"` line per setting.
//!
//! A write replaces the whole file in one step and keeps the lines of every
//! other setting, so one choice never erases another. A missing, unreadable,
//! oversized or corrupt file reads as "no saved value", and each caller decides
//! its own default.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Largest settings file that is read. A bigger file is treated as corrupt.
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
/// First line of every settings file.
const HEADER: &str = "# Loom settings\n";

/// The settings file of one application id, below the user's configuration
/// directory.
pub fn path_for_application(application_id: &str) -> PathBuf {
    settings_directory(application_id).join("settings.toml")
}

/// The saved text of one setting, or `None` when the file is missing,
/// unreadable, oversized or has no such key.
pub fn read(path: &Path, key: &str) -> Option<String> {
    read_text(path)?.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches('"').to_string())
    })
}

/// Saves one setting. The file is replaced in one step, so a crash never
/// leaves half a file, and the other settings' lines are kept.
pub fn write(path: &Path, key: &str, value: &str) -> io::Result<()> {
    let mut text = String::from(HEADER);
    text.push_str(&format!("{key} = \"{value}\"\n"));
    if let Some(existing) = read_text(path) {
        for line in existing.lines() {
            let trimmed = line.trim();
            let is_other_setting = trimmed
                .split_once('=')
                .is_some_and(|(name, _)| !trimmed.starts_with('#') && name.trim() != key);
            if is_other_setting {
                text.push_str(trimmed);
                text.push('\n');
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut temporary = path.to_path_buf().into_os_string();
    temporary.push(format!(".tmp{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_text(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// `LOOM_CONFIG_DIR` when set; otherwise `%APPDATA%` on Windows,
/// `~/Library/Application Support` on macOS, and `$XDG_CONFIG_HOME` or
/// `~/.config` elsewhere, with `loom/<id>` below it.
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

    fn temporary_path(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("loom-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("temporary directory");
        directory.join("settings.toml")
    }

    #[test]
    fn a_write_is_read_back_and_keeps_other_keys() {
        let path = temporary_path("keeps-other-keys");
        write(&path, "appearance", "dark").expect("write appearance");
        write(&path, "window_frame", "loom").expect("write frame");
        assert_eq!(read(&path, "appearance").as_deref(), Some("dark"));
        assert_eq!(read(&path, "window_frame").as_deref(), Some("loom"));
        write(&path, "appearance", "light").expect("rewrite appearance");
        assert_eq!(read(&path, "window_frame").as_deref(), Some("loom"));
        assert_eq!(read(&path, "appearance").as_deref(), Some("light"));
    }

    #[test]
    fn missing_oversized_or_unknown_keys_read_as_no_value() {
        let path = temporary_path("missing");
        assert_eq!(read(&path, "appearance"), None);
        write(&path, "appearance", "dark").expect("write");
        assert_eq!(read(&path, "window_frame"), None);
        fs::write(&path, vec![b'x'; (MAX_SETTINGS_BYTES + 1) as usize]).expect("oversized");
        assert_eq!(read(&path, "appearance"), None);
    }
}
