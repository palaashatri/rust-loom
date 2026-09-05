//! Integration tests for `loom-plugin-host`: zip installation, security
//! rejection, uninstall, and permission checks.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use loom_plugin_host::{check_permission, HostError, InstalledPlugin, PluginStore};
use loom_plugin_manifest::{parse_manifest, Capability};
use sha2::{Digest, Sha256};

/// Minimal valid wasm module (header only; never executed by tests).
const MINIMAL_WASM: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

/// Test-local temp directory that removes itself on drop.
struct TempDir(PathBuf);

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let unique = format!(
            "loom-host-test-{}-{}-{label}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest_json() -> String {
    r#"{
        "manifest_version": 1,
        "plugin_id": "demo-actions",
        "name": "Demo Actions",
        "description": "Integration test plugin.",
        "version": "0.1.0",
        "author": "Loom",
        "license": "MIT OR Apache-2.0",
        "entry": {
            "kind": "command",
            "wasm_module": "module.wasm",
            "function": "loom_plugin_invoke"
        },
        "capabilities": ["read-file", "http-request"],
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
            "network": true
        }
    }"#
    .to_string()
}

fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::FileOptions::default();
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }
    buf
}

fn make_symlink_zip() -> Vec<u8> {
    // zip 0.6.6's `FileOptions::unix_permissions` masks to 0o777, so it
    // cannot express a symlink entry. Craft a minimal single-entry "stored"
    // zip by hand with S_IFLNK (0o120000) set in the unix external
    // attributes, exactly as real archivers (`zip -y`) emit.
    let name = b"module.wasm";
    let target = b"target-marker";
    let external_attributes: u32 = 0o120777 << 16;

    let mut buf: Vec<u8> = Vec::new();
    // Local file header.
    buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
    buf.extend_from_slice(&0u16.to_le_bytes()); // flags
    buf.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
    buf.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
    buf.extend_from_slice(&crc32(target).to_le_bytes());
    buf.extend_from_slice(&(target.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(target.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
    buf.extend_from_slice(name);
    buf.extend_from_slice(target);

    // Central directory header. The local header starts at offset 0; the
    // central directory starts right after the local data.
    let local_offset = 0u32;
    let central_offset = buf.len() as u32;
    let mut central: Vec<u8> = Vec::new();
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&0x031eu16.to_le_bytes()); // made by: unix, 1.30
    central.extend_from_slice(&20u16.to_le_bytes()); // version needed
    central.extend_from_slice(&0u16.to_le_bytes()); // flags
    central.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    central.extend_from_slice(&0u16.to_le_bytes()); // mod time
    central.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
    central.extend_from_slice(&crc32(target).to_le_bytes());
    central.extend_from_slice(&(target.len() as u32).to_le_bytes());
    central.extend_from_slice(&(target.len() as u32).to_le_bytes());
    central.extend_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes()); // extra len
    central.extend_from_slice(&0u16.to_le_bytes()); // comment len
    central.extend_from_slice(&0u16.to_le_bytes()); // disk start
    central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    central.extend_from_slice(&external_attributes.to_le_bytes());
    central.extend_from_slice(&local_offset.to_le_bytes());
    central.extend_from_slice(name);

    // End of central directory.
    buf.extend_from_slice(&central);
    buf.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
    buf.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    buf.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
    buf.extend_from_slice(&1u16.to_le_bytes()); // total entries
    buf.extend_from_slice(&(central.len() as u32).to_le_bytes());
    buf.extend_from_slice(&central_offset.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
    buf
}

/// Bitwise CRC-32 (IEEE 802.3 polynomial), table-less; adequate for tiny
/// test fixtures.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn valid_zip() -> Vec<u8> {
    make_zip(&[
        ("manifest.json", manifest_json().as_bytes()),
        ("module.wasm", MINIMAL_WASM),
        ("assets/notes.txt", b"hello from the plugin package"),
    ])
}

fn store_entries(store_dir: &Path) -> Vec<String> {
    fs::read_dir(store_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn install_extracts_and_verifies_sha() {
    let tmp = TempDir::new("install");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = valid_zip();

    let installed = store.install_zip(&zip).unwrap();
    assert_eq!(installed.id, "demo-actions");
    assert_eq!(installed.version, "0.1.0");

    let install_dir = tmp.path().join("demo-actions@0.1.0");
    assert!(install_dir.join("manifest.json").is_file());
    assert!(install_dir.join("module.wasm").is_file());
    assert!(install_dir.join("assets/notes.txt").is_file());
    assert!(tmp.path().join("installed.json").is_file());

    let manifest_bytes = fs::read(install_dir.join("manifest.json")).unwrap();
    let expected: [u8; 32] = Sha256::digest(&manifest_bytes).into();
    assert_eq!(installed.manifest_sha256, expected);

    assert_eq!(store.get("demo-actions").unwrap().id, "demo-actions");
    assert!(store.get("nope").is_none());

    let listed = store.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "demo-actions");
    assert_eq!(listed[0].wasm_path, install_dir.join("module.wasm"));
}

#[test]
fn install_twice_is_rejected() {
    let tmp = TempDir::new("twice");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = valid_zip();
    store.install_zip(&zip).unwrap();
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::AlreadyInstalled), "got {err:?}");
}

