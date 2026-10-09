//! A thumbnail is the slide drawn small and the slideshow is the slide drawn
//! large. Both must set text where the editor does: the same lines in the same
//! places, the slide number grows with text size, and an empty placeholder
//! draws nothing. Measured in device pixels from real renders.

use super::keyboard_flow_tests::{launched, render, Session};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use image::RgbaImage;
use loom_test_support::capture::snapshot_component;

/// Device pixels per logical pixel. Thumbnail text is only about 5 logical
/// pixels tall, so it is measured at 2x.
const SCALE: f32 = 2.0;
const PAPER: [u8; 4] = [255, 255, 255, 255];
const LONG_TITLE: &str =
    "Quarterly results, the plan for the next two years, and the hiring that the plan needs";

/// A rectangle in device pixels: left, top, right, bottom.
type Area = (u32, u32, u32, u32);
/// An object's place in the slide's layout domain: x, y, width, height.
type Place = (f32, f32, f32, f32);

fn device_area(element: &ElementHandle) -> Area {
    let (p, s) = (element.absolute_position(), element.size());
    (
        (p.x * SCALE).floor() as u32,
        (p.y * SCALE).floor() as u32,
        ((p.x + s.width) * SCALE).ceil() as u32,
        ((p.y + s.height) * SCALE).ceil() as u32,
    )
}

/// The largest slide canvas on screen: the editor's, or the slideshow's while it runs.
fn slide_area(app: &PresentApp) -> Area {
    let canvas = ElementHandle::find_by_element_type_name(app, "PresentSlideCanvas")
        .max_by(|a, b| a.size().width.total_cmp(&b.size().width))
        .expect("slide canvas");
    device_area(&canvas)
}

/// The thumbnail button of slide `index` in a deck of `count` slides.
fn thumbnail(app: &PresentApp, index: usize, count: usize) -> ElementHandle {
    let prefix = format!("Slide {} of {count}, ", index + 1);
    app.root_element()
        .query_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .match_predicate(move |e| {
            e.accessible_label()
                .is_some_and(|label| label.starts_with(prefix.as_str()))
        })
        .find_first()
        .expect("thumbnail button")
}

/// Clears the selection, so no object draws its selection border among the text.
fn deselect(s: &Session) {
    s.state.session.borrow_mut().selected_elements.clear();
    refresh(&s.app, &s.state);
}

/// The title object's place on the first slide.
fn title_place(s: &Session) -> Place {
    let document = s.state.session.borrow();
    let title = document.document.slides[0]
        .elements
        .iter()
        .find(|element| element.id == "cover-title")
        .expect("the cover title");
    (title.x, title.y, title.width, title.height)
}

/// The slide itself: the bounding box of its paper-coloured pixels inside `area`.
fn slide_box(image: &RgbaImage, (x0, y0, x1, y1): Area) -> Area {
    let mut found: Option<Area> = None;
    for y in y0..y1 {
        for x in x0..x1 {
            if image.get_pixel(x, y).0 == PAPER {
                found = Some(match found {
                    None => (x, y, x + 1, y + 1),
                    Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x + 1), d.max(y + 1)),
                });
            }
        }
    }
    found.unwrap_or_else(|| panic!("no slide inside {:?}", (x0, y0, x1, y1)))
}

/// Where an object sits on a slide drawn at `slide`, from its place in the layout domain.
fn object_area(slide: Area, (x, y, width, height): Place) -> Area {
    let (sw, sh) = ((slide.2 - slide.0) as f32, (slide.3 - slide.1) as f32);
    let x0 = slide.0 as f32 + x * sw / 1000.0;
    let y0 = slide.1 as f32 + y * sh / 562.5;
    let x1 = x0 + width * sw / 1000.0;
    let y1 = y0 + height * sh / 562.5;
    (
        x0.round() as u32,
        y0.round() as u32,
        x1.round() as u32,
        y1.round() as u32,
    )
}

/// Bands of dark text inside `area`, as (top, bottom) fractions of its height.
fn text_lines(image: &RgbaImage, (x0, y0, x1, y1): Area) -> Vec<(f32, f32)> {
    let height = (y1 - y0) as f32;
    let mut lines = Vec::new();
    let mut top = None;
    for y in y0..=y1 {
        let inked = y < y1 && (x0..x1).any(|x| image.get_pixel(x, y).0[0] < 160);
        match (inked, top) {
            (true, None) => top = Some(y),
            (false, Some(start)) => {
                lines.push(((start - y0) as f32 / height, (y - y0) as f32 / height));
                top = None;
            }
            _ => {}
        }
    }
    lines
}

