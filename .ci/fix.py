from pathlib import Path
import re


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old[:100]!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


# Register the signed lifecycle module without rewriting the large host file.
replace_once(
    "loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs",
    "#![forbid(unsafe_code)]\n\n",
    "#![forbid(unsafe_code)]\n\n/// Signed installation, update/rollback, UI extension, migration, and native bridge APIs.\npub mod lifecycle;\n\n",
)

lifecycle = Path("loom-plugin-sdk/crates/loom-plugin-host/src/lifecycle.rs")
text = lifecycle.read_text(encoding="utf-8")
text = text.replace("use std::path::{Component, Path, PathBuf};", "use std::path::{Path, PathBuf};")
text = text.replace(
    '''            if !matches!(\n                panel.surface,\n                PanelSurface::Inspector | PanelSurface::Sidebar | PanelSurface::Utility\n            ) {\n                return Err(LifecycleError::InvalidUi("unsupported panel surface".into()));\n            }\n''',
    "",
)
text = text.replace('plugin_package("example.test", "1.0.0")', 'plugin_package("example-test", "1.0.0")')
text = text.replace('plugin_package("example.test", "2.0.0")', 'plugin_package("example-test", "2.0.0")')
text = text.replace('"example.test"', '"example-test"')
text = text.replace('plugin_id: "example-test".into(),\n            commands: vec![CommandContribution {\n                id: "wrong.command".into(),', 'plugin_id: "example-test".into(),\n            commands: vec![CommandContribution {\n                id: "wrong.command".into(),')
old_manifest = '''        let manifest = serde_json::json!({\n            "manifest_version": 1,\n            "plugin_id": id,\n            "name": "Test Plugin",\n            "version": version,\n            "vendor": "Loom Tests",\n            "description": "test",\n            "api_min_version": "0.1.0",\n            "api_max_version": "0.9.0",\n            "capabilities": ["command"],\n            "permissions": [],\n            "entry": { "wasm_module": "module.wasm", "function": "run" },\n            "resource_limits": {\n                "max_memory_bytes": 1048576,\n                "max_fuel": 100000,\n                "timeout_ms": 1000\n            }\n        });'''
new_manifest = '''        let manifest = serde_json::json!({\n            "manifest_version": 1,\n            "plugin_id": id,\n            "name": "Test Plugin",\n            "description": "test",\n            "version": version,\n            "author": "Loom Tests",\n            "license": "MIT",\n            "entry": {\n                "kind": "command",\n                "wasm_module": "module.wasm",\n                "function": "run"\n            },\n            "capabilities": [],\n            "permissions": [],\n            "api_min_version": "0.1.0",\n            "api_max_version": "0.9.0",\n            "resource_limits": {\n                "max_memory_bytes": 1048576,\n                "max_fs_bytes": 1048576,\n                "max_fs_entries": 100,\n                "max_cpu_ms_per_call": 1000,\n                "network": false\n            }\n        });'''
if old_manifest not in text:
    raise SystemExit("plugin test manifest block not found")
text = text.replace(old_manifest, new_manifest)
lifecycle.write_text(text, encoding="utf-8")

media = Path("loom-core/crates/loom-media-runtime/src/lib.rs")
text = media.read_text(encoding="utf-8")
text = text.replace("use std::thread;\n", "")
text = re.sub(
    r'''\nimpl GpuBackend \{\n    fn hardware_name\(self\) -> Option<&'static str> \{.*?\n    \}\n\}\n''',
    "\n",
    text,
    count=1,
    flags=re.S,
)
# A symlinked /usr/bin/ffmpeg is normal; inspect the canonical target instead.
old_canonical = '''fn canonical_executable(path: &Path) -> Result<PathBuf, MediaRuntimeError> {\n    let metadata = fs::symlink_metadata(path).map_err(|error| {\n        MediaRuntimeError::Unavailable(format!("cannot inspect {}: {error}", path.display()))\n    })?;\n    if metadata.file_type().is_symlink() || !metadata.is_file() {\n        return Err(MediaRuntimeError::Unavailable(\n            "FFmpeg path must be a regular non-symlink file".into(),\n        ));\n    }\n    Ok(fs::canonicalize(path)?)\n}'''
new_canonical = '''fn canonical_executable(path: &Path) -> Result<PathBuf, MediaRuntimeError> {\n    let canonical = fs::canonicalize(path).map_err(|error| {\n        MediaRuntimeError::Unavailable(format!("cannot resolve {}: {error}", path.display()))\n    })?;\n    if !fs::metadata(&canonical)?.is_file() {\n        return Err(MediaRuntimeError::Unavailable(\n            "FFmpeg path must resolve to a regular file".into(),\n        ));\n    }\n    Ok(canonical)\n}'''
if old_canonical not in text:
    raise SystemExit("canonical executable block not found")
