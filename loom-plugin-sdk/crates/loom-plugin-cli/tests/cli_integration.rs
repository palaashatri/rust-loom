//! End-to-end tests for the `loom-plugin` binary: validate, install, list,
//! and remove against the generated demo fixture.

use std::path::{Path, PathBuf};
use std::process::Command;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "loom-cli-test-{}-{}-{label}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_loom-plugin")
}

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("spawn loom-plugin");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn fixture_zip_is_generated() {
    let zip_path = loom_plugin_cli::fixture::write_demo_fixture_zip();
    assert!(zip_path.is_file());
}

#[test]
fn validate_accepts_the_demo_manifest() {
    let manifest = loom_plugin_cli::fixture::demo_manifest_path();
    let (code, stdout, stderr) = run(&["validate", manifest.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("manifest OK"));
    assert!(stdout.contains("demo-actions"));
    assert!(stdout.contains("read-file"));
}

#[test]
fn validate_rejects_a_bad_manifest() {
    let tmp = TempDir::new("validate-bad");
    let bad = tmp.path().join("bad.json");
    std::fs::write(&bad, r#"{ "manifest_version": 99 }"#).unwrap();
    let (code, _stdout, stderr) = run(&["validate", bad.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("validation failed"));
}

#[test]
fn validate_missing_file_fails() {
    let (code, _stdout, stderr) = run(&["validate", "/nonexistent/manifest.json"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot read"));
}

#[test]
fn install_list_remove_round_trip() {
    let tmp = TempDir::new("roundtrip");
    let store = tmp.path().join("store");
    let zip = loom_plugin_cli::fixture::write_demo_fixture_zip();
    let zip = zip.to_str().unwrap();
    let dir = store.to_str().unwrap();

    let (code, stdout, stderr) = run(&["install", zip, "--dir", dir]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("Installed demo-actions 0.1.0"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("sha256"), "stdout: {stdout}");
    assert!(store.join("demo-actions@0.1.0/manifest.json").is_file());
    assert!(store.join("demo-actions@0.1.0/module.wasm").is_file());
    assert!(store.join("demo-actions@0.1.0/assets/notes.txt").is_file());

    let (code, stdout, stderr) = run(&["list", "--dir", dir]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("demo-actions"), "stdout: {stdout}");
    assert!(stdout.contains("0.1.0"), "stdout: {stdout}");

    // Installing the same package twice must fail.
    let (code, _stdout, stderr) = run(&["install", zip, "--dir", dir]);
    assert_eq!(code, 1);
    assert!(stderr.contains("already installed"));

    let (code, stdout, stderr) = run(&["remove", "demo-actions", "--dir", dir]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("Removed demo-actions"));
    assert!(!store.join("demo-actions@0.1.0").exists());

    let (code, stdout, _stderr) = run(&["list", "--dir", dir]);
    assert_eq!(code, 0);
    assert!(stdout.contains("no plugins installed"));

    // Removing again must fail with a clear error.
    let (code, _stdout, stderr) = run(&["remove", "demo-actions", "--dir", dir]);
    assert_eq!(code, 1);
    assert!(stderr.contains("no such installed plugin"));
}

#[test]
fn install_missing_file_fails() {
    let tmp = TempDir::new("install-missing");
    let (code, _stdout, stderr) = run(&[
        "install",
        "/nonexistent/plugin.loomplugin",
        "--dir",
        tmp.path().join("store").to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot read"));
}

#[test]
fn install_rejects_malicious_zip() {
    let tmp = TempDir::new("install-evil");
    let store = tmp.path().join("store");

    // A zip with a traversal entry, built in memory.
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::FileOptions::default();
        writer.start_file("../evil.txt", options).unwrap();
        std::io::Write::write_all(&mut writer, b"escape").unwrap();
        writer.finish().unwrap();
    }
    let evil = tmp.path().join("evil.loomplugin");
    std::fs::write(&evil, &buf).unwrap();

    let (code, _stdout, stderr) = run(&[
        "install",
        evil.to_str().unwrap(),
        "--dir",
        store.to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("unsafe entry path"), "stderr: {stderr}");
    // The store directory may exist (created on open) but must not contain
    // any extracted plugin files.
    let leftover: Vec<String> = std::fs::read_dir(&store)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "installed.json")
        .collect();
    assert!(leftover.is_empty(), "unexpected files: {leftover:?}");
}

#[test]
fn usage_errors_exit_2() {
    let (code, _stdout, stderr) = run(&["list"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("usage"));

    let (code, _stdout, stderr) = run(&["frobnicate"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown command"));

    let (code, stdout, _stderr) = run(&["--version"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("loom-plugin 0.1.0"), "stdout: {stdout}");
}
