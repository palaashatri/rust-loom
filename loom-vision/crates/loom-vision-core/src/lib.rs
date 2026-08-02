//! # loom-vision-core
//!
//! Local-first computer-vision framework for the Loom creative suite.
//!
//! Loom Vision is a *provider framework*: applications ask for a capability
//! (for example [`CapabilityId::QrDetection`]) and receive outputs from
//! whatever provider is registered for it — a CPU reference implementation,
//! or later a model backed by ONNX Runtime, Candle, or another backend.
//! Everything is local: there is no network access, no telemetry, and no
//! remote inference anywhere in this crate.
//!
//! ## Modules
//!
//! * [`provider`] — capability ids, descriptors, inputs, outputs, and the
//!   run context (cancellation and progress).
//! * [`registry`] — ordered [`ProviderRegistry`] and per-capability
//!   [`CapabilityRegistry`] routing.
//! * [`model_pack`] — model-pack manifest parsing, validation (checksums,
//!   path-traversal and archive-bomb guards), and safe installation.
//! * [`reference`] — deterministic CPU reference providers for QR decoding,
//!   image statistics, threshold segmentation, document layout, compact image
//!   embeddings, and audio analysis.
//! * [`error`] — the shared [`VisionError`] type.
//!
//! ## Example
//!
//! ```
//! use loom_vision_core::provider::{CapabilityId, CapabilityProvider, ProviderInput, RunContext};
//! use loom_vision_core::reference::ImageStatsProvider;
//!
//! let provider = ImageStatsProvider::new();
//! let input = ProviderInput::Image {
//!     width: 2,
//!     height: 2,
//!     channels: 1,
//!     data: vec![0, 0, 255, 255],
//!     format: "gray".to_string(),
//! };
//! let mut ctx = RunContext::new();
//! let output = provider.run(&input, &mut ctx).expect("stats");
//! assert_eq!(provider.descriptor().capability_id, CapabilityId::ImageStats);
//! assert!(matches!(output, loom_vision_core::provider::ProviderOutput::ImageStats { .. }));
//! ```
//!
//! # Safety
//!
//! This crate contains no `unsafe` code (`#![forbid(unsafe_code)]`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod model_pack;
pub mod provider;
pub mod reference;
pub mod registry;

pub use error::VisionError;
pub use model_pack::{
    install_pack, install_pack_force, validate_pack, validate_pack_with_limit, ModelFile,
    ModelPackManifest, ModelPackSummary, TestVector, DEFAULT_MAX_PACK_SIZE_BYTES, MANIFEST_FILE,
};
pub use provider::{
    image_to_luma, image_to_luma_checked, BBox, Backend, CapabilityId, CapabilityProvider,
    InputType, LumaImage, ProviderDescriptor, ProviderInput, ProviderOutput, RunContext,
};
pub use reference::{
    AudioAnalysisProvider, DocumentLayoutProvider, ImageEmbeddingProvider, ImageStatsProvider,
    QrCodeProvider, ThresholdSegmentationProvider, NO_QR_CODE_MESSAGE,
};
pub use registry::{CapabilityRegistry, ProviderRegistry};

/// Current model-pack manifest format version.
///
/// Manifests declaring a different `format_version` are rejected by
/// [`model_pack::parse_manifest`].
pub const FORMAT_VERSION: u32 = 1;
