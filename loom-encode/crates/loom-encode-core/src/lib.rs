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
}
