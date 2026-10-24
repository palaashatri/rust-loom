//! Comment anchors and their margin markers, measured on the pixels the page
//! draws in every window theme. The contrast ratios are WCAG 2.x relative
//! luminance computed from those rendered colours, not from theme names.

use super::*;
use crate::projection::COMMENT_ANCHOR_FILL;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use image::RgbaImage;
use loom_test_support::capture::snapshot_component;
use std::collections::HashMap;

type Rgb = [u8; 3];

/// The page ink in light and dark, the high-contrast page ink, and the page paper.
const PAGE_INK: Rgb = [0x1d, 0x1d, 0x1f];
const HIGH_CONTRAST_INK: Rgb = [0x00, 0x00, 0x00];
const PAGE_PAPER: Rgb = [0xff, 0xff, 0xff];
const THEMES: [&str; 3] = ["light", "dark", "high-contrast"];

fn relative_luminance(rgb: Rgb) -> f64 {
    let channel = |value: u8| {
        let c = f64::from(value) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// A page with one comment on "paragraph with a remark on", rendered in `theme`.
fn commented_page(theme: &str) -> (WriterApp, RgbaImage) {
    let mut document = super::actions_tests::text_document(
        "A paragraph with a remark on some of the words in it.",
    );
    let block_id = document.blocks[0].id;
    document
        .add_comment_thread(block_id, 2, 30, "Check this wording")
        .expect("comment");
    let (app, state) = super::actions_tests::test_state(
        document,
        std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
    );
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_theme(&app, theme);
    apply_state(&app, &state);
    let frame = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render the commented page");
    (app, frame)
}

/// Window position of the editor's text box, where the page's own coordinates start.
fn document_origin(app: &WriterApp) -> (f32, f32) {
    let label = app.get_document_accessible_label().to_string();
    let body = ElementHandle::find_by_accessible_label(app, label.as_str())
        .find(|element| element.accessible_role() == Some(AccessibleRole::TextInput))
        .expect("the document editor");
    let origin = body.absolute_position();
    (origin.x, origin.y)
}

/// The first anchor rectangle in whole window pixels: left, top, right, bottom.
fn first_anchor_box(app: &WriterApp) -> (u32, u32, u32, u32) {
    let rect = app
        .get_comment_rects()
        .row_data(0)
        .expect("the comment has an anchor rectangle");
    let (ox, oy) = document_origin(app);
    let zoom = app.get_page_zoom();
    (
        (ox + rect.x * zoom).round() as u32,
        (oy + rect.y * zoom).round() as u32,
        (ox + (rect.x + rect.width) * zoom).round() as u32,
        (oy + (rect.y + rect.height) * zoom).round() as u32,
    )
}

/// The colours inside a window-pixel box, leaving out `inset` pixels on each side.
fn pixels_in(
    frame: &RgbaImage,
    (left, top, right, bottom): (u32, u32, u32, u32),
    inset: u32,
) -> Vec<Rgb> {
    let mut pixels = Vec::new();
    for y in (top + inset)..bottom.saturating_sub(inset) {
        for x in (left + inset)..right.saturating_sub(inset) {
            let [r, g, b, _] = frame.get_pixel(x, y).0;
            pixels.push([r, g, b]);
        }
    }
    assert!(
        !pixels.is_empty(),
        "the box {left},{top}..{right},{bottom} lies in the frame"
    );
    pixels
}

/// The colour that covers most of a box: its background.
fn background_of(pixels: &[Rgb]) -> Rgb {
    let mut counts: HashMap<Rgb, usize> = HashMap::new();
    for pixel in pixels {
        *counts.entry(*pixel).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(colour, _)| colour)
        .expect("the box has pixels")
}

/// The darkest colour in a box: the ink drawn on its background.
fn darkest_of(pixels: &[Rgb]) -> Rgb {
    *pixels
        .iter()
        .min_by(|a, b| {
            relative_luminance(**a)
                .partial_cmp(&relative_luminance(**b))
                .expect("luminance is finite")
        })
        .expect("the box has pixels")
}

#[test]
fn the_comment_highlight_keeps_page_ink_above_the_body_text_floor() {
    // Page ink is #1d1d1f in light and dark and #000000 in high contrast; the
    // highlight is a document colour, so it must keep both above 4.5:1.
    for ink in [PAGE_INK, HIGH_CONTRAST_INK] {
        let ratio = contrast_ratio(COMMENT_ANCHOR_FILL, ink);
        assert!(
            ratio >= 4.5,
            "page ink {ink:?} on the comment highlight {COMMENT_ANCHOR_FILL:?} measures {ratio:.2}:1"
        );
    }
}

#[test]
fn comment_anchors_draw_the_document_highlight_in_every_theme() {
    for theme in THEMES {
        let (app, frame) = commented_page(theme);
        let anchor = first_anchor_box(&app);
        let inside = pixels_in(&frame, anchor, 2);
        let background = background_of(&inside);
        assert_eq!(
            background, COMMENT_ANCHOR_FILL,
            "{theme}: the anchor is drawn in {background:?}, not the document highlight"
        );
        let ink = darkest_of(&inside);
        let ratio = contrast_ratio(background, ink);
        assert!(
            ratio >= 4.5,
            "{theme}: text on the anchor measures {ratio:.2}:1 ({ink:?} on {background:?})"
        );
        if theme == "high-contrast" {
            let (left, top, right, bottom) = anchor;
            let (mid_x, mid_y) = ((left + right) / 2, (top + bottom) / 2);
            for (x, y, edge) in [(left, mid_y, "left"), (mid_x, top, "top")] {
                let [r, g, b, _] = frame.get_pixel(x, y).0;
                let outline = contrast_ratio([r, g, b], COMMENT_ANCHOR_FILL);
                assert!(
                    outline >= 7.0,
                    "high contrast: the {edge} edge of the anchor is {outline:.2}:1 against the highlight, not a 2 px ink outline"
                );
            }
        }
    }
}

#[test]
fn comment_marker_draws_page_ink_on_page_paper_in_every_theme() {
    for theme in THEMES {
        let (app, frame) = commented_page(theme);
        let marker = ElementHandle::find_by_accessible_label(&app, "Show comment by You on page")
            .find(|element| element.size().width > 0.0)
            .expect("the comment has a visible page marker");
        let (origin, size) = (marker.absolute_position(), marker.size());
        let bounds = (
            origin.x.round() as u32,
            origin.y.round() as u32,
            (origin.x + size.width).round() as u32,
            (origin.y + size.height).round() as u32,
        );
        let pixels = pixels_in(&frame, bounds, 3);
        let paper = background_of(&pixels);
        assert_eq!(
            paper, PAGE_PAPER,
            "{theme}: the marker sits on {paper:?}, not the page paper"
        );
        let ink = darkest_of(&pixels);
        let ratio = contrast_ratio(paper, ink);
        assert!(
            ratio >= 4.5,
            "{theme}: the marker glyph measures {ratio:.2}:1 ({ink:?} on {paper:?})"
        );
    }
}
