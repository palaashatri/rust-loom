//! Command line interface for Loom Photo.

use loom_photo_core::{load_photo, save_photo, Layer, PhotoDocument};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Photo CLI");
        println!("Usage:");
        println!("  loom-photo-cli create <output.loomphoto> <name> [width] [height]");
        println!("  loom-photo-cli inspect <input.loomphoto>");
        return Ok(());
    }

    match args[1].as_str() {
        "create" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let name = args.get(3).map(|s| s.as_str()).unwrap_or("Untitled Image");
            let width: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1920);
            let height: u32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1080);
            let mut doc = PhotoDocument::new("cli-photo", name, width, height);
            doc.add_layer(Layer::new_adjustment("adj-1", "Curves", "Curves", 1.0));
            let bytes = save_photo(&doc)?;
            std::fs::write(out_path, bytes).map_err(|e| format!("write error: {e}"))?;
            println!("Created photo project: {out_path} ({width}x{height}, 2 layers)");
        }
        "inspect" => {
            let in_path = args.get(2).ok_or("missing input path")?;
            let bytes = std::fs::read(in_path).map_err(|e| format!("read error: {e}"))?;
            let doc = load_photo(&bytes)?;
            println!("Photo Project: {}", doc.name);
            println!("Canvas Dimensions: {}x{}", doc.width, doc.height);
            println!("Color Space: {}", doc.color_space);
            println!("Layers ({} total):", doc.len());
            for (idx, layer) in doc.layers.iter().enumerate() {
                println!(
                    "  Layer {}: {} [{:?}] (opacity: {:.0}%)",
                    idx + 1,
                    layer.name,
                    layer.kind,
                    layer.opacity * 100.0
                );
            }
        }
        cmd => return Err(format!("unknown command: {cmd}")),
    }

    Ok(())
}
