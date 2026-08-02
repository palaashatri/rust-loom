//! Production model execution, preprocessing, distribution, and evaluation
//! contracts for Loom Vision.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

/// Tensor element type expected by a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementType {
    /// Unsigned 8-bit integer.
    U8,
    /// IEEE 754 half precision.
    F16,
    /// IEEE 754 single precision.
    F32,
    /// Signed 64-bit integer.
    I64,
}

/// Image channel order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelOrder {
    /// Red, green, blue.
    Rgb,
    /// Blue, green, red.
    Bgr,
    /// Single luminance channel.
    Gray,
}

/// Tensor memory layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TensorLayout {
    /// Batch, channels, height, width.
    Nchw,
    /// Batch, height, width, channels.
    Nhwc,
}

/// Resize behavior before inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResizeMode {
    /// Stretch to the model dimensions.
    Stretch,
    /// Preserve aspect ratio and letterbox.
    Letterbox,
    /// Preserve aspect ratio and crop the center.
    CenterCrop,
    /// Preserve aspect ratio and crop around a detected region.
    RegionCrop,
}

/// Documented preprocessing contract distributed with a model pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreprocessingSpec {
    /// Input width in pixels.
    pub width: u32,
    /// Input height in pixels.
    pub height: u32,
    /// Tensor element type.
    pub element_type: ElementType,
    /// Channel order.
    pub channel_order: ChannelOrder,
    /// Tensor layout.
    pub layout: TensorLayout,
    /// Resize behavior.
    pub resize: ResizeMode,
    /// Per-channel value subtracted from input values.
    pub mean: Vec<f32>,
    /// Per-channel divisor applied after mean subtraction.
    pub standard_deviation: Vec<f32>,
    /// Whether EXIF orientation must be applied before resizing.
    pub apply_orientation: bool,
    /// Letterbox fill value for each channel.
    pub letterbox_value: Vec<f32>,
}

impl PreprocessingSpec {
    /// Validate dimensions and normalization vector cardinality.
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("preprocessing dimensions must be non-zero".into());
        }
        let channels = match self.channel_order {
            ChannelOrder::Gray => 1,
            ChannelOrder::Rgb | ChannelOrder::Bgr => 3,
        };
        for (name, values) in [
            ("mean", &self.mean),
            ("standard_deviation", &self.standard_deviation),
            ("letterbox_value", &self.letterbox_value),
        ] {
            if values.len() != channels {
                return Err(format!(
                    "{name} must contain {channels} values, got {}",
                    values.len()
                ));
            }
            if values.iter().any(|value| !value.is_finite()) {
                return Err(format!("{name} contains a non-finite value"));
            }
        }
        if self
            .standard_deviation
            .iter()
            .any(|value| value.abs() < f32::EPSILON)
        {
            return Err("standard_deviation must not contain zero".into());
        }
        Ok(())
    }
}

/// Local acceleration backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AccelerationBackend {
    /// Portable CPU execution.
    Cpu,
    /// ONNX Runtime CPU execution.
    OnnxCpu,
    /// NVIDIA CUDA.
    Cuda,
    /// NVIDIA TensorRT.
    TensorRt,
    /// AMD ROCm.
    Rocm,
    /// Microsoft DirectML.
    DirectMl,
    /// Apple Core ML.
    CoreMl,
    /// Apple Metal Performance Shaders.
    Metal,
    /// Vulkan compute.
    Vulkan,
}

/// One available local backend and its runtime details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendAvailability {
    /// Backend kind.
    pub backend: AccelerationBackend,
    /// Whether the runtime is currently usable.
    pub available: bool,
    /// Runtime or driver version, when known.
    pub version: Option<String>,
    /// Diagnostic explaining availability.
    pub diagnostic: String,
}

/// Snapshot of locally available execution backends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInventory {
    /// Detected backends.
    pub backends: Vec<BackendAvailability>,
}

impl RuntimeInventory {
    /// Detect conservative backend availability from platform and explicit
    /// runtime environment variables. This performs no network access.
    pub fn detect() -> Self {
        let mut backends = vec![BackendAvailability {
            backend: AccelerationBackend::Cpu,
            available: true,
            version: None,
            diagnostic: "portable CPU backend".into(),
        }];
        let explicit = [
            ("LOOM_ONNX_RUNTIME", AccelerationBackend::OnnxCpu),
            ("CUDA_PATH", AccelerationBackend::Cuda),
            ("TENSORRT_ROOT", AccelerationBackend::TensorRt),
            ("ROCM_PATH", AccelerationBackend::Rocm),
            ("VULKAN_SDK", AccelerationBackend::Vulkan),
        ];
        for (variable, backend) in explicit {
            let value = std::env::var_os(variable);
            backends.push(BackendAvailability {
                backend,
                available: value.is_some(),
                version: None,
                diagnostic: value.map_or_else(
                    || format!("{variable} is not configured"),
                    |path| format!("{variable}={}", PathBuf::from(path).display()),
                ),
            });
        }
        backends.push(BackendAvailability {
            backend: AccelerationBackend::DirectMl,
            available: cfg!(windows),
            version: None,
            diagnostic: if cfg!(windows) {
                "Windows platform supports DirectML provider discovery"
            } else {
                "DirectML is only available on Windows"
            }
            .into(),
        });
        for backend in [AccelerationBackend::CoreMl, AccelerationBackend::Metal] {
            backends.push(BackendAvailability {
                backend,
                available: cfg!(target_os = "macos"),
                version: None,
                diagnostic: if cfg!(target_os = "macos") {
                    "macOS platform backend"
                } else {
                    "backend is only available on macOS"
                }
                .into(),
            });
        }
        Self { backends }
    }

