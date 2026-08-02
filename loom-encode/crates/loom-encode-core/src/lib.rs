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
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::InvalidJob(message) => write!(f, "invalid encode job: {message}"),
            EncodeError::Io(error) => write!(f, "encoder I/O error: {error}"),
            EncodeError::ProcessFailed { code, stderr } => {
                write!(f, "encoder failed with status {code:?}: {stderr}")
            }
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

/// Executes one planned job and streams progress updates.
pub fn execute_job<F>(
    job: &mut EncodeJob,
    plan: &EncodePlan,
    duration_secs: Option<f64>,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(f32),
{
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};
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
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        if let Some(progress) = parser.push_line(&line) {
            job.status = JobStatus::Encoding { progress };
            on_progress(progress);
        }
    }
    let status = child.wait()?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| EncodeError::InvalidJob("encoder stderr reader panicked".into()))??;
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
}
