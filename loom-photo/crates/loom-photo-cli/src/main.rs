//! Command line interface for Loom Photo.

use loom_photo_core::{
    load_photo, save_photo, BlendMode, Layer, PhotoCanvas, PhotoDocument, RgbaImage,
};
use std::env;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Loom Photo CLI");
        println!("Usage:");
        println!("  loom-photo-cli create <output.loomphoto> <name> [width] [height]");
        println!("  loom-photo-cli inspect <input.loomphoto>");
        println!("  loom-photo-cli render-demo <output.ppm> [width] [height]");
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
        "render-demo" => {
            let out_path = args.get(2).ok_or("missing output path")?;
            let width = args.get(3).and_then(|value| value.parse().ok()).unwrap_or(640);
            let height = args.get(4).and_then(|value| value.parse().ok()).unwrap_or(360);
            let mut document = PhotoDocument::new("render-demo", "Reference Composite", width, height);
            document.layers[0].blend_mode = BlendMode::Normal;
            document.add_layer(Layer::new_pixel("overlay", "Copper Overlay"));
            document.layers[1].opacity = 0.55;
            document.layers[1].blend_mode = BlendMode::Screen;
            document.add_layer(Layer::new_adjustment("contrast", "Contrast", "contrast", 0.35));
            let mut canvas = PhotoCanvas::new(document)?;
            canvas.set_layer_image("layer-bg", RgbaImage::solid(width, height, [24, 28, 36, 255])?)?;
            let mut overlay = RgbaImage::transparent(width, height)?;
            for y in 0..height {
                for x in 0..width {
                    let alpha = (((x as f32 / width.max(1) as f32) * 220.0) + 20.0) as u8;
                    overlay.set_pixel(x, y, [201, 131, 75, alpha]);
                }
            }
            canvas.set_layer_image("overlay", overlay)?;
            let image = canvas.composite()?;
            std::fs::write(out_path, image.to_ppm([255, 255, 255]))
                .map_err(|e| format!("write error: {e}"))?;
            println!("Rendered reference composite: {out_path} ({width}x{height})");
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
