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
//! WebAssembly binaries are structurally validated before installation. When
//! the user has installed Wasmtime locally, [`ExternalWasmtimeRuntime`] can
//! execute a declared function with deny-by-default filesystem/network access,
//! bounded output capture, and an enforced wall-clock invocation limit.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

/// Signed installation, update/rollback, UI extension, migration, and native bridge APIs.
pub mod lifecycle;

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
/// Maximum combined stdout and stderr captured from one invocation.
pub const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
/// WebAssembly linear-memory page size.
pub const WASM_PAGE_BYTES: u64 = 65_536;
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
    /// The WebAssembly module is structurally invalid or incompatible.
    InvalidWasm(String),
    /// No supported local WebAssembly runtime was found.
    RuntimeUnavailable(String),
    /// Plugin execution failed.
    Execution(String),
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
            HostError::InvalidWasm(msg) => write!(f, "invalid WebAssembly module: {msg}"),
            HostError::RuntimeUnavailable(msg) => write!(f, "plugin runtime unavailable: {msg}"),
            HostError::Execution(msg) => write!(f, "plugin execution failed: {msg}"),
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
        let module_bytes = read_entry_limited(&mut archive, module_index, MAX_WASM_BYTES)?;
        validate_wasm_module(&module_bytes, manifest.resource_limits.max_memory_bytes)?;

        if !self.installed_dirs(&manifest.plugin_id)?.is_empty() {
            return Err(HostError::AlreadyInstalled);
        }

        let install_dir = self
            .dir
            .join(format!("{}@{}", manifest.plugin_id, manifest.version));

        let result = (|| -> Result<InstalledPlugin, HostError> {
            fs::create_dir_all(&install_dir)?;
            fs::write(install_dir.join("manifest.json"), &manifest_bytes)?;
            let module_path = install_dir.join(&module_name);
            if let Some(parent) = module_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&module_path, &module_bytes)?;
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
    let requested_path = requested_path(plugin, path);
    let requested_path = canonicalize_or_normalize(&requested_path).map_err(|error| {
        HostError::Denied(format!(
            "cannot resolve requested path {}: {error}",
            requested_path.display()
        ))
    })?;
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
        let prefix = canonicalize_or_normalize(&resolved).map_err(|error| {
            HostError::Denied(format!(
                "cannot resolve permission root {}: {error}",
                resolved.display()
            ))
        })?;
        if is_prefix(&prefix, &requested_path) {
            return Ok(());
        }
    }
    Err(HostError::Denied(format!(
        "no permission grants {requested:?} on {}",
        requested_path.display()
    )))
}

/// Write bytes through the host's permission boundary.
///
/// The target must match a `file` permission for [`Capability::WriteFile`].
/// Existing ancestors are resolved before authorization, then opened without
/// following links. The final file is written to a temporary sibling and
/// atomically renamed into place, so a pre-existing target hard link's other
/// name is not modified. This confines plugin requests routed through the host
/// API; it does not isolate the directory from another local process with the
/// same operating-system permissions.
pub fn write_file(plugin: &InstalledPlugin, path: &Path, bytes: &[u8]) -> Result<(), HostError> {
    let authorized = authorize_write_path(plugin, path)?;
    secure_write_file(&authorized.root, &authorized.relative, bytes).map_err(|error| {
        HostError::Denied(format!(
            "secure write refused for {}: {error}",
            authorized.requested.display()
        ))
    })
}

struct AuthorizedWritePath {
    root: cap_std::fs::Dir,
    // On Windows the final rename resolves directory handles back to paths.
    // Keep every ancestor handle open so none of those paths can be renamed
    // while the path-based publication is in progress.
    _path_guards: Vec<cap_std::fs::Dir>,
    relative: PathBuf,
    requested: PathBuf,
}

