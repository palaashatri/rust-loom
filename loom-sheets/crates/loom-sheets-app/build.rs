fn main() {
    println!("cargo:rerun-if-env-changed=SLINT_EMIT_DEBUG_INFO");
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui = manifest.join("ui");
    let loom_ui = manifest.join("../../../loom-core/crates/loom-ui/ui");
    println!("cargo:rerun-if-changed={}", ui.join("app.slint").display());
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("components.slint").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("inspector.slint").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("toolbar.slint").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("chart.slint").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("objects.slint").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("template_chooser.slint").display()
    );
    // Slint's ElementHandle assertions need generated debug metadata. Enable it
    // for the normal dev/test profile so plain `cargo test` can inspect the
    // accessibility tree; release builds keep the metadata opt-in.
    let emit_debug_info = std::env::var_os("SLINT_EMIT_DEBUG_INFO").is_some()
        || std::env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    let compiler_config = slint_build::CompilerConfiguration::new()
        .with_include_paths(vec![loom_ui])
        .with_debug_info(emit_debug_info);
    slint_build::compile_with_config(ui.join("app.slint"), compiler_config).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_sheets_windows_icon(&manifest);
    }
}

fn embed_sheets_windows_icon(manifest: &std::path::Path) {
    use std::path::PathBuf;
    use std::process::Command;

    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=WindowsSDKVersion");
    let icon = manifest
        .join("../../../loom-core/crates/loom-ui/ui/app-icons/loom-sheets.ico")
        .canonicalize()
        .expect("canonical Sheets ICO must exist");
    println!("cargo:rerun-if-changed={}", icon.display());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let resource_script = out_dir.join("loom-sheets-icon.rc");
    let resource_file = out_dir.join("loom-sheets-icon.res");
    let escaped_icon_path = icon
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    std::fs::write(
        &resource_script,
        format!("1 ICON \"{escaped_icon_path}\"\n"),
    )
    .expect("write Sheets icon resource script");

    let compiler = sheets_resource_compiler();
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource_file)
        .arg(&resource_script)
        .status()
        .expect("run Windows SDK resource compiler for Sheets icon");
    assert!(
        status.success(),
        "Windows resource compiler failed for Sheets icon"
    );

    println!(
        "cargo:rustc-link-arg-bin=loom-sheets={}",
        resource_file.display()
    );
}

fn sheets_resource_compiler() -> std::ffi::OsString {
    if let Some(compiler) = std::env::var_os("RC") {
        return compiler;
    }

    let resource_arch = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let mut sdk_candidates = Vec::new();
    if let Some(sdk_dir) = std::env::var_os("WindowsSdkDir") {
        let sdk_dir = std::path::PathBuf::from(sdk_dir);
        if let Some(version) = std::env::var_os("WindowsSDKVersion") {
            sdk_candidates.push(
                sdk_dir
                    .join("bin")
                    .join(version)
                    .join(resource_arch)
                    .join("rc.exe"),
            );
        }
        sdk_candidates.push(sdk_dir.join("bin").join(resource_arch).join("rc.exe"));
    }
    sdk_candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(std::path::PathBuf::into_os_string)
        .unwrap_or_else(|| "rc.exe".into())
}
