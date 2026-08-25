//! Core batch media encoding and transcoding queue engine for Loom Encode.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Queued,
    Encoding { progress: f32 },
    Complete,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodePreset {
    pub name: String,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub bitrate_kbps: u32,
}

impl EncodePreset {
    pub fn h264_1080p() -> Self {
        Self {
            name: "H.264 Web 1080p".to_string(),
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            bitrate_kbps: 8000,
        }
    }

    pub fn prores_master() -> Self {
        Self {
            name: "ProRes 422 HQ Master".to_string(),
            container: "mov".to_string(),
            video_codec: "prores".to_string(),
            audio_codec: "pcm_s24le".to_string(),
            bitrate_kbps: 220000,
        }
    }

    pub fn hevc_4k() -> Self {
        Self {
            name: "HEVC / H.265 4K UHD".to_string(),
            container: "mp4".to_string(),
            video_codec: "hevc".to_string(),
            audio_codec: "aac".to_string(),
            bitrate_kbps: 20000,
        }
    }

    pub fn vp9_web() -> Self {
        Self {
            name: "VP9 WebM 1080p".to_string(),
            container: "webm".to_string(),
            video_codec: "vp9".to_string(),
            audio_codec: "opus".to_string(),
            bitrate_kbps: 6000,
        }
    }

    pub fn audio_flac() -> Self {
        Self {
            name: "FLAC Lossless Audio".to_string(),
            container: "flac".to_string(),
            video_codec: "none".to_string(),
            audio_codec: "flac".to_string(),
            bitrate_kbps: 0,
        }
    }

    pub fn audio_mp3() -> Self {
        Self {
            name: "MP3 Audio 320k".to_string(),
            container: "mp3".to_string(),
            video_codec: "none".to_string(),
            audio_codec: "mp3".to_string(),
            bitrate_kbps: 320,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeJob {
    pub id: String,
    pub source_file: String,
    pub output_file: String,
    pub preset: EncodePreset,
    pub status: JobStatus,
}

impl EncodeJob {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        output: impl Into<String>,
        preset: EncodePreset,
    ) -> Self {
        Self {
            id: id.into(),
            source_file: source.into(),
            output_file: output.into(),
            preset,
            status: JobStatus::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeQueue {
    pub id: String,
    pub name: String,
    pub jobs: Vec<EncodeJob>,
    #[serde(default)]
    pub active_job_index: usize,
}

impl EncodeQueue {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let mut q = Self {
            id: id.into(),
            name: name.into(),
            jobs: Vec::new(),
            active_job_index: 0,
        };
        q.jobs.push(EncodeJob::new(
            "job-1",
            "Master_Cut_01.mov",
            "Master_Cut_01_Web.mp4",
            EncodePreset::h264_1080p(),
        ));
        q
    }

    pub fn add_job(&mut self, job: EncodeJob) {
        self.jobs.push(job);
    }

    pub fn select_job(&mut self, index: usize) -> bool {
        if index < self.jobs.len() {
            self.active_job_index = index;
            true
        } else {
            false
        }
    }

    pub fn pending_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Queued))
            .count()
    }

    pub fn remove_job(&mut self, index: usize) -> Option<EncodeJob> {
        if index < self.jobs.len() {
            let job = self.jobs.remove(index);
            self.active_job_index = self.active_job_index.min(self.jobs.len().saturating_sub(1));
            Some(job)
        } else {
            None
        }
    }

    pub fn move_job(&mut self, from: usize, to: usize) -> bool {
        if from >= self.jobs.len() || to >= self.jobs.len() || from == to {
            return false;
        }
        let job = self.jobs.remove(from);
        self.jobs.insert(to, job);
        self.active_job_index = to;
        true
    }

    pub fn retry_failed_jobs(&mut self) -> usize {
        let mut retried = 0;
        for job in &mut self.jobs {
            if matches!(job.status, JobStatus::Failed(_)) {
                job.status = JobStatus::Queued;
                retried += 1;
            }
        }
        retried
    }

    pub fn clear_completed_jobs(&mut self) -> usize {
        let before = self.jobs.len();
        self.jobs
            .retain(|j| !matches!(j.status, JobStatus::Complete));
        self.active_job_index = self.active_job_index.min(self.jobs.len().saturating_sub(1));
        before - self.jobs.len()
    }

    pub fn add_multi_destination_batch(
        &mut self,
        source: impl Into<String>,
        base_output_path: &str,
        presets: &[EncodePreset],
    ) {
        let source = source.into();
        for (i, preset) in presets.iter().enumerate() {
            let id = format!("job-{}-{}", self.jobs.len() + 1, i + 1);
            let out_file = format!(
                "{}_{}.{}",
                base_output_path,
                preset.name.to_lowercase().replace(' ', "_"),
                preset.container
            );
            self.jobs
                .push(EncodeJob::new(id, source.clone(), out_file, preset.clone()));
        }
    }
}

/// Progress and throughput metrics for a running encoding job.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodeProgressMetrics {
    /// Fraction of frames encoded [0.0..=1.0].
    pub progress: f32,
    /// Current encoding throughput in frames per second.
    pub fps: f32,
    /// Estimated time remaining in seconds.
    pub eta_seconds: f32,
}

impl EncodeProgressMetrics {
    /// Estimates remaining time from encoded frames, total frames, and elapsed seconds.
    pub fn estimate(encoded_frames: u64, total_frames: u64, elapsed_seconds: f32) -> Self {
        if total_frames == 0 {
            return Self {
                progress: 1.0,
                fps: 0.0,
                eta_seconds: 0.0,
            };
        }
        let progress = (encoded_frames as f32 / total_frames as f32).clamp(0.0, 1.0);
        let fps = if elapsed_seconds > 1e-4 {
            encoded_frames as f32 / elapsed_seconds
        } else {
            0.0
        };
        let remaining_frames = total_frames.saturating_sub(encoded_frames);
        let eta_seconds = if fps > 1e-4 {
            remaining_frames as f32 / fps
        } else {
            0.0
        };
        Self {
            progress,
            fps,
            eta_seconds,
        }
    }
}

/// Expands output filename template patterns for batch encoding.
pub fn format_output_template(
    template: &str,
    source_name: &str,
    preset_name: &str,
    container_ext: &str,
) -> String {
    let clean_stem = std::path::Path::new(source_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(source_name);
    let mut result = template
        .replace("{name}", clean_stem)
        .replace("{preset}", preset_name)
        .replace("{ext}", container_ext.trim_start_matches('.'));
    if !result.contains('.') && !container_ext.is_empty() {
        result.push('.');
        result.push_str(container_ext.trim_start_matches('.'));
    }
    result
}

/// Calculates required video bitrate (in kbps) to fit within a target file size in megabytes.
pub fn calculate_target_bitrate_kbps(
    target_file_size_mb: f64,
    duration_secs: f64,
    audio_bitrate_kbps: u32,
) -> Result<u32, String> {
    if target_file_size_mb <= 0.0 || duration_secs <= 0.0 {
        return Err("target file size and duration must be positive".into());
    }
    let total_kilobits = target_file_size_mb * 8192.0;
    let total_bitrate_kbps = total_kilobits / duration_secs;
    let video_bitrate = total_bitrate_kbps - audio_bitrate_kbps as f64;
    if video_bitrate < 50.0 {
        return Err("target size too small for specified duration and audio bitrate".into());
    }
    Ok(video_bitrate.round() as u32)
}

/// Computes a simplified aspect ratio string (e.g. "16:9", "4:3", "1:1").
pub fn aspect_ratio_string(width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        return "0:0".into();
    }
    fn gcd(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let temp = b;
            b = a % b;
            a = temp;
        }
        a
    }
    let d = gcd(width, height);
    format!("{}:{}", width / d, height / d)
}

/// Generates FFmpeg arguments `(pass1_args, pass2_args)` for two-pass VBR video encoding.
pub fn generate_two_pass_args(
    source: &str,
    output: &str,
    pass_logfile: &str,
    target_bitrate_kbps: u32,
) -> (Vec<String>, Vec<String>) {
    let b_arg = format!("{}k", target_bitrate_kbps);
    let pass1 = vec![
        "-y".to_string(),
        "-i".to_string(),
        source.to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-b:v".to_string(),
        b_arg.clone(),
        "-pass".to_string(),
        "1".to_string(),
        "-passlogfile".to_string(),
        pass_logfile.to_string(),
        "-an".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "/dev/null".to_string(),
    ];

    let pass2 = vec![
        "-y".to_string(),
        "-i".to_string(),
        source.to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-b:v".to_string(),
        b_arg,
        "-pass".to_string(),
        "2".to_string(),
        "-passlogfile".to_string(),
        pass_logfile.to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "192k".to_string(),
        output.to_string(),
    ];

    (pass1, pass2)
}

/// Subtitle handling strategy for transcode pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SubtitleMode {
    #[default]
    None,
    BurnIn,
    PassthroughCopy,
    ConvertSrt,
}

/// Generates FFmpeg CLI arguments for subtitle mapping and processing.
pub fn generate_subtitle_args(mode: SubtitleMode, subtitle_path: Option<&str>) -> Vec<String> {
    match mode {
        SubtitleMode::None => Vec::new(),
        SubtitleMode::BurnIn => {
            if let Some(path) = subtitle_path {
                vec!["-vf".to_string(), format!("subtitles={}", path)]
            } else {
                Vec::new()
            }
        }
        SubtitleMode::PassthroughCopy => {
            vec!["-c:s".to_string(), "copy".to_string()]
        }
        SubtitleMode::ConvertSrt => {
            vec!["-c:s".to_string(), "mov_text".to_string()]
        }
    }
}

/// Generates FFmpeg video filter arguments for scaling and padding to fit a target resolution.
pub fn generate_scale_and_pad_args(
    src_w: u32,
    src_h: u32,
    target_w: u32,
    target_h: u32,
) -> Vec<String> {
    if src_w == target_w && src_h == target_h {
        return Vec::new();
    }
    let filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
        target_w, target_h, target_w, target_h
    );
    vec!["-vf".to_string(), filter]
}

/// Standard audio sample bit depths and encoding formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioSampleFormat {
    #[default]
    S16Le,
    S24Le,
    S32Le,
    F32Le,
}

impl AudioSampleFormat {
    /// Returns the FFmpeg `-sample_fmt` argument value.
    pub fn sample_fmt_str(&self) -> &'static str {
        match self {
            Self::S16Le => "s16",
            Self::S24Le => "s32", // ffmpeg PCM 24bit uses s32 container
            Self::S32Le => "s32",
            Self::F32Le => "flt",
        }
    }
}

/// Hardware-accelerated video codec profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HardwareEncoder {
    #[default]
    None,
    NvencH264,
    NvencHevc,
    VideoToolboxH264,
    VideoToolboxHevc,
    VaapiH264,
    VaapiHevc,
}

