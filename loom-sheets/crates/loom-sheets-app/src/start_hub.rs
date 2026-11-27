//! Step 1 Start / Recents Hub controller and persisted recent workbooks store.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use loom_desktop::{settings_file, NativeMenuBar};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::appearance::APPLICATION_ID;
use crate::{
    request_replacement_after_dialog, GuiState, PendingReplacement, SheetsApp, StartHubRecentItem,
    StartHubState,
};

const RECENT_WORKBOOKS_KEY: &str = "recent_workbooks";
const MAX_RECENT_WORKBOOKS: usize = 20;

/// Persisted metadata for a recently opened or saved workbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RecentWorkbookEntry {
    pub name: String,
    pub path: String,
    pub subtitle: String,
    pub kind: String,
    #[serde(default)]
    pub last_opened_epoch_secs: u64,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn kind_badge_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("xlsx") => "XLSX".to_string(),
        Some("csv") => "CSV".to_string(),
        _ => "LOOM".to_string(),
    }
}

fn display_name_for_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .or_else(|| path.file_name().and_then(|s| s.to_str()))
        .unwrap_or("Workbook")
        .to_string()
}

fn format_subtitle_for_path(path: &Path) -> String {
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = SystemTime::now().duration_since(modified) {
                let secs = age.as_secs();
                if secs < 60 {
                    return "Modified just now".to_string();
                } else if secs < 3600 {
                    return format!("Modified {}m ago", secs / 60);
                } else if secs < 86_400 {
                    return format!("Modified {}h ago", secs / 3600);
                }
                return format!("Modified {}d ago", secs / 86_400);
            }
        }
    }
    "Recently opened".to_string()
}

/// Load persisted recent workbooks from the shared application settings file.
pub(crate) fn load_recent_workbooks() -> Vec<RecentWorkbookEntry> {
    let path = settings_file::path_for_application(APPLICATION_ID);
    load_recent_workbooks_from(&path)
}

pub(crate) fn load_recent_workbooks_from(settings_path: &Path) -> Vec<RecentWorkbookEntry> {
    let Some(raw) = settings_file::read(settings_path, RECENT_WORKBOOKS_KEY) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<RecentWorkbookEntry>>(&raw).unwrap_or_default()
}

pub(crate) fn save_recent_workbooks_to(settings_path: &Path, entries: &[RecentWorkbookEntry]) {
    if let Ok(json) = serde_json::to_string(entries) {
        let _ = settings_file::write(settings_path, RECENT_WORKBOOKS_KEY, &json);
    }
}

/// Record a workbook path at the front of the recent workbooks list.
pub(crate) fn record_recent_workbook(workbook_path: &Path) {
    let settings_path = settings_file::path_for_application(APPLICATION_ID);
    record_recent_workbook_in(&settings_path, workbook_path);
}

pub(crate) fn record_recent_workbook_in(settings_path: &Path, workbook_path: &Path) {
    let canonical = workbook_path.to_string_lossy().to_string();
    if canonical.is_empty() {
        return;
    }
    let mut entries = load_recent_workbooks_from(settings_path);
    entries.retain(|entry| entry.path != canonical);
    entries.insert(
        0,
        RecentWorkbookEntry {
            name: display_name_for_path(workbook_path),
            path: canonical,
            subtitle: format_subtitle_for_path(workbook_path),
            kind: kind_badge_for_path(workbook_path),
            last_opened_epoch_secs: now_epoch_secs(),
        },
    );
    entries.truncate(MAX_RECENT_WORKBOOKS);
    save_recent_workbooks_to(settings_path, &entries);
}

/// Filter and sort recent workbook entries by search query and sidebar tab.
///
/// Tab `0` = `Recents` (most-recently-opened order).
/// Tab `1` = `Workbooks on Disk` (existing files on disk, sorted by name).
pub(crate) fn filter_recents(
    entries: &[RecentWorkbookEntry],
    query: &str,
    tab: i32,
) -> Vec<RecentWorkbookEntry> {
    filter_recents_with_exists(entries, query, tab, |p| Path::new(p).exists())
}

