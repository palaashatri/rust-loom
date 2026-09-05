//! Demo-package fixture sources and zip builder.
//!
//! The fixture sources are committed as plain files under `fixtures/demo/`
//! (a manifest, an 8-byte minimal wasm module, and an assets file). The zip
//! itself is generated, never committed: `cargo test -p loom-plugin-cli`
//! produces `target/fixtures/demo.loomplugin` from these sources.

use std::path::PathBuf;

/// Committed demo manifest document.
pub const DEMO_MANIFEST: &str = include_str!("../fixtures/demo/manifest.json");
/// Committed 8-byte minimal wasm module (header only; never executed).
pub const DEMO_WASM: &[u8] = include_bytes!("../fixtures/demo/module.wasm");
/// Committed demo asset file.
pub const DEMO_NOTES: &str = include_str!("../fixtures/demo/assets/notes.txt");

/// Path of the committed demo manifest source file.
pub fn demo_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo/manifest.json")
}

/// Build the demo `.loomplugin` zip in memory.
pub fn build_demo_fixture_zip() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        std::io::Write::write_all(&mut writer, DEMO_MANIFEST.as_bytes()).unwrap();
        writer.start_file("module.wasm", options).unwrap();
        std::io::Write::write_all(&mut writer, DEMO_WASM).unwrap();
        writer.start_file("assets/notes.txt", options).unwrap();
        std::io::Write::write_all(&mut writer, DEMO_NOTES.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
    buf
}

/// Write the demo zip to `target/fixtures/demo.loomplugin` and return its
/// path. Used by the fixture-generation test and by integration tests.
pub fn write_demo_fixture_zip() -> PathBuf {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/fixtures/demo.loomplugin");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::fs::write(&out, build_demo_fixture_zip()).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct TempStore(PathBuf);

    impl TempStore {
        fn new() -> TempStore {
            let path = std::env::temp_dir().join(format!(
                "loom-cli-fixture-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempStore(path)
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fixture_sources_are_valid() {
        let manifest = loom_plugin_manifest::parse_manifest(DEMO_MANIFEST)
            .expect("demo manifest must parse and validate");
        assert_eq!(manifest.plugin_id, "demo-actions");
        assert_eq!(manifest.entry.wasm_module, "module.wasm");
        assert_eq!(DEMO_WASM.len(), 8);
    }

    #[test]
    fn fixture_zip_generates_into_target() {
        let zip_path = write_demo_fixture_zip();
        assert!(zip_path.is_file(), "fixture zip was not written");

        // The generated package must install cleanly into a fresh store.
        let tmp = TempStore::new();
        let store = loom_plugin_host::PluginStore::open(Path::new(&tmp.0)).unwrap();
        let installed = store.install_zip(&build_demo_fixture_zip()).unwrap();
        assert_eq!(installed.id, "demo-actions");
        assert_eq!(installed.version, "0.1.0");
        assert_eq!(store.list().len(), 1);
    }
}