/// Generates video codec FFmpeg arguments for hardware-accelerated encoders.
pub fn generate_hardware_encoder_args(hw: HardwareEncoder) -> Vec<String> {
    match hw {
        HardwareEncoder::None => Vec::new(),
        HardwareEncoder::NvencH264 => vec!["-c:v".into(), "h264_nvenc".into()],
        HardwareEncoder::NvencHevc => vec!["-c:v".into(), "hevc_nvenc".into()],
        HardwareEncoder::VideoToolboxH264 => vec!["-c:v".into(), "h264_videotoolbox".into()],
        HardwareEncoder::VideoToolboxHevc => vec!["-c:v".into(), "hevc_videotoolbox".into()],
        HardwareEncoder::VaapiH264 => vec!["-c:v".into(), "h264_vaapi".into()],
        HardwareEncoder::VaapiHevc => vec!["-c:v".into(), "hevc_vaapi".into()],
    }
}

/// Parses `ffmpeg -encoders` output lines into available encoder names. The listing shows
/// lines like " A.... libx264              (codec h264)". Extract the encoder name field
/// (second whitespace-delimited token after the flag column); ignore header/blank lines.
pub fn parse_available_encoders(ffmpeg_encoders_output: &str) -> Vec<String> {
    ffmpeg_encoders_output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let flags = fields.next()?;
            // Flag columns such as "V....D" mix uppercase capability letters with dots;
            // anything else is a title, separator, or legend line.
            if flags.len() < 2 || !flags.chars().all(|c| c.is_ascii_uppercase() || c == '.') {
                return None;
            }
            let name = fields.next()?;
            // Legend lines such as " V..... = Video" put "=" in the name position.
            if !name.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

/// Produces the probe argument vector ["-hide_banner", "-encoders"] prefixed with "ffmpeg".
pub fn generate_encoder_probe_args() -> Vec<String> {
    vec!["ffmpeg".into(), "-hide_banner".into(), "-encoders".into()]
}

/// Chooses a hardware encoder from availability: returns the first preferred candidate that
/// appears in `available` (case-insensitive match against ffmpeg names like "h264_nvenc",
/// "hevc_videotoolbox", "h264_vaapi"); None when no preference is available. Empty
/// preferences => None.
pub fn select_hardware_encoder(preferred: &[String], available: &[String]) -> Option<String> {
    preferred.iter().find_map(|candidate| {
        available
            .iter()
            .find(|encoder| encoder.eq_ignore_ascii_case(candidate))
            .cloned()
    })
}

/// Stream mapping configuration for selecting specific media tracks from an input file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StreamMapping {
    pub video_track: Option<u32>,
    pub audio_track: Option<u32>,
    pub subtitle_track: Option<u32>,
}

impl StreamMapping {
    /// Generates FFmpeg `-map` arguments for explicit track mapping.
    pub fn generate_map_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(v) = self.video_track {
            args.push("-map".into());
            args.push(format!("0:v:{}", v));
        }
        if let Some(a) = self.audio_track {
            args.push("-map".into());
            args.push(format!("0:a:{}", a));
        }
        if let Some(s) = self.subtitle_track {
            args.push("-map".into());
            args.push(format!("0:s:{}", s));
        }
        args
    }
}

/// Structured video filter nodes for constructing FFmpeg `-vf` filter chains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoFilter {
    Scale { width: u32, height: u32 },
    Fps { fps: u32 },
    PixelFormat { format: String },
    Deinterlace,
    Custom { filter_expr: String },
}

impl VideoFilter {
    pub fn to_filter_string(&self) -> String {
        match self {
            Self::Scale { width, height } => format!("scale={}:{}", width, height),
            Self::Fps { fps } => format!("fps={}", fps),
            Self::PixelFormat { format } => format!("format={}", format),
            Self::Deinterlace => "yadif=0:-1:0".to_string(),
            Self::Custom { filter_expr } => filter_expr.clone(),
        }
    }
}

/// Sequence of video filters concatenated into a single filter graph.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FilterChain {
    pub filters: Vec<VideoFilter>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, filter: VideoFilter) {
        self.filters.push(filter);
    }

    /// Generates the `-vf <filtergraph>` arguments if filters are present.
    pub fn generate_args(&self) -> Vec<String> {
        if self.filters.is_empty() {
            return Vec::new();
        }
        let graph = self
            .filters
            .iter()
            .map(|f| f.to_filter_string())
            .collect::<Vec<String>>()
            .join(",");
        vec!["-vf".into(), graph]
    }
}

/// EBU R128 / ITU-R BS.1770 audio loudness normalization configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoudnessNormConfig {
    pub integrated_lufs: f32,
    pub true_peak_db: f32,
    pub loudness_range_lu: f32,
    pub enabled: bool,
}

impl Default for LoudnessNormConfig {
    fn default() -> Self {
        Self {
            integrated_lufs: -23.0,
            true_peak_db: -1.5,
            loudness_range_lu: 11.0,
            enabled: true,
        }
    }
}

impl LoudnessNormConfig {
    /// Generates FFmpeg `-af loudnorm=...` filter arguments if enabled.
    pub fn generate_loudnorm_args(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let filter_str = format!(
            "loudnorm=I={:.1}:TP={:.1}:LRA={:.1}",
            self.integrated_lufs, self.true_peak_db, self.loudness_range_lu
        );
        vec!["-af".into(), filter_str]
    }
}

/// Positioning anchor presets for visual watermarks and logo overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WatermarkPosition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

/// Visual logo / graphic watermark overlay filter configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatermarkOverlayConfig {
    pub image_path: String,
    pub position: WatermarkPosition,
    pub margin_px: u32,
    pub opacity: f32,
}

impl Default for WatermarkOverlayConfig {
    fn default() -> Self {
        Self {
            image_path: "logo.png".into(),
            position: WatermarkPosition::BottomRight,
            margin_px: 24,
            opacity: 0.85,
        }
    }
}

impl WatermarkOverlayConfig {
    /// Generates FFmpeg complex overlay filter expression.
    pub fn generate_overlay_expr(&self) -> String {
        let m = self.margin_px;
        match self.position {
            WatermarkPosition::TopLeft => format!("overlay={m}:{m}"),
            WatermarkPosition::TopRight => format!("overlay=main_w-overlay_w-{m}:{m}"),
            WatermarkPosition::BottomLeft => format!("overlay={m}:main_h-overlay_h-{m}"),
            WatermarkPosition::BottomRight => {
                format!("overlay=main_w-overlay_w-{m}:main_h-overlay_h-{m}")
            }
            WatermarkPosition::Center => {
                "overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2".to_string()
            }
        }
    }
}

/// Standard color primaries presets for broadcast and HDR video transcode profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorPrimaries {
    #[default]
    Bt709,
    Bt2020,
    Smpte170m,
    Smpte240m,
}

impl ColorPrimaries {
    pub fn as_ffmpeg_str(&self) -> &'static str {
        match self {
            Self::Bt709 => "bt709",
            Self::Bt2020 => "bt2020",
            Self::Smpte170m => "smpte170m",
            Self::Smpte240m => "smpte240m",
        }
    }
}

/// Electro-optical transfer characteristic functions (gamma curves and HDR transfer functions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorTransfer {
    #[default]
    Bt709,
    Smpte2084Hdr10,
    AribStdB67Hlg,
    Iec6196621Srgb,
    Linear,
}

impl ColorTransfer {
    pub fn as_ffmpeg_str(&self) -> &'static str {
        match self {
            Self::Bt709 => "bt709",
            Self::Smpte2084Hdr10 => "smpte2084",
            Self::AribStdB67Hlg => "arib-std-b67",
            Self::Iec6196621Srgb => "iec61966-2-1",
            Self::Linear => "linear",
        }
    }
}

/// Color matrix coefficients for YUV-to-RGB conversion equations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorMatrix {
    #[default]
    Bt709,
    Bt2020Nc,
    Smpte170m,
    Smpte240m,
}

impl ColorMatrix {
    pub fn as_ffmpeg_str(&self) -> &'static str {
        match self {
            Self::Bt709 => "bt709",
            Self::Bt2020Nc => "bt2020nc",
            Self::Smpte170m => "smpte170m",
            Self::Smpte240m => "smpte240m",
        }
    }
}

/// Color space metadata tagging for SDR and HDR delivery profiles.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ColorMetadataConfig {
    pub primaries: ColorPrimaries,
    pub transfer: ColorTransfer,
    pub matrix: ColorMatrix,
}

impl ColorMetadataConfig {
    /// Generates FFmpeg color metadata CLI argument flags.
    pub fn generate_color_metadata_args(&self) -> Vec<String> {
        vec![
            "-color_primaries".into(),
            self.primaries.as_ffmpeg_str().into(),
            "-color_trc".into(),
            self.transfer.as_ffmpeg_str().into(),
            "-colorspace".into(),
            self.matrix.as_ffmpeg_str().into(),
        ]
    }
}

/// SMPTE ST 2086 HDR10 mastering display color volume metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasteringDisplayColorVolume {
    pub red: (f64, f64),
    pub green: (f64, f64),
    pub blue: (f64, f64),
    pub white_point: (f64, f64),
    pub max_luminance_nits: u32,
    pub min_luminance_nits: f64,
}

impl Default for MasteringDisplayColorVolume {
    fn default() -> Self {
        Self {
            red: (0.708, 0.292),
            green: (0.170, 0.797),
            blue: (0.131, 0.046),
            white_point: (0.3127, 0.3290), // D65
            max_luminance_nits: 1000,
            min_luminance_nits: 0.0001,
        }
    }
}

impl MasteringDisplayColorVolume {
    /// Formats the x265 `master-display` parameter string.
    pub fn to_x265_display_str(&self) -> String {
        format!(
            "G({},{})B({},{})R({},{})WP({},{})L({},{})",
            (self.green.0 * 50000.0).round() as u32,
            (self.green.1 * 50000.0).round() as u32,
            (self.blue.0 * 50000.0).round() as u32,
            (self.blue.1 * 50000.0).round() as u32,
            (self.red.0 * 50000.0).round() as u32,
            (self.red.1 * 50000.0).round() as u32,
            (self.white_point.0 * 50000.0).round() as u32,
            (self.white_point.1 * 50000.0).round() as u32,
            self.max_luminance_nits * 10000,
            (self.min_luminance_nits * 10000.0).round() as u32
        )
    }
}

/// CTA-861.3 HDR10 content light level metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContentLightLevel {
    /// Maximum content light level in nits (MaxCLL).
    pub max_cll: u32,
    /// Maximum frame-average light level in nits (MaxFALL).
    pub max_fall: u32,
}

impl ContentLightLevel {
    /// Formats the x265 `max-cll` parameter string `MaxCLL,MaxFALL`.
    pub fn to_x265_cll_str(&self) -> String {
        format!("{},{}", self.max_cll, self.max_fall)
    }
}

/// Generates FFmpeg x265-params arguments for HDR10 static metadata.
pub fn generate_hdr10_x265_args(
    display: &MasteringDisplayColorVolume,
    cll: &ContentLightLevel,
) -> Vec<String> {
    vec![
        "-x265-params".into(),
        format!(
            "hdr10=1:hdr10-opt=1:master-display={}:max-cll={}",
            display.to_x265_display_str(),
            cll.to_x265_cll_str()
        ),
    ]
}

/// Dithering algorithms for color palette quantization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DitherMode {
    #[default]
    FloydSteinberg,
    Bayer,
    None,
}

