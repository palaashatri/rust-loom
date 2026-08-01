//! # loom-plugin-host
//!
//! Directory-backed registry of installed Loom plugins, safe ZIP-package
//! installation, and runtime permission checks.
//!
//! The host reads `.loomplugin` packages (ZIP archives validated by
//! `loom-plugin-manifest`) and installs them into a store directory with the
//! following layout:
//!
//! ```text
//! <store>/
//!   <plugin_id>@<version>/
//!     manifest.json
//!     module.wasm
//!     assets/...
//!   installed.json        (informational index, regenerated on open)
//! ```
//!
//! Installation is defensive by construction:
//!
//! * All entry names are checked **before** anything is written (no `..`,
//!   no absolute paths, no symlink entries, no backslash paths).
//! * Archive-bomb guards: at most [`MAX_ENTRIES`] entries, at most
//!   [`MAX_TOTAL_BYTES`] of declared uncompressed content.
//! * `manifest.json` is parsed and validated before any extraction.
//! * The wasm module must be present and smaller than [`MAX_WASM_BYTES`].
//! * Streaming copies are capped; declared sizes are treated as advisory.
//!
//! This crate never executes WebAssembly. The WASI runtime that would run
//! validated plugins is explicitly out of scope for this milestone (see
//! `docs/rfcs/RFC-0009-plugin-abi-and-sandboxing.md` and `ROADMAP.md`).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use loom_plugin_manifest::{
    compare_versions, parse_manifest, Capability, Permission, PluginManifest,
};
use sha2::{Digest, Sha256};

/// Lowest plugin-API version this host supports.
pub const HOST_API_MIN_VERSION: &str = "0.1.0";
/// Highest plugin-API version this host supports.
pub const HOST_API_MAX_VERSION: &str = "0.9.0";

/// Maximum number of entries a package may contain.
pub const MAX_ENTRIES: usize = 1024;
/// Maximum declared uncompressed size of a whole package.
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// Maximum size of the wasm module inside a package.
pub const MAX_WASM_BYTES: u64 = 100 * 1024 * 1024;
/// Maximum size of the manifest document inside a package.
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Name of the informational index file inside the store directory.
pub const INDEX_FILE: &str = "installed.json";

/// Errors raised by [`PluginStore`] operations.
#[derive(Debug)]
pub enum HostError {
    /// Underlying filesystem failure.
    Io(io::Error),
    /// The archive could not be read as a ZIP file.
    Zip(String),
    /// The package manifest is invalid.
    InvalidManifest(String),
    /// A plugin with this id is already installed.
    AlreadyInstalled,
    /// No installed plugin matched the requested id.
    NotFound,
    /// The package contains an unsafe entry path.
    UnsafePath(String),
    /// A size or entry-count limit was exceeded.
    TooLarge,
    /// The plugin's API version range does not overlap the host's.
    UnsupportedApi(String),
    /// A requested operation was not granted by the manifest permissions.
    Denied(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Io(e) => write!(f, "io error: {e}"),
            HostError::Zip(msg) => write!(f, "invalid zip archive: {msg}"),
            HostError::InvalidManifest(msg) => write!(f, "invalid plugin manifest: {msg}"),
            HostError::AlreadyInstalled => write!(f, "plugin is already installed"),
            HostError::NotFound => write!(f, "no such installed plugin"),
            HostError::UnsafePath(path) => write!(f, "unsafe entry path in package: {path:?}"),
            HostError::TooLarge => write!(f, "package exceeds host size limits"),
            HostError::UnsupportedApi(msg) => write!(f, "plugin API not supported: {msg}"),
            HostError::Denied(msg) => write!(f, "permission denied: {msg}"),
        }
    }
}

impl std::error::Error for HostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HostError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for HostError {
    fn from(e: io::Error) -> Self {
        HostError::Io(e)
    }
}

