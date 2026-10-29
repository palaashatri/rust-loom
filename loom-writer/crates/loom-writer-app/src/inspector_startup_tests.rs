//! Startup's docked inspector must not become an unsolicited compact drawer.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::snapshot_component;

fn launched(width: u32) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document("Hello"), dialogs);
    wire_writer_inspector_toggle(&app, &state, None);
    wire_responsive_layout(&app);
    apply_layout_breakpoints(&app, width);
    toolbar_commands::start_with_inspector_open(&app, width);
    (app, state)
}

fn select_document_section(app: &WriterApp, label: &str, width: f32, height: f32) {
    ElementHandle::find_by_accessible_label(app, label)
        .find(|element| element.accessible_role() == Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("no accessible document section named {label:?}"))
        .invoke_accessible_default_action();
    let _ = snapshot_component(app, width, height, 1.0).expect("render selected document section");
}

#[test]
fn auto_opened_inspector_closes_when_actual_width_is_compact() {
    let (app, _state) = launched(1280);
    assert!(app.get_show_inspector());
    assert!(!app.get_compact_inspector_layout());

    app.invoke_window_resized(1018.0);

    assert!(app.get_compact_inspector_layout());
    assert!(!app.get_show_inspector(), "startup must not cover the page");
}

#[test]
fn user_opened_inspector_survives_compact_resize() {
    for width in [1280, 1024] {
        let (app, _state) = launched(width);
        if app.get_show_inspector() {
            app.invoke_toggle_inspector();
            assert!(!app.get_show_inspector());
        }
        assert!(toolbar_commands::dispatch(&app, "view.inspector"));
        assert!(app.get_show_inspector());

        app.invoke_window_resized(1018.0);

        assert!(app.get_compact_inspector_layout());
        assert!(app.get_show_inspector(), "preserve the user's open drawer");
    }
}

#[test]
fn widening_does_not_reopen_the_startup_inspector() {
    let (app, _state) = launched(1280);
    app.invoke_window_resized(1018.0);
    app.invoke_window_resized(1280.0);

    assert!(!app.get_compact_inspector_layout());
    assert!(!app.get_show_inspector(), "widening must not reopen it");
}

#[test]
fn page_summary_names_the_document_total_without_claiming_a_current_page() {
    let (app, _state) = launched(1280);
    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render Document inspector");
    select_document_section(&app, "Page Setup", 1280.0, 800.0);

    for (page_count, expected) in [(1, "1 page"), (3, "3 pages")] {
        app.set_page_count(page_count);
        let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render inspector");
        assert!(
            ElementHandle::find_by_accessible_label(&app, expected).count() >= 1,
            "the visible and accessible document summary should say {expected:?}"
        );
        assert_eq!(
            ElementHandle::find_by_accessible_label(&app, "Page 1 of 3").count(),
            0,
            "the summary must not claim the caret is on page 1"
        );
    }
}

