use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use loom_desktop::settings_file;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{PresentApp, StartHubDeckEntry, StartHubState};

const RECENT_DECKS_KEY: &str = "recent_decks";
const MAX_RECENTS: usize = 20;

fn settings_path() -> PathBuf {
    settings_file::path_for_application(crate::appearance::APPLICATION_ID)
}

pub(crate) fn format_badge(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("pptx") => ".pptx",
        _ => ".loomdeck",
    }
}

pub(crate) fn relative_time_label(modified: SystemTime, now: SystemTime) -> String {
    let Ok(elapsed) = now.duration_since(modified) else {
        return "Just now".to_string();
    };
    let secs = elapsed.as_secs();
    if secs < 60 {
        "Just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

fn load_recent_paths_at(path: &Path) -> Vec<PathBuf> {
    settings_file::read(path, RECENT_DECKS_KEY)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn save_recent_paths_at(path: &Path, entries: &[PathBuf]) -> io::Result<()> {
    let encoded = entries
        .iter()
        .take(MAX_RECENTS)
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    settings_file::write(path, RECENT_DECKS_KEY, &encoded)
}

pub(crate) fn record_recent_at(settings: &Path, deck_path: &Path) -> io::Result<()> {
    let mut entries = load_recent_paths_at(settings);
    entries.retain(|p| p != deck_path);
    entries.insert(0, deck_path.to_path_buf());
    entries.truncate(MAX_RECENTS);
    save_recent_paths_at(settings, &entries)
}

pub(crate) fn record_recent(deck_path: &Path) {
    if cfg!(test) {
        return;
    }
    let _ = record_recent_at(&settings_path(), deck_path);
}

fn to_deck_entry(path: &Path, now: SystemTime) -> StartHubDeckEntry {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled Deck")
        .to_string();
    let folder = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let modified_label = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|m| relative_time_label(m, now))
        .unwrap_or_else(|_| "Saved".to_string());
    StartHubDeckEntry {
        name: SharedString::from(name),
        path: SharedString::from(path.to_string_lossy().as_ref()),
        folder: SharedString::from(folder),
        format: SharedString::from(format_badge(path)),
        modified_label: SharedString::from(modified_label),
    }
}

pub(crate) fn refresh_recents(app: &PresentApp, query: &str) {
    let now = SystemTime::now();
    let q = query.trim().to_ascii_lowercase();
    let entries: Vec<StartHubDeckEntry> = load_recent_paths_at(&settings_path())
        .into_iter()
        .map(|p| to_deck_entry(&p, now))
        .filter(|e| q.is_empty() || e.name.to_ascii_lowercase().contains(&q))
        .collect();
    app.global::<StartHubState>()
        .set_recent_decks(ModelRc::new(VecModel::from(entries)));
}

pub(crate) fn open(app: &PresentApp) {
    let hub = app.global::<StartHubState>();
    hub.set_search_query("".into());
    refresh_recents(app, "");
    hub.set_open(true);
    let toggled = !hub.get_focus_request();
    hub.set_focus_request(toggled);
    app.set_status_left("Opened Start Hub".into());
}

pub(crate) struct StartHubCallbacks {
    pub on_open_file: Rc<dyn Fn(&PresentApp)>,
    pub on_new_presentation: Rc<dyn Fn(&PresentApp)>,
    pub on_open_template: Rc<dyn Fn(&PresentApp, &str)>,
    pub on_open_deck: Rc<dyn Fn(&PresentApp, &str)>,
}

pub(crate) fn wire(app: &PresentApp, callbacks: StartHubCallbacks) {
    let hub = app.global::<StartHubState>();

    let weak = app.as_weak();
    hub.on_close_hub(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<StartHubState>().set_open(false);
        }
    });

    let weak = app.as_weak();
    let on_open_file = Rc::clone(&callbacks.on_open_file);
    hub.on_open_file(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<StartHubState>().set_open(false);
            on_open_file(&app);
        }
    });

    let weak = app.as_weak();
    let on_new_presentation = Rc::clone(&callbacks.on_new_presentation);
    hub.on_new_presentation(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<StartHubState>().set_open(false);
            on_new_presentation(&app);
        }
    });

    let weak = app.as_weak();
    let on_open_template = Rc::clone(&callbacks.on_open_template);
    hub.on_open_template(move |id| {
        if let Some(app) = weak.upgrade() {
            app.global::<StartHubState>().set_open(false);
            on_open_template(&app, id.as_str());
        }
    });

    let weak = app.as_weak();
    let on_open_deck = Rc::clone(&callbacks.on_open_deck);
    hub.on_open_deck(move |path| {
        if let Some(app) = weak.upgrade() {
            app.global::<StartHubState>().set_open(false);
            on_open_deck(&app, path.as_str());
        }
    });

    let weak = app.as_weak();
    hub.on_search_changed(move |query| {
        if let Some(app) = weak.upgrade() {
            refresh_recents(&app, query.as_str());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn format_badge_distinguishes_loomdeck_and_pptx() {
        assert_eq!(format_badge(Path::new("deck.loomdeck")), ".loomdeck");
        assert_eq!(format_badge(Path::new("slides.PPTX")), ".pptx");
    }

    #[test]
    fn relative_time_formats_buckets() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);
        assert_eq!(
            relative_time_label(now - Duration::from_secs(10), now),
            "Just now"
        );
        assert_eq!(
            relative_time_label(now - Duration::from_secs(180), now),
            "3m ago"
        );
        assert_eq!(
            relative_time_label(now - Duration::from_secs(7200), now),
            "2h ago"
        );
        assert_eq!(
            relative_time_label(now - Duration::from_secs(86_400 * 3), now),
            "3d ago"
        );
    }

    #[test]
    fn recent_decks_persist_and_deduplicate() {
        let dir = std::env::temp_dir().join(format!("loom-start-hub-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let settings = dir.join("settings.conf");
        let first = Path::new("/tmp/a.loomdeck");
        let second = Path::new("/tmp/b.pptx");
        record_recent_at(&settings, first).expect("record first");
        record_recent_at(&settings, second).expect("record second");
        record_recent_at(&settings, first).expect("re-record first");
        let loaded = load_recent_paths_at(&settings);
        assert_eq!(loaded, vec![first.to_path_buf(), second.to_path_buf()]);
    }
}
