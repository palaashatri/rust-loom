fn main() {
    println!("cargo:rerun-if-env-changed=SLINT_EMIT_DEBUG_INFO");
    let path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("ui");
    println!(
        "cargo:rustc-env=SLINT_INCLUDE_GENERATED={}",
        path.join("generated.rs").display()
    );

    let emit_debug_info = std::env::var_os("SLINT_EMIT_DEBUG_INFO").is_some()
        || std::env::var("PROFILE").is_ok_and(|profile| profile == "debug");

    // Compile review/test roots first. `include_modules!()` consumes the last
    // generated module, so public compatibility components remain last until
    // application migration begins.
    for source in [
        "foundation/gallery.slint",
        "foundation/icon-grid.slint",
        "foundation/numeric_field_probe.slint",
        "foundation/local_menu_probe.slint",
        "foundation/rtl-specimen.slint",
        "smoke.slint",
    ] {
        slint_build::compile_with_config(
            path.join(source),
            slint_build::CompilerConfiguration::new()
                .with_include_paths(vec![path.clone()])
                .with_debug_info(emit_debug_info),
        )
        .unwrap();
    }

    slint_build::compile_with_config(
        path.join("components.slint"),
        slint_build::CompilerConfiguration::new()
            .with_include_paths(vec![path.clone()])
            .with_debug_info(emit_debug_info),
    )
    .unwrap();
}