/// An installed plugin: its parsed manifest plus resolved on-disk locations.
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    /// Plugin id (mirrors `manifest.plugin_id`).
    pub id: String,
    /// Plugin version (mirrors `manifest.version`).
    pub version: String,
    /// The validated manifest.
    pub manifest: PluginManifest,
    /// Directory containing this installation.
    pub install_dir: PathBuf,
    /// Path to the wasm module inside `install_dir`.
    pub wasm_path: PathBuf,
    /// SHA-256 of the manifest document as stored in the package.
    pub manifest_sha256: [u8; 32],
}

/// A directory-backed registry of installed plugins.
///
/// Opening a store creates the directory if needed and regenerates the
/// informational `installed.json` index from the contents of the directory,
/// so the index can never diverge from disk state.
#[derive(Debug, Clone)]
pub struct PluginStore {
    dir: PathBuf,
}

impl PluginStore {
    /// Open (creating if necessary) the store rooted at `dir`.
    pub fn open(dir: &Path) -> Result<PluginStore, HostError> {
        fs::create_dir_all(dir)?;
        let store = PluginStore {
            dir: dir.to_path_buf(),
        };
        store.refresh_index()?;
        Ok(store)
    }

    /// The store root directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Install a plugin package from raw zip bytes.
    ///
    /// The archive is fully validated (entry names, size limits, manifest,
    /// API range, wasm module presence) before anything is written to disk;
    /// on any failure the store directory is left untouched.
    pub fn install_zip(&self, zip_bytes: &[u8]) -> Result<InstalledPlugin, HostError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
            .map_err(|e| HostError::Zip(e.to_string()))?;

