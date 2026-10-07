//! Keeps the in-window menu's models in step with the menu bar without
//! replacing them. Replacing the label model rebuilds the title bar's menu
//! items, and Slint then drops keyboard focus wherever it was, so a slide
//! thumbnail that had just been moved with Ctrl+Up lost focus on every
//! refresh. Rows are written in place instead, and only when they changed.

use slint::{ModelRc, SharedString};

use loom_desktop::{project_local_menu, DesktopError, NativeMenuBar};

use crate::model_sync::synced;
use crate::{local_menu, LocalMenuEntry, PresentApp};

/// Same result as `local_menu::sync`, applied to the existing models.
pub(crate) fn sync(app: &PresentApp, menu_service: &NativeMenuBar) -> Result<(), DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("the menu bar is not installed".into()))?;
    let (labels, lines) = project_local_menu(&menu_bar, local_menu::SUPPORTED_COMMANDS)?;
    let labels: ModelRc<SharedString> = synced(
        app.get_local_menu_labels(),
        labels.into_iter().map(SharedString::from).collect(),
    );
    let items: ModelRc<LocalMenuEntry> = synced(
        app.get_local_menu_items(),
        lines
            .into_iter()
            .map(|line| LocalMenuEntry {
                menu_index: line.menu_index,
                label: line.label.into(),
                command_id: line.command_id.into(),
                shortcut: line.shortcut.into(),
                enabled: line.enabled,
                checked: line.checked,
                checkable: line.checkable,
                separator: line.separator,
            })
            .collect(),
    );
    app.set_local_menu_labels(labels);
    app.set_local_menu_items(items);
    Ok(())
}
