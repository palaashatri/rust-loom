use loom_objects::{Chart, ChartType};
use loom_present_core::{export_pdf, PresentationDocument, Slide, SlideElement};

#[test]
fn horizontal_bar_pdf_places_categories_in_rows_and_values_on_both_sides_of_zero() {
    let mut chart = Chart::new("chart", ChartType::Bar);
    chart.categories = vec!["First".into(), "Second".into()];
    chart.add_series("One", vec![20.0, -10.0], None);
    chart.add_series("Two", vec![10.0, -5.0], None);
    let mut slide = Slide::new("slide", "", "blank");
    slide.add_element(SlideElement::new_chart("chart", 0.0, 0.0, 400.0, 260.0, chart));
    let mut doc = PresentationDocument::new("deck", "Deck");
    doc.slides = vec![slide];
    let content = loom_pdf::inspect::readable_content(&export_pdf(&doc)).unwrap();
    let mut bars = Vec::new();
    for line in content.lines().filter(|line| line.contains(" re f Q")) {
        if !(line.contains("0.169 0.341 0.604 rg") || line.contains("0.929 0.490 0.192 rg")) {
            continue;
        }
        let words: Vec<_> = line.split_whitespace().collect();
        let pos = words.iter().position(|word| *word == "re").unwrap();
        let numbers: Vec<f32> = words[pos - 4..pos].iter().map(|v| v.parse().unwrap()).collect();
        bars.push((numbers[0], numbers[1], numbers[2], numbers[3]));
    }
    assert_eq!(bars.len(), 4, "expected two bars per category: {content}");
    let [a, b, c, d] = bars.as_slice() else { unreachable!() };
    assert!(a.2 > a.3 && b.2 > b.3, "bars must extend horizontally: {bars:?}");
    assert!(a.1 < c.1 && c.1 < b.1 && b.1 < d.1, "series must share category rows: {bars:?}");
    assert!((a.0 - c.0).abs() < 0.1 && (b.0 + b.2 - a.0).abs() < 0.1 && (d.0 + d.2 - c.0).abs() < 0.1, "positive and negative bars must meet at zero: {bars:?}");
    assert!((a.2 / c.2 - 2.0).abs() < 0.01 && (b.2 / d.2 - 2.0).abs() < 0.01, "bar lengths must reflect values: {bars:?}");
    assert!(content.contains("(First)") && content.contains("(Second)"));
}

#[test]
fn horizontal_bar_pdf_handles_all_zero_values() {
    let mut chart = Chart::new("zero", ChartType::Bar);
    chart.title = Some("Zeros".into());
    chart.categories = vec!["Empty".into()];
    chart.add_series("Values", vec![0.0], None);
    let mut slide = Slide::new("slide", "", "blank");
    slide.add_element(SlideElement::new_chart("chart", 0.0, 0.0, 400.0, 260.0, chart));
    let mut doc = PresentationDocument::new("deck", "Deck");
    doc.slides = vec![slide];
    let content = loom_pdf::inspect::readable_content(&export_pdf(&doc)).unwrap();
    assert!(content.contains("(Zeros)") && content.contains("(Empty)"));
    let bar = content.lines().find(|line| line.contains("0.169 0.341 0.604 rg") && line.contains(" re f Q")).unwrap();
    let words: Vec<_> = bar.split_whitespace().collect();
    let pos = words.iter().position(|word| *word == "re").unwrap();
    let geometry: Vec<f32> = words[pos - 4..pos].iter().map(|v| v.parse().unwrap()).collect();
    assert!(geometry.iter().all(|v| v.is_finite()));
    assert_eq!(geometry[2], 0.0, "zero value must not draw a nonzero bar");
}

#[test]
fn horizontal_bar_pdf_handles_large_finite_values() {
    let mut chart = Chart::new("large", ChartType::Bar);
    chart.categories = vec!["Negative".into(), "Positive".into()];
    chart.add_series("Values", vec![-1e308, 1e308], None);
    let mut slide = Slide::new("slide", "", "blank");
    slide.add_element(SlideElement::new_chart("chart", 0.0, 0.0, 400.0, 260.0, chart));
    let mut doc = PresentationDocument::new("deck", "Deck");
    doc.slides = vec![slide];
    let content = loom_pdf::inspect::readable_content(&export_pdf(&doc)).unwrap();
    let bars: Vec<_> = content.lines().filter(|line| line.contains("0.169 0.341 0.604 rg") && line.contains(" re f Q")).collect();
    assert_eq!(bars.len(), 2);
    for bar in bars {
        let words: Vec<_> = bar.split_whitespace().collect();
        let pos = words.iter().position(|word| *word == "re").unwrap();
        let width: f32 = words[pos - 2].parse().unwrap();
        assert!(width > 50.0, "finite values must produce visible bars: {bar}");
    }
}
