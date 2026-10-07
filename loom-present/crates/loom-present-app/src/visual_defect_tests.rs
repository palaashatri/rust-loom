//! Rendered geometry/pixel regressions from the hostile Linux review.
use super::keyboard_flow_tests::launched;
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::snapshot_component;

fn element(app: &PresentApp, kind: &str) -> ElementHandle {
    ElementHandle::find_by_element_type_name(app, kind)
        .next()
        .expect(kind)
}

#[test]
fn open_inspector_remains_enabled_in_compact_chrome() {
    let session = launched();
    let app = &session.app;
    Theme::get(app).set_text_scale(2.0);
    session
        .state
        .inspector_available
        .set(configure_responsive_width(app, 1280));
    app.invoke_show_inspector_tab(0);
    assert!(app.get_show_inspector());
    let menu = session.state.menu_service.as_ref().unwrap();
    sync_menu_state(menu, app, &session.state);
    let row = app
        .get_local_menu_items()
        .iter()
        .find(|row| row.command_id == "view.inspector")
        .unwrap();
    assert!(row.checked);
    assert!(
        row.enabled,
        "an inspector opened from the toolbar must remain available from View"
    );
    app.invoke_toggle_inspector();
    assert!(
        !app.get_show_inspector(),
        "View can close the open inspector"
    );
}

#[test]
fn menu_separators_stay_inside_popup() {
    for rtl in [false, true] {
        let session = launched();
        let app = &session.app;
        configure_direction(app, rtl);
        let before = snapshot_component(app, 1280.0, 800.0, 1.0).unwrap();
        app.set_local_menu_open_index(0);
        let after = snapshot_component(app, 1280.0, 800.0, 1.0).unwrap();
        // Beyond the 288px popup and its shadow: other panes must stay intact.
        let x = if rtl { 80 } else { 800 };
        let changed = (100..500)
            .filter(|y| before.get_pixel(x, *y) != after.get_pixel(x, *y))
            .count();
        assert_eq!(changed, 0, "separator escaped the popup (rtl={rtl})");
    }
}

#[test]
fn navigator_buttons_and_glyphs_scale() {
    for rtl in [false, true] {
        let session = launched();
        let app = &session.app;
        configure_direction(app, rtl);
        for scale in [1.0f32, 1.5, 2.0] {
            Theme::get(app).set_text_scale(scale);
            snapshot_component(app, 1280.0, 800.0, 1.0).unwrap();
            let strip = element(app, "PresentSlideStrip");
            for label in [
                "Add Slide",
                "Delete Slide",
                "Move Slide Up",
                "Move Slide Down",
            ] {
                let button = strip
                    .query_descendants()
                    .match_accessible_role(AccessibleRole::Button)
                    .match_predicate(move |e| e.accessible_label().as_deref() == Some(label))
                    .find_first()
                    .unwrap();
                let icon = button
                    .query_descendants()
                    .match_type_name("Icon")
                    .find_first()
                    .unwrap();
                assert!(
                    (icon.size().width - 16.0 * scale).abs() < 0.5,
                    "{label} glyph does not scale at {scale}x"
                );
                assert!(
                    button.size().height >= 28.0 * scale - 0.5,
                    "{label} target does not scale"
                );
                assert!(button.absolute_position().x >= strip.absolute_position().x);
                assert!(
                    button.absolute_position().x + button.size().width
                        <= strip.absolute_position().x + strip.size().width + 0.5
                );
            }
        }
    }
}

