#!/usr/bin/env python3
"""Render and validate every Loom application on the current native OS.

The matrix intentionally tests more than "a window launched": every application
is rendered at compact, reference, and large desktop sizes in all supported
visual themes. When a native Loom sample package exists, it is opened through
the same positional-document path used by the operating-system association.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import struct
import subprocess
import sys
from pathlib import Path

APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
THEMES = ("light", "dark", "high-contrast")
DEFAULT_SIZES = ("1024x720", "1440x900", "1920x1200")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
EXTENSIONS = {
    "writer": ".loomdoc",
    "sheets": ".loomsheet",
    "present": ".loomdeck",
    "photo": ".loomphoto",
    "motion": ".loommotion",
    "video": ".loomvideo",
    "studio": ".loomstudio",
    "encode": ".loomencode",
}


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "windows", "macos"), required=True)
    parser.add_argument(
        "--sizes",
        default=",".join(DEFAULT_SIZES),
        help="comma-separated WxH native capture sizes",
    )
    return parser.parse_args()


def parse_size(value: str) -> tuple[int, int]:
    try:
        width_text, height_text = value.lower().split("x", 1)
        width, height = int(width_text), int(height_text)
    except (TypeError, ValueError) as error:
        raise ValueError(f"invalid size '{value}', expected WxH") from error
    if width < 800 or height < 600:
        raise ValueError(f"capture size is too small for desktop QA: {value}")
    return width, height


def binary_path(root: Path, app: str, platform: str) -> Path:
    suffix = ".exe" if platform == "windows" else ""
    return root / f"loom-{app}" / "target" / "release" / f"loom-{app}{suffix}"


def execute(command: list[str], environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        env=environment,
        text=True,
        capture_output=True,
        timeout=180,
        check=False,
    )


def png_dimensions(payload: bytes) -> tuple[int, int] | None:
    if len(payload) < 24 or not payload.startswith(PNG_SIGNATURE):
        return None
    if payload[12:16] != b"IHDR":
        return None
    return struct.unpack(">II", payload[16:24])


def validate_png(path: Path, expected: tuple[int, int]) -> tuple[str, int] | str:
    if not path.is_file():
        return "screenshot file was not created"
    payload = path.read_bytes()
    dimensions = png_dimensions(payload)
    if dimensions is None:
        return "output is not a valid PNG with an IHDR chunk"
    if dimensions != expected:
        return f"PNG dimensions {dimensions[0]}x{dimensions[1]} do not match requested {expected[0]}x{expected[1]}"
    if len(payload) < 4096:
        return f"screenshot is implausibly small ({len(payload)} bytes)"
    minimum_density = max(4096, expected[0] * expected[1] // 150)
    if len(payload) < minimum_density:
        return f"screenshot lacks plausible visual density ({len(payload)} bytes, expected at least {minimum_density})"
    return hashlib.sha256(payload).hexdigest(), len(payload)


def find_sample(root: Path, app: str) -> Path | None:
    extension = EXTENSIONS[app]
    candidates: list[Path] = []
    for search_root in (
        root / "loom-bootstrap" / ".work" / "generated-samples",
        root / "loom-samples",
        root / f"loom-{app}",
    ):
        if search_root.is_dir():
            candidates.extend(path for path in search_root.rglob(f"*{extension}") if path.is_file())
    return sorted(candidates, key=lambda path: ("sample" not in path.name.lower(), len(str(path)), str(path)))[0] if candidates else None


def capture(
    binary: Path,
    target: Path,
    size: str,
    theme: str,
    environment: dict[str, str],
    document: Path | None = None,
) -> tuple[dict[str, object] | None, str | None]:
    command = [str(binary)]
    if document is not None:
        command.append(str(document))
    command.extend(["--screenshot", str(target), "--size", size, "--theme", theme])
    process = execute(command, environment)
    if process.returncode != 0:
        detail = process.stderr.strip() or process.stdout.strip()
        return None, f"screenshot command failed ({process.returncode}): {detail}"
    expected = parse_size(size)
    validated = validate_png(target, expected)
    if isinstance(validated, str):
        return None, validated
    digest, byte_count = validated
    return {
        "path": target.name,
        "bytes": byte_count,
        "sha256": digest,
        "width": expected[0],
        "height": expected[1],
        "document": str(document) if document else None,
    }, None


def main() -> int:
    arguments = parse_arguments()
    root = arguments.root.resolve()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    sizes = tuple(item.strip() for item in arguments.sizes.split(",") if item.strip())
    if len(sizes) < 3:
        print("native UI matrix requires at least compact, reference, and large sizes", file=sys.stderr)
        return 2
    try:
        for size in sizes:
            parse_size(size)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2

    environment = os.environ.copy()
    environment.setdefault("SLINT_BACKEND", "software")
    report: dict[str, object] = {
        "schema_version": 2,
        "platform": arguments.platform,
        "sizes": list(sizes),
        "themes": list(THEMES),
        "applications": {},
    }
    failures: list[str] = []

    for app in APPS:
        binary = binary_path(root, app, arguments.platform)
        if not binary.is_file():
            failures.append(f"{app}: missing release binary {binary}")
            continue
        size_records: dict[str, object] = {}
        all_hashes: set[str] = set()
        for size in sizes:
            theme_records: dict[str, object] = {}
            size_hashes: set[str] = set()
            for theme in THEMES:
                target = output / f"loom-{app}-{size}-{theme}.png"
                record, error = capture(binary, target, size, theme, environment)
                if error:
                    failures.append(f"{app}/{size}/{theme}: {error}")
                    continue
                assert record is not None
                digest = str(record["sha256"])
                size_hashes.add(digest)
                all_hashes.add(digest)
                theme_records[theme] = record
            if len(size_hashes) != len(THEMES):
                failures.append(f"{app}/{size}: light, dark, and high-contrast captures are not all distinct")
            size_records[size] = theme_records
        if len(all_hashes) != len(sizes) * len(THEMES):
            failures.append(f"{app}: one or more theme/size captures are byte-identical")

        smoke = execute([str(binary), "--smoke", "--theme", "dark"], environment)
        if smoke.returncode != 0:
            failures.append(
                f"{app}: native smoke failed ({smoke.returncode}): "
                f"{smoke.stderr.strip() or smoke.stdout.strip()}"
            )

        sample = find_sample(root, app)
        sample_record: dict[str, object] | None = None
        if sample is not None:
            target = output / f"loom-{app}-sample-open-dark.png"
            sample_record, error = capture(binary, target, sizes[1], "dark", environment, sample)
            if error:
                failures.append(f"{app}/sample-open: {error}")
            else:
                default_dark = size_records.get(sizes[1], {}).get("dark")
                if isinstance(default_dark, dict) and sample_record and sample_record["sha256"] == default_dark.get("sha256"):
                    failures.append(f"{app}: opening {sample.name} did not change the rendered application state")

        target = output / f"loom-{app}-palette-open.png"
        palette_record: dict[str, object] | None = None
        command = [str(binary), "--screenshot", str(target), "--size", sizes[1], "--theme", "dark", "--palette"]
        process = execute(command, environment)
        if process.returncode != 0:
            failures.append(
                f"{app}/palette: capture failed ({process.returncode}): "
                f"{process.stderr.strip() or process.stdout.strip()}"
            )
        else:
            validated = validate_png(target, parse_size(sizes[1]))
            if isinstance(validated, str):
                failures.append(f"{app}/palette: {validated}")
            else:
                digest, byte_count = validated
                palette_record = {
                    "path": target.name,
                    "bytes": byte_count,
                    "sha256": digest,
                    "width": parse_size(sizes[1])[0],
                    "height": parse_size(sizes[1])[1],
                    "query": "ex",
                }
                default_dark = size_records.get(sizes[1], {}).get("dark")
                if isinstance(default_dark, dict) and digest == default_dark.get("sha256"):
                    failures.append(f"{app}: palette capture is byte-identical to the default window")

        report["applications"][app] = {
            "binary": str(binary),
            "captures": size_records,
            "smoke_passed": smoke.returncode == 0,
            "sample_open": sample_record,
            "sample_available": sample is not None,
            "palette_open": palette_record,
        }

    report["passed"] = not failures
    report["failures"] = failures
    report_path = output / "native-ui-matrix.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if failures:
        for failure in failures:
            print(f"native UI matrix: {failure}", file=sys.stderr)
        return 1
    print(
        f"native UI matrix passed: {len(APPS)} apps × {len(THEMES)} themes × "
        f"{len(sizes)} sizes on {arguments.platform}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