/// Target format for animated image sequence export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnimatedFormat {
    #[default]
    Gif,
    Webp,
}

/// Animated GIF / WebP transcode configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimatedImageConfig {
    pub format: AnimatedFormat,
    pub fps: u32,
    pub loop_count: i32, // 0 = infinite loop
    pub dither: DitherMode,
    pub max_colors: u32,
}

impl Default for AnimatedImageConfig {
    fn default() -> Self {
        Self {
            format: AnimatedFormat::Gif,
            fps: 15,
            loop_count: 0,
            dither: DitherMode::FloydSteinberg,
            max_colors: 256,
        }
    }
}

impl AnimatedImageConfig {
    /// Generates FFmpeg complex filter and encoding arguments for high-quality palette-quantized animations.
    pub fn generate_animated_image_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        match self.format {
            AnimatedFormat::Gif => {
                let dither_str = match self.dither {
                    DitherMode::FloydSteinberg => "floyd_steinberg",
                    DitherMode::Bayer => "bayer",
                    DitherMode::None => "none",
                };
                let filter = format!(
                    "fps={},split[s0][s1];[s0]palettegen=max_colors={}[p];[s1][p]paletteuse=dither={}",
                    self.fps.max(1),
                    self.max_colors.clamp(2, 256),
                    dither_str
                );
                args.push("-vf".into());
                args.push(filter);
                args.push("-loop".into());
                args.push(self.loop_count.to_string());
            }
            AnimatedFormat::Webp => {
                args.push("-vcodec".into());
                args.push("libwebp".into());
                args.push("-filter:v".into());
                args.push(format!("fps={}", self.fps.max(1)));
                args.push("-loop".into());
                args.push(self.loop_count.to_string());
            }
        }
        args
    }
}

/// Standardized audio downmixing matrix profiles (e.g. ITU-R BS.775).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioDownmixMatrix {
    #[default]
    StereoToMono,
    Surround51ToStereo,
    Surround71ToStereo,
}

impl AudioDownmixMatrix {
    /// Generates FFmpeg pan / downmix audio filter arguments.
    pub fn generate_downmix_args(&self) -> Vec<String> {
        match self {
            AudioDownmixMatrix::StereoToMono => {
                vec!["-filter:a".into(), "pan=mono|c0=0.5*c0+0.5*c1".into()]
            }
            AudioDownmixMatrix::Surround51ToStereo => {
                // ITU-R BS.775 standard coefficients (FL + 0.707*FC + 0.707*BL)
                vec![
                    "-filter:a".into(),
                    "pan=stereo|FL=0.5*c0+0.3535*c2+0.3535*c4|FR=0.5*c1+0.3535*c2+0.3535*c5".into(),
                ]
            }
            AudioDownmixMatrix::Surround71ToStereo => {
                vec![
                    "-filter:a".into(),
                    "pan=stereo|FL=0.5*c0+0.3535*c2+0.3535*c4+0.3535*c6|FR=0.5*c1+0.3535*c2+0.3535*c5+0.3535*c7".into(),
                ]
            }
        }
    }
}

pub fn save_encode_queue(q: &EncodeQueue) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(q).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/queue.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Encode,
        id: q.id.clone(),
        title: q.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/queue.json".into(),
            mime: MimeType::parse("application/vnd.loom.encode-content")
                .map_err(|e| format!("invalid built-in encode MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_encode_queue(bytes: &[u8]) -> Result<EncodeQueue, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Encode {
        return Err("not an Encode queue".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/queue.json")
        .ok_or_else(|| "missing queue.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

/// External transcoder backend discovered on the local machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderBackend {
    /// Executable path.
    pub executable: std::path::PathBuf,
    /// First line of `ffmpeg -version` output.
    pub version: String,
}

/// Additional execution policy for an encode job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionPolicy {
    /// Replace an existing output file.
    pub overwrite: bool,
    /// Create the output parent directory when missing.
    pub create_parent_directories: bool,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            overwrite: false,
            create_parent_directories: true,
        }
    }
}

/// Fully resolved local encode invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodePlan {
    /// Executable path.
    pub executable: std::path::PathBuf,
    /// Command arguments, excluding the executable.
    pub arguments: Vec<String>,
    /// Input path.
    pub input: std::path::PathBuf,
    /// Output path.
    pub output: std::path::PathBuf,
    /// Whether execution may create the output parent directory.
    pub create_parent_directories: bool,
}

/// Error from planning or executing an encode job.
#[derive(Debug)]
pub enum EncodeError {
    /// Input or output configuration is invalid.
    InvalidJob(String),
    /// Encoder process could not be started or read.
    Io(std::io::Error),
    /// Encoder returned a non-zero status.
    ProcessFailed { code: Option<i32>, stderr: String },
    /// Encoding was cancelled by the user.
    Cancelled,
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::InvalidJob(message) => write!(f, "invalid encode job: {message}"),
            EncodeError::Io(error) => write!(f, "encoder I/O error: {error}"),
            EncodeError::ProcessFailed { code, stderr } => {
                write!(f, "encoder failed with status {code:?}: {stderr}")
            }
            EncodeError::Cancelled => write!(f, "encoding cancelled"),
        }
    }
}

impl std::error::Error for EncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EncodeError::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for EncodeError {
    fn from(value: std::io::Error) -> Self {
        EncodeError::Io(value)
    }
}

/// Locates and probes an FFmpeg executable without network access.
pub fn discover_ffmpeg(candidates: &[std::path::PathBuf]) -> Result<EncoderBackend, EncodeError> {
    let mut paths = candidates.to_vec();
    if paths.is_empty() {
        paths.push(std::path::PathBuf::from("ffmpeg"));
    }
    let mut last_error = None;
    for executable in paths {
        match std::process::Command::new(&executable)
            .arg("-version")
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let version = stdout.lines().next().unwrap_or("ffmpeg").to_string();
                return Ok(EncoderBackend {
                    executable,
                    version,
                });
            }
            Ok(output) => {
                last_error = Some(EncodeError::ProcessFailed {
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            Err(error) => last_error = Some(EncodeError::Io(error)),
        }
    }
    Err(last_error.unwrap_or_else(|| EncodeError::InvalidJob("no encoder candidates".into())))
}

impl EncodePreset {
    /// Resolves the user-facing video codec into an FFmpeg encoder name.
    pub fn ffmpeg_video_encoder(&self) -> Result<&'static str, EncodeError> {
        match self.video_codec.trim().to_ascii_lowercase().as_str() {
            "h264" | "avc" | "libx264" => Ok("libx264"),
            "h265" | "hevc" | "libx265" => Ok("libx265"),
            "vp9" | "libvpx-vp9" => Ok("libvpx-vp9"),
            "av1" | "libaom-av1" => Ok("libaom-av1"),
            "prores" | "prores_ks" => Ok("prores_ks"),
            "copy" => Ok("copy"),
            other => Err(EncodeError::InvalidJob(format!(
                "unsupported video codec {other:?}"
            ))),
        }
    }

    /// Resolves the audio codec into an FFmpeg encoder name.
    pub fn ffmpeg_audio_encoder(&self) -> Result<&'static str, EncodeError> {
        match self.audio_codec.trim().to_ascii_lowercase().as_str() {
            "aac" => Ok("aac"),
            "opus" | "libopus" => Ok("libopus"),
            "mp3" | "libmp3lame" => Ok("libmp3lame"),
            "flac" => Ok("flac"),
            "pcm_s16le" => Ok("pcm_s16le"),
            "pcm_s24le" => Ok("pcm_s24le"),
            "copy" => Ok("copy"),
            other => Err(EncodeError::InvalidJob(format!(
                "unsupported audio codec {other:?}"
            ))),
        }
    }
}

impl EncodeJob {
    /// Validates paths and preset values.
    pub fn validate(&self) -> Result<(), EncodeError> {
        if self.id.trim().is_empty() {
            return Err(EncodeError::InvalidJob("job id is empty".into()));
        }
        if self.source_file.trim().is_empty() || self.output_file.trim().is_empty() {
            return Err(EncodeError::InvalidJob(
                "source and output paths must be provided".into(),
            ));
        }
        if self.source_file == self.output_file {
            return Err(EncodeError::InvalidJob(
                "source and output paths must differ".into(),
            ));
        }
        if self.preset.bitrate_kbps == 0 && self.preset.video_codec != "copy" {
            return Err(EncodeError::InvalidJob(
                "video bitrate must be non-zero".into(),
            ));
        }
        self.preset.ffmpeg_video_encoder()?;
        self.preset.ffmpeg_audio_encoder()?;
        Ok(())
    }

    /// Builds a deterministic FFmpeg invocation.
    pub fn plan(
        &self,
        backend: &EncoderBackend,
        policy: ExecutionPolicy,
    ) -> Result<EncodePlan, EncodeError> {
        self.validate()?;
        let input = std::path::PathBuf::from(&self.source_file);
        let output = std::path::PathBuf::from(&self.output_file);
        if output.extension().and_then(|value| value.to_str())
            != Some(self.preset.container.as_str())
        {
            return Err(EncodeError::InvalidJob(format!(
                "output extension must match preset container .{}",
                self.preset.container
            )));
        }
        let mut arguments = vec![
            "-hide_banner".into(),
            "-nostdin".into(),
            if policy.overwrite {
                "-y".into()
            } else {
                "-n".into()
            },
            "-i".into(),
            input.to_string_lossy().into_owned(),
            "-map_metadata".into(),
            "0".into(),
            "-c:v".into(),
            self.preset.ffmpeg_video_encoder()?.into(),
            "-c:a".into(),
            self.preset.ffmpeg_audio_encoder()?.into(),
        ];
        if self.preset.ffmpeg_video_encoder()? != "copy" {
            arguments.extend(["-b:v".into(), format!("{}k", self.preset.bitrate_kbps)]);
        }
        arguments.extend([
            "-progress".into(),
            "pipe:1".into(),
            "-nostats".into(),
            output.to_string_lossy().into_owned(),
        ]);
        Ok(EncodePlan {
            executable: backend.executable.clone(),
            arguments,
            input,
            output,
            create_parent_directories: policy.create_parent_directories,
        })
    }
}

impl EncodeQueue {
    /// Returns the next queued job index.
    pub fn next_queued_index(&self) -> Option<usize> {
        self.jobs
            .iter()
            .position(|job| matches!(job.status, JobStatus::Queued))
    }

    /// Resets failed and in-progress jobs to queued state after a crash/restart.
    pub fn recover_interrupted(&mut self) -> usize {
        let mut reset = 0;
        for job in &mut self.jobs {
            if matches!(job.status, JobStatus::Encoding { .. }) {
                job.status = JobStatus::Queued;
                reset += 1;
            }
        }
        reset
    }

    /// Aggregate progress where queued jobs are 0 and completed jobs are 1.
    pub fn progress(&self) -> f32 {
        if self.jobs.is_empty() {
            return 0.0;
        }
        let sum: f32 = self
            .jobs
            .iter()
            .map(|job| match job.status {
                JobStatus::Queued | JobStatus::Failed(_) => 0.0,
                JobStatus::Encoding { progress } => progress.clamp(0.0, 1.0),
                JobStatus::Complete => 1.0,
            })
            .sum();
        sum / self.jobs.len() as f32
    }
}