#[test]
fn malicious_traversal_zip_is_rejected_and_nothing_extracted() {
    let tmp = TempDir::new("evil");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = make_zip(&[
        ("manifest.json", manifest_json().as_bytes()),
        ("module.wasm", MINIMAL_WASM),
        ("../evil.txt", b"escape"),
    ]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::UnsafePath(_)), "got {err:?}");
    assert_eq!(store.list().len(), 0);
    let names = store_entries(tmp.path());
    assert_eq!(names, vec!["installed.json".to_string()]);
}

#[test]
fn absolute_path_entry_is_rejected() {
    let tmp = TempDir::new("absolute");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = make_zip(&[
        ("manifest.json", manifest_json().as_bytes()),
        ("/etc/cron.d/loom", b"escape"),
    ]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::UnsafePath(_)), "got {err:?}");
    assert!(store.list().is_empty());
}

#[test]
fn symlink_entry_is_rejected() {
    let tmp = TempDir::new("symlink");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = make_symlink_zip();
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::UnsafePath(_)), "got {err:?}");
    assert!(store.list().is_empty());
}

#[test]
fn invalid_manifest_version_is_rejected_and_nothing_extracted() {
    let tmp = TempDir::new("badversion");
    let store = PluginStore::open(tmp.path()).unwrap();
    let bad = manifest_json().replace("\"manifest_version\": 1", "\"manifest_version\": 9");
    let zip = make_zip(&[
        ("manifest.json", bad.as_bytes()),
        ("module.wasm", MINIMAL_WASM),
    ]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::InvalidManifest(_)), "got {err:?}");
    assert!(store.list().is_empty());
    assert_eq!(
        store_entries(tmp.path()),
        vec!["installed.json".to_string()]
    );
}

#[test]
fn unknown_capability_manifest_is_rejected() {
    let tmp = TempDir::new("badcap");
    let store = PluginStore::open(tmp.path()).unwrap();
    let bad = manifest_json().replace("\"read-file\"", "\"read-everything\"");
    let zip = make_zip(&[
        ("manifest.json", bad.as_bytes()),
        ("module.wasm", MINIMAL_WASM),
    ]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::InvalidManifest(_)), "got {err:?}");
    assert!(store.list().is_empty());
}

#[test]
fn missing_manifest_is_rejected() {
    let tmp = TempDir::new("nomanifest");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = make_zip(&[("module.wasm", MINIMAL_WASM)]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::InvalidManifest(_)), "got {err:?}");
}

#[test]
fn missing_wasm_module_is_rejected() {
    let tmp = TempDir::new("nowasm");
    let store = PluginStore::open(tmp.path()).unwrap();
    let zip = make_zip(&[("manifest.json", manifest_json().as_bytes())]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::InvalidManifest(_)), "got {err:?}");
    assert!(store.list().is_empty());
}

#[test]
fn api_range_mismatch_is_rejected() {
    let tmp = TempDir::new("apimismatch");
    let store = PluginStore::open(tmp.path()).unwrap();
    // Internally consistent range that does not overlap the host's
    // 0.1.0..=0.9.0.
    let bad = manifest_json()
        .replace(
            "\"api_min_version\": \"0.1.0\"",
            "\"api_min_version\": \"2.0.0\"",
        )
        .replace(
            "\"api_max_version\": \"0.9.0\"",
            "\"api_max_version\": \"2.5.0\"",
        );
    let zip = make_zip(&[
        ("manifest.json", bad.as_bytes()),
        ("module.wasm", MINIMAL_WASM),
    ]);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::UnsupportedApi(_)), "got {err:?}");
    assert!(store.list().is_empty());
}

#[test]
fn archive_with_too_many_entries_is_rejected() {
    let tmp = TempDir::new("toomany");
    let store = PluginStore::open(tmp.path()).unwrap();
    let names: Vec<String> = (0..loom_plugin_host::MAX_ENTRIES + 1)
        .map(|i| format!("f{i}.txt"))
        .collect();
    let mut entries: Vec<(&str, &[u8])> =
        names.iter().map(|n| (n.as_str(), "x".as_bytes())).collect();
    let manifest = manifest_json();
    entries.push(("manifest.json", manifest.as_bytes()));
    let zip = make_zip(&entries);
    let err = store.install_zip(&zip).unwrap_err();
    assert!(matches!(err, HostError::TooLarge), "got {err:?}");
    assert!(store.list().is_empty());
}

#[test]
fn not_a_zip_is_rejected() {
    let tmp = TempDir::new("notzip");
    let store = PluginStore::open(tmp.path()).unwrap();
    let err = store
        .install_zip(b"definitely not a zip archive")
        .unwrap_err();
    assert!(matches!(err, HostError::Zip(_)), "got {err:?}");
}