fn authorize_write_path(
    plugin: &InstalledPlugin,
    path: &Path,
) -> Result<AuthorizedWritePath, HostError> {
    if !plugin
        .manifest
        .capabilities
        .contains(&Capability::WriteFile)
    {
        return Err(HostError::Denied(format!(
            "plugin {} does not hold capability write-file",
            plugin.id
        )));
    }
    let requested = requested_path(plugin, path);
    let requested_normalized = normalize_path(&requested);
    let requested_resolved = canonicalize_or_normalize(&requested).map_err(|error| {
        HostError::Denied(format!(
            "cannot resolve requested path {}: {error}",
            requested.display()
        ))
    })?;
    let plugin_root = fs::canonicalize(&plugin.install_dir).map_err(|error| {
        HostError::Denied(format!(
            "cannot resolve plugin install directory {}: {error}",
            plugin.install_dir.display()
        ))
    })?;

    for permission in &plugin.manifest.permissions {
        if !permission.resource.eq_ignore_ascii_case("file")
            || !mode_allows(&permission.mode, "write")
        {
            continue;
        }
        let Some(path_prefix) = permission.path_prefix.as_deref() else {
            continue;
        };
        let prefix_path = Path::new(path_prefix);
        let prefix_is_relative = !prefix_path.is_absolute();
        let prefix = if prefix_is_relative {
            plugin.install_dir.join(prefix_path)
        } else {
            prefix_path.to_path_buf()
        };
        let prefix_normalized = normalize_path(&prefix);
        if !is_prefix(&prefix_normalized, &requested_normalized) {
            continue;
        }
        let prefix_resolved = canonicalize_or_normalize(&prefix).map_err(|error| {
            HostError::Denied(format!(
                "cannot resolve permission root {}: {error}",
                prefix.display()
            ))
        })?;
        // A relative manifest prefix is confined to the plugin installation
        // directory even when a package directory contains a symlink.
        if prefix_is_relative && !is_prefix(&plugin_root, &prefix_resolved) {
            continue;
        }
        if !is_prefix(&prefix_resolved, &requested_resolved) {
            continue;
        }
        let relative = requested_resolved
            .strip_prefix(&prefix_resolved)
            .map_err(|_| HostError::Denied("requested path escaped permission root".into()))?;
        if relative.as_os_str().is_empty()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::CurDir | Component::ParentDir | Component::RootDir
                )
            })
        {
            continue;
        }
        let (root, path_guards) = open_anchored_directory(&prefix_resolved).map_err(|error| {
            HostError::Denied(format!(
                "cannot safely open permission root {}: {error}",
                prefix_resolved.display()
            ))
        })?;
        return Ok(AuthorizedWritePath {
            root,
            _path_guards: path_guards,
            relative: relative.to_path_buf(),
            requested,
        });
    }

    Err(HostError::Denied(format!(
        "no permission grants WriteFile on {}",
        requested_resolved.display()
    )))
}

fn requested_path(plugin: &InstalledPlugin, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        plugin.install_dir.join(path)
    }
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

/// Canonicalize `path`, resolving the nearest existing ancestor for a new
/// target and preserving its remaining components.
fn canonicalize_or_normalize(path: &Path) -> io::Result<PathBuf> {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(existing)?;
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "path has no existing ancestor")
                })?;
                missing.push(name.to_os_string());
                existing = existing.parent().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "path has no existing ancestor")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
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

/// Open an absolute permission root one component at a time, starting from a
/// filesystem root that cannot be swapped. Every child is opened without
/// following links, and the returned handle stays attached to this directory
/// even if a parent path is renamed later.
#[cfg(unix)]
fn open_anchored_directory(path: &Path) -> io::Result<(cap_std::fs::Dir, Vec<cap_std::fs::Dir>)> {
    use cap_fs_ext::DirExt;
    use cap_std::fs::Dir;

    let mut components = path.components();
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "permission root must be absolute",
        ));
    }
    let mut directory = Dir::open_ambient_dir(Path::new("/"), cap_std::ambient_authority())?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "permission root must not contain dot or parent components",
            ));
        };
        directory = directory.open_dir_nofollow(name)?;
    }
    Ok((directory, Vec::new()))
}

