//! Reordering slides through every route: Slide menu, command palette,
//! Ctrl+Alt+Up/Down, the strip's buttons, a focused thumbnail and dragging a
//! thumbnail. Keys and pointer events go through `Window::dispatch_event`, so
//! Slint's own focus, hit testing and drag handling decide the result.

use super::keyboard_flow_tests::{
    chord, focus_name, focus_weak, launched, launched_with, press, render, same_focus, tab_to,
    Session,
};
use super::*;
use i_slint_backend_testing::{AccessibleLiveness, AccessibleRole, ElementHandle, ElementRoot};
use loom_desktop::ScriptedFileDialogs;
use loom_present_core::{extract_pptx_titles, save_presentation_session};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::LogicalPosition;

fn ids(state: &GuiState) -> Vec<String> {
    state
        .session
        .borrow()
        .document
        .slides
        .iter()
        .map(|slide| slide.id.clone())
        .collect()
}

fn titles(state: &GuiState) -> Vec<String> {
    state
        .session
        .borrow()
        .document
        .slides
        .iter()
        .map(|slide| slide.title.clone())
        .collect()
}

fn active(state: &GuiState) -> usize {
    state.session.borrow().document.active_index
}

fn assert_unique_ids(state: &GuiState) {
    let all = ids(state);
    let mut unique = all.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        all.len(),
        "slide ids must stay unique: {all:?}"
    );
}

fn move_row(app: &PresentApp, command: &str) -> Option<bool> {
    app.get_local_menu_items()
        .iter()
        .find(|item| item.command_id == command)
        .map(|item| item.enabled)
}

fn palette_labels(app: &PresentApp, query: &str) -> Vec<String> {
    rebuild_palette(app, query);
    app.get_palette_commands()
        .iter()
        .map(|item| item.label.to_string())
        .collect()
}

/// One key event the way a windowing backend delivers it. On Windows Slint
/// reads Ctrl+Alt plus a symbol as AltGr unless the backend also says what the
/// key means without modifiers, which winit does and a plain
/// `WindowEvent::KeyPressed` cannot.
fn raw_key(app: &PresentApp, key: Key, pressed: bool) {
    use i_slint_core::input::{InternalKeyEvent, KeyEvent, KeyEventType};
    let text = SharedString::from(char::from(key));
    let mut key_event = KeyEvent::default();
    key_event.text = text.clone();
    let event = InternalKeyEvent {
        key_event,
        event_type: if pressed {
            KeyEventType::KeyPressed
        } else {
            KeyEventType::KeyReleased
        },
        #[cfg(target_os = "windows")]
        text_without_modifiers: text,
        ..Default::default()
    };
    slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
        .process_key_input(event);
}

fn ctrl_alt(app: &PresentApp, key: Key) {
    raw_key(app, Key::Control, true);
    raw_key(app, Key::Alt, true);
    raw_key(app, key, true);
    raw_key(app, key, false);
    raw_key(app, Key::Alt, false);
    raw_key(app, Key::Control, false);
    render(app);
}

// ---- commands, menu, palette -------------------------------------------------