/// Renders the window and reports where the title's text lines sit in its box.
fn title_lines(app: &PresentApp, place: Place, width: u32, height: u32) -> Vec<(f32, f32)> {
    let image = snapshot_component(app, width as f32, height as f32, SCALE).expect("render");
    let slide = slide_box(&image, slide_area(app));
    text_lines(&image, object_area(slide, place))
}

/// Asserts the same lines in the same places, to within `tolerance` of the title's
/// height. Text at a few logical pixels rounds its metrics to whole pixels.
fn assert_same_lines(what: &str, expected: &[(f32, f32)], actual: &[(f32, f32)], tolerance: f32) {
    assert!(
        expected.len() >= 2,
        "{what}: the title should wrap onto two lines, found {expected:?}"
    );
    assert_eq!(
        actual.len(),
        expected.len(),
        "{what}: the text wraps onto a different number of lines ({actual:?} against {expected:?})"
    );
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a.0 - e.0).abs() < tolerance && (a.1 - e.1).abs() < tolerance,
            "{what}: a line sits in a different place ({actual:?} against {expected:?})"
        );
    }
}

/// Height in device pixels from the first to the last row with dark ink in `area`.
fn ink_height(image: &RgbaImage, (x0, y0, x1, y1): Area) -> u32 {
    let rows: Vec<u32> = (y0..y1)
        .filter(|&y| (x0..x1).any(|x| image.get_pixel(x, y).0[0] < 160))
        .collect();
    match (rows.first(), rows.last()) {
        (Some(first), Some(last)) => last - first + 1,
        _ => 0,
    }
}

/// Bands of anything drawn in `area` (text or a light bar), as (top, bottom)
/// fractions of its height.
fn drawn_bands(image: &RgbaImage, (x0, y0, x1, y1): Area) -> Vec<(f32, f32)> {
    let height = (y1 - y0) as f32;
    let mut bands = Vec::new();
    let mut top = None;
    for y in y0..=y1 {
        let drawn = y < y1 && (x0..x1).any(|x| image.get_pixel(x, y).0[0] < 235);
        match (drawn, top) {
            (true, None) => top = Some(y),
            (false, Some(start)) => {
                bands.push(((start - y0) as f32 / height, (y - y0) as f32 / height));
                top = None;
            }
            _ => {}
        }
    }
    bands
}

const LONG_BODY: &str = "Revenue grew in every region this quarter, and the new hires in the \
    support and sales teams are on track to meet the plan for the next two years. The next \
    review covers pricing, the partner programme, and the budget for the hiring that remains.";

#[test]
fn a_thumbnail_body_draws_one_bar_per_line_its_text_would_take() {
    let s = launched();
    // The Content layout gives the first slide its body placeholder.
    s.app.invoke_apply_template(1);
    let body = s.state.session.borrow().document.slides[0]
        .elements
        .iter()
        .position(|element| element.element_type == loom_present_core::ElementType::BodyText)
        .expect("the Content layout has a body placeholder");
    let count = s.state.session.borrow().document.slides.len();

    let bands_for = |text: &str| -> usize {
        {
            let mut session = s.state.session.borrow_mut();
            session.document.slides[0].elements[body].content = text.into();
        }
        refresh(&s.app, &s.state);
        let image = snapshot_component(&s.app, 1280.0, 800.0, SCALE).expect("render");
        let mini = thumbnail(&s.app, 0, count)
            .query_descendants()
            .match_type_name("MiniSlide")
            .find_first()
            .expect("the first thumbnail's slide");
        let place = {
            let session = s.state.session.borrow();
            let element = &session.document.slides[0].elements[body];
            (element.x, element.y, element.width, element.height)
        };
        drawn_bands(
            &image,
            object_area(slide_box(&image, device_area(&mini)), place),
        )
        .len()
    };
    let short = bands_for("Thanks");
    let long = bands_for(LONG_BODY);
    assert!(short >= 1, "a short body is still drawn");
    assert!(
        long >= short + 2,
        "a body that takes several lines draws several bars: {short} for one line, {long} for the long text"
    );
}

#[test]
fn the_title_wraps_onto_the_same_lines_in_the_editor_at_any_size_and_in_the_slideshow() {
    let s = launched();
    s.app.invoke_update_element_content(LONG_TITLE.into());
    deselect(&s);
    let place = title_place(&s);
    let editor = title_lines(&s.app, place, 1280, 800);

    configure_responsive_layout(&s.app, (1920, 1200));
    let wide = title_lines(&s.app, place, 1920, 1200);

    s.app.set_is_preview_mode(true);
    let show = title_lines(&s.app, place, 1280, 800);

    assert_same_lines("the editor at 1920x1200", &editor, &wide, 0.04);
    assert_same_lines("the slideshow at 1280x800", &editor, &show, 0.04);
}