/// Windows paths include a drive or share prefix before their root separator.
/// Keep each opened directory alive: cap-std uses paths for rename on Windows,
/// and its directory handles deny delete-sharing so path components cannot be
/// renamed while secure publication is in progress.
#[cfg(windows)]
fn open_anchored_directory(path: &Path) -> io::Result<(cap_std::fs::Dir, Vec<cap_std::fs::Dir>)> {
    use cap_fs_ext::DirExt;
    use cap_std::fs::Dir;
    use std::path::Prefix;

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "permission root must have a drive or network-share prefix",
        ));
    };
    if !matches!(
        prefix.kind(),
        Prefix::Disk(_) | Prefix::UNC(_, _) | Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _)
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "permission root uses an unsupported Windows path prefix",
        ));
    }
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "permission root must be absolute",
        ));
    }

    let mut volume_root = PathBuf::from(prefix.as_os_str());
    volume_root.push("\\");
    let mut directory = Dir::open_ambient_dir(&volume_root, cap_std::ambient_authority())?;
    let mut path_guards = Vec::new();
    for component in components {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "permission root must not contain dot or parent components",
            ));
        };
        let child = directory.open_dir_nofollow(name)?;
        path_guards.push(directory);
        directory = child;
    }
    Ok((directory, path_guards))
}

/// Platforms without a reviewed handle-relative implementation fail closed.
#[cfg(not(any(unix, windows)))]
fn open_anchored_directory(_path: &Path) -> io::Result<(cap_std::fs::Dir, Vec<cap_std::fs::Dir>)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure plugin writes are not supported on this platform",
    ))
}

struct WriteTarget {
    directory: cap_std::fs::Dir,
    file_name: std::ffi::OsString,
    #[cfg(windows)]
    _path_guards: Vec<cap_std::fs::Dir>,
}

fn open_write_target(root: &cap_std::fs::Dir, relative: &Path) -> io::Result<WriteTarget> {
    use cap_fs_ext::DirExt;

    let mut directory = root.try_clone()?;
    #[cfg(windows)]
    let mut path_guards = Vec::new();
    let mut components = relative.components().peekable();
    let file_name = loop {
        match components.next() {
            Some(Component::Normal(name)) if components.peek().is_none() => {
                break name.to_os_string()
            }
            Some(Component::Normal(name)) => {
                let child = directory.open_dir_nofollow(name)?;
                // cap-std's Windows rename operation reconstructs absolute
                // paths from directory handles. Keep every ancestor open so
                // none of those path components can be renamed in between.
                #[cfg(windows)]
                path_guards.push(directory);
                directory = child;
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "write target must be a relative file path",
                ));
            }
        }
    };

    Ok(WriteTarget {
        directory,
        file_name,
        #[cfg(windows)]
        _path_guards: path_guards,
    })
}

fn secure_write_file(root: &cap_std::fs::Dir, relative: &Path, bytes: &[u8]) -> io::Result<()> {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::OpenOptions;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    // Keep this object alive through rename. On Windows it owns handles for
    // the whole directory chain, not only the final parent.
    let target = open_write_target(root, relative)?;
    let directory = target.directory.try_clone()?;
    let file_name = &target.file_name;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    options.follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut temporary_name = None;
    let result = (|| {
        let mut file = None;
        for _ in 0..32 {
            let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = format!(".loom-write-{}-{sequence}", std::process::id());
            match directory.open_with(&candidate, &options) {
                Ok(opened) => {
                    temporary_name = Some(candidate);
                    file = Some(opened);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        let temporary_name = temporary_name.as_deref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a temporary plugin-write file",
            )
        })?;
        let mut file = file.expect("temporary name and file are set together");
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        directory.rename(temporary_name, &directory, file_name)
    })();

    if let Some(temporary_name) = temporary_name {
        if result.is_err() {
            let _ = directory.remove_file(temporary_name);
        }
    }
    result
}