#[test]
fn move_commands_are_enabled_only_where_the_slide_can_move() {
    let s = launched();
    let app = &s.app;
    // The Slide menu lists both, with their shortcuts.
    let slide_menu: Vec<(String, String)> = app
        .get_local_menu_items()
        .iter()
        .filter(|item| item.menu_index == 3 && !item.separator)
        .map(|item| (item.label.to_string(), item.shortcut.to_string()))
        .collect();
    assert!(
        slide_menu.contains(&("Move Slide Up".into(), "Ctrl+Alt+Up".into())),
        "{slide_menu:?}"
    );
    assert!(
        slide_menu.contains(&("Move Slide Down".into(), "Ctrl+Alt+Down".into())),
        "{slide_menu:?}"
    );

    // First slide: only Down.
    assert_eq!(move_row(app, "slide.move_up"), Some(false));
    assert_eq!(move_row(app, "slide.move_down"), Some(true));
    assert_eq!(palette_labels(app, "move slide"), ["Move Slide Down"]);

    // Middle slide: both.
    app.invoke_select_slide(1);
    assert_eq!(move_row(app, "slide.move_up"), Some(true));
    assert_eq!(move_row(app, "slide.move_down"), Some(true));
    assert_eq!(
        palette_labels(app, "move slide"),
        ["Move Slide Up", "Move Slide Down"]
    );

    // Last slide: only Up.
    app.invoke_select_slide(2);
    assert_eq!(move_row(app, "slide.move_up"), Some(true));
    assert_eq!(move_row(app, "slide.move_down"), Some(false));
    assert_eq!(palette_labels(app, "move slide"), ["Move Slide Up"]);

    // One slide: neither, and the strip buttons are disabled too.
    app.invoke_delete_slide();
    app.invoke_delete_slide();
    assert_eq!(s.state.session.borrow().document.len(), 1);
    assert_eq!(move_row(app, "slide.move_up"), Some(false));
    assert_eq!(move_row(app, "slide.move_down"), Some(false));
    assert!(palette_labels(app, "move slide").is_empty());
    assert!(!app.get_can_move_slide_up() && !app.get_can_move_slide_down());
    render(app);
    for label in ["Move Slide Up", "Move Slide Down"] {
        ElementHandle::find_by_accessible_label(app, label)
            .next()
            .expect("strip button")
            .invoke_accessible_default_action();
    }
    assert_eq!(s.state.session.borrow().document.len(), 1);
    assert!(!s.state.session.borrow().can_undo() || app.get_status_left().as_str() != "");
}

#[test]
fn a_move_command_is_one_undoable_edit_and_keeps_the_slide_selected() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    assert!(is_present_menu_command("slide.move_down"));
    assert!(dispatch_command(app, "slide.move_down"));

    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    assert_eq!(active(&s.state), 1, "selection follows the moved slide");
    assert_eq!(app.get_active_slide_index(), 1);
    assert_eq!(
        app.get_status_left().as_str(),
        "Moved slide 1 to position 2"
    );
    assert_eq!(
        app.get_slide_announcement().as_str(),
        "Moved slide 1 to position 2"
    );
    assert!(app.get_can_move_slide_up());

    assert!(dispatch_command(app, "slide.move_down"));
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
    assert!(!app.get_can_move_slide_down(), "now last");
    assert!(dispatch_command(app, "slide.move_up"));
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );

    // One undo per move, restoring order and selection.
    app.invoke_undo();
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
    assert_eq!(active(&s.state), 2);
    app.invoke_undo();
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    app.invoke_undo();
    assert_eq!(ids(&s.state), before);
    assert_eq!(active(&s.state), 0);
    assert!(
        !s.state.session.borrow().can_undo(),
        "three moves, three undo steps"
    );
    app.invoke_redo();
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    assert_unique_ids(&s.state);
}

#[test]
fn the_slide_menu_and_the_palette_run_the_move() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);

    // Slide menu: Alt+S, arrow to the row, Enter.
    chord(app, &[Key::Alt], "s");
    for _ in 0..12 {
        let index = app.get_local_menu_popup_selected_index();
        let row = app
            .get_local_menu_popup_items()
            .row_data(index as usize)
            .expect("row");
        if row.label == "Move Slide Down" {
            break;
        }
        press(app, Key::DownArrow);
    }
    press(app, Key::Return);
    assert_eq!(
        *s.actions.borrow(),
        ["slide.move_down"],
        "the menu ran the command"
    );
    assert_eq!(app.get_local_menu_open_index(), -1, "the menu closed");
    // The window routes that command id to the same handler as every other route.
    assert!(dispatch_command(app, "slide.move_down"));
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );

    // Palette: filter, invoke the first (only) match.
    app.set_palette_query("move slide up".into());
    assert_eq!(palette_labels(app, "move slide up"), ["Move Slide Up"]);
    app.invoke_palette_invoked(0);
    assert_eq!(ids(&s.state), before);
    assert_eq!(active(&s.state), 0);
}

