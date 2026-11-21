//! Loom Present command-line interface.
//!
//! Headless commands for creating, saving, inspecting, searching, and exporting
//! `.loomdeck` presentations and `.pptx` files.

use loom_present_core::{
    export_pdf, export_pptx, import_pptx, inspect_pptx_unsupported, load_presentation,
    save_presentation, PresentationDocument, PresentationSession,
};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    match run_with(&args, &mut stdout, &mut stderr) {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}

fn print_usage(out: &mut impl Write) -> Result<(), String> {
    let usage = "\
usage: loom-present <command> [args]
  create <out.loomdeck> [title]
  info | inspect <input.loomdeck|.pptx>
  validate <input.loomdeck>
  list-slides <input.loomdeck>
  scene <input.loomdeck> <slide-index>
  pdf | export-pdf <input.loomdeck> <out.pdf>
  export-pptx <input.loomdeck> <out.pptx>
  import-pptx <input.pptx> <out.loomdeck>
  search <input.loomdeck> <query>
  replace <input.loomdeck> <find> <replace> <out.loomdeck>
  help | -h | --help";
    writeln!(out, "{usage}").map_err(|error| error.to_string())
}

fn run_with(
    args: &[String],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<i32, String> {
    if args.len() < 2 {
        print_usage(stderr)?;
        return Ok(2);
    }

    match args[1].as_str() {
        "--help" | "-h" | "help" => {
            print_usage(stdout)?;
            return Ok(0);
        }
        "create" => {
            let out_path = args.get(2).ok_or("create requires <out.loomdeck>")?;
            let title = args
                .get(3)
                .map(String::as_str)
                .unwrap_or("Untitled Presentation");
            cmd_create(out_path, title, stdout)?;
        }
        "info" | "inspect" => {
            let in_path = args
                .get(2)
                .ok_or("inspect requires <input.loomdeck|.pptx>")?;
            cmd_inspect(in_path, stdout)?;
        }
        "validate" => {
            let in_path = args.get(2).ok_or("validate requires <input.loomdeck>")?;
            cmd_validate(in_path, stdout)?;
        }
        "list-slides" => {
            let in_path = args.get(2).ok_or("list-slides requires <input.loomdeck>")?;
            cmd_list_slides(in_path, stdout)?;
        }
        "scene" => {
            let in_path = args.get(2).ok_or("scene requires <input.loomdeck>")?;
            let index_str = args.get(3).ok_or("scene requires <slide-index>")?;
            let index = index_str
                .parse::<usize>()
                .map_err(|_| "slide index must be an integer".to_string())?;
            cmd_scene(in_path, index, stdout)?;
        }
        "pdf" | "export-pdf" => {
            let in_path = args.get(2).ok_or("export-pdf requires <input.loomdeck>")?;
            let out_path = args.get(3).ok_or("export-pdf requires <out.pdf>")?;
            cmd_export_pdf(in_path, out_path, stdout)?;
        }
        "export-pptx" => {
            let in_path = args.get(2).ok_or("export-pptx requires <input.loomdeck>")?;
            let out_path = args.get(3).ok_or("export-pptx requires <out.pptx>")?;
            cmd_export_pptx(in_path, out_path, stdout)?;
        }
        "import-pptx" => {
            let in_path = args.get(2).ok_or("import-pptx requires <input.pptx>")?;
            let out_path = args.get(3).ok_or("import-pptx requires <out.loomdeck>")?;
            cmd_import_pptx(in_path, out_path, stdout, stderr)?;
        }
        "search" => {
            let in_path = args.get(2).ok_or("search requires <input.loomdeck>")?;
            let query = args.get(3).ok_or("search requires <query>")?;
            cmd_search(in_path, query, stdout)?;
        }
        "replace" => {
            let in_path = args.get(2).ok_or("replace requires <input.loomdeck>")?;
            let find = args.get(3).ok_or("replace requires <find>")?;
            let replace = args.get(4).ok_or("replace requires <replace>")?;
            let out_path = args.get(5).ok_or("replace requires <out.loomdeck>")?;
            cmd_replace(in_path, find, replace, out_path, stdout)?;
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(0)
}

fn read_deck(path: &str) -> Result<PresentationDocument, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read error: {e}"))?;
    load_presentation(&bytes)
}

fn write_deck(path: &str, doc: &PresentationDocument) -> Result<(), String> {
    let bytes = save_presentation(doc)?;
    std::fs::write(path, bytes).map_err(|e| format!("write error: {e}"))
}

fn cmd_create(out_path: &str, title: &str, stdout: &mut impl Write) -> Result<(), String> {
    let mut doc = PresentationDocument::new("cli-deck", title);
    doc.add_slide("Agenda", "bullet-list");
    doc.add_slide("Summary", "summary");
    write_deck(out_path, &doc)?;
    writeln!(
        stdout,
        "Created presentation package: {out_path} ({} slides)",
        doc.len()
    )
    .map_err(|e| e.to_string())
}

fn cmd_inspect(in_path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
    let doc = if in_path.to_ascii_lowercase().ends_with(".pptx") {
        import_pptx(&bytes)?
    } else {
        load_presentation(&bytes)?
    };
    writeln!(stdout, "Presentation: {}", doc.title).map_err(|e| e.to_string())?;
    writeln!(stdout, "Theme: {}", doc.theme).map_err(|e| e.to_string())?;
    writeln!(stdout, "Total Slides: {}", doc.len()).map_err(|e| e.to_string())?;
    for (idx, slide) in doc.slides.iter().enumerate() {
        writeln!(
            stdout,
            "  Slide {}: {} [{}]",
            idx + 1,
            slide.title,
            slide.layout
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_validate(in_path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_deck(in_path)?;
    let issues = PresentationSession::new(doc).validate();
    if issues.is_empty() {
        writeln!(stdout, "valid presentation").map_err(|e| e.to_string())?;
        Ok(())
    } else {
        for issue in &issues {
            writeln!(
                stdout,
                "slide {:?} element {:?}: {}",
                issue.slide_index, issue.element_id, issue.message
            )
            .map_err(|e| e.to_string())?;
        }
        Err(format!("{} validation issue(s)", issues.len()))
    }
}

fn cmd_list_slides(in_path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_deck(in_path)?;
    for (idx, slide) in doc.slides.iter().enumerate() {
        writeln!(
            stdout,
            "Slide {}: {} [{}] ({} elements)",
            idx + 1,
            slide.title,
            slide.layout,
            slide.elements.len()
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_scene(in_path: &str, index: usize, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_deck(in_path)?;
    let scene = PresentationSession::new(doc)
        .scene(index)
        .ok_or("slide index is out of bounds")?;
    writeln!(stdout, "slide: {}", scene.slide_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "background: {}", scene.background).map_err(|e| e.to_string())?;
    writeln!(stdout, "transition: {:?}", scene.transition).map_err(|e| e.to_string())?;
    for element in scene.elements {
        writeln!(
            stdout,
            "{} {:?} {:.3},{:.3} {:.3}x{:.3}: {}",
            element.id,
            element.element_type,
            element.x,
            element.y,
            element.width,
            element.height,
            element.content
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_export_pdf(in_path: &str, out_path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_deck(in_path)?;
    let pdf_bytes = export_pdf(&doc);
    std::fs::write(out_path, pdf_bytes).map_err(|e| format!("write pdf error: {e}"))?;
    writeln!(stdout, "Exported PDF: {out_path}").map_err(|e| e.to_string())
}

fn cmd_export_pptx(in_path: &str, out_path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_deck(in_path)?;
    let session = PresentationSession::new(doc);
    let pptx_bytes = export_pptx(&session)?;
    std::fs::write(out_path, pptx_bytes).map_err(|e| format!("write pptx error: {e}"))?;
    writeln!(stdout, "Exported PPTX: {out_path}").map_err(|e| e.to_string())
}

fn cmd_import_pptx(
    in_path: &str,
    out_path: &str,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), String> {
    let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
    let warnings = inspect_pptx_unsupported(&bytes)?;
    for warning in &warnings {
        writeln!(stderr, "warning: {warning}").map_err(|e| e.to_string())?;
    }
    let doc = import_pptx(&bytes)?;
    write_deck(out_path, &doc)?;
    writeln!(
        stdout,
        "Imported PPTX: {in_path} -> {out_path} ({} slides)",
        doc.len()
    )
    .map_err(|e| e.to_string())
}

fn cmd_search(in_path: &str, query: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_deck(in_path)?;
    let needle = query.to_lowercase();
    let mut matches = 0usize;
    for (idx, slide) in doc.slides.iter().enumerate() {
        if slide.title.to_lowercase().contains(&needle) {
            writeln!(stdout, "slide {} title: {}", idx + 1, slide.title)
                .map_err(|e| e.to_string())?;
            matches += 1;
        }
        if slide.speaker_notes.to_lowercase().contains(&needle) {
            writeln!(stdout, "slide {} notes: {}", idx + 1, slide.speaker_notes)
                .map_err(|e| e.to_string())?;
            matches += 1;
        }
        for element in &slide.elements {
            if element.content.to_lowercase().contains(&needle) {
                writeln!(
                    stdout,
                    "slide {} element {}: {}",
                    idx + 1,
                    element.id,
                    element.content
                )
                .map_err(|e| e.to_string())?;
                matches += 1;
            }
        }
    }
    if matches == 0 {
        writeln!(stdout, "no matches for \"{query}\"").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_replace(
    in_path: &str,
    find: &str,
    replace: &str,
    out_path: &str,
    stdout: &mut impl Write,
) -> Result<(), String> {
    if find.is_empty() {
        return Err("find string must not be empty".to_string());
    }
    let mut doc = read_deck(in_path)?;
    let mut count = 0usize;
    for slide in &mut doc.slides {
        let title_hits = slide.title.matches(find).count();
        if title_hits > 0 {
            slide.title = slide.title.replace(find, replace);
            count += title_hits;
        }
        let note_hits = slide.speaker_notes.matches(find).count();
        if note_hits > 0 {
            slide.speaker_notes = slide.speaker_notes.replace(find, replace);
            count += note_hits;
        }
        for element in &mut slide.elements {
            let elem_hits = element.content.matches(find).count();
            if elem_hits > 0 {
                element.content = element.content.replace(find, replace);
                count += elem_hits;
            }
        }
    }
    write_deck(out_path, &doc)?;
    writeln!(stdout, "Replaced {count} occurrence(s) -> {out_path}").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "loom-present-cli-test-{}-{}-{id}",
            std::process::id(),
            name
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn run_cli(args: &[&str]) -> (Result<i32, String>, String, String) {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = run_with(&owned, &mut stdout, &mut stderr);
        (
            res,
            String::from_utf8_lossy(&stdout).into_owned(),
            String::from_utf8_lossy(&stderr).into_owned(),
        )
    }

    #[test]
    fn help_and_no_args() {
        let (res, _out, err) = run_cli(&["loom-present"]);
        assert_eq!(res, Ok(2));
        assert!(err.contains("usage: loom-present"));

        for flag in ["--help", "-h", "help"] {
            let (res, out, err) = run_cli(&["loom-present", flag]);
            assert_eq!(res, Ok(0));
            assert!(out.contains("usage: loom-present"));
            assert!(err.is_empty());
        }
    }

    #[test]
    fn unknown_command_and_missing_args_return_err() {
        let (res, _, _) = run_cli(&["loom-present", "bogus"]);
        assert!(res.unwrap_err().contains("unknown command"));

        for args in [
            vec!["loom-present", "create"],
            vec!["loom-present", "info"],
            vec!["loom-present", "inspect"],
            vec!["loom-present", "validate"],
            vec!["loom-present", "list-slides"],
            vec!["loom-present", "scene"],
            vec!["loom-present", "scene", "deck.loomdeck"],
            vec!["loom-present", "pdf", "deck.loomdeck"],
            vec!["loom-present", "export-pdf", "deck.loomdeck"],
            vec!["loom-present", "export-pptx", "deck.loomdeck"],
            vec!["loom-present", "import-pptx", "deck.pptx"],
            vec!["loom-present", "search", "deck.loomdeck"],
            vec!["loom-present", "replace", "deck.loomdeck", "a", "b"],
        ] {
            let (res, _, _) = run_cli(&args);
            assert!(res.is_err(), "expected error for {args:?}");
        }
    }

    #[test]
    fn create_info_list_validate_scene() {
        let dir = temp_dir("basic");
        let deck = dir.join("test.loomdeck");
        let deck_s = deck.to_string_lossy().into_owned();

        let (res, out, _) = run_cli(&["loom-present", "create", &deck_s, "Q4 Review"]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("3 slides"));
        assert!(deck.exists());

        for cmd in ["info", "inspect"] {
            let (res, out, _) = run_cli(&["loom-present", cmd, &deck_s]);
            assert_eq!(res, Ok(0));
            assert!(out.contains("Presentation: Q4 Review"));
            assert!(out.contains("Total Slides: 3"));
            assert!(out.contains("Slide 2: Agenda"));
        }

        let (res, out, _) = run_cli(&["loom-present", "list-slides", &deck_s]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("Slide 1:"));
        assert!(out.contains("Slide 3: Summary [summary]"));

        let (res, out, _) = run_cli(&["loom-present", "validate", &deck_s]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("valid presentation"));

        let (res, out, _) = run_cli(&["loom-present", "scene", &deck_s, "0"]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("slide:"));

        let (res, _, _) = run_cli(&["loom-present", "scene", &deck_s, "not-an-int"]);
        assert!(res.unwrap_err().contains("integer"));

        let (res, _, _) = run_cli(&["loom-present", "scene", &deck_s, "99"]);
        assert!(res.unwrap_err().contains("out of bounds"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pdf_pptx_export_and_import_roundtrip() {
        let dir = temp_dir("interop");
        let deck = dir.join("deck.loomdeck");
        let pdf = dir.join("deck.pdf");
        let pptx = dir.join("deck.pptx");
        let reimported = dir.join("reimported.loomdeck");

        let deck_s = deck.to_string_lossy().into_owned();
        let pdf_s = pdf.to_string_lossy().into_owned();
        let pptx_s = pptx.to_string_lossy().into_owned();
        let reimported_s = reimported.to_string_lossy().into_owned();

        assert_eq!(
            run_cli(&["loom-present", "create", &deck_s, "Interop Deck"]).0,
            Ok(0)
        );

        let (res, out, _) = run_cli(&["loom-present", "export-pdf", &deck_s, &pdf_s]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("Exported PDF:"));
        assert!(std::fs::read(&pdf).expect("pdf").starts_with(b"%PDF-"));

        let (res, out, _) = run_cli(&["loom-present", "export-pptx", &deck_s, &pptx_s]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("Exported PPTX:"));
        assert!(pptx.exists());

        let (res, out, _) = run_cli(&["loom-present", "info", &pptx_s]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("Total Slides: 3"));

        let (res, out, _) = run_cli(&["loom-present", "import-pptx", &pptx_s, &reimported_s]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("Imported PPTX:"));
        assert!(reimported.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_and_replace() {
        let dir = temp_dir("search");
        let deck = dir.join("deck.loomdeck");
        let out_deck = dir.join("out.loomdeck");
        let deck_s = deck.to_string_lossy().into_owned();
        let out_s = out_deck.to_string_lossy().into_owned();

        assert_eq!(
            run_cli(&["loom-present", "create", &deck_s, "Alpha Deck"]).0,
            Ok(0)
        );

        let (res, out, _) = run_cli(&["loom-present", "search", &deck_s, "agenda"]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("slide 2 title: Agenda"));

        let (res, out, _) = run_cli(&["loom-present", "search", &deck_s, "nonexistent"]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("no matches"));

        let (res, _, _) = run_cli(&["loom-present", "replace", &deck_s, "", "X", &out_s]);
        assert!(res.unwrap_err().contains("must not be empty"));

        let (res, out, _) = run_cli(&[
            "loom-present",
            "replace",
            &deck_s,
            "Agenda",
            "Roadmap",
            &out_s,
        ]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("Replaced 1 occurrence(s)"));

        let (res, out, _) = run_cli(&["loom-present", "search", &out_s, "Roadmap"]);
        assert_eq!(res, Ok(0));
        assert!(out.contains("slide 2 title: Roadmap"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
