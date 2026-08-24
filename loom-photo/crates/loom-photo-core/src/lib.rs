//! Core nondestructive photo and image editing engine for Loom Photo.

use image::ImageEncoder;
use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayerKind {
    Pixel,
    Adjustment,
    Text,
    Vector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    HardLight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub adjustment_type: Option<String>,
    pub adjustment_value: f32,
}

impl Layer {
    pub fn new_pixel(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: LayerKind::Pixel,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustment_type: None,
            adjustment_value: 0.0,
        }
    }

    pub fn new_adjustment(
        id: impl Into<String>,
        name: impl Into<String>,
        adj_type: impl Into<String>,
        val: f32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: LayerKind::Adjustment,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustment_type: Some(adj_type.into()),
            adjustment_value: val,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoDocument {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub color_space: String,
    pub layers: Vec<Layer>,
    pub active_layer_index: usize,
}

impl PhotoDocument {
    pub fn new(id: impl Into<String>, name: impl Into<String>, width: u32, height: u32) -> Self {
        let mut doc = Self {
            id: id.into(),
            name: name.into(),
            width,
            height,
            dpi: 300,
            color_space: "sRGB".to_string(),
            layers: Vec::new(),
            active_layer_index: 0,
        };
        doc.layers.push(Layer::new_pixel("layer-bg", "Background"));
        doc
    }

    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
        self.active_layer_index = self.layers.len() - 1;
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn select_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.active_layer_index = index;
        true
    }
}

