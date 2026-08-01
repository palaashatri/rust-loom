//! # loom-plugin-cli
//!
//! A small command line tool for Loom plugin packages: `validate`, `install`,
//! `list`, and `remove`. Argument parsing is hand-rolled (no clap).
//!
//! It also hosts the demo-package fixture builder used by tests and by the
//! documented workflow in `BUILDING.md`: running `cargo test -p loom-plugin-cli`
//! produces `target/fixtures/demo.loomplugin` from the committed fixture
//! sources in `fixtures/demo/`.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod fixture;

use std::path::{Path, PathBuf};

use loom_plugin_host::{HostError, PluginStore};

/// Run the CLI with `args` (everything after `argv[0]`) and return the
/// process exit code: 0 on success, 1 on operational or validation errors,
/// 2 on usage errors.
pub fn run(args: Vec<String>) -> i32 {
    let Some(command) = args.first() else {
        eprintln!("{}", usage());
        return 2;
    };
    match command.as_str() {
        "--help" | "-h" | "help" => {
            println!("{}", usage());
            0
        }
        "--version" | "-V" => {
            println!("loom-plugin {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "validate" => match args.get(1) {
            Some(path) => cmd_validate(path),
            None => {
                eprintln!("usage: loom-plugin validate <manifest.json>\n");
                2
            }
        },
        "install" => cmd_install(&args[1..]),
        "list" => match parse_dir(&args[1..]) {
            Ok(Some(dir)) => cmd_list(&dir),
            Ok(None) => {
                eprintln!("usage: loom-plugin list --dir <store_dir>\n");
                2
            }
            Err(msg) => {
                eprintln!("{msg}\n");
                2
            }
        },
        "remove" => cmd_remove(&args[1..]),
        other => {
            eprintln!("unknown command: {other}\n");
            eprintln!("{}", usage());
            2
        }
    }
}

fn usage() -> String {
    format!(
        "loom-plugin {} — Loom plugin package tool\n\
         \n\
         Usage:\n\
         \x20 loom-plugin validate <manifest.json>\n\
         \x20 loom-plugin install <file.loomplugin> --dir <store_dir>\n\
         \x20 loom-plugin list --dir <store_dir>\n\
         \x20 loom-plugin remove <id> --dir <store_dir>\n\
         \x20 loom-plugin --help | --version\n\
         \n\
         Options:\n\
         \x20 --dir <dir>   plugin store directory\n\
         \x20 -h, --help    show this help\n\
         \x20 -V, --version show version",
        env!("CARGO_PKG_VERSION")
    )
}

/// Extract `--dir <value>` or `--dir=<value>` from the arguments.
fn parse_dir(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--dir=") {
            return Ok(Some(PathBuf::from(value)));
        }
        if arg == "--dir" {
            return match iter.next() {
                Some(value) => Ok(Some(PathBuf::from(value))),
                None => Err("--dir requires a value".into()),
            };
        }
        if arg.starts_with('-') && arg != "-" {
            return Err(format!("unknown option: {arg}"));
        }
    }
    Ok(None)
}

fn cmd_validate(manifest_path: &str) -> i32 {
    let bytes = match std::fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read {manifest_path}: {e}");
            return 1;
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("{manifest_path} is not valid UTF-8: {e}");
            return 1;
        }
    };
    match loom_plugin_manifest::parse_manifest(text) {
        Ok(m) => {
            let entry = &m.entry;
            let limits = &m.resource_limits;
            println!("manifest OK");
            println!("  id: {}", m.plugin_id);
            println!("  name: {}", m.name);
            println!("  version: {}", m.version);
            println!(
                "  entry: {} {} {}",
                entry.kind.as_str(),
                entry.wasm_module,
                entry.function
            );
            println!(
                "  api: {}..={} (host supports {}..={})",
                m.api_min_version,
                m.api_max_version,
                loom_plugin_host::HOST_API_MIN_VERSION,
                loom_plugin_host::HOST_API_MAX_VERSION
            );
            let caps: Vec<&str> = m.capabilities.iter().map(|c| c.as_str()).collect();
            println!("  capabilities: {}", caps.join(", "));
            println!("  permissions: {}", m.permissions.len());
            println!(
                "  resource limits: memory {} bytes, fs {} bytes, {} entries, {} ms/call, network {}",
                limits.max_memory_bytes,
                limits.max_fs_bytes,
                limits.max_fs_entries,
                limits.max_cpu_ms_per_call,
                if limits.network { "yes" } else { "no" }
            );
            0
        }
        Err(e) => {
            eprintln!("validation failed: {e}");
            1
        }
    }
}

