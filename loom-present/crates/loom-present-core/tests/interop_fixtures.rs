//! Gate 11 fixture producer for Present: builds a representative deck through
//! the real model and exporters and writes `present.pptx` and `present.pdf`
//! where PowerPoint and LibreOffice can open them.
//!
//! Run with:
//! `LOOM_INTEROP_OUT=<dir> cargo test -p loom-present-core --test interop_fixtures -- --ignored`
//!
//! The scripts under `loom-present/tests/interop/` and the LibreOffice
//! conversion check read these files; the expected text is fixed there.

use loom_present_core::{
    export_pdf, export_pptx, ElementType, ImageAsset, PresentationDocument, PresentationSession,
    Slide, SlideElement, TransitionKind,
};
use std::path::PathBuf;

fn element(
    id: &str,
    kind: ElementType,
    content: &str,
    rect: (f32, f32, f32, f32),
    rotation: f32,
) -> SlideElement {
    SlideElement {
        id: id.to_string(),
        element_type: kind,
        content: content.to_string(),
        x: rect.0,
        y: rect.1,
        width: rect.2,
        height: rect.3,
        rotation_deg: rotation,
        action: None,
    }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut image = image::RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([200, (x % 251) as u8, (y % 251) as u8, 255]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

fn fixture_session() -> PresentationSession {
    let mut document = PresentationDocument::new("interop", "Interop Deck");
    document.slides.clear();

    let mut one = Slide::new("s1", "Welcome", "cover");
    one.add_element(element(
        "t1",
        ElementType::Title,
        "Q3 & Q4 Plan",
        (100.0, 200.0, 800.0, 100.0),
        0.0,
    ));
    one.add_element(element(
        "sub1",
        ElementType::Subtitle,
        "Prepared by Finance \u{2013} Caf\u{e9} \u{201c}review\u{201d}",
        (100.0, 320.0, 800.0, 50.0),
        0.0,
    ));
    one.speaker_notes = "Open warmly. Mention the budget.".to_string();

    let mut two = Slide::new("s2", "Numbers", "content");
    two.add_element(element(
        "t2",
        ElementType::Title,
        "Results",
        (50.0, 30.0, 900.0, 80.0),
        0.0,
    ));
    two.add_element(element(
        "body2",
        ElementType::BodyText,
        "First line\nSecond line",
        (60.0, 140.0, 500.0, 300.0),
        0.0,
    ));
    two.add_element(element(
        "rect2",
        ElementType::ShapeRectangle,
        "Box label",
        (600.0, 140.0, 300.0, 120.0),
        15.0,
    ));
    two.add_element(element(
        "circle2",
        ElementType::ShapeCircle,
        "Circle label",
        (620.0, 300.0, 140.0, 140.0),
        0.0,
    ));
    two.add_element(element(
        "stat2",
        ElementType::StatCard,
        "42%",
        (780.0, 300.0, 160.0, 120.0),
        0.0,
    ));
    two.speaker_notes = "Walk through the numbers slowly.".to_string();

    let mut three = Slide::new("s3", "Picture", "content");
    three.add_element(element(
        "t3",
        ElementType::Title,
        "Logo",
        (50.0, 30.0, 900.0, 80.0),
        0.0,
    ));

    let mut four = Slide::new("s4", "Closer", "content");
    four.bg_color = "#16181d".to_string();
    four.add_element(element(
        "t4",
        ElementType::Title,
        "Thank you",
        (100.0, 220.0, 800.0, 100.0),
        0.0,
    ));

    document.slides = vec![one, two, three, four];
    let mut session = PresentationSession::new(document);
    assert!(session.document.select_slide(2));
    let asset = ImageAsset::from_bytes("logo.png", png(120, 60)).expect("png asset");
    session.insert_picture(asset).expect("insert picture");
    assert!(session.set_transition("s1", TransitionKind::Dissolve));
    assert!(session.set_transition("s2", TransitionKind::Push));
    assert!(session.set_transition("s4", TransitionKind::Morph));
    session
}

#[test]
#[ignore = "writes interop fixtures; set LOOM_INTEROP_OUT and run with --ignored"]
fn write_present_interop_fixtures() {
    let dir = PathBuf::from(std::env::var("LOOM_INTEROP_OUT").expect("set LOOM_INTEROP_OUT"));
    std::fs::create_dir_all(&dir).unwrap();
    let session = fixture_session();
    std::fs::write(
        dir.join("present.pptx"),
        export_pptx(&session).expect("pptx"),
    )
    .unwrap();
    std::fs::write(dir.join("present.pdf"), export_pdf(&session.document)).unwrap();
}
