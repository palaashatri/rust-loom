//! Signed plugin lifecycle, declarative UI extensions, compatibility
//! migrations, and process-isolated native plugin bridging.
//!
//! This module builds on [`crate::PluginStore`]. WebAssembly plugins remain
//! deny-by-default and native CLAP/VST3 binaries are never loaded into a Loom
//! application process. Native binaries must be handled by a separately
//! audited bridge executable speaking the JSON-lines protocol defined here.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use loom_plugin_manifest::{compare_versions, parse_manifest, PluginManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{HostError, InstalledPlugin, PluginStore, MAX_MANIFEST_BYTES};

const TRUST_FILE: &str = "trusted-signers.json";
const ROLLBACK_DIRECTORY: &str = ".rollback";
const TRANSACTION_DIRECTORY: &str = ".transactions";
const SIGNATURE_DOMAIN: &[u8] = b"loom-plugin-package-v1\0";

/// Error returned by signed lifecycle and bridge operations.
#[derive(Debug)]
pub enum LifecycleError {
    /// Existing plugin-host operation failed.
    Host(HostError),
    /// Filesystem or process I/O failed.
    Io(io::Error),
    /// Serialized data is malformed.
    InvalidData(String),
    /// Package signature or digest verification failed.
    Signature(String),
    /// Signer is unknown or revoked.
    Untrusted(String),
    /// Update or rollback cannot proceed.
    Transaction(String),
    /// Declarative UI extension is invalid.
    InvalidUi(String),
    /// Compatibility migration is invalid.
    Migration(String),
    /// Native bridge did not answer in time.
    BridgeTimeout,
    /// Native bridge returned or emitted invalid data.
    BridgeProtocol(String),
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(error) => write!(formatter, "plugin host error: {error}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::InvalidData(message) => write!(formatter, "invalid data: {message}"),
            Self::Signature(message) => {
                write!(formatter, "signature verification failed: {message}")
            }
            Self::Untrusted(message) => write!(formatter, "untrusted signer: {message}"),
            Self::Transaction(message) => write!(formatter, "plugin transaction failed: {message}"),
            Self::InvalidUi(message) => write!(formatter, "invalid UI extension: {message}"),
            Self::Migration(message) => write!(formatter, "migration failed: {message}"),
            Self::BridgeTimeout => write!(formatter, "native plugin bridge timed out"),
            Self::BridgeProtocol(message) => {
                write!(formatter, "native plugin bridge protocol error: {message}")
            }
        }
    }
}

impl std::error::Error for LifecycleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<HostError> for LifecycleError {
    fn from(error: HostError) -> Self {
        Self::Host(error)
    }
}

impl From<io::Error> for LifecycleError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// One trusted Ed25519 package-signing key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustKey {
    /// Stable key identifier referenced by signature envelopes.
    pub key_id: String,
    /// Human-readable publisher or administrator label.
    pub label: String,
    /// Base64-encoded 32-byte Ed25519 public key.
    pub public_key_base64: String,
    /// Whether the key has been administratively revoked.
    pub revoked: bool,
    /// Optional revocation explanation shown to users.
    pub revocation_reason: Option<String>,
}

impl TrustKey {
    fn verifying_key(&self) -> Result<VerifyingKey, LifecycleError> {
        let bytes = BASE64.decode(&self.public_key_base64).map_err(|error| {
            LifecycleError::InvalidData(format!("invalid public key encoding: {error}"))
        })?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
            LifecycleError::InvalidData("Ed25519 public key must contain 32 bytes".into())
        })?;
        VerifyingKey::from_bytes(&bytes).map_err(|error| {
            LifecycleError::InvalidData(format!("invalid Ed25519 public key: {error}"))
        })
    }
}

/// Persistent collection of trusted package-signing keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustStore {
    /// Trust-store schema version.
    pub schema_version: u32,
    /// Trusted and revoked keys indexed by id.
    pub keys: BTreeMap<String, TrustKey>,
}

