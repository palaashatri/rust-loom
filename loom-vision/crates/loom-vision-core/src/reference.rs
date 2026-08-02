//! Reference CPU providers: real, deterministic algorithms with no model
//! files and no network access.
//!
//! These providers exist so Loom applications have working implementations
//! of core capabilities out of the box, and so the framework can be tested
//! end-to-end without any installed models.

use std::collections::VecDeque;
use std::time::Duration;

use crate::error::VisionError;
use crate::provider::{
    image_to_luma_checked, Backend, BBox, CapabilityId, CapabilityProvider, InputType,
    ProviderDescriptor, ProviderInput, ProviderOutput, RunContext,
};

/// Message returned as [`VisionError::Internal`] when no QR code is found.
///
/// The CLI detects this exact message and prints `no QR found` to stdout
/// without treating it as a hard error.
pub const NO_QR_CODE_MESSAGE: &str = "no QR code found in image";

/// Reference QR-code detector and decoder.
///
/// Uses the pure-Rust [`rqrr`](https://crates.io/crates/rqrr) library on the
/// grayscale conversion of the input image. Accepts `"rgba"`, `"rgb"`, and
/// `"gray"` image inputs; anything else yields
/// [`VisionError::UnsupportedInput`]. Deterministic for identical inputs.
pub struct QrCodeProvider {
    descriptor: ProviderDescriptor,
}

impl QrCodeProvider {
    /// Creates the provider with its static descriptor.
    pub fn new() -> Self {
        let mut descriptor = ProviderDescriptor::new(CapabilityId::QrDetection);
        descriptor.name = "rqrr-reference-qr".to_string();
        descriptor.version = "0.1.0".to_string();
        descriptor.description = "Reference QR-code decoder using the pure-Rust rqrr library; accepts raw RGBA, RGB, or grayscale images.".to_string();
        descriptor.input_types = vec![InputType::Image];
        descriptor.output_schema =
            r#"{"type": "object", "properties": {"text": {"type": "string"}}}"#.to_string();
        descriptor.media_formats = ["png", "jpeg", "webp", "bmp", "gif", "tiff"]
            .map(String::from)
            .to_vec();
        descriptor.required_memory_bytes = 64 * 1024 * 1024;
        descriptor.estimated_latency = Duration::from_millis(150);
        descriptor.hardware_backends = vec![Backend::Cpu];
        descriptor.license = "MIT".to_string();
        descriptor.model_provenance = "none — pure algorithmic reference (rqrr 0.10)".to_string();
        descriptor.deterministic = true;
        descriptor.cancellation_support = true;
        descriptor.progress_support = true;
        QrCodeProvider { descriptor }
    }
}

impl Default for QrCodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProvider for QrCodeProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        ctx.check_cancelled()?;
        let (width, height, channels, data, _format) = match input {
            ProviderInput::Image {
                width,
                height,
                channels,
                data,
                format,
            } => (*width, *height, *channels, data.as_slice(), format.as_str()),
            _ => return Err(VisionError::UnsupportedInput),
        };

        // Grayscale conversion checks cancellation every 8 rows.
        let luma = image_to_luma_checked(width, height, channels, data, ctx)?;
        let image_width = width as usize;
        let image_height = height as usize;

        let mut prepared =
            rqrr::PreparedImage::prepare_from_bitmap(image_width, image_height, |x, y| {
                luma[y * image_width + x] < 128
            });
        ctx.check_cancelled()?;
        let grids = prepared.detect_grids();
        if grids.is_empty() {
            return Err(VisionError::Internal(NO_QR_CODE_MESSAGE.to_string()));
        }

        for (index, grid) in grids.iter().enumerate() {
            if index % 2 == 0 {
                ctx.check_cancelled()?;
            }
            if let Ok((_metadata, text)) = grid.decode() {
                if !text.is_empty() {
                    ctx.set_progress(1.0);
                    return Ok(ProviderOutput::QrDecoded { text });
                }
            }
        }
        Err(VisionError::Internal(NO_QR_CODE_MESSAGE.to_string()))
    }
}