#[test]
fn multipage_document_summary_stays_accurate_when_scrolled_and_edited() {
    let text = (1..=120)
        .map(|number| {
            format!("Paragraph number {number} lorem ipsum dolor sit amet, consectetur adipiscing elit.")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(&text), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    wire_writer_inspector_toggle(&app, &state, None);
    wire_responsive_layout(&app);
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app, 1280);
    apply_state(&app, &state);
    app.set_inspector_tab(1);

    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render multipage inspector");
    let (initial_page_count, page_pitch, page_three_offset) = {
        let document = state.current.borrow().clone();
        let style = document.page.page_style();
        let viewport = PageViewport {
            width: style.width_pt,
            height: app.get_page_view_height().max(300.0),
            ..*state.viewport.borrow()
        };
        let layout = document
            .layout(&style, viewport)
            .expect("layout the actual multipage document");
        let page_three = layout
            .pages
            .iter()
            .find(|page| page.index == 2)
            .expect("fixture has a third laid-out page");
        let fragment = page_three
            .fragments
            .first()
            .expect("third page contains document text");
        let block_index = document
            .blocks
            .iter()
            .position(|block| block.id == fragment.block_id)
            .expect("page fragment maps to a source block");
        let block_start = document
            .blocks
            .iter()
            .take(block_index)
            .map(|block| block.text.as_str().len() + 1)
            .sum::<usize>();

        (
            layout.pages.len(),
            style.height_pt + loom_writer_core::PAGE_GAP_PT,
            block_start + fragment.start + 2,
        )
    };
    assert!(
        initial_page_count >= 3,
        "the actual document layout has {initial_page_count} pages"
    );
    assert_eq!(app.get_page_count() as usize, initial_page_count);

    let assert_summary = |page_count: usize| {
        let expected = if page_count == 1 {
            "1 page".to_owned()
        } else {
            format!("{page_count} pages")
        };
        assert!(
            ElementHandle::find_by_accessible_label(&app, &expected).count() >= 1,
            "visible and accessible summary should be {expected:?}"
        );
        assert_eq!(
            ElementHandle::find_by_accessible_label(&app, &format!("Page 1 of {page_count}"))
                .count(),
            0,
            "the document summary must not claim a current page"
        );
    };
    assert_summary(initial_page_count);

    let page_three_scroll = 24.0 + 2.0 * page_pitch;
    app.invoke_page_scroll_changed(0.0, page_three_scroll);
    assert!(
        (state.viewport.borrow().scroll_y - page_three_scroll).abs() < 0.5,
        "the production scroll callback moves the viewport to page three"
    );
    let page_three_is_visible = {
        let document = state.current.borrow().clone();
        let style = document.page.page_style();
        let viewport = PageViewport {
            width: style.width_pt,
            height: app.get_page_view_height().max(300.0),
            ..*state.viewport.borrow()
        };
        document
            .layout(&style, viewport)
            .expect("layout after scrolling")
            .visible_ranges
            .iter()
            .any(|range| range.page_index == 2)
    };
    assert!(
        page_three_is_visible,
        "the real viewport layout includes page three after scrolling"
    );
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render summary after scrolling to page three");
    assert_summary(initial_page_count);

    app.invoke_selection_changed(page_three_offset as i32, page_three_offset as i32);
    let mut edited_text = state.current.borrow().editor_text();
    edited_text.replace_range(page_three_offset..page_three_offset + 1, "X");
    app.invoke_document_edited(
        edited_text.into(),
        (page_three_offset + 1) as i32,
        (page_three_offset + 1) as i32,
    );
    assert_eq!(
        &state.current.borrow().editor_text()[page_three_offset..page_three_offset + 1],
        "X",
        "the edit callback changed text at the page-three caret"
    );
    let page_three_is_still_visible = {
        let document = state.current.borrow().clone();
        let style = document.page.page_style();
        let viewport = PageViewport {
            width: style.width_pt,
            height: app.get_page_view_height().max(300.0),
            ..*state.viewport.borrow()
        };
        document
            .layout(&style, viewport)
            .expect("layout after the page-three edit")
            .visible_ranges
            .iter()
            .any(|range| range.page_index == 2)
    };
    assert!(
        page_three_is_still_visible,
        "the real viewport still includes page three after the edit"
    );

    let edited_page_count = {
        let document = state.current.borrow().clone();
        let style = document.page.page_style();
        let viewport = PageViewport {
            width: style.width_pt,
            height: app.get_page_view_height().max(300.0),
            ..*state.viewport.borrow()
        };
        document
            .layout(&style, viewport)
            .expect("layout after editing")
            .pages
            .len()
    };
    assert_eq!(edited_page_count, initial_page_count);
    assert_eq!(app.get_page_count() as usize, edited_page_count);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render summary after editing on page three");
    assert_summary(edited_page_count);
}

#[test]
fn common_formatting_and_page_setup_stay_separate_from_many_comments() {
    for (width, height) in [(1024, 720), (1280, 800)] {
        let (app, _state) = launched(width);
        app.window()
            .set_size(slint::PhysicalSize::new(width, height));
        apply_layout_breakpoints(&app, width);
        app.set_show_inspector(true);
        app.set_comment_entries(slint::ModelRc::new(VecModel::from(
            (0..24)
                .map(|index| WriterCommentEntry {
                    id: format!("comment-{index}").into(),
                    author: format!("Reviewer {index}").into(),
                    body: format!("Review note {index}: confirm this paragraph's wording.").into(),
                    resolved: false,
                    orphaned: false,
                })
                .collect::<Vec<_>>(),
        )));

        app.set_inspector_tab(0);
        let _ = snapshot_component(&app, width as f32, height as f32, 1.0)
            .expect("render common formatting at the requested viewport");
        for label in [
            "Heading Style",
            "Font family",
            "Font size",
            "Bold",
            "Text Alignment",
        ] {
            assert!(
                ElementHandle::find_by_accessible_label(&app, label).count() >= 1,
                "{label} remains directly available at {width}x{height}"
            );
        }
        assert_eq!(
            ElementHandle::find_by_accessible_label(&app, "Paragraph Direction").count(),
            0,
            "specialist paragraph controls stay collapsed at {width}x{height}"
        );
        assert_eq!(
            ElementHandle::find_by_accessible_label(&app, "Superscript").count(),
            0,
            "specialist baseline controls stay collapsed at {width}x{height}"
        );

        app.set_inspector_tab(1);
        let _ = snapshot_component(&app, width as f32, height as f32, 1.0)
            .expect("render Document inspector at the requested viewport");
        select_document_section(&app, "Page Setup", width as f32, height as f32);
        for label in ["Paper Size", "Orientation", "Margins"] {
            assert!(
                ElementHandle::find_by_accessible_label(&app, label).count() >= 1,
                "{label} remains reachable with 24 comments at {width}x{height}"
            );
        }
        assert_eq!(
            ElementHandle::find_by_accessible_label(
                &app,
                "Comment by Reviewer 0: Review note 0: confirm this paragraph's wording."
            )
            .count(),
            0,
            "review comments stay in their separate region"
        );

        app.invoke_open_comment_composer();
        let _ = snapshot_component(&app, width as f32, height as f32, 1.0)
            .expect("render the Review region with the comment composer");
        assert!(
            ElementHandle::find_by_accessible_label(&app, "New comment").count() >= 1,
            "the Review region opens with its composer at {width}x{height}"
        );
        assert!(
            ElementHandle::find_by_accessible_label(
                &app,
                "Comment by Reviewer 0: Review note 0: confirm this paragraph's wording."
            )
            .count()
                >= 1,
            "review content remains available at {width}x{height}"
        );
        assert_eq!(
            ElementHandle::find_by_accessible_label(&app, "Paper Size").count(),
            0,
            "Review and Page Setup do not compete for one scrolling region"
        );
    }
}

/// Whether the keyboard focus sits inside the inspector's Format/Document tab
/// strip, judged by window geometry (the focused item's centre inside the strip).
fn focus_is_on_inspector_tabs(app: &WriterApp) -> bool {
    use slint::private_unstable_api::re_exports::WindowInner;
    let Some(item) = WindowInner::from_pub(app.window())
        .focus_item
        .borrow()
        .upgrade()
    else {
        return false;
    };
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let (cx, cy) = (
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    let tabs = ElementHandle::find_by_accessible_label(app, "Inspector tab")
        .next()
        .expect("the inspector tab strip exists");
    let (p, s) = (tabs.absolute_position(), tabs.size());
    cx >= p.x && cx <= p.x + s.width && cy >= p.y && cy <= p.y + s.height
}

#[test]
fn startup_does_not_put_keyboard_focus_on_the_inspector_tabs() {
    // Startup as the window first renders it: the inspector opens and nothing
    // has been pressed or clicked, so no inspector tab may draw keyboard focus.
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document("Hello"), dialogs);
    wire_writer_inspector_toggle(&app, &state, None);
    wire_responsive_layout(&app);
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app, 1280);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    assert!(app.get_show_inspector());
    assert!(
        !focus_is_on_inspector_tabs(&app),
        "the inspector tab strip drew focus at startup"
    );
}

