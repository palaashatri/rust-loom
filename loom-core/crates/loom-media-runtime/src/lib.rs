//! GPU-aware media rendering and low-latency preview services.
//!
//! The runtime uses an installed FFmpeg executable as a separately isolated
//! codec and GPU worker. Render graphs are validated and translated to
//! shell-free FFmpeg argument vectors. Preview decoding runs audio and video in
//! separate low-latency workers while an audio-master clock and drift
//! controller decide whether video frames should be presented, held, or
//! dropped. CPU fallback is explicit and observable.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Error returned by rendering and preview operations.
#[derive(Debug)]
pub enum MediaRuntimeError {
    /// Filesystem or process I/O failed.
    Io(io::Error),
    /// Required FFmpeg executable or feature is unavailable.
    Unavailable(String),
    /// Render graph or preview configuration is invalid.
    Invalid(String),
    /// FFmpeg returned a failure.
    Process(String),
    /// Expected output was not produced.
    MissingOutput(PathBuf),
    /// Preview reached the end of a stream.
    EndOfStream,
    /// Shared queue state was poisoned.
    Poisoned,
}

impl std::fmt::Display for MediaRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Unavailable(message) => write!(formatter, "media runtime unavailable: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid media operation: {message}"),
            Self::Process(message) => write!(formatter, "media worker failed: {message}"),
            Self::MissingOutput(path) => write!(
                formatter,
                "media worker produced no output at {}",
                path.display()
            ),
            Self::EndOfStream => write!(formatter, "end of media stream"),
            Self::Poisoned => write!(formatter, "media queue state is poisoned"),
        }
    }
}

impl std::error::Error for MediaRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for MediaRuntimeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// FFmpeg GPU execution backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GpuBackend {
    /// CPU-only filters and compositing.
    Cpu,
    /// Vulkan hardware frames and Vulkan overlay/scale filters.
    Vulkan,
    /// NVIDIA CUDA hardware frames and CUDA overlay/scale filters.
    Cuda,
    /// Linux VA-API hardware frames.
    Vaapi,
    /// Microsoft D3D11 hardware frames.
    D3d11,
    /// Apple VideoToolbox hardware decode/encode with CPU composition.
    VideoToolbox,
}

/// Local FFmpeg capabilities relevant to Loom rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCapabilities {
    /// Canonical FFmpeg executable.
    pub ffmpeg: PathBuf,
    /// First line of `ffmpeg -version`.
    pub version: String,
    /// Hardware acceleration methods reported by FFmpeg.
    pub hardware_accelerators: BTreeSet<String>,
    /// Available filter names.
    pub filters: BTreeSet<String>,
    /// Available encoder names.
    pub encoders: BTreeSet<String>,
}

impl MediaCapabilities {
    /// Probe one FFmpeg executable without a command shell.
    pub fn probe(ffmpeg: impl AsRef<Path>) -> Result<Self, MediaRuntimeError> {
        let ffmpeg = canonical_executable(ffmpeg.as_ref())?;
        let version_output = Command::new(&ffmpeg)
            .arg("-version")
            .stdin(Stdio::null())
            .output()?;
        if !version_output.status.success() {
            return Err(MediaRuntimeError::Unavailable(
                "ffmpeg -version failed".into(),
            ));
        }
        let version = String::from_utf8_lossy(&version_output.stdout)
            .lines()
            .next()
            .unwrap_or("unknown FFmpeg")
            .trim()
            .to_string();
        let hardware_accelerators = command_names(&ffmpeg, &["-hide_banner", "-hwaccels"], false)?;
        let filters = command_names(&ffmpeg, &["-hide_banner", "-filters"], true)?;
        let encoders = command_names(&ffmpeg, &["-hide_banner", "-encoders"], true)?;
        Ok(Self {
            ffmpeg,
            version,
            hardware_accelerators,
            filters,
            encoders,
        })
    }

    /// Backends that can perform at least hardware frame transfer and one
    /// accelerated compositing or scaling operation.
    pub fn supported_backends(&self) -> Vec<GpuBackend> {
        let mut backends = vec![GpuBackend::Cpu];
        if self.hardware_accelerators.contains("vulkan")
            && (self.filters.contains("overlay_vulkan") || self.filters.contains("libplacebo"))
        {
            backends.push(GpuBackend::Vulkan);
        }
        if self.hardware_accelerators.contains("cuda") && self.filters.contains("overlay_cuda") {
            backends.push(GpuBackend::Cuda);
        }
        if self.hardware_accelerators.contains("vaapi")
            && (self.filters.contains("overlay_vaapi") || self.filters.contains("scale_vaapi"))
        {
            backends.push(GpuBackend::Vaapi);
        }
        backends
    }

    /// Select the first available backend in preference order, with optional
    /// CPU fallback.
    pub fn select_backend(
        &self,
        preferences: &[GpuBackend],
        allow_cpu_fallback: bool,
    ) -> Result<GpuBackend, MediaRuntimeError> {
        let available: BTreeSet<GpuBackend> = self.supported_backends().into_iter().collect();
        for backend in preferences {
            if available.contains(backend) {
                return Ok(*backend);
            }
        }
        if allow_cpu_fallback {
            Ok(GpuBackend::Cpu)
        } else {
            Err(MediaRuntimeError::Unavailable(
                "none of the requested GPU backends are available".into(),
            ))
        }
    }
}

fn command_names(
    program: &Path,
    arguments: &[&str],
    table: bool,
) -> Result<BTreeSet<String>, MediaRuntimeError> {
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Ok(BTreeSet::new());
    }
    let mut names = BTreeSet::new();
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.contains(':') {
            continue;
        }
        if table {
            let mut parts = trimmed.split_whitespace();
            let _flags = parts.next();
            if let Some(name) = parts.next() {
                if name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    names.insert(name.to_string());
                }
            }
        } else if trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            names.insert(trimmed.to_string());
        }
    }
    Ok(names)
}

fn canonical_executable(path: &Path) -> Result<PathBuf, MediaRuntimeError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        MediaRuntimeError::Unavailable(format!("cannot resolve {}: {error}", path.display()))
    })?;
    if !fs::metadata(&canonical)?.is_file() {
        return Err(MediaRuntimeError::Unavailable(
            "FFmpeg path must resolve to a regular file".into(),
        ));
    }
    Ok(canonical)
}

/// Two-dimensional transform applied before compositing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2D {
    /// Horizontal position in output pixels.
    pub x: f64,
    /// Vertical position in output pixels.
    pub y: f64,
    /// Horizontal scale multiplier.
    pub scale_x: f64,
    /// Vertical scale multiplier.
    pub scale_y: f64,
    /// Clockwise rotation in degrees.
    pub rotation_degrees: f64,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        }
    }
}