/// Reference image-statistics provider.
///
/// Computes mean luma, population standard deviation of luma, and Michelson
/// contrast `(max - min) / (max + min)` over the grayscale conversion of the
/// input. Accepts `"rgba"`, `"rgb"`, and `"gray"` images. Deterministic.
pub struct ImageStatsProvider {
    descriptor: ProviderDescriptor,
}

impl ImageStatsProvider {
    /// Creates the provider with its static descriptor.
    pub fn new() -> Self {
        let mut descriptor = ProviderDescriptor::new(CapabilityId::ImageStats);
        descriptor.name = "loom-reference-image-stats".to_string();
        descriptor.version = "0.1.0".to_string();
        descriptor.description = "Reference image statistics: mean luma, luma standard deviation, and Michelson contrast over a raw image buffer.".to_string();
        descriptor.input_types = vec![InputType::Image];
        descriptor.output_schema = r#"{"type": "object", "properties": {"mean_luma": {"type": "number"}, "std_luma": {"type": "number"}, "contrast": {"type": "number"}}}"#.to_string();
        descriptor.media_formats = ["png", "jpeg", "webp", "bmp", "gif", "tiff", "gray"]
            .map(String::from)
            .to_vec();
        descriptor.required_memory_bytes = 16 * 1024 * 1024;
        descriptor.estimated_latency = Duration::from_micros(250);
        descriptor.hardware_backends = vec![Backend::Cpu];
        descriptor.license = "MIT".to_string();
        descriptor.model_provenance = "none — pure algorithmic reference".to_string();
        descriptor.deterministic = true;
        descriptor.cancellation_support = true;
        descriptor.progress_support = true;
        ImageStatsProvider { descriptor }
    }
}

impl Default for ImageStatsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProvider for ImageStatsProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        ctx.check_cancelled()?;
        let (width, height, channels, data, _format) = match input {
            ProviderInput::Image {
                width,
                height,
                channels,
                data,
                format,
            } => (*width, *height, *channels, data.as_slice(), format.as_str()),
            _ => return Err(VisionError::UnsupportedInput),
        };

        // Grayscale conversion checks cancellation every 8 rows.
        let luma = image_to_luma_checked(width, height, channels, data, ctx)?;
        ctx.check_cancelled()?;

        let count = luma.len() as f64;
        let sum: f64 = luma.iter().map(|&p| f64::from(p)).sum();
        let mean = sum / count;
        let variance = luma
            .iter()
            .map(|&p| {
                let delta = f64::from(p) - mean;
                delta * delta
            })
            .sum::<f64>()
            / count;
        let std_dev = variance.sqrt();

        let min = *luma.iter().min().unwrap_or(&0);
        let max = *luma.iter().max().unwrap_or(&0);
        let contrast = if u32::from(max) + u32::from(min) == 0 {
            0.0
        } else {
            f64::from(u32::from(max) - u32::from(min)) / f64::from(u32::from(max) + u32::from(min))
        };

        ctx.set_progress(1.0);
        Ok(ProviderOutput::ImageStats {
            mean_luma: mean as f32,
            std_luma: std_dev as f32,
            contrast: contrast as f32,
        })
    }
}


/// CPU reference foreground segmentation using Otsu thresholding.
pub struct ThresholdSegmentationProvider {
    descriptor: ProviderDescriptor,
}