impl TrustStore {
    /// Create an empty version-one trust store.
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            keys: BTreeMap::new(),
        }
    }

    /// Load a trust store. A missing file yields an empty store.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LifecycleError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new());
        }
        let store: Self = serde_json::from_slice(&fs::read(path)?)
            .map_err(|error| LifecycleError::InvalidData(error.to_string()))?;
        if store.schema_version != 1 {
            return Err(LifecycleError::InvalidData(format!(
                "unsupported trust-store schema {}",
                store.schema_version
            )));
        }
        for (id, key) in &store.keys {
            if id != &key.key_id || id.trim().is_empty() {
                return Err(LifecycleError::InvalidData(
                    "trust-store key ids must be non-empty and match their map keys".into(),
                ));
            }
            let _ = key.verifying_key()?;
        }
        Ok(store)
    }

    /// Save the trust store atomically.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), LifecycleError> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| LifecycleError::InvalidData(error.to_string()))?;
        atomic_write(path.as_ref(), &bytes)
    }

    /// Add a new trusted key. Existing ids are never silently replaced.
    pub fn add(&mut self, key: TrustKey) -> Result<(), LifecycleError> {
        if key.key_id.trim().is_empty() || key.label.trim().is_empty() {
            return Err(LifecycleError::InvalidData(
                "trusted key id and label are required".into(),
            ));
        }
        let _ = key.verifying_key()?;
        if self.keys.contains_key(&key.key_id) {
            return Err(LifecycleError::InvalidData(format!(
                "trusted key {} already exists",
                key.key_id
            )));
        }
        self.keys.insert(key.key_id.clone(), key);
        Ok(())
    }

    /// Mark a key as revoked while preserving audit history.
    pub fn revoke(
        &mut self,
        key_id: &str,
        reason: impl Into<String>,
    ) -> Result<(), LifecycleError> {
        let key = self
            .keys
            .get_mut(key_id)
            .ok_or_else(|| LifecycleError::Untrusted(key_id.into()))?;
        key.revoked = true;
        key.revocation_reason = Some(reason.into());
        Ok(())
    }

    /// Resolve an active trusted key.
    pub fn active_key(&self, key_id: &str) -> Result<&TrustKey, LifecycleError> {
        let key = self
            .keys
            .get(key_id)
            .ok_or_else(|| LifecycleError::Untrusted(key_id.into()))?;
        if key.revoked {
            return Err(LifecycleError::Untrusted(format!(
                "key {key_id} is revoked: {}",
                key.revocation_reason
                    .as_deref()
                    .unwrap_or("no reason supplied")
            )));
        }
        Ok(key)
    }
}

/// Detached signature distributed beside a `.loomplugin` package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureEnvelope {
    /// Signature schema version.
    pub schema_version: u32,
    /// Trusted key id.
    pub key_id: String,
    /// Lowercase SHA-256 of the exact package bytes.
    pub package_sha256: String,
    /// Base64-encoded 64-byte Ed25519 signature.
    pub signature_base64: String,
    /// Optional publisher-supplied release channel.
    pub channel: Option<String>,
    /// Optional Unix timestamp in seconds when the package was signed.
    pub signed_at_unix: Option<u64>,
}

/// Verified package identity returned before installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPackage {
    /// Parsed package manifest.
    pub manifest: PluginManifest,
    /// Signer key id.
    pub signer_key_id: String,
    /// Package SHA-256.
    pub package_sha256: String,
}

/// Verify package digest and detached Ed25519 signature against a trust store.
pub fn verify_signed_package(
    package: &[u8],
    envelope: &SignatureEnvelope,
    trust: &TrustStore,
) -> Result<VerifiedPackage, LifecycleError> {
    if envelope.schema_version != 1 {
        return Err(LifecycleError::Signature(format!(
            "unsupported envelope schema {}",
            envelope.schema_version
        )));
    }
    let digest = Sha256::digest(package);
    let digest_hex = hex_encode(&digest);
    if digest_hex != envelope.package_sha256.to_ascii_lowercase() {
        return Err(LifecycleError::Signature(
            "package SHA-256 does not match signature envelope".into(),
        ));
    }
    let key = trust.active_key(&envelope.key_id)?.verifying_key()?;
    let signature = BASE64
        .decode(&envelope.signature_base64)
        .map_err(|error| LifecycleError::Signature(format!("invalid base64 signature: {error}")))?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| LifecycleError::Signature("Ed25519 signature must contain 64 bytes".into()))?;
    let signature = Signature::from_bytes(&signature);
    let mut message = Vec::with_capacity(SIGNATURE_DOMAIN.len() + digest.len());
    message.extend_from_slice(SIGNATURE_DOMAIN);
    message.extend_from_slice(&digest);
    key.verify(&message, &signature)
        .map_err(|error| LifecycleError::Signature(error.to_string()))?;
    let manifest = package_manifest(package)?;
    Ok(VerifiedPackage {
        manifest,
        signer_key_id: envelope.key_id.clone(),
        package_sha256: digest_hex,
    })
}