/// Structural information discovered by the bounded WebAssembly validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmModuleInfo {
    /// Names exported by the module as functions.
    pub exported_functions: BTreeSet<String>,
    /// Declared initial linear-memory pages, when a memory section exists.
    pub initial_memory_pages: Option<u64>,
    /// Declared maximum linear-memory pages, when present.
    pub maximum_memory_pages: Option<u64>,
}

impl WasmModuleInfo {
    /// Whether the named function is exported by the module.
    pub fn exports_function(&self, name: &str) -> bool {
        self.exported_functions.contains(name)
    }
}

/// Validate a WebAssembly 1.0 binary without executing it.
///
/// The parser is deliberately small and bounded. It verifies the magic and
/// version, section framing/order, UTF-8 export names, memory declarations,
/// and the host's memory limit. Unknown section payloads remain opaque.
pub fn validate_wasm_module(
    bytes: &[u8],
    max_memory_bytes: u64,
) -> Result<WasmModuleInfo, HostError> {
    if bytes.len() < 8 || &bytes[..4] != b"\0asm" {
        return Err(HostError::InvalidWasm("missing wasm magic".into()));
    }
    if bytes[4..8] != [1, 0, 0, 0] {
        return Err(HostError::InvalidWasm(
            "only WebAssembly binary version 1 is supported".into(),
        ));
    }
    let mut cursor = WasmCursor::new(&bytes[8..]);
    let mut last_section = 0u8;
    let mut exported_functions = BTreeSet::new();
    let mut initial_memory_pages = None;
    let mut maximum_memory_pages = None;
    while !cursor.is_empty() {
        let section_id = cursor.byte()?;
        if section_id > 12 {
            return Err(HostError::InvalidWasm(format!(
                "unknown section id {section_id}"
            )));
        }
        if section_id != 0 {
            if section_id <= last_section {
                return Err(HostError::InvalidWasm(
                    "non-custom sections are out of order or duplicated".into(),
                ));
            }
            last_section = section_id;
        }
        let section_len = usize::try_from(cursor.var_u32()?)
            .map_err(|_| HostError::InvalidWasm("section length overflow".into()))?;
        let payload = cursor.take(section_len)?;
        let mut section = WasmCursor::new(payload);
        match section_id {
            5 => {
                let count = section.var_u32()?;
                if count > 1 {
                    return Err(HostError::InvalidWasm(
                        "multiple linear memories are not supported".into(),
                    ));
                }
                if count == 1 {
                    let flags = section.var_u32()?;
                    if flags > 1 {
                        return Err(HostError::InvalidWasm(
                            "shared or 64-bit memories are not supported".into(),
                        ));
                    }
                    let initial = u64::from(section.var_u32()?);
                    let maximum = if flags & 1 == 1 {
                        Some(u64::from(section.var_u32()?))
                    } else {
                        None
                    };
                    if maximum.is_some_and(|max| max < initial) {
                        return Err(HostError::InvalidWasm(
                            "memory maximum is below its initial size".into(),
                        ));
                    }
                    enforce_memory_limit(initial, maximum, max_memory_bytes)?;
                    initial_memory_pages = Some(initial);
                    maximum_memory_pages = maximum;
                }
            }
            7 => {
                let count = section.var_u32()?;
                if count > 65_536 {
                    return Err(HostError::InvalidWasm("too many exports".into()));
                }
                for _ in 0..count {
                    let name = section.name()?;
                    let kind = section.byte()?;
                    let _index = section.var_u32()?;
                    if kind == 0 {
                        if !exported_functions.insert(name.to_string()) {
                            return Err(HostError::InvalidWasm(format!(
                                "duplicate function export {name:?}"
                            )));
                        }
                    } else if kind > 3 {
                        return Err(HostError::InvalidWasm(format!(
                            "invalid export kind {kind}"
                        )));
                    }
                }
            }
            _ => {}
        }
        if !section.is_empty() && matches!(section_id, 5 | 7) {
            return Err(HostError::InvalidWasm(format!(
                "trailing bytes in section {section_id}"
            )));
        }
    }
    Ok(WasmModuleInfo {
        exported_functions,
        initial_memory_pages,
        maximum_memory_pages,
    })
}