text = text.replace(old_canonical, new_canonical)
# Only advertise hardware backends that have a real accelerated overlay path.
text = text.replace(
    '''        if self.hardware_accelerators.contains("d3d11va") {\n            backends.push(GpuBackend::D3d11);\n        }\n        if self.hardware_accelerators.contains("videotoolbox") {\n            backends.push(GpuBackend::VideoToolbox);\n        }\n''',
    "",
)
# Upload the generated base frame before hardware overlay.
old_base = '''        filter.push_str(&format!(\n            "color=c=0x{red:02x}{green:02x}{blue:02x}@{:.6}:s={}x{}:r={:.6}[base0]",\n            f64::from(alpha) / 255.0,\n            self.output.width,\n            self.output.height,\n            self.output.frames_per_second\n        ));'''
new_base = '''        filter.push_str(&format!(\n            "color=c=0x{red:02x}{green:02x}{blue:02x}@{:.6}:s={}x{}:r={:.6}[base_cpu]",\n            f64::from(alpha) / 255.0,\n            self.output.width,\n            self.output.height,\n            self.output.frames_per_second\n        ));\n        match backend {\n            GpuBackend::Vulkan | GpuBackend::D3d11 => {\n                filter.push_str(";[base_cpu]format=rgba,hwupload[base0]");\n                gpu_stages.push("base_hwupload".into());\n            }\n            GpuBackend::Cuda => {\n                filter.push_str(";[base_cpu]format=rgba,hwupload_cuda[base0]");\n                gpu_stages.push("base_hwupload_cuda".into());\n            }\n            GpuBackend::Vaapi => {\n                filter.push_str(";[base_cpu]format=nv12,hwupload[base0]");\n                gpu_stages.push("base_hwupload_vaapi".into());\n            }\n            GpuBackend::VideoToolbox | GpuBackend::Cpu => {\n                filter.push_str(";[base_cpu]null[base0]");\n            }\n        }'''
if old_base not in text:
    raise SystemExit("base filter block not found")
text = text.replace(old_base, new_base)
# Download hardware frames when the selected encoder cannot consume them.
old_map = '''        arguments.push("-filter_complex".into());\n        arguments.push(filter);\n        arguments.push("-map".into());\n        arguments.push(format!("[{current_base}]"));'''
new_map = '''        let map_label = if backend != GpuBackend::Cpu\n            && !hardware_encoder_compatible(backend, &self.output.codec)\n        {\n            let final_label = "final_cpu";\n            filter.push_str(&format!(\n                ";[{current_base}]hwdownload,format=rgba[{final_label}]"\n            ));\n            cpu_stages.push("hwdownload".into());\n            final_label.to_string()\n        } else {\n            current_base\n        };\n        arguments.push("-filter_complex".into());\n        arguments.push(filter);\n        arguments.push("-map".into());\n        arguments.push(format!("[{map_label}]"));'''
if old_map not in text:
    raise SystemExit("map block not found")
text = text.replace(old_map, new_map)
insert_before = '''fn rgba_components(rgba: u32) -> (u8, u8, u8, u8) {'''
helper = '''fn hardware_encoder_compatible(backend: GpuBackend, codec: &str) -> bool {\n    let codec = codec.to_ascii_lowercase();\n    match backend {\n        GpuBackend::Cuda => codec.ends_with("_nvenc"),\n        GpuBackend::Vaapi => codec.ends_with("_vaapi"),\n        GpuBackend::Vulkan => codec.ends_with("_vulkan"),\n        GpuBackend::D3d11 => codec.contains("d3d11") || codec.contains("mf"),\n        GpuBackend::VideoToolbox => codec.ends_with("_videotoolbox"),\n        GpuBackend::Cpu => true,\n    }\n}\n\n'''
if insert_before not in text:
    raise SystemExit("rgba helper insertion point not found")
