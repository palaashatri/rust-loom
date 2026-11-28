//! Window frame for Writer: the platform's own title bar, or Loom's title bar
//! (View > Window frame). The Slint side draws Loom's title bar, caption
//! buttons, and resize edges; the shared bindings carry out the window
//! operations they ask for and remember the choice.

use crate::WriterApp;

loom_desktop::window_chrome_bindings!(WriterApp, set_status_left);

pub(crate) fn init_macos_polish(app: &WriterApp) {
    set_macos_app_icon();
    sync_shortcut_labels(app);
}

pub(crate) fn set_macos_app_icon() {
    loom_desktop::macos_window::set_app_icon(include_bytes!(
        "../../../../loom-core/crates/loom-ui/ui/app-icons/loom-writer.512.png"
    ));
}

pub(crate) fn sync_shortcut_labels(app: &WriterApp) {
    use slint::ComponentHandle;
    app.set_shortcut_modifier(crate::command_dispatch::PRIMARY_MOD.into());
    let mod_key = if cfg!(target_os = "macos") {
        "Cmd+"
    } else {
        "Ctrl+"
    };
    app.global::<crate::WelcomeGuideState>()
        .set_mod_key(mod_key.into());
}

/// The two window-frame commands for Writer's command registry, so the View
/// menu and the palette find them. Loom's own title bar is disabled where the
/// platform cannot draw it.
pub(crate) fn command_specs() -> Vec<loom_command::CommandSpec> {
    use loom_desktop::window_frame::{WindowFrame, SUPPORTED};
    WindowFrame::ALL
        .into_iter()
        .zip(40u32..)
        .map(|(choice, order)| {
            loom_command::CommandSpec::new(choice.command_id(), choice.label())
                .with_undo_label(choice.label())
                .with_description("Choose the system title bar or Loom's own title bar")
                .with_category("view")
                .with_radio_group("window-frame")
                .with_order(order)
                .with_enabled(SUPPORTED || choice == WindowFrame::System)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};
    use i_slint_backend_testing::{AccessibleRole, ElementHandle};
    use loom_test_support::capture::snapshot_component;
    use slint::ComponentHandle;

    #[test]
    fn toolbar_menu_shortcut_labels_match_platform_and_never_show_ctrl_alt_on_macos() {
        let dialogs: std::rc::Rc<dyn loom_desktop::FileDialogService> =
            std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(text_document("hello"), dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        sync_shortcut_labels(&app);
        crate::apply_state(&app, &state);
        app.window().set_size(slint::PhysicalSize::new(1280, 800));
        crate::apply_layout_breakpoints(&app, 1280);

        let expected_mod = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let expected_alt = if cfg!(target_os = "macos") {
            "Opt"
        } else {
            "Alt"
        };
        assert_eq!(app.get_shortcut_modifier().as_str(), expected_mod);
        assert_eq!(
            app.global::<crate::WelcomeGuideState>()
                .get_mod_key()
                .as_str(),
            format!("{expected_mod}+")
        );

        let row_shortcut = |kind: i32, label: &str| -> String {
            app.set_toolbar_menu(kind);
            let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render menu");
            let row = ElementHandle::find_by_accessible_label(&app, label)
                .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
                .unwrap_or_else(|| panic!("menu kind {kind} missing row {label:?}"));
            row.accessible_description()
                .map(|s| s.to_string())
                .unwrap_or_default()
        };

        assert_eq!(
            row_shortcut(3, "Page Break"),
            format!("{expected_mod}+Enter")
        );
        assert_eq!(
            row_shortcut(3, "Headers & Footers\u{2026}"),
            format!("{expected_mod}+{expected_alt}+H")
        );
        assert_eq!(
            row_shortcut(1, "Format Inspector"),
            format!("{expected_mod}+{expected_alt}+I")
        );
        assert_eq!(
            row_shortcut(5, "Heading 1"),
            format!("{expected_mod}+{expected_alt}+1")
        );

        if cfg!(target_os = "macos") {
            let menu = ElementHandle::find_by_element_type_name(&app, "WriterToolbarMenu")
                .next()
                .expect("WriterToolbarMenu element");
            for kind in [1, 2, 3, 4, 5, 6, 7, 8] {
                app.set_toolbar_menu(kind);
                let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render menu");
                let rows = menu
                    .query_descendants()
                    .match_accessible_role(AccessibleRole::ListItem)
                    .find_all();
                assert!(!rows.is_empty(), "toolbar menu kind {kind} has rows");
                for row in rows {
                    if let Some(desc) = row.accessible_description() {
                        assert!(
                            !desc.contains("Ctrl+Alt") && !desc.contains("Ctrl+"),
                            "macOS toolbar menu kind {kind} leaked non-macOS shortcut {desc:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn table_format_inspector_uses_text_table_subtab_without_stacking() {
        let dialogs: std::rc::Rc<dyn loom_desktop::FileDialogService> =
            std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
        let mut doc = loom_writer_core::WriterDocument::new("tbl", "Table Test");
        doc.blocks = vec![loom_writer_core::markdown_table_block(
            1,
            "| A | B |\n| - | - |\n| 1 | 2 |",
        )];
        let (app, state) = test_state(doc, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::wire_writer_inspector_toggle(&app, &state, None);
        app.window().set_size(slint::PhysicalSize::new(1440, 900));
        crate::apply_layout_breakpoints(&app, 1440);
        crate::toolbar_commands::start_with_inspector_open(&app, 1440);
        crate::apply_state(&app, &state);
        app.invoke_selection_changed(0, 0);
        let _ = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render table inspector");

        assert!(app.get_is_table_selected(), "caret should be inside table");

        // Table sub-tab is active by default when inside a table: table controls visible, text heading control hidden.
        assert!(
            ElementHandle::find_by_accessible_label(&app, "1st Col")
                .next()
                .is_some(),
            "Table sub-tab exposes 1st Col toggle"
        );
        assert!(
            ElementHandle::find_by_accessible_label(&app, "Heading Style")
                .next()
                .is_none(),
            "Text Style controls do not stack underneath Table controls"
        );

        // Switching the Format Scope sub-tab to Text exposes Text controls and hides Table controls.
        let text_tab = ElementHandle::find_by_accessible_label(&app, "Text")
            .find(|e| {
                e.accessible_role() == Some(AccessibleRole::Button)
                    && e.absolute_position().x > 1000.0
            })
            .expect("Format Scope segmented control has a Text button inside the inspector");
        text_tab.invoke_accessible_default_action();
        let _ = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render text subtab");

        assert!(
            ElementHandle::find_by_accessible_label(&app, "Heading Style")
                .next()
                .is_some(),
            "Text sub-tab exposes Text Style controls"
        );
        assert!(
            ElementHandle::find_by_accessible_label(&app, "1st Col")
                .next()
                .is_none(),
            "Table controls hide when Text sub-tab is active"
        );
    }
}
