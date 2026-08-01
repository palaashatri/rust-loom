//! PNG load/save helpers for visual QA artifacts.

use std::path::Path;

use image::ImageFormat;
use image::{codecs::png::PngEncoder, RgbaImage};

/// Load an RGBA PNG from disk.
pub fn load_png(path: &Path) -> Result<RgbaImage, Box<dyn std::error::Error>> {
    let img = image::open(path)?.to_rgba8();
    Ok(img)
}

/// Save an RGBA image as a PNG.
pub fn save_png(path: &Path, img: &RgbaImage) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let file = std::fs::File::create(path)?;
    let w = PngEncoder::new(file);
    img.write_with_encoder(w)?;
    Ok(())
}

/// Save an RGBA image using a format inferred from the extension.
pub fn save_auto(path: &Path, img: &RgbaImage) -> Result<(), Box<dyn std::error::Error>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = std::fs::File::create(path)?;
    let format = match ext.as_str() {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        "bmp" => ImageFormat::Bmp,
        "tiff" | "tif" => ImageFormat::Tiff,
        _ => ImageFormat::Png,
    };
    img.write_to(&mut file, format)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip() {
        let dir = std::env::temp_dir().join(format!("loom-test-support-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.png");
        let mut img = RgbaImage::new(4, 3);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([x as u8 * 40, y as u8 * 60, 100, 255]);
        }
        save_png(&path, &img).unwrap();
        let back = load_png(&path).unwrap();
        assert_eq!(back.dimensions(), (4, 3));
        assert_eq!(back.as_raw(), img.as_raw());
        std::fs::remove_dir_all(&dir).ok();
    }
}
