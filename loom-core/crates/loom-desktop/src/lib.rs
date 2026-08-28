//! Native desktop service contracts and adapters for Loom applications.
//!
//! This crate keeps platform dialogs behind an injectable interface so the
//! production applications can use native system dialogs while deterministic
//! tests use a scripted backend without opening windows.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub mod menu;
pub use menu::{
    build_standard_menu_bar, CommandAction, CommandSource, CommandState, CommandStateProjection,
    Menu, MenuBar, MenuBarService, MenuItem, MenuShortcut, NativeMenuBar, ScriptedMenuBar,
};

/// A display name and extension list presented by a native file dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFilter {
    /// Human-readable filter name, such as `Loom Writer document`.
    pub name: String,
    /// Extensions without a leading dot, such as `loomdoc` or `pdf`.
    pub extensions: Vec<String>,
}

impl FileFilter {
    /// Creates a validated file filter.
    pub fn new(
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, DesktopError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(DesktopError::InvalidRequest(
                "file-filter name must not be empty".into(),
            ));
        }
        let extensions = extensions
            .into_iter()
            .map(Into::into)
            .map(|extension: String| extension.trim_start_matches('.').to_ascii_lowercase())
            .collect::<Vec<_>>();
        if extensions.is_empty()
            || extensions.iter().any(|extension| {
                extension.is_empty()
                    || !extension
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
        {
            return Err(DesktopError::InvalidRequest(
                "file-filter extensions must be non-empty and contain only ASCII letters, digits, '-' or '_'"
                    .into(),
            ));
        }
        Ok(Self { name, extensions })
    }
}

/// Request for opening one existing file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenFileRequest {
    /// Native dialog title.
    pub title: String,
    /// Initial directory when known.
    pub initial_directory: Option<PathBuf>,
    /// Optional suggested file name.
    pub suggested_name: Option<String>,
    /// Allowed file types.
    pub filters: Vec<FileFilter>,
}

/// Request for choosing an output file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveFileRequest {
    /// Native dialog title.
    pub title: String,
    /// Initial directory when known.
    pub initial_directory: Option<PathBuf>,
    /// Suggested output file name.
    pub suggested_name: Option<String>,
    /// Allowed output file types.
    pub filters: Vec<FileFilter>,
}

/// Desktop-service failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    /// A request was malformed.
    InvalidRequest(String),
    /// A deterministic test script did not contain another response.
    ScriptExhausted(&'static str),
}

impl fmt::Display for DesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => {
                write!(formatter, "invalid desktop request: {message}")
            }
            Self::ScriptExhausted(operation) => {
                write!(
                    formatter,
                    "scripted desktop service exhausted during {operation}"
                )
            }
        }
    }
}

impl std::error::Error for DesktopError {}

/// Injectable native file-dialog interface.
pub trait FileDialogService: Send + Sync {
    /// Opens a platform file picker. `Ok(None)` means the user cancelled.
    fn open_file(&self, request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError>;

    /// Opens a platform save picker. `Ok(None)` means the user cancelled.
    fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError>;
}

/// Production file dialogs implemented through the operating system.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeFileDialogs;

impl FileDialogService for NativeFileDialogs {
    fn open_file(&self, request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_open_request(request)?;
        Ok(configure_open_dialog(request).pick_file())
    }

    fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_save_request(request)?;
        Ok(configure_save_dialog(request).save_file())
    }
}

fn configure_open_dialog(request: &OpenFileRequest) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if !request.title.trim().is_empty() {
        dialog = dialog.set_title(request.title.trim());
    }
    if let Some(directory) = request.initial_directory.as_deref() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(name) = request.suggested_name.as_deref() {
        dialog = dialog.set_file_name(name);
    }
    for filter in &request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    dialog
}

fn configure_save_dialog(request: &SaveFileRequest) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if !request.title.trim().is_empty() {
        dialog = dialog.set_title(request.title.trim());
    }
    if let Some(directory) = request.initial_directory.as_deref() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(name) = request.suggested_name.as_deref() {
        dialog = dialog.set_file_name(name);
    }
    for filter in &request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    dialog
}

fn validate_open_request(request: &OpenFileRequest) -> Result<(), DesktopError> {
    validate_initial_directory(request.initial_directory.as_deref())?;
    validate_suggested_name(request.suggested_name.as_deref())
}

fn validate_save_request(request: &SaveFileRequest) -> Result<(), DesktopError> {
    validate_initial_directory(request.initial_directory.as_deref())?;
    validate_suggested_name(request.suggested_name.as_deref())
}

fn validate_initial_directory(directory: Option<&Path>) -> Result<(), DesktopError> {
    if directory.is_some_and(|directory| directory.as_os_str().is_empty()) {
        return Err(DesktopError::InvalidRequest(
            "initial directory must not be an empty path".into(),
        ));
    }
    Ok(())
}

