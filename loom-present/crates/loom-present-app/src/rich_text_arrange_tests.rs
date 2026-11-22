use super::*;
use crate::command_dispatch::dispatch_command;

#[test]
fn rich_text_and_arrange_commands_dispatch_without_panicking() {
    let app = PresentApp::new().expect("create PresentApp");
    for id in [
        "format.bold",
        "format.italic",
        "format.underline",
        "format.strikethrough",
        "format.font_increase",
        "format.font_decrease",
        "arrange.bring_front",
        "arrange.bring_forward",
        "arrange.send_backward",
        "arrange.send_back",
    ] {
        assert!(dispatch_command(&app, id), "failed to dispatch {id}");
    }
}
