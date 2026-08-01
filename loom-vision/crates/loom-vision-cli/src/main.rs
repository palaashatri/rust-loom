//! # loom-vision — command-line interface
//!
//! A small CLI for exercising the Loom Vision framework without a GUI:
//! inspect model packs, decode QR codes, compute image statistics, and
//! benchmark the reference QR provider.
//!
//! All user-facing output goes to stdout; errors go to stderr. Exit codes:
//! `0` success, `1` runtime error, `2` usage error.

#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use loom_vision_core::model_pack::validate_pack;
use loom_vision_core::provider::{CapabilityProvider, ProviderInput, ProviderOutput, RunContext};
use loom_vision_core::reference::{ImageStatsProvider, QrCodeProvider, NO_QR_CODE_MESSAGE};
use loom_vision_core::VisionError;

const USAGE: &str = "\
loom-vision <command> [args]

Commands:
  inspect-pack <dir>    Validate a model pack directory and print its summary
  qr <image>            Decode a QR code from an image file
  stats <image>         Compute mean luma, std luma, and contrast of an image
  bench <image>         Run QR decoding 20 times; print min/median/max ms
  help                  Show this help

All output goes to stdout; errors to stderr.
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print!("{USAGE}");
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "inspect-pack" => inspect_pack(&args[1..]),
        "qr" => qr(&args[1..]),
        "stats" => stats(&args[1..]),
        "bench" => bench(&args[1..]),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown command '{other}'");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn one_arg(args: &[String]) -> Result<PathBuf, ExitCode> {
    match args {
        [path] => Ok(PathBuf::from(path)),
        [] => {
            eprintln!("error: missing argument");
            Err(ExitCode::from(2))
        }
        _ => {
            eprintln!("error: too many arguments");
            Err(ExitCode::from(2))
        }
    }
}

fn inspect_pack(args: &[String]) -> ExitCode {
    let dir = match one_arg(args) {
        Ok(dir) => dir,
        Err(code) => return code,
    };
    match validate_pack(&dir) {
        Ok(summary) => {
            println!(
                "pack: {} ({} {})",
                summary.name, summary.id, summary.version
            );
            println!("capability: {}", summary.capability);
            println!("license: {}", summary.license);
            println!(
                "models: {} file(s), {} bytes total",
                summary.model_count, summary.total_bytes
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

fn load_gray_input(path: &Path) -> Result<ProviderInput, String> {
    let image = image::open(path).map_err(|err| format!("failed to load image: {err}"))?;
    let gray = image.to_luma8();
    let (width, height) = (gray.width(), gray.height());
    Ok(ProviderInput::Image {
        width,
        height,
        channels: 1,
        data: gray.into_raw(),
        format: "gray".to_string(),
    })
}

fn load_rgb_input(path: &Path) -> Result<ProviderInput, String> {
    let image = image::open(path).map_err(|err| format!("failed to load image: {err}"))?;
    let rgb = image.to_rgb8();
    let (width, height) = (rgb.width(), rgb.height());
    Ok(ProviderInput::Image {
        width,
        height,
        channels: 3,
        data: rgb.into_raw(),
        format: "rgb".to_string(),
    })
}

fn qr(args: &[String]) -> ExitCode {
    let path = match one_arg(args) {
        Ok(path) => path,
        Err(code) => return code,
    };
    let input = match load_gray_input(&path) {
        Ok(input) => input,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
    };
    let provider = QrCodeProvider::new();
    let mut ctx = RunContext::new();
    match provider.run(&input, &mut ctx) {
        Ok(ProviderOutput::QrDecoded { text }) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(VisionError::Internal(message)) if message == NO_QR_CODE_MESSAGE => {
            println!("no QR found");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
        Ok(other) => {
            eprintln!("error: unexpected provider output: {other:?}");
            ExitCode::from(1)
        }
    }
}

fn stats(args: &[String]) -> ExitCode {
    let path = match one_arg(args) {
        Ok(path) => path,
        Err(code) => return code,
    };
    let input = match load_rgb_input(&path) {
        Ok(input) => input,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
    };
    let provider = ImageStatsProvider::new();
    let mut ctx = RunContext::new();
    match provider.run(&input, &mut ctx) {
        Ok(ProviderOutput::ImageStats {
            mean_luma,
            std_luma,
            contrast,
        }) => {
            println!("mean luma: {mean_luma:.2}");
            println!("std luma: {std_luma:.2}");
            println!("contrast: {contrast:.2}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
        Ok(other) => {
            eprintln!("error: unexpected provider output: {other:?}");
            ExitCode::from(1)
        }
    }
}

fn bench(args: &[String]) -> ExitCode {
    let path = match one_arg(args) {
        Ok(path) => path,
        Err(code) => return code,
    };
    let input = match load_gray_input(&path) {
        Ok(input) => input,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
    };
    const RUNS: usize = 20;
    let provider = QrCodeProvider::new();
    let mut times = Vec::with_capacity(RUNS);
    let mut decodes = 0usize;
    for _ in 0..RUNS {
        let mut ctx = RunContext::new();
        let start = Instant::now();
        if let Ok(ProviderOutput::QrDecoded { .. }) = provider.run(&input, &mut ctx) {
            decodes += 1;
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let min = times[0];
    let median = times[RUNS / 2];
    let max = times[RUNS - 1];
    println!("runs: {RUNS}");
    println!("decoded: {decodes}/{RUNS}");
    println!("min: {min:.3} ms");
    println!("median: {median:.3} ms");
    println!("max: {max:.3} ms");
    ExitCode::SUCCESS
}
