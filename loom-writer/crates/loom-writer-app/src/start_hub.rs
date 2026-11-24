//! Start / Recents Hub controller and persisted recent documents store.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use loom_desktop::{settings_file, NativeMenuBar};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::appearance::APPLICATION_ID;
use crate::{GuiState, StartHubRecentItem, StartHubState, WriterApp};

const RECENT_DOCUMENTS_KEY: &str = "recent_documents";
const MAX_RECENT_DOCUMENTS: usize = 20;

/// Persisted metadata for a recently opened or saved document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentDocumentEntry {
    pub name: String,
    pub path: String,
    pub subtitle: String,
    pub kind: String,
    pub last_opened_epoch_secs: u64,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn kind_badge_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("docx") => "DOCX".to_string(),
        _ => "LOOM".to_string(),
    }
}

pub(crate) fn display_name_for_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .or_else(|| path.file_name().and_then(|s| s.to_str()))
        .unwrap_or("Document")
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

fn encode_field(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '%' => out.push_str("%25"),
            '|' => out.push_str("%7C"),
            ';' => out.push_str("%3B"),
            '"' => out.push_str("%22"),
            '\\' => out.push_str("%5C"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            _ => out.push(ch),
        }
    }
    out
}

fn decode_field(encoded: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut chars = encoded.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = chars.next()?;
            let lo = chars.next()?;
            let hex = [hi, lo].iter().collect::<String>();
            let byte = u8::from_str_radix(&hex, 16).ok()?;
            bytes.push(byte);
        } else {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    String::from_utf8(bytes).ok()
}