impl Transform2D {
    fn validate(self) -> Result<(), MediaRuntimeError> {
        let values = [
            self.x,
            self.y,
            self.scale_x,
            self.scale_y,
            self.rotation_degrees,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(MediaRuntimeError::Invalid(
                "transform contains a non-finite value".into(),
            ));
        }
        if self.scale_x <= 0.0 || self.scale_y <= 0.0 {
            return Err(MediaRuntimeError::Invalid(
                "transform scale must be positive".into(),
            ));
        }
        Ok(())
    }
}

/// Professional image/video effect supported by the render planner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// Exposure adjustment in stops.
    Exposure(f64),
    /// Contrast multiplier where 1.0 is neutral.
    Contrast(f64),
    /// Saturation multiplier where 1.0 is neutral.
    Saturation(f64),
    /// Gamma multiplier where 1.0 is neutral.
    Gamma(f64),
    /// Gaussian blur radius in pixels.
    GaussianBlur(f64),
    /// Unsharp-mask amount.
    Sharpen(f64),
    /// Chroma-key RGB color and similarity threshold.
    ChromaKey {
        /// Key color encoded as `0xRRGGBB`.
        rgb: u32,
        /// Similarity in the range 0..=1.
        similarity: f64,
        /// Blend/softness in the range 0..=1.
        blend: f64,
    },
    /// Three-dimensional color lookup table.
    Lut3d(PathBuf),
    /// Premultiplied alpha multiplier.
    Opacity(f64),
}

impl Effect {
    fn validate(&self) -> Result<(), MediaRuntimeError> {
        match self {
            Self::Exposure(value) if !(-10.0..=10.0).contains(value) => Err(
                MediaRuntimeError::Invalid("exposure is outside -10..10 stops".into()),
            ),
            Self::Contrast(value) if !(0.0..=4.0).contains(value) => Err(
                MediaRuntimeError::Invalid("contrast is outside 0..4".into()),
            ),
            Self::Saturation(value) if !(0.0..=4.0).contains(value) => Err(
                MediaRuntimeError::Invalid("saturation is outside 0..4".into()),
            ),
            Self::Gamma(value) if !(0.1..=10.0).contains(value) => Err(MediaRuntimeError::Invalid(
                "gamma is outside 0.1..10".into(),
            )),
            Self::GaussianBlur(value) if !(0.0..=100.0).contains(value) => Err(
                MediaRuntimeError::Invalid("blur radius is outside 0..100".into()),
            ),
            Self::Sharpen(value) if !(0.0..=5.0).contains(value) => Err(
                MediaRuntimeError::Invalid("sharpen amount is outside 0..5".into()),
            ),
            Self::ChromaKey {
                similarity, blend, ..
            } if !(0.0..=1.0).contains(similarity) || !(0.0..=1.0).contains(blend) => {
                Err(MediaRuntimeError::Invalid(
                    "chroma-key similarity and blend must be in 0..=1".into(),
                ))
            }
            Self::Lut3d(path) if !path.is_file() => Err(MediaRuntimeError::Invalid(format!(
                "LUT file does not exist: {}",
                path.display()
            ))),
            Self::Opacity(value) if !(0.0..=1.0).contains(value) => Err(
                MediaRuntimeError::Invalid("opacity must be in 0..=1".into()),
            ),
            _ => Ok(()),
        }
    }
}

/// One source layer in a render graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLayer {
    /// Stable layer id.
    pub id: String,
    /// Source image or media file.
    pub source: PathBuf,
    /// Start time in seconds for time-based media.
    pub source_start_seconds: f64,
    /// Optional source duration in seconds.
    pub source_duration_seconds: Option<f64>,
    /// Transform applied before compositing.
    pub transform: Transform2D,
    /// Ordered effects.
    pub effects: Vec<Effect>,
    /// Whether the layer is visible.
    pub visible: bool,
}

impl RenderLayer {
    fn validate(&self) -> Result<(), MediaRuntimeError> {
        if self.id.trim().is_empty() {
            return Err(MediaRuntimeError::Invalid("layer id is empty".into()));
        }
        if !self.source.is_file() {
            return Err(MediaRuntimeError::Invalid(format!(
                "layer source does not exist: {}",
                self.source.display()
            )));
        }
        if !self.source_start_seconds.is_finite() || self.source_start_seconds < 0.0 {
            return Err(MediaRuntimeError::Invalid(
                "layer source start is invalid".into(),
            ));
        }
        if self
            .source_duration_seconds
            .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
        {
            return Err(MediaRuntimeError::Invalid(
                "layer source duration is invalid".into(),
            ));
        }
        self.transform.validate()?;
        for effect in &self.effects {
            effect.validate()?;
        }
        Ok(())
    }
}

/// Output settings for a render graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderOutput {
    /// Output width.
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// Frames per second for time-based output.
    pub frames_per_second: f64,
    /// Optional duration. `None` produces a single still frame.
    pub duration_seconds: Option<f64>,
    /// Output file path.
    pub path: PathBuf,
    /// Video or image codec name passed to FFmpeg.
    pub codec: String,
    /// Pixel format.
    pub pixel_format: String,
    /// Optional target bitrate.
    pub bitrate: Option<String>,
}

impl RenderOutput {
    fn validate(&self) -> Result<(), MediaRuntimeError> {
        if self.width == 0 || self.height == 0 || self.width > 32_768 || self.height > 32_768 {
            return Err(MediaRuntimeError::Invalid(
                "render dimensions must be within 1..32768".into(),
            ));
        }
        if !self.frames_per_second.is_finite() || !(1.0..=240.0).contains(&self.frames_per_second) {
            return Err(MediaRuntimeError::Invalid(
                "frame rate must be within 1..240".into(),
            ));
        }
        if self
            .duration_seconds
            .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
        {
            return Err(MediaRuntimeError::Invalid(
                "render duration is invalid".into(),
            ));
        }
        if self.path.file_name().is_none() || self.codec.trim().is_empty() {
            return Err(MediaRuntimeError::Invalid(
                "output path and codec are required".into(),
            ));
        }
        Ok(())
    }
}

/// Validated compositing graph shared by Photo, Motion, and Video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderGraph {
    /// Background RGBA color encoded as `0xRRGGBBAA`.
    pub background_rgba: u32,
    /// Layers from bottom to top.
    pub layers: Vec<RenderLayer>,
    /// Output settings.
    pub output: RenderOutput,
}