        if archive.len() > MAX_ENTRIES {
            return Err(HostError::TooLarge);
        }
        let mut declared_total: u64 = 0;
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| HostError::Zip(e.to_string()))?;
            declared_total = declared_total.saturating_add(entry.size());
            if declared_total > MAX_TOTAL_BYTES {
                return Err(HostError::TooLarge);
            }
        }

        let mut names: Vec<String> = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| HostError::Zip(e.to_string()))?;
            let name = entry.name().to_string();
            if !is_safe_entry_name(&name) {
                return Err(HostError::UnsafePath(name));
            }
            if entry_is_symlink(&entry) {
                return Err(HostError::UnsafePath(name));
            }
            names.push(name);
        }

        let manifest_index = names
            .iter()
            .position(|n| n == "manifest.json")
            .ok_or_else(|| HostError::InvalidManifest("missing manifest.json".into()))?;
        let manifest_bytes = read_entry_limited(&mut archive, manifest_index, MAX_MANIFEST_BYTES)?;
        let manifest_text = std::str::from_utf8(&manifest_bytes)
            .map_err(|_| HostError::InvalidManifest("manifest.json is not valid UTF-8".into()))?;
        let manifest =
            parse_manifest(manifest_text).map_err(|e| HostError::InvalidManifest(e.to_string()))?;

        if compare_versions(manifest.api_min_version.as_str(), HOST_API_MAX_VERSION)
            == std::cmp::Ordering::Greater
            || compare_versions(HOST_API_MIN_VERSION, manifest.api_max_version.as_str())
                == std::cmp::Ordering::Greater
        {
            return Err(HostError::UnsupportedApi(format!(
                "plugin requires api {}..={} but host provides {}..={}",
                manifest.api_min_version,
                manifest.api_max_version,
                HOST_API_MIN_VERSION,
                HOST_API_MAX_VERSION
            )));
        }

        let module_name = manifest.entry.wasm_module.clone();
        let module_index = names
            .iter()
            .position(|n| *n == module_name)
            .ok_or_else(|| {
                HostError::InvalidManifest(format!("wasm module {module_name} not in package"))
            })?;
        {
            let entry = archive
                .by_index(module_index)
                .map_err(|e| HostError::Zip(e.to_string()))?;
            if entry.size() > MAX_WASM_BYTES {
                return Err(HostError::TooLarge);
            }
        }

        if !self.installed_dirs(&manifest.plugin_id)?.is_empty() {
            return Err(HostError::AlreadyInstalled);
        }

        let install_dir = self
            .dir
            .join(format!("{}@{}", manifest.plugin_id, manifest.version));

        let result = (|| -> Result<InstalledPlugin, HostError> {
            fs::create_dir_all(&install_dir)?;
            fs::write(install_dir.join("manifest.json"), &manifest_bytes)?;
            copy_entry_limited(
                &mut archive,
                module_index,
                install_dir.join(&module_name),
                MAX_WASM_BYTES,
            )?;
            for i in 0..archive.len() {
                if i == manifest_index || i == module_index {
                    continue;
                }
                let entry = archive
                    .by_index(i)
                    .map_err(|e| HostError::Zip(e.to_string()))?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_string();
                let dest = install_dir.join(&name);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                drop(entry);
                copy_entry_limited(&mut archive, i, dest, MAX_TOTAL_BYTES)?;
            }
            let digest: [u8; 32] = Sha256::digest(&manifest_bytes).into();
            Ok(InstalledPlugin {
                id: manifest.plugin_id.clone(),
                version: manifest.version.clone(),
                manifest,
                install_dir: install_dir.clone(),
                wasm_path: install_dir.join(&module_name),
                manifest_sha256: digest,
            })
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&install_dir);
        } else {
            self.refresh_index()?;
        }
        result
    }

    /// List installed plugins in id order.
    ///
    /// Installations whose `manifest.json` cannot be parsed, or whose
    /// directory name does not match the manifest's `id@version`, are
    /// skipped (and the index regenerated to match disk reality).
    pub fn list(&self) -> Vec<InstalledPlugin> {
        let mut plugins = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return plugins;
        };
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            let Some((id, version)) = name.split_once('@') else {
                continue;
            };
            if id.is_empty() || version.is_empty() {
                continue;
            }
            let Some(manifest) = load_manifest(&entry.path()) else {
                continue;
            };
            if manifest.plugin_id != id || manifest.version != version {
                continue;
            }
            plugins.push(InstalledPlugin {
                id: id.to_string(),
                version: version.to_string(),
                wasm_path: entry.path().join(&manifest.entry.wasm_module),
                install_dir: entry.path(),
                manifest_sha256: [0; 32],
                manifest,
            });
        }
        plugins.sort_by(|a, b| a.id.cmp(&b.id));
        plugins
    }

    /// Look up an installed plugin by id.
    pub fn get(&self, id: &str) -> Option<InstalledPlugin> {
        self.list().into_iter().find(|p| p.id == id)
    }

    /// Remove all installations of `id`. Returns [`HostError::NotFound`] if
    /// nothing matched.
    pub fn uninstall(&self, id: &str) -> Result<(), HostError> {
        let dirs = self.installed_dirs(id)?;
        if dirs.is_empty() {
            return Err(HostError::NotFound);
        }
        for dir in dirs {
            fs::remove_dir_all(dir)?;
        }
        self.refresh_index()?;
        Ok(())
    }

    /// Regenerate the informational `installed.json` index from disk.
    pub fn refresh_index(&self) -> Result<(), HostError> {
        let plugins: Vec<serde_json::Value> = self
            .list()
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "version": p.version,
                    "install_dir": p.install_dir,
                })
            })
            .collect();
        let index = serde_json::json!({
            "schema_version": 1,
            "plugins": plugins,
        });
        let text =
            serde_json::to_string_pretty(&index).map_err(|e| HostError::Io(io::Error::other(e)))?;
        fs::write(self.dir.join(INDEX_FILE), text)?;
        Ok(())
    }

    /// Directories matching `id@*` inside the store.
    fn installed_dirs(&self, id: &str) -> Result<Vec<PathBuf>, HostError> {
        let prefix = format!("{id}@");
        let mut dirs = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with(&prefix) && entry.path().is_dir() {
                dirs.push(entry.path());
            }
        }
        Ok(dirs)
    }
}