/// Parses FFmpeg `-progress pipe:1` output.
#[derive(Debug, Clone)]
pub struct ProgressParser {
    duration_micros: Option<u64>,
    progress: f32,
}

impl ProgressParser {
    /// Creates a parser. Unknown duration reports 0 until `progress=end`.
    pub fn new(duration_secs: Option<f64>) -> Self {
        let duration_micros = duration_secs
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .map(|duration| (duration * 1_000_000.0).round() as u64);
        Self {
            duration_micros,
            progress: 0.0,
        }
    }

    /// Consumes one `key=value` line and returns updated progress when relevant.
    pub fn push_line(&mut self, line: &str) -> Option<f32> {
        let (key, value) = line.trim().split_once('=')?;
        match key {
            "out_time_us" | "out_time_ms" => {
                let micros: u64 = value.parse().ok()?;
                if let Some(duration) = self.duration_micros {
                    self.progress = (micros as f64 / duration as f64).clamp(0.0, 0.999) as f32;
                    Some(self.progress)
                } else {
                    None
                }
            }
            "progress" if value == "end" => {
                self.progress = 1.0;
                Some(1.0)
            }
            _ => None,
        }
    }

    /// Current parsed progress.
    pub fn progress(&self) -> f32 {
        self.progress
    }
}

/// Probes media duration using the FFprobe executable paired with FFmpeg.
pub fn probe_duration(
    backend: &EncoderBackend,
    input: &std::path::Path,
) -> Result<Option<f64>, EncodeError> {
    if !input.is_file() {
        return Err(EncodeError::InvalidJob(format!(
            "input does not exist: {}",
            input.display()
        )));
    }
    let sibling = backend.executable.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let executable = if sibling.is_file() {
        sibling
    } else {
        std::path::PathBuf::from(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        })
    };
    let output = std::process::Command::new(executable)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()?;
    if !output.status.success() {
        return Err(EncodeError::ProcessFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|duration| *duration > 0.0))
}

/// Executes one planned job and streams progress updates.
pub fn execute_job<F>(
    job: &mut EncodeJob,
    plan: &EncodePlan,
    duration_secs: Option<f64>,
    on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(f32),
{
    let cancel = std::sync::atomic::AtomicBool::new(false);
    execute_job_with_cancel(job, plan, duration_secs, &cancel, on_progress)
}

/// Executes one job with a cooperative cancellation signal.
pub fn execute_job_with_cancel<F>(
    job: &mut EncodeJob,
    plan: &EncodePlan,
    duration_secs: Option<f64>,
    cancel: &std::sync::atomic::AtomicBool,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(f32),
{
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    use std::thread;

    if !plan.input.is_file() {
        let error =
            EncodeError::InvalidJob(format!("input does not exist: {}", plan.input.display()));
        job.status = JobStatus::Failed(error.to_string());
        return Err(error);
    }
    if plan.output.exists() && plan.arguments.iter().any(|argument| argument == "-n") {
        let error =
            EncodeError::InvalidJob(format!("output already exists: {}", plan.output.display()));
        job.status = JobStatus::Failed(error.to_string());
        return Err(error);
    }
    if let Some(parent) = plan.output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            if plan.create_parent_directories {
                std::fs::create_dir_all(parent)?;
            } else {
                let error = EncodeError::InvalidJob(format!(
                    "output parent does not exist: {}",
                    parent.display()
                ));
                job.status = JobStatus::Failed(error.to_string());
                return Err(error);
            }
        }
    }
    let mut child = Command::new(&plan.executable)
        .args(&plan.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    job.status = JobStatus::Encoding { progress: 0.0 };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EncodeError::InvalidJob("encoder stdout was not captured".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| EncodeError::InvalidJob("encoder stderr was not captured".into()))?;
    let stderr_thread = thread::spawn(move || -> std::io::Result<String> {
        let mut output = String::new();
        stderr.take(16 * 1024 * 1024).read_to_string(&mut output)?;
        Ok(output)
    });
    let mut parser = ProgressParser::new(duration_secs);
    let mut cancelled = false;
    for line in BufReader::new(stdout).lines() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            let _ = child.kill();
            break;
        }
        let line = line?;
        if let Some(progress) = parser.push_line(&line) {
            job.status = JobStatus::Encoding { progress };
            on_progress(progress);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        cancelled = true;
        let _ = child.kill();
    }
    let status = child.wait()?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| EncodeError::InvalidJob("encoder stderr reader panicked".into()))??;
    if cancelled {
        job.status = JobStatus::Failed("Cancelled".into());
        return Err(EncodeError::Cancelled);
    }
    if status.success() {
        job.status = JobStatus::Complete;
        on_progress(1.0);
        Ok(())
    } else {
        let error = EncodeError::ProcessFailed {
            code: status.code(),
            stderr,
        };
        job.status = JobStatus::Failed(error.to_string());
        Err(error)
    }
}

/// Post-encode conformance checks evaluated against a finished output file.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ConformanceCheck {
    /// Full-stream decode integrity pass (ffmpeg null muxer with error termination).
    DecodeIntegrity,
    /// Verify measured duration stays within +/- tolerance seconds of expected.
    DurationTolerance {
        expected_seconds: f64,
        tolerance_seconds: f64,
    },
    /// Verify at least the given number of streams exist of any type.
    MinimumStreamCount { count: usize },
    /// Verify audio loudness integrated value does not exceed target LUFS by more than 1 LU.
    LoudnessCeiling { target_lufs: f64 },
}

/// Builds the ffprobe/ffmpeg argument vector that GATHERS evidence for the given checks against
/// `output`. DecodeIntegrity uses ffmpeg (`-v error -xerror -i <out> -f null -`); the rest are
/// ffprobe/loudnorm measurement invocations flattened into one deterministic Vec<String> where each
/// probe starts with its subcommand token ("ffprobe" or "ffmpeg").
pub fn generate_conformance_probe_args(output: &str, checks: &[ConformanceCheck]) -> Vec<String> {
    let mut probes = Vec::new();
    for check in checks {
        match *check {
            ConformanceCheck::DecodeIntegrity => {
                probes.push("ffmpeg".into());
                probes.extend([
                    "-v".into(),
                    "error".into(),
                    "-xerror".into(),
                    "-i".into(),
                    output.into(),
                    "-f".into(),
                    "null".into(),
                    "-".into(),
                ]);
            }
            ConformanceCheck::DurationTolerance { .. } => {
                probes.push("ffprobe".into());
                probes.extend([
                    "-v".into(),
                    "error".into(),
                    "-show_entries".into(),
                    "format=duration".into(),
                    "-of".into(),
                    "default=noprint_wrappers=1:nokey=1".into(),
                    output.into(),
                ]);
            }
            ConformanceCheck::MinimumStreamCount { .. } => {
                probes.push("ffprobe".into());
                probes.extend([
                    "-v".into(),
                    "error".into(),
                    "-show_entries".into(),
                    "stream=index".into(),
                    "-of".into(),
                    "csv=p=0".into(),
                    output.into(),
                ]);
            }
            ConformanceCheck::LoudnessCeiling { .. } => {
                probes.push("ffmpeg".into());
                probes.extend([
                    "-nostdin".into(),
                    "-i".into(),
                    output.into(),
                    "-vn".into(),
                    "-af".into(),
                    "loudnorm=print_format=json".into(),
                    "-f".into(),
                    "null".into(),
                    "-".into(),
                ]);
            }
        }
    }
    probes
}

/// Extracts the duration in seconds from raw `ffprobe -show_entries format=duration` default
/// output ("[FORMAT]\nduration=12.345000\n...[/FORMAT]") or from bare-value output
/// ("-of default=noprint_wrappers=1:nokey=1" giving just "12.345000"). Tolerates surrounding
/// whitespace/newlines. Err when no parsable finite non-negative value exists.
pub fn parse_probe_duration(probe_output: &str) -> Result<f64, String> {
    let trimmed = probe_output.trim();
    let raw_value = if let Some(block_start) = trimmed.find("[FORMAT]") {
        let body = &trimmed[block_start + "[FORMAT]".len()..];
        let body_end = body
            .find("[/FORMAT]")
            .ok_or_else(|| "ffprobe output has an unterminated [FORMAT] block".to_string())?;
        body[..body_end]
            .lines()
            .find_map(|line| line.trim().strip_prefix("duration="))
            .ok_or_else(|| "ffprobe [FORMAT] block contains no duration entry".to_string())?
    } else {
        trimmed
    };
    let duration: f64 = raw_value
        .trim()
        .parse()
        .map_err(|_| format!("cannot parse ffprobe duration from '{raw_value}'"))?;
    if !duration.is_finite() || duration < 0.0 {
        return Err(format!(
            "ffprobe duration '{raw_value}' is not finite and non-negative"
        ));
    }
    Ok(duration)
}

/// Evaluates a DurationTolerance-style check: |measured - expected| <= tolerance.
pub fn duration_within_tolerance(
    measured_seconds: f64,
    expected_seconds: f64,
    tolerance_seconds: f64,
) -> bool {
    (measured_seconds - expected_seconds).abs() <= tolerance_seconds
}

/// Counts stream lines from `ffprobe -show_entries stream=index` csv output
/// ("-of csv=p=0" prints one index per line). Blank lines ignored.
pub fn count_probe_streams(stream_output: &str) -> usize {
    stream_output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// A watched-folder ingestion rule: files appearing in `watch_path` whose extension matches
/// are queued for transcoding with the named destination preset after a stability delay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchFolderRule {
    /// Absolute or relative directory path to watch.
    pub watch_path: String,
    /// Lowercased file extensions to ingest WITHOUT the dot, e.g. ["mov", "mp4"].
    pub extensions: Vec<String>,
    /// Preset id applied to ingested files.
    pub preset_id: String,
    /// Output directory for completed transcodes; empty means alongside source.
    pub output_directory: Option<String>,
    /// Seconds a file must be unmodified before ingestion (size-stability check).
    pub stability_seconds: u32,
}

impl WatchFolderRule {
    /// Validates: non-empty path, at least one extension, every extension lowercase,
    /// dot-free, alphanumeric; stability_seconds <= 3600. Err names the violated rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.watch_path.trim().is_empty() {
            return Err("watch folder path must not be empty".to_string());
        }
        if self.extensions.is_empty() {
            return Err("watch folder rule must list at least one extension".to_string());
        }
        for extension in &self.extensions {
            if extension.is_empty()
                || !extension
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
            {
                return Err(format!(
                    "invalid watch folder extension '{extension}': entries must be lowercase, alphanumeric, and free of dots"
                ));
            }
        }
        if self.stability_seconds > 3600 {
            return Err(format!(
                "stability_seconds {} exceeds the maximum allowed 3600",
                self.stability_seconds
            ));
        }
        Ok(())
    }

    /// True when a filename would be picked up by this rule (case-insensitive extension match).
    pub fn matches_file(&self, file_name: &str) -> bool {
        if file_name.ends_with('/') || file_name.ends_with('\\') {
            return false;
        }
        let Some(extension) = std::path::Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
        else {
            return false;
        };
        self.extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    }
}