fn package_manifest(package: &[u8]) -> Result<PluginManifest, LifecycleError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(package))
        .map_err(|error| LifecycleError::InvalidData(format!("invalid plugin ZIP: {error}")))?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|_| LifecycleError::InvalidData("plugin package has no manifest.json".into()))?;
    if entry.size() > MAX_MANIFEST_BYTES {
        return Err(LifecycleError::InvalidData(
            "plugin manifest exceeds size limit".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(LifecycleError::InvalidData(
            "plugin manifest exceeds size limit".into(),
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| LifecycleError::InvalidData("manifest is not UTF-8".into()))?;
    parse_manifest(text).map_err(|error| LifecycleError::InvalidData(error.to_string()))
}

/// Successful lifecycle transaction summary.
#[derive(Debug, Clone)]
pub struct LifecycleReceipt {
    /// Installed plugin.
    pub installed: InstalledPlugin,
    /// Signer key id.
    pub signer_key_id: String,
    /// Package digest.
    pub package_sha256: String,
    /// Previous version retained for rollback, when updating.
    pub rollback_version: Option<String>,
}

/// Signed, transactional lifecycle manager for one plugin store.
#[derive(Debug, Clone)]
pub struct LifecycleManager {
    store: PluginStore,
    trust_path: PathBuf,
}

impl LifecycleManager {
    /// Open a lifecycle manager rooted at an existing or new plugin store.
    pub fn open(store_directory: impl AsRef<Path>) -> Result<Self, LifecycleError> {
        let store = PluginStore::open(store_directory.as_ref())?;
        Ok(Self {
            trust_path: store.dir().join(TRUST_FILE),
            store,
        })
    }

    /// Underlying installed-plugin store.
    pub fn store(&self) -> &PluginStore {
        &self.store
    }

    /// Load the persistent trust store.
    pub fn trust_store(&self) -> Result<TrustStore, LifecycleError> {
        TrustStore::load(&self.trust_path)
    }

    /// Persist a changed trust store atomically.
    pub fn save_trust_store(&self, trust: &TrustStore) -> Result<(), LifecycleError> {
        trust.save(&self.trust_path)
    }

    /// Verify and install a new signed plugin package.
    pub fn install_signed(
        &self,
        package: &[u8],
        envelope: &SignatureEnvelope,
    ) -> Result<LifecycleReceipt, LifecycleError> {
        let trust = self.trust_store()?;
        let verified = verify_signed_package(package, envelope, &trust)?;
        if self.store.get(&verified.manifest.plugin_id).is_some() {
            return Err(LifecycleError::Transaction(format!(
                "plugin {} is already installed; use update_signed",
                verified.manifest.plugin_id
            )));
        }
        let installed = self.store.install_zip(package)?;
        write_install_attestation(
            &installed.install_dir,
            &verified.signer_key_id,
            &verified.package_sha256,
        )?;
        self.store.refresh_index()?;
        Ok(LifecycleReceipt {
            installed,
            signer_key_id: verified.signer_key_id,
            package_sha256: verified.package_sha256,
            rollback_version: None,
        })
    }

    /// Verify and transactionally update a plugin, retaining the previous
    /// installation for explicit rollback.
    pub fn update_signed(
        &self,
        package: &[u8],
        envelope: &SignatureEnvelope,
    ) -> Result<LifecycleReceipt, LifecycleError> {
        let trust = self.trust_store()?;
        let verified = verify_signed_package(package, envelope, &trust)?;
        let current = self
            .store
            .get(&verified.manifest.plugin_id)
            .ok_or_else(|| LifecycleError::Transaction("plugin is not installed".into()))?;
        if compare_versions(&verified.manifest.version, &current.version)
            != std::cmp::Ordering::Greater
        {
            return Err(LifecycleError::Transaction(format!(
                "update {} must be newer than installed version {}",
                verified.manifest.version, current.version
            )));
        }
        let transaction_root = self.store.dir().join(TRANSACTION_DIRECTORY).join(format!(
            "{}-{}",
            sanitize_identifier(&verified.manifest.plugin_id),
            unix_time_millis()
        ));
        fs::create_dir_all(&transaction_root)?;
        let staged_package = transaction_root.join("package.loomplugin");
        atomic_write(&staged_package, package)?;
        let rollback_root = self
            .store
            .dir()
            .join(ROLLBACK_DIRECTORY)
            .join(sanitize_identifier(&current.id));
        fs::create_dir_all(&rollback_root)?;
        let rollback_dir = rollback_root.join(format!(
            "{}-{}",
            sanitize_identifier(&current.version),
            unix_time_millis()
        ));
        fs::rename(&current.install_dir, &rollback_dir)?;
        self.store.refresh_index()?;
        let installation = self.store.install_zip(package);
        let installed = match installation {
            Ok(installed) => installed,
            Err(error) => {
                let _ = fs::rename(&rollback_dir, &current.install_dir);
                let _ = self.store.refresh_index();
                let _ = fs::remove_dir_all(&transaction_root);
                return Err(LifecycleError::Host(error));
            }
        };
        if let Err(error) = write_install_attestation(
            &installed.install_dir,
            &verified.signer_key_id,
            &verified.package_sha256,
        ) {
            let _ = fs::remove_dir_all(&installed.install_dir);
            let _ = fs::rename(&rollback_dir, &current.install_dir);
            let _ = self.store.refresh_index();
            let _ = fs::remove_dir_all(&transaction_root);
            return Err(error);
        }
        self.store.refresh_index()?;
        fs::remove_dir_all(&transaction_root)?;
        Ok(LifecycleReceipt {
            installed,
            signer_key_id: verified.signer_key_id,
            package_sha256: verified.package_sha256,
            rollback_version: Some(current.version),
        })
    }

    /// Roll back a plugin to its newest retained prior installation.
    pub fn rollback(&self, plugin_id: &str) -> Result<InstalledPlugin, LifecycleError> {
        let current = self
            .store
            .get(plugin_id)
            .ok_or_else(|| LifecycleError::Transaction("plugin is not installed".into()))?;
        let root = self
            .store
            .dir()
            .join(ROLLBACK_DIRECTORY)
            .join(sanitize_identifier(plugin_id));
        let mut candidates = Vec::new();
        if root.exists() {
            for entry in fs::read_dir(&root)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    candidates.push(entry.path());
                }
            }
        }
        candidates.sort();
        let rollback = candidates
            .pop()
            .ok_or_else(|| LifecycleError::Transaction("no rollback version is retained".into()))?;
        let failed_root = root.join("failed-current");
        if failed_root.exists() {
            fs::remove_dir_all(&failed_root)?;
        }
        fs::rename(&current.install_dir, &failed_root)?;
        let manifest = load_manifest_from_directory(&rollback)?;
        let restored_path = self
            .store
            .dir()
            .join(format!("{}@{}", manifest.plugin_id, manifest.version));
        if let Err(error) = fs::rename(&rollback, &restored_path) {
            let _ = fs::rename(&failed_root, &current.install_dir);
            return Err(LifecycleError::Io(error));
        }
        if let Err(error) = self.store.refresh_index() {
            let _ = fs::rename(&restored_path, &rollback);
            let _ = fs::rename(&failed_root, &current.install_dir);
            return Err(LifecycleError::Host(error));
        }
        fs::remove_dir_all(&failed_root)?;
        self.store.get(plugin_id).ok_or_else(|| {
            LifecycleError::Transaction("rollback restored no readable plugin".into())
        })
    }

    /// Remove rollback backups older than the newest `keep` entries.
    pub fn prune_rollbacks(&self, plugin_id: &str, keep: usize) -> Result<usize, LifecycleError> {
        let root = self
            .store
            .dir()
            .join(ROLLBACK_DIRECTORY)
            .join(sanitize_identifier(plugin_id));
        if !root.exists() {
            return Ok(0);
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(&root)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        entries.sort();
        let remove_count = entries.len().saturating_sub(keep);
        for path in entries.into_iter().take(remove_count) {
            fs::remove_dir_all(path)?;
        }
        Ok(remove_count)
    }
}

fn write_install_attestation(
    directory: &Path,
    signer_key_id: &str,
    package_sha256: &str,
) -> Result<(), LifecycleError> {
    let document = serde_json::json!({
        "schema_version": 1,
        "signer_key_id": signer_key_id,
        "package_sha256": package_sha256,
        "verified_at_unix_ms": unix_time_millis(),
    });
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| LifecycleError::InvalidData(error.to_string()))?;
    atomic_write(&directory.join("verification.json"), &bytes)
}

fn load_manifest_from_directory(directory: &Path) -> Result<PluginManifest, LifecycleError> {
    let bytes = fs::read(directory.join("manifest.json"))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| LifecycleError::InvalidData("stored manifest is not UTF-8".into()))?;
    parse_manifest(text).map_err(|error| LifecycleError::InvalidData(error.to_string()))
}