pub fn save_photo(doc: &PhotoDocument) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/photo.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Photo,
        id: doc.id.clone(),
        title: doc.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/photo.json".into(),
            mime: MimeType::parse("application/vnd.loom.photo-content")
                .map_err(|e| format!("invalid built-in photo MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_photo(bytes: &[u8]) -> Result<PhotoDocument, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Photo {
        return Err("not a Photo project".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/photo.json")
        .ok_or_else(|| "missing photo.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

/// Maximum raster size accepted by the in-memory reference compositor.
pub const MAX_REFERENCE_PIXELS: usize = 100_000_000;

/// A validated row-major RGBA8 image.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RgbaImage {
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
    /// Row-major RGBA bytes.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Creates a transparent image.
    pub fn transparent(width: u32, height: u32) -> Result<Self, String> {
        let bytes = image_byte_len(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; bytes],
        })
    }

    /// Creates an image filled with one RGBA color.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Result<Self, String> {
        let mut image = Self::transparent(width, height)?;
        for pixel in image.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&rgba);
        }
        Ok(image)
    }

    /// Validates dimensions and buffer length.
    pub fn validate(&self) -> Result<(), String> {
        let expected = image_byte_len(self.width, self.height)?;
        if self.pixels.len() != expected {
            return Err(format!(
                "RGBA buffer length {} does not match {}x{}",
                self.pixels.len(),
                self.width,
                self.height
            ));
        }
        Ok(())
    }

    /// Reads a pixel.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y as usize * self.width as usize) + x as usize) * 4;
        Some([
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ])
    }

    /// Writes a pixel.
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let index = ((y as usize * self.width as usize) + x as usize) * 4;
        self.pixels[index..index + 4].copy_from_slice(&rgba);
        true
    }

    /// Crops an axis-aligned rectangle.
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Self, String> {
        if width == 0
            || height == 0
            || x.checked_add(width).is_none()
            || y.checked_add(height).is_none()
            || x + width > self.width
            || y + height > self.height
        {
            return Err("crop rectangle is outside the image".into());
        }
        let mut output = Self::transparent(width, height)?;
        for row in 0..height {
            let src_start = (((y + row) * self.width + x) as usize) * 4;
            let dst_start = (row as usize * width as usize) * 4;
            let row_bytes = width as usize * 4;
            output.pixels[dst_start..dst_start + row_bytes]
                .copy_from_slice(&self.pixels[src_start..src_start + row_bytes]);
        }
        Ok(output)
    }

    /// Computes maximum centered crop bounds fitting within this image for the target aspect ratio.
    pub fn aspect_crop_bounds(&self, aspect: CropAspectRatio) -> (u32, u32, u32, u32) {
        if let Some(target_ratio) = aspect.ratio() {
            let current_ratio = self.width as f32 / self.height as f32;
            if current_ratio > target_ratio {
                let target_w = (self.height as f32 * target_ratio).round() as u32;
                let x = (self.width.saturating_sub(target_w)) / 2;
                (x, 0, target_w.min(self.width), self.height)
            } else {
                let target_h = (self.width as f32 / target_ratio).round() as u32;
                let y = (self.height.saturating_sub(target_h)) / 2;
                (0, y, self.width, target_h.min(self.height))
            }
        } else {
            (0, 0, self.width, self.height)
        }
    }

    /// Nearest-neighbour resize used by deterministic previews and tests.
    pub fn resize_nearest(&self, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("resize dimensions must be non-zero".into());
        }
        let mut output = Self::transparent(width, height)?;
        for y in 0..height {
            let source_y = ((u64::from(y) * u64::from(self.height)) / u64::from(height)) as u32;
            for x in 0..width {
                let source_x = ((u64::from(x) * u64::from(self.width)) / u64::from(width)) as u32;
                let pixel = self
                    .pixel(source_x.min(self.width - 1), source_y.min(self.height - 1))
                    .expect("source coordinate is clamped");
                output.set_pixel(x, y, pixel);
            }
        }
        Ok(output)
    }

    /// Flips the image horizontally.
    pub fn flip_horizontal(&self) -> Result<Self, String> {
        let mut output = Self::transparent(self.width, self.height)?;
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixel(x, y).expect("pixel within bounds");
                output.set_pixel(self.width - 1 - x, y, pixel);
            }
        }
        Ok(output)
    }

    /// Flips the image vertically.
    pub fn flip_vertical(&self) -> Result<Self, String> {
        let mut output = Self::transparent(self.width, self.height)?;
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixel(x, y).expect("pixel within bounds");
                output.set_pixel(x, self.height - 1 - y, pixel);
            }
        }
        Ok(output)
    }

    /// Rotates the image 90 degrees clockwise (swapping width and height).
    pub fn rotate_90_cw(&self) -> Result<Self, String> {
        let mut output = Self::transparent(self.height, self.width)?;
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixel(x, y).expect("pixel within bounds");
                output.set_pixel(self.height - 1 - y, x, pixel);
            }
        }
        Ok(output)
    }

    /// Rotates the image 180 degrees.
    pub fn rotate_180(&self) -> Result<Self, String> {
        let mut output = Self::transparent(self.width, self.height)?;
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixel(x, y).expect("pixel within bounds");
                output.set_pixel(self.width - 1 - x, self.height - 1 - y, pixel);
            }
        }
        Ok(output)
    }

    /// Performs a two-pass (horizontal + vertical) box blur with a specified pixel radius.
    pub fn box_blur(&self, radius: u32) -> Result<Self, String> {
        if radius == 0 {
            return Ok(self.clone());
        }
        let w = self.width as usize;
        let h = self.height as usize;
        let rad = radius as usize;

        // Pass 1: Horizontal Blur
        let mut temp = Self::transparent(self.width, self.height)?;
        for y in 0..h {
            for x in 0..w {
                let x_start = x.saturating_sub(rad);
                let x_end = (x + rad).min(w - 1);
                let count = (x_end - x_start + 1) as u32;

                let mut r_sum = 0u32;
                let mut g_sum = 0u32;
                let mut b_sum = 0u32;
                let mut a_sum = 0u32;

                for kx in x_start..=x_end {
                    let p = self
                        .pixel(kx as u32, y as u32)
                        .expect("pixel within bounds");
                    r_sum += p[0] as u32;
                    g_sum += p[1] as u32;
                    b_sum += p[2] as u32;
                    a_sum += p[3] as u32;
                }

                temp.set_pixel(
                    x as u32,
                    y as u32,
                    [
                        (r_sum / count) as u8,
                        (g_sum / count) as u8,
                        (b_sum / count) as u8,
                        (a_sum / count) as u8,
                    ],
                );
            }
        }

        // Pass 2: Vertical Blur
        let mut output = Self::transparent(self.width, self.height)?;
        for y in 0..h {
            let y_start = y.saturating_sub(rad);
            let y_end = (y + rad).min(h - 1);
            let count = (y_end - y_start + 1) as u32;

            for x in 0..w {
                let mut r_sum = 0u32;
                let mut g_sum = 0u32;
                let mut b_sum = 0u32;
                let mut a_sum = 0u32;

                for ky in y_start..=y_end {
                    let p = temp
                        .pixel(x as u32, ky as u32)
                        .expect("pixel within bounds");
                    r_sum += p[0] as u32;
                    g_sum += p[1] as u32;
                    b_sum += p[2] as u32;
                    a_sum += p[3] as u32;
                }

                output.set_pixel(
                    x as u32,
                    y as u32,
                    [
                        (r_sum / count) as u8,
                        (g_sum / count) as u8,
                        (b_sum / count) as u8,
                        (a_sum / count) as u8,
                    ],
                );
            }
        }

        Ok(output)
    }

    /// Performs separable two-pass Gaussian blur on this image.
    pub fn gaussian_blur(&self, radius: u32, sigma: f32) -> Result<Self, String> {
        if radius == 0 {
            return Ok(self.clone());
        }
        let kernel = generate_gaussian_kernel(radius, sigma);
        let w = self.width as i32;
        let h = self.height as i32;
        let r = radius as i32;

        let mut temp = Self::transparent(self.width, self.height)?;

        for y in 0..h {
            for x in 0..w {
                let mut r_acc = 0.0_f32;
                let mut g_acc = 0.0_f32;
                let mut b_acc = 0.0_f32;
                let mut a_acc = 0.0_f32;

                for (idx, &weight) in kernel.iter().enumerate() {
                    let kx = (x - r + idx as i32).clamp(0, w - 1);
                    let p = self.pixel(kx as u32, y as u32).unwrap_or([0, 0, 0, 0]);
                    r_acc += p[0] as f32 * weight;
                    g_acc += p[1] as f32 * weight;
                    b_acc += p[2] as f32 * weight;
                    a_acc += p[3] as f32 * weight;
                }

                temp.set_pixel(
                    x as u32,
                    y as u32,
                    [
                        r_acc.round().clamp(0.0, 255.0) as u8,
                        g_acc.round().clamp(0.0, 255.0) as u8,
                        b_acc.round().clamp(0.0, 255.0) as u8,
                        a_acc.round().clamp(0.0, 255.0) as u8,
                    ],
                );
            }
        }

        let mut output = Self::transparent(self.width, self.height)?;

        for y in 0..h {
            for x in 0..w {
                let mut r_acc = 0.0_f32;
                let mut g_acc = 0.0_f32;
                let mut b_acc = 0.0_f32;
                let mut a_acc = 0.0_f32;

                for (idx, &weight) in kernel.iter().enumerate() {
                    let ky = (y - r + idx as i32).clamp(0, h - 1);
                    let p = temp.pixel(x as u32, ky as u32).unwrap_or([0, 0, 0, 0]);
                    r_acc += p[0] as f32 * weight;
                    g_acc += p[1] as f32 * weight;
                    b_acc += p[2] as f32 * weight;
                    a_acc += p[3] as f32 * weight;
                }

                output.set_pixel(
                    x as u32,
                    y as u32,
                    [
                        r_acc.round().clamp(0.0, 255.0) as u8,
                        g_acc.round().clamp(0.0, 255.0) as u8,
                        b_acc.round().clamp(0.0, 255.0) as u8,
                        a_acc.round().clamp(0.0, 255.0) as u8,
                    ],
                );
            }
        }

        Ok(output)
    }

    /// Extracts a single color channel (0: Red, 1: Green, 2: Blue, 3: Alpha) as a byte slice.
    pub fn extract_channel(&self, channel_idx: usize) -> Result<Vec<u8>, String> {
        if channel_idx > 3 {
            return Err("channel index must be 0..=3".into());
        }
        let count = (self.width * self.height) as usize;
        let mut channel = Vec::with_capacity(count);
        for y in 0..self.height {
            for x in 0..self.width {
                let p = self.pixel(x, y).unwrap_or([0, 0, 0, 0]);
                channel.push(p[channel_idx]);
            }
        }
        Ok(channel)
    }

    /// Applies a tone curve LUT to all RGB color channels, preserving alpha.
    pub fn apply_tone_curve(&mut self, lut: &ToneCurveLUT) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel[0] = lut.map[pixel[0] as usize];
            pixel[1] = lut.map[pixel[1] as usize];
            pixel[2] = lut.map[pixel[2] as usize];
        }
    }

    /// Applies 3-way color grading (Lift, Gamma, Gain) to RGB channels, preserving alpha.
    pub fn apply_lift_gamma_gain(&mut self, lgg: &LiftGammaGain) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            for (idx, (lift, gamma, gain)) in [
                (lgg.lift.0, lgg.gamma.0, lgg.gain.0),
                (lgg.lift.1, lgg.gamma.1, lgg.gain.1),
                (lgg.lift.2, lgg.gamma.2, lgg.gain.2),
            ]
            .iter()
            .enumerate()
            {
                let v = pixel[idx] as f32 / 255.0;
                let inv_gamma = if *gamma > 0.001 { 1.0 / gamma } else { 1.0 };
                let graded = lift * (1.0 - v) + gain * v.powf(inv_gamma);
                pixel[idx] = (graded.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }

    /// High-pass raster filter extracting high-frequency edge details with a 128-gray baseline.
    pub fn high_pass_filter(&self, radius: u32, sigma: f32) -> Result<RgbaImage, String> {
        let blurred = self.gaussian_blur(radius, sigma)?;
        let mut out = self.clone();

        for (src, blur) in out
            .pixels
            .chunks_exact_mut(4)
            .zip(blurred.pixels.chunks_exact(4))
        {
            for i in 0..3 {
                let diff = src[i] as f32 - blur[i] as f32 + 128.0;
                src[i] = diff.clamp(0.0, 255.0).round() as u8;
            }
        }
        Ok(out)
    }

    /// Unsharp mask sharpening filter emphasizing local edge contrast.
    pub fn unsharp_mask(
        &self,
        radius: u32,
        sigma: f32,
        amount: f32,
        threshold: u8,
    ) -> Result<RgbaImage, String> {
        let blurred = self.gaussian_blur(radius, sigma)?;
        let mut out = self.clone();

        for (src, blur) in out
            .pixels
            .chunks_exact_mut(4)
            .zip(blurred.pixels.chunks_exact(4))
        {
            for i in 0..3 {
                let diff = src[i] as f32 - blur[i] as f32;
                if diff.abs() >= threshold as f32 {
                    let sharpened = src[i] as f32 + diff * amount;
                    src[i] = sharpened.clamp(0.0, 255.0).round() as u8;
                }
            }
        }
        Ok(out)
    }

    /// Applies a photographic vignette darkening or lightening image corners.
    pub fn apply_vignette(&mut self, config: &VignetteConfig) {
        if config.amount.abs() < 1e-4 {
            return;
        }

        let cx = self.width as f32 / 2.0;
        let cy = self.height as f32 / 2.0;

        for y in 0..self.height {
            for x in 0..self.width {
                let dx = (x as f32 - cx) / cx;
                let dy = (y as f32 - cy) / cy;

                let r_dist = (dx * dx + dy * dy).sqrt();
                let aspect_dist = dx.abs().max(dy.abs());
                let dist = r_dist * config.roundness + aspect_dist * (1.0 - config.roundness);

                let start = (config.midpoint * 0.8).clamp(0.0, 1.0);
                let end = (start + config.feather * 0.8 + 0.2).clamp(start + 0.01, 2.0);

                let factor = if dist <= start {
                    0.0
                } else if dist >= end {
                    1.0
                } else {
                    let t = (dist - start) / (end - start);
                    t * t * (3.0 - 2.0 * t) // smoothstep
                };

                let multiplier = (1.0 + config.amount * factor).clamp(0.0, 2.0);

                if let Some(pixel) = self.pixel(x, y) {
                    let r = (pixel[0] as f32 * multiplier).round().clamp(0.0, 255.0) as u8;
                    let g = (pixel[1] as f32 * multiplier).round().clamp(0.0, 255.0) as u8;
                    let b = (pixel[2] as f32 * multiplier).round().clamp(0.0, 255.0) as u8;
                    self.set_pixel(x, y, [r, g, b, pixel[3]]);
                }
            }
        }
    }

    /// Corrects or simulates optical lens distortion (barrel / pincushion) using polynomial radial mapping.
    pub fn apply_lens_distortion(&self, config: &LensDistortionConfig) -> Result<Self, String> {
        let mut out = Self::transparent(self.width, self.height)?;
        let cx = self.width as f32 / 2.0;
        let cy = self.height as f32 / 2.0;
        let norm_factor = (cx * cx + cy * cy).sqrt();

        for y in 0..self.height {
            for x in 0..self.width {
                let dx = (x as f32 - cx) / norm_factor;
                let dy = (y as f32 - cy) / norm_factor;
                let r2 = dx * dx + dy * dy;
                let r4 = r2 * r2;

                let distortion = 1.0 + config.k1 * r2 + config.k2 * r4;
                let src_x = cx + (dx * distortion * config.scale) * norm_factor;
                let src_y = cy + (dy * distortion * config.scale) * norm_factor;

                if src_x >= 0.0
                    && src_x < (self.width - 1) as f32
                    && src_y >= 0.0
                    && src_y < (self.height - 1) as f32
                {
                    let x0 = src_x.floor() as u32;
                    let y0 = src_y.floor() as u32;
                    let x1 = (x0 + 1).min(self.width - 1);
                    let y1 = (y0 + 1).min(self.height - 1);

                    let fx = src_x - x0 as f32;
                    let fy = src_y - y0 as f32;

                    let p00 = self.pixel(x0, y0).unwrap_or([0, 0, 0, 0]);
                    let p10 = self.pixel(x1, y0).unwrap_or([0, 0, 0, 0]);
                    let p01 = self.pixel(x0, y1).unwrap_or([0, 0, 0, 0]);
                    let p11 = self.pixel(x1, y1).unwrap_or([0, 0, 0, 0]);

                    let mut sampled = [0u8; 4];
                    for c in 0..4 {
                        let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
                        let bottom = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
                        let val = top * (1.0 - fy) + bottom * fy;
                        sampled[c] = val.round().clamp(0.0, 255.0) as u8;
                    }
                    out.set_pixel(x, y, sampled);
                }
            }
        }
        Ok(out)
    }

    /// Corrects or simulates lateral chromatic aberration (color fringing) by shifting color channels.
    pub fn apply_chromatic_aberration(
        &self,
        config: &ChromaticAberrationConfig,
    ) -> Result<Self, String> {
        let mut out = self.clone();
        let cx = self.width as f32 / 2.0;
        let cy = self.height as f32 / 2.0;
        let norm_factor = (cx * cx + cy * cy).sqrt().max(1.0);

        for y in 0..self.height {
            for x in 0..self.width {
                let dx = (x as f32 - cx) / norm_factor;
                let dy = (y as f32 - cy) / norm_factor;

                // Red channel sampling location
                let rx = ((x as f32 + config.red_shift.0 + dx * config.radial_fringe * norm_factor)
                    .round() as i32)
                    .clamp(0, self.width as i32 - 1) as u32;
                let ry = ((y as f32 + config.red_shift.1 + dy * config.radial_fringe * norm_factor)
                    .round() as i32)
                    .clamp(0, self.height as i32 - 1) as u32;

                // Blue channel sampling location
                let bx = ((x as f32 + config.blue_shift.0 - dx * config.radial_fringe * norm_factor)
                    .round() as i32)
                    .clamp(0, self.width as i32 - 1) as u32;
                let by = ((y as f32 + config.blue_shift.1 - dy * config.radial_fringe * norm_factor)
                    .round() as i32)
                    .clamp(0, self.height as i32 - 1) as u32;

                let r_val = self.pixel(rx, ry).unwrap_or([0, 0, 0, 0])[0];
                let g_val = self.pixel(x, y).unwrap_or([0, 0, 0, 0])[1];
                let b_val = self.pixel(bx, by).unwrap_or([0, 0, 0, 0])[2];
                let a_val = self.pixel(x, y).unwrap_or([0, 0, 0, 0])[3];

                out.set_pixel(x, y, [r_val, g_val, b_val, a_val]);
            }
        }
        Ok(out)
    }

    /// Applies dual-tone color grading to shadows and highlights (Split Toning).
    pub fn apply_split_toning(&self, config: &SplitToningConfig) -> Result<Self, String> {
        let mut out = self.clone();
        let s_rgb = hue_to_rgb(config.shadows_hue_deg);
        let h_rgb = hue_to_rgb(config.highlights_hue_deg);
        let balance_offset = config.balance.clamp(-1.0, 1.0) * 0.25;

        for pixel in out.pixels.chunks_exact_mut(4) {
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;
            let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);

            // Midpoint adjusted by balance
            let mid = 0.5 + balance_offset;
            let shadow_weight =
                ((mid - luma) / mid).clamp(0.0, 1.0) * config.shadows_saturation.clamp(0.0, 1.0);
            let highlight_weight = ((luma - mid) / (1.0 - mid).max(0.01)).clamp(0.0, 1.0)
                * config.highlights_saturation.clamp(0.0, 1.0);

            let r_toned = r * (1.0 - shadow_weight) + s_rgb[0] * shadow_weight;
            let g_toned = g * (1.0 - shadow_weight) + s_rgb[1] * shadow_weight;
            let b_toned = b * (1.0 - shadow_weight) + s_rgb[2] * shadow_weight;

            let r_final = r_toned * (1.0 - highlight_weight) + h_rgb[0] * highlight_weight;
            let g_final = g_toned * (1.0 - highlight_weight) + h_rgb[1] * highlight_weight;
            let b_final = b_toned * (1.0 - highlight_weight) + h_rgb[2] * highlight_weight;

            pixel[0] = (r_final * 255.0).round().clamp(0.0, 255.0) as u8;
            pixel[1] = (g_final * 255.0).round().clamp(0.0, 255.0) as u8;
            pixel[2] = (b_final * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        Ok(out)
    }

    /// Simulates analog photographic film grain with luminance-dependent density.
    pub fn apply_film_grain(&self, config: &FilmGrainConfig) -> Result<Self, String> {
        let mut out = self.clone();
        let amount = config.amount.clamp(0.0, 1.0);
        if amount == 0.0 {
            return Ok(out);
        }

        // Pseudo-random hash helper
        let hash = |x: u32, y: u32, seed: u64, channel: u32| -> f32 {
            let mut h = (seed
                .wrapping_add(x as u64 * 374761393)
                .wrapping_add(y as u64 * 668265263))
            .wrapping_add(channel as u64 * 961748941);
            h = (h ^ (h >> 13)).wrapping_mul(1274126177);
            let val = ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0; // in [0, 1]
            val * 2.0 - 1.0 // in [-1.0, 1.0]
        };

        for y in 0..out.height {
            for x in 0..out.width {
                let px = out.pixel(x, y).unwrap();
                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;
                let a = px[3];

                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                // Physical film grain is most prominent in midtones and shadows
                let luma_scale = (1.0 - (luma - 0.4).abs() * 1.2).clamp(0.2, 1.0);

                let (nr, ng, nb) = if config.is_colored {
                    (
                        hash(x, y, config.seed, 0),
                        hash(x, y, config.seed, 1),
                        hash(x, y, config.seed, 2),
                    )
                } else {
                    let mono = hash(x, y, config.seed, 0);
                    (mono, mono, mono)
                };

                let grain_scale = amount * luma_scale;
                let r_out = ((r + nr * grain_scale) * 255.0).round().clamp(0.0, 255.0) as u8;
                let g_out = ((g + ng * grain_scale) * 255.0).round().clamp(0.0, 255.0) as u8;
                let b_out = ((b + nb * grain_scale) * 255.0).round().clamp(0.0, 255.0) as u8;

                out.set_pixel(x, y, [r_out, g_out, b_out, a]);
            }
        }
        Ok(out)
    }

    /// Encodes a portable pixmap (P6), flattening alpha against `background`.
    pub fn to_ppm(&self, background: [u8; 3]) -> Vec<u8> {
        let mut output = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        output.reserve(self.width as usize * self.height as usize * 3);
        for pixel in self.pixels.chunks_exact(4) {
            let alpha = pixel[3] as f32 / 255.0;
            for (source, backdrop) in pixel.iter().take(3).zip(background.iter()) {
                let value = *source as f32 * alpha + *backdrop as f32 * (1.0 - alpha);
                output.push(value.round().clamp(0.0, 255.0) as u8);
            }
        }
        output
    }

    /// Computes 256-bin histograms for R, G, B, and Luminance channels.
    pub fn compute_histogram(&self) -> ImageHistogram {
        let mut r = [0u32; 256];
        let mut g = [0u32; 256];
        let mut b = [0u32; 256];
        let mut luma = [0u32; 256];
        for pixel in self.pixels.chunks_exact(4) {
            let pr = pixel[0] as usize;
            let pg = pixel[1] as usize;
            let pb = pixel[2] as usize;
            let y = (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32)
                .round()
                .clamp(0.0, 255.0) as usize;
            r[pr] += 1;
            g[pg] += 1;
            b[pb] += 1;
            luma[y] += 1;
        }
        ImageHistogram { r, g, b, luma }
    }
}

/// Standard crop aspect ratio presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropAspectRatio {
    Free,
    Square1x1,
    Standard4x3,
    Widescreen16x9,
    Photo3x2,
}

