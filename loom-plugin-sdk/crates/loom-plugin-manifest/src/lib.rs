//! # loom-plugin-manifest
//!
//! Schema, validation, and version-compatibility logic for Loom plugin package
//! manifests.
//!
//! A plugin package is a ZIP archive with the `.loomplugin` extension (see the
//! [`PluginPackageLayout`] documentation below). This crate defines the
//! `manifest.json` document that every package must contain at its root, the
//! validation rules that turn a raw JSON string into a trustworthy
//! [`PluginManifest`], and the host/plugin API-version negotiation helpers.
//!
//! This crate performs no filesystem access, no networking, and no WebAssembly
//! execution. It is pure data transformation plus validation, which makes it
//! safe to reuse from sandboxed or untrusted contexts.
//!
//! The WASI runtime that would execute a validated plugin is intentionally out
//! of scope for this milestone. See `docs/rfcs/RFC-0009-plugin-abi-and-sandboxing.md`
//! and `ROADMAP.md` for the execution story.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// Current supported value of the `manifest_version` field.
pub const SUPPORTED_MANIFEST_VERSION: u32 = 1;

/// Upper bound (in bytes) for a manifest document accepted by
/// [`parse_manifest`]. Larger documents are rejected with
/// [`ManifestError::TooLarge`] to keep hosts safe from oversized inputs.
pub const DEFAULT_MAX_MANIFEST_BYTES: usize = 1024 * 1024;

/// Maximum length of a `plugin_id` (regex `^[a-z0-9][a-z0-9-]{0,62}$`).
pub const MAX_PLUGIN_ID_LEN: usize = 63;

/// The kind of entry point a plugin declares. Each variant maps to a family of
/// Loom extension points; a plugin may declare exactly one entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// A named command invocable from menus, the command palette, or scripts.
    Command,
    /// A file importer.
    Importer,
    /// A file exporter.
    Exporter,
    /// An effect applied to pixels, audio, or text.
    Effect,
    /// A procedural generator.
    Generator,
    /// An inspector that reports on a document or selection.
    Inspector,
    /// A metadata provider (codec, camera, lens, ...).
    MetadataProvider,
    /// A computer-vision provider registered with Loom Vision.
    VisionProvider,
    /// A document processor (import/export pipelines, batch actions).
    DocumentProcessor,
    /// A media processor (decode/encode/transform jobs).
    MediaProcessor,
}

impl EntryKind {
    /// Parse a kebab-case entry kind string, e.g. `"metadata-provider"`.
    ///
    /// Matching is case-sensitive and expects exactly the canonical names
    /// produced by [`EntryKind::as_str`].
    pub fn parse(s: &str) -> Option<EntryKind> {
        match s {
            "command" => Some(EntryKind::Command),
            "importer" => Some(EntryKind::Importer),
            "exporter" => Some(EntryKind::Exporter),
            "effect" => Some(EntryKind::Effect),
            "generator" => Some(EntryKind::Generator),
            "inspector" => Some(EntryKind::Inspector),
            "metadata-provider" => Some(EntryKind::MetadataProvider),
            "vision-provider" => Some(EntryKind::VisionProvider),
            "document-processor" => Some(EntryKind::DocumentProcessor),
            "media-processor" => Some(EntryKind::MediaProcessor),
            _ => None,
        }
    }

    /// Canonical kebab-case name of this entry kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::Command => "command",
            EntryKind::Importer => "importer",
            EntryKind::Exporter => "exporter",
            EntryKind::Effect => "effect",
            EntryKind::Generator => "generator",
            EntryKind::Inspector => "inspector",
            EntryKind::MetadataProvider => "metadata-provider",
            EntryKind::VisionProvider => "vision-provider",
            EntryKind::DocumentProcessor => "document-processor",
            EntryKind::MediaProcessor => "media-processor",
        }
    }
}

/// A capability a plugin may request. Capabilities are coarse permission
/// grants; fine-grained path scoping is expressed separately via
/// [`Permission`] entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    /// Read files within granted path prefixes.
    ReadFile,
    /// Write files within granted path prefixes.
    WriteFile,
    /// List directories within granted path prefixes.
    ReadDir,
    /// Create directories within granted path prefixes.
    WriteDir,
    /// Make HTTP(S) requests (requires `network` resource limit too).
    HttpRequest,
    /// Read the clipboard.
    ClipboardRead,
    /// Write to the clipboard.
    ClipboardWrite,
    /// Run local computer-vision inference through Loom Vision.
    VisionInference,
    /// Access the host's per-plugin temporary directory.
    AccessTemp,
    /// Persist state across invocations in the host-managed state directory.
    PersistState,
}