fn enforce_memory_limit(
    initial_pages: u64,
    maximum_pages: Option<u64>,
    max_memory_bytes: u64,
) -> Result<(), HostError> {
    let page_limit = max_memory_bytes / WASM_PAGE_BYTES;
    if initial_pages > page_limit || maximum_pages.is_some_and(|pages| pages > page_limit) {
        return Err(HostError::InvalidWasm(format!(
            "module memory exceeds manifest limit of {max_memory_bytes} bytes"
        )));
    }
    Ok(())
}

struct WasmCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> WasmCursor<'a> {
    fn new(bytes: &'a [u8]) -> WasmCursor<'a> {
        WasmCursor { bytes, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn byte(&mut self) -> Result<u8, HostError> {
        let byte = *self
            .bytes
            .get(self.offset)
            .ok_or_else(|| HostError::InvalidWasm("unexpected end of module".into()))?;
        self.offset += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], HostError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| HostError::InvalidWasm("section length overflow".into()))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| HostError::InvalidWasm("truncated section".into()))?;
        self.offset = end;
        Ok(slice)
    }

    fn var_u32(&mut self) -> Result<u32, HostError> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            if shift == 28 && byte & 0xf0 != 0 {
                return Err(HostError::InvalidWasm("u32 LEB128 overflow".into()));
            }
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(HostError::InvalidWasm("invalid u32 LEB128".into()))
    }

    fn name(&mut self) -> Result<&'a str, HostError> {
        let len = usize::try_from(self.var_u32()?)
            .map_err(|_| HostError::InvalidWasm("name length overflow".into()))?;
        std::str::from_utf8(self.take(len)?)
            .map_err(|_| HostError::InvalidWasm("export name is not UTF-8".into()))
    }
}

/// A fully validated request to invoke one installed plugin function.
#[derive(Debug, Clone)]
pub struct PluginInvocation {
    /// Installed plugin to execute.
    pub plugin: InstalledPlugin,
    /// Function exported by the guest. This normally equals the manifest entry.
    pub function: String,
    /// Positional string arguments passed after the module path.
    pub arguments: Vec<String>,
    /// Bytes written to guest stdin.
    pub stdin: Vec<u8>,
}

impl PluginInvocation {
    /// Build an invocation for the plugin's declared entry point and validate
    /// that the installed module exports it.
    pub fn declared(plugin: &InstalledPlugin) -> Result<PluginInvocation, HostError> {
        let bytes = fs::read(&plugin.wasm_path)?;
        let info = validate_wasm_module(&bytes, plugin.manifest.resource_limits.max_memory_bytes)?;
        if !info.exports_function(&plugin.manifest.entry.function) {
            return Err(HostError::InvalidWasm(format!(
                "declared entry function {:?} is not exported",
                plugin.manifest.entry.function
            )));
        }
        Ok(PluginInvocation {
            plugin: plugin.clone(),
            function: plugin.manifest.entry.function.clone(),
            arguments: Vec::new(),
            stdin: Vec::new(),
        })
    }
}

/// Captured result of a local plugin invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationResult {
    /// Guest process exit code, or `None` when terminated by a signal.
    pub exit_code: Option<i32>,
    /// Captured standard output.
    pub stdout: Vec<u8>,
    /// Captured standard error.
    pub stderr: Vec<u8>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u128,
}

/// Local Wasmtime command-line backend.
///
/// Loom never downloads a runtime. Discovery checks `LOOM_WASMTIME` first,
/// then the current `PATH`. The guest receives no network or filesystem
/// preopens by default, preserving the manifest's deny-by-default contract.
#[derive(Debug, Clone)]
pub struct ExternalWasmtimeRuntime {
    executable: PathBuf,
}