/// Resolves duplicate output filenames in a batch by appending " - 2", " - 3", ... before the
/// extension (order of first occurrence wins the original name). Returns the resolved list in
/// input order. Empty stems/names pass through unchanged. Case-insensitive collision detection
/// (A.MP4 vs a.mp4 collide). Paths may include directories; only the stem gets the suffix.
pub fn resolve_output_collisions(outputs: &[String]) -> Vec<String> {
    let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut resolved = Vec::with_capacity(outputs.len());
    for output in outputs {
        if output.trim().is_empty() {
            resolved.push(output.clone());
            continue;
        }
        let path = std::path::Path::new(output);
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(output.as_str());
        let extension = path.extension().and_then(|value| value.to_str());
        let mut attempt = 1;
        loop {
            let numbered_stem = if attempt == 1 {
                stem.to_string()
            } else {
                format!("{stem} - {attempt}")
            };
            let candidate_file_name = match extension {
                Some(extension) if !extension.is_empty() => {
                    format!("{numbered_stem}.{extension}")
                }
                _ => numbered_stem,
            };
            let candidate = match path.parent() {
                Some(directory) if !directory.as_os_str().is_empty() => {
                    directory.join(&candidate_file_name)
                }
                _ => std::path::PathBuf::from(&candidate_file_name),
            };
            let candidate = candidate.to_string_lossy().into_owned();
            if claimed.insert(candidate.to_lowercase()) {
                resolved.push(candidate);
                break;
            }
            attempt += 1;
        }
    }
    resolved
}

/// Sanitizes a filename for cross-platform safety: replaces characters invalid on common
/// filesystems (\ / : * ? " < > | and control chars) with '_', collapses whitespace runs,
/// trims leading/trailing dots and spaces, caps length at `max_len` bytes without splitting a
/// UTF-8 char boundary, preserving the extension when possible.
pub fn sanitize_output_filename(name: &str, max_len: usize) -> String {
    let is_forbidden = |character: char| {
        matches!(
            character,
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
        ) || character.is_control()
    };
    let mut cleaned = String::with_capacity(name.len());
    let mut previous_was_space = false;
    for character in name.chars() {
        if character.is_whitespace() {
            if !previous_was_space {
                cleaned.push(' ');
            }
            previous_was_space = true;
            continue;
        }
        previous_was_space = false;
        if is_forbidden(character) {
            cleaned.push('_');
        } else {
            cleaned.push(character);
        }
    }
    let trimmed = cleaned.trim_matches(['.', ' ']);
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }
    let trimmed_path = std::path::Path::new(trimmed);
    let stem = trimmed_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(trimmed);
    let Some(extension) = trimmed_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
    else {
        return truncate_on_char_boundary(stem, max_len).to_string();
    };
    let extension_bytes = extension.len() + 1;
    if extension_bytes >= max_len {
        return truncate_on_char_boundary(stem, max_len).to_string();
    }
    let stem_budget = max_len - extension_bytes;
    let truncated_stem = truncate_on_char_boundary(stem, stem_budget).trim_matches(['.', ' ']);
    format!("{truncated_stem}.{extension}")
}

/// Shortens `value` to at most `limit` bytes without splitting a UTF-8 character boundary.
fn truncate_on_char_boundary(value: &str, limit: usize) -> &str {
    if value.len() <= limit {
        return value;
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Estimates the encoded output size in bytes from component bitrates and duration.
/// Total kilobits = (video + audio kbps) * seconds; bytes = kilobits * 1000 / 8.
pub fn estimate_output_size_bytes(
    video_bitrate_kbps: u32,
    audio_bitrate_kbps: u32,
    duration_secs: f64,
) -> Result<u64, String> {
    if duration_secs <= 0.0 || !duration_secs.is_finite() {
        return Err("duration must be positive and finite".into());
    }
    let total_kilobits = (video_bitrate_kbps as f64 + audio_bitrate_kbps as f64) * duration_secs;
    Ok((total_kilobits * 1000.0 / 8.0).round() as u64)
}

/// Estimates free-space feasibility: true when `available_bytes` exceeds the estimate plus
/// the given safety margin fraction of it (e.g. 0.1 reserves 10% headroom).
pub fn has_room_for_output(
    estimated_bytes: u64,
    available_bytes: u64,
    margin_fraction: f64,
) -> Result<bool, String> {
    if !(0.0..=1.0).contains(&margin_fraction) {
        return Err("margin fraction must be within [0, 1]".into());
    }
    let required = (estimated_bytes as f64 * (1.0 + margin_fraction)).ceil() as u64;
    Ok(available_bytes >= required)
}

/// Retry policy for transient encode failures using exponential backoff.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    /// Maximum attempts including the first try (>= 1).
    pub max_attempts: u32,
    /// Delay in seconds before the first retry.
    pub initial_delay_seconds: f64,
    /// Multiplier applied to each successive delay (>= 1).
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_seconds: 2.0,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Validates the policy fields.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_attempts == 0 {
            return Err("max_attempts must be at least 1".into());
        }
        if self.initial_delay_seconds < 0.0 || !self.initial_delay_seconds.is_finite() {
            return Err("initial delay must be finite and non-negative".into());
        }
        if self.backoff_multiplier < 1.0 || !self.backoff_multiplier.is_finite() {
            return Err("backoff multiplier must be finite and at least 1".into());
        }
        Ok(())
    }

    /// Whether an attempt number (1-based) is allowed by this policy.
    pub fn allows_attempt(&self, attempt: u32) -> bool {
        attempt >= 1 && attempt <= self.max_attempts
    }

    /// Delay in seconds to wait before `attempt` (1-based): zero for the first attempt and
    /// exponentially compounded afterwards. Attempts beyond the policy yield None.
    pub fn delay_before_attempt(&self, attempt: u32) -> Option<f64> {
        if !self.allows_attempt(attempt) {
            return None;
        }
        if attempt == 1 {
            return Some(0.0);
        }
        let exponent = attempt - 2;
        Some(self.initial_delay_seconds * self.backoff_multiplier.powi(exponent as i32))
    }
}

/// The suite application submitting a job into the Encode queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SourceApp {
    #[default]
    Writer,
    Sheets,
    Present,
    Photo,
    Motion,
    Video,
    Studio,
}

/// A typed job submitted into the encode queue by another Loom application. The submitter
/// references its own project without mutating it; destination overrides live here only.
///
/// Validation contract: `label`, `preset_id`, and `project_path` must each be non-empty.
/// This model deliberately carries no inline media channel, so an empty `project_path`
/// is rejected rather than silently interpreted as "rendered bytes attached": an
/// application holding rendered bytes instead of a saved project must stage them to a
/// file first and submit that path.
#[derive(Debug, Clone, PartialEq)]
pub struct InboundJobRequest {
    pub source_app: SourceApp,
    /// Path of the exporting project document on disk. Must be non-empty; see the
    /// validation contract above for rendered-byte submissions.
    pub project_path: String,
    /// Preset id requested as the starting point.
    pub preset_id: String,
    /// Optional destination directory overriding the preset default.
    pub output_directory_override: Option<String>,
    /// Human-facing label shown in the queue (e.g. "Sequence 3 — Final").
    pub label: String,
}

impl InboundJobRequest {
    /// Validates the request per the validation contract on the struct.
    pub fn validate(&self) -> Result<(), String> {
        if self.label.trim().is_empty() {
            return Err("label must not be empty".into());
        }
        if self.preset_id.trim().is_empty() {
            return Err("preset id must not be empty".into());
        }
        if self.project_path.trim().is_empty() {
            return Err(
                "project path must not be empty; stage rendered bytes to a file and submit that path"
                    .into(),
            );
        }
        Ok(())
    }

    /// Resolves the effective output directory: an explicit override wins over the
    /// preset's default directory.
    pub fn effective_output_directory(&self, preset_default_directory: &str) -> String {
        self.output_directory_override
            .clone()
            .unwrap_or_else(|| preset_default_directory.to_string())
    }
}

/// Intake log entry recording a submission for queue audit history.
#[derive(Debug, Clone, PartialEq)]
pub struct IntakeRecord {
    pub received_at_unix_ms: u64,
    pub request: InboundJobRequest,
    pub accepted: bool,
    pub reason: String,
}

impl IntakeRecord {
    /// Records a submission by running `validate_request` against it. Acceptance stores a
    /// fixed confirmation reason and rejection stores the validator's error verbatim, so
    /// the same request and validator always produce the same record.
    pub fn new(
        received_at_unix_ms: u64,
        request: InboundJobRequest,
        validate_request: impl Fn(&InboundJobRequest) -> Result<(), String>,
    ) -> Self {
        match validate_request(&request) {
            Ok(()) => Self {
                received_at_unix_ms,
                request,
                accepted: true,
                reason: "accepted".to_string(),
            },
            Err(reason) => Self {
                received_at_unix_ms,
                request,
                accepted: false,
                reason,
            },
        }
    }
}

/// A version-tagged queue payload captured from disk.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedJobPayloadV1 {
    pub job_id: String,
    /// Legacy single destination path.
    pub output_path: String,
    pub preset_id: String,
}

/// Current-generation payload: destinations became a list.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedJobPayloadV2 {
    pub job_id: String,
    pub output_paths: Vec<String>,
    pub preset_id: String,
    /// Set during migration from V1 payloads that predate the field.
    pub migrated_from_v1: bool,
}

/// Migrates a V1 payload to V2. Empty job_id/preset_id err; empty output_path becomes a V2
/// payload with zero destinations (documented, allowed).
pub fn migrate_queue_payload_v1_to_v2(
    payload: &QueuedJobPayloadV1,
) -> Result<QueuedJobPayloadV2, String> {
    if payload.job_id.trim().is_empty() {
        return Err("job id must not be empty".into());
    }
    if payload.preset_id.trim().is_empty() {
        return Err("preset id must not be empty".into());
    }
    let output_paths = if payload.output_path.trim().is_empty() {
        Vec::new()
    } else {
        vec![payload.output_path.clone()]
    };
    Ok(QueuedJobPayloadV2 {
        job_id: payload.job_id.clone(),
        output_paths,
        preset_id: payload.preset_id.clone(),
        migrated_from_v1: true,
    })
}

/// Migrates a whole batch, reporting per-item failures as Err naming the offending index.
pub fn migrate_queue_batch_v1_to_v2(
    payloads: &[QueuedJobPayloadV1],
) -> Result<Vec<QueuedJobPayloadV2>, String> {
    let mut migrated = Vec::with_capacity(payloads.len());
    for (index, payload) in payloads.iter().enumerate() {
        match migrate_queue_payload_v1_to_v2(payload) {
            Ok(item) => migrated.push(item),
            Err(error) => return Err(format!("payload at index {index}: {error}")),
        }
    }
    Ok(migrated)
}

