//! Loom Writer command-line interface.
//!
//! Headless commands for creating, saving, inspecting, and exporting
//! `.loomdoc` documents. This is also used by the Docker visual-QA pipeline
//! to exercise document workflows without a display.

use loom_writer_core::{
    load_document, save_document, PageStyle, RichBlock, WriterDocument,
};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: loom-writer <command> [args]");
        eprintln!("  info <file.loomdoc>");
        eprintln!("  export-md <file.loomdoc> <out.md>");
        eprintln!("  create <file.loomdoc> <title> <body>");
        eprintln!("  validate <file.loomdoc>");
        eprintln!("  search <file.loomdoc> <query>");
        eprintln!("  replace <file.loomdoc> <query> <replacement> <out.loomdoc>");
        eprintln!("  toc <file.loomdoc>");
        eprintln!("  paginate <file.loomdoc>");
        std::process::exit(2);
    }
    match args[1].as_str() {
        "info" => cmd_info(args.get(2).ok_or("info requires <file.loomdoc>")?),
        "export-md" => cmd_export_md(
            args.get(2).ok_or("export-md requires <file.loomdoc>")?,
            args.get(3).ok_or("export-md requires <out.md>")?,
        ),
        "create" => cmd_create(
            args.get(2).ok_or("create requires <file.loomdoc>")?,
            args.get(3).ok_or("create requires <title>")?,
            args.get(4).ok_or("create requires <body>")?,
        ),
        "validate" => cmd_validate(args.get(2).ok_or("validate requires <file.loomdoc>")?),
        "search" => cmd_search(
            args.get(2).ok_or("search requires <file.loomdoc>")?,
            args.get(3).ok_or("search requires <query>")?,
        ),
        "replace" => cmd_replace(
            args.get(2).ok_or("replace requires <file.loomdoc>")?,
            args.get(3).ok_or("replace requires <query>")?,
            args.get(4).ok_or("replace requires <replacement>")?,
            args.get(5).ok_or("replace requires <out.loomdoc>")?,
        ),
        "toc" => cmd_toc(args.get(2).ok_or("toc requires <file.loomdoc>")?),
        "paginate" => cmd_paginate(args.get(2).ok_or("paginate requires <file.loomdoc>")?),
        other => Err(format!("unknown command: {other}")),
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
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    println!("wrote {path}");
    Ok(())
}

fn cmd_validate(path: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc = load_document(&bytes).map_err(|e| e.to_string())?;
    let re = save_document(&doc).map_err(|e| e.to_string())?;
    if re.is_empty() {
        return Err("empty save".into());
    }
    println!("valid: {} ({})", doc.id, doc.title);
    Ok(())
}


fn read_document(path: &str) -> Result<WriterDocument, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    load_document(&bytes).map_err(|error| error.to_string())
}

fn cmd_search(path: &str, query: &str) -> Result<(), String> {
    let document = read_document(path)?;
    let matches = document.find_all(query, false);
    println!("matches: {}", matches.len());
    for hit in matches {
        let block = document.get(hit.block_id).ok_or("search returned unknown block")?;
        println!(
            "block {} bytes {}..{}: {}",
            hit.block_id,
            hit.start,
            hit.end,
            block.text.as_str()
        );
    }
    Ok(())
}

fn cmd_replace(path: &str, query: &str, replacement: &str, out: &str) -> Result<(), String> {
    let mut document = read_document(path)?;
    let count = document.replace_all(query, replacement, false);
    let bytes = save_document(&document).map_err(|error| error.to_string())?;
    std::fs::write(out, bytes).map_err(|error| error.to_string())?;
    println!("replaced {count} occurrence(s); wrote {out}");
    Ok(())
}

fn cmd_toc(path: &str) -> Result<(), String> {
    let document = read_document(path)?;
    let entries = document.table_of_contents();
    println!("entries: {}", entries.len());
    for entry in entries {
        println!("{}{}", "  ".repeat(entry.level.saturating_sub(1) as usize), entry.title);
    }
    Ok(())
}

fn cmd_paginate(path: &str) -> Result<(), String> {
    let document = read_document(path)?;
    let pages = document.paginate(&PageStyle::default())?;
    println!("pages: {}", pages.len());
    for page in pages {
        let chars: usize = page.fragments.iter().map(|fragment| fragment.text.chars().count()).sum();
        println!("page {}: {} fragment(s), {chars} characters", page.index + 1, page.fragments.len());
    }
    Ok(())
}

#[allow(dead_code)]
fn _path(p: &str) -> PathBuf {
    PathBuf::from(p)
}