impl ThresholdSegmentationProvider {
    /// Creates the provider.
    pub fn new() -> Self {
        let mut descriptor = ProviderDescriptor::new(CapabilityId::Segmentation);
        descriptor.name = "loom-reference-otsu-segmentation".into();
        descriptor.version = "0.1.0".into();
        descriptor.description = "Deterministic foreground mask using grayscale conversion and Otsu thresholding.".into();
        descriptor.input_types = vec![InputType::Image];
        descriptor.output_schema = r#"{"type":"object","properties":{"width":{"type":"integer"},"height":{"type":"integer"},"mask":{"type":"array","items":{"type":"integer"}}}}"#.into();
        descriptor.media_formats = ["png", "jpeg", "webp", "bmp", "tiff", "gray"]
            .map(String::from)
            .to_vec();
        descriptor.required_memory_bytes = 32 * 1024 * 1024;
        descriptor.estimated_latency = Duration::from_millis(8);
        descriptor.hardware_backends = vec![Backend::Cpu];
        descriptor.license = "MIT".into();
        descriptor.model_provenance = "none — Otsu reference algorithm".into();
        descriptor.deterministic = true;
        descriptor.cancellation_support = true;
        descriptor.progress_support = true;
        Self { descriptor }
    }
}

impl Default for ThresholdSegmentationProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProvider for ThresholdSegmentationProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        let (width, height, channels, data) = image_parts(input)?;
        let luma = image_to_luma_checked(width, height, channels, data, ctx)?;
        ctx.set_progress(0.45);
        let threshold = otsu_threshold(&luma);
        let mut mask = Vec::with_capacity(luma.len());
        for (index, value) in luma.iter().enumerate() {
            if index % (width as usize * 8).max(1) == 0 {
                ctx.check_cancelled()?;
            }
            // Printed/document foreground is usually darker than background.
            mask.push(if *value <= threshold { 255 } else { 0 });
        }
        ctx.set_progress(1.0);
        Ok(ProviderOutput::SegmentationMask {
            width,
            height,
            mask,
        })
    }
}

/// CPU reference document-layout provider using connected components.
pub struct DocumentLayoutProvider {
    descriptor: ProviderDescriptor,
}

impl DocumentLayoutProvider {
    /// Creates the provider.
    pub fn new() -> Self {
        let mut descriptor = ProviderDescriptor::new(CapabilityId::DocumentAnalysis);
        descriptor.name = "loom-reference-document-layout".into();
        descriptor.version = "0.1.0".into();
        descriptor.description = "Connected-component document region detection for high-contrast scans and screenshots.".into();
        descriptor.input_types = vec![InputType::Image];
        descriptor.output_schema = r#"{"type":"object","properties":{"boxes":{"type":"array"}}}"#.into();
        descriptor.media_formats = ["png", "jpeg", "webp", "bmp", "tiff", "gray"]
            .map(String::from)
            .to_vec();
        descriptor.required_memory_bytes = 64 * 1024 * 1024;
        descriptor.estimated_latency = Duration::from_millis(25);
        descriptor.hardware_backends = vec![Backend::Cpu];
        descriptor.license = "MIT".into();
        descriptor.model_provenance = "none — connected-component reference algorithm".into();
        descriptor.deterministic = true;
        descriptor.cancellation_support = true;
        descriptor.progress_support = true;
        Self { descriptor }
    }
}

