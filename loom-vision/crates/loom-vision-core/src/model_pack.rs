//! Model-pack manifests: parsing, validation, and safe installation.
//!
//! A model pack is a directory containing a `manifest.json` and the model
//! files it references. All validation is local; there is no network
//! anywhere in this module.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::VisionError;
use crate::provider::CapabilityId;
use crate::FORMAT_VERSION;

/// File name of the model-pack manifest inside a pack directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Default maximum total unpacked size of a model pack: 2 GiB.
///
/// Used as an archive-bomb guard during validation; installs that would
/// exceed this limit are rejected.
pub const DEFAULT_MAX_PACK_SIZE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// A validated, human-readable summary of a model pack.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPackSummary {
    /// Pack id.
    pub id: String,
    /// Pack name.
    pub name: String,
    /// Pack version string.
    pub version: String,
    /// Capability provided by the pack.
    pub capability: CapabilityId,
    /// SPDX license identifier of the pack contents.
    pub license: String,
    /// Number of model files declared and verified.
    pub model_count: usize,
    /// Sum of verified model file sizes in bytes.
    pub total_bytes: u64,
}

/// A model file declared by a pack manifest.
///
/// `sha256` is serialized to JSON as a lowercase 64-character hex string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    /// Relative path of the file inside the pack directory.
    pub path: String,
    /// Expected SHA-256 digest of the file contents.
    pub sha256: [u8; 32],
    /// Expected size of the file in bytes.
    pub size: u64,
}

impl ModelFile {
    /// Returns the SHA-256 digest as a lowercase hex string.
    pub fn sha256_hex(&self) -> String {
        hex_encode(&self.sha256)
    }
}

/// A test vector included with a model pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestVector {
    /// Test-vector name.
    pub name: String,
    /// Reference to an input asset inside the pack.
    pub input: String,
    /// Reference to the expected output inside the pack.
    pub expected: String,
}

/// The parsed contents of a `manifest.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPackManifest {
    /// Manifest format version; must equal [`crate::FORMAT_VERSION`].
    pub format_version: u32,
    /// Stable pack id (used in the installation directory name).
    pub id: String,
    /// Human-readable pack name.
    pub name: String,
    /// Pack version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// SPDX license identifier of the pack contents.
    pub license: String,
    /// Where the pack came from.
    pub provenance: String,
    /// Capability provided by the pack.
    pub capability: CapabilityId,
    /// Free-form runtime requirements (e.g. `"onnxruntime>=1.18"`).
    pub runtime_requirements: Vec<String>,
    /// Declared peak memory requirement in bytes.
    pub required_memory_bytes: u64,
    /// Model files to validate and install.
    pub models: Vec<ModelFile>,
    /// Test vectors bundled with the pack.
    pub test_vectors: Vec<TestVector>,
    /// Minimum compatible Loom Vision version.
    pub compatibility_min: String,
    /// Maximum compatible Loom Vision version.
    pub compatibility_max: String,
}