#[test]
fn compact_inspector_drawer_does_not_dim_the_page_outside_the_panel() {
    let (app, _state) = launched(1024);
    assert!(app.get_compact_inspector_layout());
    assert!(!app.get_show_inspector());
    let closed_frame = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render closed");
    let closed_pixel = closed_frame.get_pixel(250, 300).0;

    app.invoke_toggle_inspector();
    assert!(app.get_show_inspector());
    let open_frame = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render open");
    let open_pixel = open_frame.get_pixel(250, 300).0;
    assert_eq!(
        open_pixel, closed_pixel,
        "opening the compact inspector drawer must not dim the page outside the panel"
    );

    let pos = slint::LogicalPosition::new(250.0, 300.0);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: pos,
            button: slint::platform::PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: pos,
            button: slint::platform::PointerEventButton::Left,
        });
    assert!(
        !app.get_show_inspector(),
        "clicking outside the compact inspector drawer still closes it"
    );
}

#[test]
fn focus_page_at_launch_focuses_editor_synchronously_without_mutating_focus_during_tree_instantiation(
) {
    use slint::private_unstable_api::re_exports::WindowInner;
    let (app, _state) = launched(1280);
    // Simulate X11 app.show() initial draw instantiating the tree before focus_page_at_launch.
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("initial show draw");

    focus_page_at_launch(&app);
    let window_inner = WindowInner::from_pub(app.window());
    let before = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("focused item immediately after focus_page_at_launch");

    // AccessKitAdapter::build_new_tree calls ensure_tree_instantiated() while holding
    // accesskit_adapter.borrow_mut(). If ensure_tree_instantiated() changes focus_item,
    // WinitWindowAdapter::handle_focus_change re-borrows accesskit_adapter mutably and panics.
    window_inner.ensure_tree_instantiated();
    let after = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("focused item after ensure_tree_instantiated");

    assert_eq!(
        before, after,
        "ensure_tree_instantiated must not mutate focus_item after focus_page_at_launch"
    );
}

