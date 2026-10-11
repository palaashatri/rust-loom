//! What the font picker remembers between runs, and the system font scan that
//! runs off the interface thread.
//!
//! The recently used families and the per-file scan results live in the same
//! per-user directory as the other settings (`LOOM_CONFIG_DIR`, or the
//! platform's configuration directory). Both are caches of convenience: a
//! missing, oversized or damaged file only costs a re-scan or an empty list.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use loom_writer_core::fonts::{scan_system_fonts, FontCatalog, ScanConfig, ScanLimits};

/// Most recently used families kept.
pub(crate) const MAX_RECENTS: usize = 8;
/// Longest family name kept, in characters.
const MAX_NAME_CHARS: usize = 128;
/// Largest recents file read.
const MAX_RECENTS_BYTES: u64 = 16 * 1024;
/// Largest scan cache read. A thousand families is a few megabytes.
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// The bounds of the scan the picker runs: the library default for files,
/// depth and entries, and a wall-clock budget so a slow network volume cannot
/// keep the worker busy for long.
pub(crate) fn scan_limits() -> ScanLimits {
    ScanLimits {
        max_duration: Some(Duration::from_secs(15)),
        ..ScanLimits::default()
    }
}

/// The picker's files in one directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontStore {
    directory: PathBuf,
}

impl FontStore {
    /// A store in `directory` (created on first save).
    pub(crate) fn at(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// The per-user store of the application with id `application_id`, next to
    /// its settings file.
    pub(crate) fn for_application(application_id: &str) -> Self {
        let settings = loom_desktop::AppearanceStore::for_application(application_id);
        let directory = settings
            .path()
            .parent()
            .map_or_else(PathBuf::new, PathBuf::from);
        Self::at(directory)
    }

    fn recents_path(&self) -> PathBuf {
        self.directory.join("recent-fonts.txt")
    }

    fn cache_path(&self) -> PathBuf {
        self.directory.join("font-scan-cache.txt")
    }

    fn read_bounded(path: &PathBuf, limit: u64) -> Option<String> {
        let file = std::fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() > limit {
            return None;
        }
        let mut text = String::new();
        file.take(limit).read_to_string(&mut text).ok()?;
        Some(text)
    }

    /// The remembered families, most recent first.
    pub(crate) fn load_recents(&self) -> Vec<String> {
        let Some(text) = Self::read_bounded(&self.recents_path(), MAX_RECENTS_BYTES) else {
            return Vec::new();
        };
        let mut recents: Vec<String> = Vec::new();
        for line in text.lines() {
            let name = line.trim();
            if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
                continue;
            }
            if !recents.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                recents.push(name.to_owned());
            }
            if recents.len() == MAX_RECENTS {
                break;
            }
        }
        recents
    }

    /// Stores `recents`, replacing the file in one step.
    pub(crate) fn save_recents(&self, recents: &[String]) -> Result<(), String> {
        std::fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        let mut text = recents.join("\n");
        text.push('\n');
        loom_storage::atomic_write(&self.recents_path(), text.as_bytes())
            .map_err(|error| error.to_string())
    }

    /// The text of the last scan's cache, if one was stored and is not absurd.
    pub(crate) fn load_scan_cache(&self) -> Option<String> {
        Self::read_bounded(&self.cache_path(), MAX_CACHE_BYTES)
    }

    /// Stores the scan cache text, replacing the file in one step.
    pub(crate) fn save_scan_cache(&self, text: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        loom_storage::atomic_write(&self.cache_path(), text.as_bytes())
            .map_err(|error| error.to_string())
    }
}

/// `recents` with `family` moved to the front, without repeats, at most
/// [`MAX_RECENTS`] long. An empty or overlong name is not remembered.
pub(crate) fn with_recent(recents: &[String], family: &str) -> Vec<String> {
    let family = family.trim();
    if family.is_empty() || family.chars().count() > MAX_NAME_CHARS {
        return recents.to_vec();
    }
    let mut updated = vec![family.to_owned()];
    updated.extend(
        recents
            .iter()
            .filter(|known| !known.eq_ignore_ascii_case(family))
            .cloned(),
    );
    updated.truncate(MAX_RECENTS);
    updated
}

