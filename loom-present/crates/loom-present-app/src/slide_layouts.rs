//! What a new slide, and each slide layout, starts with: empty text
//! placeholders.
//!
//! A placeholder is a real, empty text element. The canvas shows a faint prompt
//! ("Click to add title") over it while the slide is being edited, but the
//! prompt is never part of the deck: it is not saved, exported or drawn in a
//! thumbnail. Filler text typed into a slide on the user's behalf would be all
//! of those things, which is why no layout ever supplies any.

use loom_present_core::{
    ElementType, PresentationDocument, PresentationSession, Slide, SlideElement,
};

/// Internal name of a slide nobody has titled yet. It names the slide for
/// screen readers and the strip; it is not placed on the slide.
pub(crate) const UNTITLED_SLIDE: &str = "Untitled Slide";

/// Where one placeholder sits, in slide points.
struct Spot {
    kind: ElementType,
    id_stem: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

const fn spot(
    kind: ElementType,
    id_stem: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Spot {
    Spot {
        kind,
        id_stem,
        x,
        y,
        width,
        height,
    }
}

/// Layout name for a slide a user creates with the plain New Slide command.
pub(crate) const DEFAULT_LAYOUT: &str = "content";

/// Layout name for each entry of the inspector's Title / Content / 2 Col / Image
/// chooser.
pub(crate) fn layout_for_choice(index: i32) -> &'static str {
    match index {
        0 => "cover",
        2 => "two-column",
        3 => "image-text",
        _ => DEFAULT_LAYOUT,
    }
}

/// The chooser entry that names `layout`: the inverse of `layout_for_choice`.
/// A layout the chooser does not offer is shown as Content, the default.
pub(crate) fn choice_for_layout(layout: &str) -> i32 {
    match layout {
        "cover" => 0,
        "two-column" => 2,
        "image-text" => 3,
        _ => 1,
    }
}

/// The placeholders a layout offers, in reading order. The Image layout keeps
/// the left half free: a picture is a real asset the user inserts, so it has no
/// empty placeholder of its own.
fn spots(layout: &str) -> Vec<Spot> {
    let title = spot(ElementType::Title, "title", 80.0, 70.0, 820.0, 90.0);
    match layout {
        "cover" => vec![
            spot(ElementType::Title, "title", 100.0, 190.0, 800.0, 110.0),
            spot(ElementType::Subtitle, "subtitle", 100.0, 320.0, 800.0, 60.0),
        ],
        "two-column" => vec![
            title,
            spot(ElementType::BodyText, "body", 80.0, 190.0, 390.0, 260.0),
            spot(ElementType::BodyText, "body", 490.0, 190.0, 390.0, 260.0),
        ],
        "image-text" => vec![
            title,
            spot(ElementType::BodyText, "body", 500.0, 190.0, 400.0, 260.0),
        ],
        _ => vec![
            title,
            spot(ElementType::BodyText, "body", 82.0, 190.0, 760.0, 170.0),
        ],
    }
}

fn unused_id(slide: &Slide, stem: &str) -> String {
    (1..)
        .map(|n| format!("{}-{stem}-{n}", slide.id))
        .find(|id| slide.elements.iter().all(|element| &element.id != id))
        .expect("an unbounded range always yields an unused id")
}

/// Gives `slide` the placeholders of `layout`: the nth text element of a kind
/// takes the nth spot of that kind when it is still empty, and a spot with no
/// element gets a new empty one. Elements that hold content are never moved,
/// resized or removed.
pub(crate) fn apply_layout(slide: &mut Slide, layout: &str) {
    slide.layout = layout.into();
    let mut seen: Vec<(ElementType, usize)> = Vec::new();
    for spot in spots(layout) {
        let nth = seen
            .iter()
            .find(|(kind, _)| *kind == spot.kind)
            .map_or(0, |(_, count)| *count);
        match seen.iter_mut().find(|(kind, _)| *kind == spot.kind) {
            Some((_, count)) => *count += 1,
            None => seen.push((spot.kind.clone(), 1)),
        }
        let existing = slide
            .elements
            .iter_mut()
            .filter(|element| element.element_type == spot.kind)
            .nth(nth);
        match existing {
            Some(element) if element.content.trim().is_empty() => {
                element.x = spot.x;
                element.y = spot.y;
                element.width = spot.width;
                element.height = spot.height;
            }
            Some(_) => {}
            None => {
                let id = unused_id(slide, spot.id_stem);
                slide.add_element(SlideElement {
                    id,
                    element_type: spot.kind,
                    content: String::new(),
                    x: spot.x,
                    y: spot.y,
                    width: spot.width,
                    height: spot.height,
                    rotation_deg: 0.0,
                    action: None,
                });
            }
        }
    }
}

/// Appends a slide with the default layout's empty placeholders and makes it
/// the active slide.
pub(crate) fn add_blank_slide(document: &mut PresentationDocument) {
    document.add_slide(UNTITLED_SLIDE, DEFAULT_LAYOUT);
    if let Some(slide) = document.active_slide_mut() {
        apply_layout(slide, DEFAULT_LAYOUT);
    }
}

/// What the "New from Template" chooser offers, in card order. Each template
/// is a set of slides with empty placeholders; none carries colours, fonts or
/// backgrounds, because a deck does not store any.
pub(crate) const TEMPLATE_NAMES: [&str; 4] = [
    "Blank",
    "Title and Content",
    "Two Columns",
    "Image and Text",
];

/// The deck a template creates, built on `blank` (the app's empty one-slide
/// deck). An unknown index gives the blank deck.
pub(crate) fn template_session(blank: PresentationSession, choice: i32) -> PresentationSession {
    let mut session = blank;
    match choice {
        1 => {
            if let Some(slide) = session.document.active_slide_mut() {
                apply_layout(slide, "cover");
            }
            add_blank_slide(&mut session.document);
        }
        2 | 3 => {
            if let Some(slide) = session.document.active_slide_mut() {
                apply_layout(slide, layout_for_choice(choice));
            }
        }
        _ => {}
    }
    session.document.active_index = 0;
    session
}