#[test]
fn uninstall_removes_everything() {
    let tmp = TempDir::new("uninstall");
    let store = PluginStore::open(tmp.path()).unwrap();
    store.install_zip(&valid_zip()).unwrap();

    store.uninstall("demo-actions").unwrap();
    assert!(store.get("demo-actions").is_none());
    assert!(store.list().is_empty());
    assert!(!tmp.path().join("demo-actions@0.1.0").exists());

    let err = store.uninstall("demo-actions").unwrap_err();
    assert!(matches!(err, HostError::NotFound), "got {err:?}");
}

#[test]
fn list_skips_corrupt_install_dirs() {
    let tmp = TempDir::new("corrupt");
    let store = PluginStore::open(tmp.path()).unwrap();
    store.install_zip(&valid_zip()).unwrap();
    fs::write(
        tmp.path().join("demo-actions@0.1.0/manifest.json"),
        b"{ broken json",
    )
    .unwrap();
    assert!(store.list().is_empty());
}

#[test]
fn check_permission_matrix() {
    let tmp = TempDir::new("perms");
    let store = PluginStore::open(tmp.path()).unwrap();
    let installed: InstalledPlugin = store.install_zip(&valid_zip()).unwrap();

    // Capability present and path inside the granted prefix.
    let allowed_file = tmp.path().join("demo-actions@0.1.0/assets/notes.txt");
    assert!(allowed_file.exists());
    assert!(check_permission(&installed, &Capability::ReadFile, Some(&allowed_file)).is_ok());

    // Same capability, path outside every granted prefix.
    let outside = tmp.path().join("demo-actions@0.1.0/module.wasm");
    assert!(check_permission(&installed, &Capability::ReadFile, Some(&outside)).is_err());

    // Capability not declared at all.
    assert!(check_permission(&installed, &Capability::WriteFile, Some(&allowed_file)).is_err());
    assert!(check_permission(&installed, &Capability::ClipboardRead, None).is_err());

    // HttpRequest: capability declared and network limit on -> allowed.
    assert!(check_permission(&installed, &Capability::HttpRequest, None).is_ok());

    // Prefix matching must not match partial names.
    let partial = tmp.path().join("demo-actions@0.1.0/assets2/file.txt");
    fs::create_dir_all(partial.parent().unwrap()).unwrap();
    fs::write(&partial, b"x").unwrap();
    assert!(check_permission(&installed, &Capability::ReadFile, Some(&partial)).is_err());
}

#[test]
fn check_permission_network_limit_blocks_http() {
    let tmp = TempDir::new("nonet");
    let store = PluginStore::open(tmp.path()).unwrap();
    let json = manifest_json().replace("\"network\": true", "\"network\": false");
    let zip = make_zip(&[
        ("manifest.json", json.as_bytes()),
        ("module.wasm", MINIMAL_WASM),
    ]);
    let installed = store.install_zip(&zip).unwrap();
    let err = check_permission(&installed, &Capability::HttpRequest, None).unwrap_err();
    assert!(matches!(err, HostError::Denied(_)), "got {err:?}");
}

#[test]
fn check_permission_create_mode_satisfies_write() {
    let tmp = TempDir::new("create");
    let store = PluginStore::open(tmp.path()).unwrap();
    let json = manifest_json()
        .replace(
            r#"{ "resource": "file", "mode": "read", "path_prefix": "assets" }"#,
            r#"{ "resource": "file", "mode": "create", "path_prefix": "outbox" }"#,
        )
        .replace(
            r#""capabilities": ["read-file", "http-request"]"#,
            r#""capabilities": ["read-file", "write-file", "http-request"]"#,
        );
    let zip = make_zip(&[
        ("manifest.json", json.as_bytes()),
        ("module.wasm", MINIMAL_WASM),
    ]);
    let installed = store.install_zip(&zip).unwrap();

    let target = tmp.path().join("demo-actions@0.1.0/outbox/new.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"x").unwrap();
    assert!(check_permission(&installed, &Capability::WriteFile, Some(&target)).is_ok());

    let read_target = tmp.path().join("demo-actions@0.1.0/outbox/new.txt");
    assert!(check_permission(&installed, &Capability::ReadFile, Some(&read_target)).is_err());
}

#[test]
fn index_is_regenerated_from_disk_on_open() {
    let tmp = TempDir::new("regen");
    let store = PluginStore::open(tmp.path()).unwrap();
    store.install_zip(&valid_zip()).unwrap();

    // Sabotage the index; a fresh open must rebuild it from disk.
    fs::write(tmp.path().join("installed.json"), b"[]").unwrap();
    let reopened = PluginStore::open(tmp.path()).unwrap();
    let index = fs::read_to_string(tmp.path().join("installed.json")).unwrap();
    assert!(index.contains("demo-actions"));
    assert_eq!(reopened.list().len(), 1);
}

#[test]
fn installed_plugin_manifest_round_trips_through_parse() {
    let tmp = TempDir::new("roundtrip");
    let store = PluginStore::open(tmp.path()).unwrap();
    let installed = store.install_zip(&valid_zip()).unwrap();
    // The manifest stored on disk must equal what the host parsed.
    let on_disk = fs::read_to_string(tmp.path().join("demo-actions@0.1.0/manifest.json")).unwrap();
    let reparsed = parse_manifest(&on_disk).unwrap();
    assert_eq!(reparsed, installed.manifest);
}