impl RenderGraph {
    /// Validate graph limits, paths, identifiers, and effect ranges.
    pub fn validate(&self) -> Result<(), MediaRuntimeError> {
        if self.layers.is_empty() || self.layers.len() > 256 {
            return Err(MediaRuntimeError::Invalid(
                "render graph must contain 1..=256 layers".into(),
            ));
        }
        self.output.validate()?;
        let mut ids = BTreeSet::new();
        for layer in &self.layers {
            layer.validate()?;
            if !ids.insert(layer.id.clone()) {
                return Err(MediaRuntimeError::Invalid(format!(
                    "duplicate layer id {}",
                    layer.id
                )));
            }
        }
        Ok(())
    }

    /// Compile the graph to a shell-free FFmpeg plan. GPU overlays are used
    /// only when the selected backend and probed filter inventory support them;
    /// otherwise compilation fails rather than silently claiming acceleration.
    pub fn compile(
        &self,
        capabilities: &MediaCapabilities,
        backend: GpuBackend,
    ) -> Result<CompiledRender, MediaRuntimeError> {
        self.validate()?;
        validate_backend(capabilities, backend)?;
        let mut arguments = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into()];
        append_device_arguments(&mut arguments, backend);
        let visible: Vec<&RenderLayer> = self.layers.iter().filter(|layer| layer.visible).collect();
        if visible.is_empty() {
            return Err(MediaRuntimeError::Invalid(
                "render graph has no visible layers".into(),
            ));
        }
        for layer in &visible {
            if layer.source_start_seconds > 0.0 {
                arguments.push("-ss".into());
                arguments.push(format_decimal(layer.source_start_seconds));
            }
            if let Some(duration) = layer.source_duration_seconds {
                arguments.push("-t".into());
                arguments.push(format_decimal(duration));
            }
            arguments.push("-i".into());
            arguments.push(
                fs::canonicalize(&layer.source)?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let mut filter = String::new();
        let mut gpu_stages = Vec::new();
        let mut cpu_stages = Vec::new();
        let (red, green, blue, alpha) = rgba_components(self.background_rgba);
        filter.push_str(&format!(
            "color=c=0x{red:02x}{green:02x}{blue:02x}@{:.6}:s={}x{}:r={:.6}[base_cpu]",
            f64::from(alpha) / 255.0,
            self.output.width,
            self.output.height,
            self.output.frames_per_second
        ));
        match backend {
            GpuBackend::Vulkan | GpuBackend::D3d11 => {
                filter.push_str(";[base_cpu]format=rgba,hwupload[base0]");
                gpu_stages.push("base_hwupload".into());
            }
            GpuBackend::Cuda => {
                filter.push_str(";[base_cpu]format=rgba,hwupload_cuda[base0]");
                gpu_stages.push("base_hwupload_cuda".into());
            }
            GpuBackend::Vaapi => {
                filter.push_str(";[base_cpu]format=nv12,hwupload[base0]");
                gpu_stages.push("base_hwupload_vaapi".into());
            }
            GpuBackend::VideoToolbox | GpuBackend::Cpu => {
                filter.push_str(";[base_cpu]null[base0]");
            }
        }
        let mut current_base = "base0".to_string();
        for (index, layer) in visible.iter().enumerate() {
            let source_label = format!("layer{index}");
            filter.push(';');
            filter.push_str(&format!("[{index}:v]"));
            let (chain, stage_kinds) = compile_layer_chain(layer, &self.output, backend)?;
            filter.push_str(&chain);
            filter.push_str(&format!("[{source_label}]"));
            for stage in stage_kinds {
                match stage {
                    StageKind::Gpu(name) => gpu_stages.push(name),
                    StageKind::Cpu(name) => cpu_stages.push(name),
                }
            }
            let output_label = format!("base{}", index + 1);
            filter.push(';');
            let overlay = overlay_filter(backend, layer.transform.x, layer.transform.y)?;
            if backend == GpuBackend::Cpu {
                cpu_stages.push("overlay".into());
            } else {
                gpu_stages.push(overlay.split('=').next().unwrap_or(&overlay).to_string());
            }
            filter.push_str(&format!(
                "[{current_base}][{source_label}]{overlay}[{output_label}]"
            ));
            current_base = output_label;
        }
        let map_label = if backend != GpuBackend::Cpu
            && !hardware_encoder_compatible(backend, &self.output.codec)
        {
            let final_label = "final_cpu";
            filter.push_str(&format!(
                ";[{current_base}]hwdownload,format=rgba[{final_label}]"
            ));
            cpu_stages.push("hwdownload".into());
            final_label.to_string()
        } else {
            current_base
        };
        arguments.push("-filter_complex".into());
        arguments.push(filter);
        arguments.push("-map".into());
        arguments.push(format!("[{map_label}]"));
        if let Some(duration) = self.output.duration_seconds {
            arguments.push("-t".into());
            arguments.push(format_decimal(duration));
        } else {
            arguments.push("-frames:v".into());
            arguments.push("1".into());
        }
        arguments.push("-c:v".into());
        arguments.push(self.output.codec.clone());
        arguments.push("-pix_fmt".into());
        arguments.push(self.output.pixel_format.clone());
        if let Some(bitrate) = &self.output.bitrate {
            arguments.push("-b:v".into());
            arguments.push(bitrate.clone());
        }
        arguments.push(self.output.path.to_string_lossy().into_owned());
        Ok(CompiledRender {
            program: capabilities.ffmpeg.clone(),
            arguments,
            output: self.output.path.clone(),
            backend,
            gpu_stages,
            cpu_stages,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StageKind {
    Gpu(String),
    Cpu(String),
}

fn validate_backend(
    capabilities: &MediaCapabilities,
    backend: GpuBackend,
) -> Result<(), MediaRuntimeError> {
    if backend == GpuBackend::Cpu {
        return Ok(());
    }
    if capabilities.supported_backends().contains(&backend) {
        Ok(())
    } else {
        Err(MediaRuntimeError::Unavailable(format!(
            "requested backend {backend:?} lacks required FFmpeg acceleration/filter support"
        )))
    }
}

fn append_device_arguments(arguments: &mut Vec<String>, backend: GpuBackend) {
    match backend {
        GpuBackend::Vulkan => {
            arguments.extend([
                "-init_hw_device".into(),
                "vulkan=loom".into(),
                "-filter_hw_device".into(),
                "loom".into(),
            ]);
        }
        GpuBackend::Cuda => {
            arguments.extend([
                "-init_hw_device".into(),
                "cuda=loom".into(),
                "-filter_hw_device".into(),
                "loom".into(),
            ]);
        }
        GpuBackend::Vaapi => {
            arguments.extend([
                "-init_hw_device".into(),
                "vaapi=loom:/dev/dri/renderD128".into(),
                "-filter_hw_device".into(),
                "loom".into(),
            ]);
        }
        GpuBackend::D3d11 => {
            arguments.extend([
                "-init_hw_device".into(),
                "d3d11va=loom".into(),
                "-filter_hw_device".into(),
                "loom".into(),
            ]);
        }
        GpuBackend::VideoToolbox | GpuBackend::Cpu => {}
    }
}

fn compile_layer_chain(
    layer: &RenderLayer,
    output: &RenderOutput,
    backend: GpuBackend,
) -> Result<(String, Vec<StageKind>), MediaRuntimeError> {
    let mut filters = Vec::new();
    let mut stages = Vec::new();
    let scaled_width = (f64::from(output.width) * layer.transform.scale_x)
        .round()
        .clamp(1.0, 32_768.0) as u32;
    let scaled_height = (f64::from(output.height) * layer.transform.scale_y)
        .round()
        .clamp(1.0, 32_768.0) as u32;
    filters.push("format=rgba".to_string());
    stages.push(StageKind::Cpu("format".into()));
    for effect in &layer.effects {
        let (expression, stage) = compile_effect(effect)?;
        filters.push(expression);
        stages.push(stage);
    }
    if layer.transform.rotation_degrees.abs() > f64::EPSILON {
        filters.push(format!(
            "rotate={:.10}*PI/180:ow=rotw(iw):oh=roth(ih):c=none",
            layer.transform.rotation_degrees
        ));
        stages.push(StageKind::Cpu("rotate".into()));
    }
    match backend {
        GpuBackend::Vulkan => {
            filters.push("hwupload".into());
            filters.push(format!("scale_vulkan={scaled_width}:{scaled_height}"));
            stages.push(StageKind::Gpu("scale_vulkan".into()));
        }
        GpuBackend::Cuda => {
            filters.push("hwupload_cuda".into());
            filters.push(format!("scale_cuda={scaled_width}:{scaled_height}"));
            stages.push(StageKind::Gpu("scale_cuda".into()));
        }
        GpuBackend::Vaapi => {
            filters.push("format=nv12".into());
            filters.push("hwupload".into());
            filters.push(format!("scale_vaapi=w={scaled_width}:h={scaled_height}"));
            stages.push(StageKind::Gpu("scale_vaapi".into()));
        }
        GpuBackend::D3d11 => {
            filters.push("hwupload".into());
            filters.push(format!("scale={scaled_width}:{scaled_height}"));
            stages.push(StageKind::Cpu("scale".into()));
        }
        GpuBackend::VideoToolbox | GpuBackend::Cpu => {
            filters.push(format!(
                "scale={scaled_width}:{scaled_height}:flags=lanczos"
            ));
            stages.push(StageKind::Cpu("scale".into()));
        }
    }
    Ok((filters.join(","), stages))
}

fn compile_effect(effect: &Effect) -> Result<(String, StageKind), MediaRuntimeError> {
    effect.validate()?;
    Ok(match effect {
        Effect::Exposure(stops) => {
            let brightness = (2_f64.powf(*stops) - 1.0).clamp(-1.0, 1.0);
            (
                format!("eq=brightness={brightness:.10}"),
                StageKind::Cpu("exposure".into()),
            )
        }
        Effect::Contrast(value) => (
            format!("eq=contrast={value:.10}"),
            StageKind::Cpu("contrast".into()),
        ),
        Effect::Saturation(value) => (
            format!("eq=saturation={value:.10}"),
            StageKind::Cpu("saturation".into()),
        ),
        Effect::Gamma(value) => (
            format!("eq=gamma={value:.10}"),
            StageKind::Cpu("gamma".into()),
        ),
        Effect::GaussianBlur(radius) => (
            format!("gblur=sigma={radius:.10}"),
            StageKind::Cpu("gblur".into()),
        ),
        Effect::Sharpen(amount) => (
            format!("unsharp=5:5:{amount:.10}:5:5:0"),
            StageKind::Cpu("unsharp".into()),
        ),
        Effect::ChromaKey {
            rgb,
            similarity,
            blend,
        } => (
            format!("chromakey=0x{rgb:06x}:{similarity:.10}:{blend:.10}"),
            StageKind::Cpu("chromakey".into()),
        ),
        Effect::Lut3d(path) => (
            format!(
                "lut3d=file={}",
                escape_filter_path(&fs::canonicalize(path)?)
            ),
            StageKind::Cpu("lut3d".into()),
        ),
        Effect::Opacity(value) => (
            format!("colorchannelmixer=aa={value:.10}"),
            StageKind::Cpu("opacity".into()),
        ),
    })
}

fn overlay_filter(backend: GpuBackend, x: f64, y: f64) -> Result<String, MediaRuntimeError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(MediaRuntimeError::Invalid(
            "overlay position contains a non-finite value".into(),
        ));
    }
    let x = format_decimal(x);
    let y = format_decimal(y);
    Ok(match backend {
        GpuBackend::Vulkan => format!("overlay_vulkan=x={x}:y={y}"),
        GpuBackend::Cuda => format!("overlay_cuda=x={x}:y={y}"),
        GpuBackend::Vaapi => format!("overlay_vaapi=x={x}:y={y}"),
        GpuBackend::D3d11 | GpuBackend::VideoToolbox | GpuBackend::Cpu => {
            format!("overlay=x={x}:y={y}:format=auto")
        }
    })
}

fn hardware_encoder_compatible(backend: GpuBackend, codec: &str) -> bool {
    let codec = codec.to_ascii_lowercase();
    match backend {
        GpuBackend::Cuda => codec.ends_with("_nvenc"),
        GpuBackend::Vaapi => codec.ends_with("_vaapi"),
        GpuBackend::Vulkan => codec.ends_with("_vulkan"),
        GpuBackend::D3d11 => codec.contains("d3d11") || codec.contains("mf"),
        GpuBackend::VideoToolbox => codec.ends_with("_videotoolbox"),
        GpuBackend::Cpu => true,
    }
}

fn rgba_components(rgba: u32) -> (u8, u8, u8, u8) {
    (
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        rgba as u8,
    )
}

fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
        .replace(',', "\\,")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn format_decimal(value: f64) -> String {
    let mut text = format!("{value:.10}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

/// Shell-free executable rendering plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRender {
    /// FFmpeg executable.
    pub program: PathBuf,
    /// FFmpeg arguments excluding the executable.
    pub arguments: Vec<String>,
    /// Expected output path.
    pub output: PathBuf,
    /// Selected backend.
    pub backend: GpuBackend,
    /// Filters that execute on hardware frames.
    pub gpu_stages: Vec<String>,
    /// Filters that execute on CPU frames.
    pub cpu_stages: Vec<String>,
}

impl CompiledRender {
    /// Execute the render, reporting progress from FFmpeg's machine-readable
    /// `-progress pipe:2` stream.
    pub fn execute<F>(&self, mut progress: F) -> Result<(), MediaRuntimeError>
    where
        F: FnMut(RenderProgress),
    {
        if let Some(parent) = self
            .output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut arguments = self.arguments.clone();
        let insertion = arguments.len().saturating_sub(1);
        arguments.splice(
            insertion..insertion,
            ["-progress".into(), "pipe:2".into(), "-nostats".into()],
        );
        let mut child = Command::new(&self.program)
            .args(&arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| MediaRuntimeError::Process("progress stream unavailable".into()))?;
        let mut fields = BTreeMap::new();
        for line in BufReader::new(stderr).lines() {
            let line = line?;
            if let Some((key, value)) = line.split_once('=') {
                fields.insert(key.to_string(), value.to_string());
                if key == "progress" {
                    progress(RenderProgress::from_fields(&fields));
                    fields.clear();
                }
            }
        }
        let status = child.wait()?;
        if !status.success() {
            return Err(MediaRuntimeError::Process(status.to_string()));
        }
        let metadata = fs::metadata(&self.output)
            .map_err(|_| MediaRuntimeError::MissingOutput(self.output.clone()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(MediaRuntimeError::MissingOutput(self.output.clone()));
        }
        Ok(())
    }
}

/// Machine-readable render progress snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderProgress {
    /// Current encoded frame, when reported.
    pub frame: Option<u64>,
    /// Current output time in microseconds, when reported.
    pub output_time_micros: Option<u64>,
    /// Current speed string, such as `1.2x`.
    pub speed: Option<String>,
    /// Whether FFmpeg reported completion.
    pub complete: bool,
}

impl RenderProgress {
    fn from_fields(fields: &BTreeMap<String, String>) -> Self {
        Self {
            frame: fields.get("frame").and_then(|value| value.parse().ok()),
            output_time_micros: fields
                .get("out_time_us")
                .or_else(|| fields.get("out_time_ms"))
                .and_then(|value| value.parse().ok()),
            speed: fields.get("speed").cloned(),
            complete: fields.get("progress").is_some_and(|value| value == "end"),
        }
    }
}

/// Configuration for low-latency dual-worker preview decoding.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewConfiguration {
    /// Source media file.
    pub source: PathBuf,
    /// Output video width.
    pub width: u32,
    /// Output video height.
    pub height: u32,
    /// Video frame rate.
    pub frames_per_second: f64,
    /// Audio sample rate.
    pub sample_rate: u32,
    /// Audio channel count.
    pub channels: u16,
    /// Initial seek position.
    pub start_seconds: f64,
    /// Maximum decoded video frames retained in memory.
    pub video_queue_frames: usize,
    /// Maximum decoded audio frames retained in memory.
    pub audio_queue_frames: usize,
}

impl PreviewConfiguration {
    fn validate(&self) -> Result<(), MediaRuntimeError> {
        if !self.source.is_file() {
            return Err(MediaRuntimeError::Invalid(format!(
                "preview source does not exist: {}",
                self.source.display()
            )));
        }
        if self.width == 0 || self.height == 0 || self.width > 8192 || self.height > 8192 {
            return Err(MediaRuntimeError::Invalid(
                "preview dimensions must be within 1..8192".into(),
            ));
        }
        if !self.frames_per_second.is_finite()
            || !(1.0..=240.0).contains(&self.frames_per_second)
            || self.sample_rate < 8_000
            || self.sample_rate > 384_000
            || self.channels == 0
            || self.channels > 32
            || !self.start_seconds.is_finite()
            || self.start_seconds < 0.0
            || self.video_queue_frames == 0
            || self.audio_queue_frames == 0
        {
            return Err(MediaRuntimeError::Invalid(
                "preview timing, audio, or queue configuration is invalid".into(),
            ));
        }
        Ok(())
    }

    /// Bytes in one decoded RGBA video frame.
    pub fn video_frame_bytes(&self) -> Result<usize, MediaRuntimeError> {
        let pixels = usize::try_from(self.width)
            .ok()
            .and_then(|width| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                MediaRuntimeError::Invalid("preview frame size overflows usize".into())
            })?;
        Ok(pixels)
    }
}

/// Decoded video frame with media timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFrame {
    /// Presentation timestamp relative to the source.
    pub presentation_time: Duration,
    /// RGBA8 pixels.
    pub rgba: Vec<u8>,
}

/// Decoded interleaved floating-point audio block.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioBlock {
    /// Presentation timestamp of the first frame.
    pub presentation_time: Duration,
    /// Number of interleaved channels.
    pub channels: u16,
    /// Interleaved `f32` samples.
    pub samples: Vec<f32>,
}

