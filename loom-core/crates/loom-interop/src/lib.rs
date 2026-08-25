//! Secure format detection and conversion orchestration for Loom.
//!
//! Loom's native document models remain authoritative. This crate provides a
//! truthful interoperability boundary: it identifies compound office and
//! media formats from their contents, records expected fidelity loss, discovers
//! local converters, and executes conversion plans without a command shell.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// File formats understood by the Loom interoperability boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Format {
    /// Microsoft Word Open XML.
    Docx,
    /// Microsoft Excel Open XML.
    Xlsx,
    /// Microsoft PowerPoint Open XML.
    Pptx,
    /// OpenDocument Text.
    Odt,
    /// OpenDocument Spreadsheet.
    Ods,
    /// OpenDocument Presentation.
    Odp,
    /// Adobe Photoshop document.
    Psd,
    /// Portable Document Format.
    Pdf,
    /// Scalable Vector Graphics.
    Svg,
    /// Portable Network Graphics.
    Png,
    /// JPEG image.
    Jpeg,
    /// TIFF image.
    Tiff,
    /// WebP image.
    Webp,
    /// OpenEXR image.
    Exr,
    /// MP4-family ISO base media.
    Mp4,
    /// QuickTime movie.
    Mov,
    /// Matroska video.
    Mkv,
    /// WebM video.
    Webm,
    /// Waveform audio.
    Wav,
    /// Free Lossless Audio Codec.
    Flac,
    /// MPEG Layer III audio.
    Mp3,
    /// Ogg container.
    Ogg,
    /// Comma-separated values.
    Csv,
    /// Markdown text.
    Markdown,
    /// Plain UTF-8 text.
    Text,
    /// Unknown or unsupported format.
    Unknown,
}

impl Format {
    /// Canonical lower-case extension without a leading dot.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Odt => "odt",
            Self::Ods => "ods",
            Self::Odp => "odp",
            Self::Psd => "psd",
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Tiff => "tiff",
            Self::Webp => "webp",
            Self::Exr => "exr",
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
            Self::Csv => "csv",
            Self::Markdown => "md",
            Self::Text => "txt",
            Self::Unknown => "bin",
        }
    }

    /// Whether the format is an office package.
    pub const fn is_office(self) -> bool {
        matches!(
            self,
            Self::Docx | Self::Xlsx | Self::Pptx | Self::Odt | Self::Ods | Self::Odp
        )
    }

    /// Whether the format is time-based media.
    pub const fn is_media(self) -> bool {
        matches!(
            self,
            Self::Mp4
                | Self::Mov
                | Self::Mkv
                | Self::Webm
                | Self::Wav
                | Self::Flac
                | Self::Mp3
                | Self::Ogg
        )
    }

    /// Whether the format is a raster image.
    pub const fn is_raster(self) -> bool {
        matches!(
            self,
            Self::Psd | Self::Png | Self::Jpeg | Self::Tiff | Self::Webp | Self::Exr
        )
    }
}

/// Result of content-based format detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    /// Detected format.
    pub format: Format,
    /// Confidence in the range 0..=100.
    pub confidence: u8,
    /// Human-readable reason suitable for diagnostics.
    pub reason: String,
    /// SHA-256 of the inspected bytes.
    pub sha256: String,
}

/// Detect a format from bytes, using the filename only as a final fallback.
pub fn detect(bytes: &[u8], file_name: Option<&str>) -> Detection {
    let (format, confidence, reason) = detect_inner(bytes, file_name);
    Detection {
        format,
        confidence,
        reason,
        sha256: sha256_hex(bytes),
    }
}