fn cmd_install(args: &[String]) -> i32 {
    let Some(zip_path) = args.first().filter(|a| !a.starts_with('-')) else {
        eprintln!("usage: loom-plugin install <file.loomplugin> --dir <store_dir>\n");
        return 2;
    };
    let store_dir = match parse_dir(args) {
        Ok(Some(dir)) => dir,
        Ok(None) => {
            eprintln!("usage: loom-plugin install <file.loomplugin> --dir <store_dir>\n");
            return 2;
        }
        Err(msg) => {
            eprintln!("{msg}\n");
            return 2;
        }
    };
    let bytes = match std::fs::read(zip_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read {zip_path}: {e}");
            return 1;
        }
    };
    let store = match PluginStore::open(&store_dir) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("cannot open store {:?}: {e}", store_dir);
            return 1;
        }
    };
    match store.install_zip(&bytes) {
        Ok(plugin) => {
            println!(
                "Installed {} {} (sha256 {}) -> {}",
                plugin.id,
                plugin.version,
                hex(&plugin.manifest_sha256),
                plugin.install_dir.display()
            );
            0
        }
        Err(e) => {
            eprintln!("install failed: {e}");
            1
        }
    }
}

fn cmd_list(store_dir: &Path) -> i32 {
    let store = match PluginStore::open(store_dir) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("cannot open store {:?}: {e}", store_dir);
            return 1;
        }
    };
    let plugins = store.list();
    if plugins.is_empty() {
        println!("no plugins installed in {}", store_dir.display());
        return 0;
    }
    let id_w = plugins.iter().map(|p| p.id.len()).max().unwrap_or(0);
    let kind_w = plugins
        .iter()
        .map(|p| p.manifest.entry.kind.as_str().len())
        .max()
        .unwrap_or(0);
    let module_w = plugins
        .iter()
        .map(|p| p.manifest.entry.wasm_module.len())
        .max()
        .unwrap_or(0);
    println!(
        "{:<id_w$}  {:<10}  {:<kind_w$}  {:<module_w$}  FUNCTION",
        "ID", "VERSION", "KIND", "MODULE",
    );
    for p in &plugins {
        println!(
            "{:<id_w$}  {:<10}  {:<kind_w$}  {:<module_w$}  {}",
            p.id,
            p.version,
            p.manifest.entry.kind.as_str(),
            p.manifest.entry.wasm_module,
            p.manifest.entry.function,
        );
    }
    0
}

fn cmd_remove(args: &[String]) -> i32 {
    let Some(id) = args.first().filter(|a| !a.starts_with('-')) else {
        eprintln!("usage: loom-plugin remove <id> --dir <store_dir>\n");
        return 2;
    };
    let store_dir = match parse_dir(args) {
        Ok(Some(dir)) => dir,
        Ok(None) => {
            eprintln!("usage: loom-plugin remove <id> --dir <store_dir>\n");
            return 2;
        }
        Err(msg) => {
            eprintln!("{msg}\n");
            return 2;
        }
    };
    let store = match PluginStore::open(&store_dir) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("cannot open store {:?}: {e}", store_dir);
            return 1;
        }
    };
    match store.uninstall(id) {
        Ok(()) => {
            println!("Removed {id}");
            0
        }
        Err(HostError::NotFound) => {
            eprintln!("no such installed plugin: {id}");
            1
        }
        Err(e) => {
            eprintln!("remove failed: {e}");
            1
        }
    }
}

/// Lowercase hex encoding of a byte slice (no external hex crate needed).
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}