#[test]
fn template_cards_use_content_width_in_both_directions() {
    for rtl in [false, true] {
        for width in [1024.0f32, 1280.0] {
            let session = launched();
            let app = &session.app;
            configure_direction(app, rtl);
            app.set_theme_chooser_open(true);
            snapshot_component(app, width, 800.0, 1.0).unwrap();
            let sidebar = ElementHandle::find_by_element_id(app, "PresentThemeChooser::sidebar")
                .next()
                .unwrap();
            let cards: Vec<_> =
                ElementHandle::find_by_element_type_name(app, "PresentThemeOption").collect();
            assert_eq!(cards.len(), 4);
            let left = cards
                .iter()
                .map(|c| c.absolute_position().x)
                .fold(f32::INFINITY, f32::min);
            let right = cards
                .iter()
                .map(|c| c.absolute_position().x + c.size().width)
                .fold(0.0f32, f32::max);
            let dialog_width = (width - 48.0).min(1160.0);
            let dialog_x = (width - dialog_width) / 2.0;
            let content_left = dialog_x + if rtl { 0.0 } else { sidebar.size().width };
            let content_right =
                dialog_x + dialog_width - if rtl { sidebar.size().width } else { 0.0 };
            let (start_gap, end_gap) = if rtl {
                (content_right - right, left - content_left)
            } else {
                (left - content_left, content_right - right)
            };
            assert!(
                (0.0..=40.0).contains(&start_gap),
                "dead space at the reading-start edge: {start_gap}px (rtl={rtl})"
            );
            assert!(end_gap >= 0.0, "cards overflow the dialog (rtl={rtl})");
        }
    }
}

#[test]
fn inspector_has_only_tab_label_and_accessible_name() {
    let session = launched();
    let app = &session.app;
    app.set_show_inspector(true);
    snapshot_component(app, 1280.0, 800.0, 1.0).unwrap();
    let inspector = element(app, "PresentInspector");
    let labels = inspector
        .query_descendants()
        .match_type_name("Text")
        .match_predicate(|e| e.accessible_label().as_deref() == Some("Format"))
        .find_all();
    assert_eq!(
        labels.len(),
        1,
        "Format should appear only in the tab strip"
    );
    assert_eq!(inspector.accessible_role(), Some(AccessibleRole::Groupbox));
    assert_eq!(
        inspector.accessible_label().as_deref(),
        Some("Slide inspector")
    );
}

mod reference {
    slint::slint! {
        import { Icon } from "../../../../loom-core/crates/loom-ui/ui/icons.slint";
        import { Theme } from "../../../../loom-core/crates/loom-ui/ui/theme.slint";
        export component CategoryIcon inherits Window {
            in property <string> name;
            in property <length> offset-x;
            in property <length> offset-y;
            background: Theme.palette().panel;
            Icon { x: root.offset-x; y: root.offset-y; size: 16px; icon: root.name; tint: Theme.palette().ink-secondary; }
        }
    }
}

#[test]
fn template_categories_draw_layout_and_blank_icons() {
    for rtl in [false, true] {
        let session = launched();
        let app = &session.app;
        configure_direction(app, rtl);
        app.set_theme_chooser_open(true);
        let image = snapshot_component(app, 1280.0, 800.0, 1.0).unwrap();
        // Reference captures replace the shared capture window. Read both app
        // positions before showing that second component.
        let categories: Vec<_> = [("Layouts", "slide"), ("Blank", "stop")]
            .into_iter()
            .map(|(label, expected)| {
                let category =
                    ElementHandle::find_by_element_type_name(app, "PresentThemeCategory")
                        .find(|c| {
                            !c.query_descendants()
                                .match_predicate(move |e| {
                                    e.accessible_label().as_deref() == Some(label)
                                })
                                .find_all()
                                .is_empty()
                        })
                        .unwrap();
                let icon = category
                    .query_descendants()
                    .match_type_name("Icon")
                    .find_first()
                    .unwrap();
                (label, expected, icon.absolute_position())
            })
            .collect();
        for (label, expected, p) in categories {
            let reference = reference::CategoryIcon::new().unwrap();
            reference.set_name(expected.into());
            reference.set_offset_x(p.x.fract());
            reference.set_offset_y(p.y.fract());
            let expected_image = snapshot_component(&reference, 20.0, 20.0, 1.0).unwrap();
            let mut difference = 0u32;
            for y in 0..16 {
                for x in 0..16 {
                    let a = image.get_pixel(p.x.floor() as u32 + x, p.y.floor() as u32 + y);
                    let b = expected_image.get_pixel(x, y);
                    difference += (0..3).map(|ch| a[ch].abs_diff(b[ch]) as u32).sum::<u32>();
                }
            }
            assert!(
                difference < 1000,
                "{label} does not draw its {expected} icon: pixel difference {difference}"
            );
        }
    }
}
