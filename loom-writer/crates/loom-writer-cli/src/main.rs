//! Loom Writer command-line interface.
//!
//! Headless commands for creating, saving, inspecting, and exporting
//! `.loomdoc` documents. This is also used by the Docker visual-QA pipeline
//! to exercise document workflows without a display.

use loom_writer_core::{
    export_docx, export_pdf, import_docx, inspect_docx_unsupported, load_document, save_document,
    PageStyle, RichBlock, WriterDocument,
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
usage: loom-writer <command> [args]
  info <file.loomdoc>
  export-md <file.loomdoc> <out.md>
  export-pdf <file.loomdoc> <out.pdf>
  export-docx <file.loomdoc> <out.docx>
  import-docx <file.docx> <out.loomdoc>
  create <file.loomdoc> <title> <body>
  validate <file.loomdoc>
  search <file.loomdoc> <query>
  replace <file.loomdoc> <query> <replacement> <out.loomdoc>
  toc <file.loomdoc>
  paginate <file.loomdoc>
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
        "info" => cmd_info(args.get(2).ok_or("info requires <file.loomdoc>")?, stdout)?,
        "export-md" => cmd_export_md(
            args.get(2).ok_or("export-md requires <file.loomdoc>")?,
            args.get(3).ok_or("export-md requires <out.md>")?,
            stdout,
        )?,
        "export-pdf" => cmd_export_pdf(
            args.get(2).ok_or("export-pdf requires <file.loomdoc>")?,
            args.get(3).ok_or("export-pdf requires <out.pdf>")?,
            stdout,
        )?,
        "export-docx" => cmd_export_docx(
            args.get(2).ok_or("export-docx requires <file.loomdoc>")?,
            args.get(3).ok_or("export-docx requires <out.docx>")?,
            stdout,
        )?,
        "import-docx" => cmd_import_docx(
            args.get(2).ok_or("import-docx requires <file.docx>")?,
            args.get(3).ok_or("import-docx requires <out.loomdoc>")?,
            stdout,
            stderr,
        )?,
        "create" => cmd_create(
            args.get(2).ok_or("create requires <file.loomdoc>")?,
            args.get(3).ok_or("create requires <title>")?,
            args.get(4).ok_or("create requires <body>")?,
            stdout,
        )?,
        "validate" => cmd_validate(
            args.get(2).ok_or("validate requires <file.loomdoc>")?,
            stdout,
        )?,
        "search" => cmd_search(
            args.get(2).ok_or("search requires <file.loomdoc>")?,
            args.get(3).ok_or("search requires <query>")?,
            stdout,
        )?,
        "replace" => cmd_replace(
            args.get(2).ok_or("replace requires <file.loomdoc>")?,
            args.get(3).ok_or("replace requires <query>")?,
            args.get(4).ok_or("replace requires <replacement>")?,
            args.get(5).ok_or("replace requires <out.loomdoc>")?,
            stdout,
        )?,
        "toc" => cmd_toc(args.get(2).ok_or("toc requires <file.loomdoc>")?, stdout)?,
        "paginate" => cmd_paginate(
            args.get(2).ok_or("paginate requires <file.loomdoc>")?,
            stdout,
        )?,
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(0)
}

