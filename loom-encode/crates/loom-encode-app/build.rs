fn main() {
    let config = slint_build::CompilerConfiguration::new().with_include_paths(vec![
        std::path::PathBuf::from("../../../loom-core/crates/loom-ui/ui"),
    ]);
    slint_build::compile_with_config("ui/app.slint", config).unwrap();
}
