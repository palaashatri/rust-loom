//! A deck's PDF must look like the slide editor: a 16:9 page, titles centred in
//! their boxes, labels centred vertically, and text wrapped to the box it sits
//! in. These tests read the PDF's own content stream, so the positions they
//! check are the positions a reader draws.

use loom_pdf::{text_width_pt, TextStyle};
use loom_present_core::{export_pdf, ElementType, PresentationDocument, Slide, SlideElement};

/// One text run as the page draws it: its text, font size and bold face, and its
/// baseline origin in page points (bottom-left origin, as PDF user space has it).
#[derive(Debug)]
struct Run {
    text: String,
    size: f32,
    bold: bool,
    x: f32,
    y: f32,
}

fn content(pdf: &[u8]) -> String {
    loom_pdf::inspect::readable_content(pdf).expect("readable PDF")
}

/// Every text run. Each is drawn as `q a b c d e f cm ... BT 1 0 0 -1 x y Tm (text) Tj ET Q`,
/// so its page position is the matrix applied to `(x, y)`.
fn runs(pdf: &[u8]) -> Vec<Run> {
    content(pdf)
        .lines()
        .filter(|line| line.contains(" Tm ("))
        .map(|line| {
            let t: Vec<&str> = line.split_whitespace().collect();
            let m: Vec<f32> = t[1..7].iter().map(|n| n.parse().expect("matrix")).collect();
            let tm = t.iter().position(|token| *token == "Tm").expect("Tm");
            let (lx, ly): (f32, f32) = (t[tm - 2].parse().unwrap(), t[tm - 1].parse().unwrap());
            let tf = t.iter().position(|token| *token == "Tf").expect("Tf");
            let start = line.find(" Tm (").expect("Tm text") + " Tm (".len();
            let end = line.rfind(") Tj").expect("Tj");
            Run {
                text: line[start..end].to_string(),
                size: t[tf - 1].parse().expect("size"),
                bold: t[tf - 2] == "/F2",
                x: m[0] * lx + m[2] * ly + m[4],
                y: m[1] * lx + m[3] * ly + m[5],
            }
        })
        .collect()
}

/// Page points per scene unit: the deck's 1000-unit slide on a 960 pt page.
const POINTS_PER_UNIT: f32 = 0.96;
/// Editor text: 1 px of its 840 px reference slide is 1000/840 scene units.
const UNITS_PER_PX: f32 = 1000.0 / 840.0;
/// The editor's display size (24 px) and body size (14 px), in scene units.
const DISPLAY_UNITS: f32 = 24.0 * UNITS_PER_PX;
const BODY_UNITS: f32 = 14.0 * UNITS_PER_PX;

fn element(
    id: &str,
    element_type: ElementType,
    content: &str,
    (x, y, width, height): (f32, f32, f32, f32),
) -> SlideElement {
    SlideElement {
        id: id.to_string(),
        element_type,
        content: content.to_string(),
        x,
        y,
        width,
        height,
        rotation_deg: 0.0,
        action: None,
    }
}

fn deck_with(elements: Vec<SlideElement>) -> PresentationDocument {
    let mut slide = Slide::new("s1", "One", "content");
    for element in elements {
        slide.add_element(element);
    }
    let mut document = PresentationDocument::new("deck", "Deck");
    document.slides.clear();
    document.slides.push(slide);
    document
}

#[test]
fn the_page_has_the_slides_sixteen_by_nine_shape() {
    let pdf = export_pdf(&deck_with(Vec::new()));
    // The page box is in the object dictionary, not the (compressed) content.
    let text = String::from_utf8_lossy(&pdf).into_owned();
    let at = text.find("/MediaBox [").expect("a page box") + "/MediaBox [".len();
    let numbers: Vec<f32> = text[at..]
        .split(']')
        .next()
        .expect("the box")
        .split_whitespace()
        .map(|n| n.parse().expect("a number"))
        .collect();
    let (width, height) = (numbers[2], numbers[3]);
    assert!(
        (width / height - 16.0 / 9.0).abs() < 0.001,
        "the page is {width} by {height} points, not 16:9"
    );
}