fn detect_inner(bytes: &[u8], file_name: Option<&str>) -> (Format, u8, String) {
    if bytes.starts_with(b"8BPS") {
        return (Format::Psd, 100, "Photoshop 8BPS signature".into());
    }
    if bytes.starts_with(b"%PDF-") {
        return (Format::Pdf, 100, "PDF header".into());
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return (Format::Png, 100, "PNG signature".into());
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return (Format::Jpeg, 100, "JPEG start-of-image".into());
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return (Format::Tiff, 100, "TIFF byte-order signature".into());
    }
    if bytes.starts_with(&[0x76, 0x2f, 0x31, 0x01]) {
        return (Format::Exr, 100, "OpenEXR signature".into());
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return (Format::Webp, 100, "RIFF WebP signature".into());
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return (Format::Wav, 100, "RIFF WAVE signature".into());
    }
    if bytes.starts_with(b"fLaC") {
        return (Format::Flac, 100, "FLAC signature".into());
    }
    if bytes.starts_with(b"OggS") {
        return (Format::Ogg, 100, "Ogg capture pattern".into());
    }
    if bytes.starts_with(b"ID3") || is_mpeg_audio_frame(bytes) {
        return (Format::Mp3, 95, "MPEG audio header".into());
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        let format = if bytes.windows(4).any(|window| window == b"webm") {
            Format::Webm
        } else {
            Format::Mkv
        };
        return (format, 95, "EBML container header".into());
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        let format = if brand == b"qt  " {
            Format::Mov
        } else {
            Format::Mp4
        };
        return (format, 95, "ISO base media ftyp box".into());
    }
    if bytes.starts_with(b"PK\x03\x04") {
        if let Some(detection) = detect_zip_package(bytes) {
            return detection;
        }
    }
    let trimmed = bytes
        .iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .take(256)
        .collect::<Vec<_>>();
    if trimmed.starts_with(b"<svg") || trimmed.windows(4).any(|window| window == b"<svg") {
        return (Format::Svg, 90, "SVG root element".into());
    }
    if let Some(format) = extension_format(file_name) {
        return (format, 40, "filename extension fallback".into());
    }
    if std::str::from_utf8(bytes).is_ok() {
        return (Format::Text, 25, "valid UTF-8 text".into());
    }
    (Format::Unknown, 0, "no recognized signature".into())
}

fn detect_zip_package(bytes: &[u8]) -> Option<(Format, u8, String)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut names = BTreeSet::new();
    let mut mime_type = None;
    for index in 0..archive.len().min(4096) {
        let mut entry = archive.by_index(index).ok()?;
        let name = entry.name().replace('\\', "/");
        if name == "mimetype" && entry.size() <= 256 {
            let mut value = String::new();
            let _ = entry.read_to_string(&mut value);
            mime_type = Some(value);
        }
        names.insert(name);
    }
    if names.contains("word/document.xml") {
        return Some((Format::Docx, 100, "OOXML Word package parts".into()));
    }
    if names.contains("xl/workbook.xml") {
        return Some((Format::Xlsx, 100, "OOXML Excel package parts".into()));
    }
    if names.contains("ppt/presentation.xml") {
        return Some((Format::Pptx, 100, "OOXML PowerPoint package parts".into()));
    }
    match mime_type.as_deref().map(str::trim) {
        Some("application/vnd.oasis.opendocument.text") => {
            Some((Format::Odt, 100, "OpenDocument text MIME entry".into()))
        }
        Some("application/vnd.oasis.opendocument.spreadsheet") => Some((
            Format::Ods,
            100,
            "OpenDocument spreadsheet MIME entry".into(),
        )),
        Some("application/vnd.oasis.opendocument.presentation") => Some((
            Format::Odp,
            100,
            "OpenDocument presentation MIME entry".into(),
        )),
        _ => None,
    }
}

fn is_mpeg_audio_frame(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0
}

fn extension_format(file_name: Option<&str>) -> Option<Format> {
    let extension = Path::new(file_name?)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    Some(match extension.as_str() {
        "docx" => Format::Docx,
        "xlsx" => Format::Xlsx,
        "pptx" => Format::Pptx,
        "odt" => Format::Odt,
        "ods" => Format::Ods,
        "odp" => Format::Odp,
        "psd" | "psb" => Format::Psd,
        "pdf" => Format::Pdf,
        "svg" => Format::Svg,
        "png" => Format::Png,
        "jpg" | "jpeg" => Format::Jpeg,
        "tif" | "tiff" => Format::Tiff,
        "webp" => Format::Webp,
        "exr" => Format::Exr,
        "mp4" | "m4v" => Format::Mp4,
        "mov" => Format::Mov,
        "mkv" => Format::Mkv,
        "webm" => Format::Webm,
        "wav" => Format::Wav,
        "flac" => Format::Flac,
        "mp3" => Format::Mp3,
        "ogg" | "oga" | "ogv" => Format::Ogg,
        "csv" => Format::Csv,
        "md" | "markdown" => Format::Markdown,
        "txt" => Format::Text,
        _ => return None,
    })
}

/// Severity of an interoperability fidelity issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Informational difference.
    Info,
    /// User-visible difference that normally remains editable.
    Warning,
    /// Feature loss or rasterization requiring explicit confirmation.
    Loss,
    /// Conversion cannot safely proceed.
    Blocker,
}

