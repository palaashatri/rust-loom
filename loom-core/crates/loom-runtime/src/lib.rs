//! Shared desktop runtime services for every Loom application.
//!
//! The runtime is intentionally small and local-first. It owns durable settings,
//! bounded diagnostic logs, autosave/recovery snapshots, shortcut resolution,
//! recent-file bookkeeping, and typed clipboard payloads. It performs no network
//! access and contains no UI framework dependency.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum clipboard payload accepted by default (64 MiB).
pub const DEFAULT_CLIPBOARD_LIMIT: usize = 64 * 1024 * 1024;
/// Default number of diagnostic messages retained in memory.
pub const DEFAULT_LOG_CAPACITY: usize = 2048;

/// Runtime failures surfaced to applications.
#[derive(Debug)]
pub enum RuntimeError {
    /// Filesystem operation failed.
    Io(io::Error),
    /// Persisted runtime data was malformed.
    InvalidData(String),
    /// A configured safety limit was exceeded.
    LimitExceeded(String),
    /// A requested item was not found.
    NotFound(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Io(err) => write!(f, "I/O error: {err}"),
            RuntimeError::InvalidData(message) => write!(f, "invalid runtime data: {message}"),
            RuntimeError::LimitExceeded(message) => write!(f, "runtime limit exceeded: {message}"),
            RuntimeError::NotFound(message) => write!(f, "not found: {message}"),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RuntimeError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for RuntimeError {
    fn from(value: io::Error) -> Self {
        RuntimeError::Io(value)
    }
}

/// Standard per-application filesystem locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    /// Application data root.
    pub data_dir: PathBuf,
    /// User configuration root.
    pub config_dir: PathBuf,
    /// Cache root.
    pub cache_dir: PathBuf,
    /// Autosave and recovery root.
    pub recovery_dir: PathBuf,
    /// Diagnostic log root.
    pub log_dir: PathBuf,
    /// Per-plugin state root.
    pub plugin_state_dir: PathBuf,
}

impl RuntimePaths {
    /// Builds deterministic paths below `root` for `app_id`.
    pub fn under(root: impl AsRef<Path>, app_id: &str) -> Result<Self, RuntimeError> {
        validate_component(app_id)?;
        let root = root.as_ref();
        let data_dir = root.join("data").join(app_id);
        let config_dir = root.join("config").join(app_id);
        let cache_dir = root.join("cache").join(app_id);
        let recovery_dir = data_dir.join("recovery");
        let log_dir = data_dir.join("logs");
        let plugin_state_dir = data_dir.join("plugins");
        for path in [
            &data_dir,
            &config_dir,
            &cache_dir,
            &recovery_dir,
            &log_dir,
            &plugin_state_dir,
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(Self {
            data_dir,
            config_dir,
            cache_dir,
            recovery_dir,
            log_dir,
            plugin_state_dir,
        })
    }
}

fn validate_component(value: &str) -> Result<(), RuntimeError> {
    let valid = !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != "..";
    if valid {
        Ok(())
    } else {
        Err(RuntimeError::InvalidData(format!(
            "unsafe path component {value:?}"
        )))
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let nonce = now_millis();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("loom-data");
    let temp = path.with_file_name(format!(".{file_name}.{nonce}.tmp"));
    fs::write(&temp, bytes)?;
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(RuntimeError::Io(err))
        }
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn encode_field(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b' ') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

fn decode_field(input: &str) -> Result<String, RuntimeError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(RuntimeError::InvalidData("truncated escape".into()));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| RuntimeError::InvalidData("non-UTF8 escape".into()))?;
            let byte = u8::from_str_radix(hex, 16)
                .map_err(|_| RuntimeError::InvalidData(format!("invalid escape %{hex}")))?;
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| RuntimeError::InvalidData("invalid UTF-8 field".into()))
}

/// Durable key/value settings backed by a deterministic text file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    values: BTreeMap<String, String>,
}

impl Settings {
    /// Reads settings from `path`; a missing file yields an empty store.
    pub fn load(path: &Path) -> Result<Self, RuntimeError> {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err.into()),
        };
        let mut values = BTreeMap::new();
        for (line_number, line) in raw.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(RuntimeError::InvalidData(format!(
                    "settings line {} has no '='",
                    line_number + 1
                )));
            };
            values.insert(decode_field(key)?, decode_field(value)?);
        }
        Ok(Self { values })
    }

    /// Saves settings atomically.
    pub fn save(&self, path: &Path) -> Result<(), RuntimeError> {
        let mut raw = String::from("# Loom settings v1\n");
        for (key, value) in &self.values {
            raw.push_str(&encode_field(key));
            raw.push('=');
            raw.push_str(&encode_field(value));
            raw.push('\n');
        }
        atomic_write(path, raw.as_bytes())
    }

    /// Returns a setting.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Sets a setting.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    /// Removes a setting.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.values.remove(key)
    }

    /// Iterates settings in stable key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