/// The permissions a plugin was granted (direct accessor for hosts that want
/// to expose them in a permission UI).
pub fn permissions_for(plugin: &InstalledPlugin) -> Vec<Permission> {
    plugin.manifest.permissions.clone()
}

/// Check whether `requested` is granted to `plugin`, optionally for a
/// specific `path`.
///
/// Rules enforced:
///
/// 1. The capability must be listed in `manifest.capabilities`.
/// 2. [`Capability::HttpRequest`] additionally requires
///    `resource_limits.network == true`.
/// 3. When a `path` is supplied, at least one permission whose `resource`
///    matches the capability's resource family, whose `mode` permits the
///    operation, and whose `path_prefix` is a path-component prefix of the
///    canonicalized requested path must exist. Relative `path_prefix` values
///    are resolved against the plugin's install directory.
///
/// Denials are reported as [`HostError::Denied`]. Paths are canonicalized
/// when they exist and lexically normalized otherwise; permission prefixes
/// are treated the same way.
pub fn check_permission(
    plugin: &InstalledPlugin,
    requested: &Capability,
    path: Option<&Path>,
) -> Result<(), HostError> {
    if !plugin.manifest.capabilities.contains(requested) {
        return Err(HostError::Denied(format!(
            "plugin {} does not hold capability {}",
            plugin.id,
            requested.as_str()
        )));
    }
    if *requested == Capability::HttpRequest {
        if !plugin.manifest.resource_limits.network {
            return Err(HostError::Denied(format!(
                "plugin {} has no network allowance in resource limits",
                plugin.id
            )));
        }
        return Ok(());
    }
    let Some(path) = path else {
        // Non-path capabilities are fully granted by their presence.
        return Ok(());
    };
    let resource = capability_resource(requested);
    let mode = capability_mode(requested);
    let requested_path = canonicalize_or_normalize(path);
    for permission in &plugin.manifest.permissions {
        if !permission.resource.eq_ignore_ascii_case(resource) {
            continue;
        }
        if !mode_allows(&permission.mode, mode) {
            continue;
        }
        let Some(prefix) = &permission.path_prefix else {
            continue;
        };
        let prefix_path = Path::new(prefix);
        let resolved = if prefix_path.is_absolute() {
            prefix_path.to_path_buf()
        } else {
            plugin.install_dir.join(prefix_path)
        };
        let prefix = canonicalize_or_normalize(&resolved);
        if is_prefix(&prefix, &requested_path) {
            return Ok(());
        }
    }
    Err(HostError::Denied(format!(
        "no permission grants {requested:?} on {}",
        requested_path.display()
    )))
}

/// Resource family a capability maps to for permission matching.
fn capability_resource(cap: &Capability) -> &'static str {
    match cap {
        Capability::ReadFile | Capability::WriteFile => "file",
        Capability::ReadDir | Capability::WriteDir => "dir",
        Capability::HttpRequest => "network",
        Capability::ClipboardRead | Capability::ClipboardWrite => "clipboard",
        Capability::VisionInference => "vision",
        Capability::AccessTemp => "temp",
        Capability::PersistState => "state",
    }
}

/// Mode a capability requires of a matching permission.
fn capability_mode(cap: &Capability) -> &'static str {
    match cap {
        Capability::ReadFile | Capability::ReadDir | Capability::ClipboardRead => "read",
        Capability::WriteFile | Capability::WriteDir | Capability::PersistState => "write",
        Capability::HttpRequest => "exec",
        Capability::VisionInference | Capability::ClipboardWrite | Capability::AccessTemp => "exec",
    }
}

/// `perm_mode` satisfies `required_mode` when equal, or when the permission
/// allows `create` and a write is required.
fn mode_allows(perm_mode: &str, required_mode: &str) -> bool {
    perm_mode == required_mode || (required_mode == "write" && perm_mode == "create")
}