impl CropAspectRatio {
    /// Ratio of width / height.
    pub fn ratio(&self) -> Option<f32> {
        match self {
            CropAspectRatio::Free => None,
            CropAspectRatio::Square1x1 => Some(1.0),
            CropAspectRatio::Standard4x3 => Some(4.0 / 3.0),
            CropAspectRatio::Widescreen16x9 => Some(16.0 / 9.0),
            CropAspectRatio::Photo3x2 => Some(3.0 / 2.0),
        }
    }
}

/// Generates a 1D normalized Gaussian kernel of length `2 * radius + 1`.
pub fn generate_gaussian_kernel(radius: u32, sigma: f32) -> Vec<f32> {
    if radius == 0 {
        return vec![1.0];
    }
    let s = if sigma > 0.0 {
        sigma
    } else {
        radius as f32 / 2.0
    };
    let size = (radius * 2 + 1) as usize;
    let mut kernel = Vec::with_capacity(size);
    let mut sum = 0.0_f32;

    for i in -(radius as i32)..=(radius as i32) {
        let x = i as f32;
        let weight = (-x * x / (2.0 * s * s)).exp();
        kernel.push(weight);
        sum += weight;
    }

    if sum > 0.0 {
        for w in &mut kernel {
            *w /= sum;
        }
    }
    kernel
}

/// Generates a radial gradient image from center_x, center_y outward to radius.
pub fn generate_radial_gradient(
    width: u32,
    height: u32,
    center_x: f32,
    center_y: f32,
    radius: f32,
    inner_color: [u8; 4],
    outer_color: [u8; 4],
) -> Result<RgbaImage, String> {
    let mut img = RgbaImage::transparent(width, height)?;
    let rad = radius.max(1.0);

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let t = (dist / rad).clamp(0.0, 1.0);

            let r = (inner_color[0] as f32 * (1.0 - t) + outer_color[0] as f32 * t).round() as u8;
            let g = (inner_color[1] as f32 * (1.0 - t) + outer_color[1] as f32 * t).round() as u8;
            let b = (inner_color[2] as f32 * (1.0 - t) + outer_color[2] as f32 * t).round() as u8;
            let a = (inner_color[3] as f32 * (1.0 - t) + outer_color[3] as f32 * t).round() as u8;

            img.set_pixel(x, y, [r, g, b, a]);
        }
    }

    Ok(img)
}

/// 256-entry lookup table for tone curve mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToneCurveLUT {
    pub map: [u8; 256],
}

impl ToneCurveLUT {
    /// Identity tone curve (output == input).
    pub fn identity() -> Self {
        let mut map = [0u8; 256];
        for (i, val) in map.iter_mut().enumerate() {
            *val = i as u8;
        }
        Self { map }
    }

    /// Inverted tone curve (output == 255 - input).
    pub fn inverted() -> Self {
        let mut map = [0u8; 256];
        for (i, val) in map.iter_mut().enumerate() {
            *val = (255 - i) as u8;
        }
        Self { map }
    }

    /// Generates an S-curve contrast lookup table with strength in `[0.0, 2.0]`.
    pub fn s_curve(strength: f32) -> Self {
        let mut map = [0u8; 256];
        let s = strength.clamp(0.0, 2.0);
        for (i, val) in map.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            // Sigmoidal / Hermite S-curve blending: (3x^2 - 2x^3) * s + x * (1 - s)
            let s_val = x * x * (3.0 - 2.0 * x);
            let out = (s_val * s + x * (1.0 - s)).clamp(0.0, 1.0);
            *val = (out * 255.0).round() as u8;
        }
        Self { map }
    }
}

