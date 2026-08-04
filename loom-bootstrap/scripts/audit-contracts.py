#!/usr/bin/env python3
"""Fail CI when Loom's declared UI contract and implementation drift apart."""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
errors: list[str] = []


def root_callback_declarations(source: str) -> set[str]:
    match = re.search(r"export component\s+\w+App\s+inherits\s+Window\s*\{", source)
    if not match:
        return set()
    body = source[match.end():]
    layout = body.find("VerticalLayout {")
    if layout >= 0:
        body = body[:layout]
    return set(re.findall(r"^\s{4}callback\s+([\w-]+)", body, re.MULTILINE))


for app in APPS:
    ui = ROOT / f"loom-{app}" / "crates" / f"loom-{app}-app" / "ui" / "app.slint"
    main = ROOT / f"loom-{app}" / "crates" / f"loom-{app}-app" / "src" / "main.rs"
    if not ui.is_file() or not main.is_file():
        errors.append(f"{app}: missing UI or Rust application entry point")
        continue

    ui_text = ui.read_text(encoding="utf-8")
    main_text = main.read_text(encoding="utf-8")
    declared = root_callback_declarations(ui_text)
    wired = {
        name.replace("_", "-")
        for name in re.findall(r"\bon_([a-zA-Z0-9_]+)\b", main_text)
    }
    missing = sorted(declared - wired)
    if missing:
        errors.append(f"{app}: declared but unwired callbacks: {', '.join(missing)}")

    if "out.to_str().unwrap()" in main_text:
        errors.append(f"{app}: smoke path can panic on non-UTF8 paths")

    core = ROOT / f"loom-{app}" / "crates" / f"loom-{app}-core" / "src" / "lib.rs"
    if core.is_file():
        production = core.read_text(encoding="utf-8").split("#[cfg(test)]", 1)[0]
        if re.search(r"MimeType::parse\([^\n]+\)\.unwrap\(\)", production):
            errors.append(f"{app}: production package writer unwraps a built-in MIME parse")

for forbidden in (
    "Started batch encoding engine",
    "/Users/Shared/LoomExports",
):
    for path in ROOT.glob("loom-*/crates/*-app/src/main.rs"):
        if forbidden in path.read_text(encoding="utf-8"):
            errors.append(f"{path.relative_to(ROOT)}: contains misleading hard-coded behavior: {forbidden}")

# Root status reports are required §25 deliverables and are committed, so
# their presence is expected. VERIFICATION_REPORT.md is generated from current
# command evidence (scripts/generate-status-report.sh): verify it is not stale
# against the latest test logs instead of forbidding the file.
report = ROOT / "VERIFICATION_REPORT.md"
work = ROOT / "loom-bootstrap" / ".work"
if report.is_file():
    report_text = report.read_text(encoding="utf-8")
    for log in sorted(work.glob("test-*.log")):
        repo = log.name[len("test-"):-len(".log")]
        count = 0
        for line in log.read_text(encoding="utf-8").splitlines():
            match = re.match(r"^test result: ok\.\s+(\d+)", line)
            if match:
                count += int(match.group(1))
        if count and repo not in report_text:
            errors.append(
                f"stale root generated report: {report.name} has no evidence for {repo} "
                f"(latest log: {log.name}, {count} tests)"
            )

if errors:
    print("Loom contract audit: FAIL", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"Loom contract audit: PASS ({len(APPS)} application contracts checked)")
