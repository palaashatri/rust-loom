//! The Loom Vision provider model: capability identifiers, descriptors,
//! inputs, outputs, and the per-run context.

use std::cell::Cell;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::VisionError;

/// Stable identifier for a capability area that a provider can implement.
///
/// These identifiers are serialized as snake_case strings in model-pack
/// manifests and CLI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    /// Optical character recognition (printed text).
    Ocr,
    /// Document layout and structure analysis.
    DocumentAnalysis,
    /// Object detection.
    ObjectDetection,
    /// Semantic or instance segmentation.
    Segmentation,
    /// Image matting (foreground extraction).
    Matting,
    /// Human pose estimation.
    Pose,
    /// Image or video embedding generation.
    Embedding,
    /// Object tracking across video frames.
    Tracking,
    /// Optical flow estimation.
    OpticalFlow,
    /// Speech recognition.
    SpeechRecognition,
    /// Audio analysis (tempo, key, beat, transients).
    AudioAnalysis,
    /// Image generation.
    ImageGeneration,
    /// Image inpainting.
    Inpainting,
    /// Super-resolution.
    SuperResolution,
    /// Barcode recognition.
    Barcode,
    /// QR-code detection and decoding.
    QrDetection,
    /// Image statistics (luma mean, standard deviation, contrast).
    ImageStats,
}

impl CapabilityId {
    /// Returns the stable snake_case string form of this capability.
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityId::Ocr => "ocr",
            CapabilityId::DocumentAnalysis => "document_analysis",
            CapabilityId::ObjectDetection => "object_detection",
            CapabilityId::Segmentation => "segmentation",
            CapabilityId::Matting => "matting",
            CapabilityId::Pose => "pose",
            CapabilityId::Embedding => "embedding",
            CapabilityId::Tracking => "tracking",
            CapabilityId::OpticalFlow => "optical_flow",
            CapabilityId::SpeechRecognition => "speech_recognition",
            CapabilityId::AudioAnalysis => "audio_analysis",
            CapabilityId::ImageGeneration => "image_generation",
            CapabilityId::Inpainting => "inpainting",
            CapabilityId::SuperResolution => "super_resolution",
            CapabilityId::Barcode => "barcode",
            CapabilityId::QrDetection => "qr_detection",
            CapabilityId::ImageStats => "image_stats",
        }
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The media types a provider can accept as input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    /// A single image frame.
    Image,
    /// A video stream or sequence of frames.
    Video,
    /// An audio stream.
    Audio,
    /// Text.
    Text,
}

impl InputType {
    /// Returns the stable snake_case string form of this input type.
    pub fn as_str(&self) -> &'static str {
        match self {
            InputType::Image => "image",
            InputType::Video => "video",
            InputType::Audio => "audio",
            InputType::Text => "text",
        }
    }
}

impl fmt::Display for InputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Hardware or software backends a provider may run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Pure CPU execution.
    Cpu,
    /// Vulkan compute.
    Vulkan,
    /// ONNX Runtime.
    Onnx,
    /// The Candle engine.
    Candle,
    /// NVIDIA CUDA.
    Cuda,
    /// NVIDIA TensorRT.
    TensorRt,
    /// AMD ROCm.
    Rocm,
    /// Intel OpenVINO.
    OpenVino,
    /// Microsoft DirectML (future Windows builds).
    DirectML,
    /// Apple Core ML (future macOS builds).
    CoreML,
}

impl Backend {
    /// Returns the stable snake_case string form of this backend.
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Cpu => "cpu",
            Backend::Vulkan => "vulkan",
            Backend::Onnx => "onnx",
            Backend::Candle => "candle",
            Backend::Cuda => "cuda",
            Backend::TensorRt => "tensorrt",
            Backend::Rocm => "rocm",
            Backend::OpenVino => "openvino",
            Backend::DirectML => "directml",
            Backend::CoreML => "coreml",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Static description of a capability provider.
///
/// A descriptor is immutable for the lifetime of the provider and must be
/// returned verbatim by [`CapabilityProvider::descriptor`].
#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    /// The capability this provider implements.
    pub capability_id: CapabilityId,
    /// Machine-readable provider name (e.g. `"rqrr-reference-qr"`).
    pub name: String,
    /// Semantic version of the provider implementation.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Accepted input types.
    pub input_types: Vec<InputType>,
    /// JSON-Schema-style description of the output the provider produces.
    pub output_schema: String,
    /// Media formats the provider can consume.
    pub media_formats: Vec<String>,
    /// ISO 639 language codes supported (empty when not applicable).
    pub languages: Vec<String>,
    /// Peak memory requirement in bytes.
    pub required_memory_bytes: u64,
    /// Estimated latency for a typical input.
    pub estimated_latency: Duration,
    /// Backends this provider can run on.
    pub hardware_backends: Vec<Backend>,
    /// SPDX license identifier of the provider implementation.
    pub license: String,
    /// Description of the model provenance (or `"none"` for algorithmic providers).
    pub model_provenance: String,
    /// Whether repeated runs on identical input produce identical output.
    pub deterministic: bool,
    /// Whether the provider can process multiple inputs per run.
    pub batch_support: bool,
    /// Whether the provider can consume streaming input.
    pub streaming_support: bool,
    /// Whether the provider respects cancellation via [`RunContext::cancel`].
    pub cancellation_support: bool,
    /// Whether the provider reports progress via [`RunContext::set_progress`].
    pub progress_support: bool,
}