/// 3-way color grading controls (Lift, Gamma, Gain) for shadows, midtones, and highlights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftGammaGain {
    /// Lift (shadows RGB offset, default: (0.0, 0.0, 0.0)).
    pub lift: (f32, f32, f32),
    /// Gamma (midtones RGB exponent, default: (1.0, 1.0, 1.0)).
    pub gamma: (f32, f32, f32),
    /// Gain (highlights RGB multiplier, default: (1.0, 1.0, 1.0)).
    pub gain: (f32, f32, f32),
}

impl Default for LiftGammaGain {
    fn default() -> Self {
        Self {
            lift: (0.0, 0.0, 0.0),
            gamma: (1.0, 1.0, 1.0),
            gain: (1.0, 1.0, 1.0),
        }
    }
}

/// Photographic lens vignette configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct VignetteConfig {
    /// Vignette intensity in `[-1.0, 1.0]`. Negative values darken corners; positive values lighten.
    pub amount: f32,
    /// Center clear radius in `[0.0, 1.0]`.
    pub midpoint: f32,
    /// Softness falloff width in `[0.0, 1.0]`.
    pub feather: f32,
    /// Shape roundness from 0.0 (aspect-ratio bound) to 1.0 (circular).
    pub roundness: f32,
}

impl Default for VignetteConfig {
    fn default() -> Self {
        Self {
            amount: -0.5,
            midpoint: 0.5,
            feather: 0.5,
            roundness: 1.0,
        }
    }
}

/// Optical lens distortion correction and simulation parameters (Brown-Conrady radial model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensDistortionConfig {
    /// Radial quadratic coefficient (negative: barrel, positive: pincushion).
    pub k1: f32,
    /// Radial quartic coefficient.
    pub k2: f32,
    /// Output scaling / framing factor.
    pub scale: f32,
}

impl Default for LensDistortionConfig {
    fn default() -> Self {
        Self {
            k1: -0.1,
            k2: 0.0,
            scale: 1.0,
        }
    }
}

/// Lateral and transverse chromatic aberration correction/simulation parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChromaticAberrationConfig {
    /// Cartesian pixel shift applied to the red channel (dx, dy).
    pub red_shift: (f32, f32),
    /// Cartesian pixel shift applied to the blue channel (dx, dy).
    pub blue_shift: (f32, f32),
    /// Radial lateral color fringing factor scaled with distance from image center.
    pub radial_fringe: f32,
}

impl Default for ChromaticAberrationConfig {
    fn default() -> Self {
        Self {
            red_shift: (1.0, 0.0),
            blue_shift: (-1.0, 0.0),
            radial_fringe: 0.0,
        }
    }
}

/// Converts a hue angle in degrees [0, 360) to a normalized full-saturation RGB tuple.
pub fn hue_to_rgb(hue_deg: f32) -> [f32; 3] {
    let h = ((hue_deg % 360.0 + 360.0) % 360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    if h < 1.0 {
        [1.0, x, 0.0]
    } else if h < 2.0 {
        [x, 1.0, 0.0]
    } else if h < 3.0 {
        [0.0, 1.0, x]
    } else if h < 4.0 {
        [0.0, x, 1.0]
    } else if h < 5.0 {
        [x, 0.0, 1.0]
    } else {
        [1.0, 0.0, x]
    }
}

/// Split toning / color balance dual-tone adjustment configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitToningConfig {
    /// Shadows tint hue in degrees [0, 360).
    pub shadows_hue_deg: f32,
    /// Shadows tint saturation / strength [0.0, 1.0].
    pub shadows_saturation: f32,
    /// Highlights tint hue in degrees [0, 360).
    pub highlights_hue_deg: f32,
    /// Highlights tint saturation / strength [0.0, 1.0].
    pub highlights_saturation: f32,
    /// Balance shift between shadows and highlights [-1.0, 1.0].
    pub balance: f32,
}

impl Default for SplitToningConfig {
    fn default() -> Self {
        Self {
            shadows_hue_deg: 215.0, // Cool teal/blue
            shadows_saturation: 0.25,
            highlights_hue_deg: 35.0, // Warm amber/orange
            highlights_saturation: 0.25,
            balance: 0.0,
        }
    }
}

/// Analog photographic film grain simulation parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilmGrainConfig {
    /// Grain intensity / amount [0.0, 1.0].
    pub amount: f32,
    /// Color noise vs monochromatic film grain.
    pub is_colored: bool,
    /// Randomization seed.
    pub seed: u64,
}

impl Default for FilmGrainConfig {
    fn default() -> Self {
        Self {
            amount: 0.12,
            is_colored: false,
            seed: 1337,
        }
    }
}

/// 256-bin color channel and luminance histograms.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageHistogram {
    /// Red channel bins [0..=255].
    pub r: [u32; 256],
    /// Green channel bins [0..=255].
    pub g: [u32; 256],
    /// Blue channel bins [0..=255].
    pub b: [u32; 256],
    /// Rec. 709 Luminance bins [0..=255].
    pub luma: [u32; 256],
}

/// 2D affine transformation matrix.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AffineTransform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl AffineTransform2D {
    /// Identity matrix.
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Translation matrix.
    pub fn translation(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx,
            ty,
        }
    }

    /// Scale matrix.
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Rotation matrix by angle in radians.
    pub fn rotation(radians: f32) -> Self {
        let cos = radians.cos();
        let sin = radians.sin();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Transforms a point `(x, y)`.
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }
}

impl Default for AffineTransform2D {
    fn default() -> Self {
        Self::identity()
    }
}

fn image_byte_len(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err("image dimensions must be non-zero".into());
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| "image dimensions overflow".to_string())?;
    if pixels > MAX_REFERENCE_PIXELS {
        return Err(format!(
            "image has {pixels} pixels; reference compositor limit is {MAX_REFERENCE_PIXELS}"
        ));
    }
    pixels
        .checked_mul(4)
        .ok_or_else(|| "image byte length overflow".to_string())
}

/// Pixel and mask content associated with a metadata-only [`PhotoDocument`].
#[derive(Debug, Clone)]
pub struct PhotoCanvas {
    /// Nondestructive document metadata.
    pub document: PhotoDocument,
    layer_images: BTreeMap<String, RgbaImage>,
    layer_masks: BTreeMap<String, Vec<u8>>,
}

impl PhotoCanvas {
    /// Creates an empty canvas for an existing document.
    pub fn new(document: PhotoDocument) -> Result<Self, String> {
        image_byte_len(document.width, document.height)?;
        Ok(Self {
            document,
            layer_images: BTreeMap::new(),
            layer_masks: BTreeMap::new(),
        })
    }

    /// Attaches pixels to a pixel layer.
    pub fn set_layer_image(&mut self, layer_id: &str, image: RgbaImage) -> Result<(), String> {
        image.validate()?;
        if image.width != self.document.width || image.height != self.document.height {
            return Err("layer image dimensions must match the document".into());
        }
        let layer = self
            .document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or_else(|| format!("unknown layer {layer_id}"))?;
        if layer.kind != LayerKind::Pixel {
            return Err(format!("layer {layer_id} is not a pixel layer"));
        }
        self.layer_images.insert(layer_id.to_string(), image);
        Ok(())
    }

    /// Attaches an 8-bit mask to a layer; 0 hides and 255 reveals.
    pub fn set_layer_mask(&mut self, layer_id: &str, mask: Vec<u8>) -> Result<(), String> {
        if !self
            .document
            .layers
            .iter()
            .any(|layer| layer.id == layer_id)
        {
            return Err(format!("unknown layer {layer_id}"));
        }
        let expected = self.document.width as usize * self.document.height as usize;
        if mask.len() != expected {
            return Err(format!(
                "mask length {} does not match document pixel count {expected}",
                mask.len()
            ));
        }
        self.layer_masks.insert(layer_id.to_string(), mask);
        Ok(())
    }

    /// Inverts all values in a layer mask (255 - value).
    pub fn invert_layer_mask(&mut self, layer_id: &str) -> Result<(), String> {
        let mask = self
            .layer_masks
            .get_mut(layer_id)
            .ok_or_else(|| format!("layer {layer_id} has no attached mask"))?;
        for byte in mask.iter_mut() {
            *byte = 255 - *byte;
        }
        Ok(())
    }

    /// Applies a hard threshold to a layer mask (values >= threshold become 255, else 0).
    pub fn apply_mask_threshold(&mut self, layer_id: &str, threshold: u8) -> Result<(), String> {
        let mask = self
            .layer_masks
            .get_mut(layer_id)
            .ok_or_else(|| format!("layer {layer_id} has no attached mask"))?;
        for byte in mask.iter_mut() {
            *byte = if *byte >= threshold { 255 } else { 0 };
        }
        Ok(())
    }

    /// Returns attached pixels for a layer.
    pub fn layer_image(&self, layer_id: &str) -> Option<&RgbaImage> {
        self.layer_images.get(layer_id)
    }

    /// Returns an attached mask for a layer.
    pub fn layer_mask(&self, layer_id: &str) -> Option<&[u8]> {
        self.layer_masks.get(layer_id).map(Vec::as_slice)
    }

    /// Removes pixel and mask payloads for a layer.
    pub fn remove_layer_payload(&mut self, layer_id: &str) {
        self.layer_images.remove(layer_id);
        self.layer_masks.remove(layer_id);
    }

    /// Returns the number of pixel payloads stored by this canvas.
    pub fn pixel_payload_count(&self) -> usize {
        self.layer_images.len()
    }