impl Capability {
    /// Parse a kebab-case capability string, e.g. `"read-file"`.
    pub fn parse(s: &str) -> Option<Capability> {
        match s {
            "read-file" => Some(Capability::ReadFile),
            "write-file" => Some(Capability::WriteFile),
            "read-dir" => Some(Capability::ReadDir),
            "write-dir" => Some(Capability::WriteDir),
            "http-request" => Some(Capability::HttpRequest),
            "clipboard-read" => Some(Capability::ClipboardRead),
            "clipboard-write" => Some(Capability::ClipboardWrite),
            "vision-inference" => Some(Capability::VisionInference),
            "access-temp" => Some(Capability::AccessTemp),
            "persist-state" => Some(Capability::PersistState),
            _ => None,
        }
    }

    /// Canonical kebab-case name of this capability.
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::ReadFile => "read-file",
            Capability::WriteFile => "write-file",
            Capability::ReadDir => "read-dir",
            Capability::WriteDir => "write-dir",
            Capability::HttpRequest => "http-request",
            Capability::ClipboardRead => "clipboard-read",
            Capability::ClipboardWrite => "clipboard-write",
            Capability::VisionInference => "vision-inference",
            Capability::AccessTemp => "access-temp",
            Capability::PersistState => "persist-state",
        }
    }
}

impl<'de> Deserialize<'de> for Capability {
    /// Deserialize a capability from its canonical kebab-case string.
    ///
    /// Unknown names produce a custom deserialization error whose message
    /// begins with the `unknown capability: ` prefix, which
    /// [`parse_manifest`] reclassifies as
    /// [`ManifestError::UnknownCapability`].
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str as Deserialize<'de>>::deserialize(deserializer)?;
        Capability::parse(s).ok_or_else(|| D::Error::custom(format!("unknown capability: {s}")))
    }
}

/// A fine-grained permission entry: access to `resource` under an optional
/// path prefix, in a given `mode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    /// Resource family the permission applies to, e.g. `"file"`, `"dir"`,
    /// `"temp"`, `"state"`, `"clipboard"`, `"vision"`, or `"network"`.
    pub resource: String,
    /// Access mode: one of `"read"`, `"write"`, `"exec"`, `"create"`.
    pub mode: String,
    /// Path prefix (relative or absolute) the permission is scoped to.
    /// `None` grants no path-based access (e.g. clipboard or network).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
}

/// Resource consumption limits enforced by the host around plugin execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum guest memory the runtime may allocate per instance, in bytes.
    pub max_memory_bytes: u64,
    /// Maximum cumulative bytes a plugin may read or write on the filesystem.
    pub max_fs_bytes: u64,
    /// Maximum number of filesystem entries a plugin may access.
    pub max_fs_entries: u64,
    /// Maximum CPU time per invocation, in milliseconds.
    pub max_cpu_ms_per_call: u64,
    /// Whether network access is permitted at all (required for
    /// [`Capability::HttpRequest`]).
    pub network: bool,
}

/// The entry point a host loads to execute this plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Kind of entry point.
    pub kind: EntryKind,
    /// Path of the WASM module inside the package (e.g. `"module.wasm"`).
    pub wasm_module: String,
    /// Name of the exported guest function the host calls.
    pub function: String,
}

/// A validated Loom plugin manifest.
///
/// Constructed by [`parse_manifest`] or by hand followed by
/// [`PluginManifest::validate`]. The struct itself is plain data; all
/// semantic rules live in [`PluginManifest::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Package format version; must equal [`SUPPORTED_MANIFEST_VERSION`].
    pub manifest_version: u32,
    /// Stable identifier, matching `^[a-z0-9][a-z0-9-]{0,62}$`.
    pub plugin_id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Short description of what the plugin does.
    #[serde(default)]
    pub description: String,
    /// Plugin version, semver-style dotted string.
    pub version: String,
    /// Author of the plugin.
    #[serde(default)]
    pub author: String,
    /// SPDX license expression for the plugin itself.
    pub license: String,
    /// The entry point host loads.
    pub entry: EntryPoint,
    /// Capabilities the plugin may exercise.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Fine-grained permissions granted to the plugin.
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// Lowest plugin API version this plugin requires.
    pub api_min_version: String,
    /// Highest plugin API version this plugin supports.
    pub api_max_version: String,
    /// Resource limits the host must enforce.
    pub resource_limits: ResourceLimits,
}

