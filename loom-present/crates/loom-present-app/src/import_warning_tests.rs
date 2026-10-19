use super::*;
use i_slint_backend_testing::{AccessibleLiveness, AccessibleRole, ElementHandle, ElementRoot};
use slint::{
    platform::{Key, WindowEvent},
    LogicalPosition, PhysicalSize,
};

const PRIOR_RECOVERY: &[u8] = b"prior import-warning recovery";
const WARNING: &str = "This PowerPoint presentation contains elements that Loom Present cannot fully edit:\n\n• SmartArt diagrams will be skipped\n\nDo you want to continue importing?";

fn warning_body(app: &PresentApp) -> String {
    app.get_pptx_import_warning_message().to_string()
}

fn accessible_labels(app: &PresentApp) -> Vec<String> {
    app.root_element()
        .query_descendants()
        .match_predicate(|_| true)
        .find_all()
        .into_iter()
        .filter_map(|element| {
            element.accessible_label().map(|label| {
                format!(
                    "{:?} {:?} {:?}",
                    element.accessible_role(),
                    element.absolute_position(),
                    label
                )
            })
        })
        .collect()
}

fn rect(app: &PresentApp, label: &str) -> (f32, f32, f32, f32) {
    let item = ElementHandle::find_by_accessible_label(app, label)
        .next()
        .unwrap_or_else(|| panic!("missing accessible {label:?}"));
    let position = item.absolute_position();
    let size = item.size();
    (position.x, position.y, size.width, size.height)
}

fn assert_rect_inside_window(rect: (f32, f32, f32, f32), width: f32, height: f32, label: &str) {
    assert!(
        rect.0 >= 0.0 && rect.1 >= 0.0 && rect.0 + rect.2 <= width && rect.1 + rect.3 <= height,
        "{label} must remain visible inside {width}x{height}; rect={rect:?}"
    );
}

fn scroll_warning_to_tail(app: &PresentApp, message: &str) -> f32 {
    let body = ElementHandle::find_by_accessible_label(app, message)
        .next()
        .expect("warning body remains exposed to accessibility while scrolling");
    let position = body.absolute_position();
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(position.x + 4.0, position.y + 4.0),
        delta_x: 0.0,
        delta_y: -6000.0,
    });
    keyboard_flow_tests::render(app);
    let scrolled = ElementHandle::find_by_accessible_label(app, message)
        .next()
        .expect("scrolled warning body remains exposed to accessibility");
    let scrolled_y = scrolled.absolute_position().y;
    assert!(
        scrolled_y < position.y - 40.0,
        "long warning should scroll to its tail: before={}, after={scrolled_y}",
        position.y
    );
    position.y
}

fn assert_warning_at_top(app: &PresentApp, message: &str, top: f32) {
    let body = ElementHandle::find_by_accessible_label(app, message)
        .next()
        .expect("warning body remains exposed to accessibility");
    let y = body.absolute_position().y;
    assert!(
        (y - top).abs() < 1.0,
        "the latest error must be visible at the top after retry: expected y={top}, got y={y}"
    );
}

