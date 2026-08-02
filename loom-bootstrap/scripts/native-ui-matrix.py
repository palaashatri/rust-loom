#!/usr/bin/env python3
"""Render and validate every Loom application on the current native OS."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
THEMES = ("light", "dark", "high-contrast")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "windows", "macos"), required=True)
    parser.add_argument("--size", default="1440x900")
    return parser.parse_args()


def binary_path(root: Path, app: str, platform: str) -> Path:
    suffix = ".exe" if platform == "windows" else ""
    return root / f"loom-{app}" / "target" / "release" / f"loom-{app}{suffix}"


def execute(command: list[str], environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        env=environment,
        text=True,
        capture_output=True,
        timeout=120,
        check=False,
    )


def main() -> int:
    arguments = parse_arguments()
    root = arguments.root.resolve()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment.setdefault("SLINT_BACKEND", "software")
    report: dict[str, object] = {
        "schema_version": 1,
        "platform": arguments.platform,
        "size": arguments.size,
        "applications": {},
    }
    failures: list[str] = []
    for app in APPS:
        binary = binary_path(root, app, arguments.platform)
        if not binary.is_file():
            failures.append(f"{app}: missing release binary {binary}")
            continue
        theme_records: dict[str, object] = {}
        hashes: set[str] = set()
        for theme in THEMES:
            target = output / f"loom-{app}-{theme}.png"
            process = execute(
                [str(binary), "--screenshot", str(target), "--size", arguments.size, "--theme", theme],
                environment,
            )
            if process.returncode != 0:
                failures.append(
                    f"{app}/{theme}: screenshot command failed ({process.returncode}): "
                    f"{process.stderr.strip() or process.stdout.strip()}"
                )
                continue
            if not target.is_file():
                failures.append(f"{app}/{theme}: screenshot file was not created")
                continue
            payload = target.read_bytes()
            if not payload.startswith(PNG_SIGNATURE):
                failures.append(f"{app}/{theme}: output is not a PNG")
                continue
            if len(payload) < 4096:
                failures.append(f"{app}/{theme}: screenshot is implausibly small ({len(payload)} bytes)")
                continue
            digest = hashlib.sha256(payload).hexdigest()
            hashes.add(digest)
            theme_records[theme] = {"path": target.name, "bytes": len(payload), "sha256": digest}
        if len(hashes) != len(THEMES):
            failures.append(f"{app}: light, dark, and high-contrast captures are not all distinct")
        smoke = execute([str(binary), "--smoke", "--theme", "dark"], environment)
        if smoke.returncode != 0:
            failures.append(
                f"{app}: native smoke failed ({smoke.returncode}): "
                f"{smoke.stderr.strip() or smoke.stdout.strip()}"
            )
        report["applications"][app] = {
            "binary": str(binary),
            "themes": theme_records,
            "smoke_passed": smoke.returncode == 0,
        }
    report["passed"] = not failures
    report["failures"] = failures
    report_path = output / "native-ui-matrix.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if failures:
        for failure in failures:
            print(f"native UI matrix: {failure}", file=sys.stderr)
        return 1
    print(f"native UI matrix passed: {len(APPS)} apps × {len(THEMES)} themes on {arguments.platform}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