    /// Return whether a backend is available.
    pub fn contains(&self, backend: AccelerationBackend) -> bool {
        self.backends
            .iter()
            .any(|item| item.backend == backend && item.available)
    }

    /// Select the first available backend in preference order, falling back to
    /// CPU when permitted.
    pub fn select(
        &self,
        preferences: &[AccelerationBackend],
        allow_cpu_fallback: bool,
    ) -> Result<AccelerationBackend, String> {
        for backend in preferences {
            if self.contains(*backend) {
                return Ok(*backend);
            }
        }
        if allow_cpu_fallback && self.contains(AccelerationBackend::Cpu) {
            return Ok(AccelerationBackend::Cpu);
        }
        Err("no compatible acceleration backend is available".into())
    }
}

/// Dataset split used by an evaluation metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetDescriptor {
    /// Stable dataset id.
    pub id: String,
    /// Dataset version or immutable revision.
    pub version: String,
    /// SPDX expression or documented use terms.
    pub license: String,
    /// Number of examples evaluated.
    pub examples: u64,
    /// SHA-256 of the evaluation manifest.
    pub manifest_sha256: String,
    /// Demographic/device strata represented by the evaluation.
    pub strata: Vec<String>,
}

/// One quality, fairness, or robustness measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationMetric {
    /// Metric name, such as `f1`, `wer`, `far`, or `iou`.
    pub name: String,
    /// Metric value.
    pub value: f64,
    /// Optional lower confidence bound.
    pub lower_bound: Option<f64>,
    /// Optional upper confidence bound.
    pub upper_bound: Option<f64>,
    /// Dataset stratum, or `overall`.
    pub stratum: String,
}

/// Latency and memory benchmark for one backend/device combination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendBenchmark {
    /// Backend used.
    pub backend: AccelerationBackend,
    /// Device description.
    pub device: String,
    /// Runtime version.
    pub runtime_version: String,
    /// Warmup iterations excluded from measurements.
    pub warmup_iterations: u32,
    /// Measured iterations.
    pub measured_iterations: u32,
    /// Median latency.
    pub p50_latency_ms: f64,
    /// 95th percentile latency.
    pub p95_latency_ms: f64,
    /// Peak resident memory in bytes.
    pub peak_memory_bytes: u64,
    /// Throughput in inputs per second.
    pub throughput_per_second: f64,
}

impl BackendBenchmark {
    /// Construct a benchmark from measured durations.
    pub fn from_durations(
        backend: AccelerationBackend,
        device: impl Into<String>,
        runtime_version: impl Into<String>,
        warmup_iterations: u32,
        durations: &[Duration],
        peak_memory_bytes: u64,
    ) -> Result<Self, String> {
        if durations.is_empty() {
            return Err("at least one measured duration is required".into());
        }
        let mut milliseconds: Vec<f64> = durations
            .iter()
            .map(|duration| duration.as_secs_f64() * 1_000.0)
            .collect();
        milliseconds.sort_by(f64::total_cmp);
        let p50 = percentile(&milliseconds, 50);
        let p95 = percentile(&milliseconds, 95);
        let total_seconds: f64 = durations.iter().map(Duration::as_secs_f64).sum();
        let throughput = if total_seconds <= f64::EPSILON {
            0.0
        } else {
            durations.len() as f64 / total_seconds
        };
        Ok(Self {
            backend,
            device: device.into(),
            runtime_version: runtime_version.into(),
            warmup_iterations,
            measured_iterations: durations.len().try_into().unwrap_or(u32::MAX),
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            peak_memory_bytes,
            throughput_per_second: throughput,
        })
    }
}

fn percentile(values: &[f64], percentile: usize) -> f64 {
    let index = ((values.len() - 1) * percentile).div_ceil(100);
    values[index.min(values.len() - 1)]
}

/// Distribution and safety metadata required for a production model release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionModelRelease {
    /// Stable release id.
    pub release_id: String,
    /// SPDX license for weights and bundled assets.
    pub weights_license: String,
    /// Source/provenance statement.
    pub provenance: String,
    /// Whether redistribution is explicitly permitted.
    pub redistributable: bool,
    /// Whether the capability requires explicit informed consent.
    pub requires_consent: bool,
    /// Documented preprocessing.
    pub preprocessing: PreprocessingSpec,
    /// Supported acceleration preference order.
    pub preferred_backends: Vec<AccelerationBackend>,
    /// Evaluation dataset.
    pub dataset: DatasetDescriptor,
    /// Quality/fairness metrics.
    pub metrics: Vec<EvaluationMetric>,
    /// Backend benchmarks.
    pub benchmarks: Vec<BackendBenchmark>,
}

