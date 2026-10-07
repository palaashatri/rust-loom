//! Keeps the in-window menu's models in step with the menu bar without
//! replacing them. Replacing the label model rebuilds the title bar's menu
//! items, and Slint then drops keyboard focus wherever it was, so a slide
//! thumbnail that had just been moved with Ctrl+Up lost focus on every
//! refresh. Rows are written in place instead, and only when they changed.

use slint::{ModelRc, SharedString};

use loom_desktop::{
    project_local_menu, CommandStateProjection, DesktopError, MenuBarService, NativeMenuBar,
};

use crate::model_sync::synced;
use crate::{
    appearance, local_menu, rebuild_palette, set_status, slide_order, view_state, GuiState,
    LocalMenuEntry, PresentApp,
};

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

pub(crate) fn menu_projection(
    menu_service: &NativeMenuBar,
    app: &PresentApp,
    state: &GuiState,
) -> Result<CommandStateProjection, DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu bar is not installed".into()))?;
    let mut projection = menu_bar.command_state_projection();

    let session = state.session.borrow();
    let can_undo = session.can_undo();
    let can_redo = session.can_redo();
    let deck_len = session.document.len();
    let active_index = session.document.active_index;

    let mut undo = projection
        .get("edit.undo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing edit.undo".into()))?;
    undo.enabled = can_undo;
    projection.insert(undo);

    let mut redo = projection
        .get("edit.redo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing edit.redo".into()))?;
    redo.enabled = can_redo;
    projection.insert(redo);

    let mut inspector = projection.get("view.inspector").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Present menu is missing view.inspector".into())
    })?;
    inspector.enabled = state.inspector_available.get() || app.get_show_inspector();
    inspector.checked = Some(app.get_show_inspector());
    projection.insert(inspector);
    view_state::project(&mut projection, app);
    loom_desktop::appearance::project_checks(&mut projection, appearance::current(app));
    slide_order::project(&mut projection, &session);

    let mut slide_delete = projection.get("slide.delete").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Present menu is missing slide.delete".into())
    })?;
    slide_delete.enabled = deck_len > 1;
    projection.insert(slide_delete);

    let mut slide_prev = projection
        .get("slide.prev")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing slide.prev".into()))?;
    slide_prev.enabled = active_index > 0;
    projection.insert(slide_prev);

    let mut slide_next = projection
        .get("slide.next")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Present menu is missing slide.next".into()))?;
    slide_next.enabled = active_index < deck_len.saturating_sub(1);
    projection.insert(slide_next);

    Ok(projection)
}

pub(crate) fn sync_menu_state_result(
    menu_service: &NativeMenuBar,
    app: &PresentApp,
    state: &GuiState,
) -> Result<(), DesktopError> {
    rebuild_palette(app, app.get_palette_query().as_str());
    let projection = menu_projection(menu_service, app, state)?;
    menu_service.sync_command_states(&projection)?;
    sync(app, menu_service)
}

pub(crate) fn sync_menu_state(menu_service: &NativeMenuBar, app: &PresentApp, state: &GuiState) {
    if let Err(error) = sync_menu_state_result(menu_service, app, state) {
        set_status(app, format!("Menu update failed: {error}"));
    }
}