/// One recoverable autosave snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEntry {
    /// Stable document id.
    pub document_id: String,
    /// Display title captured at autosave time.
    pub title: String,
    /// Millisecond UNIX timestamp.
    pub saved_at_millis: u128,
    /// Original document path, when the file had been saved before.
    pub original_path: Option<PathBuf>,
    /// Snapshot payload size.
    pub size_bytes: u64,
    /// Path of the persisted recovery payload.
    pub snapshot_path: PathBuf,
}

/// Crash-recovery storage with atomic snapshots and bounded retention.
#[derive(Debug, Clone)]
pub struct RecoveryStore {
    root: PathBuf,
    max_snapshots_per_document: usize,
    max_snapshot_bytes: usize,
}

impl RecoveryStore {
    /// Opens a recovery store below `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, RuntimeError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            max_snapshots_per_document: 12,
            max_snapshot_bytes: 512 * 1024 * 1024,
        })
    }

    /// Overrides retention and payload limits.
    pub fn with_limits(mut self, max_snapshots: usize, max_snapshot_bytes: usize) -> Self {
        self.max_snapshots_per_document = max_snapshots.max(1);
        self.max_snapshot_bytes = max_snapshot_bytes.max(1);
        self
    }

    /// Writes an atomic snapshot and prunes older generations.
    pub fn save_snapshot(
        &self,
        document_id: &str,
        title: &str,
        original_path: Option<&Path>,
        payload: &[u8],
    ) -> Result<RecoveryEntry, RuntimeError> {
        validate_component(document_id)?;
        if payload.len() > self.max_snapshot_bytes {
            return Err(RuntimeError::LimitExceeded(format!(
                "snapshot is {} bytes; limit is {}",
                payload.len(),
                self.max_snapshot_bytes
            )));
        }
        let document_dir = self.root.join(document_id);
        fs::create_dir_all(&document_dir)?;
        let mut timestamp = now_millis();
        let (snapshot_path, metadata_path) = loop {
            let stem = format!("{timestamp:032}");
            let snapshot_path = document_dir.join(format!("{stem}.snapshot"));
            let metadata_path = document_dir.join(format!("{stem}.meta"));
            if !snapshot_path.exists() && !metadata_path.exists() {
                break (snapshot_path, metadata_path);
            }
            timestamp = timestamp.saturating_add(1);
        };
        atomic_write(&snapshot_path, payload)?;
        let original = original_path
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let metadata = format!(
            "document_id={}\ntitle={}\nsaved_at={}\noriginal_path={}\nsize={}\n",
            encode_field(document_id),
            encode_field(title),
            timestamp,
            encode_field(&original),
            payload.len()
        );
        if let Err(err) = atomic_write(&metadata_path, metadata.as_bytes()) {
            let _ = fs::remove_file(&snapshot_path);
            return Err(err);
        }
        self.prune_document(document_id)?;
        Ok(RecoveryEntry {
            document_id: document_id.to_string(),
            title: title.to_string(),
            saved_at_millis: timestamp,
            original_path: original_path.map(Path::to_path_buf),
            size_bytes: payload.len() as u64,
            snapshot_path,
        })
    }

    /// Lists all valid recovery entries newest-first.
    pub fn list(&self) -> Result<Vec<RecoveryEntry>, RuntimeError> {
        let mut entries = Vec::new();
        for directory in fs::read_dir(&self.root)? {
            let directory = directory?;
            if !directory.file_type()?.is_dir() {
                continue;
            }
            for file in fs::read_dir(directory.path())? {
                let file = file?;
                if file.path().extension().and_then(|value| value.to_str()) != Some("meta") {
                    continue;
                }
                if let Ok(entry) = read_recovery_metadata(&file.path()) {
                    if entry.snapshot_path.is_file() {
                        entries.push(entry);
                    }
                }
            }
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.saved_at_millis));
        Ok(entries)
    }

    /// Reads a recovery payload.
    pub fn read(&self, entry: &RecoveryEntry) -> Result<Vec<u8>, RuntimeError> {
        let canonical_root = fs::canonicalize(&self.root)?;
        let canonical_snapshot = fs::canonicalize(&entry.snapshot_path)?;
        if !canonical_snapshot.starts_with(canonical_root) {
            return Err(RuntimeError::InvalidData(
                "recovery entry escaped the configured root".into(),
            ));
        }
        Ok(fs::read(canonical_snapshot)?)
    }

    /// Deletes one snapshot and its metadata.
    pub fn discard(&self, entry: &RecoveryEntry) -> Result<(), RuntimeError> {
        let snapshot = &entry.snapshot_path;
        let metadata = snapshot.with_extension("meta");
        match fs::remove_file(snapshot) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        match fs::remove_file(metadata) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        Ok(())
    }

    fn prune_document(&self, document_id: &str) -> Result<(), RuntimeError> {
        let document_dir = self.root.join(document_id);
        let mut metadata: Vec<PathBuf> = fs::read_dir(&document_dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("meta"))
            .collect();
        metadata.sort();
        let excess = metadata
            .len()
            .saturating_sub(self.max_snapshots_per_document);
        for path in metadata.into_iter().take(excess) {
            let snapshot = path.with_extension("snapshot");
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(snapshot);
        }
        Ok(())
    }
}