    /// Composites all visible layers bottom-to-top.
    pub fn composite(&self) -> Result<RgbaImage, String> {
        let mut output = RgbaImage::transparent(self.document.width, self.document.height)?;
        for layer in &self.document.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }
            match layer.kind {
                LayerKind::Pixel => {
                    let Some(source) = self.layer_images.get(&layer.id) else {
                        continue;
                    };
                    let mask = self.layer_masks.get(&layer.id);
                    blend_image(&mut output, source, mask, layer.opacity, &layer.blend_mode);
                }
                LayerKind::Adjustment => apply_adjustment(
                    &mut output,
                    layer.adjustment_type.as_deref().unwrap_or("brightness"),
                    layer.adjustment_value,
                    layer.opacity,
                    self.layer_masks.get(&layer.id),
                ),
                LayerKind::Text | LayerKind::Vector => {
                    // Text and vector layers are represented by the document model; a
                    // future vector renderer rasterizes them before this compositor.
                }
            }
        }
        Ok(output)
    }
}

/// A serializable, undoable authoring session around a [`PhotoCanvas`].
#[derive(Debug, Clone)]
pub struct PhotoSession {
    /// Current nondestructive canvas.
    pub canvas: PhotoCanvas,
    undo: Vec<PhotoCanvas>,
    redo: Vec<PhotoCanvas>,
    history_limit: usize,
}

impl PhotoSession {
    /// Creates a session with bounded snapshot history.
    pub fn new(canvas: PhotoCanvas) -> Self {
        Self {
            canvas,
            undo: Vec::new(),
            redo: Vec::new(),
            history_limit: 32,
        }
    }

    /// Returns whether undo is currently possible.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Returns whether redo is currently possible.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Stores the current canvas before a mutation.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.canvas.clone());
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Restores the previous canvas state.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo
            .push(std::mem::replace(&mut self.canvas, previous));
        true
    }

    /// Reapplies the next canvas state.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.canvas, next));
        true
    }

    /// Adds an empty pixel layer and records history.
    pub fn add_pixel_layer(&mut self, id: impl Into<String>, name: impl Into<String>) {
        self.checkpoint();
        self.canvas.document.add_layer(Layer::new_pixel(id, name));
    }

    /// Adds an adjustment layer and records history.
    pub fn add_adjustment(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        adjustment_type: impl Into<String>,
        value: f32,
    ) {
        self.checkpoint();
        self.canvas
            .document
            .add_layer(Layer::new_adjustment(id, name, adjustment_type, value));
    }

    /// Removes a layer while keeping at least one layer in the document.
    pub fn remove_layer(&mut self, index: usize) -> bool {
        if self.canvas.document.layers.len() <= 1 || index >= self.canvas.document.layers.len() {
            return false;
        }
        self.checkpoint();
        let removed = self.canvas.document.layers.remove(index);
        self.canvas.remove_layer_payload(&removed.id);
        self.canvas.document.active_layer_index = self
            .canvas
            .document
            .active_layer_index
            .min(self.canvas.document.layers.len().saturating_sub(1));
        true
    }

    /// Moves a layer in the compositing order.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        if from >= self.canvas.document.layers.len()
            || to >= self.canvas.document.layers.len()
            || from == to
        {
            return false;
        }
        self.checkpoint();
        let layer = self.canvas.document.layers.remove(from);
        self.canvas.document.layers.insert(to, layer);
        self.canvas.document.active_layer_index = to;
        true
    }
}

/// Encodes the full nondestructive canvas, including pixel and mask payloads.
pub fn save_photo_canvas(canvas: &PhotoCanvas) -> Result<Vec<u8>, String> {
    canvas.composite()?;
    let json = serde_json::to_vec_pretty(&canvas.document).map_err(|error| error.to_string())?;
    let mut archive = PackageArchive::new();
    let mut entries = Vec::new();
    add_canvas_entry(
        &mut archive,
        &mut entries,
        "content/photo.json",
        "application/vnd.loom.photo-content",
        json,
    )?;

    for (index, layer) in canvas.document.layers.iter().enumerate() {
        if let Some(image) = canvas.layer_images.get(&layer.id) {
            image.validate()?;
            add_canvas_entry(
                &mut archive,
                &mut entries,
                &format!("assets/layers/{index}.rgba"),
                "application/vnd.loom.rgba8",
                image.pixels.clone(),
            )?;
        }
        if let Some(mask) = canvas.layer_masks.get(&layer.id) {
            add_canvas_entry(
                &mut archive,
                &mut entries,
                &format!("assets/masks/{index}.gray8"),
                "application/vnd.loom.gray8",
                mask.clone(),
            )?;
        }
    }

    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Photo,
        id: canvas.document.id.clone(),
        title: canvas.document.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries,
    };
    archive
        .add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|error| error.to_string())?;
    archive.to_bytes().map_err(|error| error.to_string())
}

fn add_canvas_entry(
    archive: &mut PackageArchive,
    entries: &mut Vec<ManifestEntry>,
    path: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let entry = ManifestEntry {
        path: path.to_string(),
        mime: MimeType::parse(mime)
            .map_err(|error| format!("invalid built-in MIME type: {error}"))?,
        size: bytes.len() as u64,
        sha256: Checksum::from_bytes(zip::sha256(&bytes)),
    };
    archive
        .add(path, bytes)
        .map_err(|error| error.to_string())?;
    entries.push(entry);
    Ok(())
}

/// Loads a full nondestructive canvas. Metadata-only legacy projects remain valid.
pub fn load_photo_canvas(bytes: &[u8]) -> Result<PhotoCanvas, String> {
    let archive = PackageArchive::from_bytes(bytes).map_err(|error| error.to_string())?;
    let manifest_bytes = archive
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_text =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest =
        pkg_json::parse_manifest(manifest_text).map_err(|error| format!("manifest: {error}"))?;
    if manifest.kind != PackageKind::Photo {
        return Err("not a Loom Photo project".into());
    }
    archive
        .validate_manifest(&manifest)
        .map_err(|error| format!("validation: {error}"))?;
    let content = archive
        .get("content/photo.json")
        .ok_or_else(|| "missing content/photo.json".to_string())?;
    let document: PhotoDocument =
        serde_json::from_slice(content).map_err(|error| format!("parse payload: {error}"))?;
    let width = document.width;
    let height = document.height;
    let mut canvas = PhotoCanvas::new(document)?;
    let expected_rgba = image_byte_len(width, height)?;
    let expected_mask = width as usize * height as usize;

    for (index, layer) in canvas.document.layers.clone().iter().enumerate() {
        let image_path = format!("assets/layers/{index}.rgba");
        if let Some(payload) = archive.get(&image_path) {
            if payload.len() != expected_rgba {
                return Err(format!("{image_path} has invalid byte length"));
            }
            canvas.set_layer_image(
                &layer.id,
                RgbaImage {
                    width,
                    height,
                    pixels: payload.to_vec(),
                },
            )?;
        }
        let mask_path = format!("assets/masks/{index}.gray8");
        if let Some(payload) = archive.get(&mask_path) {
            if payload.len() != expected_mask {
                return Err(format!("{mask_path} has invalid byte length"));
            }
            canvas.set_layer_mask(&layer.id, payload.to_vec())?;
        }
    }
    Ok(canvas)
}

/// Decodes a supported raster file into the reference RGBA representation.
pub fn decode_raster(bytes: &[u8]) -> Result<RgbaImage, String> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| format!("decode image: {error}"))?
        .into_rgba8();
    let result = RgbaImage {
        width: decoded.width(),
        height: decoded.height(),
        pixels: decoded.into_raw(),
    };
    result.validate()?;
    Ok(result)
}

/// Encodes an RGBA image as PNG.
pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    image.validate()?;
    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.pixels.clone())
        .ok_or_else(|| "invalid RGBA buffer".to_string())?;
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|error| format!("encode PNG: {error}"))?;
    Ok(cursor.into_inner())
}

/// Encodes an RGBA image as JPEG, flattening transparency against white.
pub fn encode_jpeg(image: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    image.validate()?;
    let mut rgb = Vec::with_capacity(image.width as usize * image.height as usize * 3);
    for pixel in image.pixels.chunks_exact(4) {
        let alpha = pixel[3] as f32 / 255.0;
        for source in pixel.iter().take(3) {
            let value = *source as f32 * alpha + 255.0 * (1.0 - alpha);
            rgb.push(value.round().clamp(0.0, 255.0) as u8);
        }
    }
    let mut output = Vec::new();
    let encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality.clamp(1, 100));
    encoder
        .write_image(
            &rgb,
            image.width,
            image.height,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("encode JPEG: {error}"))?;
    Ok(output)
}