/// Scans `config` on a new thread and hands the finished catalogue to
/// `deliver` (called on that thread). The per-file results are read from and
/// written back to `store`, so a restart reads only files that changed.
pub(crate) fn scan_in_background(
    config: ScanConfig,
    store: Option<FontStore>,
    deliver: impl FnOnce(Arc<FontCatalog>) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let cache_text = store.as_ref().and_then(FontStore::load_scan_cache);
        let (catalog, cache_text) = scan_system_fonts(&config, cache_text.as_deref());
        if let Some(store) = &store {
            // A cache that cannot be written only costs the next start a scan.
            let _ = store.save_scan_cache(&cache_text);
        }
        deliver(Arc::new(catalog));
    })
}

/// The scan configuration of the running system, with the picker's limits.
pub(crate) fn system_scan_config() -> ScanConfig {
    let mut config = ScanConfig::system();
    config.limits = scan_limits();
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_writer_core::test_fonts;
    use std::sync::mpsc;

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "loom-writer-font-store-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn recents_keep_the_newest_first_without_repeats_and_are_bounded() {
        let mut recents: Vec<String> = Vec::new();
        for name in ["A", "B", "C", "B", "d", "D", "E", "F", "G", "H", "I", "J"] {
            recents = with_recent(&recents, name);
        }
        assert_eq!(recents.len(), MAX_RECENTS);
        assert_eq!(recents[0], "J");
        // Ten distinct names were used; the two used longest ago fell off, and
        // "d" then "D" is one family, kept as it was last written.
        assert_eq!(recents, ["J", "I", "H", "G", "F", "E", "D", "B"]);
        assert_eq!(
            with_recent(&recents, "  "),
            recents,
            "an empty name is ignored"
        );
        let moved = with_recent(&recents, "g");
        assert_eq!(moved[0], "g");
        assert_eq!(moved.len(), MAX_RECENTS);
    }

    #[test]
    fn recents_round_trip_through_the_directory_and_survive_damage() {
        let directory = scratch("recents");
        let store = FontStore::at(&directory);
        assert!(store.load_recents().is_empty(), "nothing stored yet");
        let recents = vec!["Georgia".to_owned(), "Courier Prime".to_owned()];
        store.save_recents(&recents).expect("save");
        assert_eq!(store.load_recents(), recents);
        // An oversized file is ignored, not read.
        std::fs::write(store.recents_path(), vec![b'x'; 32 * 1024]).unwrap();
        assert!(store.load_recents().is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_scan_runs_on_its_own_thread_and_fills_the_cache() {
        let fonts = test_fonts::install(&[("Wider", 200)]);
        let directory = scratch("scan");
        let store = FontStore::at(&directory);
        let (sender, receiver) = mpsc::channel();
        let main_thread = std::thread::current().id();
        let handle = scan_in_background(fonts.scan_config(), Some(store.clone()), move |catalog| {
            let _ = sender.send((std::thread::current().id(), catalog));
        });
        let (worker, catalog) = receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("the scan finishes");
        handle.join().unwrap();
        assert_ne!(
            worker, main_thread,
            "scanning happened off the calling thread"
        );
        assert!(catalog.has_family("Wider"));
        assert!(
            catalog.has_family("Inter"),
            "the bundled faces are always there"
        );
        assert!(store.load_scan_cache().is_some(), "the cache was written");

        // A second scan reads no font file: it is answered from the cache.
        let (sender, receiver) = mpsc::channel();
        scan_in_background(fonts.scan_config(), Some(store), move |catalog| {
            let _ = sender.send(catalog);
        })
        .join()
        .unwrap();
        let second = receiver.recv().unwrap();
        assert!(second.report().cache_hits > 0);
        assert_eq!(second.report().files_parsed, 0);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_picker_scan_is_bounded() {
        let limits = scan_limits();
        assert!(limits.max_duration.is_some());
        assert!(limits.max_files <= 20_000 && limits.max_entries <= 200_000);
    }
}