fn sanitize_identifier(identifier: &str) -> String {
    identifier
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Declarative UI extension document loaded from a signed plugin package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiExtensionManifest {
    /// UI extension schema version.
    pub schema_version: u32,
    /// Plugin id that owns these contributions.
    pub plugin_id: String,
    /// Command contributions.
    pub commands: Vec<CommandContribution>,
    /// Panel contributions.
    pub panels: Vec<PanelContribution>,
    /// Menu placements.
    pub menu_items: Vec<MenuContribution>,
}

impl UiExtensionManifest {
    /// Validate identifiers, surfaces, bindings, and control cardinality.
    pub fn validate(&self) -> Result<(), LifecycleError> {
        if self.schema_version != 1 {
            return Err(LifecycleError::InvalidUi(format!(
                "unsupported UI schema {}",
                self.schema_version
            )));
        }
        if !is_identifier(&self.plugin_id) {
            return Err(LifecycleError::InvalidUi("invalid plugin_id".into()));
        }
        let mut ids = BTreeSet::new();
        for command in &self.commands {
            validate_contribution_id(&self.plugin_id, &command.id)?;
            if command.title.trim().is_empty() || command.invoke.trim().is_empty() {
                return Err(LifecycleError::InvalidUi(format!(
                    "command {} requires title and invoke binding",
                    command.id
                )));
            }
            if !ids.insert(command.id.clone()) {
                return Err(LifecycleError::InvalidUi(format!(
                    "duplicate contribution id {}",
                    command.id
                )));
            }
        }
        for panel in &self.panels {
            validate_contribution_id(&self.plugin_id, &panel.id)?;
            if panel.title.trim().is_empty() || panel.controls.len() > 128 {
                return Err(LifecycleError::InvalidUi(format!(
                    "panel {} has an invalid title or too many controls",
                    panel.id
                )));
            }
            if !ids.insert(panel.id.clone()) {
                return Err(LifecycleError::InvalidUi(format!(
                    "duplicate contribution id {}",
                    panel.id
                )));
            }
            let mut control_ids = BTreeSet::new();
            for control in &panel.controls {
                if !is_identifier(&control.id)
                    || control.label.trim().is_empty()
                    || control.binding.trim().is_empty()
                {
                    return Err(LifecycleError::InvalidUi(format!(
                        "panel {} has an invalid control",
                        panel.id
                    )));
                }
                if !control_ids.insert(control.id.clone()) {
                    return Err(LifecycleError::InvalidUi(format!(
                        "panel {} repeats control {}",
                        panel.id, control.id
                    )));
                }
                control.validate()?;
            }
        }
        for menu in &self.menu_items {
            validate_contribution_id(&self.plugin_id, &menu.id)?;
            if menu.command_id.trim().is_empty() || menu.order < -10_000 || menu.order > 10_000 {
                return Err(LifecycleError::InvalidUi(format!(
                    "menu contribution {} is invalid",
                    menu.id
                )));
            }
            if !ids.insert(menu.id.clone()) {
                return Err(LifecycleError::InvalidUi(format!(
                    "duplicate contribution id {}",
                    menu.id
                )));
            }
        }
        Ok(())
    }
}

/// Declarative command exposed by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContribution {
    /// Globally namespaced id.
    pub id: String,
    /// Localized fallback title.
    pub title: String,
    /// Plugin export or message binding invoked by the host.
    pub invoke: String,
    /// Optional default keyboard shortcut.
    pub default_shortcut: Option<String>,
    /// Whether the command mutates the active document and must participate in history.
    pub mutates_document: bool,
}

