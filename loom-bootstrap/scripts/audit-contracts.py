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

# Root prose is deliberately kept to the project contract, public overview,
# and human-maintained truth statement. Generated status reports belong in CI
# artifacts, not in source control where they quickly become contradictory.
for name in (
    "ACCESSIBILITY_REPORT.md",
    "BUILD_ALL.md",
    "BUILD_STATUS.md",
    "DEPENDENCY_REPORT.md",
    "FEATURE_STATUS.md",
    "KNOWN_LIMITATIONS.md",
    "LICENSE_REPORT.md",
    "LOOM_MASTER_INDEX.md",
    "PERFORMANCE_REPORT.md",
    "REPOSITORY_MAP.md",
    "RUN_ALL.md",
    "SECURITY_REPORT.md",
    "TEST_ALL.md",
    "VERIFICATION_REPORT.md",
    "VISUAL_QA_ALL.md",
    "visual-qa-report.md",
):
    if (ROOT / name).exists():
        errors.append(f"root generated report must be a CI artifact, not committed prose: {name}")

# Keep the corrected native-evidence model durable. Palette screenshots are
# visual evidence only; actual journey executables must run before scoring.
cross_platform = (ROOT / ".github/workflows/cross-platform.yml").read_text(encoding="utf-8")
journey_step = cross_platform.find("Record real command-palette keyboard journeys")
score_step = cross_platform.find("Run suite contracts and strict readiness score")
package_step = cross_platform.find("Build native validation package")
if journey_step < 0 or ' --journey ' not in cross_platform:
    errors.append("native workflow must execute every application --journey recorder")
if journey_step >= 0 and score_step >= 0 and journey_step > score_step:
    errors.append("native workflow scores keyboard evidence before journeys run")
if score_step >= 0 and package_step >= 0 and score_step > package_step:
    errors.append("native workflow must produce readiness evidence even when packaging fails")

packaging = (ROOT / "loom-bootstrap/packaging/release.py").read_text(encoding="utf-8")
if 'Platform="{platform}"' in packaging:
    errors.append("WiX v4 Package must not use the removed Platform attribute")
if "run_with_retries" not in packaging or "hdiutil" not in packaging:
    errors.append("macOS DMG creation must retain bounded transient-failure retry")

readiness = (ROOT / "loom-bootstrap/scripts/audit-product-readiness.py").read_text(encoding="utf-8")
if "not AGENTS.md feature-completion parity" not in readiness:
    errors.append("readiness output must state that it is not product feature parity")
if "Ctrl/Cmd+K opening and per-command application side effects" not in readiness:
    errors.append("readiness audit must retain the keyboard side-effect evidence blocker")

if errors:
    print("Loom contract audit: FAIL", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"Loom contract audit: PASS ({len(APPS)} application contracts checked)")