impl PluginManifest {
    /// Validate every semantic rule defined by the package format.
    ///
    /// Checks run in this order:
    ///
    /// 1. `manifest_version` must equal [`SUPPORTED_MANIFEST_VERSION`]
    ///    (`UnsupportedVersion`).
    /// 2. `plugin_id` must match `^[a-z0-9][a-z0-9-]{0,62}$` (`InvalidId`).
    /// 3. `name` and `version` must be non-empty (`Malformed`).
    /// 4. `api_min_version` / `api_max_version` must be non-empty and
    ///    `api_max_version >= api_min_version` (`Malformed`).
    /// 5. `entry.wasm_module` must be non-empty, not absolute, and free of
    ///    `..` / `.` path components (`Malformed`).
    /// 6. `entry.function` must be non-empty (`Malformed`).
    /// 7. Every `Permission.mode` must be one of `read|write|exec|create`
    ///    (`Malformed`).
    /// 8. Every resource limit must be `> 0` (`Malformed`).
    /// 9. A permission whose resource is `"network"` requires the
    ///    [`Capability::HttpRequest`] capability (`Malformed`).
    ///
    /// Capability and permission *names* are enforced by the typed
    /// [`Capability`] deserializer at parse time, so they cannot be invalid at
    /// this point.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.manifest_version != SUPPORTED_MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedVersion {
                found: self.manifest_version,
                supported: SUPPORTED_MANIFEST_VERSION,
            });
        }
        if !valid_plugin_id(&self.plugin_id) {
            return Err(ManifestError::InvalidId(self.plugin_id.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Malformed("name must not be empty".into()));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Malformed("version must not be empty".into()));
        }
        if self.api_min_version.trim().is_empty() || self.api_max_version.trim().is_empty() {
            return Err(ManifestError::Malformed(
                "api_min_version and api_max_version must not be empty".into(),
            ));
        }
        if compare_versions(&self.api_min_version, &self.api_max_version) == Ordering::Greater {
            return Err(ManifestError::Malformed(format!(
                "api_max_version ({}) must be >= api_min_version ({})",
                self.api_max_version, self.api_min_version
            )));
        }
        if !valid_module_path(&self.entry.wasm_module) {
            return Err(ManifestError::Malformed(format!(
                "invalid wasm_module path: {:?} (must be relative, without '..' or '.' components)",
                self.entry.wasm_module
            )));
        }
        if self.entry.function.trim().is_empty() {
            return Err(ManifestError::Malformed(
                "entry.function must not be empty".into(),
            ));
        }
        for permission in &self.permissions {
            if !valid_permission_mode(&permission.mode) {
                return Err(ManifestError::Malformed(format!(
                    "invalid permission mode {:?} for resource {:?} \
                     (expected read, write, exec, or create)",
                    permission.mode, permission.resource
                )));
            }
        }
        let limits = &self.resource_limits;
        if limits.max_memory_bytes == 0
            || limits.max_fs_bytes == 0
            || limits.max_fs_entries == 0
            || limits.max_cpu_ms_per_call == 0
        {
            return Err(ManifestError::Malformed(
                "resource limits must all be > 0".into(),
            ));
        }
        let requests_network = self
            .permissions
            .iter()
            .any(|p| p.resource.eq_ignore_ascii_case("network"));
        if requests_network && !self.capabilities.contains(&Capability::HttpRequest) {
            return Err(ManifestError::Malformed(
                "a network permission requires the http-request capability".into(),
            ));
        }
        Ok(())
    }
}

