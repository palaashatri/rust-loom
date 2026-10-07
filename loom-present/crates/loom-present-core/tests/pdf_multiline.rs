//! A body text element with several lines must reach the PDF as several
//! lines. A line feed inside one PDF string draws no line break, so the
//! export used to run "First line" and "Second line" together on one baseline.

use loom_present_core::{export_pdf, ElementType, PresentationDocument, Slide, SlideElement};

fn body(content: &str) -> SlideElement {
    SlideElement {
        id: "body".to_string(),
        element_type: ElementType::BodyText,
        content: content.to_string(),
        x: 60.0,
        y: 140.0,
        width: 500.0,
        height: 300.0,
        rotation_deg: 0.0,
        action: None,
    }
}

/// `(text)` strings with the text-matrix baseline `y` that precedes them.
fn lines(pdf: &[u8]) -> Vec<(f32, String)> {
    let text: String = pdf.iter().map(|&byte| char::from(byte)).collect();
    let mut found = Vec::new();
    for op in text.lines().filter(|line| line.contains(" Tm (")) {
        let tm = op.split(" Tm (").next().unwrap();
        let y: f32 = tm.split_whitespace().last().unwrap().parse().unwrap();
        let start = op.find(" Tm (").unwrap() + 5;
        let end = op[start..].find(") Tj").unwrap() + start;
        found.push((y, op[start..end].to_string()));
    }
    found
}

#[test]
fn multi_line_body_text_is_drawn_on_separate_baselines() {
    let mut document = PresentationDocument::new("multi", "Multi");
    document.slides.clear();
    let mut slide = Slide::new("s1", "One", "content");
    slide.add_element(body("First line\nSecond line\n\nAfter a blank"));
    document.slides.push(slide);

    let drawn = lines(&export_pdf(&document));
    let find = |needle: &str| {
        drawn
            .iter()
            .find(|(_, text)| text == needle)
            .unwrap_or_else(|| panic!("{needle:?} is drawn as its own line: {drawn:?}"))
            .0
    };
    let (first, second, third) = (
        find("First line"),
        find("Second line"),
        find("After a blank"),
    );
    assert!(second > first, "second line sits below the first");
    assert!(
        third - second > second - first,
        "the blank line leaves a gap"
    );
}