/// True when every path component of `prefix` equals the leading components
/// of `path` (no partial-name matches like `/foo/bar` vs `/foo/barbaz`).
fn is_prefix(prefix: &Path, path: &Path) -> bool {
    let normalized_prefix = normalize_path(prefix);
    let normalized_path = normalize_path(path);
    let prefix: Vec<Component> = normalized_prefix.components().collect();
    let path: Vec<Component> = normalized_path.components().collect();
    prefix.len() <= path.len() && prefix.iter().zip(&path).all(|(a, b)| a == b)
}

/// Canonicalize `path` when it exists; otherwise normalize it lexically.
fn canonicalize_or_normalize(path: &Path) -> PathBuf {
    if path.exists() {
        if let Ok(canonical) = fs::canonicalize(path) {
            return canonical;
        }
    }
    normalize_path(path)
}

/// Lexical path normalization: resolves `.` and `..` components without
/// touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A zip entry name is safe when it is relative, free of `.`/`..` components,
/// and contains no backslashes (defense-in-depth against path-slip variants).
fn is_safe_entry_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        return false;
    }
    !name.split('/').any(|c| c == ".." || c == ".")
}

/// Detect symlink entries via their unix mode bits (`S_IFLNK`).
fn entry_is_symlink(entry: &zip::read::ZipFile<'_>) -> bool {
    entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
}

/// Read a single zip entry into memory, streaming-capped at `limit` bytes.
fn read_entry_limited(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    index: usize,
    limit: u64,
) -> Result<Vec<u8>, HostError> {
    let mut entry = archive
        .by_index(index)
        .map_err(|e| HostError::Zip(e.to_string()))?;
    let mut buf = Vec::new();
    entry.by_ref().take(limit + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(HostError::TooLarge);
    }
    Ok(buf)
}

/// Stream-copy a zip entry to `dest`, failing when more than `limit` bytes
/// are produced (declared sizes are advisory; the stream is the truth).
fn copy_entry_limited(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    index: usize,
    dest: PathBuf,
    limit: u64,
) -> Result<(), HostError> {
    let mut entry = archive
        .by_index(index)
        .map_err(|e| HostError::Zip(e.to_string()))?;
    let mut out = fs::File::create(dest)?;
    let mut limited = entry.by_ref().take(limit + 1);
    let written = io::copy(&mut limited, &mut out)?;
    out.flush()?;
    if written > limit {
        return Err(HostError::TooLarge);
    }
    Ok(())
}

/// Load and validate the manifest of an installed plugin directory.
fn load_manifest(install_dir: &Path) -> Option<PluginManifest> {
    let bytes = fs::read(install_dir.join("manifest.json")).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    parse_manifest(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_entry_name_accepts_normal_paths() {
        assert!(is_safe_entry_name("manifest.json"));
        assert!(is_safe_entry_name("module.wasm"));
        assert!(is_safe_entry_name("assets/notes.txt"));
    }

    #[test]
    fn safe_entry_name_rejects_hostile_paths() {
        for bad in [
            "../evil",
            "/etc/passwd",
            "a/../../b",
            ".",
            "a/./b",
            "a\\b",
            "",
        ] {
            assert!(!is_safe_entry_name(bad), "expected unsafe: {bad:?}");
        }
    }

    #[test]
    fn normalize_path_resolves_dots() {
        assert_eq!(normalize_path(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize_path(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalize_path(Path::new("/x/..")), PathBuf::from("/"));
    }

    #[test]
    fn prefix_matching_is_component_aware() {
        assert!(is_prefix(Path::new("/a/b"), Path::new("/a/b/c")));
        assert!(is_prefix(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!is_prefix(Path::new("/a/b"), Path::new("/a/bc")));
        assert!(!is_prefix(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_prefix(Path::new("a/b"), Path::new("a/b/c")));
    }
}