fn read_recovery_metadata(path: &Path) -> Result<RecoveryEntry, RuntimeError> {
    let raw = fs::read_to_string(path)?;
    let mut fields = HashMap::new();
    for line in raw.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        fields.insert(key.to_string(), decode_field(value)?);
    }
    let document_id = fields
        .remove("document_id")
        .ok_or_else(|| RuntimeError::InvalidData("missing document_id".into()))?;
    let title = fields.remove("title").unwrap_or_default();
    let saved_at_millis = fields
        .remove("saved_at")
        .ok_or_else(|| RuntimeError::InvalidData("missing saved_at".into()))?
        .parse()
        .map_err(|_| RuntimeError::InvalidData("saved_at is not an integer".into()))?;
    let size_bytes = fields
        .remove("size")
        .ok_or_else(|| RuntimeError::InvalidData("missing size".into()))?
        .parse()
        .map_err(|_| RuntimeError::InvalidData("size is not an integer".into()))?;
    let original_path = fields
        .remove("original_path")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    Ok(RecoveryEntry {
        document_id,
        title,
        saved_at_millis,
        original_path,
        size_bytes,
        snapshot_path: path.with_extension("snapshot"),
    })
}

/// Severity of a diagnostic event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Detailed troubleshooting information.
    Debug,
    /// Ordinary lifecycle information.
    Info,
    /// Recoverable unexpected behavior.
    Warn,
    /// Failed operation requiring attention.
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// One diagnostic event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// UNIX timestamp in milliseconds.
    pub timestamp_millis: u128,
    /// Severity.
    pub level: LogLevel,
    /// Stable subsystem name.
    pub target: String,
    /// Redacted human-readable message.
    pub message: String,
}

/// Bounded, redactable diagnostics buffer.
#[derive(Debug, Clone)]
pub struct DiagnosticLog {
    capacity: usize,
    entries: VecDeque<LogEntry>,
    redactions: Vec<(String, String)>,
}

impl DiagnosticLog {
    /// Creates a bounded log.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
            redactions: Vec::new(),
        }
    }

    /// Adds a literal redaction replacement.
    pub fn add_redaction(&mut self, secret: impl Into<String>, replacement: impl Into<String>) {
        let secret = secret.into();
        if !secret.is_empty() {
            self.redactions.push((secret, replacement.into()));
            self.redactions
                .sort_by_key(|(secret, _)| std::cmp::Reverse(secret.len()));
        }
    }

    /// Appends a redacted entry.
    pub fn push(&mut self, level: LogLevel, target: impl Into<String>, message: impl Into<String>) {
        let mut message = message.into();
        for (secret, replacement) in &self.redactions {
            message = message.replace(secret, replacement);
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry {
            timestamp_millis: now_millis(),
            level,
            target: target.into(),
            message,
        });
    }

    /// Returns entries in chronological order.
    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    /// Writes a deterministic text report atomically.
    pub fn write_report(&self, path: &Path) -> Result<(), RuntimeError> {
        let mut report = String::new();
        for entry in &self.entries {
            report.push_str(&format!(
                "{} {} {}: {}\n",
                entry.timestamp_millis,
                entry.level.as_str(),
                entry.target,
                entry.message.replace('\n', "\\n")
            ));
        }
        atomic_write(path, report.as_bytes())
    }
}

impl Default for DiagnosticLog {
    fn default() -> Self {
        Self::new(DEFAULT_LOG_CAPACITY)
    }
}

/// Keyboard shortcut conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutConflict {
    /// Normalized chord.
    pub chord: String,
    /// Previously bound command.
    pub existing_command: String,
    /// Command that attempted to claim the chord.
    pub requested_command: String,
}