fn blend_image(
    destination: &mut RgbaImage,
    source: &RgbaImage,
    mask: Option<&Vec<u8>>,
    opacity: f32,
    mode: &BlendMode,
) {
    for (pixel_index, (dst, src)) in destination
        .pixels
        .chunks_exact_mut(4)
        .zip(source.pixels.chunks_exact(4))
        .enumerate()
    {
        let mask_alpha = mask
            .map(|mask| mask[pixel_index] as f32 / 255.0)
            .unwrap_or(1.0);
        let source_alpha = src[3] as f32 / 255.0 * opacity.clamp(0.0, 1.0) * mask_alpha;
        let destination_alpha = dst[3] as f32 / 255.0;
        let out_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
        if out_alpha <= f32::EPSILON {
            dst.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for channel in 0..3 {
            let source_channel = src[channel] as f32 / 255.0;
            let destination_channel = dst[channel] as f32 / 255.0;
            let blended = blend_channel(source_channel, destination_channel, mode);
            let premultiplied = blended * source_alpha
                + destination_channel * destination_alpha * (1.0 - source_alpha);
            dst[channel] = (premultiplied / out_alpha * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        dst[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

fn blend_channel(source: f32, destination: f32, mode: &BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => source,
        BlendMode::Multiply => source * destination,
        BlendMode::Screen => 1.0 - (1.0 - source) * (1.0 - destination),
        BlendMode::Overlay => {
            if destination <= 0.5 {
                2.0 * source * destination
            } else {
                1.0 - 2.0 * (1.0 - source) * (1.0 - destination)
            }
        }
        BlendMode::Darken => source.min(destination),
        BlendMode::Lighten => source.max(destination),
        BlendMode::Difference => (source - destination).abs(),
        BlendMode::HardLight => {
            if source <= 0.5 {
                2.0 * source * destination
            } else {
                1.0 - 2.0 * (1.0 - source) * (1.0 - destination)
            }
        }
    }
}

fn apply_adjustment(
    image: &mut RgbaImage,
    adjustment_type: &str,
    value: f32,
    opacity: f32,
    mask: Option<&Vec<u8>>,
) {
    let normalized_name = adjustment_type.trim().to_ascii_lowercase();
    for (pixel_index, pixel) in image.pixels.chunks_exact_mut(4).enumerate() {
        let mask_alpha = mask
            .map(|mask| mask[pixel_index] as f32 / 255.0)
            .unwrap_or(1.0);
        let strength = opacity.clamp(0.0, 1.0) * mask_alpha;
        if strength <= 0.0 {
            continue;
        }
        let original = [pixel[0] as f32, pixel[1] as f32, pixel[2] as f32];
        let mut adjusted = original;
        match normalized_name.as_str() {
            "exposure" => {
                let multiplier = 2.0_f32.powf(value.clamp(-8.0, 8.0));
                for channel in &mut adjusted {
                    *channel *= multiplier;
                }
            }
            "contrast" => {
                let factor = 1.0 + value.clamp(-1.0, 4.0);
                for channel in &mut adjusted {
                    *channel = ((*channel / 255.0 - 0.5) * factor + 0.5) * 255.0;
                }
            }
            "saturation" => {
                let luma = 0.299 * original[0] + 0.587 * original[1] + 0.114 * original[2];
                let factor = 1.0 + value.clamp(-1.0, 4.0);
                for channel in &mut adjusted {
                    *channel = luma + (*channel - luma) * factor;
                }
            }
            "invert" => {
                for channel in &mut adjusted {
                    *channel = 255.0 - *channel;
                }
            }
            "gamma" => {
                let exponent = 1.0 / value.clamp(0.1, 10.0);
                for channel in &mut adjusted {
                    *channel = ((*channel / 255.0).powf(exponent)) * 255.0;
                }
            }
            "temperature" => {
                let warm = value.clamp(-1.0, 1.0) * 50.0;
                adjusted[0] += warm;
                adjusted[2] -= warm;
            }
            "tint" => {
                let tint = value.clamp(-1.0, 1.0) * 50.0;
                adjusted[1] += tint;
            }
            "sepia" => {
                let r = original[0];
                let g = original[1];
                let b = original[2];
                let sepia_r = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
                let sepia_g = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
                let sepia_b = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
                let t = value.clamp(0.0, 1.0);
                adjusted[0] = r + (sepia_r - r) * t;
                adjusted[1] = g + (sepia_g - g) * t;
                adjusted[2] = b + (sepia_b - b) * t;
            }
            _ => {
                let offset = value.clamp(-1.0, 1.0) * 255.0;
                for channel in &mut adjusted {
                    *channel += offset;
                }
            }
        }
        for channel in 0..3 {
            pixel[channel] = (original[channel]
                + (adjusted[channel] - original[channel]) * strength)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

/// Round brush configuration with hardness-controlled falloff.
#[derive(Debug, Clone, PartialEq)]
pub struct BrushConfig {
    /// Brush radius in pixels (> 0).
    pub radius: f32,
    /// 0.0 = fully soft feathered edge, 1.0 = hard edge. Clamped to [0, 1].
    pub hardness: f32,
    /// Per-dab opacity multiplier in [0, 1].
    pub opacity: f32,
    /// Brush RGBA color.
    pub color: [u8; 4],
}

impl Default for BrushConfig {
    fn default() -> Self {
        Self {
            radius: 12.0,
            hardness: 0.8,
            opacity: 1.0,
            color: [0, 0, 0, 255],
        }
    }
}

/// Returns the normalized dab alpha in [0, 1] at a pixel whose distance from the dab
/// center is `distance`. `distance >= radius` yields 0; within the hard core
/// (`distance <= hardness * radius`) it yields 1; otherwise it falls off linearly.
pub fn brush_dab_alpha(distance: f32, config: &BrushConfig) -> f64 {
    let radius = config.radius.max(f32::EPSILON);
    if distance >= radius {
        return 0.0;
    }
    let hard_edge = config.hardness.clamp(0.0, 1.0) * radius;
    if distance <= hard_edge {
        return 1.0;
    }
    ((radius - distance) / (radius - hard_edge)) as f64
}

/// Stamps one circular dab centered at (`cx`, `cy`) using source-over compositing.
/// The canvas is treated as an opaque backdrop: RGB channels blend toward the dab
/// color by the dab alpha and the destination alpha moves toward 255 accordingly.
fn stamp_dab(canvas: &mut RgbaImage, cx: f32, cy: f32, config: &BrushConfig) {
    if config.radius <= 0.0 {
        return;
    }
    let opacity = config.opacity.clamp(0.0, 1.0) as f64;
    if opacity <= 0.0 {
        return;
    }
    let radius = config.radius;
    let min_x = ((cx - radius).floor() as i64).max(0);
    let min_y = ((cy - radius).floor() as i64).max(0);
    let max_x = ((cx + radius).ceil() as i64).min(canvas.width as i64 - 1);
    let max_y = ((cy + radius).ceil() as i64).min(canvas.height as i64 - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let alpha = brush_dab_alpha((dx * dx + dy * dy).sqrt(), config) * opacity;
            if alpha <= 0.0 {
                continue;
            }
            let x = x as u32;
            let y = y as u32;
            let Some(destination) = canvas.pixel(x, y) else {
                continue;
            };
            let inverse = 1.0 - alpha;
            let blended = [
                (config.color[0] as f64 * alpha + destination[0] as f64 * inverse)
                    .round()
                    .clamp(0.0, 255.0) as u8,
                (config.color[1] as f64 * alpha + destination[1] as f64 * inverse)
                    .round()
                    .clamp(0.0, 255.0) as u8,
                (config.color[2] as f64 * alpha + destination[2] as f64 * inverse)
                    .round()
                    .clamp(0.0, 255.0) as u8,
                (alpha * 255.0 + destination[3] as f64 * inverse)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            ];
            canvas.set_pixel(x, y, blended);
        }
    }
}

/// Paints one straight brush stroke from `from` to `to`, stamping circular dabs spaced
/// `spacing` pixels apart along the line (including both endpoints), alpha-compositing
/// each dab over the canvas with `dab alpha * opacity`.
pub fn paint_stroke(
    canvas: &mut RgbaImage,
    from: (f32, f32),
    to: (f32, f32),
    spacing: f32,
    config: &BrushConfig,
) {
    let delta = (to.0 - from.0, to.1 - from.1);
    let length = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let spacing = spacing.abs().max(1e-3);
    let steps = (length / spacing).ceil().max(1.0);
    for step in 0..=steps as u32 {
        let t = step as f32 / steps;
        stamp_dab(canvas, from.0 + delta.0 * t, from.1 + delta.1 * t, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_photo_doc_creation() {
        let doc = PhotoDocument::new("photo-1", "Portrait Retouch", 3840, 2160);
        assert_eq!(doc.width, 3840);
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.layers[0].name, "Background");
    }

    #[test]
    fn test_add_layers() {
        let mut doc = PhotoDocument::new("photo-1", "Portrait Retouch", 1920, 1080);
        doc.add_layer(Layer::new_adjustment(
            "adj-1",
            "Exposure Adjustment",
            "Exposure",
            0.5,
        ));
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.layers[1].kind, LayerKind::Adjustment);
    }

    #[test]
    fn test_select_layer_rejects_invalid_index() {
        let mut doc = PhotoDocument::new("photo-1", "Portrait Retouch", 1920, 1080);
        doc.add_layer(Layer::new_pixel("layer-2", "Foreground"));
        assert!(doc.select_layer(0));
        assert!(!doc.select_layer(2));
        assert_eq!(doc.active_layer_index, 0);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut doc = PhotoDocument::new("photo-test", "Landscape Composite", 4000, 3000);
        doc.add_layer(Layer::new_pixel("layer-sky", "Sky Mask"));
        let bytes = save_photo(&doc).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Photo);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_photo(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Landscape Composite");
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn reference_compositor_blends_pixels_and_adjustments() {
        let mut doc = PhotoDocument::new("photo-small", "Composite", 2, 1);
        doc.layers[0].opacity = 1.0;
        doc.add_layer(Layer::new_pixel("top", "Top"));
        doc.layers[1].opacity = 0.5;
        doc.add_layer(Layer::new_adjustment("bright", "Bright", "brightness", 0.1));
        let mut canvas = PhotoCanvas::new(doc).expect("canvas");
        canvas
            .set_layer_image(
                "layer-bg",
                RgbaImage::solid(2, 1, [100, 100, 100, 255]).unwrap(),
            )
            .unwrap();
        canvas
            .set_layer_image("top", RgbaImage::solid(2, 1, [200, 0, 0, 255]).unwrap())
            .unwrap();
        let result = canvas.composite().expect("composite");
        let pixel = result.pixel(0, 0).unwrap();
        assert!(pixel[0] > 150);
        assert!(pixel[1] > 50);
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn masks_and_ppm_export_are_deterministic() {
        let doc = PhotoDocument::new("photo-mask", "Mask", 2, 1);
        let mut canvas = PhotoCanvas::new(doc).unwrap();
        canvas
            .set_layer_image(
                "layer-bg",
                RgbaImage::solid(2, 1, [255, 0, 0, 255]).unwrap(),
            )
            .unwrap();
        canvas.set_layer_mask("layer-bg", vec![255, 0]).unwrap();
        let result = canvas.composite().unwrap();
        assert_eq!(result.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(result.pixel(1, 0), Some([0, 0, 0, 0]));
        let ppm = result.to_ppm([255, 255, 255]);
        assert!(ppm.starts_with(b"P6\n2 1\n255\n"));
    }

    #[test]
    fn crop_and_resize_preserve_expected_pixels() {
        let mut image = RgbaImage::solid(2, 2, [0, 0, 0, 255]).unwrap();
        image.set_pixel(1, 1, [10, 20, 30, 255]);
        let crop = image.crop(1, 1, 1, 1).unwrap();
        assert_eq!(crop.pixel(0, 0), Some([10, 20, 30, 255]));
        let resized = crop.resize_nearest(3, 2).unwrap();
        assert_eq!(resized.pixel(2, 1), Some([10, 20, 30, 255]));
    }

    #[test]
    fn canvas_package_round_trip_preserves_pixels_masks_and_adjustments() {
        let mut document = PhotoDocument::new("canvas", "Canvas", 2, 1);
        document.add_layer(Layer::new_adjustment(
            "adjust", "Contrast", "contrast", 0.25,
        ));
        let mut canvas = PhotoCanvas::new(document).unwrap();
        canvas
            .set_layer_image(
                "layer-bg",
                RgbaImage::solid(2, 1, [30, 60, 90, 255]).unwrap(),
            )
            .unwrap();
        canvas.set_layer_mask("adjust", vec![255, 0]).unwrap();
        let bytes = save_photo_canvas(&canvas).unwrap();
        let loaded = load_photo_canvas(&bytes).unwrap();
        assert_eq!(loaded.pixel_payload_count(), 1);
        assert_eq!(loaded.layer_mask("adjust"), Some(&[255, 0][..]));
        assert_eq!(loaded.composite().unwrap(), canvas.composite().unwrap());
    }

    #[test]
    fn png_and_jpeg_are_real_decodable_exports() {
        let source = RgbaImage::solid(3, 2, [20, 40, 80, 255]).unwrap();
        let png = encode_png(&source).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(decode_raster(&png).unwrap(), source);
        let jpeg = encode_jpeg(&source, 90).unwrap();
        assert!(jpeg.starts_with(&[0xff, 0xd8, 0xff]));
        let decoded = decode_raster(&jpeg).unwrap();
        assert_eq!((decoded.width, decoded.height), (3, 2));
    }

    #[test]
    fn photo_session_undo_redo_restores_layer_stack() {
        let canvas = PhotoCanvas::new(PhotoDocument::new("session", "Session", 1, 1)).unwrap();
        let mut session = PhotoSession::new(canvas);
        session.add_adjustment("a", "Exposure", "exposure", 1.0);
        assert_eq!(session.canvas.document.layers.len(), 2);
        assert!(session.undo());
        assert_eq!(session.canvas.document.layers.len(), 1);
        assert!(session.redo());
        assert_eq!(session.canvas.document.layers.len(), 2);
    }

    #[test]
    fn image_transforms_flip_and_rotate_correctly() {
        let mut img = RgbaImage::transparent(2, 2).unwrap();
        img.set_pixel(0, 0, [255, 0, 0, 255]);
        img.set_pixel(1, 0, [0, 255, 0, 255]);
        img.set_pixel(0, 1, [0, 0, 255, 255]);
        img.set_pixel(1, 1, [255, 255, 255, 255]);

        let h_flipped = img.flip_horizontal().unwrap();
        assert_eq!(h_flipped.pixel(0, 0).unwrap(), [0, 255, 0, 255]);
        assert_eq!(h_flipped.pixel(1, 0).unwrap(), [255, 0, 0, 255]);

        let v_flipped = img.flip_vertical().unwrap();
        assert_eq!(v_flipped.pixel(0, 0).unwrap(), [0, 0, 255, 255]);
        assert_eq!(v_flipped.pixel(0, 1).unwrap(), [255, 0, 0, 255]);

        let rotated = img.rotate_90_cw().unwrap();
        assert_eq!(rotated.width, 2);
        assert_eq!(rotated.height, 2);
        assert_eq!(rotated.pixel(1, 0).unwrap(), [255, 0, 0, 255]);
    }

    #[test]
    fn blend_modes_and_adjustments_evaluate_correctly() {
        assert_eq!(blend_channel(0.4, 0.6, &BlendMode::Darken), 0.4);
        assert_eq!(blend_channel(0.4, 0.6, &BlendMode::Lighten), 0.6);
        assert!((blend_channel(0.4, 0.6, &BlendMode::Difference) - 0.2).abs() < 1e-5);
        assert!((blend_channel(0.4, 0.6, &BlendMode::HardLight) - 0.48).abs() < 1e-5);

        let mut img = RgbaImage::solid(1, 1, [100, 150, 200, 255]).unwrap();
        apply_adjustment(&mut img, "invert", 1.0, 1.0, None);
        assert_eq!(img.pixel(0, 0).unwrap(), [155, 105, 55, 255]);
    }

    #[test]
    fn compute_histogram_counts_channel_distributions() {
        let mut img = RgbaImage::transparent(2, 2).unwrap();
        img.set_pixel(0, 0, [255, 0, 0, 255]);
        img.set_pixel(1, 0, [0, 255, 0, 255]);
        img.set_pixel(0, 1, [0, 0, 255, 255]);
        img.set_pixel(1, 1, [255, 255, 255, 255]);

        let hist = img.compute_histogram();
        assert_eq!(hist.r[255], 2);
        assert_eq!(hist.r[0], 2);
        assert_eq!(hist.g[255], 2);
        assert_eq!(hist.b[255], 2);
    }

    #[test]
    fn aspect_ratio_crop_bounds_calculate_centered_rectangles() {
        let img = RgbaImage::transparent(1920, 1080).unwrap();
        // 16:9 on 1920x1080 -> exact match
        assert_eq!(
            img.aspect_crop_bounds(CropAspectRatio::Widescreen16x9),
            (0, 0, 1920, 1080)
        );

        // 1:1 on 1920x1080 -> centered 1080x1080, x = (1920-1080)/2 = 420
        assert_eq!(
            img.aspect_crop_bounds(CropAspectRatio::Square1x1),
            (420, 0, 1080, 1080)
        );

        // 4:3 on 1920x1080 -> width = 1080 * 4/3 = 1440, x = (1920-1440)/2 = 240
        assert_eq!(
            img.aspect_crop_bounds(CropAspectRatio::Standard4x3),
            (240, 0, 1440, 1080)
        );
    }

    #[test]
    fn tint_and_sepia_adjustments() {
        let mut img = RgbaImage::solid(1, 1, [100, 100, 100, 255]).unwrap();
        apply_adjustment(&mut img, "tint", 0.5, 1.0, None);
        let pix = img.pixel(0, 0).unwrap();
        assert_eq!(pix[0], 100);
        assert_eq!(pix[1], 125);
        assert_eq!(pix[2], 100);

        let mut sepia_img = RgbaImage::solid(1, 1, [100, 100, 100, 255]).unwrap();
        apply_adjustment(&mut sepia_img, "sepia", 1.0, 1.0, None);
        let sepia_pix = sepia_img.pixel(0, 0).unwrap();
        assert!(sepia_pix[0] > 130); // Red is enhanced in sepia
        assert!(sepia_pix[2] < 100); // Blue is reduced in sepia
    }

    #[test]
    fn layer_mask_inversion_and_threshold() {
        let doc = PhotoDocument::new("p1", "Mask Test", 2, 2);
        let mut canvas = PhotoCanvas::new(doc).unwrap();
        let layer_id = canvas.document.layers[0].id.clone();

        // Attach mask with values [0, 100, 200, 255]
        canvas
            .set_layer_mask(&layer_id, vec![0, 100, 200, 255])
            .unwrap();

        // Invert mask -> [255, 155, 55, 0]
        canvas.invert_layer_mask(&layer_id).unwrap();
        assert_eq!(canvas.layer_mask(&layer_id).unwrap(), &[255, 155, 55, 0]);

        // Threshold at 100 -> [255, 255, 0, 0]
        canvas.apply_mask_threshold(&layer_id, 100).unwrap();
        assert_eq!(canvas.layer_mask(&layer_id).unwrap(), &[255, 255, 0, 0]);
    }

    #[test]
    fn box_blur_smoothing() {
        let mut img = RgbaImage::transparent(3, 3).unwrap();
        // Set center pixel to white
        img.set_pixel(1, 1, [255, 255, 255, 255]);

        let blurred = img.box_blur(1).unwrap();
        // Corner pixels now receive spread intensity > 0
        let corner = blurred.pixel(0, 0).unwrap();
        assert!(corner[0] > 0);
        assert!(corner[3] > 0);

        // Radius 0 returns exact clone
        let clone_blur = img.box_blur(0).unwrap();
        assert_eq!(clone_blur.pixel(1, 1).unwrap(), [255, 255, 255, 255]);
    }

    #[test]
    fn affine_transform_2d_operations() {
        let t = AffineTransform2D::translation(10.0, 20.0);
        let p1 = t.transform_point(5.0, 5.0);
        assert_eq!(p1, (15.0, 25.0));

        let s = AffineTransform2D::scale(2.0, 3.0);
        let p2 = s.transform_point(4.0, 5.0);
        assert_eq!(p2, (8.0, 15.0));

        let r = AffineTransform2D::rotation(std::f32::consts::FRAC_PI_2);
        let (rx, ry) = r.transform_point(1.0, 0.0);
        assert!(rx.abs() < 1e-4);
        assert!((ry - 1.0).abs() < 1e-4);
    }

    #[test]
    fn gaussian_blur_filtering() {
        let kernel = generate_gaussian_kernel(2, 1.0);
        assert_eq!(kernel.len(), 5);
        let sum: f32 = kernel.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4);

        let mut img = RgbaImage::transparent(5, 5).unwrap();
        img.set_pixel(2, 2, [200, 100, 50, 255]);

        let blurred = img.gaussian_blur(1, 1.0).unwrap();
        let center = blurred.pixel(2, 2).unwrap();
        assert!(center[0] > 0 && center[0] < 200);

        let neighbor = blurred.pixel(2, 1).unwrap();
        assert!(neighbor[0] > 0);
    }

    #[test]
    fn radial_gradient_and_channel_extraction() {
        let grad =
            generate_radial_gradient(10, 10, 5.0, 5.0, 5.0, [255, 0, 0, 255], [0, 0, 255, 255])
                .unwrap();

        // Center should be red
        let center = grad.pixel(5, 5).unwrap();
        assert_eq!(center[0], 255);
        assert_eq!(center[2], 0);

        // Far edge should be blue
        let edge = grad.pixel(0, 0).unwrap();
        assert!(edge[2] > 200);

        // Channel extraction
        let red_ch = grad.extract_channel(0).unwrap();
        assert_eq!(red_ch.len(), 100);
        assert_eq!(red_ch[55], 255); // center pixel index in 10x10 is 5*10 + 5 = 55
    }

    #[test]
    fn tone_curve_lut_mapping() {
        let id_lut = ToneCurveLUT::identity();
        assert_eq!(id_lut.map[0], 0);
        assert_eq!(id_lut.map[128], 128);
        assert_eq!(id_lut.map[255], 255);

        let inv_lut = ToneCurveLUT::inverted();
        assert_eq!(inv_lut.map[0], 255);
        assert_eq!(inv_lut.map[255], 0);

        let s_lut = ToneCurveLUT::s_curve(1.0);
        // Midpoint 128 is ~0.5 -> mapped to ~128
        assert!((s_lut.map[128] as i32 - 128).abs() <= 1);
        // Shadows are pulled down, highlights pushed up
        assert!(s_lut.map[64] < 64);
        assert!(s_lut.map[192] > 192);

        let mut img = RgbaImage::transparent(2, 2).unwrap();
        img.set_pixel(0, 0, [100, 150, 200, 255]);
        img.apply_tone_curve(&inv_lut);

        let px = img.pixel(0, 0).unwrap();
        assert_eq!(px, [155, 105, 55, 255]); // 255 - input
    }

    #[test]
    fn lift_gamma_gain_color_grading() {
        let mut img = RgbaImage::transparent(2, 2).unwrap();
        img.set_pixel(0, 0, [128, 128, 128, 255]);

        let mut lgg = LiftGammaGain::default();
        // Boost gain (highlights multiplier) on Red
        lgg.gain.0 = 1.5;
        // Lift shadows on Blue
        lgg.lift.2 = 0.2;

        img.apply_lift_gamma_gain(&lgg);
        let px = img.pixel(0, 0).unwrap();
        assert!(px[0] > 180); // Red boosted by gain
        assert_eq!(px[1], 128); // Green default
        assert!(px[2] > 140); // Blue lifted
        assert_eq!(px[3], 255); // Alpha preserved
    }

    #[test]
    fn high_pass_and_unsharp_mask_filtering() {
        let mut img = RgbaImage::transparent(4, 4).unwrap();
        // Flat gray image -> high pass should produce exact neutral 128 gray
        for y in 0..4 {
            for x in 0..4 {
                img.set_pixel(x, y, [100, 100, 100, 255]);
            }
        }
        let hp = img.high_pass_filter(1, 1.0).unwrap();
        let hp_px = hp.pixel(1, 1).unwrap();
        assert_eq!(hp_px, [128, 128, 128, 255]);

        let usm = img.unsharp_mask(1, 1.0, 1.5, 5).unwrap();
        let usm_px = usm.pixel(1, 1).unwrap();
        assert_eq!(usm_px, [100, 100, 100, 255]); // Unchanged on flat area
    }

    #[test]
    fn vignette_filter_application() {
        let mut img = RgbaImage::transparent(10, 10).unwrap();
        for y in 0..10 {
            for x in 0..10 {
                img.set_pixel(x, y, [200, 200, 200, 255]);
            }
        }
        let vig = VignetteConfig {
            amount: -0.6,
            midpoint: 0.2,
            feather: 0.5,
            roundness: 1.0,
        };
        img.apply_vignette(&vig);

        // Center pixel (5, 5) should stay bright
        let center_px = img.pixel(5, 5).unwrap();
        assert!(center_px[0] >= 190);

        // Corner pixel (0, 0) should be darkened
        let corner_px = img.pixel(0, 0).unwrap();
        assert!(corner_px[0] < 120);
        assert_eq!(corner_px[3], 255); // Alpha preserved
    }

    #[test]
    fn lens_distortion_radial_correction() {
        let mut img = RgbaImage::transparent(16, 16).unwrap();
        // Fill center block with opaque white
        for y in 4..12 {
            for x in 4..12 {
                img.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        let barrel_config = LensDistortionConfig {
            k1: -0.2,
            k2: 0.0,
            scale: 1.0,
        };
        let distorted = img.apply_lens_distortion(&barrel_config).unwrap();
        assert_eq!(distorted.width, 16);
        assert_eq!(distorted.height, 16);

        // Center pixel (8, 8) must retain white color
        let center_px = distorted.pixel(8, 8).unwrap();
        assert_eq!(center_px, [255, 255, 255, 255]);
    }

    #[test]
    fn chromatic_aberration_channel_shifting() {
        let mut img = RgbaImage::transparent(10, 10).unwrap();
        // Set single pixel at center (5, 5) to white
        img.set_pixel(5, 5, [255, 255, 255, 255]);

        let config = ChromaticAberrationConfig {
            red_shift: (1.0, 0.0),
            blue_shift: (-1.0, 0.0),
            radial_fringe: 0.0,
        };
        let shifted = img.apply_chromatic_aberration(&config).unwrap();

        // Pixel (4, 5) should sample the shifted red channel from (5, 5)
        let px4 = shifted.pixel(4, 5).unwrap();
        assert_eq!(px4[0], 255); // Red shifted left to (4, 5)

        // Pixel (6, 5) should sample the shifted blue channel from (5, 5)
        let px6 = shifted.pixel(6, 5).unwrap();
        assert_eq!(px6[2], 255); // Blue shifted right to (6, 5)
    }

    #[test]
    fn split_toning_color_balance() {
        let mut img = RgbaImage::transparent(4, 4).unwrap();
        // Top half dark/shadows (30, 30, 30), bottom half bright/highlights (220, 220, 220)
        for y in 0..2 {
            for x in 0..4 {
                img.set_pixel(x, y, [30, 30, 30, 255]);
            }
        }
        for y in 2..4 {
            for x in 0..4 {
                img.set_pixel(x, y, [220, 220, 220, 255]);
            }
        }

        let config = SplitToningConfig {
            shadows_hue_deg: 240.0, // Pure blue shadows
            shadows_saturation: 0.5,
            highlights_hue_deg: 0.0, // Pure red highlights
            highlights_saturation: 0.5,
            balance: 0.0,
        };

        let toned = img.apply_split_toning(&config).unwrap();
        let shadow_px = toned.pixel(0, 0).unwrap();
        let highlight_px = toned.pixel(0, 3).unwrap();

        // Shadow pixel should have more blue than red
        assert!(shadow_px[2] > shadow_px[0]);

        // Highlight pixel should have more red than blue
        assert!(highlight_px[0] > highlight_px[2]);
    }

    #[test]
    fn film_grain_simulation() {
        let mut img = RgbaImage::transparent(8, 8).unwrap();
        // Midtone gray image (128, 128, 128)
        for y in 0..8 {
            for x in 0..8 {
                img.set_pixel(x, y, [128, 128, 128, 255]);
            }
        }

        let config = FilmGrainConfig {
            amount: 0.2,
            is_colored: false,
            seed: 9876,
        };

        let grained = img.apply_film_grain(&config).unwrap();
        let p0 = grained.pixel(0, 0).unwrap();
        let p1 = grained.pixel(1, 1).unwrap();

        // Grain should alter pixel values around 128
        assert_ne!(p0[0], 128);
        assert_ne!(p0, p1);

        // Monochromatic grain keeps R == G == B
        assert_eq!(p0[0], p0[1]);
        assert_eq!(p0[1], p0[2]);
    }

    #[test]
    fn brush_dab_and_stroke_painting() {
        let config = BrushConfig {
            radius: 10.0,
            hardness: 0.5,
            opacity: 1.0,
            color: [255, 0, 0, 255],
        };

        // Center of the dab is fully opaque, beyond the radius is empty.
        assert_eq!(brush_dab_alpha(0.0, &config), 1.0);
        assert_eq!(brush_dab_alpha(11.0, &config), 0.0);

        // Mid-feather (between hard core at 5.0 and radius at 10.0) is partial coverage.
        let mid_feather = brush_dab_alpha(7.5, &config);
        assert!(
            mid_feather > 0.0 && mid_feather < 1.0,
            "expected partial alpha in the feather, got {mid_feather}"
        );

        // Horizontal stroke with an opaque hard red brush on a white canvas.
        let mut canvas = RgbaImage::solid(100, 100, [255, 255, 255, 255]).unwrap();
        let hard_brush = BrushConfig {
            radius: 6.0,
            hardness: 1.0,
            opacity: 1.0,
            color: [220, 30, 30, 255],
        };
        paint_stroke(&mut canvas, (20.0, 50.0), (80.0, 50.0), 2.0, &hard_brush);

        let center = canvas.pixel(50, 50).unwrap();
        assert!(
            center[0] > 200 && center[1] < 60 && center[2] < 60,
            "stroke center line should be red-ish, got {center:?}"
        );

        // Far corners are untouched by the stroke.
        assert_eq!(canvas.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(canvas.pixel(99, 99), Some([255, 255, 255, 255]));
    }
}