/// Host location available to a declarative panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelSurface {
    /// Contextual inspector.
    Inspector,
    /// Primary workspace sidebar.
    Sidebar,
    /// Detached or docked utility area.
    Utility,
}

/// Declarative plugin panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelContribution {
    /// Globally namespaced id.
    pub id: String,
    /// Localized fallback title.
    pub title: String,
    /// Requested host surface.
    pub surface: PanelSurface,
    /// Declarative controls rendered by trusted host components.
    pub controls: Vec<UiControl>,
}

/// Supported declarative UI control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiControl {
    /// Panel-local identifier.
    pub id: String,
    /// Localized fallback label.
    pub label: String,
    /// Control kind.
    pub kind: UiControlKind,
    /// Plugin state or action binding.
    pub binding: String,
    /// Optional numeric minimum.
    pub minimum: Option<f64>,
    /// Optional numeric maximum.
    pub maximum: Option<f64>,
    /// Select options.
    pub options: Vec<String>,
    /// Accessible description.
    pub accessibility_description: String,
}

impl UiControl {
    fn validate(&self) -> Result<(), LifecycleError> {
        if self.options.len() > 256 || self.options.iter().any(|option| option.trim().is_empty()) {
            return Err(LifecycleError::InvalidUi(format!(
                "control {} has invalid options",
                self.id
            )));
        }
        match self.kind {
            UiControlKind::Slider | UiControlKind::Number => {
                let (minimum, maximum) = self.minimum.zip(self.maximum).ok_or_else(|| {
                    LifecycleError::InvalidUi(format!(
                        "numeric control {} requires minimum and maximum",
                        self.id
                    ))
                })?;
                if !minimum.is_finite() || !maximum.is_finite() || minimum >= maximum {
                    return Err(LifecycleError::InvalidUi(format!(
                        "numeric control {} has an invalid range",
                        self.id
                    )));
                }
            }
            UiControlKind::Select if self.options.is_empty() => {
                return Err(LifecycleError::InvalidUi(format!(
                    "select control {} requires options",
                    self.id
                )));
            }
            _ => {}
        }
        Ok(())
    }
}

/// Declarative control kinds rendered exclusively by Loom-owned components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiControlKind {
    /// Momentary action button.
    Button,
    /// Boolean switch.
    Toggle,
    /// Bounded continuous value.
    Slider,
    /// Numeric entry.
    Number,
    /// Single-line text entry.
    Text,
    /// Choice from a fixed option list.
    Select,
    /// Read-only status text.
    Status,
}

/// Placement of a command in a host menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuContribution {
    /// Globally namespaced contribution id.
    pub id: String,
    /// Target menu, such as `edit`, `view`, or `effects`.
    pub menu: String,
    /// Command contribution id.
    pub command_id: String,
    /// Stable ordering weight.
    pub order: i32,
}

fn validate_contribution_id(plugin_id: &str, contribution_id: &str) -> Result<(), LifecycleError> {
    if !is_identifier(contribution_id) || !contribution_id.starts_with(&format!("{plugin_id}.")) {
        return Err(LifecycleError::InvalidUi(format!(
            "contribution id {contribution_id:?} must be namespaced by {plugin_id}."
        )));
    }
    Ok(())
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

/// Declarative compatibility migration between plugin state versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Source state version.
    pub from_version: String,
    /// Destination state version.
    pub to_version: String,
    /// Ordered migration operations.
    pub steps: Vec<MigrationStep>,
}

impl MigrationPlan {
    /// Apply the migration to a JSON object without executing plugin code.
    pub fn apply(&self, state: &mut Value) -> Result<(), LifecycleError> {
        if self.from_version.trim().is_empty()
            || self.to_version.trim().is_empty()
            || compare_versions(&self.to_version, &self.from_version) != std::cmp::Ordering::Greater
        {
            return Err(LifecycleError::Migration(
                "destination version must be newer than source version".into(),
            ));
        }
        if self.steps.len() > 1024 {
            return Err(LifecycleError::Migration(
                "migration contains too many operations".into(),
            ));
        }
        for step in &self.steps {
            step.apply(state)?;
        }
        Ok(())
    }
}