impl Default for DocumentLayoutProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProvider for DocumentLayoutProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        let (width, height, channels, data) = image_parts(input)?;
        let luma = image_to_luma_checked(width, height, channels, data, ctx)?;
        let threshold = otsu_threshold(&luma);
        ctx.set_progress(0.25);
        let mut visited = vec![false; luma.len()];
        let mut boxes = Vec::new();
        let image_width = width as usize;
        let image_height = height as usize;
        for y in 0..image_height {
            if y % 8 == 0 {
                ctx.check_cancelled()?;
                ctx.set_progress(0.25 + 0.7 * y as f32 / image_height.max(1) as f32);
            }
            for x in 0..image_width {
                let index = y * image_width + x;
                if visited[index] || luma[index] > threshold {
                    continue;
                }
                let mut queue = VecDeque::from([(x, y)]);
                visited[index] = true;
                let mut min_x = x;
                let mut max_x = x;
                let mut min_y = y;
                let mut max_y = y;
                let mut area = 0usize;
                while let Some((current_x, current_y)) = queue.pop_front() {
                    area += 1;
                    min_x = min_x.min(current_x);
                    max_x = max_x.max(current_x);
                    min_y = min_y.min(current_y);
                    max_y = max_y.max(current_y);
                    for (next_x, next_y) in neighbours(current_x, current_y, image_width, image_height) {
                        let next_index = next_y * image_width + next_x;
                        if !visited[next_index] && luma[next_index] <= threshold {
                            visited[next_index] = true;
                            queue.push_back((next_x, next_y));
                        }
                    }
                }
                let component_width = max_x - min_x + 1;
                let component_height = max_y - min_y + 1;
                if area >= 4 && component_width >= 2 && component_height >= 2 {
                    let density = area as f32 / (component_width * component_height) as f32;
                    boxes.push(BBox {
                        x: min_x as f32,
                        y: min_y as f32,
                        w: component_width as f32,
                        h: component_height as f32,
                        label: if component_width > component_height * 4 {
                            "text-line"
                        } else if density < 0.35 {
                            "outline-region"
                        } else {
                            "content-region"
                        }
                        .into(),
                        confidence: density.clamp(0.1, 1.0),
                    });
                }
            }
        }
        boxes.sort_by(|left, right| {
            left.y
                .total_cmp(&right.y)
                .then_with(|| left.x.total_cmp(&right.x))
        });
        ctx.set_progress(1.0);
        Ok(ProviderOutput::DetectionResult { boxes })
    }
}

/// CPU reference image embedding based on an 8×8 luminance grid.
pub struct ImageEmbeddingProvider {
    descriptor: ProviderDescriptor,
}

impl ImageEmbeddingProvider {
    /// Creates the provider.
    pub fn new() -> Self {
        let mut descriptor = ProviderDescriptor::new(CapabilityId::Embedding);
        descriptor.name = "loom-reference-luma-embedding".into();
        descriptor.version = "0.1.0".into();
        descriptor.description = "L2-normalized 64-value luminance embedding for local similarity search.".into();
        descriptor.input_types = vec![InputType::Image];
        descriptor.output_schema = r#"{"type":"object","properties":{"values":{"type":"array","minItems":64,"maxItems":64}}}"#.into();
        descriptor.media_formats = ["png", "jpeg", "webp", "bmp", "tiff", "gray"]
            .map(String::from)
            .to_vec();
        descriptor.required_memory_bytes = 16 * 1024 * 1024;
        descriptor.estimated_latency = Duration::from_millis(3);
        descriptor.hardware_backends = vec![Backend::Cpu];
        descriptor.license = "MIT".into();
        descriptor.model_provenance = "none — luminance-grid reference descriptor".into();
        descriptor.deterministic = true;
        descriptor.batch_support = true;
        descriptor.cancellation_support = true;
        descriptor.progress_support = true;
        Self { descriptor }
    }
}

impl Default for ImageEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProvider for ImageEmbeddingProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        let (width, height, channels, data) = image_parts(input)?;
        let luma = image_to_luma_checked(width, height, channels, data, ctx)?;
        let mut values = vec![0.0_f32; 64];
        let mut counts = [0_u32; 64];
        for y in 0..height as usize {
            if y % 8 == 0 {
                ctx.check_cancelled()?;
            }
            let grid_y = (y * 8 / height as usize).min(7);
            for x in 0..width as usize {
                let grid_x = (x * 8 / width as usize).min(7);
                let bucket = grid_y * 8 + grid_x;
                values[bucket] += luma[y * width as usize + x] as f32 / 255.0;
                counts[bucket] += 1;
            }
        }
        for (value, count) in values.iter_mut().zip(counts) {
            if count > 0 {
                *value /= count as f32;
            }
        }
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > f32::EPSILON {
            for value in &mut values {
                *value /= norm;
            }
        }
        ctx.set_progress(1.0);
        Ok(ProviderOutput::Embedding { values })
    }
}