/// Errors produced while parsing or validating a plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The document is not well-formed JSON or violates the schema in a
    /// non-specific way. The payload describes the problem.
    Malformed(String),
    /// A capability name in `capabilities` is not a known capability.
    UnknownCapability(String),
    /// `manifest_version` is not supported by this host.
    UnsupportedVersion {
        /// Version found in the document.
        found: u32,
        /// Highest version this crate supports.
        supported: u32,
    },
    /// `plugin_id` violates `^[a-z0-9][a-z0-9-]{0,62}$`.
    InvalidId(String),
    /// A required field is absent from the document.
    MissingField(String),
    /// The document exceeds the caller's size limit.
    TooLarge,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Malformed(msg) => write!(f, "malformed manifest: {msg}"),
            ManifestError::UnknownCapability(cap) => write!(f, "unknown capability: {cap}"),
            ManifestError::UnsupportedVersion { found, supported } => {
                write!(
                    f,
                    "unsupported manifest_version {found} (supported: {supported})"
                )
            }
            ManifestError::InvalidId(id) => write!(
                f,
                "invalid plugin_id {id:?} (must match ^[a-z0-9][a-z0-9-]{{0,62}}$)"
            ),
            ManifestError::MissingField(field) => write!(f, "missing required field: {field}"),
            ManifestError::TooLarge => write!(f, "manifest exceeds the size limit"),
        }
    }
}

impl Error for ManifestError {}

/// Translate a `serde_json::Error` into a [`ManifestError`].
///
/// `serde_json` does not expose the original custom deserialization error
/// object, so structured outcomes are recovered from the message prefixes
/// this crate itself generates (`unknown capability: `) and from serde's
/// stable `missing field \`name\`` message format. Anything else becomes a
/// generic [`ManifestError::Malformed`].
fn classify_json_error(error: serde_json::Error) -> ManifestError {
    let message = error.to_string();
    if let Some(rest) = message.strip_prefix("unknown capability: ") {
        let cap = rest.split(" at line ").next().unwrap_or(rest);
        return ManifestError::UnknownCapability(cap.to_string());
    }
    if let Some(field) = missing_field_from_message(&message) {
        return ManifestError::MissingField(field);
    }
    ManifestError::Malformed(message)
}

/// Parse and validate a manifest document.
///
/// The document must not exceed [`DEFAULT_MAX_MANIFEST_BYTES`] bytes; larger
/// inputs yield [`ManifestError::TooLarge`]. Use
/// [`parse_manifest_with_limit`] for a custom bound.
pub fn parse_manifest(json: &str) -> Result<PluginManifest, ManifestError> {
    parse_manifest_with_limit(json, DEFAULT_MAX_MANIFEST_BYTES)
}