impl Serialize for ModelFile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        JsonModelFile {
            path: &self.path,
            sha256: hex_encode(&self.sha256),
            size: self.size,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModelFile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = JsonModelFile::deserialize(deserializer)?;
        let sha256 = hex_decode(&raw.sha256).map_err(serde::de::Error::custom)?;
        Ok(ModelFile {
            path: raw.path.to_string(),
            sha256,
            size: raw.size,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct JsonModelFile<'a> {
    path: &'a str,
    sha256: String,
    size: u64,
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!(
            "sha256 must be 64 hex characters, got {}: {s:?}",
            s.len()
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_value(chunk[0]).ok_or_else(|| format!("invalid hex byte in sha256: {s:?}"))?;
        let lo = hex_value(chunk[1]).ok_or_else(|| format!("invalid hex byte in sha256: {s:?}"))?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parses and structurally validates a `manifest.json` document.
///
/// Checks that the JSON is well-formed, `format_version` matches
/// [`crate::FORMAT_VERSION`], and that `id`, `name`, `version` are non-empty
/// and at least one model is declared. Content is not touched here; use
/// [`validate_pack`] for full validation.
pub fn parse_manifest(json: &str) -> Result<ModelPackManifest, VisionError> {
    let manifest: ModelPackManifest = serde_json::from_str(json).map_err(|err| {
        VisionError::InvalidModelPack(format!("manifest is not valid JSON: {err}"))
    })?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(VisionError::InvalidModelPack(format!(
            "unsupported manifest format_version {} (this build supports {FORMAT_VERSION})",
            manifest.format_version
        )));
    }
    if manifest.id.trim().is_empty()
        || manifest.name.trim().is_empty()
        || manifest.version.trim().is_empty()
    {
        return Err(VisionError::InvalidModelPack(
            "manifest must declare non-empty id, name, and version".to_string(),
        ));
    }
    if manifest.models.is_empty() {
        return Err(VisionError::InvalidModelPack(
            "manifest must declare at least one model file".to_string(),
        ));
    }
    Ok(manifest)
}

/// Validates a model pack directory at `dir` with the default size limit.
///
/// See [`validate_pack_with_limit`].
pub fn validate_pack(dir: &Path) -> Result<ModelPackSummary, VisionError> {
    validate_pack_with_limit(dir, DEFAULT_MAX_PACK_SIZE_BYTES)
}

/// Validates a model pack directory at `dir`.
///
/// Reads `manifest.json`, then verifies every declared model file:
///
/// * the path is a safe relative path (no absolute paths, no `..` or `.`
///   components),
/// * the file exists and is a regular file (symlinks are rejected),
/// * its size matches the declared size,
/// * its SHA-256 matches the declared digest,
/// * the cumulative size does not exceed `max_total_size` (archive-bomb
///   guard).
///
/// Returns a [`ModelPackSummary`] on success. This function never performs
/// any network access.
pub fn validate_pack_with_limit(
    dir: &Path,
    max_total_size: u64,
) -> Result<ModelPackSummary, VisionError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let raw = match fs::read(&manifest_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(VisionError::InvalidModelPack(format!(
                "no {MANIFEST_FILE} found in {}",
                dir.display()
            )));
        }
        Err(err) => return Err(VisionError::Io(err)),
    };
    let manifest = parse_manifest(&String::from_utf8_lossy(&raw))?;

    let mut total_bytes: u64 = 0;
    for model in &manifest.models {
        check_relative_path(&model.path)?;
        let full_path = dir.join(&model.path);
        let metadata = match fs::symlink_metadata(&full_path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(VisionError::InvalidModelPack(format!(
                    "model file is missing: {}",
                    model.path
                )));
            }
            Err(err) => return Err(VisionError::Io(err)),
        };
        if metadata.file_type().is_symlink() {
            return Err(VisionError::InvalidModelPack(format!(
                "model file must not be a symlink: {}",
                model.path
            )));
        }
        if !metadata.file_type().is_file() {
            return Err(VisionError::InvalidModelPack(format!(
                "model path is not a regular file: {}",
                model.path
            )));
        }
        if metadata.len() != model.size {
            return Err(VisionError::InvalidModelPack(format!(
                "size mismatch for {}: declared {}, actual {}",
                model.path,
                model.size,
                metadata.len()
            )));
        }
        total_bytes = total_bytes
            .checked_add(model.size)
            .ok_or_else(|| VisionError::InvalidModelPack("pack size overflow".to_string()))?;
        if total_bytes > max_total_size {
            return Err(VisionError::InvalidModelPack(format!(
                "pack exceeds the maximum unpacked size of {max_total_size} bytes (archive-bomb guard)"
            )));
        }
        let digest = sha256_file(&full_path)?;
        if digest != model.sha256 {
            return Err(VisionError::ChecksumMismatch);
        }
    }

    Ok(ModelPackSummary {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        capability: manifest.capability,
        license: manifest.license,
        model_count: manifest.models.len(),
        total_bytes,
    })
}

