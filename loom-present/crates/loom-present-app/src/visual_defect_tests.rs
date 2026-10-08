//! Rendered geometry/pixel regressions from the hostile Linux review.
use super::keyboard_flow_tests::launched;
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use image::RgbaImage;
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

/// Light-theme paper, the slide's own colour.
const PAPER: [u8; 4] = [255, 255, 255, 255];

/// The largest slide-stage canvas (thumbnails also hold a canvas; the editor's is widest).
fn editor_canvas(app: &PresentApp) -> (f32, f32, f32, f32) {
    ElementHandle::find_by_element_type_name(app, "PresentSlideCanvas")
        .map(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            (p.x, p.y, s.width, s.height)
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .expect("editor slide canvas")
}

/// Longest run of paper-coloured pixels along a row or column.
fn longest_paper_run(pixels: impl Iterator<Item = [u8; 4]>) -> u32 {
    let (mut best, mut run) = (0u32, 0u32);
    for pixel in pixels {
        if pixel == PAPER {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

#[test]
fn slide_stage_fills_most_of_the_canvas_and_keeps_16_by_9() {
    for (width, height) in [(1440u32, 900u32), (1920, 1200)] {
        let session = launched();
        let app = &session.app;
        apply_theme(app, "light");
        configure_responsive_layout(app, (width, height));
        let image = snapshot_component(app, width as f32, height as f32, 1.0).unwrap();
        let (cx, cy, cw, ch) = editor_canvas(app);
        // Best paper run over many rows and columns: text can only shorten a run.
        let mut slide_w = 0u32;
        for step in 1..40 {
            let y = (cy + ch * step as f32 / 40.0) as u32;
            let run =
                longest_paper_run((cx as u32..(cx + cw) as u32).map(|x| image.get_pixel(x, y).0));
            slide_w = slide_w.max(run);
        }
        let mut slide_h = 0u32;
        for step in 1..40 {
            let x = (cx + cw * step as f32 / 40.0) as u32;
            let run =
                longest_paper_run((cy as u32..(cy + ch) as u32).map(|y| image.get_pixel(x, y).0));
            slide_h = slide_h.max(run);
        }
        let fills_width = slide_w as f32 >= 0.85 * cw;
        let fills_height = slide_h as f32 >= 0.85 * ch;
        assert!(
            fills_width || fills_height,
            "{width}x{height}: slide {slide_w}x{slide_h} px in a {cw}x{ch} canvas leaves dead space"
        );
        let ratio = slide_w as f32 / slide_h as f32;
        assert!(
            (ratio - 16.0 / 9.0).abs() < 0.03,
            "{width}x{height}: slide aspect {ratio:.3} is not 16:9"
        );
    }
}

/// Widths of the blank gaps between ink columns in a row band: letters are
/// separated by a pixel or two, a word space is wider.
fn ink_gaps(image: &RgbaImage, x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u32> {
    let ink = |x: u32| (y0..y1).any(|y| image.get_pixel(x, y).0[0] < 160);
    let (mut gaps, mut gap, mut seen) = (Vec::new(), 0u32, false);
    for x in x0..x1 {
        if ink(x) {
            if seen && gap > 0 {
                gaps.push(gap);
            }
            seen = true;
            gap = 0;
        } else if seen {
            gap += 1;
        }
    }
    gaps
}

#[test]
fn thumbnail_title_words_keep_their_spacing_at_1x() {
    let session = launched();
    let app = &session.app;
    apply_theme(app, "light");
    configure_responsive_layout(app, (1920, 1200));
    let image = snapshot_component(app, 1920.0, 1200.0, 1.0).unwrap();
    let thumb = ElementHandle::find_by_element_type_name(app, "MiniSlide")
        .find(|e| {
            e.accessible_label()
                .is_some_and(|label| label.ends_with("Built around ownership"))
        })
        .expect("third thumbnail");
    let (p, s) = (thumb.absolute_position(), thumb.size());
    let (x0, x1) = (p.x as u32 + 2, (p.x + s.width) as u32 - 2);
    let (y0, y1) = (
        (p.y + s.height * 0.17) as u32,
        (p.y + s.height * 0.30) as u32,
    );
    // "Built around ownership" has two word spaces: the two widest gaps must be
    // clearly wider than the letter gaps, or the words have run together.
    let mut gaps = ink_gaps(&image, x0, x1, y0, y1);
    gaps.sort_unstable();
    let widest_two = gaps.iter().rev().take(2).copied().collect::<Vec<_>>();
    assert!(
        widest_two.len() == 2 && widest_two[1] >= 3 && gaps.len() >= 2,
        "thumbnail title lost its word spacing; gaps {gaps:?}"
    );
}

#[test]
fn rtl_inspector_hint_follows_reading_direction() {
    let session = launched();
    let app = &session.app;
    apply_theme(app, "light");
    configure_direction(app, true);
    configure_responsive_layout(app, (1920, 1200));
    // With nothing selected the inspector shows the hint instead of object fields.
    app.set_selection_count(0);
    app.set_show_inspector(true);
    let image = snapshot_component(app, 1920.0, 1200.0, 1.0).unwrap();
    let texts: Vec<_> = ElementHandle::find_by_element_type_name(app, "Text")
        .filter(|e| e.absolute_position().x < 300.0 && e.size().width > 200.0)
        .collect();
    let hint = texts
        .iter()
        .find(|e| {
            e.accessible_label()
                .is_some_and(|label| label.starts_with("Select an object"))
        })
        .unwrap_or_else(|| {
            let found: Vec<_> = texts
                .iter()
                .map(|e| (e.accessible_label(), e.absolute_position(), e.size()))
                .collect();
            panic!("inspector hint not found among {found:?}")
        });
    let (p, s) = (hint.absolute_position(), hint.size());
    // Ink rows inside the hint box, grouped into lines; the last line is short.
    let ink_at = |x: u32, y: u32| image.get_pixel(x, y).0[0] < 160;
    let (x0, x1) = (p.x as u32, (p.x + s.width) as u32);
    let rows: Vec<u32> = (p.y as u32..(p.y + s.height) as u32)
        .filter(|&y| (x0..x1).any(|x| ink_at(x, y)))
        .collect();
    let mut lines: Vec<Vec<u32>> = Vec::new();
    for y in rows {
        match lines.last_mut() {
            Some(line) if *line.last().unwrap() + 1 >= y => line.push(y),
            _ => lines.push(vec![y]),
        }
    }
    let last = lines.last().expect("hint ink");
    let last_columns: Vec<u32> = (x0..x1)
        .filter(|&x| last.iter().any(|&y| ink_at(x, y)))
        .collect();
    let right_gap = (p.x + s.width) - (*last_columns.last().unwrap() as f32);
    assert!(
        right_gap <= 4.0,
        "the hint's last line must sit at the reading-start (right) edge: right gap {right_gap}"
    );
}

#[test]
fn high_contrast_segmented_control_draws_no_partial_dividers() {
    let session = launched();
    let app = &session.app;
    apply_theme(app, "high-contrast");
    configure_responsive_layout(app, (1920, 1200));
    let image = snapshot_component(app, 1920.0, 1200.0, 1.0).unwrap();
    // Inspector tabs: Format is selected, so the boundary Animate|Document is
    // not next to the selection and is where a stray divider would show.
    let segments: Vec<_> = ["Format", "Animate", "Document"]
        .iter()
        .map(|label| {
            ElementHandle::find_by_accessible_label(app, label)
                .find(|e| {
                    let p = e.absolute_position();
                    // The inspector's tab strip, not the toolbar's identical labels.
                    p.x > 1600.0 && p.y > 90.0 && p.y < 200.0
                })
                .expect(label)
        })
        .collect();
    let mut dividers = Vec::new();
    for pair in segments.windows(2) {
        let right = pair[1].absolute_position();
        let s = pair[1].size();
        let x = right.x as u32;
        let cy = (right.y + s.height / 2.0) as u32;
        let differs = image.get_pixel(x, cy).0 != image.get_pixel(x, cy.saturating_sub(10)).0;
        dividers.push(differs);
    }
    // Only the boundary away from the selected segment is judged: a selected
    // segment is bounded by its own raised edge, which is not a divider.
    assert!(
        !dividers[1],
        "high-contrast segmented control draws a divider between Animate and Document but not between Format and Animate"
    );
}