/// Bidirectional shortcut registry used by menus, command palette, and plugins.
#[derive(Debug, Clone, Default)]
pub struct ShortcutRegistry {
    by_chord: BTreeMap<String, String>,
    by_command: BTreeMap<String, String>,
}

impl ShortcutRegistry {
    /// Binds `chord` to `command_id`, rejecting conflicts.
    pub fn bind(&mut self, command_id: &str, chord: &str) -> Result<String, ShortcutConflict> {
        let chord = normalize_chord(chord);
        if let Some(existing) = self.by_chord.get(&chord) {
            if existing != command_id {
                return Err(ShortcutConflict {
                    chord,
                    existing_command: existing.clone(),
                    requested_command: command_id.to_string(),
                });
            }
        }
        if let Some(old_chord) = self
            .by_command
            .insert(command_id.to_string(), chord.clone())
        {
            self.by_chord.remove(&old_chord);
        }
        self.by_chord.insert(chord.clone(), command_id.to_string());
        Ok(chord)
    }

    /// Removes a command binding.
    pub fn unbind_command(&mut self, command_id: &str) -> Option<String> {
        let chord = self.by_command.remove(command_id)?;
        self.by_chord.remove(&chord);
        Some(chord)
    }

    /// Resolves a chord to a command.
    pub fn resolve(&self, chord: &str) -> Option<&str> {
        self.by_chord
            .get(&normalize_chord(chord))
            .map(String::as_str)
    }

    /// Returns the chord for a command.
    pub fn chord_for(&self, command_id: &str) -> Option<&str> {
        self.by_command.get(command_id).map(String::as_str)
    }
}

fn normalize_chord(chord: &str) -> String {
    let mut modifiers: Vec<String> = Vec::new();
    let mut key = String::new();
    for part in chord
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.push("Ctrl".into()),
            "alt" | "option" => modifiers.push("Alt".into()),
            "shift" => modifiers.push("Shift".into()),
            "meta" | "cmd" | "command" | "super" => modifiers.push("Meta".into()),
            _ => key = canonical_key(part),
        }
    }
    modifiers.sort_unstable();
    modifiers.dedup();
    if !key.is_empty() {
        modifiers.push(key);
    }
    modifiers.join("+")
}