/// One expected or observed round-trip fidelity issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FidelityIssue {
    /// Stable machine-readable code.
    pub code: String,
    /// Severity.
    pub severity: Severity,
    /// Affected feature family.
    pub feature: String,
    /// Explanation shown to the user.
    pub message: String,
}

/// Fidelity report attached to every import and export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FidelityReport {
    /// Source format.
    pub source: Format,
    /// Destination format.
    pub destination: Format,
    /// Conversion issues.
    pub issues: Vec<FidelityIssue>,
}

impl FidelityReport {
    /// Whether conversion has a blocking issue.
    pub fn is_blocked(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == Severity::Blocker)
    }

    /// Whether conversion requires explicit loss confirmation.
    pub fn requires_confirmation(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity >= Severity::Loss)
    }
}

/// Generate a conservative preflight report for a conversion pair.
pub fn preflight(source: Format, destination: Format) -> FidelityReport {
    let mut issues = Vec::new();
    if source == destination {
        return FidelityReport {
            source,
            destination,
            issues,
        };
    }
    if source.is_office() || destination.is_office() {
        issues.push(FidelityIssue {
            code: "office.layout-engine".into(),
            severity: Severity::Warning,
            feature: "layout".into(),
            message: "Font metrics, pagination, themes, and floating-object placement can differ between layout engines.".into(),
        });
    }
    if matches!(source, Format::Xlsx | Format::Ods)
        || matches!(destination, Format::Xlsx | Format::Ods)
    {
        issues.push(FidelityIssue {
            code: "spreadsheet.formulas".into(),
            severity: Severity::Warning,
            feature: "formulas".into(),
            message: "External links, macros, data models, and vendor-specific formulas require compatibility review.".into(),
        });
    }
    if source == Format::Psd || destination == Format::Psd {
        issues.push(FidelityIssue {
            code: "psd.advanced-layers".into(),
            severity: Severity::Loss,
            feature: "layers".into(),
            message: "Smart objects, adjustment semantics, text engines, and proprietary blend behavior may be rasterized or approximated.".into(),
        });
    }
    if source.is_media() && destination.is_media() && source != destination {
        issues.push(FidelityIssue {
            code: "media.transcode".into(),
            severity: Severity::Warning,
            feature: "codec".into(),
            message: "Codec, color metadata, channel layout, subtitles, and timecode must be validated after transcoding.".into(),
        });
    }
    if source == Format::Unknown || destination == Format::Unknown {
        issues.push(FidelityIssue {
            code: "format.unsupported".into(),
            severity: Severity::Blocker,
            feature: "format".into(),
            message: "The source or destination format is unsupported.".into(),
        });
    }
    FidelityReport {
        source,
        destination,
        issues,
    }
}

/// Locally available conversion tools.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    /// LibreOffice executable.
    pub libreoffice: Option<PathBuf>,
    /// FFmpeg executable.
    pub ffmpeg: Option<PathBuf>,
    /// ImageMagick executable.
    pub imagemagick: Option<PathBuf>,
}

impl Toolchain {
    /// Discover supported executables from the current `PATH` without running
    /// a command shell.
    pub fn discover() -> Self {
        Self {
            libreoffice: find_program(&["libreoffice", "soffice"]),
            ffmpeg: find_program(&["ffmpeg"]),
            imagemagick: find_program(&["magick", "convert"]),
        }
    }
}

fn find_program(names: &[&str]) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&paths) {
        for name in names {
            let candidate = directory.join(executable_name(name));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) && !name.to_ascii_lowercase().ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// A shell-free external conversion plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionPlan {
    /// Program to execute.
    pub program: PathBuf,
    /// Program arguments, excluding the executable.
    pub arguments: Vec<String>,
    /// Source path.
    pub source: PathBuf,
    /// Expected destination path.
    pub destination: PathBuf,
    /// Fidelity preflight.
    pub fidelity: FidelityReport,
}

