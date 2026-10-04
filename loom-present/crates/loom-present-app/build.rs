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
}
