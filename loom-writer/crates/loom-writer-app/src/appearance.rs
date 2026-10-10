//! Writer's appearance choice (System, Light, Dark, High Contrast): the View
//! menu rows, the command-palette entries, the toolbar View menu and the
//! saved setting all go through the shared bindings in `loom-desktop`; this
//! file only names the application and its registry entries.

use loom_command::CommandSpec;
use loom_desktop::Appearance;

use crate::WriterApp;

/// The id under which Writer remembers its settings.
pub(crate) const APPLICATION_ID: &str = "org.loom.writer";

loom_desktop::appearance_bindings!(WriterApp, set_status_left);
loom_desktop::ui_font_bindings!(WriterApp, crate::Theme);

/// The four appearance commands for Writer's command registry, so the palette
/// finds them and every surface shares one command id.
pub(crate) fn command_specs() -> Vec<CommandSpec> {
    Appearance::ALL
        .into_iter()
        .zip(30u32..)
        .map(|(choice, order)| {
            CommandSpec::new(choice.command_id(), choice.label())
                .with_undo_label(choice.label())
                .with_description("Choose the colors of the application window")
                .with_category("view")
                .with_radio_group("appearance")
                .with_order(order)
        })
        .collect()
}