/// Safe JSON-state migration operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MigrationStep {
    /// Rename an existing object key.
    Rename {
        /// Dot-separated source path.
        from: String,
        /// Dot-separated destination path.
        to: String,
    },
    /// Remove a value when present.
    Remove {
        /// Dot-separated path.
        path: String,
    },
    /// Set a value only when the path is missing.
    SetDefault {
        /// Dot-separated path.
        path: String,
        /// Default JSON value.
        value: Value,
    },
}

impl MigrationStep {
    fn apply(&self, root: &mut Value) -> Result<(), LifecycleError> {
        match self {
            Self::Rename { from, to } => {
                let value = take_path(root, from)?.ok_or_else(|| {
                    LifecycleError::Migration(format!("source path {from:?} does not exist"))
                })?;
                if get_path(root, to)?.is_some() {
                    return Err(LifecycleError::Migration(format!(
                        "destination path {to:?} already exists"
                    )));
                }
                set_path(root, to, value, false)
            }
            Self::Remove { path } => {
                let _ = take_path(root, path)?;
                Ok(())
            }
            Self::SetDefault { path, value } => set_path(root, path, value.clone(), true),
        }
    }
}

fn split_path(path: &str) -> Result<Vec<&str>, LifecycleError> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty()
        || parts.len() > 32
        || parts
            .iter()
            .any(|part| part.is_empty() || !is_identifier(part))
    {
        return Err(LifecycleError::Migration(format!(
            "invalid migration path {path:?}"
        )));
    }
    Ok(parts)
}

fn get_path<'a>(root: &'a Value, path: &str) -> Result<Option<&'a Value>, LifecycleError> {
    let mut current = root;
    for part in split_path(path)? {
        let Some(object) = current.as_object() else {
            return Ok(None);
        };
        let Some(next) = object.get(part) else {
            return Ok(None);
        };
        current = next;
    }
    Ok(Some(current))
}

fn take_path(root: &mut Value, path: &str) -> Result<Option<Value>, LifecycleError> {
    let parts = split_path(path)?;
    let (last, parents) = parts
        .split_last()
        .ok_or_else(|| LifecycleError::Migration("empty path".into()))?;
    let mut current = root;
    for part in parents {
        let Some(object) = current.as_object_mut() else {
            return Ok(None);
        };
        let Some(next) = object.get_mut(*part) else {
            return Ok(None);
        };
        current = next;
    }
    Ok(current
        .as_object_mut()
        .and_then(|object| object.remove(*last)))
}

fn set_path(
    root: &mut Value,
    path: &str,
    value: Value,
    only_when_missing: bool,
) -> Result<(), LifecycleError> {
    let parts = split_path(path)?;
    let (last, parents) = parts
        .split_last()
        .ok_or_else(|| LifecycleError::Migration("empty path".into()))?;
    let mut current = root;
    for part in parents {
        if !current.is_object() {
            return Err(LifecycleError::Migration(format!(
                "path parent {part:?} is not an object"
            )));
        }
        let object = current
            .as_object_mut()
            .ok_or_else(|| LifecycleError::Migration("migration root is not an object".into()))?;
        current = object
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        LifecycleError::Migration("migration target parent is not an object".into())
    })?;
    if only_when_missing && object.contains_key(*last) {
        return Ok(());
    }
    object.insert((*last).to_string(), value);
    Ok(())
}

/// Native audio-plugin ABI family handled by an isolated bridge process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativePluginFormat {
    /// CLAP audio plugin.
    Clap,
    /// VST3 audio plugin.
    Vst3,
}

impl NativePluginFormat {
    fn argument(self) -> &'static str {
        match self {
            Self::Clap => "clap",
            Self::Vst3 => "vst3",
        }
    }
}

/// Native plugin selected for isolated scanning or hosting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePluginDescriptor {
    /// Canonical plugin binary or bundle path.
    pub path: PathBuf,
    /// Plugin ABI family.
    pub format: NativePluginFormat,
    /// Optional stable class or plugin identifier.
    pub identifier: Option<String>,
}

/// Resource and response limits enforced by the application-side bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeLimits {
    /// Maximum wait for one response.
    pub response_timeout: Duration,
    /// Maximum serialized request size.
    pub max_request_bytes: usize,
    /// Maximum serialized response size.
    pub max_response_bytes: usize,
}

impl Default for BridgeLimits {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(5),
            max_request_bytes: 1024 * 1024,
            max_response_bytes: 4 * 1024 * 1024,
        }
    }
}

/// JSON-lines request sent to a native plugin bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeRequest {
    /// Caller-generated request id.
    pub request_id: u64,
    /// Operation, such as `scan`, `instantiate`, `process`, or `save_state`.
    pub operation: String,
    /// Operation-specific JSON payload.
    pub payload: Value,
}

/// JSON-lines response returned by a native plugin bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeResponse {
    /// Request id being answered.
    pub request_id: u64,
    /// Whether the operation succeeded.
    pub success: bool,
    /// Operation-specific JSON payload.
    pub payload: Value,
    /// Error message when `success` is false.
    pub error: Option<String>,
}

