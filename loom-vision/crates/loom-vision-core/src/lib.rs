//! # loom-vision-core
//!
//! Local-first computer-vision framework for the Loom creative suite.
//!
//! Loom Vision is a *provider framework*: applications ask for a capability
//! (for example [`CapabilityId::QrDetection`]) and receive outputs from
//! whatever provider is registered for it — a CPU reference implementation,
//! or a model backed by a local acceleration runtime. Everything is local:
//! there is no network access, telemetry, or remote inference in this crate.
//!
//! ## Modules
//!
//! * [`provider`] — capability ids, descriptors, inputs, outputs, and run context.
//! * [`registry`] — ordered provider and per-capability routing.
//! * [`model_pack`] — model-pack parsing, integrity validation, and safe installation.
//! * [`production`] — preprocessing, acceleration selection, evaluation, benchmarks,
//!   redistribution, and production-readiness validation.
//! * [`reference`] — deterministic CPU reference providers.
//! * [`error`] — the shared [`VisionError`] type.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod model_pack;
pub mod production;
pub mod provider;
pub mod reference;
pub mod registry;

pub use error::VisionError;
pub use model_pack::{
    install_pack, install_pack_force, validate_pack, validate_pack_with_limit, ModelFile,
    ModelPackManifest, ModelPackSummary, TestVector, DEFAULT_MAX_PACK_SIZE_BYTES, MANIFEST_FILE,
};
pub use production::{
    AccelerationBackend, BackendAvailability, BackendBenchmark, ChannelOrder, DatasetDescriptor,
    ElementType, EvaluationMetric, PreprocessingSpec, ProductionModelRelease, ResizeMode,
    RuntimeInventory, TensorLayout,
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