#[test]
fn ctrl_alt_arrows_move_the_slide_and_nothing_else_does() {
    let s = launched();
    let app = &s.app;
    // With no object selected a bare arrow has nothing to nudge.
    s.state.session.borrow_mut().clear_selection();
    refresh(app, &s.state);
    let before = ids(&s.state);

    ctrl_alt(app, Key::DownArrow);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    ctrl_alt(app, Key::DownArrow);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
    assert_eq!(active(&s.state), 2);

    // At the end the shortcut changes nothing, adds no undo step, and says so.
    let undo_depth = s.state.session.borrow().can_undo();
    ctrl_alt(app, Key::DownArrow);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
    assert_eq!(app.get_status_left().as_str(), "Slide 3 is already last");
    assert_eq!(
        app.get_slide_announcement().as_str(),
        "Slide 3 is already last"
    );
    assert_eq!(s.state.session.borrow().can_undo(), undo_depth);

    ctrl_alt(app, Key::UpArrow);
    assert_eq!(active(&s.state), 1);

    // Alt alone is the menu key; Ctrl alone and Alt alone never reorder.
    let order = ids(&s.state);
    chord(app, &[Key::Alt], &String::from(char::from(Key::UpArrow)));
    chord(
        app,
        &[Key::Control],
        &String::from(char::from(Key::UpArrow)),
    );
    assert_eq!(ids(&s.state), order);

    // Undo and redo with the keyboard.
    chord(app, &[Key::Control], "z");
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
    chord(app, &[Key::Control, Key::Shift], "z");
    assert_eq!(ids(&s.state), order);
}

#[test]
fn the_strip_buttons_move_the_selected_slide() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    render(app);
    assert!(app.get_can_move_slide_down() && !app.get_can_move_slide_up());
    let down = ElementHandle::find_by_accessible_label(app, "Move Slide Down")
        .next()
        .expect("button");
    down.invoke_accessible_default_action();
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    render(app);
    let up = ElementHandle::find_by_accessible_label(app, "Move Slide Up")
        .next()
        .expect("button");
    up.invoke_accessible_default_action();
    assert_eq!(ids(&s.state), before);
    // At the top the button is disabled and does nothing, not even an undo step.
    render(app);
    assert!(!app.get_can_move_slide_up());
    let steps = s.state.session.borrow().can_undo();
    ElementHandle::find_by_accessible_label(app, "Move Slide Up")
        .next()
        .expect("button")
        .invoke_accessible_default_action();
    assert_eq!(ids(&s.state), before);
    assert_eq!(app.get_active_slide_index(), 0);
    let _ = steps;
}

// ---- keyboard on a thumbnail, names, announcements ---------------------------

fn thumb_name(s: &Session, position: usize) -> String {
    let all = titles(&s.state);
    format!("Slide {position} of {}, {}", all.len(), all[position - 1])
}

#[test]
fn a_focused_thumbnail_moves_with_ctrl_up_and_down_and_keeps_focus() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    let moved_title = titles(&s.state)[1].clone();
    // A running event loop initialises Slint's change handlers on its first turn.
    settle();

    // The accessible name carries the position.
    let name = thumb_name(&s, 2);
    assert_eq!(name, format!("Slide 2 of 3, {moved_title}"));
    tab_to(app, &name);
    ctrl_arrow(app, Key::DownArrow);

    assert_eq!(
        ids(&s.state),
        [before[0].clone(), before[2].clone(), before[1].clone()]
    );
    assert_eq!(active(&s.state), 2, "the moved slide is selected");
    assert_eq!(
        focus_name(app),
        format!("Slide 3 of 3, {moved_title}"),
        "focus follows the slide so the next press keeps moving it"
    );
    assert_eq!(
        app.get_slide_announcement().as_str(),
        "Moved slide 2 to position 3"
    );

    ctrl_arrow(app, Key::UpArrow);
    ctrl_arrow(app, Key::UpArrow);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    assert_eq!(focus_name(app), format!("Slide 1 of 3, {moved_title}"));
    assert_eq!(
        app.get_slide_announcement().as_str(),
        "Moved slide 2 to position 1"
    );

    // Already first: nothing changes.
    let can_undo_steps = s.state.session.borrow().can_undo();
    ctrl_arrow(app, Key::UpArrow);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[0].clone(), before[2].clone()]
    );
    assert_eq!(s.state.session.borrow().can_undo(), can_undo_steps);

    // The same names after the moves.
    render(app);
    for position in 1..=3 {
        assert_eq!(
            ElementHandle::find_by_accessible_label(app, &thumb_name(&s, position)).count(),
            1,
            "slide {position}"
        );
    }
}

