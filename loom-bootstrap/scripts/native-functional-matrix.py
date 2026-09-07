#!/usr/bin/env python3
"""Exercise native Loom CLI workflows and generate UI-ready project samples.

The matrix validates package integrity, selected domain mutations, command
output invariants, and representative exported files. It is a regression suite
for executable reference workflows, not proof of full application parity.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import zipfile
from pathlib import Path

APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
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
    return parser.parse_args()


def cli_path(root: Path, app: str, platform: str) -> Path:
    suffix = ".exe" if platform == "windows" else ""
    return root / f"loom-{app}" / "target" / "release" / f"loom-{app}-cli{suffix}"


def digest(path: Path) -> dict[str, object]:
    payload = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
    }


def run(command: list[str], cwd: Path) -> dict[str, object]:
    process = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=120,
        check=False,
        env={**os.environ, "NO_COLOR": "1"},
    )
    record: dict[str, object] = {
        "command": command,
        "returncode": process.returncode,
        "stdout": process.stdout.strip(),
        "stderr": process.stderr.strip(),
    }
    if process.returncode != 0:
        detail = process.stderr.strip() or process.stdout.strip()
        raise RuntimeError(f"command failed ({process.returncode}): {' '.join(command)}: {detail}")
    return record


def validate_package(path: Path) -> dict[str, object]:
    if not path.is_file() or path.stat().st_size < 128:
        raise RuntimeError(f"missing or implausibly small package: {path}")
    try:
        with zipfile.ZipFile(path) as archive:
            names = sorted(archive.namelist())
            if "manifest.json" not in names:
                raise RuntimeError(f"{path.name}: missing manifest.json")
            manifest = json.loads(archive.read("manifest.json"))
            if not any(name.startswith("content/") for name in names):
                raise RuntimeError(f"{path.name}: missing content payload")
            bad = archive.testzip()
            if bad:
                raise RuntimeError(f"{path.name}: corrupt ZIP member {bad}")
    except (zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise RuntimeError(f"invalid Loom package {path}: {error}") from error
    return {**digest(path), "entries": names, "manifest": manifest}


def expect_signature(path: Path, signature: bytes, label: str) -> dict[str, object]:
    if not path.is_file():
        raise RuntimeError(f"{label} was not created: {path}")
    payload = path.read_bytes()
    if not payload.startswith(signature):
        raise RuntimeError(f"{label} has an invalid signature: {path}")
    return digest(path)


def expect_stdout(
    records: list[dict[str, object]],
    command_index: int,
    expected_fragments: tuple[str, ...],
    label: str,
) -> None:
    try:
        stdout = str(records[command_index]["stdout"])
    except (IndexError, KeyError) as error:
        raise RuntimeError(f"{label}: missing command record {command_index}") from error
    missing = [fragment for fragment in expected_fragments if fragment not in stdout]
    if missing:
        raise RuntimeError(
            f"{label}: command {command_index} output missed {missing!r}; stdout={stdout!r}"
        )


def main() -> int:
    arguments = parse_arguments()
    root = arguments.root.resolve()
    output = arguments.output.resolve()
    samples = output / "generated-samples"
    work = output / "journeys"
    samples.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)
    report: dict[str, object] = {
        "schema_version": 2,
        "scope": "native CLI reference workflows; not full in-app feature parity",
        "platform": arguments.platform,
        "applications": {},
        "passed": False,
        "failures": [],
    }
    failures: list[str] = []

    def journey(
        app: str,
        commands: list[list[str]],
        outputs: tuple[tuple[Path, bytes, str], ...] = (),
        stdout_expectations: tuple[tuple[int, tuple[str, ...], str], ...] = (),
    ) -> None:
        try:
            cli = cli_path(root, app, arguments.platform)
            if not cli.is_file():
                raise RuntimeError(f"missing CLI binary: {cli}")
            records = [run([str(cli), *command], work) for command in commands]
            for command_index, expected, label in stdout_expectations:
                expect_stdout(records, command_index, expected, label)
            package = samples / f"sample{EXTENSIONS[app]}"
            package_record = validate_package(package)
            output_records = [
                expect_signature(path, signature, label)
                for path, signature, label in outputs
            ]
            report["applications"][app] = {
                "passed": True,
                "commands": records,
                "package": package_record,
                "outputs": output_records,
            }
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            failures.append(f"{app}: {error}")
            report["applications"][app] = {"passed": False, "error": str(error)}

    writer = samples / "sample.loomdoc"
    markdown = work / "writer.md"
    journey(
        "writer",
        [
            [
                "create",
                str(writer),
                "Native Journey",
                "Loom creates, validates, exports, and reopens documents locally.",
            ],
            ["validate", str(writer)],
            ["info", str(writer)],
            ["search", str(writer), "locally"],
            ["paginate", str(writer)],
            ["export-md", str(writer), str(markdown)],
        ],
        ((markdown, b"# Native Journey", "Writer Markdown export"),),
    )

    sheets = samples / "sample.loomsheet"
    csv_input = work / "input.csv"
    csv_output = work / "normalized.csv"
    csv_input.write_text(
        "Item,Amount\nDesign,8000\nEngineering,15000\nTotal,=SUM(B2:B3)\n",
        encoding="utf-8",
    )
    journey(
        "sheets",
        [
            ["create", str(sheets), "Native Budget"],
            ["eval", str(csv_input)],
            ["recalc", str(csv_input), "B2", "9000"],
            ["sort", str(csv_input), "A2:B3", "1", "desc"],
            ["to-csv", str(csv_input), str(csv_output)],
        ],
        ((csv_output, b"Item,Amount", "Sheets CSV export"),),
    )

    present = samples / "sample.loomdeck"
    pdf = work / "presentation.pdf"
    journey(
        "present",
        [
            ["create", str(present), "Native Presentation"],
            ["validate", str(present)],
            ["inspect", str(present)],
            ["scene", str(present), "0"],
            ["pdf", str(present), str(pdf)],
        ],
        ((pdf, b"%PDF", "Present PDF export"),),
    )

    photo = samples / "sample.loomphoto"
    ppm = work / "photo.ppm"
    journey(
        "photo",
        [
            ["create", str(photo), "Native Photo", "640", "360"],
            ["inspect", str(photo)],
            ["render-demo", str(ppm), "320", "180"],
        ],
        ((ppm, b"P6\n", "Photo composite render"),),
    )

    motion = samples / "sample.loommotion"
    journey(
        "motion",
        [
            ["create", str(motion), "Native Motion"],
            ["validate", str(motion)],
            ["inspect", str(motion)],
            ["frame", str(motion), "12"],
        ],
    )

    video = samples / "sample.loomvideo"
    edl = work / "video.edl"
    journey(
        "video",
        [
            ["create", str(video), "Native Video"],
            ["edit-demo", str(video)],
            ["inspect", str(video)],
            ["plan", str(video)],
            ["edl", str(video), str(edl)],
        ],
        ((edl, b"TITLE: Native Video", "Video EDL export"),),
        (
            (
                1,
                ("Edited video project: 3 clips, 1 marker, 1 caption",),
                "Video persisted edit",
            ),
            (2, ("Timeline: 3 clips, 1 markers, 1 captions",), "Video inspection"),
            (3, ("c-interview-b", "rate 1.500"), "Video render plan"),
        ),
    )

    studio = samples / "sample.loomstudio"
    sine = work / "sine.wav"
    synth = work / "synth.wav"
    journey(
        "studio",
        [
            ["create", str(studio), "Native Song", "124"],
            ["validate", str(studio)],
            ["inspect", str(studio)],
            ["sine", str(sine), "440", "0.25"],
            ["synth", str(synth), "64", "0.25"],
        ],
        (
            (sine, b"RIFF", "Studio oscillator WAV"),
            (synth, b"RIFF", "Studio MIDI WAV"),
        ),
    )

    encode = samples / "sample.loomencode"
    journey(
        "encode",
        [
            ["create", str(encode), "Native Delivery Queue"],
            ["prepare-recovery-demo", str(encode)],
            ["inspect", str(encode)],
            ["recover", str(encode)],
            ["inspect", str(encode)],
        ],
        stdout_expectations=(
            (1, ("Prepared recovery demo: 3 jobs",), "Encode recovery setup"),
            (
                2,
                ("Pending Jobs: 2", "Encoding { progress: 0.42 }"),
                "Encode interrupted state",
            ),
            (3, ("requeued 1 interrupted job",), "Encode recovery"),
            (
                4,
                ("Pending Jobs: 3", "Aggregate Progress: 0.000"),
                "Encode recovered state",
            ),
        ),
    )

    report["passed"] = not failures
    report["failures"] = failures
    report_path = output / "native-functional-matrix.json"
    report_path.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    if failures:
        for failure in failures:
            print(f"native functional matrix: {failure}", file=sys.stderr)
        return 1
    print(f"native functional matrix passed: {len(APPS)} applications on {arguments.platform}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
