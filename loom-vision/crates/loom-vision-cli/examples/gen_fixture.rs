//! Regenerates the CLI test fixtures: a QR code rendered into a PNG file.
//!
//! Usage (from the workspace root):
//!
//! ```text
//! cargo run --example gen_fixture -- crates/loom-vision-cli/fixtures/hello.png
//! ```
//!
//! The default output path is `crates/loom-vision-cli/fixtures/hello.png`
//! and the default payload is `Hello, Loom!`.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use image::{Rgb, RgbImage};
use qrcode::types::Color;
use qrcode::QrCode;

const MODULE_PX: i64 = 8;
const QUIET_MODULES: i64 = 4;
const DEFAULT_TEXT: &str = "Hello, Loom!";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| DEFAULT_TEXT.to_string());
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/loom-vision-cli/fixtures/hello.png"));

    let code = QrCode::new(text.as_bytes())?;
    let modules = code.width() as i64;
    let size = ((modules + 2 * QUIET_MODULES) * MODULE_PX) as u32;
    let mut image = RgbImage::new(size, size);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let mx = i64::from(x) / MODULE_PX - QUIET_MODULES;
        let my = i64::from(y) / MODULE_PX - QUIET_MODULES;
        let dark = mx >= 0
            && my >= 0
            && mx < modules
            && my < modules
            && code[(mx as usize, my as usize)] == Color::Dark;
        *pixel = if dark {
            Rgb([0, 0, 0])
        } else {
            Rgb([255, 255, 255])
        };
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image.save(&output)?;
    println!(
        "wrote {} ({}x{}) encoding {text:?}",
        output.display(),
        size,
        size
    );
    Ok(())
}