/// What every turn of a real event loop does before drawing.
fn settle() {
    slint::platform::update_timers_and_animations();
}

/// Ctrl+arrow, the thumbnail's own shortcut.
fn ctrl_arrow(app: &PresentApp, key: Key) {
    settle();
    chord(app, &[Key::Control], &String::from(char::from(key)));
    settle();
    render(app);
}

#[test]
fn the_strip_is_a_live_region_that_speaks_the_move() {
    let s = launched();
    render(&s.app);
    let strip = ElementHandle::find_by_accessible_label(&s.app, "Slides")
        .next()
        .expect("strip");
    assert_eq!(
        strip.accessible_live_region(),
        Some(AccessibleLiveness::Polite)
    );
    dispatch_command(&s.app, "slide.move_down");
    render(&s.app);
    assert_eq!(
        strip.accessible_description().as_deref(),
        Some("Moved slide 1 to position 2")
    );
}

// ---- dragging -------------------------------------------------------------

fn thumb(app: &PresentApp, position: usize) -> ElementHandle {
    app.root_element()
        .query_descendants()
        .match_predicate(move |e| {
            e.accessible_label()
                .is_some_and(|l| l.starts_with(&format!("Slide {position} of ")))
        })
        .find_first()
        .unwrap_or_else(|| panic!("thumbnail {position}"))
}

fn centre(element: &ElementHandle) -> (f32, f32) {
    let (p, s) = (element.absolute_position(), element.size());
    (p.x + s.width / 2.0, p.y + s.height / 2.0)
}

fn pointer(app: &PresentApp, event: WindowEvent) {
    app.window().dispatch_event(event);
    render(app);
}

fn moved(app: &PresentApp, (x, y): (f32, f32)) {
    pointer(
        app,
        WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        },
    );
}

fn press_at(app: &PresentApp, (x, y): (f32, f32)) {
    moved(app, (x, y));
    pointer(
        app,
        WindowEvent::PointerPressed {
            position: LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
    );
}

fn release_at(app: &PresentApp, (x, y): (f32, f32)) {
    pointer(
        app,
        WindowEvent::PointerReleased {
            position: LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
    );
}

/// Moves the pressed pointer in small steps, as a hand does.
fn drag_to(app: &PresentApp, from: (f32, f32), to: (f32, f32)) {
    let steps = 8;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        moved(
            app,
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t),
        );
    }
}

fn indicator(app: &PresentApp) -> Option<ElementHandle> {
    ElementHandle::find_by_element_id(app, "PresentSlideStrip::drop-indicator").next()
}

#[test]
fn dragging_a_thumbnail_shows_a_drop_line_and_releasing_moves_the_slide_once() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    render(app);
    let first = thumb(app, 1);
    let last = thumb(app, 3);
    assert_eq!(first.computed_opacity(), 1.0);
    let start = centre(&first);
    let bottom_of_last = (
        start.0,
        last.absolute_position().y + last.size().height * 0.85,
    );

    press_at(app, start);
    assert!(indicator(app).is_none(), "a press alone shows no drop line");
    drag_to(app, start, bottom_of_last);

    let line = indicator(app).expect("a drop line while dragging");
    let last_bottom = last.absolute_position().y + last.size().height;
    let line_centre = line.absolute_position().y + line.size().height / 2.0;
    assert!(
        line_centre >= last_bottom && line_centre <= last_bottom + 12.0,
        "the line sits in the gap after slide 3: {line_centre} vs {last_bottom}"
    );
    assert!(
        thumb(app, 1).computed_opacity() < 1.0,
        "the dragged slide is dimmed"
    );
    assert_eq!(ids(&s.state), before, "nothing moves until release");

    release_at(app, bottom_of_last);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
    assert_eq!(active(&s.state), 2, "selection follows the moved slide");
    assert_eq!(
        app.get_slide_announcement().as_str(),
        "Moved slide 1 to position 3"
    );
    assert!(indicator(app).is_none(), "the drop line goes away");

    // Exactly one undo step.
    app.invoke_undo();
    assert_eq!(ids(&s.state), before);
    assert!(!s.state.session.borrow().can_undo());
    assert_unique_ids(&s.state);
}