#[test]
fn a_thumbnail_is_the_slide_drawn_small() {
    let s = launched();
    s.app.invoke_update_element_content(LONG_TITLE.into());
    deselect(&s);
    let place = title_place(&s);
    let editor = title_lines(&s.app, place, 1280, 800);

    let image = snapshot_component(&s.app, 1280.0, 800.0, SCALE).expect("render");
    let mini = thumbnail(&s.app, 0, 3)
        .query_descendants()
        .match_type_name("MiniSlide")
        .find_first()
        .expect("the first thumbnail's slide");
    let lines = text_lines(
        &image,
        object_area(slide_box(&image, device_area(&mini)), place),
    );

    // The thumbnail's title is about 5 logical pixels tall, where font metrics round to
    // whole pixels: its lines may sit up to about 1.5 logical pixels from the editor's.
    assert_same_lines("the thumbnail", &editor, &lines, 0.06);
}

#[test]
fn the_slide_number_grows_with_text_scale() {
    let s = launched();
    let mut heights = Vec::new();
    for scale in [1.0f32, 2.0] {
        Theme::get(&s.app).set_text_scale(scale);
        let image = snapshot_component(&s.app, 1280.0, 800.0, SCALE).expect("render");
        let row = thumbnail(&s.app, 0, 3);
        // Inside the row's border, over the number column at its reading start.
        let (p, size) = (row.absolute_position(), row.size());
        let number = (
            ((p.x + 4.0) * SCALE) as u32,
            ((p.y + 4.0) * SCALE) as u32,
            ((p.x + 32.0) * SCALE) as u32,
            ((p.y + size.height - 4.0) * SCALE) as u32,
        );
        heights.push(ink_height(&image, number));
    }
    assert!(heights[0] > 0, "the slide number is drawn: {heights:?}");
    assert!(
        heights[1] as f32 >= heights[0] as f32 * 1.6,
        "the slide number did not grow with text scale: {heights:?} device pixels"
    );
}

#[test]
fn an_empty_placeholder_draws_nothing_in_its_thumbnail() {
    let s = launched();
    {
        let mut session = s.state.session.borrow_mut();
        session.document.add_slide("", "content");
        let slide = session.document.active_slide_mut().expect("the new slide");
        slide.add_element(text_element(
            "blank-body",
            ElementType::BodyText,
            "",
            60.0,
            180.0,
            880.0,
            300.0,
        ));
    }
    refresh(&s.app, &s.state);
    render(&s.app);

    let image = snapshot_component(&s.app, 1280.0, 800.0, SCALE).expect("render");
    let mini = thumbnail(&s.app, 3, 4)
        .query_descendants()
        .match_type_name("MiniSlide")
        .find_first()
        .expect("the empty slide's thumbnail");
    let (x0, y0, x1, y1) = slide_box(&image, device_area(&mini));
    // Inside the slide's rounded corners, where its own colour must be unbroken.
    let margin = (10.0 * SCALE) as u32;
    let drawn = (y0 + margin..y1 - margin)
        .flat_map(|y| (x0 + margin..x1 - margin).map(move |x| (x, y)))
        .filter(|&(x, y)| image.get_pixel(x, y).0 != PAPER)
        .count();
    assert_eq!(
        drawn, 0,
        "an empty placeholder drew {drawn} pixels in its thumbnail"
    );
}

/// At 2x text scale a thumbnail is one framed miniature: its frame sits on the
/// slide's own top edge. A frame drawn around a taller row, with the slide
/// inside it, is the double frame the thumbnails showed at this scale.
#[test]
fn a_thumbnail_at_two_times_text_is_one_frame_on_the_slide_edge() {
    let s = launched();
    Theme::get(&s.app).set_text_scale(2.0);
    refresh(&s.app, &s.state);
    render(&s.app);
    let count = s.state.session.borrow().document.slides.len();
    let image = snapshot_component(&s.app, 1280.0, 800.0, SCALE).expect("render");
    // Slide 2 is not selected, so its background is the plain surface colour.
    let button = device_area(&thumbnail(&s.app, 1, count));
    let mini = device_area(
        &thumbnail(&s.app, 1, count)
            .query_descendants()
            .match_type_name("MiniSlide")
            .find_first()
            .expect("the second thumbnail's slide"),
    );
    let (x0, y0, x1, _) = slide_box(&image, mini);
    let column = (x0 + x1) / 2;
    // Below the button's own 1 px border and inside its padding, where only the surface shows.
    let padding = button.1 + 5;
    let background = *image.get_pixel(column, padding);
    let frame_top = (padding..=y0)
        .find(|&y| *image.get_pixel(column, y) != background)
        .unwrap_or(y0);
    assert!(
        y0 - frame_top <= 2,
        "the frame starts {} device pixels above the slide, so a second frame surrounds it",
        y0 - frame_top
    );
}
