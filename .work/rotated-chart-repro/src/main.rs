use std::{env, fs, path::PathBuf};

use loom_objects::chart::{Chart, ChartType};
use loom_present_core::{PresentationDocument, Slide, SlideElement, export_pdf, save_presentation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::args().nth(1).unwrap_or_else(|| ".work/rotated-chart-repro/out".into()));
    fs::create_dir_all(&out_dir)?;

    for (name, rotation) in [("upright", 0.0), ("rotated", 30.0)] {
        let mut chart = Chart::new("chart-repro", ChartType::Column);
        chart.title = Some("Quarterly net revenue".into());
        chart.categories = vec![
            "First quarter".into(),
            "Second quarter".into(),
            "Third quarter".into(),
            "Fourth quarter".into(),
        ];
        chart.add_series("Revenue", vec![12.0, 24.0, 16.0, 31.0], None);

        let mut element = SlideElement::new_chart("chart-object", 180.0, 120.0, 620.0, 340.0, chart);
        element.rotation_deg = rotation;

        let mut slide = Slide::new("slide-repro", "Chart repro", "blank");
        slide.add_element(element);
        let mut document = PresentationDocument::new("deck-repro", "Rotated chart text repro");
        document.slides.clear();
        document.slides.push(slide);
        document.active_index = 0;

        fs::write(out_dir.join(format!("{name}.loomdeck")), save_presentation(&document)?)?;
        fs::write(out_dir.join(format!("{name}.pdf")), export_pdf(&document))?;
    }
    Ok(())
}