fn canonical_key(key: &str) -> String {
    let lower = key.to_ascii_lowercase();
    match lower.as_str() {
        "space" => "Space".into(),
        "enter" | "return" => "Enter".into(),
        "escape" | "esc" => "Escape".into(),
        _ if key.chars().count() == 1 => key.to_ascii_uppercase(),
        _ => {
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Multi-format clipboard payload with a global size cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardPayload {
    formats: BTreeMap<String, Vec<u8>>,
    total_bytes: usize,
    max_bytes: usize,
}

impl ClipboardPayload {
    /// Creates an empty payload with `max_bytes` total capacity.
    pub fn with_limit(max_bytes: usize) -> Self {
        Self {
            formats: BTreeMap::new(),
            total_bytes: 0,
            max_bytes: max_bytes.max(1),
        }
    }

    /// Inserts or replaces one MIME payload.
    pub fn insert(&mut self, mime: &str, bytes: Vec<u8>) -> Result<(), RuntimeError> {
        if !valid_mime(mime) {
            return Err(RuntimeError::InvalidData(format!(
                "invalid clipboard MIME type {mime:?}"
            )));
        }
        let mime = mime.to_ascii_lowercase();
        let previous = self.formats.get(&mime).map(Vec::len).unwrap_or(0);
        let new_total = self
            .total_bytes
            .saturating_sub(previous)
            .saturating_add(bytes.len());
        if new_total > self.max_bytes {
            return Err(RuntimeError::LimitExceeded(format!(
                "clipboard payload is {new_total} bytes; limit is {}",
                self.max_bytes
            )));
        }
        self.formats.insert(mime, bytes);
        self.total_bytes = new_total;
        Ok(())
    }

    /// Retrieves a MIME payload.
    pub fn get(&self, mime: &str) -> Option<&[u8]> {
        self.formats
            .get(&mime.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    /// Lists formats in stable order.
    pub fn formats(&self) -> impl Iterator<Item = &str> {
        self.formats.keys().map(String::as_str)
    }

    /// Total bytes across all formats.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl Default for ClipboardPayload {
    fn default() -> Self {
        Self::with_limit(DEFAULT_CLIPBOARD_LIMIT)
    }
}

fn valid_mime(mime: &str) -> bool {
    let Some((kind, subtype)) = mime.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && mime
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'+' | b'.'))
}

/// Stable recent-file list with deduplication and bounded length.
#[derive(Debug, Clone)]
pub struct RecentFiles {
    capacity: usize,
    entries: VecDeque<PathBuf>,
}

impl RecentFiles {
    /// Creates a list with a maximum length.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    /// Promotes a file to the front.
    pub fn touch(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.entries.retain(|existing| existing != &path);
        self.entries.push_front(path);
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }

    /// Removes files that no longer exist.
    pub fn prune_missing(&mut self) {
        self.entries.retain(|path| path.exists());
    }

    /// Iterates newest-first.
    pub fn iter(&self) -> impl Iterator<Item = &Path> {
        self.entries.iter().map(PathBuf::as_path)
    }
}

/// Computes whether an autosave interval has elapsed.
pub fn autosave_due(last_save: SystemTime, interval: Duration, now: SystemTime) -> bool {
    now.duration_since(last_save)
        .map(|elapsed| elapsed >= interval)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("loom-runtime-{name}-{}", now_millis()));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn settings_roundtrip_preserves_escaped_values() {
        let root = temp_root("settings");
        let path = root.join("settings.conf");
        let mut settings = Settings::default();
        settings.set("editor.theme", "graphite=copper\n高 contrast");
        settings.set("recent path", "/tmp/A B.loomdoc");
        settings.save(&path).expect("save");
        assert_eq!(Settings::load(&path).expect("load"), settings);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovery_store_roundtrip_and_retention() {
        let root = temp_root("recovery");
        let store = RecoveryStore::open(&root)
            .expect("open")
            .with_limits(2, 1024);
        let first = store
            .save_snapshot(
                "doc-1",
                "Draft",
                Some(Path::new("/tmp/draft.loomdoc")),
                b"one",
            )
            .expect("save first");
        store
            .save_snapshot("doc-1", "Draft", None, b"two")
            .expect("save second");
        let last = store
            .save_snapshot("doc-1", "Draft", None, b"three")
            .expect("save third");
        let entries = store.list().expect("list");
        assert_eq!(entries.len(), 2);
        assert_eq!(store.read(&last).expect("read"), b"three");
        assert!(!first.snapshot_path.exists());
        store.discard(&last).expect("discard");
        assert_eq!(store.list().expect("list after discard").len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn diagnostic_log_is_bounded_and_redacted() {
        let mut log = DiagnosticLog::new(2);
        log.add_redaction("/home/alice", "$HOME");
        log.push(LogLevel::Info, "runtime", "opened /home/alice/file");
        log.push(LogLevel::Warn, "runtime", "second");
        log.push(LogLevel::Error, "runtime", "third");
        let entries: Vec<_> = log.entries().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "second");
        assert!(!entries.iter().any(|entry| entry.message.contains("alice")));
    }

    #[test]
    fn shortcut_registry_normalizes_and_rejects_conflicts() {
        let mut shortcuts = ShortcutRegistry::default();
        assert_eq!(
            shortcuts
                .bind("file.save", "shift + ctrl + s")
                .expect("bind"),
            "Ctrl+Shift+S"
        );
        assert_eq!(shortcuts.resolve("CTRL+SHIFT+s"), Some("file.save"));
        let conflict = shortcuts
            .bind("file.save_as", "Ctrl+Shift+S")
            .expect_err("conflict");
        assert_eq!(conflict.existing_command, "file.save");
    }

    #[test]
    fn clipboard_enforces_mime_and_size() {
        let mut payload = ClipboardPayload::with_limit(8);
        payload
            .insert("text/plain", b"hello".to_vec())
            .expect("insert");
        assert_eq!(payload.get("TEXT/PLAIN"), Some(&b"hello"[..]));
        assert!(matches!(
            payload.insert("bad", vec![1]),
            Err(RuntimeError::InvalidData(_))
        ));
        assert!(matches!(
            payload.insert("application/x-loom", vec![0; 9]),
            Err(RuntimeError::LimitExceeded(_))
        ));
    }

    #[test]
    fn recent_files_deduplicate_and_bound() {
        let mut recent = RecentFiles::new(2);
        recent.touch("a");
        recent.touch("b");
        recent.touch("a");
        recent.touch("c");
        let entries: Vec<_> = recent.iter().collect();
        assert_eq!(entries, vec![Path::new("c"), Path::new("a")]);
    }

    #[test]
    fn autosave_due_handles_clock_skew() {
        let start = UNIX_EPOCH + Duration::from_secs(100);
        assert!(autosave_due(
            start,
            Duration::from_secs(30),
            start + Duration::from_secs(30)
        ));
        assert!(!autosave_due(
            start,
            Duration::from_secs(30),
            start - Duration::from_secs(1)
        ));
    }
}