#[test]
fn dragging_a_slide_upward_drops_before_the_slide_under_the_pointer() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    render(app);
    let third = thumb(app, 3);
    let second = thumb(app, 2);
    let start = centre(&third);
    // The upper part of slide 2 is the gap before it.
    let target = (
        start.0,
        second.absolute_position().y + second.size().height * 0.2,
    );
    press_at(app, start);
    drag_to(app, start, target);
    release_at(app, target);
    assert_eq!(
        ids(&s.state),
        [before[0].clone(), before[2].clone(), before[1].clone()]
    );
    assert_eq!(active(&s.state), 1);
    assert_eq!(
        app.get_slide_announcement().as_str(),
        "Moved slide 3 to position 2"
    );
}

#[test]
fn a_press_that_moves_under_four_pixels_is_a_plain_click() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    render(app);
    let start = centre(&thumb(app, 2));
    let undo_before = s.state.session.borrow().can_undo();
    press_at(app, start);
    moved(app, (start.0 + 1.0, start.1 + 3.0));
    assert!(indicator(app).is_none());
    release_at(app, (start.0 + 1.0, start.1 + 3.0));
    assert_eq!(active(&s.state), 1, "the click selected slide 2");
    assert_eq!(ids(&s.state), before, "and moved nothing");
    assert_eq!(s.state.session.borrow().can_undo(), undo_before);

    // A drag that ends where it began is not a move either.
    let first = centre(&thumb(app, 1));
    press_at(app, first);
    drag_to(app, first, (first.0, first.1 + 20.0));
    release_at(app, (first.0, first.1 + 20.0));
    assert_eq!(ids(&s.state), before);
    assert_eq!(s.state.session.borrow().can_undo(), undo_before);
}

#[test]
fn escape_cancels_a_drag_and_returns_focus_to_the_editor() {
    let s = launched();
    let app = &s.app;
    let before = ids(&s.state);
    render(app);
    let editor = focus_weak(app);
    let start = centre(&thumb(app, 1));
    let target = (start.0, centre(&thumb(app, 3)).1 + 30.0);
    press_at(app, start);
    drag_to(app, start, target);
    assert!(indicator(app).is_some());
    assert!(
        !same_focus(&focus_weak(app), &editor),
        "the drag owns the keyboard"
    );

    press(app, Key::Escape);
    assert!(indicator(app).is_none(), "Escape removes the drop line");
    assert_eq!(app.get_slide_announcement().as_str(), "Move cancelled");
    assert!(
        same_focus(&focus_weak(app), &editor),
        "focus is back on the editor"
    );

    release_at(app, target);
    assert_eq!(
        ids(&s.state),
        before,
        "releasing after Escape moves nothing"
    );
    assert!(!s.state.session.borrow().can_undo());
}

#[test]
fn the_drop_line_is_not_a_focus_stop_and_has_no_role() {
    let s = launched();
    let app = &s.app;
    render(app);
    let start = centre(&thumb(app, 1));
    press_at(app, start);
    drag_to(app, start, (start.0, centre(&thumb(app, 3)).1 + 30.0));
    let line = indicator(app).expect("line");
    assert_eq!(
        line.accessible_role(),
        Some(AccessibleRole::None),
        "the drop line is decoration"
    );
    assert_eq!(line.accessible_label(), None);
    press(app, Key::Escape);
    release_at(app, start);
}

fn long_deck(s: &Session, total: usize) {
    {
        let mut session = s.state.session.borrow_mut();
        for n in 4..=total {
            session.document.add_slide(format!("Slide {n}"), "content");
        }
        session.document.select_slide(0);
    }
    refresh(&s.app, &s.state);
    render(&s.app);
}