/// Encode recent entries into a single quote-free, newline-free string suitable
/// for `loom_desktop::settings_file`.
pub(crate) fn encode_recent_documents(entries: &[RecentDocumentEntry]) -> String {
    entries
        .iter()
        .take(MAX_RECENT_DOCUMENTS)
        .map(|entry| {
            format!(
                "{}|{}|{}|{}|{}",
                encode_field(&entry.name),
                encode_field(&entry.path),
                encode_field(&entry.subtitle),
                encode_field(&entry.kind),
                entry.last_opened_epoch_secs
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Decode recent entries from the serialized `settings_file` value, skipping
/// malformed records without panicking.
pub(crate) fn decode_recent_documents(raw: &str) -> Vec<RecentDocumentEntry> {
    let mut out = Vec::new();
    for record in raw.split(';') {
        if record.is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.split('|').collect();
        if parts.len() != 5 {
            continue;
        }
        let (Some(name), Some(path), Some(subtitle), Some(kind), Ok(epoch)) = (
            decode_field(parts[0]),
            decode_field(parts[1]),
            decode_field(parts[2]),
            decode_field(parts[3]),
            parts[4].parse::<u64>(),
        ) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        out.push(RecentDocumentEntry {
            name,
            path,
            subtitle,
            kind,
            last_opened_epoch_secs: epoch,
        });
        if out.len() >= MAX_RECENT_DOCUMENTS {
            break;
        }
    }
    out
}

/// Load persisted recent documents from the shared application settings file.
pub(crate) fn load_recent_documents() -> Vec<RecentDocumentEntry> {
    let path = settings_file::path_for_application(APPLICATION_ID);
    load_recent_documents_from(&path)
}

pub(crate) fn load_recent_documents_from(settings_path: &Path) -> Vec<RecentDocumentEntry> {
    let Some(raw) = settings_file::read(settings_path, RECENT_DOCUMENTS_KEY) else {
        return Vec::new();
    };
    decode_recent_documents(&raw)
}

pub(crate) fn save_recent_documents_to(settings_path: &Path, entries: &[RecentDocumentEntry]) {
    let encoded = encode_recent_documents(entries);
    let _ = settings_file::write(settings_path, RECENT_DOCUMENTS_KEY, &encoded);
}

/// Record a document path at the front of the recent documents list.
pub(crate) fn record_recent_document(document_path: &Path) {
    if cfg!(test) {
        return;
    }
    let settings_path = settings_file::path_for_application(APPLICATION_ID);
    record_recent_document_in(&settings_path, document_path);
}

std::thread_local! {
    static PENDING_RECENT_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Completes a deferred recent-document open after a dirty-document Save/Discard confirmation.
pub(crate) fn continue_pending_recent_open(
    app: &WriterApp,
    state: &GuiState,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    if let Some(path) = PENDING_RECENT_PATH.with(|cell| cell.borrow_mut().take()) {
        open_document_at_path(app, state, path, menu_service);
    }
}

pub(crate) fn record_recent_document_in(settings_path: &Path, document_path: &Path) {
    let canonical = document_path.to_string_lossy().to_string();
    if canonical.is_empty() {
        return;
    }
    let mut entries = load_recent_documents_from(settings_path);
    entries.retain(|entry| entry.path != canonical);
    entries.insert(
        0,
        RecentDocumentEntry {
            name: display_name_for_path(document_path),
            path: canonical,
            subtitle: format_subtitle_for_path(document_path),
            kind: kind_badge_for_path(document_path),
            last_opened_epoch_secs: now_epoch_secs(),
        },
    );
    entries.truncate(MAX_RECENT_DOCUMENTS);
    save_recent_documents_to(settings_path, &entries);
}

/// Filter and sort recent document entries by search query and sidebar tab.
///
/// Tab `0` = `Recents` (most-recently-opened order).
/// Tab `1` = `Documents on Disk` (existing files on disk, sorted by name).
pub(crate) fn filter_recents(
    entries: &[RecentDocumentEntry],
    query: &str,
    tab: i32,
) -> Vec<RecentDocumentEntry> {
    filter_recents_with_exists(entries, query, tab, |p| Path::new(p).exists())
}

pub(crate) fn filter_recents_with_exists<F>(
    entries: &[RecentDocumentEntry],
    query: &str,
    tab: i32,
    exists: F,
) -> Vec<RecentDocumentEntry>
where
    F: Fn(&str) -> bool,
{
    let needle = query.trim().to_ascii_lowercase();
    let mut filtered: Vec<RecentDocumentEntry> = entries
        .iter()
        .filter(|entry| {
            if tab == 1 && !exists(&entry.path) {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            entry.name.to_lowercase().contains(&needle)
                || entry.path.to_lowercase().contains(&needle)
                || entry.kind.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();

    if tab == 1 {
        filtered.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
    }
    filtered
}

/// Sync the filtered recent documents into the `StartHubState` Slint global.
pub(crate) fn sync_recents_to_app(app: &WriterApp) {
    let hub = app.global::<StartHubState>();
    let query = hub.get_search_query().to_string();
    let tab = hub.get_sidebar_tab();
    let entries = load_recent_documents();
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
pub(crate) fn open_hub(app: &WriterApp) {
    app.global::<crate::WelcomeGuideState>().set_open(false);
    app.global::<crate::PrintPreviewState>().set_open(false);
    app.global::<crate::WordCountState>().set_open(false);
    let hub = app.global::<StartHubState>();
    hub.set_search_query(SharedString::default());
    hub.set_sidebar_tab(0);
    sync_recents_to_app(app);
    hub.set_open(true);
    let toggled = !hub.get_focus_request();
    hub.set_focus_request(toggled);
}

/// Close the Start Hub modal and return focus to the page.
pub(crate) fn close_hub(app: &WriterApp) {
    app.global::<StartHubState>().set_open(false);
    app.invoke_focus_page();
}

/// Open a `.loomdoc` or `.docx` file directly from a recent-documents path.
pub(crate) fn open_document_at_path(
    app: &WriterApp,
    state: &GuiState,
    path: PathBuf,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"))
    {
        match crate::docx_actions::open_docx_path(app, state, path.clone(), menu_service) {
            Ok(true) => record_recent_document(&path),
            Ok(false) => {}
            Err(error) => app.set_status_left(SharedString::from(format!("Open failed: {error}"))),
        }
    } else {
        match crate::load_file(&path) {
            Ok(document) => {
                crate::replace_opened_document(app, state, path.clone(), document);
                crate::sync_writer_menu_if_present(menu_service, app, state);
                record_recent_document(&path);
                app.set_status_left(SharedString::from(format!("Opened {}", path.display())));
            }
            Err(error) => app.set_status_left(SharedString::from(format!("Open failed: {error}"))),
        }
    }
    app.invoke_focus_page();
}

/// Wire `StartHubState` callbacks for opening recents, blank/template creation,
/// and browsing files.
pub(crate) fn wire(
    app: &WriterApp,
    state: &Rc<GuiState>,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
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
        let state = Rc::clone(state);
        let menu_service = menu_service.clone();
        hub.on_blank_document(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            app.global::<StartHubState>().set_open(false);
            let needs_reset = crate::document_is_dirty(&state)
                || state.save_path.borrow().is_some()
                || !state.current.borrow().is_empty();
            if needs_reset
                && !crate::request_document_replacement(
                    &app,
                    &state,
                    crate::PendingReplacement::CreateTemplate(crate::TemplateId::Blank),
                )
            {
                crate::templates::apply_template_document(
                    &app,
                    &state,
                    crate::TemplateId::Blank,
                    menu_service.as_deref(),
                );
            }
            app.invoke_focus_page();
        });
    }
    {
        let app_ref = app.as_weak();
        hub.on_new_document(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<StartHubState>().set_open(false);
                app.set_template_selected("blank".into());
                app.set_template_category(0);
                app.set_template_chooser_open(true);
                app.set_status_left("Choose a document template".into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        hub.on_open_file(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<StartHubState>().set_open(false);
                app.invoke_open_doc();
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = menu_service.clone();
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
            PENDING_RECENT_PATH.with(|cell| *cell.borrow_mut() = Some(path.clone()));
            if !crate::request_document_replacement(
                &app,
                &state,
                crate::PendingReplacement::OpenRecent,
            ) {
                PENDING_RECENT_PATH.with(|cell| *cell.borrow_mut() = None);
                open_document_at_path(&app, &state, path, &menu_service);
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

    fn sample_entries() -> Vec<RecentDocumentEntry> {
        vec![
            RecentDocumentEntry {
                name: "Q4 Proposal".to_string(),
                path: "/tmp/docs/q4_proposal.loomdoc".to_string(),
                subtitle: "Modified 5m ago".to_string(),
                kind: "LOOM".to_string(),
                last_opened_epoch_secs: 200,
            },
            RecentDocumentEntry {
                name: "Annual Brief".to_string(),
                path: "/tmp/docs/annual_brief.docx".to_string(),
                subtitle: "Modified 1d ago".to_string(),
                kind: "DOCX".to_string(),
                last_opened_epoch_secs: 100,
            },
        ]
    }

    #[test]
    fn percent_encoding_round_trips_reserved_and_unicode_characters() {
        let entries = vec![RecentDocumentEntry {
            name: "Report | 100%; \"Final\"\nDraft\\v2".to_string(),
            path: "C:\\Users\\Ada\\Docs\\Report | 50%; \"Final\".docx".to_string(),
            subtitle: "Modified just now".to_string(),
            kind: "DOCX".to_string(),
            last_opened_epoch_secs: 1_700_000_123,
        }];
        let encoded = encode_recent_documents(&entries);
        assert!(!encoded.contains('"'));
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains('\r'));
        let decoded = decode_recent_documents(&encoded);
        assert_eq!(decoded, entries);

        let unicode = vec![RecentDocumentEntry {
            name: "مذكرة عمل — Résumé".to_string(),
            path: "/tmp/مذكرة|résumé;2026.loomdoc".to_string(),
            subtitle: "Recently opened".to_string(),
            kind: "LOOM".to_string(),
            last_opened_epoch_secs: 42,
        }];
        assert_eq!(
            decode_recent_documents(&encode_recent_documents(&unicode)),
            unicode
        );
    }

    #[test]
    fn decode_skips_corrupt_or_incomplete_records_safely() {
        let valid = RecentDocumentEntry {
            name: "Notes".to_string(),
            path: "/tmp/notes.loomdoc".to_string(),
            subtitle: "Recently opened".to_string(),
            kind: "LOOM".to_string(),
            last_opened_epoch_secs: 99,
        };
        let encoded_valid = encode_recent_documents(std::slice::from_ref(&valid));
        let mixed = format!(
            ";bad|record;too|few|fields|here;broken%ZZ|path|sub|LOOM|1;noepoch|/p|s|LOOM|abc;empty||sub|LOOM|1;{encoded_valid}"
        );
        assert_eq!(decode_recent_documents(&mixed), vec![valid]);
        assert!(decode_recent_documents("").is_empty());
    }

    #[test]
    fn filter_recents_matches_name_path_and_kind_case_insensitively() {
        let entries = sample_entries();
        let by_name = filter_recents_with_exists(&entries, "proposal", 0, |_| true);
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "Q4 Proposal");

        let by_kind = filter_recents_with_exists(&entries, "docx", 0, |_| true);
        assert_eq!(by_kind.len(), 1);
        assert_eq!(by_kind[0].name, "Annual Brief");

        let all = filter_recents_with_exists(&entries, "  ", 0, |_| true);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn documents_on_disk_tab_filters_missing_and_sorts_by_name() {
        let entries = sample_entries();
        let disk = filter_recents_with_exists(&entries, "", 1, |p| {
            p.ends_with(".docx") || p.ends_with(".loomdoc")
        });
        assert_eq!(disk.len(), 2);
        assert_eq!(disk[0].name, "Annual Brief");
        assert_eq!(disk[1].name, "Q4 Proposal");

        let only_existing =
            filter_recents_with_exists(&entries, "", 1, |p| p.ends_with("q4_proposal.loomdoc"));
        assert_eq!(only_existing.len(), 1);
        assert_eq!(only_existing[0].name, "Q4 Proposal");
    }

    #[test]
    fn record_recent_document_deduplicates_moves_to_front_and_caps_at_twenty() {
        let dir =
            std::env::temp_dir().join(format!("loom-writer-start-hub-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let settings = dir.join("settings.toml");
        let first = dir.join("Alpha.loomdoc");
        let second = dir.join("Beta.DOCX");
        std::fs::write(&first, b"{}").expect("write first");
        std::fs::write(&second, b"{}").expect("write second");

        record_recent_document_in(&settings, &first);
        record_recent_document_in(&settings, &second);
        record_recent_document_in(&settings, &first);

        let loaded = load_recent_documents_from(&settings);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "Alpha");
        assert_eq!(loaded[0].kind, "LOOM");
        assert_eq!(loaded[1].name, "Beta");
        assert_eq!(loaded[1].kind, "DOCX");

        for i in 0..25 {
            let p = dir.join(format!("Doc_{i:02}.loomdoc"));
            std::fs::write(&p, b"{}").expect("write numbered doc");
            record_recent_document_in(&settings, &p);
        }
        let capped = load_recent_documents_from(&settings);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(capped.len(), MAX_RECENT_DOCUMENTS);
        assert_eq!(capped[0].name, "Doc_24");
        assert_eq!(capped[19].name, "Doc_05");
    }

    #[test]
    fn edge_case_path_helpers_handle_empty_and_extensionless_paths() {
        assert_eq!(kind_badge_for_path(Path::new("/tmp/readme")), "LOOM");
        assert_eq!(kind_badge_for_path(Path::new("/tmp/memo.DocX")), "DOCX");
        assert_eq!(display_name_for_path(Path::new("")), "Document");
        assert_eq!(display_name_for_path(Path::new("/tmp/.hidden")), ".hidden");
    }
}