fn cmd_info(path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_document(path)?;
    writeln!(stdout, "id: {}", doc.id).map_err(|e| e.to_string())?;
    writeln!(stdout, "title: {}", doc.title).map_err(|e| e.to_string())?;
    writeln!(stdout, "blocks: {}", doc.len()).map_err(|e| e.to_string())?;
    for b in &doc.blocks {
        writeln!(stdout, "  [{}] {}", b.kind, b.text.as_str()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_export_md(path: &str, out: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_document(path)?;
    let md = doc.to_markdown();
    std::fs::write(out, md).map_err(|e| e.to_string())?;
    writeln!(stdout, "wrote {out}").map_err(|e| e.to_string())
}

fn cmd_export_pdf(path: &str, out: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_document(path)?;
    let pdf = export_pdf(&doc).map_err(|e| e.to_string())?;
    std::fs::write(out, pdf).map_err(|e| e.to_string())?;
    writeln!(stdout, "wrote {out}").map_err(|e| e.to_string())
}

fn cmd_export_docx(path: &str, out: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_document(path)?;
    let export = export_docx(&doc).map_err(|e| e.to_string())?;
    std::fs::write(out, export.bytes).map_err(|e| e.to_string())?;
    writeln!(stdout, "wrote {out}").map_err(|e| e.to_string())
}

fn cmd_import_docx(
    path: &str,
    out: &str,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), String> {
    let docx_bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let warnings = inspect_docx_unsupported(&docx_bytes)?;
    for warning in &warnings {
        writeln!(stderr, "warning: {warning}").map_err(|e| e.to_string())?;
    }
    let doc = import_docx(&docx_bytes)?;
    let bytes = save_document(&doc).map_err(|e| e.to_string())?;
    std::fs::write(out, bytes).map_err(|e| e.to_string())?;
    writeln!(stdout, "wrote {out}").map_err(|e| e.to_string())
}

fn cmd_create(path: &str, title: &str, body: &str, stdout: &mut impl Write) -> Result<(), String> {
    let mut d = WriterDocument::new("cli-doc", title);
    d.push(RichBlock::new(d.next_id(), "heading1", title));
    d.push(RichBlock::new(d.next_id(), "paragraph", body));
    let bytes = save_document(&d).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    writeln!(stdout, "wrote {path}").map_err(|e| e.to_string())
}

fn cmd_validate(path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let doc = read_document(path)?;
    let re = save_document(&doc).map_err(|e| e.to_string())?;
    if re.is_empty() {
        return Err("empty save".into());
    }
    writeln!(stdout, "valid: {} ({})", doc.id, doc.title).map_err(|e| e.to_string())
}

fn read_document(path: &str) -> Result<WriterDocument, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    load_document(&bytes).map_err(|error| error.to_string())
}

fn cmd_search(path: &str, query: &str, stdout: &mut impl Write) -> Result<(), String> {
    let document = read_document(path)?;
    let matches = document.find_all(query, false);
    writeln!(stdout, "matches: {}", matches.len()).map_err(|e| e.to_string())?;
    for hit in matches {
        let block = document
            .get(hit.block_id)
            .ok_or("search returned unknown block")?;
        writeln!(
            stdout,
            "block {} bytes {}..{}: {}",
            hit.block_id,
            hit.start,
            hit.end,
            block.text.as_str()
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_replace(
    path: &str,
    query: &str,
    replacement: &str,
    out: &str,
    stdout: &mut impl Write,
) -> Result<(), String> {
    let mut document = read_document(path)?;
    let count = document.replace_all(query, replacement, false);
    let bytes = save_document(&document).map_err(|error| error.to_string())?;
    std::fs::write(out, bytes).map_err(|error| error.to_string())?;
    writeln!(stdout, "replaced {count} occurrence(s); wrote {out}").map_err(|e| e.to_string())
}

fn cmd_toc(path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let document = read_document(path)?;
    let entries = document.table_of_contents();
    writeln!(stdout, "entries: {}", entries.len()).map_err(|e| e.to_string())?;
    for entry in entries {
        writeln!(
            stdout,
            "{}{}",
            "  ".repeat(entry.level.saturating_sub(1) as usize),
            entry.title
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cmd_paginate(path: &str, stdout: &mut impl Write) -> Result<(), String> {
    let document = read_document(path)?;
    let pages = document.paginate(&PageStyle::default())?;
    writeln!(stdout, "pages: {}", pages.len()).map_err(|e| e.to_string())?;
    for page in pages {
        let chars: usize = page
            .fragments
            .iter()
            .map(|fragment| fragment.text.chars().count())
            .sum();
        writeln!(
            stdout,
            "page {}: {} fragment(s), {chars} characters",
            page.index + 1,
            page.fragments.len()
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cli_args(parts: &[&str]) -> Vec<String> {
        std::iter::once("loom-writer-cli".to_string())
            .chain(parts.iter().map(|s| (*s).to_string()))
            .collect()
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("loom_writer_cli_{label}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn help_flags_print_full_usage_to_stdout_with_exit_zero() {
        for flag in ["--help", "-h", "help"] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = run_with(&cli_args(&[flag]), &mut stdout, &mut stderr)
                .expect("help should succeed");
            assert_eq!(code, 0, "{flag} exits with code 0");
            assert!(stderr.is_empty(), "{flag} writes nothing to stderr");
            let text = String::from_utf8(stdout).expect("utf-8 usage");
            for command in [
                "info",
                "export-md",
                "export-pdf",
                "export-docx",
                "import-docx",
                "create",
                "validate",
                "search",
                "replace",
                "toc",
                "paginate",
            ] {
                assert!(
                    text.contains(command),
                    "{flag} usage lists {command}: {text}"
                );
            }
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(&["loom-writer-cli".into()], &mut stdout, &mut stderr)
            .expect("missing command returns usage code 2");
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(String::from_utf8_lossy(&stderr).contains("usage: loom-writer"));
    }

    #[test]
    fn create_validate_export_pdf_export_docx_and_import_docx_round_trip() {
        let dir = temp_dir("roundtrip");
        let loomdoc = dir.join("sample.loomdoc");
        let pdf = dir.join("sample.pdf");
        let docx = dir.join("sample.docx");
        let reimported = dir.join("reimported.loomdoc");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run_with(
                &cli_args(&[
                    "create",
                    loomdoc.to_str().unwrap(),
                    "Quarterly Report",
                    "Body paragraph for CLI round-trip.",
                ]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        assert!(stderr.is_empty());

        stdout.clear();
        assert_eq!(
            run_with(
                &cli_args(&["validate", loomdoc.to_str().unwrap()]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        assert!(String::from_utf8_lossy(&stdout).contains("valid: cli-doc (Quarterly Report)"));

        stdout.clear();
        assert_eq!(
            run_with(
                &cli_args(&[
                    "export-pdf",
                    loomdoc.to_str().unwrap(),
                    pdf.to_str().unwrap()
                ]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        let pdf_bytes = std::fs::read(&pdf).expect("read exported pdf");
        assert!(pdf_bytes.starts_with(b"%PDF-1.4"));

        stdout.clear();
        assert_eq!(
            run_with(
                &cli_args(&[
                    "export-docx",
                    loomdoc.to_str().unwrap(),
                    docx.to_str().unwrap()
                ]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        let docx_bytes = std::fs::read(&docx).expect("read exported docx");
        assert!(docx_bytes.starts_with(b"PK"));

        stdout.clear();
        assert_eq!(
            run_with(
                &cli_args(&[
                    "import-docx",
                    docx.to_str().unwrap(),
                    reimported.to_str().unwrap(),
                ]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        assert!(stderr.is_empty(), "clean round-trip emits no warnings");

        stdout.clear();
        assert_eq!(
            run_with(
                &cli_args(&["validate", reimported.to_str().unwrap()]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        let reloaded =
            load_document(&std::fs::read(&reimported).unwrap()).expect("load reimported");
        assert_eq!(reloaded.title, "Quarterly Report");
        let combined: String = reloaded
            .blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(combined.contains("Quarterly Report"));
        assert!(combined.contains("Body paragraph for CLI round-trip."));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_docx_prints_unsupported_warnings_to_stderr() {
        let dir = temp_dir("warnings");
        let docx = dir.join("with_warnings.docx");
        let out = dir.join("out.loomdoc");

        let mut archive = loom_package::zip::PackageArchive::new();
        let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:rPr><w:bCs/></w:rPr><w:t>Hello</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        archive
            .add("word/document.xml", doc_xml.as_bytes().to_vec())
            .unwrap();
        std::fs::write(&docx, archive.to_bytes().unwrap()).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run_with(
                &cli_args(&["import-docx", docx.to_str().unwrap(), out.to_str().unwrap()]),
                &mut stdout,
                &mut stderr,
            )
            .unwrap(),
            0
        );
        let warning_text = String::from_utf8(stderr).expect("utf-8 stderr");
        assert!(
            warning_text.contains("warning: Complex-script"),
            "{warning_text}"
        );
        assert!(warning_text.contains("w:bCs"), "{warning_text}");
        assert!(out.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
