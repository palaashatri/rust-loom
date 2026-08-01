//! Loom Writer command-line interface.
//!
//! Headless commands for creating, saving, inspecting, and exporting
//! `.loomdoc` documents. This is also used by the Docker visual-QA pipeline
//! to exercise document workflows without a display.

use loom_writer_core::{load_document, save_document, RichBlock, WriterDocument};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: loom-writer <command> [args]");
        eprintln!("  info <file.loomdoc>");
        eprintln!("  export-md <file.loomdoc> <out.md>");
        eprintln!("  create <file.loomdoc> <title> <body>");
        eprintln!("  validate <file.loomdoc>");
        std::process::exit(2);
    }
    let result = match args[1].as_str() {
        "info" => cmd_info(&args[2]),
        "export-md" => cmd_export_md(&args[2], &args[3]),
        "create" => cmd_create(&args[2], &args[3], &args[4]),
        "validate" => cmd_validate(&args[2]),
        other => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_info(path: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc = load_document(&bytes).map_err(|e| e.to_string())?;
    println!("id: {}", doc.id);
    println!("title: {}", doc.title);
    println!("blocks: {}", doc.len());
    for b in &doc.blocks {
        println!("  [{}] {}", b.kind, b.text.as_str());
    }
    Ok(())
}

fn cmd_export_md(path: &str, out: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc = load_document(&bytes).map_err(|e| e.to_string())?;
    let md = doc.to_markdown();
    std::fs::write(out, md).map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}

fn cmd_create(path: &str, title: &str, body: &str) -> Result<(), String> {
    let mut d = WriterDocument::new("cli-doc", title);
    d.push(RichBlock::new(d.next_id(), "heading1", title));
    d.push(RichBlock::new(d.next_id(), "paragraph", body));
    let bytes = save_document(&d).map_err(|e| e.to_string())?;
    // Write via re-exported save; write to disk.
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    println!("wrote {path}");
    Ok(())
}

fn cmd_validate(path: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc = load_document(&bytes).map_err(|e| e.to_string())?;
    // Also verify round-trip via save.
    let re = save_document(&doc).map_err(|e| e.to_string())?;
    if re.is_empty() {
        return Err("empty save".into());
    }
    println!("valid: {} ({})", doc.id, doc.title);
    Ok(())
}

// Keep PathBuf import used for future path operations.
#[allow(dead_code)]
fn _path(p: &str) -> PathBuf {
    PathBuf::from(p)
}