/// Fixed-capacity queue that drops the oldest item when a producer outruns the
/// consumer. Drop counts are observable for diagnostics.
#[derive(Debug, Clone)]
pub struct BoundedQueue<T> {
    inner: Arc<Mutex<QueueState<T>>>,
}

#[derive(Debug)]
struct QueueState<T> {
    capacity: usize,
    values: VecDeque<T>,
    dropped: u64,
    closed: bool,
}

impl<T> BoundedQueue<T> {
    /// Create a queue with non-zero capacity.
    pub fn new(capacity: usize) -> Result<Self, MediaRuntimeError> {
        if capacity == 0 {
            return Err(MediaRuntimeError::Invalid(
                "queue capacity must be non-zero".into(),
            ));
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(QueueState {
                capacity,
                values: VecDeque::with_capacity(capacity),
                dropped: 0,
                closed: false,
            })),
        })
    }

    /// Push a value, dropping the oldest value if full.
    pub fn push(&self, value: T) -> Result<(), MediaRuntimeError> {
        let mut state = self.inner.lock().map_err(|_| MediaRuntimeError::Poisoned)?;
        if state.closed {
            return Err(MediaRuntimeError::EndOfStream);
        }
        if state.values.len() == state.capacity {
            let _ = state.values.pop_front();
            state.dropped = state.dropped.saturating_add(1);
        }
        state.values.push_back(value);
        Ok(())
    }

    /// Pop the oldest queued value.
    pub fn pop(&self) -> Result<Option<T>, MediaRuntimeError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| MediaRuntimeError::Poisoned)?
            .values
            .pop_front())
    }

    /// Current queue length.
    pub fn len(&self) -> Result<usize, MediaRuntimeError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| MediaRuntimeError::Poisoned)?
            .values
            .len())
    }

    /// Whether the queue contains no values.
    pub fn is_empty(&self) -> Result<bool, MediaRuntimeError> {
        Ok(self.len()? == 0)
    }

    /// Total values dropped because producers outran consumers.
    pub fn dropped(&self) -> Result<u64, MediaRuntimeError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| MediaRuntimeError::Poisoned)?
            .dropped)
    }

    /// Mark the producer side closed.
    pub fn close(&self) -> Result<(), MediaRuntimeError> {
        self.inner
            .lock()
            .map_err(|_| MediaRuntimeError::Poisoned)?
            .closed = true;
        Ok(())
    }

    /// Whether the producer closed the queue.
    pub fn is_closed(&self) -> Result<bool, MediaRuntimeError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| MediaRuntimeError::Poisoned)?
            .closed)
    }
}