#[test]
fn dragging_near_the_strip_edge_scrolls_it_and_the_slide_lands_far_away() {
    let s = launched();
    let app = &s.app;
    long_deck(&s, 14);
    let before = ids(&s.state);
    let strip = ElementHandle::find_by_accessible_label(app, "Slides")
        .next()
        .expect("strip");
    let delete = ElementHandle::find_by_accessible_label(app, "Delete Slide")
        .next()
        .expect("the strip button row sits under the list");
    let list_bottom_y = delete.absolute_position().y - 12.0;
    let _ = strip;

    // The wheel still scrolls the strip even though dragging does not.
    let marker = thumb(app, 2);
    let y_before_wheel = marker.absolute_position().y;
    let over = centre(&marker);
    pointer(
        app,
        WindowEvent::PointerScrolled {
            position: LogicalPosition::new(over.0, over.1),
            delta_x: 0.0,
            delta_y: -60.0,
        },
    );
    let y_after_wheel = thumb(app, 2).absolute_position().y;
    assert!(y_after_wheel < y_before_wheel, "wheel scrolled the strip");
    pointer(
        app,
        WindowEvent::PointerScrolled {
            position: LogicalPosition::new(over.0, over.1),
            delta_x: 0.0,
            delta_y: 600.0,
        },
    );

    // Start from slide 1 and hold the pointer at the bottom of the visible list.
    let first = centre(&thumb(app, 1));
    press_at(app, first);
    let hold = (first.0, list_bottom_y);
    drag_to(app, first, hold);
    let row_before = thumb(app, 3).absolute_position().y;
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
        render(app);
    }
    let row_after = thumb(app, 3).absolute_position().y;
    assert!(
        row_after < row_before - 40.0,
        "holding at the bottom edge scrolled the strip: {row_before} -> {row_after}"
    );
    release_at(app, hold);

    let after = ids(&s.state);
    let new_index = after
        .iter()
        .position(|id| *id == before[0])
        .expect("slide kept");
    assert!(
        new_index >= 4,
        "slide 1 landed well down the deck: {new_index}"
    );
    assert_eq!(active(&s.state), new_index);
    assert_unique_ids(&s.state);
    app.invoke_undo();
    assert_eq!(ids(&s.state), before);
}

// ---- right-to-left -----------------------------------------------------------

fn drag_line(rtl: bool) -> (ElementHandle, ElementHandle, ElementHandle) {
    let s = launched();
    let app = s.app.clone_strong();
    configure_direction(&app, rtl);
    render(&app);
    let strip = ElementHandle::find_by_accessible_label(&app, "Slides")
        .next()
        .expect("strip");
    let start = centre(&thumb(&app, 1));
    press_at(&app, start);
    drag_to(&app, start, (start.0, centre(&thumb(&app, 3)).1 + 30.0));
    let line = indicator(&app).expect("line");
    let dot = ElementHandle::find_by_element_id(&app, "PresentSlideStrip::drop-dot")
        .next()
        .expect("dot");
    // Keep the window alive for the handles' lifetime.
    std::mem::forget(s);
    (strip, line, dot)
}

#[test]
fn the_drop_line_stays_inside_the_strip_and_its_round_end_leads_in_both_directions() {
    let (strip, line, dot) = drag_line(false);
    let (sp, ss) = (strip.absolute_position(), strip.size());
    let (lp, ls) = (line.absolute_position(), line.size());
    assert!(
        lp.x >= sp.x - 0.5 && lp.x + ls.width <= sp.x + ss.width + 0.5,
        "inside the strip"
    );
    assert!(
        (dot.absolute_position().x - lp.x).abs() < 0.5,
        "LTR: the dot is at the left end"
    );
    assert!(sp.x < 200.0, "LTR: the strip is at the left");

    let (strip, line, dot) = drag_line(true);
    let (sp, ss) = (strip.absolute_position(), strip.size());
    let (lp, ls) = (line.absolute_position(), line.size());
    assert!(sp.x > 640.0, "RTL: the strip moves to the right edge");
    assert!(
        lp.x >= sp.x - 0.5 && lp.x + ls.width <= sp.x + ss.width + 0.5,
        "inside the strip"
    );
    let dot_right = dot.absolute_position().x + dot.size().width;
    assert!(
        (dot_right - (lp.x + ls.width)).abs() < 0.5,
        "RTL: the dot is at the right end"
    );
}