/// CPU reference audio analyser.
pub struct AudioAnalysisProvider {
    descriptor: ProviderDescriptor,
}

impl AudioAnalysisProvider {
    /// Creates the provider.
    pub fn new() -> Self {
        let mut descriptor = ProviderDescriptor::new(CapabilityId::AudioAnalysis);
        descriptor.name = "loom-reference-audio-analysis".into();
        descriptor.version = "0.1.0".into();
        descriptor.description = "Deterministic RMS, peak, and zero-crossing analysis for interleaved PCM.".into();
        descriptor.input_types = vec![InputType::Audio];
        descriptor.output_schema = r#"{"type":"object","properties":{"rms":{"type":"number"},"peak":{"type":"number"},"zero_crossing_rate":{"type":"number"}}}"#.into();
        descriptor.media_formats = ["pcm-f32"].map(String::from).to_vec();
        descriptor.required_memory_bytes = 4 * 1024 * 1024;
        descriptor.estimated_latency = Duration::from_millis(2);
        descriptor.hardware_backends = vec![Backend::Cpu];
        descriptor.license = "MIT".into();
        descriptor.model_provenance = "none — signal-analysis reference algorithm".into();
        descriptor.deterministic = true;
        descriptor.streaming_support = true;
        descriptor.cancellation_support = true;
        descriptor.progress_support = true;
        Self { descriptor }
    }
}

impl Default for AudioAnalysisProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProvider for AudioAnalysisProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError> {
        let ProviderInput::Audio {
            sample_rate,
            channels,
            samples,
        } = input
        else {
            return Err(VisionError::UnsupportedInput);
        };
        if *sample_rate == 0 || *channels == 0 || samples.len() % *channels as usize != 0 {
            return Err(VisionError::Internal("invalid interleaved audio buffer".into()));
        }
        if samples.is_empty() {
            return Ok(ProviderOutput::AudioAnalysis {
                rms: 0.0,
                peak: 0.0,
                zero_crossing_rate: 0.0,
            });
        }
        let mut sum_squares = 0.0_f64;
        let mut peak = 0.0_f32;
        let mut crossings = 0_u64;
        let mut previous = samples[0];
        for (index, sample) in samples.iter().copied().enumerate() {
            if index % 8192 == 0 {
                ctx.check_cancelled()?;
                ctx.set_progress(index as f32 / samples.len() as f32);
            }
            if !sample.is_finite() {
                return Err(VisionError::Internal("audio contains non-finite samples".into()));
            }
            sum_squares += f64::from(sample) * f64::from(sample);
            peak = peak.max(sample.abs());
            if index > 0 && (sample >= 0.0) != (previous >= 0.0) {
                crossings += 1;
            }
            previous = sample;
        }
        ctx.set_progress(1.0);
        Ok(ProviderOutput::AudioAnalysis {
            rms: (sum_squares / samples.len() as f64).sqrt() as f32,
            peak,
            zero_crossing_rate: crossings as f32 / samples.len().saturating_sub(1).max(1) as f32,
        })
    }
}

fn image_parts(input: &ProviderInput) -> Result<(u32, u32, u8, &[u8]), VisionError> {
    match input {
        ProviderInput::Image {
            width,
            height,
            channels,
            data,
            ..
        } => Ok((*width, *height, *channels, data)),
        _ => Err(VisionError::UnsupportedInput),
    }
}

fn otsu_threshold(luma: &[u8]) -> u8 {
    let mut histogram = [0_u64; 256];
    for value in luma {
        histogram[*value as usize] += 1;
    }
    let total = luma.len() as f64;
    let weighted_sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as f64 * *count as f64)
        .sum();
    let mut background_weight = 0.0_f64;
    let mut background_sum = 0.0_f64;
    let mut best_variance = -1.0_f64;
    let mut best_threshold = 127_u8;
    for (threshold, count) in histogram.iter().enumerate() {
        background_weight += *count as f64;
        if background_weight <= 0.0 {
            continue;
        }
        let foreground_weight = total - background_weight;
        if foreground_weight <= 0.0 {
            break;
        }
        background_sum += threshold as f64 * *count as f64;
        let background_mean = background_sum / background_weight;
        let foreground_mean = (weighted_sum - background_sum) / foreground_weight;
        let variance = background_weight
            * foreground_weight
            * (background_mean - foreground_mean)
            * (background_mean - foreground_mean);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = threshold as u8;
        }
    }
    best_threshold
}