#[test]
fn failed_import_callbacks_show_one_current_error_above_losses_and_preserve_state() {
    let _state_directory = crate::test_environment::StateDirectoryScratch::new();
    let recovery = keyboard_flow_tests::RecoveryScratch::new_checkpoint(PRIOR_RECOVERY);
    let s = keyboard_flow_tests::launched();
    let original = s.state.session.borrow().clone();
    let candidate_path = recovery.directory().join("lossy-candidate.pptx");
    keyboard_flow_tests::make_unreadable_pending_import(
        candidate_path.clone(),
        b"unreadable recovered draft".to_vec(),
    );
    table_actions::PENDING_PPTX_IMPORT.with(|slot| {
        let mut pending = slot.borrow_mut();
        let pending = pending.as_mut().expect("pending import");
        pending.session.document.title = "Candidate must remain pending".into();
        pending.warnings = (0..48)
            .map(|index| {
                if index == 0 {
                    "SmartArt diagrams will be skipped".to_string()
                } else {
                    format!("Unsupported object {index}: keep this loss reviewable.")
                }
            })
            .collect();
    });

    let kept_directory = startup_recovery::kept_drafts_directory().expect("kept-drafts path");
    std::fs::create_dir_all(kept_directory.parent().expect("kept-drafts parent"))
        .expect("create isolated state parent");
    std::fs::write(&kept_directory, b"force quarantine failure")
        .expect("place a file at the kept-drafts directory path");
    assert!(kept_directory.is_file());

    s.app.set_pptx_import_warning_message(WARNING.into());
    s.app.set_pptx_import_warning_open(true);
    keyboard_flow_tests::render(&s.app);
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Cancel");
    keyboard_flow_tests::press(&s.app, Key::Tab);
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Continue Import");

    keyboard_flow_tests::press(&s.app, Key::Return);
    keyboard_flow_tests::render(&s.app);
    let first_status = s.app.get_status_left().to_string();
    assert!(
        first_status.starts_with("Import not completed:"),
        "{first_status}"
    );
    let first_message = warning_body(&s.app);
    assert!(
        first_message.starts_with(&first_status),
        "the complete Continue error must be at the top of the warning; message={first_message:?}, status={first_status:?}"
    );
    assert!(first_message.contains("SmartArt diagrams will be skipped"));
    assert_eq!(first_message.matches("Import not completed:").count(), 1);
    assert_eq!(
        first_message
            .matches("SmartArt diagrams will be skipped")
            .count(),
        1
    );
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Continue Import");
    let accessible = ElementHandle::find_by_accessible_label(&s.app, &first_message)
        .next()
        .unwrap_or_else(|| {
            panic!(
                "warning body exposes the complete current error to accessibility; visible labels={:?}",
                accessible_labels(&s.app)
            )
        });
    assert_eq!(accessible.accessible_role(), Some(AccessibleRole::Text));
    assert_eq!(
        accessible.accessible_live_region(),
        Some(AccessibleLiveness::Polite),
        "screen readers should receive the updated error through a polite live region"
    );

    let continue_top = scroll_warning_to_tail(&s.app, &first_message);
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Continue Import");
    keyboard_flow_tests::press(&s.app, Key::Return);
    keyboard_flow_tests::render(&s.app);
    let retry_status = s.app.get_status_left().to_string();
    let retry_message = warning_body(&s.app);
    assert!(
        retry_status.starts_with("Import not completed:"),
        "{retry_status}"
    );
    assert_eq!(
        retry_status, first_status,
        "the retry reproduces the same error"
    );
    assert_eq!(
        retry_message, first_message,
        "the error does not accumulate on retry"
    );
    assert!(retry_message.starts_with(&retry_status));
    assert_eq!(retry_message.matches("Import not completed:").count(), 1);
    assert_eq!(
        retry_message
            .matches("SmartArt diagrams will be skipped")
            .count(),
        1
    );
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Continue Import");
    assert_warning_at_top(&s.app, &retry_message, continue_top);

    keyboard_flow_tests::press(&s.app, Key::Escape);
    keyboard_flow_tests::render(&s.app);
    let cancel_status = s.app.get_status_left().to_string();
    let cancel_message = warning_body(&s.app);
    assert!(
        cancel_status.starts_with("Cancel not completed:"),
        "{cancel_status}"
    );
    assert!(cancel_message.starts_with(&cancel_status));
    assert_eq!(cancel_message.matches("Cancel not completed:").count(), 1);
    assert_eq!(
        cancel_message
            .matches("SmartArt diagrams will be skipped")
            .count(),
        1
    );
    assert!(!cancel_message.contains("Import not completed:"));
    assert!(s.app.get_pptx_import_warning_open());
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Continue Import");

    let cancel_top = scroll_warning_to_tail(&s.app, &cancel_message);
    keyboard_flow_tests::press(&s.app, Key::Escape);
    let cancel_retry_status = s.app.get_status_left().to_string();
    let cancel_retry_message = warning_body(&s.app);
    assert_eq!(cancel_retry_status, cancel_status);
    assert_eq!(cancel_retry_message, cancel_message);
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Continue Import");
    assert_warning_at_top(&s.app, &cancel_retry_message, cancel_top);

    let cancel_mouse_top = scroll_warning_to_tail(&s.app, &cancel_retry_message);
    let cancel_button = rect(&s.app, "Cancel");
    let cancel_position = LogicalPosition::new(
        cancel_button.0 + cancel_button.2 / 2.0,
        cancel_button.1 + cancel_button.3 / 2.0,
    );
    let left_button = slint::platform::PointerEventButton::Left;
    s.app.window().dispatch_event(WindowEvent::PointerPressed {
        position: cancel_position,
        button: left_button,
    });
    s.app.window().dispatch_event(WindowEvent::PointerReleased {
        position: cancel_position,
        button: left_button,
    });
    keyboard_flow_tests::render(&s.app);
    assert_eq!(s.app.get_status_left().to_string(), cancel_status);
    assert_eq!(warning_body(&s.app), cancel_message);
    assert_warning_at_top(&s.app, &cancel_message, cancel_mouse_top);
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Cancel");
    assert_eq!(
        table_actions::PENDING_PPTX_IMPORT
            .with(|slot| slot.borrow().as_ref().map(|pending| pending.path.clone())),
        Some(candidate_path),
        "the pending candidate remains available after both failures"
    );
    assert!(crate::deck_guard::content_matches(
        &s.state.session.borrow().document,
        &original.document
    ));
    assert_eq!(s.state.session.borrow().transitions, original.transitions);
    assert_eq!(
        keyboard_flow_tests::read_test_recovery(recovery.directory()),
        Some(PRIOR_RECOVERY.to_vec()),
        "failed Continue and Cancel preserve the existing recovery payload"
    );
}