/// Audio-master playback clock based on rendered sample frames.
#[derive(Debug, Clone)]
pub struct PlaybackClock {
    sample_rate: u32,
    anchor_media_time: Duration,
    anchor_instant: Instant,
    rendered_audio_frames: u64,
    running: bool,
}

impl PlaybackClock {
    /// Create a paused clock at a media position.
    pub fn new(sample_rate: u32, media_time: Duration) -> Result<Self, MediaRuntimeError> {
        if sample_rate == 0 {
            return Err(MediaRuntimeError::Invalid(
                "clock sample rate must be non-zero".into(),
            ));
        }
        Ok(Self {
            sample_rate,
            anchor_media_time: media_time,
            anchor_instant: Instant::now(),
            rendered_audio_frames: 0,
            running: false,
        })
    }

    /// Start or resume monotonic playback.
    pub fn start(&mut self) {
        if !self.running {
            self.anchor_media_time = self.position();
            self.anchor_instant = Instant::now();
            self.rendered_audio_frames = 0;
            self.running = true;
        }
    }

    /// Pause at the current position.
    pub fn pause(&mut self) {
        if self.running {
            self.anchor_media_time = self.position();
            self.anchor_instant = Instant::now();
            self.rendered_audio_frames = 0;
            self.running = false;
        }
    }