/// Installs a validated model pack into `dest_dir/<id>-<version>/`.
///
/// The source is validated first (with the default size limit). The pack is
/// installed into a versioned subdirectory whose name is derived from the
/// sanitized pack id and version. If the destination already holds a pack
/// with an identical manifest (same id, version, and model checksums) the
/// call is a no-op; a destination with different content is refused — use
/// [`install_pack_force`] to overwrite it. Symlinked destination paths are
/// refused. No network access is performed.
pub fn install_pack(src: &Path, dest_dir: &Path) -> Result<(), VisionError> {
    let manifest = read_manifest(src)?;
    let dest = destination_dir(dest_dir, &manifest)?;
    match fs::symlink_metadata(&dest) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(VisionError::InvalidModelPack(format!(
                    "destination {} is a symlink; refusing to install through it",
                    dest.display()
                )));
            }
            if !metadata.is_dir() {
                return Err(VisionError::InvalidModelPack(format!(
                    "destination {} exists and is not a directory",
                    dest.display()
                )));
            }
            let existing = read_manifest(&dest)?;
            if same_pack(&existing, &manifest) {
                return Ok(());
            }
            return Err(VisionError::InvalidModelPack(format!(
                "a different pack already exists at {}; use install_pack_force to overwrite",
                dest.display()
            )));
        }
        Err(err) => return Err(VisionError::Io(err)),
    }
    copy_pack(src, &dest, &manifest)
}

/// Installs a validated model pack, overwriting an existing destination pack.
///
/// Removes `dest_dir/<id>-<version>/` if it exists (including force on any
/// non-symlink content) before performing the safe install of
/// [`install_pack`].
pub fn install_pack_force(src: &Path, dest_dir: &Path) -> Result<(), VisionError> {
    let manifest = read_manifest(src)?;
    let dest = destination_dir(dest_dir, &manifest)?;
    match fs::symlink_metadata(&dest) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(VisionError::InvalidModelPack(format!(
                    "destination {} is a symlink; refusing to remove it",
                    dest.display()
                )));
            }
            if metadata.is_dir() {
                fs::remove_dir_all(&dest)?;
            } else {
                fs::remove_file(&dest)?;
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(VisionError::Io(err)),
    }
    copy_pack(src, &dest, &manifest)
}

fn read_manifest(pack_dir: &Path) -> Result<ModelPackManifest, VisionError> {
    // Validate first so all checks (paths, checksums, size limits) apply to
    // anything we install.
    validate_pack(pack_dir)?;
    let raw = fs::read(pack_dir.join(MANIFEST_FILE))?;
    parse_manifest(&String::from_utf8_lossy(&raw))
}

fn destination_dir(dest_dir: &Path, manifest: &ModelPackManifest) -> Result<PathBuf, VisionError> {
    let id = sanitize_component(&manifest.id)?;
    let version = sanitize_component(&manifest.version)?;
    Ok(dest_dir.join(format!("{id}-{version}")))
}

fn copy_pack(src: &Path, dest: &Path, manifest: &ModelPackManifest) -> Result<(), VisionError> {
    fs::create_dir_all(dest)?;
    fs::copy(src.join(MANIFEST_FILE), dest.join(MANIFEST_FILE))?;
    for model in &manifest.models {
        let target = dest.join(&model.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src.join(&model.path), &target)?;
    }
    Ok(())
}

fn same_pack(a: &ModelPackManifest, b: &ModelPackManifest) -> bool {
    a.id == b.id && a.version == b.version && a.models == b.models
}

/// Restricts a manifest-provided string to a safe directory component.
///
/// Only `[A-Za-z0-9._-]` is allowed, the result must not start with a dot
/// (no dotfiles, no `..` prefixes), and it must not be `.` or `..`.
fn sanitize_component(value: &str) -> Result<String, VisionError> {
    let mut sanitized: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    while sanitized.starts_with('.') {
        sanitized.remove(0);
    }
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        return Err(VisionError::InvalidModelPack(format!(
            "pack component {value:?} is not a valid directory name"
        )));
    }
    Ok(sanitized)
}

