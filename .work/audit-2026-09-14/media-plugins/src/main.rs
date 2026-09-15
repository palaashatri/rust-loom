use loom_plugin_host::{check_permission, InstalledPlugin, ExternalWasmtimeRuntime, PluginInvocation};
use loom_plugin_manifest::{parse_manifest, Capability};
use std::{fs, path::PathBuf, os::unix::fs::{symlink, PermissionsExt}};
fn main() {
    let root = PathBuf::from("/tmp/loom-audit-media/evidence");
    fs::create_dir_all(&root).unwrap();
    let allowed = root.join("allowed");
    let outside = root.join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let _ = fs::remove_file(allowed.join("linked"));
    symlink(&outside, allowed.join("linked")).unwrap();
    let text = fs::read_to_string("/home/patri/code/rust-loom/loom-plugin-sdk/crates/loom-plugin-cli/fixtures/demo/manifest.json").unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["permissions"] = serde_json::json!([{"resource":"file","mode":"write","path_prefix":allowed}]);
    json["resource_limits"]["max_cpu_ms_per_call"] = serde_json::json!(50);
    let manifest = parse_manifest(&json.to_string()).unwrap();
    let plugin = InstalledPlugin { id:manifest.plugin_id.clone(), version:manifest.version.clone(), manifest, install_dir:root.clone(), wasm_path:root.join("module.wasm"), manifest_sha256:[0;32] };
    if std::env::args().nth(1).as_deref() == Some("stdin") {
       let name = b"loom_plugin_invoke";
       let mut wasm = b"\0asm\x01\0\0\0".to_vec();
       wasm.extend_from_slice(&[1, 4, 1, 0x60, 0, 0, 3, 2, 1, 0]);
       wasm.extend_from_slice(&[7, (name.len() + 4) as u8, 1, name.len() as u8]);
       wasm.extend_from_slice(name);
       wasm.extend_from_slice(&[0, 0, 10, 4, 1, 2, 0, 0x0b]);
       fs::write(&plugin.wasm_path, wasm).unwrap();
       let script = root.join("nonreading-runtime");
       fs::write(&script, "#!/bin/sh\nexec sleep 2\n").unwrap();
       fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
       let runtime = ExternalWasmtimeRuntime::new(script).unwrap();
       let mut invocation = PluginInvocation::declared(&plugin).unwrap();
       invocation.stdin = vec![b'x'; 1024*1024];
       let start = std::time::Instant::now();
       let result = runtime.invoke(&invocation);
       println!("Configured deadline=50ms; elapsed={}ms; result={result:?}",start.elapsed().as_millis());
    } else {
       let target = allowed.join("linked/new.txt");
       let _ = fs::remove_file(&target);
       let decision = check_permission(&plugin, &Capability::WriteFile, Some(&target));
       println!("Authorization for {}: {decision:?}",target.display());
       if decision.is_ok() { fs::write(&target,"outside-boundary").unwrap(); }
       println!("Created unauthorized file: {}",outside.join("new.txt").is_file());
    }
}