fn neighbours(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let mut neighbours = [(0, 0); 4];
    let mut count = 0;
    if x > 0 {
        neighbours[count] = (x - 1, y);
        count += 1;
    }
    if x + 1 < width {
        neighbours[count] = (x + 1, y);
        count += 1;
    }
    if y > 0 {
        neighbours[count] = (x, y - 1);
        count += 1;
    }
    if y + 1 < height {
        neighbours[count] = (x, y + 1);
        count += 1;
    }
    neighbours.into_iter().take(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrcode::types::Color;
    use qrcode::QrCode;

    /// Renders a QR code into a raw image buffer with a quiet zone.
    ///
    /// `scale` is the pixel size of each module; `channels` is 1 (gray),
    /// 3 (rgb), or 4 (rgba). White background, black modules.
    fn make_qr_image(text: &str, scale: u32, channels: u8) -> (u32, u32, Vec<u8>) {
        let code = QrCode::new(text.as_bytes()).expect("encode QR");
        let modules = code.width() as i64;
        let quiet = 4i64;
        let size = ((modules + 2 * quiet) * i64::from(scale)) as u32;
        let mut data = vec![0u8; size as usize * size as usize * channels as usize];
        for y in 0..size {
            for x in 0..size {
                let mx = i64::from(x) / i64::from(scale) - quiet;
                let my = i64::from(y) / i64::from(scale) - quiet;
                let dark = mx >= 0
                    && my >= 0
                    && mx < modules
                    && my < modules
                    && code[(mx as usize, my as usize)] == Color::Dark;
                let value = if dark { 0 } else { 255 };
                let offset = (y as usize * size as usize + x as usize) * channels as usize;
                match channels {
                    4 => {
                        data[offset] = value;
                        data[offset + 1] = value;
                        data[offset + 2] = value;
                        data[offset + 3] = 255;
                    }
                    3 => {
                        data[offset] = value;
                        data[offset + 1] = value;
                        data[offset + 2] = value;
                    }
                    _ => data[offset] = value,
                }
            }
        }
        (size, size, data)
    }

    fn qr_input(text: &str, scale: u32, channels: u8) -> ProviderInput {
        let (width, height, data) = make_qr_image(text, scale, channels);
        let format = match channels {
            4 => "rgba",
            3 => "rgb",
            _ => "gray",
        };
        ProviderInput::Image {
            width,
            height,
            channels,
            data,
            format: format.to_string(),
        }
    }

    #[test]
    fn qr_decode_roundtrip_rgba() {
        let provider = QrCodeProvider::new();
        let input = qr_input("loom-vision-roundtrip", 4, 4);
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("decode");
        match output {
            ProviderOutput::QrDecoded { text } => {
                assert_eq!(text, "loom-vision-roundtrip");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn qr_decode_roundtrip_rgb() {
        let provider = QrCodeProvider::new();
        let input = qr_input("hello-rgb", 4, 3);
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("decode");
        assert!(matches!(
            output,
            ProviderOutput::QrDecoded { text } if text == "hello-rgb"
        ));
    }

    #[test]
    fn qr_decode_roundtrip_gray() {
        let provider = QrCodeProvider::new();
        let input = qr_input("hello-gray", 4, 1);
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("decode");
        assert!(matches!(
            output,
            ProviderOutput::QrDecoded { text } if text == "hello-gray"
        ));
    }

    #[test]
    fn qr_decode_no_code_returns_internal_error() {
        let provider = QrCodeProvider::new();
        let input = ProviderInput::Image {
            width: 64,
            height: 64,
            channels: 1,
            data: vec![255u8; 64 * 64],
            format: "gray".to_string(),
        };
        let mut ctx = RunContext::new();
        let result = provider.run(&input, &mut ctx);
        assert!(matches!(
            result,
            Err(VisionError::Internal(msg)) if msg == NO_QR_CODE_MESSAGE
        ));
    }

    #[test]
    fn qr_decode_cancel_before_run_returns_cancelled() {
        let provider = QrCodeProvider::new();
        let input = qr_input("cancelled", 4, 4);
        let mut ctx = RunContext::new();
        ctx.cancel();
        assert!(matches!(
            provider.run(&input, &mut ctx),
            Err(VisionError::Cancelled)
        ));
    }

    #[test]
    fn qr_decode_rejects_text_input() {
        let provider = QrCodeProvider::new();
        let input = ProviderInput::Text {
            text: "not an image".to_string(),
        };
        let mut ctx = RunContext::new();
        assert!(matches!(
            provider.run(&input, &mut ctx),
            Err(VisionError::UnsupportedInput)
        ));
    }

    #[test]
    fn qr_progress_reaches_one_on_success() {
        let provider = QrCodeProvider::new();
        let input = qr_input("progress", 4, 1);
        let mut ctx = RunContext::new();
        provider.run(&input, &mut ctx).expect("decode");
        assert_eq!(ctx.progress(), 1.0);
    }

    #[test]
    fn qr_descriptor_is_documented() {
        let provider = QrCodeProvider::new();
        let descriptor = provider.descriptor();
        assert_eq!(descriptor.capability_id, CapabilityId::QrDetection);
        assert!(descriptor.deterministic);
        assert!(descriptor.cancellation_support);
        assert_eq!(descriptor.input_types, vec![InputType::Image]);
        assert!(!descriptor.media_formats.is_empty());
    }

    #[test]
    fn stats_known_2x2_black_white() {
        // Two black and two white pixels: mean 127.5, std 127.5, contrast 1.0.
        let provider = ImageStatsProvider::new();
        let input = ProviderInput::Image {
            width: 2,
            height: 2,
            channels: 1,
            data: vec![0, 0, 255, 255],
            format: "gray".to_string(),
        };
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("stats");
        match output {
            ProviderOutput::ImageStats {
                mean_luma,
                std_luma,
                contrast,
            } => {
                assert!((mean_luma - 127.5).abs() < 1e-3, "mean was {mean_luma}");
                assert!((std_luma - 127.5).abs() < 1e-3, "std was {std_luma}");
                assert!((contrast - 1.0).abs() < 1e-6, "contrast was {contrast}");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn stats_uniform_image_has_zero_std_and_contrast() {
        let provider = ImageStatsProvider::new();
        let input = ProviderInput::Image {
            width: 2,
            height: 2,
            channels: 1,
            data: vec![42; 4],
            format: "gray".to_string(),
        };
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("stats");
        assert!(matches!(
            output,
            ProviderOutput::ImageStats { mean_luma, std_luma, contrast }
                if (mean_luma - 42.0).abs() < 1e-3
                    && std_luma == 0.0
                    && contrast == 0.0
        ));
    }

    #[test]
    fn stats_all_black_has_zero_contrast() {
        let provider = ImageStatsProvider::new();
        let input = ProviderInput::Image {
            width: 2,
            height: 2,
            channels: 1,
            data: vec![0; 4],
            format: "gray".to_string(),
        };
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("stats");
        assert!(matches!(
            output,
            ProviderOutput::ImageStats { mean_luma, contrast, .. }
                if mean_luma == 0.0 && contrast == 0.0
        ));
    }

    #[test]
    fn stats_on_rgb_uses_luma_weights() {
        let provider = ImageStatsProvider::new();
        // Two pure-red and two pure-white pixels.
        let input = ProviderInput::Image {
            width: 2,
            height: 2,
            channels: 3,
            data: vec![255, 0, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
            format: "rgb".to_string(),
        };
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).expect("stats");
        assert!(matches!(
            output,
            ProviderOutput::ImageStats { mean_luma, .. } if (mean_luma - 165.5).abs() < 1e-3
        ));
    }

    #[test]
    fn stats_cancel_before_run_returns_cancelled() {
        let provider = ImageStatsProvider::new();
        let input = ProviderInput::Image {
            width: 2,
            height: 2,
            channels: 1,
            data: vec![0, 0, 255, 255],
            format: "gray".to_string(),
        };
        let mut ctx = RunContext::new();
        ctx.cancel();
        assert!(matches!(
            provider.run(&input, &mut ctx),
            Err(VisionError::Cancelled)
        ));
    }

    #[test]
    fn stats_rejects_audio_input() {
        let provider = ImageStatsProvider::new();
        let input = ProviderInput::Audio {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![0.0; 128],
        };
        let mut ctx = RunContext::new();
        assert!(matches!(
            provider.run(&input, &mut ctx),
            Err(VisionError::UnsupportedInput)
        ));
    }

    #[test]
    fn stats_descriptor_is_documented() {
        let provider = ImageStatsProvider::new();
        let descriptor = provider.descriptor();
        assert_eq!(descriptor.capability_id, CapabilityId::ImageStats);
        assert!(descriptor.deterministic);
    }

    #[test]
    fn segmentation_produces_binary_mask() {
        let provider = ThresholdSegmentationProvider::new();
        let input = ProviderInput::Image {
            width: 4,
            height: 1,
            channels: 1,
            data: vec![0, 0, 255, 255],
            format: "gray".into(),
        };
        let mut ctx = RunContext::new();
        let output = provider.run(&input, &mut ctx).unwrap();
        assert!(matches!(
            output,
            ProviderOutput::SegmentationMask { width: 4, height: 1, mask }
                if mask == vec![255, 255, 0, 0]
        ));
        assert_eq!(ctx.progress(), 1.0);
    }

    #[test]
    fn document_layout_finds_components() {
        let provider = DocumentLayoutProvider::new();
        let mut data = vec![255_u8; 12 * 8];
        for y in 2..5 {
            for x in 1..10 {
                data[y * 12 + x] = 0;
            }
        }
        let input = ProviderInput::Image {
            width: 12,
            height: 8,
            channels: 1,
            data,
            format: "gray".into(),
        };
        let mut ctx = RunContext::new();
        match provider.run(&input, &mut ctx).unwrap() {
            ProviderOutput::DetectionResult { boxes } => {
                assert_eq!(boxes.len(), 1);
                assert_eq!(boxes[0].label, "content-region");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn embedding_is_fixed_length_and_normalized() {
        let provider = ImageEmbeddingProvider::new();
        let input = ProviderInput::Image {
            width: 8,
            height: 8,
            channels: 1,
            data: (0_u8..64).collect(),
            format: "gray".into(),
        };
        let mut ctx = RunContext::new();
        match provider.run(&input, &mut ctx).unwrap() {
            ProviderOutput::Embedding { values } => {
                assert_eq!(values.len(), 64);
                let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
                assert!((norm - 1.0).abs() < 0.001);
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn audio_analysis_reports_known_peak_and_rms() {
        let provider = AudioAnalysisProvider::new();
        let input = ProviderInput::Audio {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![-1.0, 1.0, -1.0, 1.0],
        };
        let mut ctx = RunContext::new();
        assert!(matches!(
            provider.run(&input, &mut ctx).unwrap(),
            ProviderOutput::AudioAnalysis { rms, peak, zero_crossing_rate }
                if (rms - 1.0).abs() < 0.001
                    && (peak - 1.0).abs() < 0.001
                    && (zero_crossing_rate - 1.0).abs() < 0.001
        ));
    }

}
