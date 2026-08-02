//! Command line tool for Loom Present.

use loom_present_core::{
    export_pdf, load_presentation, save_presentation, PresentationDocument, PresentationSession,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Present CLI");
        println!("Usage:");
        println!("  loom-present-cli create <output.loomdeck> <title>");
        println!("  loom-present-cli inspect <input.loomdeck>");
        println!("  loom-present-cli pdf <input.loomdeck> <output.pdf>");
        println!("  loom-present-cli validate <input.loomdeck>");
        println!("  loom-present-cli scene <input.loomdeck> <slide-index>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let title = args
                .get(3)
                .map(|s| s.as_str())
                .unwrap_or("Untitled Presentation");
            let mut doc = PresentationDocument::new("cli-deck", title);
            doc.add_slide("Agenda", "bullet-list");
            doc.add_slide("Summary", "summary");
            let bytes = save_presentation(&doc)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created presentation package: {out_path} (3 slides)");
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_presentation(&bytes)?;
            println!("Presentation: {}", doc.title);
            println!("Theme: {}", doc.theme);
            println!("Total Slides: {}", doc.len());
            for (idx, slide) in doc.slides.iter().enumerate() {
                println!("  Slide {}: {} [{}]", idx + 1, slide.title, slide.layout);
            }
        }
        "validate" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_presentation(&bytes)?;
            let issues = PresentationSession::new(doc).validate();
            if issues.is_empty() {
                println!("valid presentation");
            } else {
                for issue in &issues {
                    println!("slide {:?} element {:?}: {}", issue.slide_index, issue.element_id, issue.message);
                }
                return Err(format!("{} validation issue(s)", issues.len()));
            }
        }
        "scene" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let index = args
                .get(3)
                .ok_or("missing slide index")?
                .parse::<usize>()
                .map_err(|_| "slide index must be an integer".to_string())?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_presentation(&bytes)?;
            let scene = PresentationSession::new(doc)
                .scene(index)
                .ok_or("slide index is out of bounds")?;
            println!("slide: {}", scene.slide_id);
            println!("background: {}", scene.background);
            println!("transition: {:?}", scene.transition);
            for element in scene.elements {
                println!(
                    "{} {:?} {:.3},{:.3} {:.3}x{:.3}: {}",
                    element.id, element.element_type, element.x, element.y,
                    element.width, element.height, element.content
                );
            }
        }
        "pdf" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let out_path = args.get(3).ok_or("missing pdf output path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_presentation(&bytes)?;
            let pdf_bytes = export_pdf(&doc);
            std::fs::write(out_path, pdf_bytes).map_err(|e| format!("write pdf error: {e}"))?;
            println!("Exported PDF: {out_path}");
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