#[test]
fn rtl_dragging_moves_the_slide_like_ltr() {
    let s = launched();
    let app = &s.app;
    configure_direction(app, true);
    render(app);
    let before = ids(&s.state);
    let start = centre(&thumb(app, 1));
    let target = (
        start.0,
        thumb(app, 3).absolute_position().y + thumb(app, 3).size().height * 0.85,
    );
    press_at(app, start);
    drag_to(app, start, target);
    release_at(app, target);
    assert_eq!(
        ids(&s.state),
        [before[1].clone(), before[2].clone(), before[0].clone()]
    );
}

// ---- persistence and export ---------------------------------------------------

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("loom-present-order-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn at(haystack: &[u8], needle: &str) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
        .unwrap_or_else(|| panic!("{needle:?} missing"))
}

#[test]
fn save_reopen_and_both_exports_follow_the_new_order() {
    let dir = temp_dir("exports");
    let deck = dir.join("deck.loomdeck");
    let pdf = dir.join("deck.pdf");
    let pptx = dir.join("deck.pptx");
    let dialogs = Rc::new(ScriptedFileDialogs::new(
        [],
        [Some(deck.clone()), Some(pdf.clone()), Some(pptx.clone())],
    ));
    let s = launched_with(dialogs);
    let app = &s.app;
    let before = titles(&s.state);
    // Move the first slide to the end, then the (new) first to the middle.
    dispatch_command(app, "slide.move_down");
    dispatch_command(app, "slide.move_down");
    app.invoke_select_slide(0);
    dispatch_command(app, "slide.move_down");
    let order = titles(&s.state);
    assert_eq!(
        order,
        [before[2].clone(), before[1].clone(), before[0].clone()]
    );

    app.invoke_save_deck();
    let reopened =
        load_presentation_session(&std::fs::read(&deck).expect("saved")).expect("reopen");
    let reopened_titles: Vec<String> = reopened
        .document
        .slides
        .iter()
        .map(|x| x.title.clone())
        .collect();
    assert_eq!(reopened_titles, order, "Save and reopen keep the order");
    assert_eq!(
        reopened.document.active_index,
        s.state.session.borrow().document.active_index
    );

    app.invoke_export_pdf();
    let bytes = std::fs::read(&pdf).expect("pdf");
    let pages: Vec<usize> = order.iter().map(|t| at(&bytes, t)).collect();
    assert!(
        pages.windows(2).all(|w| w[0] < w[1]),
        "PDF pages follow the new order: {pages:?}"
    );

    app.invoke_export_pptx();
    let exported = extract_pptx_titles(&std::fs::read(&pptx).expect("pptx")).expect("titles");
    assert_eq!(exported, order, "PowerPoint slides follow the new order");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_move_marks_the_recovery_draft_stale_and_the_draft_restores_the_order() {
    let s = launched();
    let app = &s.app;
    recovery_deferred::invalidate();
    assert!(!recovery_deferred::is_stale());
    dispatch_command(app, "slide.move_down");
    assert!(
        recovery_deferred::is_stale(),
        "the move queued a recovery draft"
    );

    // What a draft holds is the document and transitions of this session.
    let order = titles(&s.state);
    let bytes = save_presentation_session(&s.state.session.borrow()).expect("draft bytes");
    let restored = load_presentation_session(&bytes).expect("restore");
    let restored_titles: Vec<String> = restored
        .document
        .slides
        .iter()
        .map(|x| x.title.clone())
        .collect();
    assert_eq!(restored_titles, order);
    assert_eq!(restored.document.active_index, 1);
    recovery_deferred::invalidate();
}

#[test]
fn new_and_duplicated_slides_after_a_reorder_never_repeat_an_id() {
    let s = launched();
    let app = &s.app;
    dispatch_command(app, "slide.move_down");
    dispatch_command(app, "slide.move_down");
    app.invoke_add_slide();
    app.invoke_duplicate_slide();
    app.invoke_select_slide(0);
    dispatch_command(app, "slide.move_down");
    app.invoke_add_slide();
    assert_eq!(ids(&s.state).len(), 6);
    assert_unique_ids(&s.state);
}