#[test]
fn long_import_error_scrolls_to_tail_without_moving_footer_at_small_scaled_size() {
    let s = keyboard_flow_tests::launched();
    let (width, height) = (520.0f32, 360.0f32);
    s.app
        .window()
        .set_size(PhysicalSize::new(width as u32, height as u32));
    configure_responsive_width(&s.app, width as u32);
    Theme::get(&s.app).set_text_scale(2.0);
    s.app
        .set_pptx_import_warning_message("Initial PowerPoint warning".into());
    s.app.set_pptx_import_warning_open(true);
    let _ = snapshot_component(&s.app, width, height, 1.0).expect("render initial warning");

    let warnings = (0..48)
        .map(|index| format!("Loss {index}: unsupported item that must remain reviewable."))
        .collect::<Vec<_>>()
        .join("\n");
    let long_message = format!(
        "Import not completed: native recovery error at the top.\n\n{warnings}\n\nTAIL_SENTINEL"
    );
    s.app
        .set_pptx_import_warning_message(long_message.clone().into());
    let _ = snapshot_component(&s.app, width, height, 1.0).expect("render long warning");
    assert_eq!(warning_body(&s.app), long_message);

    let cancel_before = rect(&s.app, "Cancel");
    let continue_before = rect(&s.app, "Continue Import");
    let title_before = rect(&s.app, "PowerPoint Import Notice");
    assert_rect_inside_window(cancel_before, width, height, "Cancel");
    assert_rect_inside_window(continue_before, width, height, "Continue Import");
    assert_rect_inside_window(title_before, width, height, "dialog title");
    let message_before = ElementHandle::find_by_accessible_label(&s.app, &long_message)
        .next()
        .unwrap_or_else(|| {
            panic!(
                "long warning body is exposed to accessibility; visible labels={:?}",
                accessible_labels(&s.app)
            )
        });
    assert_eq!(message_before.accessible_role(), Some(AccessibleRole::Text));
    let body_before = message_before.absolute_position();
    assert!(
        title_before.1 + title_before.3 <= body_before.y + 1.0,
        "dialog title must remain above the scrollable warning body: title={title_before:?}, body={body_before:?}"
    );
    assert!(
        body_before.y < continue_before.1,
        "body must sit above footer"
    );
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Cancel");

    s.app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(width / 2.0, body_before.y + 8.0),
        delta_x: 0.0,
        delta_y: -6000.0,
    });
    let _ = snapshot_component(&s.app, width, height, 1.0).expect("render scrolled warning");

    let message_after = ElementHandle::find_by_accessible_label(&s.app, &long_message)
        .next()
        .expect("scrolled warning body remains accessible");
    let body_after = message_after.absolute_position();
    let body_size = message_after.size();
    assert!(
        body_after.y < body_before.y - 40.0,
        "body should scroll to its tail"
    );
    let cancel_after = rect(&s.app, "Cancel");
    let continue_after = rect(&s.app, "Continue Import");
    let title_after = rect(&s.app, "PowerPoint Import Notice");
    assert_rect_inside_window(cancel_after, width, height, "Cancel");
    assert_rect_inside_window(continue_after, width, height, "Continue Import");
    assert_rect_inside_window(title_after, width, height, "dialog title");
    assert!((cancel_after.1 - cancel_before.1).abs() < 1.0);
    assert!((continue_after.1 - continue_before.1).abs() < 1.0);
    assert!((title_after.1 - title_before.1).abs() < 1.0);
    assert!(
        body_after.y + body_size.height <= continue_after.1 + 1.0,
        "scrolled body tail must end above the fixed buttons: body={body_after:?} size={body_size:?}, footer={continue_after:?}"
    );
    assert_eq!(keyboard_flow_tests::focus_name(&s.app), "Cancel");
}
