fn main() {
    let path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("ui");
    println!(
        "cargo:rustc-env=SLINT_INCLUDE_GENERATED={}",
        path.join("generated.rs").display()
    );

    // Compile review/test roots first. `include_modules!()` consumes the last
    // generated module, so public compatibility components remain last until
    // application migration begins.
    for source in ["foundation/gallery.slint", "smoke.slint"] {
        slint_build::compile_with_config(
            path.join(source),
            slint_build::CompilerConfiguration::new().with_include_paths(vec![path.clone()]),
        )
        .unwrap();
    }

    slint_build::compile_with_config(
        path.join("components.slint"),
        slint_build::CompilerConfiguration::new().with_include_paths(vec![path.clone()]),
    )
    .unwrap();
}