/// FNV-1a 64-bit hash over bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl EncodeQueue {
    /// Stable integrity digest of the queue's state: hashes queue identity and each
    /// job's id/state/preset/paths in order, so reordering, state transitions, and
    /// cleanup all change the digest. Uses [`fnv1a64`].
    pub fn queue_digest(&self) -> u64 {
        let mut input = format!("queue:{}\njobs:{}\n", self.id, self.jobs.len());
        for (index, job) in self.jobs.iter().enumerate() {
            let state = match &job.status {
                JobStatus::Queued => "queued".to_string(),
                JobStatus::Encoding { progress } => format!("encoding:{}", progress.to_bits()),
                JobStatus::Complete => "complete".to_string(),
                JobStatus::Failed(reason) => format!("failed:{reason}"),
            };
            input.push_str(&format!(
                "job:{index}:{}:{}:preset:{}:{}:{}:{}:{}kbps:src:{}:out:{}\n",
                job.id,
                state,
                job.preset.name,
                job.preset.container,
                job.preset.video_codec,
                job.preset.audio_codec,
                job.preset.bitrate_kbps,
                job.source_file,
                job.output_file
            ));
        }
        fnv1a64(input.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_creation() {
        let q = EncodeQueue::new("q-1", "Broadcast Delivery");
        assert_eq!(q.jobs.len(), 1);
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn queue_digest_stability() {
        let build = || {
            let mut queue = EncodeQueue::new("q-1", "Broadcast Delivery");
            queue.add_job(EncodeJob::new(
                "job-2",
                "Episode_01.mov",
                "Episode_01_Web.mp4",
                EncodePreset::hevc_4k(),
            ));
            queue.add_job(EncodeJob::new(
                "job-3",
                "Episode_02.mov",
                "Episode_02_Web.mp4",
                EncodePreset::vp9_web(),
            ));
            queue
        };
        let queue = build();
        let baseline = queue.queue_digest();

        // Stable across repeated calls and identical rebuilds.
        assert_eq!(baseline, queue.queue_digest());
        assert_eq!(baseline, build().queue_digest());

        // Job reorder changes the digest.
        let mut reordered = queue.clone();
        assert!(reordered.move_job(0, 2));
        assert_ne!(
            baseline,
            reordered.queue_digest(),
            "reordering jobs must change the queue digest"
        );

        // State transition changes the digest.
        let mut progressed = queue.clone();
        progressed.jobs[0].status = JobStatus::Encoding { progress: 0.5 };
        assert_ne!(
            baseline,
            progressed.queue_digest(),
            "a job state transition must change the queue digest"
        );

        // Completed-job cleanup changes the digest.
        let mut finished = queue;
        finished.jobs[1].status = JobStatus::Complete;
        let with_complete = finished.queue_digest();
        assert_ne!(baseline, with_complete);
        let removed = finished.clear_completed_jobs();
        assert_eq!(removed, 1);
        assert_ne!(
            with_complete,
            finished.queue_digest(),
            "clearing completed jobs must change the queue digest"
        );
    }

    #[test]
    fn test_select_job_rejects_invalid_index() {
        let mut q = EncodeQueue::new("q-1", "Broadcast Delivery");
        q.add_job(EncodeJob::new(
            "job-2",
            "Source.mov",
            "Output.mp4",
            EncodePreset::h264_1080p(),
        ));
        assert!(q.select_job(1));
        assert!(!q.select_job(2));
        assert_eq!(q.active_job_index, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut q = EncodeQueue::new("q-test", "Daily Dailies");
        q.add_job(EncodeJob::new(
            "j-2",
            "Day1_Take2.mov",
            "Day1_Take2_ProRes.mov",
            EncodePreset::prores_master(),
        ));
        let bytes = save_encode_queue(&q).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Encode);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_encode_queue(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Daily Dailies");
        assert_eq!(loaded.jobs.len(), 2);
    }

    #[test]
    fn ffmpeg_plan_is_deterministic_and_safe() {
        let job = EncodeJob::new("job", "input.mov", "output.mp4", EncodePreset::h264_1080p());
        let backend = EncoderBackend {
            executable: "/usr/bin/ffmpeg".into(),
            version: "ffmpeg version test".into(),
        };
        let plan = job.plan(&backend, ExecutionPolicy::default()).unwrap();
        assert_eq!(plan.arguments[2], "-n");
        assert!(plan
            .arguments
            .windows(2)
            .any(|pair| pair[0] == "-c:v" && pair[1] == "libx264"));
        assert_eq!(plan.arguments.last().unwrap(), "output.mp4");
    }

    #[test]
    fn invalid_encode_paths_and_extensions_are_rejected() {
        let same = EncodeJob::new("job", "same.mov", "same.mov", EncodePreset::h264_1080p());
        assert!(same.validate().is_err());
        let bad_extension =
            EncodeJob::new("job", "input.mov", "output.mkv", EncodePreset::h264_1080p());
        let backend = EncoderBackend {
            executable: "ffmpeg".into(),
            version: String::new(),
        };
        assert!(bad_extension
            .plan(&backend, ExecutionPolicy::default())
            .is_err());
    }

    #[test]
    fn progress_parser_and_queue_recovery_are_truthful() {
        let mut parser = ProgressParser::new(Some(10.0));
        assert_eq!(parser.push_line("out_time_us=5000000"), Some(0.5));
        assert_eq!(parser.push_line("progress=end"), Some(1.0));
        let mut queue = EncodeQueue::new("queue", "Queue");
        queue.jobs[0].status = JobStatus::Encoding { progress: 0.4 };
        assert_eq!(queue.progress(), 0.4);
        assert_eq!(queue.recover_interrupted(), 1);
        assert!(matches!(queue.jobs[0].status, JobStatus::Queued));
    }

    #[test]
    fn cancellation_error_is_explicit() {
        assert_eq!(EncodeError::Cancelled.to_string(), "encoding cancelled");
    }

    #[test]
    fn queue_batch_reorder_and_retry_operations() {
        let mut queue = EncodeQueue::new("batch-q", "Batch Queue");
        queue.jobs.clear();
        let presets = vec![EncodePreset::h264_1080p(), EncodePreset::prores_master()];
        queue.add_multi_destination_batch("source.mov", "/export/master", &presets);
        assert_eq!(queue.jobs.len(), 2);
        assert_eq!(queue.pending_count(), 2);

        // Reorder
        assert!(queue.move_job(0, 1));
        assert_eq!(queue.active_job_index, 1);

        // Fail and retry
        queue.jobs[0].status = JobStatus::Failed("timeout".into());
        queue.jobs[1].status = JobStatus::Complete;
        assert_eq!(queue.pending_count(), 0);

        assert_eq!(queue.retry_failed_jobs(), 1);
        assert_eq!(queue.pending_count(), 1);

        // Clear completed
        assert_eq!(queue.clear_completed_jobs(), 1);
        assert_eq!(queue.jobs.len(), 1);
    }

    #[test]
    fn preset_definitions_are_valid() {
        let hevc = EncodePreset::hevc_4k();
        assert_eq!(hevc.video_codec, "hevc");
        assert_eq!(hevc.bitrate_kbps, 20000);

        let vp9 = EncodePreset::vp9_web();
        assert_eq!(vp9.container, "webm");
        assert_eq!(vp9.audio_codec, "opus");

        let flac = EncodePreset::audio_flac();
        assert_eq!(flac.video_codec, "none");
        assert_eq!(flac.container, "flac");

        let mp3 = EncodePreset::audio_mp3();
        assert_eq!(mp3.bitrate_kbps, 320);
    }

    #[test]
    fn progress_metrics_throughput_and_eta_estimation() {
        // 500 out of 1000 frames in 10 seconds -> 50 fps, ETA = 10s
        let m = EncodeProgressMetrics::estimate(500, 1000, 10.0);
        assert!((m.progress - 0.5).abs() < 1e-4);
        assert!((m.fps - 50.0).abs() < 1e-4);
        assert!((m.eta_seconds - 10.0).abs() < 1e-4);

        // Finished job
        let m_done = EncodeProgressMetrics::estimate(1000, 1000, 20.0);
        assert!((m_done.progress - 1.0).abs() < 1e-4);
        assert_eq!(m_done.eta_seconds, 0.0);
    }

    #[test]
    fn output_template_interpolation() {
        let out = format_output_template(
            "{name}_{preset}.{ext}",
            "/Users/projects/Promo_Final.mov",
            "1080p_h264",
            "mp4",
        );
        assert_eq!(out, "Promo_Final_1080p_h264.mp4");

        // Without extension placeholder
        let out2 = format_output_template("{name}_converted", "Clip1.mkv", "ProRes", "mov");
        assert_eq!(out2, "Clip1_converted.mov");
    }

    #[test]
    fn collision_resolution_and_sanitization() {
        // First occurrence keeps the name; later duplicates count across case-insensitive
        // matches ("MOVIE.mp4" collides with both prior entries, so it becomes " - 3").
        assert_eq!(
            resolve_output_collisions(&[
                "movie.mp4".to_string(),
                "movie.mp4".to_string(),
                "MOVIE.mp4".to_string(),
                "other.mp4".to_string(),
            ]),
            vec![
                "movie.mp4".to_string(),
                "movie - 2.mp4".to_string(),
                "MOVIE - 3.mp4".to_string(),
                "other.mp4".to_string(),
            ]
        );

        // Directories are preserved; only the stem receives a suffix.
        assert_eq!(
            resolve_output_collisions(&[
                "exports/clip.mkv".to_string(),
                "exports/clip.mkv".to_string(),
            ]),
            vec![
                "exports/clip.mkv".to_string(),
                "exports/clip - 2.mkv".to_string(),
            ]
        );

        // Extension-less names still collide and resolve.
        assert_eq!(
            resolve_output_collisions(&["archive".to_string(), "archive".to_string()]),
            vec!["archive".to_string(), "archive - 2".to_string()]
        );

        // Empty names pass through unchanged.
        assert_eq!(resolve_output_collisions(&[]), Vec::<String>::new());
        assert_eq!(
            resolve_output_collisions(&[String::new(), String::new()]),
            vec![String::new(), String::new()]
        );

        // Illegal filesystem characters become underscores; control characters too.
        assert_eq!(
            sanitize_output_filename("a<b>:c*d?e\"f|g", 255),
            "a_b__c_d_e_f_g"
        );
        assert_eq!(sanitize_output_filename("a\u{0}b", 16), "a_b");

        // Whitespace runs collapse; leading/trailing dots and spaces are trimmed.
        assert_eq!(
            sanitize_output_filename("  ..my  report .final..  ", 255),
            "my report .final"
        );

        // Unicode letters survive; truncation keeps whole characters and preserves ".png".
        assert_eq!(sanitize_output_filename("héllo wörld.png", 10), "héllo.png");
        assert_eq!(
            sanitize_output_filename("verylongfilename.mp4", 12),
            "verylong.mp4"
        );
    }

    #[test]
    fn bitrate_calculation_and_aspect_ratios() {
        // Target: 100 MB, 60 seconds duration, 192 kbps audio -> video bitrate ~ 13,461 kbps
        let v_br = calculate_target_bitrate_kbps(100.0, 60.0, 192).unwrap();
        assert!(v_br > 13000 && v_br < 14000);

        // Aspect ratios
        assert_eq!(aspect_ratio_string(1920, 1080), "16:9");
        assert_eq!(aspect_ratio_string(1080, 1080), "1:1");
        assert_eq!(aspect_ratio_string(1440, 1080), "4:3");
        assert_eq!(aspect_ratio_string(0, 1080), "0:0");
    }

    #[test]
    fn two_pass_encoding_args() {
        let (pass1, pass2) =
            generate_two_pass_args("input.mov", "output.mp4", "/tmp/ffmpeg2pass", 5000);

        assert!(pass1.contains(&"-pass".to_string()));
        assert!(pass1.contains(&"1".to_string()));
        assert!(pass1.contains(&"-an".to_string()));
        assert!(pass1.contains(&"5000k".to_string()));

        assert!(pass2.contains(&"-pass".to_string()));
        assert!(pass2.contains(&"2".to_string()));
        assert!(pass2.contains(&"-c:a".to_string()));
        assert!(pass2.contains(&"output.mp4".to_string()));
    }

    #[test]
    fn subtitle_arguments_generation() {
        assert!(generate_subtitle_args(SubtitleMode::None, None).is_empty());

        let burn_args = generate_subtitle_args(SubtitleMode::BurnIn, Some("/path/to/subs.srt"));
        assert_eq!(burn_args, vec!["-vf", "subtitles=/path/to/subs.srt"]);

        let copy_args = generate_subtitle_args(SubtitleMode::PassthroughCopy, None);
        assert_eq!(copy_args, vec!["-c:s", "copy"]);

        let conv_args = generate_subtitle_args(SubtitleMode::ConvertSrt, None);
        assert_eq!(conv_args, vec!["-c:s", "mov_text"]);
    }

    #[test]
    fn scale_and_pad_filter_generation() {
        assert!(generate_scale_and_pad_args(1920, 1080, 1920, 1080).is_empty());

        let pad_args = generate_scale_and_pad_args(1440, 1080, 1920, 1080);
        assert_eq!(
            pad_args,
            vec![
                "-vf",
                "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2"
            ]
        );

        assert_eq!(AudioSampleFormat::S16Le.sample_fmt_str(), "s16");
        assert_eq!(AudioSampleFormat::F32Le.sample_fmt_str(), "flt");
    }

    #[test]
    fn hardware_encoder_and_stream_mapping_args() {
        assert!(generate_hardware_encoder_args(HardwareEncoder::None).is_empty());
        assert_eq!(
            generate_hardware_encoder_args(HardwareEncoder::NvencH264),
            vec!["-c:v", "h264_nvenc"]
        );
        assert_eq!(
            generate_hardware_encoder_args(HardwareEncoder::VideoToolboxHevc),
            vec!["-c:v", "hevc_videotoolbox"]
        );

        let map = StreamMapping {
            video_track: Some(0),
            audio_track: Some(1),
            subtitle_track: Some(0),
        };
        let args = map.generate_map_args();
        assert_eq!(
            args,
            vec!["-map", "0:v:0", "-map", "0:a:1", "-map", "0:s:0"]
        );
    }

    #[test]
    fn encoder_probe_parsing_and_selection() {
        let sample = "\
Encoders:
 V..... = Video
 A..... = Audio
 -------
 V....D libx264              (codec h264)
 V..... h264_nvenc           NVIDIA NVENC H.264 encoder
 A....D aac                  AAC (Advanced Audio Coding)

";
        let parsed = parse_available_encoders(sample);
        assert_eq!(parsed.len(), 3);
        assert!(parsed.iter().any(|name| name == "libx264"));
        assert!(parsed.iter().any(|name| name == "h264_nvenc"));
        assert!(parsed.iter().any(|name| name == "aac"));

        assert_eq!(
            generate_encoder_probe_args(),
            vec!["ffmpeg", "-hide_banner", "-encoders"]
        );

        let available = vec![
            "h264_videotoolbox".to_string(),
            "hevc_videotoolbox".to_string(),
        ];
        let preferences = vec!["h264_nvenc".to_string(), "H264_VIDEOTOOLBOX".to_string()];
        assert_eq!(
            select_hardware_encoder(&preferences, &available),
            Some("h264_videotoolbox".to_string())
        );
        assert_eq!(
            select_hardware_encoder(&["h264_vaapi".to_string()], &available),
            None
        );
        assert_eq!(select_hardware_encoder(&[], &available), None);
    }

    #[test]
    fn filter_chain_argument_generation() {
        let mut chain = FilterChain::new();
        assert!(chain.generate_args().is_empty());

        chain.add(VideoFilter::Scale {
            width: 1280,
            height: 720,
        });
        chain.add(VideoFilter::Fps { fps: 30 });
        chain.add(VideoFilter::PixelFormat {
            format: "yuv420p".into(),
        });

        let args = chain.generate_args();
        assert_eq!(args, vec!["-vf", "scale=1280:720,fps=30,format=yuv420p"]);
    }

    #[test]
    fn loudness_norm_argument_generation() {
        let lnorm = LoudnessNormConfig::default();
        let args = lnorm.generate_loudnorm_args();
        assert_eq!(args, vec!["-af", "loudnorm=I=-23.0:TP=-1.5:LRA=11.0"]);

        let disabled = LoudnessNormConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(disabled.generate_loudnorm_args().is_empty());
    }

    #[test]
    fn watermark_overlay_filter_expression() {
        let mut overlay = WatermarkOverlayConfig {
            margin_px: 16,
            position: WatermarkPosition::TopLeft,
            ..Default::default()
        };
        assert_eq!(overlay.generate_overlay_expr(), "overlay=16:16");

        overlay.position = WatermarkPosition::BottomRight;
        assert_eq!(
            overlay.generate_overlay_expr(),
            "overlay=main_w-overlay_w-16:main_h-overlay_h-16"
        );

        overlay.position = WatermarkPosition::Center;
        assert_eq!(
            overlay.generate_overlay_expr(),
            "overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2"
        );
    }

    #[test]
    fn color_metadata_argument_generation() {
        let color = ColorMetadataConfig {
            primaries: ColorPrimaries::Bt2020,
            transfer: ColorTransfer::Smpte2084Hdr10,
            matrix: ColorMatrix::Bt2020Nc,
        };

        let args = color.generate_color_metadata_args();
        assert_eq!(
            args,
            vec![
                "-color_primaries",
                "bt2020",
                "-color_trc",
                "smpte2084",
                "-colorspace",
                "bt2020nc"
            ]
        );
    }

    #[test]
    fn hdr10_metadata_argument_generation() {
        let display = MasteringDisplayColorVolume::default();
        let cll = ContentLightLevel {
            max_cll: 1000,
            max_fall: 400,
        };

        let args = generate_hdr10_x265_args(&display, &cll);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-x265-params");
        assert!(args[1].contains("hdr10=1"));
        assert!(args[1].contains("max-cll=1000,400"));
        assert!(args[1].contains("master-display="));
    }

    #[test]
    fn animated_image_gif_webp_arguments() {
        let gif_cfg = AnimatedImageConfig {
            format: AnimatedFormat::Gif,
            fps: 20,
            loop_count: 0,
            dither: DitherMode::FloydSteinberg,
            max_colors: 128,
        };

        let gif_args = gif_cfg.generate_animated_image_args();
        assert_eq!(gif_args[0], "-vf");
        assert!(gif_args[1].contains("fps=20"));
        assert!(gif_args[1].contains("palettegen=max_colors=128"));
        assert!(gif_args[1].contains("paletteuse=dither=floyd_steinberg"));
        assert_eq!(gif_args[2], "-loop");
        assert_eq!(gif_args[3], "0");

        let webp_cfg = AnimatedImageConfig {
            format: AnimatedFormat::Webp,
            fps: 24,
            loop_count: 3,
            dither: DitherMode::None,
            max_colors: 256,
        };

        let webp_args = webp_cfg.generate_animated_image_args();
        assert_eq!(webp_args[0], "-vcodec");
        assert_eq!(webp_args[1], "libwebp");
        assert_eq!(webp_args[2], "-filter:v");
        assert_eq!(webp_args[3], "fps=24");
        assert_eq!(webp_args[4], "-loop");
        assert_eq!(webp_args[5], "3");
    }

    #[test]
    fn audio_downmix_arguments() {
        let stereo_to_mono = AudioDownmixMatrix::StereoToMono;
        let mono_args = stereo_to_mono.generate_downmix_args();
        assert_eq!(mono_args[0], "-filter:a");
        assert!(mono_args[1].contains("pan=mono"));

        let surround_51 = AudioDownmixMatrix::Surround51ToStereo;
        let s51_args = surround_51.generate_downmix_args();
        assert_eq!(s51_args[0], "-filter:a");
        assert!(s51_args[1].contains("pan=stereo"));
        assert!(s51_args[1].contains("FL="));
        assert!(s51_args[1].contains("FR="));
    }

    #[test]
    fn conformance_probe_args_generation() {
        let checks = [
            ConformanceCheck::DecodeIntegrity,
            ConformanceCheck::DurationTolerance {
                expected_seconds: 90.0,
                tolerance_seconds: 0.5,
            },
            ConformanceCheck::MinimumStreamCount { count: 2 },
            ConformanceCheck::LoudnessCeiling { target_lufs: -16.0 },
        ];

        let args = generate_conformance_probe_args("Master_Cut_01_Web.mp4", &checks);

        // Each probe begins with its subcommand token; split the flat vector on those markers.
        let starts: Vec<usize> = args
            .iter()
            .enumerate()
            .filter_map(|(index, token)| {
                if token == "ffmpeg" || token == "ffprobe" {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(starts.len(), checks.len());

        let mut probes: Vec<Vec<&str>> = Vec::new();
        for (position, &start) in starts.iter().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(args.len());
            probes.push(args[start..end].iter().map(String::as_str).collect());
        }

        // Ordering matches slice order.
        let markers: Vec<&str> = probes.iter().map(|probe| probe[0]).collect();
        assert_eq!(markers, vec!["ffmpeg", "ffprobe", "ffprobe", "ffmpeg"]);

        // DecodeIntegrity: ffmpeg with error termination and null muxer.
        assert_eq!(
            probes[0],
            vec![
                "ffmpeg",
                "-v",
                "error",
                "-xerror",
                "-i",
                "Master_Cut_01_Web.mp4",
                "-f",
                "null",
                "-"
            ]
        );

        // Duration evidence via ffprobe format duration.
        assert_eq!(
            probes[1],
            vec![
                "ffprobe",
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                "Master_Cut_01_Web.mp4"
            ]
        );

        // Stream count evidence via ffprobe stream index listing.
        assert_eq!(
            probes[2],
            vec![
                "ffprobe",
                "-v",
                "error",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
                "Master_Cut_01_Web.mp4"
            ]
        );

        // Loudness evidence via a loudnorm JSON measurement pass.
        assert_eq!(
            probes[3],
            vec![
                "ffmpeg",
                "-nostdin",
                "-i",
                "Master_Cut_01_Web.mp4",
                "-vn",
                "-af",
                "loudnorm=print_format=json",
                "-f",
                "null",
                "-"
            ]
        );

        assert!(generate_conformance_probe_args("unused.mp4", &[]).is_empty());
    }

    #[test]
    fn probe_output_parsing_conformance() {
        // Bracketed default-writer FORMAT block with the duration entry mid-section.
        let bracketed = "[FORMAT]\nfilename=out.mp4\nduration=12.345000\nsize=1048576\n[/FORMAT]";
        assert_eq!(parse_probe_duration(bracketed).unwrap(), 12.345);

        // Bare noprint_wrappers=nokey value
        assert_eq!(parse_probe_duration("12.345000").unwrap(), 12.345);

        // Surrounding whitespace and newlines are tolerated in both shapes
        assert_eq!(parse_probe_duration("\n \n12.345000\n\n ").unwrap(), 12.345);
        assert_eq!(
            parse_probe_duration("\n\n[FORMAT]\nduration=12.345000\n[/FORMAT]\n").unwrap(),
            12.345
        );

        // Zero is a valid finite non-negative duration
        assert_eq!(parse_probe_duration("0").unwrap(), 0.0);

        // Garbage, empty output, negatives, and non-finite values all err
        assert!(parse_probe_duration("not_a_number").is_err());
        assert!(parse_probe_duration("").is_err());
        assert!(parse_probe_duration("-3.5").is_err());
        assert!(parse_probe_duration("NaN").is_err());

        // Tolerance boundary: |measured - expected| <= tolerance, inclusive edge
        assert!(duration_within_tolerance(90.5, 90.0, 0.5));
        assert!(!duration_within_tolerance(90.5001, 90.0, 0.5));

        // Stream counting ignores blank lines and handles empty output
        assert_eq!(count_probe_streams("0\n1\n2\n\n"), 3);
        assert_eq!(count_probe_streams(""), 0);
    }

    #[test]
    fn watch_folder_rule_matching_and_validation() {
        let rule = WatchFolderRule {
            watch_path: "/imports/camera".into(),
            extensions: vec!["mov".into(), "mp4".into()],
            preset_id: "h264_1080p".into(),
            output_directory: Some("/exports/web".into()),
            stability_seconds: 30,
        };
        assert!(rule.validate().is_ok());

        // Case-insensitive extension matching, including multi-dot names.
        assert!(rule.matches_file("A.MOV"));
        assert!(rule.matches_file("b.mp4"));
        assert!(rule.matches_file("clip.v2.mov"));

        // Non-matching extensions, extension-less names, and directory-like paths.
        assert!(!rule.matches_file("c.avi"));
        assert!(!rule.matches_file("no_extension"));
        assert!(!rule.matches_file("drop.mov/"));

        let mut broken = rule.clone();
        broken.watch_path = String::new();
        assert!(broken.validate().is_err());

        let mut broken = rule.clone();
        broken.extensions = Vec::new();
        assert!(broken.validate().is_err());

        let mut broken = rule.clone();
        broken.extensions = vec!["MP4".into()];
        assert!(broken.validate().is_err());

        let mut broken = rule.clone();
        broken.extensions = vec![".mov".into()];
        assert!(broken.validate().is_err());

        let mut broken = rule.clone();
        broken.stability_seconds = 4000;
        assert!(broken.validate().is_err());
    }

    #[test]
    fn output_size_estimation_and_disk_feasibility() {
        // 8 Mbps video + 192 kbps audio over 60 s = 8192 kbit/s * 60 = 491,520 kbits
        // = 61,440,000 bytes exactly
        let size = estimate_output_size_bytes(8000, 192, 60.0).unwrap();
        assert_eq!(size, 61_440_000);

        // Video-only second: 5128 kbps * 1 s = 5,128,000 bits = 641,000 bytes
        assert_eq!(estimate_output_size_bytes(5128, 0, 1.0).unwrap(), 641_000);
        // Tiny slice: 5128 kbps * 0.0001 s rounds down to 64 bytes
        assert_eq!(estimate_output_size_bytes(5128, 0, 0.0001).unwrap(), 64);
        // Rounding to whole bytes: 1 kbps * 1 s = 1000 bits = 125 bytes
        assert_eq!(estimate_output_size_bytes(1, 0, 1.0).unwrap(), 125);

        assert!(estimate_output_size_bytes(5000, 128, 0.0).is_err());
        assert!(estimate_output_size_bytes(5000, 128, -1.0).is_err());
        assert!(estimate_output_size_bytes(5000, 128, f64::NAN).is_err());

        // Feasibility with a 10% margin
        assert!(has_room_for_output(10_000, 11_000, 0.1).unwrap());
        assert!(!has_room_for_output(10_000, 10_999, 0.1).unwrap());

        // Zero margin requires the exact estimate
        assert!(has_room_for_output(10_000, 10_000, 0.0).unwrap());
        assert!(!has_room_for_output(10_000, 9_999, 0.0).unwrap());

        // Invalid margins are rejected rather than silently applied
        assert!(has_room_for_output(1, 1, -0.5).is_err());
        assert!(has_room_for_output(1, 1, 1.5).is_err());
    }

    #[test]
    fn retry_policy_backoff_schedule() {
        // Default policy: 3 attempts, 2 s initial delay, doubling backoff
        let policy = RetryPolicy::default();
        assert!(policy.validate().is_ok());

        assert_eq!(policy.delay_before_attempt(1), Some(0.0));
        assert_eq!(policy.delay_before_attempt(2), Some(2.0));
        assert_eq!(policy.delay_before_attempt(3), Some(4.0));
        assert_eq!(policy.delay_before_attempt(4), None);
        assert_eq!(policy.delay_before_attempt(0), None);

        assert!(policy.allows_attempt(1));
        assert!(policy.allows_attempt(3));
        assert!(!policy.allows_attempt(4));
        assert!(!policy.allows_attempt(0));

        // A single-attempt policy never retries
        let once = RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        };
        assert_eq!(once.delay_before_attempt(2), None);

        // Validation rejects bad fields
        assert!(RetryPolicy {
            max_attempts: 0,
            ..RetryPolicy::default()
        }
        .validate()
        .is_err());
        assert!(RetryPolicy {
            initial_delay_seconds: -1.0,
            ..RetryPolicy::default()
        }
        .validate()
        .is_err());
        assert!(RetryPolicy {
            backoff_multiplier: 0.5,
            ..RetryPolicy::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn inbound_job_intake_and_overrides() {
        let valid = InboundJobRequest {
            source_app: SourceApp::Video,
            project_path: "/projects/sequence_3.loomvid".into(),
            preset_id: "h264-1080p".into(),
            output_directory_override: None,
            label: "Sequence 3 — Final".into(),
        };
        assert!(valid.validate().is_ok());

        // Empty labels are rejected
        let mut no_label = valid.clone();
        no_label.label = String::new();
        assert!(no_label.validate().is_err());
        // So are empty preset ids and empty project paths
        let mut no_preset = valid.clone();
        no_preset.preset_id = "  ".into();
        assert!(no_preset.validate().is_err());
        let mut no_path = valid.clone();
        no_path.project_path = String::new();
        assert!(no_path.validate().is_err());

        // An explicit override wins over the preset's default directory
        let mut overridden = valid.clone();
        overridden.output_directory_override = Some("/exports/final".into());
        assert_eq!(
            overridden.effective_output_directory("/exports/default"),
            "/exports/final"
        );

        // Without an override the preset default applies
        assert_eq!(
            valid.effective_output_directory("/exports/default"),
            "/exports/default"
        );

        // Intake records apply the supplied validator deterministically: a valid
        // request is accepted with a fixed reason
        let accepted = IntakeRecord::new(1_700_000_000_000, valid.clone(), |request| {
            request.validate()
        });
        assert!(accepted.accepted);
        assert_eq!(accepted.reason, "accepted");

        // An invalid request is rejected with the validator's error verbatim
        let mut rejected_request = valid.clone();
        rejected_request.preset_id = String::new();
        let rejected = IntakeRecord::new(
            1_700_000_000_001,
            rejected_request,
            InboundJobRequest::validate,
        );
        assert!(!rejected.accepted);
        assert_eq!(rejected.reason, "preset id must not be empty");

        // Re-evaluating the same inputs yields an identical record
        let replay = IntakeRecord::new(1_700_000_000_000, valid, |request| request.validate());
        assert_eq!(replay, accepted);
    }

    #[test]
    fn queue_schema_migration_v1_to_v2() {
        // A single migration sets the flag and wraps the legacy path as a list
        let single = migrate_queue_payload_v1_to_v2(&QueuedJobPayloadV1 {
            job_id: "j-1".into(),
            output_path: "/exports/a.mp4".into(),
            preset_id: "h264-web".into(),
        })
        .expect("single migration failed");
        assert!(single.migrated_from_v1);
        assert_eq!(single.output_paths, vec!["/exports/a.mp4"]);
        assert_eq!(single.job_id, "j-1");
        assert_eq!(single.preset_id, "h264-web");

        // A batch migrates every item in order
        let batch = migrate_queue_batch_v1_to_v2(&[
            QueuedJobPayloadV1 {
                job_id: "j-1".into(),
                output_path: "/exports/a.mp4".into(),
                preset_id: "h264-web".into(),
            },
            QueuedJobPayloadV1 {
                job_id: "j-2".into(),
                output_path: "/exports/b.webm".into(),
                preset_id: "vp9-web".into(),
            },
        ])
        .expect("batch migration failed");
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].job_id, "j-1");
        assert_eq!(batch[0].output_paths, vec!["/exports/a.mp4"]);
        assert_eq!(batch[1].job_id, "j-2");
        assert_eq!(batch[1].output_paths, vec!["/exports/b.webm"]);

        // A bad item fails the batch naming the offending index
        let bad_batch = migrate_queue_batch_v1_to_v2(&[
            QueuedJobPayloadV1 {
                job_id: "j-ok".into(),
                output_path: "/exports/a.mp4".into(),
                preset_id: "h264-web".into(),
            },
            QueuedJobPayloadV1 {
                job_id: String::new(),
                output_path: "/exports/broken.webm".into(),
                preset_id: "vp9-web".into(),
            },
        ])
        .expect_err("batch with an empty job id must fail");
        assert!(bad_batch.contains("index 1"), "error was: {bad_batch}");

        // Empty legacy paths are allowed and become zero destinations
        let no_destinations = migrate_queue_payload_v1_to_v2(&QueuedJobPayloadV1 {
            job_id: "j-3".into(),
            output_path: String::new(),
            preset_id: "prores-master".into(),
        })
        .expect("empty path migration failed");
        assert!(no_destinations.migrated_from_v1);
        assert!(no_destinations.output_paths.is_empty());

        // Empty job ids err, and so do empty preset ids
        assert!(migrate_queue_payload_v1_to_v2(&QueuedJobPayloadV1 {
            job_id: String::new(),
            output_path: "/exports/c.mov".into(),
            preset_id: "prores-master".into(),
        })
        .is_err());
        assert!(migrate_queue_payload_v1_to_v2(&QueuedJobPayloadV1 {
            job_id: "j-4".into(),
            output_path: "/exports/c.mov".into(),
            preset_id: String::new(),
        })
        .is_err());
    }
}