text = text.replace(insert_before, helper + insert_before)
# Use owned argument vectors so no temporary string references escape.
video_pattern = re.compile(r'''fn spawn_video_decoder\(.*?\n\}\n\nfn spawn_audio_decoder''', re.S)
video_replacement = '''fn spawn_video_decoder(\n    ffmpeg: &Path,\n    source: &Path,\n    configuration: &PreviewConfiguration,\n) -> Result<(Child, ChildStdout), MediaRuntimeError> {\n    let mut arguments = low_latency_input_arguments(configuration.start_seconds);\n    arguments.push("-i".into());\n    arguments.push(source.to_string_lossy().into_owned());\n    arguments.extend([\n        "-map".into(),\n        "0:v:0".into(),\n        "-an".into(),\n        "-sn".into(),\n        "-dn".into(),\n        "-vf".into(),\n        format!(\n            "scale={}:{}:flags=fast_bilinear,format=rgba",\n            configuration.width, configuration.height\n        ),\n        "-r".into(),\n        format_decimal(configuration.frames_per_second),\n        "-f".into(),\n        "rawvideo".into(),\n        "-pix_fmt".into(),\n        "rgba".into(),\n        "pipe:1".into(),\n    ]);\n    let mut child = Command::new(ffmpeg)\n        .args(&arguments)\n        .stdin(Stdio::null())\n        .stdout(Stdio::piped())\n        .stderr(Stdio::null())\n        .spawn()?;\n    let stdout = child\n        .stdout\n        .take()\n        .ok_or_else(|| MediaRuntimeError::Process("video decoder stdout unavailable".into()))?;\n    Ok((child, stdout))\n}\n\nfn spawn_audio_decoder'''
text, count = video_pattern.subn(video_replacement, text, count=1)
if count != 1:
    raise SystemExit("video decoder function not found")
audio_pattern = re.compile(r'''fn spawn_audio_decoder\(.*?\n\}\n\nfn low_latency_input_arguments''', re.S)
audio_replacement = '''fn spawn_audio_decoder(\n    ffmpeg: &Path,\n    source: &Path,\n    configuration: &PreviewConfiguration,\n) -> Result<(Child, ChildStdout), MediaRuntimeError> {\n    let mut arguments = low_latency_input_arguments(configuration.start_seconds);\n    arguments.push("-i".into());\n    arguments.push(source.to_string_lossy().into_owned());\n    arguments.extend([\n        "-map".into(),\n        "0:a:0?".into(),\n        "-vn".into(),\n        "-sn".into(),\n        "-dn".into(),\n        "-ac".into(),\n        configuration.channels.to_string(),\n        "-ar".into(),\n        configuration.sample_rate.to_string(),\n        "-f".into(),\n        "f32le".into(),\n        "-acodec".into(),\n        "pcm_f32le".into(),\n        "pipe:1".into(),\n    ]);\n    let mut child = Command::new(ffmpeg)\n        .args(&arguments)\n        .stdin(Stdio::null())\n        .stdout(Stdio::piped())\n        .stderr(Stdio::null())\n        .spawn()?;\n    let stdout = child\n        .stdout\n        .take()\n        .ok_or_else(|| MediaRuntimeError::Process("audio decoder stdout unavailable".into()))?;\n    Ok((child, stdout))\n}\n\nfn low_latency_input_arguments'''
text, count = audio_pattern.subn(audio_replacement, text, count=1)
if count != 1:
    raise SystemExit("audio decoder function not found")
# Remove unnecessary foreign-trait implementations; derived comparisons suffice.
start = text.find("impl PartialOrd<SignedDuration> for i128")
end = text.find("/// Running low-latency preview decoder pair.")
if start == -1 or end == -1 or start >= end:
    raise SystemExit("SignedDuration cleanup range not found")
text = text[:start] + text[end:]
text = text.replace(
    '''            return SyncAction::HoldVideo(Duration::from_nanos(\n                drift.unsigned_abs().as_nanos().min(u64::MAX as u128) as u64,\n            ));''',
    '''            return SyncAction::HoldVideo(drift.unsigned_abs());''',
)
text = text.replace(
    '''        if let Some(parent) = self.output.parent() {\n            fs::create_dir_all(parent)?;\n        }''',
    '''        if let Some(parent) = self.output.parent().filter(|path| !path.as_os_str().is_empty()) {\n            fs::create_dir_all(parent)?;\n        }''',
)
text = text.replace(
    '''        assert!(compiled\n            .arguments\n            .windows(2)\n            .any(|pair| pair == ["-filter_complex", pair[1].as_str()]));''',
    '''        assert!(compiled\n            .arguments\n            .iter()\n            .any(|argument| argument == "-filter_complex"));''',
)
media.write_text(text, encoding="utf-8")

# Fix slice/array comparison in interop detection for broad compiler compatibility.
interop = Path("loom-core/crates/loom-interop/src/lib.rs")
text = interop.read_text(encoding="utf-8")
text = text.replace('let format = if matches!(brand, b"qt  ") {', 'let format = if brand == b"qt  " {')
interop.write_text(text, encoding="utf-8")