    /// Seek to a media position and reset rendered-sample accounting.
    pub fn seek(&mut self, media_time: Duration) {
        self.anchor_media_time = media_time;
        self.anchor_instant = Instant::now();
        self.rendered_audio_frames = 0;
    }

    /// Report audio frames accepted by the output device.
    pub fn audio_frames_rendered(&mut self, frames: u64) {
        self.rendered_audio_frames = self.rendered_audio_frames.saturating_add(frames);
    }

    /// Current audio-master media position.
    pub fn position(&self) -> Duration {
        if !self.running {
            return self.anchor_media_time;
        }
        if self.rendered_audio_frames > 0 {
            return self.anchor_media_time
                + Duration::from_secs_f64(
                    self.rendered_audio_frames as f64 / f64::from(self.sample_rate),
                );
        }
        self.anchor_media_time + self.anchor_instant.elapsed()
    }
}

/// A/V synchronization action for the next decoded video frame or audio block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncAction {
    /// Present normally.
    Present,
    /// Hold the current video frame because the candidate is early.
    HoldVideo(Duration),
    /// Drop the candidate video frame because it is late.
    DropVideo,
    /// Gently resample audio by the supplied rate multiplier.
    ResampleAudio(f64),
    /// Hard resynchronization is required.
    Discontinuity,
}

/// Audio/video drift controller with bounded soft correction.
#[derive(Debug, Clone)]
pub struct AvSyncController {
    /// Video drift tolerated without correction.
    pub video_tolerance: Duration,
    /// Drift at which a hard discontinuity is emitted.
    pub discontinuity_threshold: Duration,
    /// Maximum audio resampling correction in either direction.
    pub max_resample_fraction: f64,
}

impl Default for AvSyncController {
    fn default() -> Self {
        Self {
            video_tolerance: Duration::from_millis(20),
            discontinuity_threshold: Duration::from_millis(250),
            max_resample_fraction: 0.005,
        }
    }
}

impl AvSyncController {
    /// Decide how to handle a candidate video frame relative to audio-master
    /// time.
    pub fn video_action(&self, frame_time: Duration, master_time: Duration) -> SyncAction {
        let drift = signed_duration(frame_time, master_time);
        if drift.unsigned_abs() >= self.discontinuity_threshold {
            return SyncAction::Discontinuity;
        }
        if drift > duration_to_signed(self.video_tolerance) {
            return SyncAction::HoldVideo(drift.unsigned_abs());
        }
        if drift < -duration_to_signed(self.video_tolerance) {
            return SyncAction::DropVideo;
        }
        SyncAction::Present
    }