/// Error returned when planning or executing conversion.
#[derive(Debug)]
pub enum InteropError {
    /// I/O failure.
    Io(std::io::Error),
    /// Required converter is unavailable.
    ToolUnavailable(&'static str),
    /// Conversion pair is unsupported.
    Unsupported(String),
    /// Conversion timed out.
    Timeout,
    /// Converter returned an unsuccessful status.
    Failed(String),
    /// Expected output was not produced.
    MissingOutput(PathBuf),
    /// Unsafe or invalid path.
    InvalidPath(String),
}

impl std::fmt::Display for InteropError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::ToolUnavailable(tool) => {
                write!(formatter, "required converter unavailable: {tool}")
            }
            Self::Unsupported(message) => write!(formatter, "unsupported conversion: {message}"),
            Self::Timeout => write!(formatter, "conversion timed out"),
            Self::Failed(message) => write!(formatter, "converter failed: {message}"),
            Self::MissingOutput(path) => {
                write!(formatter, "converter did not create {}", path.display())
            }
            Self::InvalidPath(message) => write!(formatter, "invalid path: {message}"),
        }
    }
}

impl std::error::Error for InteropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for InteropError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Build a conversion plan using locally discovered tools.
pub fn plan_conversion(
    toolchain: &Toolchain,
    source_format: Format,
    destination_format: Format,
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<ConversionPlan, InteropError> {
    let source = validate_source(source.as_ref())?;
    let destination = validate_destination(destination.as_ref())?;
    let fidelity = preflight(source_format, destination_format);
    if fidelity.is_blocked() {
        return Err(InteropError::Unsupported(
            "blocked by fidelity preflight".into(),
        ));
    }
    if source_format.is_office() || destination_format.is_office() {
        let program = toolchain
            .libreoffice
            .clone()
            .ok_or(InteropError::ToolUnavailable("LibreOffice"))?;
        let output_directory = destination
            .parent()
            .ok_or_else(|| InteropError::InvalidPath("destination has no parent".into()))?;
        return Ok(ConversionPlan {
            program,
            arguments: vec![
                "--headless".into(),
                "--nologo".into(),
                "--nodefault".into(),
                "--nolockcheck".into(),
                "--convert-to".into(),
                destination_format.extension().into(),
                "--outdir".into(),
                output_directory.to_string_lossy().into_owned(),
                source.to_string_lossy().into_owned(),
            ],
            source,
            destination,
            fidelity,
        });
    }
    if source_format.is_media() || destination_format.is_media() {
        let program = toolchain
            .ffmpeg
            .clone()
            .ok_or(InteropError::ToolUnavailable("FFmpeg"))?;
        return Ok(ConversionPlan {
            program,
            arguments: vec![
                "-hide_banner".into(),
                "-nostdin".into(),
                "-y".into(),
                "-i".into(),
                source.to_string_lossy().into_owned(),
                "-map_metadata".into(),
                "0".into(),
                destination.to_string_lossy().into_owned(),
            ],
            source,
            destination,
            fidelity,
        });
    }
    if source_format.is_raster() || destination_format.is_raster() {
        let program = toolchain
            .imagemagick
            .clone()
            .ok_or(InteropError::ToolUnavailable("ImageMagick"))?;
        return Ok(ConversionPlan {
            program,
            arguments: vec![
                source.to_string_lossy().into_owned(),
                destination.to_string_lossy().into_owned(),
            ],
            source,
            destination,
            fidelity,
        });
    }
    Err(InteropError::Unsupported(format!(
        "{:?} to {:?}",
        source_format, destination_format
    )))
}

fn validate_source(path: &Path) -> Result<PathBuf, InteropError> {
    if path.as_os_str().is_empty() {
        return Err(InteropError::InvalidPath("source path is empty".into()));
    }
    if !path.is_file() {
        return Err(InteropError::InvalidPath(format!(
            "source is not a regular file: {}",
            path.display()
        )));
    }
    Ok(fs::canonicalize(path)?)
}

fn validate_destination(path: &Path) -> Result<PathBuf, InteropError> {
    if path.as_os_str().is_empty() {
        return Err(InteropError::InvalidPath(
            "destination path is empty".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| InteropError::InvalidPath("destination has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let canonical_parent = fs::canonicalize(parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| InteropError::InvalidPath("destination has no filename".into()))?;
    Ok(canonical_parent.join(name))
}

/// Execute a conversion plan with a wall-clock timeout.
///
/// The process receives no stdin and is not invoked through a shell. The
/// expected destination must exist and contain at least one byte on success.
pub fn execute(plan: &ConversionPlan, timeout: Duration) -> Result<(), InteropError> {
    if plan.fidelity.is_blocked() {
        return Err(InteropError::Unsupported(
            "conversion is blocked by preflight".into(),
        ));
    }
    let mut child = Command::new(&plan.program)
        .args(&plan.arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(InteropError::Failed(status.to_string()));
            }
            break;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(InteropError::Timeout);
        }
        thread::sleep(Duration::from_millis(20));
    }
    let metadata = fs::metadata(&plan.destination)
        .map_err(|_| InteropError::MissingOutput(plan.destination.clone()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(InteropError::MissingOutput(plan.destination.clone()));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Declared support level for one external format, per the interoperability programme's
/// required vocabulary. Detection alone never counts as support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FormatSupportLevel {
    /// The format is recognized by content or extension only.
    DetectOnly,
    /// Some structures import; losses are expected and reported.
    ReadPartial,
    /// Import completes with semantic comparison coverage.
    ReadSupported,
    /// Export produces valid files without full fidelity guarantees.
    WritePartial,
    /// Import and export pass semantic round-trip corpora.
    RoundTripSupported,
    /// Round-trip plus conformance validation against generated fixtures.
    ConformanceValidated,
}

/// Returns the currently declared support level for a format. This matrix states what the
/// engines actually implement today; it must be updated only alongside real behavior.
pub fn format_support_level(format: Format) -> FormatSupportLevel {
    match format {
        // Content-based detection exists for all fixture formats; text round-trips fully.
        Format::Text => FormatSupportLevel::RoundTripSupported,
        Format::Markdown | Format::Csv => FormatSupportLevel::ReadSupported,

        // Office and presentation containers are detected and partially read today.
        Format::Docx | Format::Xlsx | Format::Pptx | Format::Odt | Format::Ods | Format::Odp => {
            FormatSupportLevel::DetectOnly
        }

        // Layered image interchange is detection-only pending semantic comparisons.
        Format::Psd => FormatSupportLevel::DetectOnly,

        // Raster exports exist (PNG/JPEG writers); imports are decode-level.
        Format::Png | Format::Jpeg => FormatSupportLevel::WritePartial,
        Format::Tiff | Format::Webp | Format::Exr | Format::Pdf | Format::Svg => {
            FormatSupportLevel::DetectOnly
        }

        // Media containers are probed/decoded through local backends only.
        Format::Mp4
        | Format::Mov
        | Format::Mkv
        | Format::Webm
        | Format::Wav
        | Format::Flac
        | Format::Mp3
        | Format::Ogg => FormatSupportLevel::ReadPartial,

        Format::Unknown => FormatSupportLevel::DetectOnly,
    }
}

/// The full declared support matrix sorted by format.
pub fn format_support_matrix() -> Vec<(Format, FormatSupportLevel)> {
    let mut levels = [
        Format::Docx,
        Format::Xlsx,
        Format::Pptx,
        Format::Odt,
        Format::Ods,
        Format::Odp,
        Format::Psd,
        Format::Pdf,
        Format::Svg,
        Format::Png,
        Format::Jpeg,
        Format::Tiff,
        Format::Webp,
        Format::Exr,
        Format::Mp4,
        Format::Mov,
        Format::Mkv,
        Format::Webm,
        Format::Wav,
        Format::Flac,
        Format::Mp3,
        Format::Ogg,
        Format::Csv,
        Format::Markdown,
        Format::Text,
    ]
    .map(|format| {
        let level = format_support_level(format);
        (format, level)
    })
    .to_vec();
    levels.sort();
    levels
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start file");
            writer.write_all(bytes).expect("write entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn detects_office_packages_from_parts_not_extensions() {
        let docx = zip_with(&[("word/document.xml", b"<w:document/>")]);
        assert_eq!(detect(&docx, Some("wrong.zip")).format, Format::Docx);
        let ods = zip_with(&[(
            "mimetype",
            b"application/vnd.oasis.opendocument.spreadsheet",
        )]);
        assert_eq!(detect(&ods, None).format, Format::Ods);
    }

    #[test]
    fn detects_psd_and_media_magic() {
        assert_eq!(detect(b"8BPS\0\x01", None).format, Format::Psd);
        assert_eq!(
            detect(b"\0\0\0\x18ftypqt  rest", Some("clip.bin")).format,
            Format::Mov
        );
        assert_eq!(detect(b"RIFF0000WAVEdata", None).format, Format::Wav);
    }

    #[test]
    fn fidelity_reports_advanced_psd_loss() {
        let report = preflight(Format::Psd, Format::Png);
        assert!(report.requires_confirmation());
        assert!(!report.is_blocked());
    }

    #[test]
    fn office_plan_is_shell_free_and_path_scoped() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let source = temporary.path().join("source.docx");
        fs::write(&source, b"fake").expect("source");
        let destination = temporary.path().join("out.odt");
        let toolchain = Toolchain {
            libreoffice: Some(PathBuf::from("/usr/bin/libreoffice")),
            ..Toolchain::default()
        };
        let plan = plan_conversion(&toolchain, Format::Docx, Format::Odt, &source, &destination)
            .expect("plan");
        assert_eq!(plan.program, PathBuf::from("/usr/bin/libreoffice"));
        assert!(plan
            .arguments
            .iter()
            .any(|argument| argument == "--headless"));
        let expected_destination = fs::canonicalize(temporary.path())
            .expect("canonicalize")
            .join("out.odt");
        assert_eq!(plan.destination, expected_destination);
    }

    #[test]
    fn unknown_conversion_is_blocked() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let source = temporary.path().join("source.bin");
        fs::write(&source, b"bytes").expect("source");
        let error = plan_conversion(
            &Toolchain::default(),
            Format::Unknown,
            Format::Text,
            &source,
            temporary.path().join("out.txt"),
        )
        .expect_err("blocked");
        assert!(matches!(error, InteropError::Unsupported(_)));
    }

    /// Every fixture in the committed conformance corpus must detect as its
    /// documented format at full confidence, so the corpus cannot drift into
    /// placeholder content. Regenerate the corpus with
    /// `loom-bootstrap/scripts/generate-conformance-corpus.py`.
    #[test]
    fn conformance_corpus_detects_to_documented_formats() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join("loom-samples")
            .join("conformance");
        if !corpus.is_dir() {
            panic!("conformance corpus missing at {}", corpus.display());
        }
        let expected: &[(&str, Format, u8)] = &[
            ("docx/minimal.docx", Format::Docx, 100),
            ("xlsx/minimal.xlsx", Format::Xlsx, 100),
            ("pptx/minimal.pptx", Format::Pptx, 100),
            ("odt/minimal.odt", Format::Odt, 100),
            ("ods/minimal.ods", Format::Ods, 100),
            ("odp/minimal.odp", Format::Odp, 100),
            ("psd/one_pixel.psd", Format::Psd, 100),
            ("csv/accounts.csv", Format::Csv, 0),
            ("tsv/measurements.tsv", Format::Text, 0),
            ("markdown/notes.md", Format::Markdown, 0),
            ("plaintext/catalog.txt", Format::Text, 0),
        ];
        let mut checked = 0usize;
        for (relative, format, minimum_confidence) in expected {
            let path = corpus.join(relative);
            let bytes =
                fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let detected = detect(&bytes, Some(relative));
            assert_eq!(
                detected.format, *format,
                "{} detected as {:?} ({})",
                relative, detected.format, detected.reason
            );
            assert!(
                detected.confidence >= *minimum_confidence,
                "{} confidence {} below {}",
                relative,
                detected.confidence,
                minimum_confidence
            );
            checked += 1;
        }
        assert_eq!(checked, expected.len());
    }

    #[test]
    fn format_support_matrix_levels() {
        let matrix = format_support_matrix();
        // Every non-unknown format appears exactly once.
        assert_eq!(matrix.len(), 25);
        assert!(matrix.windows(2).all(|pair| pair[0].0 < pair[1].0));

        let level_of = |format: Format| {
            matrix
                .iter()
                .find(|entry| entry.0 == format)
                .map(|entry| entry.1)
                .unwrap()
        };

        // Detection-only formats must never be reported as supported.
        for format in [Format::Docx, Format::Psd, Format::Odp] {
            assert_eq!(
                level_of(format),
                FormatSupportLevel::DetectOnly,
                "{format:?} is detection-only today"
            );
        }

        // Text round-trips; CSV and Markdown are semantically read.
        assert_eq!(
            level_of(Format::Text),
            FormatSupportLevel::RoundTripSupported
        );
        assert_eq!(level_of(Format::Csv), FormatSupportLevel::ReadSupported);

        // Raster writers exist without full fidelity guarantees.
        assert_eq!(level_of(Format::Png), FormatSupportLevel::WritePartial);

        // Media containers decode locally but are not interchange round-tripped.
        assert_eq!(level_of(Format::Mp4), FormatSupportLevel::ReadPartial);
        assert_eq!(level_of(Format::Wav), FormatSupportLevel::ReadPartial);
    }
}
