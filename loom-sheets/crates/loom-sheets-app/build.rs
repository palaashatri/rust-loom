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
    println!(
        "cargo:rerun-if-changed={}",
        ui.join("local_menu.slint").display()
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
}
