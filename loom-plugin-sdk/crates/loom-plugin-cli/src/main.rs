fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(loom_plugin_cli::run(args));
}
