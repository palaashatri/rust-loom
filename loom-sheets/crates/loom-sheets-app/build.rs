fn main() {
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
    slint_build::compile_with_config(
        ui.join("app.slint"),
        slint_build::CompilerConfiguration::new().with_include_paths(vec![loom_ui]),
    )
    .unwrap();
}