impl ProviderDescriptor {
    /// Creates a descriptor with conservative defaults for a capability.
    ///
    /// Providers that want more precise metadata should mutate the returned
    /// struct. Defaults: name is the capability id, version `0.1.0`, CPU
    /// backend, deterministic, no batch/streaming, cancellation and progress
    /// supported, no model (provenance `"none"`), MIT license.
    pub fn new(capability_id: CapabilityId) -> Self {
        ProviderDescriptor {
            capability_id,
            name: capability_id.as_str().to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            input_types: vec![InputType::Image],
            output_schema: String::new(),
            media_formats: Vec::new(),
            languages: Vec::new(),
            required_memory_bytes: 0,
            estimated_latency: Duration::from_millis(1),
            hardware_backends: vec![Backend::Cpu],
            license: String::new(),
            model_provenance: "none".to_string(),
            deterministic: true,
            batch_support: false,
            streaming_support: false,
            cancellation_support: true,
            progress_support: true,
        }
    }
}

/// A decoded grayscale (luma-only) image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LumaImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Row-major pixel data, one byte per pixel.
    pub data: Vec<u8>,
}

/// Input payload handed to a provider.
///
/// Image data is a raw, row-major byte buffer; the `format` field names the
/// channel layout (`"rgba"`, `"rgb"`, or `"gray"`) and must be consistent
/// with `channels`.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderInput {
    /// A raw image frame.
    Image {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Channels per pixel: 1 (gray), 3 (rgb), or 4 (rgba).
        channels: u8,
        /// Row-major pixel data; length must equal `width * height * channels`.
        data: Vec<u8>,
        /// Channel layout name: `"gray"`, `"rgb"`, or `"rgba"`.
        format: String,
    },
    /// Raw interleaved audio samples.
    Audio {
        /// Sample rate in Hz.
        sample_rate: u32,
        /// Number of interleaved channels.
        channels: u16,
        /// Interleaved f32 samples in `[-1.0, 1.0]`.
        samples: Vec<f32>,
    },
    /// Text input.
    Text {
        /// The text payload.
        text: String,
    },
}

impl ProviderInput {
    /// Converts image inputs to grayscale.
    ///
    /// Returns [`VisionError::UnsupportedInput`] for non-image inputs.
    /// RGBA and RGB pixels are converted with ITU-R BT.601 luma weights.
    pub fn to_luma(&self) -> Result<LumaImage, VisionError> {
        match self {
            ProviderInput::Image {
                width,
                height,
                channels,
                data,
                ..
            } => {
                let data = image_to_luma(*width, *height, *channels, data)?;
                Ok(LumaImage {
                    width: *width,
                    height: *height,
                    data,
                })
            }
            _ => Err(VisionError::UnsupportedInput),
        }
    }
}

/// Converts an RGBA, RGB, or grayscale buffer into a grayscale buffer.
///
/// `channels` must be 1, 3, or 4; any other value yields
/// [`VisionError::UnsupportedInput`]. The buffer length must equal
/// `width * height * channels`; a mismatch yields
/// [`VisionError::Internal`]. Zero-sized images are rejected.
pub fn image_to_luma(
    width: u32,
    height: u32,
    channels: u8,
    data: &[u8],
) -> Result<Vec<u8>, VisionError> {
    validate_image(width, height, channels, data)?;
    let mut luma = Vec::with_capacity(width as usize * height as usize);
    let row_bytes = width as usize * channels as usize;
    for row in data.chunks_exact(row_bytes) {
        luma_row(channels, row, &mut luma);
    }
    Ok(luma)
}

