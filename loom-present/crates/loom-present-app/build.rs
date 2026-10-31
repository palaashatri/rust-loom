fn main() {
    println!("cargo:rerun-if-env-changed=SLINT_EMIT_DEBUG_INFO");
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui = manifest.join("ui");
    let loom_ui = manifest.join("../../../loom-core/crates/loom-ui/ui");
    println!("cargo:rerun-if-changed={}", ui.join("app.slint").display());
    // ElementHandle (accessibility-tree tests) needs generated debug metadata;
    // debug builds enable it, release builds do not carry it.
    let emit_debug_info = std::env::var_os("SLINT_EMIT_DEBUG_INFO").is_some()
        || std::env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    slint_build::compile_with_config(
        ui.join("app.slint"),
        slint_build::CompilerConfiguration::new()
            .with_include_paths(vec![loom_ui])
            .with_debug_info(emit_debug_info),
    )
    .unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_present_windows_icon(&manifest);
    }
}

fn embed_present_windows_icon(manifest: &std::path::Path) {
    use std::path::PathBuf;
    use std::process::Command;

    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=WindowsSDKVersion");
    let icon = manifest
        .join("../../../loom-core/crates/loom-ui/ui/app-icons/loom-present.ico")
        .canonicalize()
        .expect("canonical Present ICO must exist");
    println!("cargo:rerun-if-changed={}", icon.display());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let resource_script = out_dir.join("loom-present-icon.rc");
    let resource_file = out_dir.join("loom-present-icon.res");
    let escaped_icon_path = icon
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    std::fs::write(
        &resource_script,
        format!("1 ICON \"{escaped_icon_path}\"\n"),
    )
    .expect("write Present icon resource script");

    let compiler = present_resource_compiler();
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource_file)
        .arg(&resource_script)
        .status()
        .expect("run Windows SDK resource compiler for Present icon");
    assert!(
        status.success(),
        "Windows resource compiler failed for Present icon"
    );

    println!(
        "cargo:rustc-link-arg-bin=loom-present={}",
        resource_file.display()
    );
}

fn present_resource_compiler() -> std::ffi::OsString {
    if let Some(compiler) = std::env::var_os("RC") {
        return compiler;
    }

    let mut sdk_candidates = Vec::new();
    if let Some(sdk_dir) = std::env::var_os("WindowsSdkDir") {
        let sdk_dir = std::path::PathBuf::from(sdk_dir);
        if let Some(version) = std::env::var_os("WindowsSDKVersion") {
            sdk_candidates.push(sdk_dir.join("bin").join(version).join("x64").join("rc.exe"));
        }
        sdk_candidates.push(sdk_dir.join("bin").join("x64").join("rc.exe"));
    }
    sdk_candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(std::path::PathBuf::into_os_string)
        .unwrap_or_else(|| "rc.exe".into())
}
