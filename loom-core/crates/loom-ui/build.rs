fn main() {
    let path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("ui");
    println!(
        "cargo:rustc-env=SLINT_INCLUDE_GENERATED={}",
        path.join("generated.rs").display()
    );
    // The smoke fixture first: `include_modules!()` only picks up the last
    // compiled file, so the public components must be compiled last.
    slint_build::compile_with_config(
        path.join("smoke.slint"),
        slint_build::CompilerConfiguration::new().with_include_paths(vec![path.clone()]),
    )
    .unwrap();
    slint_build::compile_with_config(
        path.join("components.slint"),
        slint_build::CompilerConfiguration::new().with_include_paths(vec![path.clone()]),
    )
    .unwrap();
}