pub(crate) fn filter_recents_with_exists<F>(
    entries: &[RecentWorkbookEntry],
    query: &str,
    tab: i32,
    exists: F,
) -> Vec<RecentWorkbookEntry>
where
    F: Fn(&str) -> bool,
{
    let needle = query.trim().to_ascii_lowercase();
    let mut filtered: Vec<RecentWorkbookEntry> = entries
        .iter()
        .filter(|entry| {
            if tab == 1 && !exists(&entry.path) {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            entry.name.to_ascii_lowercase().contains(&needle)
                || entry.path.to_ascii_lowercase().contains(&needle)
        })
        .cloned()
        .collect();

    if tab == 1 {
        filtered.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
    }
    filtered
}

/// Sync the filtered recent workbooks into the `StartHubState` Slint global.
pub(crate) fn sync_recents_to_app(app: &SheetsApp) {
    let hub = app.global::<StartHubState>();
    let query = hub.get_search_query().to_string();
    let tab = hub.get_sidebar_tab();
    let entries = load_recent_workbooks();
    let filtered = filter_recents(&entries, &query, tab);
    let items: Vec<StartHubRecentItem> = filtered
        .into_iter()
        .map(|entry| StartHubRecentItem {
            name: SharedString::from(entry.name),
            path: SharedString::from(entry.path),
            subtitle: SharedString::from(entry.subtitle),
            kind: SharedString::from(entry.kind),
        })
        .collect();
    let count = items.len();
    hub.set_recents(ModelRc::new(VecModel::from(items)));
    let current = hub.get_selected_index();
    if count == 0 {
        hub.set_selected_index(-1);
    } else if current < 0 || current as usize >= count {
        hub.set_selected_index(0);
    }
}

/// Open the Step 1 Start / Recents Hub modal.
pub(crate) fn open_hub(app: &SheetsApp) {
    app.set_welcome_open(false);
    app.global::<crate::PivotBuilderState>().set_open(false);
    app.global::<crate::PrintPreviewState>().set_open(false);
    let dt = app.global::<crate::DataToolsState>();
    dt.set_find_open(false);
    dt.set_filter_popup_open(false);
    dt.set_named_ranges_open(false);
    let hub = app.global::<StartHubState>();
    hub.set_search_query(SharedString::default());
    hub.set_sidebar_tab(0);
    sync_recents_to_app(app);
    hub.set_open(true);
    let toggled = !hub.get_focus_request();
    hub.set_focus_request(toggled);
}

/// Close the Start Hub modal and return focus to the grid.
pub(crate) fn close_hub(app: &SheetsApp) {
    app.global::<StartHubState>().set_open(false);
    app.invoke_focus_grid();
}

/// Wire `StartHubState` callbacks for opening recents, browsing files, and
/// transitioning to Step 2 (`TemplateChooserModal`).
pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, _menu_service: &Arc<NativeMenuBar>) {
    let hub = app.global::<StartHubState>();

    {
        let app_ref = app.as_weak();
        hub.on_close_hub(move || {
            if let Some(app) = app_ref.upgrade() {
                close_hub(&app);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        hub.on_new_document(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<StartHubState>().set_open(false);
                app.set_template_chooser_open(true);
                app.invoke_focus_template_chooser();
            }
        });
    }
    {
        let app_ref = app.as_weak();
        hub.on_open_file(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<StartHubState>().set_open(false);
                app.invoke_open_sheet();
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        hub.on_open_recent(move |path_str| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let path = PathBuf::from(path_str.as_str());
            if !path.exists() {
                app.set_status_left(SharedString::from(format!(
                    "File not found: {}",
                    path.display()
                )));
                return;
            }
            app.global::<StartHubState>().set_open(false);
            if !request_replacement_after_dialog(&app, &state, PendingReplacement::OpenWorkbook) {
                crate::open_operations::open_workbook_at_path(&app, &state, path, None);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        hub.on_search_edited(move |_query| {
            if let Some(app) = app_ref.upgrade() {
                sync_recents_to_app(&app);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        hub.on_tab_changed(move |_tab| {
            if let Some(app) = app_ref.upgrade() {
                sync_recents_to_app(&app);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<RecentWorkbookEntry> {
        vec![
            RecentWorkbookEntry {
                name: "Q4 Revenue".to_string(),
                path: "/tmp/workbooks/q4_revenue.looms".to_string(),
                subtitle: "Modified 5m ago".to_string(),
                kind: "LOOM".to_string(),
                last_opened_epoch_secs: 200,
            },
            RecentWorkbookEntry {
                name: "Budget 2026".to_string(),
                path: "/tmp/workbooks/budget_2026.xlsx".to_string(),
                subtitle: "Modified 1d ago".to_string(),
                kind: "XLSX".to_string(),
                last_opened_epoch_secs: 100,
            },
        ]
    }

    #[test]
    fn filter_recents_matches_name_and_path_case_insensitively() {
        let entries = sample_entries();
        let by_name = filter_recents_with_exists(&entries, "revenue", 0, |_| true);
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "Q4 Revenue");

        let by_ext = filter_recents_with_exists(&entries, "XLSX", 0, |_| true);
        assert_eq!(by_ext.len(), 1);
        assert_eq!(by_ext[0].name, "Budget 2026");

        let all = filter_recents_with_exists(&entries, "  ", 0, |_| true);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn workbooks_on_disk_tab_filters_missing_and_sorts_by_name() {
        let entries = sample_entries();
        let disk = filter_recents_with_exists(&entries, "", 1, |p| {
            p.ends_with(".xlsx") || p.ends_with(".looms")
        });
        assert_eq!(disk.len(), 2);
        assert_eq!(disk[0].name, "Budget 2026");
        assert_eq!(disk[1].name, "Q4 Revenue");

        let only_existing =
            filter_recents_with_exists(&entries, "", 1, |p| p.ends_with("q4_revenue.looms"));
        assert_eq!(only_existing.len(), 1);
        assert_eq!(only_existing[0].name, "Q4 Revenue");
    }

    #[test]
    fn record_recent_workbook_deduplicates_and_moves_to_front() {
        let dir =
            std::env::temp_dir().join(format!("loom-sheets-start-hub-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let settings = dir.join("settings.json");
        let first = dir.join("Alpha.looms");
        let second = dir.join("Beta.xlsx");
        std::fs::write(&first, b"{}").expect("write first");
        std::fs::write(&second, b"{}").expect("write second");

        record_recent_workbook_in(&settings, &first);
        record_recent_workbook_in(&settings, &second);
        record_recent_workbook_in(&settings, &first);

        let loaded = load_recent_workbooks_from(&settings);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "Alpha");
        assert_eq!(loaded[0].kind, "LOOM");
        assert_eq!(loaded[1].name, "Beta");
        assert_eq!(loaded[1].kind, "XLSX");
    }
}