impl ProductionModelRelease {
    /// Validate release readiness. An empty result means every mandatory
    /// production requirement is documented and internally consistent.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.release_id.trim().is_empty() {
            errors.push("release_id is required".into());
        }
        if self.weights_license.trim().is_empty() {
            errors.push("weights_license is required".into());
        }
        if self.provenance.trim().is_empty() {
            errors.push("provenance is required".into());
        }
        if !self.redistributable {
            errors.push("model weights are not redistributable".into());
        }
        if let Err(error) = self.preprocessing.validate() {
            errors.push(error);
        }
        if self.dataset.examples == 0 {
            errors.push("evaluation dataset contains no examples".into());
        }
        if self.dataset.manifest_sha256.len() != 64 {
            errors.push("dataset manifest_sha256 must contain 64 hex characters".into());
        }
        if self.metrics.is_empty() {
            errors.push("at least one evaluation metric is required".into());
        }
        if self.benchmarks.is_empty() {
            errors.push("at least one backend benchmark is required".into());
        }
        let benchmarked: BTreeSet<AccelerationBackend> = self
            .benchmarks
            .iter()
            .map(|benchmark| benchmark.backend)
            .collect();
        for backend in &self.preferred_backends {
            if !benchmarked.contains(backend) {
                errors.push(format!("preferred backend {backend:?} has no benchmark"));
            }
        }
        for metric in &self.metrics {
            if !metric.value.is_finite()
                || metric.lower_bound.is_some_and(|value| !value.is_finite())
                || metric.upper_bound.is_some_and(|value| !value.is_finite())
            {
                errors.push(format!("metric {} contains a non-finite value", metric.name));
            }
            if let (Some(lower), Some(upper)) = (metric.lower_bound, metric.upper_bound) {
                if lower > metric.value || metric.value > upper {
                    errors.push(format!(
                        "metric {} lies outside its confidence interval",
                        metric.name
                    ));
                }
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preprocessing() -> PreprocessingSpec {
        PreprocessingSpec {
            width: 224,
            height: 224,
            element_type: ElementType::F32,
            channel_order: ChannelOrder::Rgb,
            layout: TensorLayout::Nchw,
            resize: ResizeMode::Letterbox,
            mean: vec![0.485, 0.456, 0.406],
            standard_deviation: vec![0.229, 0.224, 0.225],
            apply_orientation: true,
            letterbox_value: vec![0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn preprocessing_rejects_incomplete_normalization() {
        let mut spec = preprocessing();
        spec.mean.pop();
        assert!(spec.validate().is_err());
    }

    #[test]
    fn runtime_selection_is_deterministic() {
        let inventory = RuntimeInventory {
            backends: vec![
                BackendAvailability {
                    backend: AccelerationBackend::Cpu,
                    available: true,
                    version: None,
                    diagnostic: "cpu".into(),
                },
                BackendAvailability {
                    backend: AccelerationBackend::Cuda,
                    available: true,
                    version: Some("13".into()),
                    diagnostic: "cuda".into(),
                },
            ],
        };
        assert_eq!(
            inventory
                .select(
                    &[AccelerationBackend::TensorRt, AccelerationBackend::Cuda],
                    true
                )
                .expect("backend"),
            AccelerationBackend::Cuda
        );
    }

    #[test]
    fn benchmark_reports_latency_and_throughput() {
        let benchmark = BackendBenchmark::from_durations(
            AccelerationBackend::Cpu,
            "test cpu",
            "1",
            2,
            &[
                Duration::from_millis(10),
                Duration::from_millis(20),
                Duration::from_millis(30),
            ],
            1024,
        )
        .expect("benchmark");
        assert_eq!(benchmark.p50_latency_ms, 20.0);
        assert_eq!(benchmark.p95_latency_ms, 30.0);
        assert!(benchmark.throughput_per_second > 0.0);
    }

    #[test]
    fn production_release_requires_redistribution_evaluation_and_benchmarks() {
        let release = ProductionModelRelease {
            release_id: "ocr-latin-1".into(),
            weights_license: "Apache-2.0".into(),
            provenance: "trained from documented public datasets".into(),
            redistributable: false,
            requires_consent: false,
            preprocessing: preprocessing(),
            preferred_backends: vec![AccelerationBackend::Cpu],
            dataset: DatasetDescriptor {
                id: "ocr-eval".into(),
                version: "1".into(),
                license: "CC-BY-4.0".into(),
                examples: 0,
                manifest_sha256: "0".repeat(64),
                strata: vec!["scanner".into(), "phone".into()],
            },
            metrics: Vec::new(),
            benchmarks: Vec::new(),
        };
        let errors = release.validate();
        assert!(errors.iter().any(|error| error.contains("redistributable")));
        assert!(errors.iter().any(|error| error.contains("no examples")));
        assert!(errors.iter().any(|error| error.contains("evaluation metric")));
        assert!(errors.iter().any(|error| error.contains("benchmark")));
    }
}