#[test]
fn narrowing_after_launch_closes_the_inspector_without_moving_focus_during_tree_build() {
    use slint::private_unstable_api::re_exports::WindowInner;
    // Launch docked (1296 px) with the inspector open and draw once as the
    // window first shows; the page owns keyboard focus.
    let (app, _state) = launched(1296);
    let _ = snapshot_component(&app, 1296.0, 800.0, 1.0).expect("initial show draw");
    let window_inner = WindowInner::from_pub(app.window());
    window_inner.ensure_tree_instantiated();
    let page = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("the page owns focus after launch");

    // Narrow to 1060 px right after launch: the auto-opened inspector closes.
    app.invoke_window_resized(1060.0);
    assert!(
        !app.get_show_inspector(),
        "the auto-opened inspector closes"
    );

    // AccessKit's build_new_tree runs pending change handlers while it holds
    // its adapter borrow. Closing must not move focus from inside them: a
    // focus change there re-borrows the adapter and panics on Windows and Linux.
    window_inner.ensure_tree_instantiated();
    let after = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("focused item after the tree build");
    assert_eq!(
        page, after,
        "closing the auto-opened inspector moved focus during tree instantiation"
    );
}

/// Whether the keyboard focus sits inside any element with this accessible
/// label, judged by window geometry (the focused item's centre inside it).
fn focus_is_on_element_labelled(app: &WriterApp, label: &str) -> bool {
    use slint::private_unstable_api::re_exports::WindowInner;
    let Some(item) = WindowInner::from_pub(app.window())
        .focus_item
        .borrow()
        .upgrade()
    else {
        return false;
    };
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let (cx, cy) = (
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    ElementHandle::find_by_accessible_label(app, label).any(|element| {
        let (p, s) = (element.absolute_position(), element.size());
        cx >= p.x && cx <= p.x + s.width && cy >= p.y && cy <= p.y + s.height
    })
}

#[test]
fn closing_the_docked_inspector_returns_focus_to_its_toolbar_tab() {
    // A user closes the inspector (the same callback its close button, its
    // Format tab and the View menu use). Focus goes back to the Format tab.
    let (app, _state) = launched(1280);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render open");
    assert!(app.get_show_inspector());

    app.invoke_toggle_inspector();
    assert!(!app.get_show_inspector());
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render closed");

    assert!(
        focus_is_on_element_labelled(&app, "Format inspector"),
        "closing the inspector must return focus to its Format tab"
    );
}