fn validate_suggested_name(name: Option<&str>) -> Result<(), DesktopError> {
    if let Some(name) = name {
        let name = name.trim();
        if name.is_empty() {
            return Err(DesktopError::InvalidRequest(
                "suggested file name must not be empty".into(),
            ));
        }
        if Path::new(name).components().count() != 1 {
            return Err(DesktopError::InvalidRequest(
                "suggested file name must not contain a directory".into(),
            ));
        }
    }
    Ok(())
}

/// Deterministic dialog backend for unit and application-controller tests.
#[derive(Debug, Default)]
pub struct ScriptedFileDialogs {
    open_results: Mutex<VecDeque<Option<PathBuf>>>,
    save_results: Mutex<VecDeque<Option<PathBuf>>>,
}

impl ScriptedFileDialogs {
    /// Creates a backend with ordered open and save responses.
    pub fn new(
        open_results: impl IntoIterator<Item = Option<PathBuf>>,
        save_results: impl IntoIterator<Item = Option<PathBuf>>,
    ) -> Self {
        Self {
            open_results: Mutex::new(open_results.into_iter().collect()),
            save_results: Mutex::new(save_results.into_iter().collect()),
        }
    }
}

impl FileDialogService for ScriptedFileDialogs {
    fn open_file(&self, request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_open_request(request)?;
        self.open_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .ok_or(DesktopError::ScriptExhausted("open_file"))
    }

    fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_save_request(request)?;
        self.save_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .ok_or(DesktopError::ScriptExhausted("save_file"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_normalizes_dotted_extensions() {
        let filter = FileFilter::new("Writer", [".LOOMDOC", "md"]).expect("valid filter");
        assert_eq!(filter.extensions, ["loomdoc", "md"]);
    }

    #[test]
    fn filter_rejects_path_like_extensions() {
        assert!(matches!(
            FileFilter::new("Invalid", ["../loomdoc"]),
            Err(DesktopError::InvalidRequest(_))
        ));
    }

    #[test]
    fn scripted_dialogs_preserve_cancel_and_paths() {
        let dialogs = ScriptedFileDialogs::new(
            [Some(PathBuf::from("draft.loomdoc")), None],
            [Some(PathBuf::from("export.pdf"))],
        );
        let open = OpenFileRequest {
            title: "Open".into(),
            filters: vec![FileFilter::new("Writer", ["loomdoc"]).expect("filter")],
            ..OpenFileRequest::default()
        };
        let save = SaveFileRequest {
            title: "Export".into(),
            suggested_name: Some("document.pdf".into()),
            filters: vec![FileFilter::new("PDF", ["pdf"]).expect("filter")],
            ..SaveFileRequest::default()
        };

        assert_eq!(
            dialogs.open_file(&open).expect("first open"),
            Some(PathBuf::from("draft.loomdoc"))
        );
        assert_eq!(dialogs.open_file(&open).expect("cancelled open"), None);
        assert_eq!(
            dialogs.save_file(&save).expect("save"),
            Some(PathBuf::from("export.pdf"))
        );
        assert_eq!(
            dialogs.save_file(&save),
            Err(DesktopError::ScriptExhausted("save_file"))
        );
    }

    #[test]
    fn menu_bar_item_lookup_and_state_updates() {
        let mut menu_bar = build_standard_menu_bar(
            "Loom Test",
            vec![MenuItem::action("file.export", "Export Custom...")],
            vec![],
            vec![],
            vec![],
        );
        assert!(menu_bar.find_item("file.new").is_some());
        assert!(menu_bar.find_item("file.export").is_some());
        assert_eq!(menu_bar.find_item("nonexistent"), None);

        // Update state
        assert!(menu_bar.update_item_state("file.save", false, None));
        let item = menu_bar.find_item("file.save").unwrap();
        assert!(!item.is_enabled());

        // Update check state
        assert!(menu_bar.update_item_state("view.inspector", true, Some(false)));
        if let Some(MenuItem::Check { checked, .. }) = menu_bar.find_item("view.inspector") {
            assert!(!*checked);
        } else {
            panic!("expected check item");
        }
    }

    #[test]
    fn menu_bar_dbusmenu_json_generation() {
        let menu_bar = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        let json = menu_bar.to_dbusmenu_json();
        assert!(json.contains("File"));
        assert!(json.contains("Edit"));
        assert!(json.contains("View"));
        assert!(json.contains("Window"));
        assert!(json.contains("Help"));
    }

    #[test]
    fn scripted_menu_bar_records_install_and_actions() {
        let service = ScriptedMenuBar::new();
        let bar = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        service.install_menu_bar(&bar).expect("install");
        service.dispatch_action("file.new").expect("dispatch");
        service.dispatch_action("edit.undo").expect("dispatch");

        assert_eq!(service.installed_bars().len(), 1);
        assert_eq!(
            service.dispatched_actions(),
            vec!["file.new".to_string(), "edit.undo".to_string()]
        );
    }
    #[test]
    fn suggested_name_cannot_escape_the_dialog_directory() {
        let dialogs = ScriptedFileDialogs::default();
        let request = SaveFileRequest {
            suggested_name: Some("../escape.loomdoc".into()),
            ..SaveFileRequest::default()
        };
        assert!(matches!(
            dialogs.save_file(&request),
            Err(DesktopError::InvalidRequest(_))
        ));
    }

    #[test]
    fn command_projection_feeds_menu_toolbar_keyboard_and_accessibility() {
        let projection = CommandStateProjection::new([
            CommandState::action("file.save", "Save").with_shortcut(MenuShortcut::primary("S"))
        ]);

        let menu_item = projection.menu_item("file.save").expect("menu state");
        assert_eq!(menu_item.id(), Some("file.save"));
        assert_eq!(menu_item.label(), Some("Save"));
        assert!(menu_item.is_enabled());
        assert_eq!(
            projection.toolbar_state("file.save"),
            projection.get("file.save").cloned()
        );
        assert_eq!(
            projection.keyboard_action("file.save"),
            Some(CommandAction::new("file.save", CommandSource::Keyboard))
        );
        assert_eq!(
            projection.accessibility_default_action("file.save"),
            Some(CommandAction::new(
                "file.save",
                CommandSource::Accessibility
            ))
        );

        let no_shortcut = CommandState::action("file.close", "Close");
        assert!(no_shortcut.keyboard_action().is_none());
        assert_eq!(
            no_shortcut.accessibility_default_action(),
            Some(CommandAction::new(
                "file.close",
                CommandSource::Accessibility
            ))
        );
    }

    #[test]
    fn standard_document_commands_share_one_projection() {
        let bar = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        let projection = bar.command_state_projection();
        for id in [
            "file.new",
            "file.open",
            "file.save",
            "file.save_as",
            "edit.undo",
            "edit.redo",
        ] {
            assert!(
                projection.get(id).is_some(),
                "missing projected command {id}"
            );
            assert!(bar.find_item(id).is_some(), "missing menu command {id}");
        }

        let menu_action = bar
            .dispatch_action("file.save", CommandSource::Menu)
            .expect("menu command enabled");
        let keyboard_action = projection
            .dispatch("file.save", CommandSource::Keyboard)
            .expect("keyboard command enabled");
        assert_eq!(menu_action.id, keyboard_action.id);
        assert_eq!(menu_action.source, CommandSource::Menu);
        assert_eq!(keyboard_action.source, CommandSource::Keyboard);
    }

    #[test]
    fn disabled_projection_and_menu_never_dispatch() {
        let disabled = CommandState::action("edit.undo", "Undo").with_enabled(false);
        assert!(disabled.keyboard_action().is_none());
        assert!(disabled.accessibility_default_action().is_none());
        let projection = CommandStateProjection::new([disabled]);
        assert!(projection.keyboard_action("edit.undo").is_none());
        assert!(projection
            .accessibility_default_action("edit.undo")
            .is_none());
        assert!(matches!(
            projection.dispatch("edit.undo", CommandSource::Keyboard),
            Err(DesktopError::InvalidRequest(message)) if message.contains("disabled")
        ));

        let mut bar = MenuBar::new([Menu::new(
            "Edit",
            [MenuItem::Action {
                id: "edit.undo".into(),
                label: "Undo".into(),
                shortcut: None,
                enabled: false,
            }],
        )]);
        assert!(bar
            .find_item("edit.undo")
            .expect("item")
            .keyboard_action()
            .is_none());
        assert!(bar
            .find_item("edit.undo")
            .expect("item")
            .accessibility_default_action()
            .is_none());
        assert!(matches!(
            bar.dispatch_action("edit.undo", CommandSource::Menu),
            Err(DesktopError::InvalidRequest(message)) if message.contains("disabled")
        ));
        projection.apply_to_menu_bar(&mut bar);
        assert!(!bar.find_item("edit.undo").expect("item").is_enabled());
    }

    #[test]
    fn scripted_menu_rejects_disabled_actions_and_records_only_successes() {
        let service = ScriptedMenuBar::new();
        let mut bar = MenuBar::new([Menu::new(
            "File",
            [MenuItem::Action {
                id: "file.save".into(),
                label: "Save".into(),
                shortcut: None,
                enabled: false,
            }],
        )]);
        service.install_menu_bar(&bar).expect("install");
        assert!(service.dispatch_action("file.save").is_err());
        assert!(service.dispatched_actions().is_empty());
        bar.update_item_state("file.save", true, None);
        service.install_menu_bar(&bar).expect("reinstall");
        service
            .dispatch_action_from("file.save", CommandSource::Toolbar)
            .expect("enabled toolbar action");
        assert_eq!(service.dispatched_actions(), vec!["file.save"]);
    }

    #[test]
    fn default_inspector_state_is_closed_and_custom_state_is_not_duplicated() {
        let default_bar = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        match default_bar.find_item("view.inspector") {
            Some(MenuItem::Check { checked, .. }) => assert!(!checked),
            other => panic!("expected default inspector check item, got {other:?}"),
        }

        let custom_bar = build_standard_menu_bar(
            "Loom",
            vec![],
            vec![],
            vec![MenuItem::check("view.inspector", "Inspector", true)],
            vec![],
        );
        let inspector_items = custom_bar
            .menus
            .iter()
            .flat_map(|menu| menu.items.iter())
            .filter(|item| item.id() == Some("view.inspector"))
            .count();
        assert_eq!(inspector_items, 1);
        match custom_bar.find_item("view.inspector") {
            Some(MenuItem::Check { checked, .. }) => assert!(*checked),
            other => panic!("expected custom inspector check item, got {other:?}"),
        }
    }

    #[test]
    fn menu_sync_updates_dynamic_label_shortcut_and_enabled_state() {
        let service = ScriptedMenuBar::new();
        let bar = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        service.install_menu_bar(&bar).expect("install");

        let projection =
            CommandStateProjection::new([CommandState::action("edit.undo", "Undo Typing")
                .with_shortcut(MenuShortcut::primary_shift("Z"))]);
        service
            .sync_command_states(&projection)
            .expect("sync command projection");

        let synchronized = service.installed_bars().pop().expect("installed menu bar");
        match synchronized.find_item("edit.undo") {
            Some(MenuItem::Action {
                label,
                shortcut: Some(shortcut),
                enabled,
                ..
            }) => {
                assert_eq!(label, "Undo Typing");
                assert_eq!(shortcut, &MenuShortcut::primary_shift("Z"));
                assert!(*enabled);
            }
            other => panic!("expected synchronized undo action, got {other:?}"),
        }
    }

    #[test]
    fn menu_sync_rejects_uninstalled_or_unknown_commands() {
        let projection = CommandStateProjection::new([CommandState::action("edit.undo", "Undo")]);
        let service = ScriptedMenuBar::new();
        assert!(matches!(
            service.sync_command_states(&projection),
            Err(DesktopError::InvalidRequest(message))
                if message.contains("not installed")
        ));

        let bar = build_standard_menu_bar("Loom", vec![], vec![], vec![], vec![]);
        service.install_menu_bar(&bar).expect("install");
        let unknown =
            CommandStateProjection::new([CommandState::action("missing.command", "Missing")]);
        assert!(matches!(
            service.sync_command_states(&unknown),
            Err(DesktopError::InvalidRequest(message))
                if message.contains("not present")
        ));
    }

    #[test]
    fn inspector_deduplication_detects_nested_view_entries() {
        let nested = MenuItem::Submenu(Menu::new(
            "More View",
            [MenuItem::Submenu(Menu::new(
                "Deep View",
                [MenuItem::check("view.inspector", "Inspector", true)],
            ))],
        ));
        assert!(nested.contains_id("view.inspector"));
        let bar = build_standard_menu_bar("Loom", vec![], vec![], vec![nested], vec![]);
        fn count_id(items: &[MenuItem], id: &str) -> usize {
            items
                .iter()
                .map(|item| match item {
                    MenuItem::Submenu(submenu) => count_id(&submenu.items, id),
                    _ => usize::from(item.id() == Some(id)),
                })
                .sum()
        }
        let inspector_items = bar
            .menus
            .iter()
            .map(|menu| count_id(&menu.items, "view.inspector"))
            .sum::<usize>();
        assert_eq!(inspector_items, 1);
        assert!(matches!(
            bar.find_item("view.inspector"),
            Some(MenuItem::Check { checked: true, .. })
        ));
    }

    #[test]
    fn duplicate_ids_are_deduplicated_before_projection() {
        let bar = build_standard_menu_bar(
            "Loom",
            vec![MenuItem::action("file.new", "Custom New")],
            vec![MenuItem::action("edit.undo", "Custom Undo")],
            vec![],
            vec![Menu::new(
                "Nested",
                [MenuItem::action("file.new", "Nested New")],
            )],
        );
        let projection = bar.command_state_projection();
        assert_eq!(projection.len(), projection.iter().count());
        assert_eq!(projection.get("file.new").expect("file.new").label, "New");
        assert_eq!(
            bar.find_item("file.new").expect("file.new").label(),
            Some("New")
        );
        let count = bar
            .menus
            .iter()
            .flat_map(|menu| menu.items.iter())
            .filter(|item| item.id() == Some("file.new"))
            .count();
        assert_eq!(count, 1);
    }
}