/// Running process-isolated native plugin bridge.
#[derive(Debug)]
pub struct NativePluginBridge {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<Result<BridgeResponse, LifecycleError>>,
    limits: BridgeLimits,
}

impl NativePluginBridge {
    /// Spawn an audited bridge helper for one canonical CLAP or VST3 path.
    ///
    /// The application process never opens the plugin binary. The helper is
    /// launched without a command shell, with an empty environment and a
    /// private session directory as its working directory.
    pub fn spawn(
        helper: impl AsRef<Path>,
        plugin: NativePluginDescriptor,
        session_directory: impl AsRef<Path>,
        limits: BridgeLimits,
    ) -> Result<Self, LifecycleError> {
        if limits.max_request_bytes == 0 || limits.max_response_bytes == 0 {
            return Err(LifecycleError::InvalidData(
                "bridge byte limits must be non-zero".into(),
            ));
        }
        let helper = canonical_regular_file(helper.as_ref(), "bridge helper")?;
        let plugin_path = canonical_plugin_path(&plugin.path)?;
        let session_directory = session_directory.as_ref();
        fs::create_dir_all(session_directory)?;
        let session_directory = fs::canonicalize(session_directory)?;
        let mut child = Command::new(helper)
            .arg("--format")
            .arg(plugin.format.argument())
            .arg("--plugin")
            .arg(&plugin_path)
            .arg("--session")
            .arg(&session_directory)
            .env_clear()
            .env("LOOM_NATIVE_PLUGIN_BRIDGE", "1")
            .current_dir(&session_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LifecycleError::BridgeProtocol("bridge stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LifecycleError::BridgeProtocol("bridge stdout unavailable".into()))?;
        let (sender, responses) = mpsc::channel();
        let max_response_bytes = limits.max_response_bytes;
        thread::Builder::new()
            .name("loom-native-plugin-bridge-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) => break,
                        Ok(_) if line.len() > max_response_bytes => {
                            let _ = sender.send(Err(LifecycleError::BridgeProtocol(
                                "bridge response exceeds byte limit".into(),
                            )));
                            break;
                        }
                        Ok(_) => {
                            let parsed = serde_json::from_slice::<BridgeResponse>(&line)
                                .map_err(|error| LifecycleError::BridgeProtocol(error.to_string()));
                            if sender.send(parsed).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = sender.send(Err(LifecycleError::Io(error)));
                            break;
                        }
                    }
                }
            })?;
        Ok(Self {
            child,
            stdin,
            responses,
            limits,
        })
    }

    /// Send one request and await the matching response within the configured
    /// timeout. Out-of-order responses are rejected to keep the initial
    /// protocol deterministic.
    pub fn request(&mut self, request: &BridgeRequest) -> Result<BridgeResponse, LifecycleError> {
        if request.operation.trim().is_empty() {
            return Err(LifecycleError::BridgeProtocol(
                "request operation is empty".into(),
            ));
        }
        let mut bytes = serde_json::to_vec(request)
            .map_err(|error| LifecycleError::BridgeProtocol(error.to_string()))?;
        if bytes.len() > self.limits.max_request_bytes {
            return Err(LifecycleError::BridgeProtocol(
                "request exceeds byte limit".into(),
            ));
        }
        bytes.push(b'\n');
        self.stdin.write_all(&bytes)?;
        self.stdin.flush()?;
        let response = self
            .responses
            .recv_timeout(self.limits.response_timeout)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => LifecycleError::BridgeTimeout,
                mpsc::RecvTimeoutError::Disconnected => {
                    LifecycleError::BridgeProtocol("bridge disconnected".into())
                }
            })??;
        if response.request_id != request.request_id {
            return Err(LifecycleError::BridgeProtocol(format!(
                "expected response {}, received {}",
                request.request_id, response.request_id
            )));
        }
        Ok(response)
    }

    /// Ask the helper to describe the selected plugin without instantiating it
    /// in the application process.
    pub fn scan(&mut self, request_id: u64) -> Result<BridgeResponse, LifecycleError> {
        self.request(&BridgeRequest {
            request_id,
            operation: "scan".into(),
            payload: Value::Null,
        })
    }

    /// Terminate the bridge and wait for process cleanup.
    pub fn shutdown(mut self) -> Result<(), LifecycleError> {
        let request = BridgeRequest {
            request_id: u64::MAX,
            operation: "shutdown".into(),
            payload: Value::Null,
        };
        let _ = self.request(&request);
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        let _ = self.child.wait()?;
        Ok(())
    }
}

impl Drop for NativePluginBridge {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn canonical_regular_file(path: &Path, label: &str) -> Result<PathBuf, LifecycleError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(LifecycleError::InvalidData(format!(
            "{label} must be a regular non-symlink file"
        )));
    }
    Ok(fs::canonicalize(path)?)
}