/// Parse and validate a manifest document with a custom size bound in bytes.
pub fn parse_manifest_with_limit(
    json: &str,
    max_bytes: usize,
) -> Result<PluginManifest, ManifestError> {
    if json.len() > max_bytes {
        return Err(ManifestError::TooLarge);
    }
    let manifest: PluginManifest = serde_json::from_str(json).map_err(classify_json_error)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Extract the field name from serde's `missing field \`name\`` messages.
fn missing_field_from_message(msg: &str) -> Option<String> {
    msg.strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// Hand-rolled check for `^[a-z0-9][a-z0-9-]{0,62}$` — no regex dependency.
fn valid_plugin_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_PLUGIN_ID_LEN {
        return false;
    }
    if !(bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit()) {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A module path is safe when it is relative and contains no `.` or `..`
/// components and no backslashes (defense against path-slip on extraction).
fn valid_module_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return false;
    }
    !path.split('/').any(|c| c == ".." || c == ".")
}

/// Permission modes the schema accepts.
fn valid_permission_mode(mode: &str) -> bool {
    matches!(mode, "read" | "write" | "exec" | "create")
}

/// Split a version string into `(major, minor, patch)`.
///
/// Missing parts and non-numeric parts are treated as `0`, so `"0.1"`,
/// `"0.1.0"` and `"0.1.0-alpha"` all compare as `(0, 1, 0)`.
fn parse_version(version: &str) -> (u64, u64, u64) {
    let mut parts = version.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Compare two dotted numeric version strings (semver-ish; see
/// [`parse_version`] for tolerance rules).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    parse_version(a).cmp(&parse_version(b))
}

/// Check that a single plugin-API version falls within a host's supported
/// range: `host_min <= manifest_api <= host_max`.
///
/// This is the version-negotiation entry point between a plugin manifest and
/// a host runtime. Hosts call it before executing a plugin; the host crate
/// also uses it to reject incompatible packages at install time.
pub fn version_compatible(manifest_api: &str, host_min: &str, host_max: &str) -> bool {
    compare_versions(manifest_api, host_min) != Ordering::Less
        && compare_versions(manifest_api, host_max) != Ordering::Greater
}

/// # Plugin package layout
///
/// A `.loomplugin` package is a ZIP archive with the following expected
/// layout:
///
/// ```text
/// manifest.json     required  manifest document (this crate's schema)
/// module.wasm       required  WASM32-WASI module referenced by entry.wasm_module
/// assets/           optional  plugin-owned data files
/// ```
///
/// Constraints enforced by consumers (`loom-plugin-host`):
///
/// * Entry names must be relative, without `..` or `.` components.
/// * Symlink entries are rejected.
/// * `manifest.json` must sit at the archive root.
/// * The wasm module named by `entry.wasm_module` must be present.
///
/// The package is validated as a whole before anything is extracted; a
/// hostile archive never touches the store directory.
pub struct PluginPackageLayout;

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> &'static str {
        r#"{
            "manifest_version": 1,
            "plugin_id": "demo-actions",
            "name": "Demo Actions",
            "description": "A minimal demo plugin.",
            "version": "0.1.0",
            "author": "Loom",
            "license": "MIT OR Apache-2.0",
            "entry": {
                "kind": "command",
                "wasm_module": "module.wasm",
                "function": "loom_plugin_invoke"
            },
            "capabilities": ["read-file", "write-file"],
            "permissions": [
                { "resource": "file", "mode": "read", "path_prefix": "assets" }
            ],
            "api_min_version": "0.1.0",
            "api_max_version": "0.9.0",
            "resource_limits": {
                "max_memory_bytes": 33554432,
                "max_fs_bytes": 10485760,
                "max_fs_entries": 1024,
                "max_cpu_ms_per_call": 5000,
                "network": false
            }
        }"#
    }

    #[test]
    fn valid_manifest_parses_and_validates() {
        let manifest = parse_manifest(valid_json()).expect("valid manifest should parse");
        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.plugin_id, "demo-actions");
        assert_eq!(manifest.entry.kind, EntryKind::Command);
        assert_eq!(manifest.entry.wasm_module, "module.wasm");
        assert_eq!(manifest.capabilities.len(), 2);
        assert!(manifest.capabilities.contains(&Capability::ReadFile));
        assert_eq!(manifest.permissions.len(), 1);
        assert_eq!(manifest.resource_limits.max_memory_bytes, 33_554_432);
        assert!(!manifest.resource_limits.network);
    }

    #[test]
    fn missing_required_field_is_reported() {
        let json = valid_json().replace("\"plugin_id\": \"demo-actions\",", "");
        let err = parse_manifest(&json).unwrap_err();
        assert_eq!(err, ManifestError::MissingField("plugin_id".into()));
    }

    #[test]
    fn missing_entry_field_is_reported() {
        let json = valid_json().replace("\"wasm_module\": \"module.wasm\",", "");
        let err = parse_manifest(&json).unwrap_err();
        assert_eq!(err, ManifestError::MissingField("wasm_module".into()));
    }

    #[test]
    fn unknown_capability_is_reported() {
        let json = valid_json().replace(
            "\"read-file\", \"write-file\"",
            "\"read-file\", \"delete-everything\"",
        );
        let err = parse_manifest(&json).unwrap_err();
        assert_eq!(
            err,
            ManifestError::UnknownCapability("delete-everything".into())
        );
    }

    #[test]
    fn unknown_entry_kind_is_malformed() {
        let json = valid_json().replace("\"kind\": \"command\"", "\"kind\": \"hologram\"");
        let err = parse_manifest(&json).unwrap_err();
        assert!(matches!(err, ManifestError::Malformed(_)), "got {err:?}");
    }

    #[test]
    fn unsupported_manifest_version() {
        let json = valid_json().replace("\"manifest_version\": 1", "\"manifest_version\": 2");
        let err = parse_manifest(&json).unwrap_err();
        assert_eq!(
            err,
            ManifestError::UnsupportedVersion {
                found: 2,
                supported: 1
            }
        );
    }

    #[test]
    fn invalid_plugin_ids_are_rejected() {
        let cases = [
            "Demo",   // uppercase
            "demo_2", // underscore
            "_demo",  // leading underscore
            "0demo!", // punctuation
            "",       // empty
            "demo.",  // trailing dot
            "-demo",  // leading dash
        ];
        // Start from the valid case: only the one below must pass.
        assert!(valid_plugin_id("a"));
        for case in cases {
            assert!(!valid_plugin_id(case), "expected invalid: {case:?}");
        }
        let json = valid_json().replace("demo-actions", "Demo_2");
        let err = parse_manifest(&json).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidId(_)), "got {err:?}");
    }

    #[test]
    fn plugin_id_length_boundary() {
        assert!(valid_plugin_id(&"a".repeat(63)));
        assert!(!valid_plugin_id(&"a".repeat(64)));
        assert!(valid_plugin_id("z9"));
        assert!(valid_plugin_id("a-b-c"));
        assert!(!valid_plugin_id("aB"));
    }

    #[test]
    fn api_range_inversion_is_rejected() {
        let json = valid_json().replace(
            "\"api_min_version\": \"0.1.0\",\n            \"api_max_version\": \"0.9.0\"",
            "\"api_min_version\": \"0.9.0\",\n            \"api_max_version\": \"0.1.0\"",
        );
        let err = parse_manifest(&json).unwrap_err();
        assert!(matches!(err, ManifestError::Malformed(msg) if msg.contains("api_max_version")));
    }

    #[test]
    fn unsafe_module_paths_are_rejected() {
        for bad in [
            "../evil.wasm",
            "/etc/evil.wasm",
            "a/b/../../evil.wasm",
            "a\\b.wasm",
            "",
        ] {
            // JSON-escape the path so it cannot smuggle escape sequences
            // into the document.
            let escaped = serde_json::to_string(bad).unwrap();
            let json = valid_json().replace(
                "\"wasm_module\": \"module.wasm\"",
                &format!("\"wasm_module\": {escaped}"),
            );
            let err = parse_manifest(&json).unwrap_err();
            assert!(
                matches!(&err, ManifestError::Malformed(msg) if msg.contains("wasm_module")),
                "path {bad:?} got {err:?}"
            );
        }
    }

    #[test]
    fn empty_name_or_version_is_rejected() {
        let json = valid_json().replace("\"name\": \"Demo Actions\"", "\"name\": \"\"");
        assert!(matches!(
            parse_manifest(&json).unwrap_err(),
            ManifestError::Malformed(msg) if msg.contains("name")
        ));
        let json = valid_json().replace("\"version\": \"0.1.0\"", "\"version\": \"  \"");
        assert!(matches!(
            parse_manifest(&json).unwrap_err(),
            ManifestError::Malformed(msg) if msg.contains("version")
        ));
    }

    #[test]
    fn invalid_permission_mode_is_rejected() {
        let json = valid_json().replace("\"mode\": \"read\"", "\"mode\": \"rm -rf\"");
        let err = parse_manifest(&json).unwrap_err();
        assert!(matches!(err, ManifestError::Malformed(msg) if msg.contains("permission mode")));
    }

    #[test]
    fn zero_resource_limits_are_rejected() {
        let json = valid_json().replace("33554432", "0");
        let err = parse_manifest(&json).unwrap_err();
        assert!(matches!(err, ManifestError::Malformed(msg) if msg.contains("resource limits")));
    }

    #[test]
    fn network_permission_requires_http_capability() {
        let json = valid_json().replace(
            "\"permissions\": [\n                { \"resource\": \"file\", \"mode\": \"read\", \"path_prefix\": \"assets\" }\n            ]",
            "\"permissions\": [\n                { \"resource\": \"network\", \"mode\": \"read\" }\n            ]",
        );
        let err = parse_manifest(&json).unwrap_err();
        assert!(matches!(err, ManifestError::Malformed(msg) if msg.contains("network permission")));

        // With the http-request capability present it must pass.
        let json = valid_json().replace(
            "\"capabilities\": [\"read-file\", \"write-file\"]",
            "\"capabilities\": [\"read-file\", \"http-request\"]",
        );
        let manifest = parse_manifest(&json).expect("network permission with capability is valid");
        assert!(manifest.capabilities.contains(&Capability::HttpRequest));
    }

    #[test]
    fn oversized_document_is_rejected() {
        let mut big = String::with_capacity(200);
        big.push_str(valid_json());
        big.push_str(&" ".repeat(64));
        let err = parse_manifest_with_limit(&big, 64).unwrap_err();
        assert_eq!(err, ManifestError::TooLarge);
    }

    #[test]
    fn malformed_json_is_reported() {
        let err = parse_manifest("{ not json").unwrap_err();
        assert!(matches!(err, ManifestError::Malformed(_)));
    }

    #[test]
    fn version_compatible_matrix() {
        // Exact boundaries are inclusive.
        assert!(version_compatible("0.1.0", "0.1.0", "0.9.0"));
        assert!(version_compatible("0.9.0", "0.1.0", "0.9.0"));
        // Inside the range.
        assert!(version_compatible("0.5.2", "0.1.0", "0.9.0"));
        // Outside the range.
        assert!(!version_compatible("0.0.9", "0.1.0", "0.9.0"));
        assert!(!version_compatible("0.10.0", "0.1.0", "0.9.0"));
        assert!(!version_compatible("1.0.0", "0.1.0", "0.9.0"));
        // Missing parts count as 0: "0.1" == "0.1.0".
        assert!(version_compatible("0.1", "0.1.0", "0.9.0"));
        assert!(!version_compatible("1", "0.1.0", "0.9.0"));
        // Non-numeric parts count as 0 (documented tolerance).
        assert!(version_compatible("0.1.0-alpha", "0.1.0", "0.9.0"));
        // Empty strings compare as 0.0.0.
        assert!(!version_compatible("", "0.1.0", "0.9.0"));
        // Minor increments matter.
        assert!(!version_compatible("0.9.9", "0.1.0", "0.9.0"));
        assert!(version_compatible("0.9.9", "0.1.0", "0.10.0"));
    }

    #[test]
    fn compare_versions_orderings() {
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.1", "0.1.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.0", "0.1.1"), Ordering::Less);
        assert_eq!(compare_versions("1.0.0", "0.999.999"), Ordering::Greater);
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("junk", "0.0.0"), Ordering::Equal);
    }

    #[test]
    fn entry_kind_parse_round_trip() {
        for kind in [
            EntryKind::Command,
            EntryKind::Importer,
            EntryKind::Exporter,
            EntryKind::Effect,
            EntryKind::Generator,
            EntryKind::Inspector,
            EntryKind::MetadataProvider,
            EntryKind::VisionProvider,
            EntryKind::DocumentProcessor,
            EntryKind::MediaProcessor,
        ] {
            assert_eq!(EntryKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EntryKind::parse("Command"), None);
        assert_eq!(EntryKind::parse(""), None);
    }

    #[test]
    fn capability_parse_round_trip() {
        for cap in [
            Capability::ReadFile,
            Capability::WriteFile,
            Capability::ReadDir,
            Capability::WriteDir,
            Capability::HttpRequest,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::VisionInference,
            Capability::AccessTemp,
            Capability::PersistState,
        ] {
            assert_eq!(Capability::parse(cap.as_str()), Some(cap));
        }
        assert_eq!(Capability::parse("read_file"), None);
        assert_eq!(Capability::parse("READ-FILE"), None);
    }

    #[test]
    fn serialize_then_parse_round_trip() {
        let original = parse_manifest(valid_json()).expect("valid fixture");
        let serialized = serde_json::to_string(&original).expect("serialize");
        let reparsed = parse_manifest(&serialized).expect("reparse");
        assert_eq!(original, reparsed);
    }

    #[test]
    fn serialized_form_is_canonical_kebab_case() {
        let manifest = parse_manifest(valid_json()).unwrap();
        let serialized = serde_json::to_string(&manifest).unwrap();
        assert!(serialized.contains("\"read-file\""));
        assert!(serialized.contains("\"write-file\""));
        assert!(serialized.contains("\"kind\":\"command\""));
        assert!(!serialized.contains("ReadFile"));
    }

    #[test]
    fn permission_without_path_prefix_is_allowed() {
        let json = valid_json().replace(
            "\"permissions\": [\n                { \"resource\": \"file\", \"mode\": \"read\", \"path_prefix\": \"assets\" }\n            ]",
            "\"permissions\": [\n                { \"resource\": \"clipboard\", \"mode\": \"read\" }\n            ]",
        );
        let manifest = parse_manifest(&json).expect("permission without path_prefix is valid");
        assert_eq!(manifest.permissions[0].path_prefix, None);
    }

    #[test]
    fn defaulted_fields_are_optional() {
        let json = valid_json().replace(
            ",\n            \"description\": \"A minimal demo plugin.\"",
            "",
        );
        let manifest = parse_manifest(&json).expect("description is optional");
        assert_eq!(manifest.description, "");
    }
}