impl ExternalWasmtimeRuntime {
    /// Discover a local `wasmtime` executable.
    pub fn discover() -> Result<ExternalWasmtimeRuntime, HostError> {
        if let Some(path) = std::env::var_os("LOOM_WASMTIME") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(ExternalWasmtimeRuntime { executable: path });
            }
        }
        find_executable("wasmtime")
            .map(|executable| ExternalWasmtimeRuntime { executable })
            .ok_or_else(|| {
                HostError::RuntimeUnavailable(
                    "install Wasmtime locally or set LOOM_WASMTIME".into(),
                )
            })
    }

    /// Construct a backend using an explicit executable path.
    pub fn new(executable: PathBuf) -> Result<ExternalWasmtimeRuntime, HostError> {
        if executable.is_file() {
            Ok(ExternalWasmtimeRuntime { executable })
        } else {
            Err(HostError::RuntimeUnavailable(format!(
                "{} is not a file",
                executable.display()
            )))
        }
    }

    /// Execute an invocation with the manifest CPU limit enforced as a
    /// wall-clock timeout and bounded output capture.
    pub fn invoke(&self, request: &PluginInvocation) -> Result<InvocationResult, HostError> {
        let verified = PluginInvocation::declared(&request.plugin)?;
        if verified.function != request.function {
            let bytes = fs::read(&request.plugin.wasm_path)?;
            let info = validate_wasm_module(
                &bytes,
                request.plugin.manifest.resource_limits.max_memory_bytes,
            )?;
            if !info.exports_function(&request.function) {
                return Err(HostError::InvalidWasm(format!(
                    "requested function {:?} is not exported",
                    request.function
                )));
            }
        }
        let mut command = Command::new(&self.executable);
        command
            .arg("run")
            .arg("--invoke")
            .arg(&request.function)
            .arg(&request.plugin.wasm_path)
            .args(&request.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let started = Instant::now();
        let mut child = command
            .spawn()
            .map_err(|error| HostError::Execution(error.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&request.stdin)
                .map_err(|error| HostError::Execution(error.to_string()))?;
        }
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HostError::Execution("runtime stdout pipe missing".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HostError::Execution("runtime stderr pipe missing".into()))?;
        let stdout_thread = thread::spawn(move || read_bounded(stdout, MAX_CAPTURE_BYTES));
        let stderr_thread = thread::spawn(move || read_bounded(stderr, MAX_CAPTURE_BYTES));
        let timeout = Duration::from_millis(
            request
                .plugin
                .manifest
                .resource_limits
                .max_cpu_ms_per_call
                .max(1),
        );
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| HostError::Execution(error.to_string()))?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HostError::Execution(format!(
                    "invocation exceeded {} ms limit",
                    timeout.as_millis()
                )));
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_thread
            .join()
            .map_err(|_| HostError::Execution("stdout reader panicked".into()))??;
        let stderr = stderr_thread
            .join()
            .map_err(|_| HostError::Execution("stderr reader panicked".into()))??;
        Ok(InvocationResult {
            exit_code: status.code(),
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis(),
        })
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, HostError> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(HostError::Execution(
            "runtime output exceeded capture limit".into(),
        ));
    }
    Ok(bytes)
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = directory.join(format!("{name}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
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
    use tempfile::TempDir;

    fn plugin_with_write_permission(install_dir: &Path) -> InstalledPlugin {
        let manifest = parse_manifest(
            r#"{
                "manifest_version": 1,
                "plugin_id": "write-test",
                "name": "Write Test",
                "version": "0.1.0",
                "license": "MIT",
                "entry": {
                    "kind": "command",
                    "wasm_module": "module.wasm",
                    "function": "invoke"
                },
                "capabilities": ["write-file"],
                "permissions": [{
                    "resource": "file",
                    "mode": "write",
                    "path_prefix": "allowed"
                }],
                "api_min_version": "0.1.0",
                "api_max_version": "0.9.0",
                "resource_limits": {
                    "max_memory_bytes": 1024,
                    "max_fs_bytes": 1024,
                    "max_fs_entries": 8,
                    "max_cpu_ms_per_call": 1000,
                    "network": false
                }
            }"#,
        )
        .expect("test manifest parses");
        InstalledPlugin {
            id: manifest.plugin_id.clone(),
            version: manifest.version.clone(),
            manifest,
            install_dir: install_dir.to_path_buf(),
            wasm_path: install_dir.join("module.wasm"),
            manifest_sha256: [0; 32],
        }
    }

    fn write_plugin_fixture() -> (TempDir, InstalledPlugin) {
        let temp = TempDir::new().expect("temporary directory");
        let install_dir = temp.path().join("plugin");
        fs::create_dir_all(install_dir.join("allowed")).expect("allowed directory");
        let plugin = plugin_with_write_permission(&install_dir);
        (temp, plugin)
    }

    #[cfg(any(unix, windows))]
    fn create_directory_link(link: &Path, target: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()?;
            if output.status.success() {
                Ok(())
            } else {
                Err(io::Error::other(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ))
            }
        }
    }

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

    #[test]
    fn validates_minimal_module() {
        let info = validate_wasm_module(b"\0asm\x01\0\0\0", 64 * 1024).unwrap();
        assert!(info.exported_functions.is_empty());
        assert_eq!(info.initial_memory_pages, None);
    }

    #[test]
    fn reads_function_export() {
        let module = b"\0asm\x01\0\0\0\x07\x16\x01\x12loom_plugin_invoke\x00\x00";
        let info = validate_wasm_module(module, 64 * 1024).unwrap();
        assert!(info.exports_function("loom_plugin_invoke"));
    }

    #[test]
    fn rejects_memory_over_manifest_limit() {
        let module = b"\0asm\x01\0\0\0\x05\x03\x01\x00\x02";
        let error = validate_wasm_module(module, 64 * 1024).unwrap_err();
        assert!(matches!(error, HostError::InvalidWasm(_)));
    }

    #[test]
    fn allowed_write_still_succeeds() {
        let (_temp, plugin) = write_plugin_fixture();
        write_file(&plugin, Path::new("allowed/notes.txt"), b"saved").unwrap();
        write_file(&plugin, Path::new("allowed/notes.txt"), b"updated").unwrap();

        assert_eq!(
            fs::read(plugin.install_dir.join("allowed/notes.txt")).unwrap(),
            b"updated"
        );
    }

    #[test]
    fn traversal_write_is_denied() {
        let (temp, plugin) = write_plugin_fixture();
        let outside = temp.path().join("outside.txt");

        assert!(write_file(&plugin, Path::new("allowed/../outside.txt"), b"no").is_err());
        assert!(!outside.exists());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn nested_link_cannot_create_a_file_outside_the_permission_root() {
        let (temp, plugin) = write_plugin_fixture();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        create_directory_link(&plugin.install_dir.join("allowed/shortcut"), &outside).unwrap();

        assert!(write_file(&plugin, Path::new("allowed/shortcut/new-file.txt"), b"no").is_err());
        assert!(!outside.join("new-file.txt").exists());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn nested_parent_swap_after_authorization_cannot_redirect_a_write() {
        let (temp, plugin) = write_plugin_fixture();
        let nested = plugin.install_dir.join("allowed/nested");
        let moved_nested = plugin.install_dir.join("allowed/nested-before-swap");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let authorized =
            authorize_write_path(&plugin, Path::new("allowed/nested/payload.txt")).unwrap();

        fs::rename(&nested, &moved_nested).unwrap();
        create_directory_link(&nested, &outside).unwrap();

        assert!(secure_write_file(&authorized.root, &authorized.relative, b"no").is_err());
        assert!(!outside.join("payload.txt").exists());
        assert!(!moved_nested.join("payload.txt").exists());
    }

    #[test]
    fn hard_link_target_is_replaced_without_changing_its_other_name() {
        let (temp, plugin) = write_plugin_fixture();
        let outside = temp.path().join("outside.txt");
        let allowed_link = plugin.install_dir.join("allowed/linked.txt");
        fs::write(&outside, b"keep this outside data").unwrap();
        fs::hard_link(&outside, &allowed_link).unwrap();

        write_file(&plugin, Path::new("allowed/linked.txt"), b"plugin data").unwrap();

        assert_eq!(fs::read(&outside).unwrap(), b"keep this outside data");
        assert_eq!(fs::read(&allowed_link).unwrap(), b"plugin data");
    }

    #[cfg(unix)]
    #[test]
    fn permission_root_ancestor_swap_cannot_redirect_an_authorized_write() {
        let (temp, plugin) = write_plugin_fixture();
        let authorized = authorize_write_path(&plugin, Path::new("allowed/payload.txt")).unwrap();
        let moved_install = temp.path().join("plugin-before-swap");
        let outside = temp.path().join("outside");
        fs::create_dir_all(outside.join("allowed")).unwrap();
        fs::write(outside.join("allowed/payload.txt"), b"outside stays").unwrap();

        fs::rename(&plugin.install_dir, &moved_install).unwrap();
        create_directory_link(&plugin.install_dir, &outside)
            .expect("create a directory link at the swapped root");
        secure_write_file(&authorized.root, &authorized.relative, b"authorized data")
            .expect("the pinned permission directory remains writable after its path moves");

        assert_eq!(
            fs::read(outside.join("allowed/payload.txt")).unwrap(),
            b"outside stays",
            "the swapped path must never redirect the write outside"
        );
        assert_eq!(
            fs::read(moved_install.join("allowed/payload.txt")).unwrap(),
            b"authorized data",
            "a pinned permission root may still be written after its name moves"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_nested_write_ancestors_pinned_until_publication() {
        let (temp, plugin) = write_plugin_fixture();
        let nested = plugin.install_dir.join("allowed/nested");
        let deeper = nested.join("deeper");
        let moved_nested = plugin.install_dir.join("allowed/nested-before-swap");
        let outside = temp.path().join("outside/nested/deeper");
        fs::create_dir_all(&deeper).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("payload.txt"), b"outside stays").unwrap();

        let authorized =
            authorize_write_path(&plugin, Path::new("allowed/nested/deeper/payload.txt")).unwrap();
        let target = open_write_target(&authorized.root, &authorized.relative).unwrap();
        let error = fs::rename(&nested, &moved_nested)
            .expect_err("an open intermediate ancestor must not be renamed on Windows");
        assert_eq!(
            error.kind(),
            io::ErrorKind::PermissionDenied,
            "the nested ancestor move must fail while its path guard is open"
        );
        drop(target);

        secure_write_file(&authorized.root, &authorized.relative, b"authorized data")
            .expect("the original nested target must remain writable");
        assert_eq!(
            fs::read(nested.join("deeper/payload.txt")).unwrap(),
            b"authorized data"
        );
        assert_eq!(
            fs::read(outside.join("payload.txt")).unwrap(),
            b"outside stays"
        );
        assert!(!moved_nested.exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_permission_ancestors_pinned_until_write_finishes() {
        let (temp, plugin) = write_plugin_fixture();
        let authorized = authorize_write_path(&plugin, Path::new("allowed/payload.txt")).unwrap();
        let moved_install = temp.path().join("plugin-before-swap");
        let outside = temp.path().join("outside");
        fs::create_dir_all(outside.join("allowed")).unwrap();
        fs::write(outside.join("allowed/payload.txt"), b"outside stays").unwrap();

        let error = fs::rename(&plugin.install_dir, &moved_install)
            .expect_err("an open permission ancestor must not be renamed on Windows");
        assert_eq!(
            error.kind(),
            io::ErrorKind::PermissionDenied,
            "the ancestor move must fail because secure publication pins its path"
        );

        secure_write_file(&authorized.root, &authorized.relative, b"authorized data")
            .expect("the original authorized handle must remain writable");
        assert_eq!(
            fs::read(plugin.install_dir.join("allowed/payload.txt")).unwrap(),
            b"authorized data"
        );
        assert_eq!(
            fs::read(outside.join("allowed/payload.txt")).unwrap(),
            b"outside stays"
        );
        assert!(!moved_install.exists());
    }
}