/// Rejects absolute paths and paths containing `.`, `..`, root, or prefix
/// components (path-traversal guard).
fn check_relative_path(path: &str) -> Result<(), VisionError> {
    if path.trim().is_empty() {
        return Err(VisionError::InvalidModelPack(
            "model path must not be empty".to_string(),
        ));
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(VisionError::InvalidModelPack(format!(
            "model path must be relative, got absolute path {path:?}"
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(VisionError::InvalidModelPack(format!(
                    "model path must not contain {component:?} components: {path:?}"
                )));
            }
        }
    }
    Ok(())
}

/// Computes the SHA-256 of a file without loading it fully into memory.
fn sha256_file(path: &Path) -> Result<[u8; 32], VisionError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_manifest(dir: &Path, manifest: &ModelPackManifest) {
        let json = serde_json::to_string_pretty(manifest).expect("serialize manifest");
        fs::write(dir.join(MANIFEST_FILE), json).expect("write manifest");
    }

    fn sample_manifest(id: &str) -> ModelPackManifest {
        ModelPackManifest {
            format_version: FORMAT_VERSION,
            id: id.to_string(),
            name: "Sample Pack".to_string(),
            version: "1.2.3".to_string(),
            description: "A sample model pack".to_string(),
            license: "MIT".to_string(),
            provenance: "generated in tests".to_string(),
            capability: CapabilityId::QrDetection,
            runtime_requirements: vec!["cpu".to_string()],
            required_memory_bytes: 1024,
            models: vec![ModelFile {
                path: "model.bin".to_string(),
                sha256: [0u8; 32],
                size: 0,
            }],
            test_vectors: vec![],
            compatibility_min: "0.1.0".to_string(),
            compatibility_max: "0.2.0".to_string(),
        }
    }

    /// Builds a valid pack directory: writes the model bytes, computes the
    /// real SHA-256 and size, and writes the manifest.
    fn build_pack(dir: &Path, model_bytes: &[u8]) -> ModelPackManifest {
        fs::write(dir.join("model.bin"), model_bytes).expect("write model");
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].sha256 = Sha256::digest(model_bytes).into();
        manifest.models[0].size = model_bytes.len() as u64;
        write_manifest(dir, &manifest);
        manifest
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0u8; 32];
        let hex = hex_encode(&bytes);
        assert_eq!(hex, "0".repeat(64));
        assert_eq!(hex_decode(&hex).unwrap(), bytes);

        let bytes: [u8; 32] = (0..32).collect::<Vec<u8>>().try_into().unwrap();
        let hex = hex_encode(&bytes);
        assert_eq!(hex_decode(&hex).unwrap(), bytes);
        assert!(hex_decode("not-hex!").is_err());
        assert!(hex_decode(&"0".repeat(63)).is_err());
    }

    #[test]
    fn parse_manifest_ok() {
        let dir = tempdir().unwrap();
        build_pack(dir.path(), b"data");
        let raw = fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
        let manifest = parse_manifest(&raw).unwrap();
        assert_eq!(manifest.id, "testpack");
        assert_eq!(manifest.models.len(), 1);
        assert_eq!(manifest.models[0].sha256_hex().len(), 64);
    }

    #[test]
    fn parse_manifest_rejects_invalid_json() {
        assert!(matches!(
            parse_manifest("{not json"),
            Err(VisionError::InvalidModelPack(_))
        ));
    }

    #[test]
    fn parse_manifest_rejects_wrong_format_version() {
        let dir = tempdir().unwrap();
        let mut manifest = build_pack(dir.path(), b"data");
        manifest.format_version = 99;
        write_manifest(dir.path(), &manifest);
        let raw = fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
        assert!(matches!(
            parse_manifest(&raw),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("format_version")
        ));
    }

    #[test]
    fn parse_manifest_rejects_empty_id() {
        let mut manifest = sample_manifest("x");
        manifest.id = "".to_string();
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(matches!(
            parse_manifest(&json),
            Err(VisionError::InvalidModelPack(_))
        ));
    }

    #[test]
    fn parse_manifest_rejects_zero_models() {
        let mut manifest = sample_manifest("x");
        manifest.models = vec![];
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(matches!(
            parse_manifest(&json),
            Err(VisionError::InvalidModelPack(_))
        ));
    }

    #[test]
    fn validate_pack_ok() {
        let dir = tempdir().unwrap();
        build_pack(dir.path(), b"model-data-0123456789");
        let summary = validate_pack(dir.path()).unwrap();
        assert_eq!(summary.id, "testpack");
        assert_eq!(summary.model_count, 1);
        assert_eq!(summary.total_bytes, 21);
        assert_eq!(summary.capability, CapabilityId::QrDetection);
    }

    #[test]
    fn validate_pack_reports_missing_manifest() {
        let dir = tempdir().unwrap();
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("manifest.json")
        ));
    }

    #[test]
    fn validate_pack_detects_checksum_mismatch() {
        let dir = tempdir().unwrap();
        build_pack(dir.path(), b"good bytes");
        fs::write(dir.path().join("model.bin"), b"evil bytes").unwrap();
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::ChecksumMismatch)
        ));
    }

    #[test]
    fn validate_pack_detects_missing_model_file() {
        let dir = tempdir().unwrap();
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].sha256 = [0u8; 32];
        manifest.models[0].size = 0;
        write_manifest(dir.path(), &manifest);
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("missing")
        ));
    }

    #[test]
    fn validate_pack_detects_size_mismatch() {
        let dir = tempdir().unwrap();
        let mut manifest = build_pack(dir.path(), b"12345678");
        manifest.models[0].size = 100;
        write_manifest(dir.path(), &manifest);
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("size mismatch")
        ));
    }

    #[test]
    fn validate_pack_rejects_parent_dir_traversal() {
        let dir = tempdir().unwrap();
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].path = "../evil.bin".to_string();
        write_manifest(dir.path(), &manifest);
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("must not contain")
        ));
    }

    #[test]
    fn validate_pack_rejects_absolute_path() {
        let dir = tempdir().unwrap();
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].path = "/etc/passwd".to_string();
        write_manifest(dir.path(), &manifest);
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("relative")
        ));
    }

    #[test]
    fn validate_pack_rejects_current_dir_component() {
        let dir = tempdir().unwrap();
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].path = "./model.bin".to_string();
        write_manifest(dir.path(), &manifest);
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(_))
        ));
    }

    #[test]
    fn validate_pack_enforces_size_limit() {
        let dir = tempdir().unwrap();
        build_pack(dir.path(), &[7u8; 1024]);
        let result = validate_pack_with_limit(dir.path(), 100);
        assert!(matches!(
            result,
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("archive-bomb")
        ));
    }

    #[test]
    fn validate_pack_accepts_nested_model_paths() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("models")).unwrap();
        fs::write(dir.path().join("models/quant.onnx"), b"onnx-data").unwrap();
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].path = "models/quant.onnx".to_string();
        manifest.models[0].sha256 = Sha256::digest(b"onnx-data").into();
        manifest.models[0].size = 9;
        write_manifest(dir.path(), &manifest);
        let summary = validate_pack(dir.path()).unwrap();
        assert_eq!(summary.total_bytes, 9);
    }

    #[cfg(unix)]
    #[test]
    fn validate_pack_rejects_symlinked_model_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("real.bin"), b"payload").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.bin"), dir.path().join("model.bin"))
            .unwrap();
        let mut manifest = sample_manifest("testpack");
        manifest.models[0].sha256 = Sha256::digest(b"payload").into();
        manifest.models[0].size = 7;
        write_manifest(dir.path(), &manifest);
        assert!(matches!(
            validate_pack(dir.path()),
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("symlink")
        ));
    }

    #[test]
    fn install_pack_copies_files() {
        let src = tempdir().unwrap();
        build_pack(src.path(), b"model-bytes");
        let dest = tempdir().unwrap();

        install_pack(src.path(), dest.path()).unwrap();

        let installed = dest.path().join("testpack-1.2.3");
        assert!(installed.join(MANIFEST_FILE).is_file());
        assert!(installed.join("model.bin").is_file());
        assert_eq!(
            fs::read(installed.join("model.bin")).unwrap(),
            b"model-bytes"
        );
    }

    #[test]
    fn install_pack_is_noop_when_checksum_matches() {
        let src = tempdir().unwrap();
        build_pack(src.path(), b"model-bytes");
        let dest = tempdir().unwrap();

        install_pack(src.path(), dest.path()).unwrap();
        install_pack(src.path(), dest.path()).unwrap();
        assert!(dest.path().join("testpack-1.2.3/model.bin").is_file());
    }

    #[test]
    fn install_pack_refuses_different_checksum() {
        let src = tempdir().unwrap();
        build_pack(src.path(), b"version-one");
        let dest = tempdir().unwrap();
        install_pack(src.path(), dest.path()).unwrap();

        // Rewrite the source as a *valid* pack with different content.
        let mut manifest = read_manifest(src.path()).unwrap();
        fs::write(src.path().join("model.bin"), b"version-two").unwrap();
        manifest.models[0].sha256 = Sha256::digest(b"version-two").into();
        manifest.models[0].size = 11;
        write_manifest(src.path(), &manifest);

        let result = install_pack(src.path(), dest.path());
        assert!(matches!(
            result,
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("already exists")
        ));
    }

    #[test]
    fn install_pack_force_overwrites() {
        let src = tempdir().unwrap();
        build_pack(src.path(), b"version-one");
        let dest = tempdir().unwrap();
        install_pack(src.path(), dest.path()).unwrap();

        let mut manifest = read_manifest(src.path()).unwrap();
        fs::write(src.path().join("model.bin"), b"version-two!").unwrap();
        manifest.models[0].sha256 = Sha256::digest(b"version-two!").into();
        manifest.models[0].size = 12;
        write_manifest(src.path(), &manifest);

        install_pack_force(src.path(), dest.path()).unwrap();
        assert_eq!(
            fs::read(dest.path().join("testpack-1.2.3/model.bin")).unwrap(),
            b"version-two!"
        );
    }

    #[test]
    fn install_pack_sanitizes_directory_component() {
        let src = tempdir().unwrap();
        let mut manifest = build_pack(src.path(), b"data");
        manifest.id = "../escape".to_string();
        write_manifest(src.path(), &manifest);
        let dest = tempdir().unwrap();

        let result = install_pack(src.path(), dest.path());
        // "../escape" sanitizes to "escape-1.2.3" and stays inside dest.
        assert!(result.is_ok());
        assert!(dest.path().join("escape-1.2.3/model.bin").is_file());
        assert!(!dest.path().parent().unwrap().join("escape-1.2.3").exists());
    }

    #[cfg(unix)]
    #[test]
    fn install_pack_refuses_symlinked_destination() {
        let src = tempdir().unwrap();
        build_pack(src.path(), b"data");
        let dest = tempdir().unwrap();
        let link = dest.path().join("testpack-1.2.3");
        std::os::unix::fs::symlink(dest.path().join("elsewhere"), &link).unwrap();

        let result = install_pack(src.path(), dest.path());
        assert!(matches!(
            result,
            Err(VisionError::InvalidModelPack(msg)) if msg.contains("symlink")
        ));
    }

    #[test]
    fn sanitize_component_handles_weird_input() {
        assert_eq!(sanitize_component("hello world").unwrap(), "helloworld");
        assert!(sanitize_component("..")
            .unwrap_err()
            .to_string()
            .contains("not a valid"));
        assert!(sanitize_component("###").is_err());
        assert_eq!(sanitize_component("v1.0-rc2").unwrap(), "v1.0-rc2");
    }
}