#[test]
fn a_title_is_centred_in_its_box_and_vertically_centred() {
    let title = element(
        "title",
        ElementType::Title,
        "Quarterly results",
        (100.0, 80.0, 800.0, 160.0),
    );
    let pdf = export_pdf(&deck_with(vec![title]));
    let run = runs(&pdf)
        .into_iter()
        .find(|run| run.text == "Quarterly results")
        .expect("the title is drawn");

    let style = TextStyle {
        size_pt: run.size * POINTS_PER_UNIT,
        bold: run.bold,
        ..TextStyle::default()
    };
    let centre_x = run.x + text_width_pt(&run.text, &style) / 2.0;
    let expected_x = (100.0 + 800.0 / 2.0) * POINTS_PER_UNIT;
    assert!(
        (centre_x - expected_x).abs() < 0.02 * 800.0 * POINTS_PER_UNIT,
        "the title is centred at {centre_x}, the box at {expected_x}"
    );
    // A capital's visual middle sits about 0.36 em above the baseline.
    let visual_y = run.y + 0.36 * style.size_pt;
    let expected_y = 540.0 - (80.0 + 160.0 / 2.0) * POINTS_PER_UNIT;
    assert!(
        (visual_y - expected_y).abs() < 0.25 * style.size_pt,
        "the title's middle is at {visual_y}, the box's middle at {expected_y}"
    );
}

#[test]
fn a_shape_label_is_centred_vertically_in_its_shape() {
    let shape = element(
        "shape",
        ElementType::ShapeRectangle,
        "Label",
        (100.0, 100.0, 400.0, 200.0),
    );
    let pdf = export_pdf(&deck_with(vec![shape]));
    let run = runs(&pdf)
        .into_iter()
        .find(|run| run.text == "Label")
        .expect("the label is drawn");
    let visual_y = run.y + 0.36 * run.size * POINTS_PER_UNIT;
    let expected_y = 540.0 - (100.0 + 200.0 / 2.0) * POINTS_PER_UNIT;
    assert!(
        (visual_y - expected_y).abs() < 0.25 * run.size * POINTS_PER_UNIT,
        "the label's middle is at {visual_y}, the shape's middle at {expected_y}"
    );
}

#[test]
fn body_text_wraps_inside_its_box_as_it_does_on_the_slide() {
    let text = "Revenue grew in every region this quarter, and the northern team led the gains.";
    let body = element(
        "body",
        ElementType::BodyText,
        text,
        (60.0, 140.0, 300.0, 300.0),
    );
    let pdf = export_pdf(&deck_with(vec![body]));
    let drawn = runs(&pdf);
    assert!(drawn.len() > 1, "the text takes several lines: {drawn:?}");

    // The editor's text box is the element less 12 px on each side, horizontally.
    let box_points = (300.0 - 2.0 * 12.0 * UNITS_PER_PX) * POINTS_PER_UNIT;
    for run in &drawn {
        let style = TextStyle {
            size_pt: run.size * POINTS_PER_UNIT,
            ..TextStyle::default()
        };
        let width = text_width_pt(&run.text, &style);
        assert!(
            width <= box_points + 0.5,
            "{:?} is {width} points wide, the box is {box_points}",
            run.text
        );
    }
    let printed: Vec<&str> = drawn
        .iter()
        .flat_map(|run| run.text.split_whitespace())
        .collect();
    for word in text.split_whitespace() {
        assert!(printed.contains(&word), "{word:?} is printed: {printed:?}");
    }
}

#[test]
fn titles_and_body_text_use_the_editor_type_sizes() {
    let title = element(
        "t",
        ElementType::Title,
        "Heading",
        (60.0, 40.0, 880.0, 120.0),
    );
    let body = element(
        "b",
        ElementType::BodyText,
        "Body",
        (60.0, 200.0, 880.0, 200.0),
    );
    let pdf = export_pdf(&deck_with(vec![title, body]));
    let drawn = runs(&pdf);
    let size_of = |text: &str| {
        drawn
            .iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("{text:?} is drawn"))
            .size
    };
    assert!((size_of("Heading") - DISPLAY_UNITS).abs() < 0.01);
    assert!((size_of("Body") - BODY_UNITS).abs() < 0.01);
}