    /// Calculate a small audio resampling ratio for sustained drift. Values
    /// remain within `1 ± max_resample_fraction`.
    pub fn audio_action(&self, audio_time: Duration, master_time: Duration) -> SyncAction {
        let drift = signed_duration(audio_time, master_time);
        if drift.unsigned_abs() >= self.discontinuity_threshold {
            return SyncAction::Discontinuity;
        }
        let seconds = drift.as_secs_f64();
        let correction =
            (-seconds * 0.05).clamp(-self.max_resample_fraction, self.max_resample_fraction);
        if correction.abs() < 0.000_01 {
            SyncAction::Present
        } else {
            SyncAction::ResampleAudio(1.0 + correction)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SignedDuration(i128);

impl SignedDuration {
    fn unsigned_abs(self) -> Duration {
        let nanos = self.0.unsigned_abs().min(u64::MAX as u128) as u64;
        Duration::from_nanos(nanos)
    }

    fn as_secs_f64(self) -> f64 {
        self.0 as f64 / 1_000_000_000.0
    }
}

impl std::ops::Neg for SignedDuration {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

fn signed_duration(first: Duration, second: Duration) -> SignedDuration {
    SignedDuration(duration_to_nanos(first) - duration_to_nanos(second))
}

fn duration_to_signed(duration: Duration) -> SignedDuration {
    SignedDuration(duration_to_nanos(duration))
}

fn duration_to_nanos(duration: Duration) -> i128 {
    duration.as_nanos().min(i128::MAX as u128) as i128
}

/// Running low-latency preview decoder pair.
#[derive(Debug)]
pub struct PreviewSession {
    capabilities: MediaCapabilities,
    configuration: PreviewConfiguration,
    video_child: Child,
    audio_child: Child,
    video_stdout: ChildStdout,
    audio_stdout: ChildStdout,
    next_video_frame: u64,
    next_audio_frame: u64,
    /// Decoded video queue suitable for a renderer thread.
    pub video_queue: BoundedQueue<VideoFrame>,
    /// Decoded audio queue suitable for an audio callback feeder.
    pub audio_queue: BoundedQueue<AudioBlock>,
}

impl PreviewSession {
    /// Start separate low-latency FFmpeg video and audio decoders at an exact
    /// common seek position. Both workers use timestamp-preserving decoding;
    /// [`PlaybackClock`] and [`AvSyncController`] provide presentation sync.
    pub fn start(
        capabilities: MediaCapabilities,
        configuration: PreviewConfiguration,
    ) -> Result<Self, MediaRuntimeError> {
        configuration.validate()?;
        let source = fs::canonicalize(&configuration.source)?;
        let (mut video_child, video_stdout) =
            spawn_video_decoder(&capabilities.ffmpeg, &source, &configuration)?;
        let audio_result = spawn_audio_decoder(&capabilities.ffmpeg, &source, &configuration);
        let (audio_child, audio_stdout) = match audio_result {
            Ok(pair) => pair,
            Err(error) => {
                let _ = video_child.kill();
                let _ = video_child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            capabilities,
            video_child,
            audio_child,
            video_stdout,
            audio_stdout,
            next_video_frame: 0,
            next_audio_frame: 0,
            video_queue: BoundedQueue::new(configuration.video_queue_frames)?,
            audio_queue: BoundedQueue::new(configuration.audio_queue_frames)?,
            configuration,
        })
    }

    /// Decode and enqueue one RGBA video frame.
    pub fn decode_video_frame(&mut self) -> Result<VideoFrame, MediaRuntimeError> {
        let mut rgba = vec![0; self.configuration.video_frame_bytes()?];
        read_exact_or_eof(&mut self.video_stdout, &mut rgba)?;
        let presentation_time = Duration::from_secs_f64(
            self.configuration.start_seconds
                + self.next_video_frame as f64 / self.configuration.frames_per_second,
        );
        self.next_video_frame = self.next_video_frame.saturating_add(1);
        let frame = VideoFrame {
            presentation_time,
            rgba,
        };
        self.video_queue.push(frame.clone())?;
        Ok(frame)
    }

    /// Decode and enqueue up to `frames` interleaved floating-point audio
    /// frames. A short read at stream end is returned when it contains at least
    /// one complete frame.
    pub fn decode_audio_frames(&mut self, frames: usize) -> Result<AudioBlock, MediaRuntimeError> {
        if frames == 0 {
            return Err(MediaRuntimeError::Invalid(
                "audio decode frame count must be non-zero".into(),
            ));
        }
        let channels = usize::from(self.configuration.channels);
        let sample_count = frames
            .checked_mul(channels)
            .ok_or_else(|| MediaRuntimeError::Invalid("audio request overflows usize".into()))?;
        let byte_count = sample_count
            .checked_mul(std::mem::size_of::<f32>())
            .ok_or_else(|| MediaRuntimeError::Invalid("audio byte count overflows usize".into()))?;
        let mut bytes = vec![0_u8; byte_count];
        let read = read_some(&mut self.audio_stdout, &mut bytes)?;
        let complete_samples = read / 4;
        let complete_frames = complete_samples / channels;
        if complete_frames == 0 {
            return Err(MediaRuntimeError::EndOfStream);
        }
        bytes.truncate(complete_frames * channels * 4);
        let samples = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        let presentation_time = Duration::from_secs_f64(
            self.configuration.start_seconds
                + self.next_audio_frame as f64 / f64::from(self.configuration.sample_rate),
        );
        self.next_audio_frame = self
            .next_audio_frame
            .saturating_add(complete_frames.try_into().unwrap_or(u64::MAX));
        let block = AudioBlock {
            presentation_time,
            channels: self.configuration.channels,
            samples,
        };
        self.audio_queue.push(block.clone())?;
        Ok(block)
    }

    /// Restart both decoders at a new exact seek position while retaining the
    /// same decode format and queue capacities.
    pub fn seek(&mut self, seconds: f64) -> Result<(), MediaRuntimeError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(MediaRuntimeError::Invalid(
                "seek position must be finite and non-negative".into(),
            ));
        }
        self.stop_workers();
        self.configuration.start_seconds = seconds;
        let source = fs::canonicalize(&self.configuration.source)?;
        let (video_child, video_stdout) =
            spawn_video_decoder(&self.capabilities.ffmpeg, &source, &self.configuration)?;
        let audio_result =
            spawn_audio_decoder(&self.capabilities.ffmpeg, &source, &self.configuration);
        let (audio_child, audio_stdout) = match audio_result {
            Ok(pair) => pair,
            Err(error) => {
                let mut child = video_child;
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        self.video_child = video_child;
        self.video_stdout = video_stdout;
        self.audio_child = audio_child;
        self.audio_stdout = audio_stdout;
        self.next_video_frame = 0;
        self.next_audio_frame = 0;
        Ok(())
    }

    /// Stop both decoder workers.
    pub fn stop(mut self) {
        self.stop_workers();
    }

    fn stop_workers(&mut self) {
        for child in [&mut self.video_child, &mut self.audio_child] {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

impl Drop for PreviewSession {
    fn drop(&mut self) {
        self.stop_workers();
    }
}

fn spawn_video_decoder(
    ffmpeg: &Path,
    source: &Path,
    configuration: &PreviewConfiguration,
) -> Result<(Child, ChildStdout), MediaRuntimeError> {
    let mut arguments = low_latency_input_arguments(configuration.start_seconds);
    arguments.push("-i".into());
    arguments.push(source.to_string_lossy().into_owned());
    arguments.extend([
        "-map".into(),
        "0:v:0".into(),
        "-an".into(),
        "-sn".into(),
        "-dn".into(),
        "-vf".into(),
        format!(
            "scale={}:{}:flags=fast_bilinear,format=rgba",
            configuration.width, configuration.height
        ),
        "-r".into(),
        format_decimal(configuration.frames_per_second),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "pipe:1".into(),
    ]);
    let mut child = Command::new(ffmpeg)
        .args(&arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MediaRuntimeError::Process("video decoder stdout unavailable".into()))?;
    Ok((child, stdout))
}

fn spawn_audio_decoder(
    ffmpeg: &Path,
    source: &Path,
    configuration: &PreviewConfiguration,
) -> Result<(Child, ChildStdout), MediaRuntimeError> {
    let mut arguments = low_latency_input_arguments(configuration.start_seconds);
    arguments.push("-i".into());
    arguments.push(source.to_string_lossy().into_owned());
    arguments.extend([
        "-map".into(),
        "0:a:0?".into(),
        "-vn".into(),
        "-sn".into(),
        "-dn".into(),
        "-ac".into(),
        configuration.channels.to_string(),
        "-ar".into(),
        configuration.sample_rate.to_string(),
        "-f".into(),
        "f32le".into(),
        "-acodec".into(),
        "pcm_f32le".into(),
        "pipe:1".into(),
    ]);
    let mut child = Command::new(ffmpeg)
        .args(&arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MediaRuntimeError::Process("audio decoder stdout unavailable".into()))?;
    Ok((child, stdout))
}

fn low_latency_input_arguments(start_seconds: f64) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-fflags".into(),
        "nobuffer+discardcorrupt+genpts".into(),
        "-flags".into(),
        "low_delay".into(),
        "-probesize".into(),
        "1048576".into(),
        "-analyzeduration".into(),
        "1000000".into(),
        "-threads".into(),
        "0".into(),
        "-ss".into(),
        format_decimal(start_seconds),
    ]
}

fn read_exact_or_eof(reader: &mut impl Read, buffer: &mut [u8]) -> Result<(), MediaRuntimeError> {
    let mut offset = 0;
    while offset < buffer.len() {
        let read = reader.read(&mut buffer[offset..])?;
        if read == 0 {
            return Err(MediaRuntimeError::EndOfStream);
        }
        offset += read;
    }
    Ok(())
}

fn read_some(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, MediaRuntimeError> {
    let mut offset = 0;
    while offset < buffer.len() {
        let read = reader.read(&mut buffer[offset..])?;
        if read == 0 {
            break;
        }
        offset += read;
    }
    Ok(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_capabilities(filters: &[&str], accelerators: &[&str]) -> MediaCapabilities {
        MediaCapabilities {
            ffmpeg: PathBuf::from("/usr/bin/ffmpeg"),
            version: "ffmpeg test".into(),
            hardware_accelerators: accelerators.iter().map(|value| (*value).into()).collect(),
            filters: filters.iter().map(|value| (*value).into()).collect(),
            encoders: BTreeSet::new(),
        }
    }

    #[test]
    fn backend_selection_requires_real_filter_support() {
        let capabilities = fake_capabilities(&["overlay_vulkan"], &["vulkan"]);
        assert_eq!(
            capabilities
                .select_backend(&[GpuBackend::Cuda, GpuBackend::Vulkan], true)
                .expect("backend"),
            GpuBackend::Vulkan
        );
        assert_eq!(
            capabilities
                .select_backend(&[GpuBackend::Cuda], true)
                .expect("fallback"),
            GpuBackend::Cpu
        );
    }

    #[test]
    fn render_graph_compiles_gpu_overlay_and_reports_cpu_effects() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let source = temporary.path().join("source.png");
        fs::write(&source, b"not decoded during planning").expect("source");
        let graph = RenderGraph {
            background_rgba: 0x101010ff,
            layers: vec![RenderLayer {
                id: "foreground".into(),
                source,
                source_start_seconds: 0.0,
                source_duration_seconds: None,
                transform: Transform2D::default(),
                effects: vec![Effect::Contrast(1.2), Effect::Opacity(0.8)],
                visible: true,
            }],
            output: RenderOutput {
                width: 1920,
                height: 1080,
                frames_per_second: 30.0,
                duration_seconds: Some(1.0),
                path: temporary.path().join("out.mp4"),
                codec: "h264_nvenc".into(),
                pixel_format: "yuv420p".into(),
                bitrate: Some("12M".into()),
            },
        };
        let capabilities = fake_capabilities(&["overlay_cuda", "scale_cuda"], &["cuda"]);
        let compiled = graph
            .compile(&capabilities, GpuBackend::Cuda)
            .expect("compile");
        assert!(compiled.gpu_stages.contains(&"overlay_cuda".into()));
        assert!(compiled.gpu_stages.contains(&"scale_cuda".into()));
        assert!(compiled.cpu_stages.contains(&"contrast".into()));
        assert!(compiled
            .arguments
            .iter()
            .any(|argument| argument == "-filter_complex"));
    }

    #[test]
    fn bounded_queue_drops_oldest_and_reports_count() {
        let queue = BoundedQueue::new(2).expect("queue");
        queue.push(1).expect("push");
        queue.push(2).expect("push");
        queue.push(3).expect("push");
        assert_eq!(queue.dropped().expect("dropped"), 1);
        assert_eq!(queue.pop().expect("pop"), Some(2));
        assert_eq!(queue.pop().expect("pop"), Some(3));
    }

    #[test]
    fn audio_master_clock_advances_from_rendered_frames() {
        let mut clock = PlaybackClock::new(48_000, Duration::from_secs(2)).expect("clock");
        clock.start();
        clock.audio_frames_rendered(24_000);
        let position = clock.position();
        assert!(position >= Duration::from_millis(2499));
        assert!(position <= Duration::from_millis(2501));
    }

    #[test]
    fn sync_controller_drops_late_and_holds_early_video() {
        let controller = AvSyncController::default();
        assert_eq!(
            controller.video_action(Duration::from_millis(900), Duration::from_secs(1)),
            SyncAction::DropVideo
        );
        assert!(matches!(
            controller.video_action(Duration::from_millis(1100), Duration::from_secs(1)),
            SyncAction::HoldVideo(_)
        ));
        assert_eq!(
            controller.video_action(Duration::from_secs(2), Duration::from_secs(1)),
            SyncAction::Discontinuity
        );
    }

    #[test]
    fn preview_frame_size_is_checked() {
        let configuration = PreviewConfiguration {
            source: PathBuf::from("missing"),
            width: 1920,
            height: 1080,
            frames_per_second: 30.0,
            sample_rate: 48_000,
            channels: 2,
            start_seconds: 0.0,
            video_queue_frames: 3,
            audio_queue_frames: 8,
        };
        assert_eq!(configuration.video_frame_bytes().expect("bytes"), 8_294_400);
    }
}