/// Row-by-row grayscale conversion that checks cancellation every 8 rows.
///
/// Same conversion as [`image_to_luma`], but suitable for long-running
/// providers that must honour [`RunContext`] cancellation during the
/// conversion pass. Returns [`VisionError::Cancelled`] once the context is
/// cancelled.
pub fn image_to_luma_checked(
    width: u32,
    height: u32,
    channels: u8,
    data: &[u8],
    ctx: &mut RunContext,
) -> Result<Vec<u8>, VisionError> {
    validate_image(width, height, channels, data)?;
    let mut luma = Vec::with_capacity(width as usize * height as usize);
    let row_bytes = width as usize * channels as usize;
    for (row_index, row) in data.chunks_exact(row_bytes).enumerate() {
        if row_index % 8 == 0 {
            ctx.check_cancelled()?;
        }
        luma_row(channels, row, &mut luma);
    }
    Ok(luma)
}

fn validate_image(width: u32, height: u32, channels: u8, data: &[u8]) -> Result<(), VisionError> {
    if !matches!(channels, 1 | 3 | 4) {
        return Err(VisionError::UnsupportedInput);
    }
    if width == 0 || height == 0 {
        return Err(VisionError::Internal(
            "image has a zero dimension".to_string(),
        ));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels as usize))
        .ok_or_else(|| VisionError::Internal("image size overflow".to_string()))?;
    if data.len() != expected {
        return Err(VisionError::Internal(format!(
            "image buffer length {} does not match {width}x{height}x{channels}",
            data.len()
        )));
    }
    Ok(())
}

fn luma_row(channels: u8, row: &[u8], out: &mut Vec<u8>) {
    match channels {
        1 => out.extend_from_slice(row),
        3 | 4 => {
            for px in row.chunks_exact(channels as usize) {
                out.push(rgb_to_luma(px[0], px[1], px[2]));
            }
        }
        _ => unreachable!("channel count validated before conversion"),
    }
}

fn rgb_to_luma(r: u8, g: u8, b: u8) -> u8 {
    ((299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000) as u8
}

/// Output payload produced by a provider.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderOutput {
    /// OCR text with a confidence score in `[0.0, 1.0]`.
    OcrResult {
        /// Recognized text.
        text: String,
        /// Confidence score.
        confidence: f32,
    },
    /// A set of detected bounding boxes.
    DetectionResult {
        /// Detected boxes.
        boxes: Vec<BBox>,
    },
    /// Decoded QR-code payload.
    QrDecoded {
        /// The decoded text.
        text: String,
    },
    /// Image statistics.
    ImageStats {
        /// Mean luma in `[0.0, 255.0]`.
        mean_luma: f32,
        /// Population standard deviation of luma.
        std_luma: f32,
        /// Michelson contrast in `[0.0, 1.0]`.
        contrast: f32,
    },
    /// A generic string result.
    Generic {
        /// The message.
        message: String,
    },
}

/// An axis-aligned bounding box in image pixel coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct BBox {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// Human-readable label.
    pub label: String,
    /// Confidence in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// Per-run mutable context: cancellation flag and progress reporting.
///
/// The caller owns the context for the duration of
/// [`CapabilityProvider::run`]. It is not `Sync` by design; [`RunContext::cancel`]
/// exists so the same thread that owns the run can cancel it (for example via
/// an interruption hook or a watched flag), mirroring the cancellation-token
/// pattern. Providers should call [`RunContext::check_cancelled`] periodically
/// and return [`VisionError::Cancelled`] as soon as possible.
pub struct RunContext {
    cancelled: AtomicBool,
    progress: Cell<f32>,
}

impl RunContext {
    /// Creates a fresh, uncancelled context at 0% progress.
    pub fn new() -> Self {
        RunContext {
            cancelled: AtomicBool::new(false),
            progress: Cell::new(0.0),
        }
    }

