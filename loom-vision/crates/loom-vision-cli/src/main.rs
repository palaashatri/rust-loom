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
use loom_vision_core::reference::{
    AudioAnalysisProvider, DocumentLayoutProvider, ImageEmbeddingProvider, ImageStatsProvider,
    QrCodeProvider, ThresholdSegmentationProvider, NO_QR_CODE_MESSAGE,
};
use loom_vision_core::VisionError;

const USAGE: &str = "\
loom-vision <command> [args]

Commands:
  inspect-pack <dir>    Validate a model pack directory and print its summary
  qr <image>            Decode a QR code from an image file
  stats <image>         Compute mean luma, std luma, and contrast of an image
  segment <image> <pgm> Write a deterministic foreground mask as PGM
  layout <image>        Print connected document regions
  embed <image>         Print the 64-value local image embedding
  audio <pcm16le> <rate> <channels>  Analyse raw signed PCM16 audio
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
        "segment" => segment(&args[1..]),
        "layout" => layout(&args[1..]),
        "embed" => embed(&args[1..]),
        "audio" => audio(&args[1..]),
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

fn segment(args: &[String]) -> ExitCode {
    let [input_path, output_path] = args else {
        eprintln!("error: segment requires <image> <mask.pgm>");
        return ExitCode::from(2);
    };
    let input = match load_rgb_input(Path::new(input_path)) {
        Ok(input) => input,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
    };
    let mut context = RunContext::new();
    match ThresholdSegmentationProvider::new().run(&input, &mut context) {
        Ok(ProviderOutput::SegmentationMask {
            width,
            height,
            mask,
        }) => {
            let mut pgm = format!("P5\n{width} {height}\n255\n").into_bytes();
            pgm.extend_from_slice(&mask);
            if let Err(error) = std::fs::write(output_path, pgm) {
                eprintln!("error: failed to write {output_path}: {error}");
                return ExitCode::from(1);
            }
            println!("wrote {output_path} ({width}x{height})");
            ExitCode::SUCCESS
        }
        Ok(other) => {
            eprintln!("error: unexpected provider output: {other:?}");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

fn layout(args: &[String]) -> ExitCode {
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
    let mut context = RunContext::new();
    match DocumentLayoutProvider::new().run(&input, &mut context) {
        Ok(ProviderOutput::DetectionResult { boxes }) => {
            println!("regions: {}", boxes.len());
            for (index, region) in boxes.iter().enumerate() {
                println!(
                    "{}\t{}\t{:.0},{:.0} {:.0}x{:.0}\t{:.3}",
                    index + 1,
                    region.label,
                    region.x,
                    region.y,
                    region.w,
                    region.h,
                    region.confidence
                );
            }
            ExitCode::SUCCESS
        }
        Ok(other) => {
            eprintln!("error: unexpected provider output: {other:?}");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

fn embed(args: &[String]) -> ExitCode {
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
    let mut context = RunContext::new();
    match ImageEmbeddingProvider::new().run(&input, &mut context) {
        Ok(ProviderOutput::Embedding { values }) => {
            println!("dimensions: {}", values.len());
            println!(
                "{}",
                values
                    .iter()
                    .map(|value| format!("{value:.6}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            ExitCode::SUCCESS
        }
        Ok(other) => {
            eprintln!("error: unexpected provider output: {other:?}");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

fn audio(args: &[String]) -> ExitCode {
    let [path, sample_rate, channels] = args else {
        eprintln!("error: audio requires <pcm16le> <sample-rate> <channels>");
        return ExitCode::from(2);
    };
    let sample_rate = match sample_rate.parse::<u32>() {
        Ok(value) if value > 0 => value,
        _ => {
            eprintln!("error: sample rate must be a positive integer");
            return ExitCode::from(2);
        }
    };
    let channels = match channels.parse::<u16>() {
        Ok(value) if value > 0 => value,
        _ => {
            eprintln!("error: channels must be a positive integer");
            return ExitCode::from(2);
        }
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("error: failed to read {path}: {error}");
            return ExitCode::from(1);
        }
    };
    if bytes.len() % 2 != 0 {
        eprintln!("error: PCM16 input has an odd byte count");
        return ExitCode::from(1);
    }
    let samples = bytes
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
        .collect::<Vec<_>>();
    let input = ProviderInput::Audio {
        sample_rate,
        channels,
        samples,
    };
    let mut context = RunContext::new();
    match AudioAnalysisProvider::new().run(&input, &mut context) {
        Ok(ProviderOutput::AudioAnalysis {
            rms,
            peak,
            zero_crossing_rate,
        }) => {
            println!("rms: {rms:.6}");
            println!("peak: {peak:.6}");
            println!("zero crossing rate: {zero_crossing_rate:.6}");
            ExitCode::SUCCESS
        }
        Ok(other) => {
            eprintln!("error: unexpected provider output: {other:?}");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("error: {error}");
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