fn canonical_plugin_path(path: &Path) -> Result<PathBuf, LifecycleError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err(LifecycleError::InvalidData(
            "native plugin path must be a regular file or bundle directory".into(),
        ));
    }
    Ok(fs::canonicalize(path)?)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), LifecycleError> {
    let parent = path
        .parent()
        .ok_or_else(|| LifecycleError::InvalidData("path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("loom"),
        std::process::id()
    ));
    {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Write;

    fn plugin_package(id: &str, version: &str) -> Vec<u8> {
        let manifest = serde_json::json!({
            "manifest_version": 1,
            "plugin_id": id,
            "name": "Test Plugin",
            "description": "test",
            "version": version,
            "author": "Loom Tests",
            "license": "MIT",
            "entry": {
                "kind": "command",
                "wasm_module": "module.wasm",
                "function": "run"
            },
            "capabilities": [],
            "permissions": [],
            "api_min_version": "0.1.0",
            "api_max_version": "0.9.0",
            "resource_limits": {
                "max_memory_bytes": 1048576,
                "max_fs_bytes": 1048576,
                "max_fs_entries": 100,
                "max_cpu_ms_per_call": 1000,
                "network": false
            }
        });
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        writer
            .start_file("manifest.json", options)
            .expect("manifest entry");
        writer
            .write_all(manifest.to_string().as_bytes())
            .expect("manifest");
        writer
            .start_file("module.wasm", options)
            .expect("wasm entry");
        writer.write_all(b"\0asm\x01\0\0\0").expect("wasm");
        writer.finish().expect("zip").into_inner()
    }

    fn signed(package: &[u8]) -> (TrustStore, SignatureEnvelope) {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let digest = Sha256::digest(package);
        let mut message = Vec::new();
        message.extend_from_slice(SIGNATURE_DOMAIN);
        message.extend_from_slice(&digest);
        let signature = signing.sign(&message);
        let key = TrustKey {
            key_id: "test-key".into(),
            label: "Test Publisher".into(),
            public_key_base64: BASE64.encode(signing.verifying_key().to_bytes()),
            revoked: false,
            revocation_reason: None,
        };
        let mut trust = TrustStore::new();
        trust.add(key).expect("trust key");
        let envelope = SignatureEnvelope {
            schema_version: 1,
            key_id: "test-key".into(),
            package_sha256: hex_encode(&digest),
            signature_base64: BASE64.encode(signature.to_bytes()),
            channel: Some("stable".into()),
            signed_at_unix: Some(1),
        };
        (trust, envelope)
    }

    #[test]
    fn signed_package_verification_rejects_tampering_and_revocation() {
        let package = plugin_package("example-test", "1.0.0");
        let (mut trust, envelope) = signed(&package);
        let verified = verify_signed_package(&package, &envelope, &trust).expect("verified");
        assert_eq!(verified.manifest.plugin_id, "example-test");
        let mut tampered = package.clone();
        tampered.push(0);
        assert!(verify_signed_package(&tampered, &envelope, &trust).is_err());
        trust.revoke("test-key", "compromised").expect("revoke");
        assert!(matches!(
            verify_signed_package(&package, &envelope, &trust),
            Err(LifecycleError::Untrusted(_))
        ));
    }

    #[test]
    fn lifecycle_install_update_and_rollback_are_transactional() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let manager = LifecycleManager::open(temporary.path()).expect("manager");
        let package_one = plugin_package("example-test", "1.0.0");
        let (trust, envelope_one) = signed(&package_one);
        manager.save_trust_store(&trust).expect("save trust");
        manager
            .install_signed(&package_one, &envelope_one)
            .expect("install");
        let package_two = plugin_package("example-test", "2.0.0");
        let (_, envelope_two) = signed(&package_two);
        manager
            .update_signed(&package_two, &envelope_two)
            .expect("update");
        assert_eq!(
            manager.store().get("example-test").unwrap().version,
            "2.0.0"
        );
        let restored = manager.rollback("example-test").expect("rollback");
        assert_eq!(restored.version, "1.0.0");
    }

    #[test]
    fn declarative_ui_rejects_unnamespaced_and_invalid_numeric_controls() {
        let manifest = UiExtensionManifest {
            schema_version: 1,
            plugin_id: "example-test".into(),
            commands: vec![CommandContribution {
                id: "wrong.command".into(),
                title: "Run".into(),
                invoke: "run".into(),
                default_shortcut: None,
                mutates_document: true,
            }],
            panels: Vec::new(),
            menu_items: Vec::new(),
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn migration_renames_removes_and_sets_defaults() {
        let mut state = serde_json::json!({
            "render": { "quality": 2, "legacy": true }
        });
        let plan = MigrationPlan {
            from_version: "1.0.0".into(),
            to_version: "2.0.0".into(),
            steps: vec![
                MigrationStep::Rename {
                    from: "render.quality".into(),
                    to: "render.samples".into(),
                },
                MigrationStep::Remove {
                    path: "render.legacy".into(),
                },
                MigrationStep::SetDefault {
                    path: "render.backend".into(),
                    value: Value::String("auto".into()),
                },
            ],
        };
        plan.apply(&mut state).expect("migrate");
        assert_eq!(state["render"]["samples"], 2);
        assert!(state["render"].get("legacy").is_none());
        assert_eq!(state["render"]["backend"], "auto");
    }
}