    /// Requests cancellation of the current run.
    ///
    /// The next [`RunContext::check_cancelled`] call made by the provider
    /// will return [`VisionError::Cancelled`].
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Returns [`VisionError::Cancelled`] if cancellation was requested.
    pub fn check_cancelled(&self) -> Result<(), VisionError> {
        if self.is_cancelled() {
            Err(VisionError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Sets progress, clamped to `[0.0, 1.0]`.
    pub fn set_progress(&self, progress: f32) {
        self.progress.set(progress.clamp(0.0, 1.0));
    }

    /// Returns the current progress in `[0.0, 1.0]`.
    pub fn progress(&self) -> f32 {
        self.progress.get()
    }
}

impl Default for RunContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A capability provider: something that can process inputs for one
/// [`CapabilityId`].
///
/// Implementations must be `Send + Sync` and should be deterministic where
/// the descriptor claims so. `run` must never block on I/O or the network,
/// must honour [`RunContext::check_cancelled`] where `cancellation_support`
/// is true, and must return [`VisionError::UnsupportedInput`] for input types
/// it does not declare in its descriptor.
pub trait CapabilityProvider: Send + Sync {
    /// Returns the static descriptor of this provider.
    fn descriptor(&self) -> &ProviderDescriptor;

    /// Runs this provider on `input`, reporting progress and cancellation
    /// through `ctx`.
    fn run(
        &self,
        input: &ProviderInput,
        ctx: &mut RunContext,
    ) -> Result<ProviderOutput, VisionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_image(pixels: &[u8]) -> ProviderInput {
        ProviderInput::Image {
            width: 2,
            height: 2,
            channels: 3,
            data: pixels.to_vec(),
            format: "rgb".to_string(),
        }
    }

    #[test]
    fn luma_conversion_rgb_matches_bt601() {
        // Pure red, green, blue, and white.
        let input = rgb_image(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        let luma = input.to_luma().expect("conversion");
        assert_eq!(luma.width, 2);
        assert_eq!(luma.height, 2);
        assert_eq!(luma.data, vec![76, 149, 29, 255]);
    }

    #[test]
    fn luma_conversion_gray_is_identity() {
        let input = ProviderInput::Image {
            width: 3,
            height: 1,
            channels: 1,
            data: vec![0, 128, 255],
            format: "gray".to_string(),
        };
        assert_eq!(input.to_luma().unwrap().data, vec![0, 128, 255]);
    }

    #[test]
    fn luma_conversion_rgba_ignores_alpha() {
        let input = ProviderInput::Image {
            width: 1,
            height: 1,
            channels: 4,
            data: vec![0, 0, 0, 0],
            format: "rgba".to_string(),
        };
        assert_eq!(input.to_luma().unwrap().data, vec![0]);
    }

    #[test]
    fn luma_rejects_bad_channel_count() {
        let input = ProviderInput::Image {
            width: 1,
            height: 1,
            channels: 2,
            data: vec![0, 0],
            format: "??".to_string(),
        };
        assert!(matches!(
            input.to_luma(),
            Err(VisionError::UnsupportedInput)
        ));
    }

    #[test]
    fn luma_rejects_wrong_buffer_length() {
        let input = rgb_image(&[0, 0, 0]);
        assert!(matches!(input.to_luma(), Err(VisionError::Internal(_))));
    }

    #[test]
    fn luma_rejects_zero_dimension() {
        let input = ProviderInput::Image {
            width: 0,
            height: 10,
            channels: 1,
            data: vec![],
            format: "gray".to_string(),
        };
        assert!(matches!(input.to_luma(), Err(VisionError::Internal(_))));
    }

    #[test]
    fn to_luma_rejects_text_input() {
        let input = ProviderInput::Text {
            text: "hello".to_string(),
        };
        assert!(matches!(
            input.to_luma(),
            Err(VisionError::UnsupportedInput)
        ));
    }

    #[test]
    fn checked_luma_honours_cancellation() {
        let mut ctx = RunContext::new();
        ctx.cancel();
        let data = vec![0u8; 64 * 64];
        let result = image_to_luma_checked(64, 64, 1, &data, &mut ctx);
        assert!(matches!(result, Err(VisionError::Cancelled)));
    }

    #[test]
    fn run_context_progress_clamps_to_unit_interval() {
        let ctx = RunContext::new();
        assert_eq!(ctx.progress(), 0.0);
        ctx.set_progress(1.5);
        assert_eq!(ctx.progress(), 1.0);
        ctx.set_progress(-0.5);
        assert_eq!(ctx.progress(), 0.0);
        ctx.set_progress(0.4);
        assert_eq!(ctx.progress(), 0.4);
    }

    #[test]
    fn run_context_cancel_roundtrip() {
        let ctx = RunContext::new();
        assert!(!ctx.is_cancelled());
        assert!(ctx.check_cancelled().is_ok());
        ctx.cancel();
        assert!(ctx.is_cancelled());
        assert!(matches!(ctx.check_cancelled(), Err(VisionError::Cancelled)));
    }

    #[test]
    fn capability_id_string_forms_are_stable() {
        assert_eq!(CapabilityId::QrDetection.as_str(), "qr_detection");
        assert_eq!(CapabilityId::DocumentAnalysis.as_str(), "document_analysis");
        assert_eq!(CapabilityId::ImageStats.as_str(), "image_stats");
        assert_eq!(CapabilityId::Ocr.to_string(), "ocr");
    }

    #[test]
    fn descriptor_defaults_are_sane() {
        let d = ProviderDescriptor::new(CapabilityId::Ocr);
        assert_eq!(d.capability_id, CapabilityId::Ocr);
        assert_eq!(d.name, "ocr");
        assert_eq!(d.hardware_backends, vec![Backend::Cpu]);
        assert!(d.deterministic);
        assert!(d.cancellation_support);
        assert_eq!(d.model_provenance, "none");
    }
}
