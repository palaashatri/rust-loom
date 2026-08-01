//! Reference CPU providers: real, deterministic algorithms with no model
//! files and no network access.
//!
//! These providers exist so Loom applications have working implementations
//! of core capabilities out of the box, and so the framework can be tested
//! end-to-end without any installed models.

use std::time::Duration;

use crate::error::VisionError;
use crate::provider::{
    image_to_luma_checked, Backend, CapabilityId, CapabilityProvider, InputType,
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
}
