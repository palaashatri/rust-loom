//! Window frame for Writer: the platform's own title bar, or Loom's title bar
//! (View > Window frame). The Slint side draws Loom's title bar, caption
//! buttons, and resize edges; the shared bindings carry out the window
//! operations they ask for and remember the choice.

use crate::WriterApp;

loom_desktop::window_chrome_bindings!(WriterApp, set_status_left);

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
