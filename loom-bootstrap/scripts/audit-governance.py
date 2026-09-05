#!/usr/bin/env python3
"""Validate Loom's authority model and serial workflow lock."""
from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / "loom-bootstrap/contracts/workflow.toml"
AGENTS = ROOT / "AGENTS.MD"
TRUTH = ROOT / "TRUTH.md"
APPS = ("sheets", "writer", "present", "photo", "motion", "video", "studio", "encode")
errors: list[str] = []


def fail(message: str) -> None:
    errors.append(message)


if not WORKFLOW.is_file():
    fail("missing loom-bootstrap/contracts/workflow.toml")
else:
    workflow = tomllib.loads(WORKFLOW.read_text(encoding="utf-8"))
    phase = workflow.get("phase")
    foundation_status = workflow.get("foundation_status")

    if phase == "ui-foundation":
        if foundation_status != "ACCEPTANCE_BLOCKED":
            fail("foundation status changed without the acceptance procedure")
        if workflow.get("application_development_locked") is not True:
            fail("application development must remain locked during UI foundation work")
        if workflow.get("consumer_imports_allowed") is not False:
            fail("applications may not import the unaccepted UI foundation")
    elif phase == "sheets":
        if foundation_status != "ACCEPTED":
            fail("sheets phase requires accepted foundation")
        if workflow.get("application_development_locked") is not False:
            fail("application development should be unlocked for sheets")
        if workflow.get("consumer_imports_allowed") is not True:
            fail("consumer imports should be allowed for sheets")
        if workflow.get("application_status", {}).get("sheets") != "IN_PROGRESS":
            fail("sheets status must be IN_PROGRESS in sheets phase")
        for locked_app in ("writer", "present", "photo", "motion", "video", "studio", "encode"):
            if workflow.get("application_status", {}).get(locked_app) != "LOCKED":
                fail(f"application {locked_app} must remain LOCKED during sheets phase")
    elif phase == "writer":
        if foundation_status != "ACCEPTED":
            fail("writer phase requires accepted foundation")
        if workflow.get("application_development_locked") is not False:
            fail("application development should be unlocked for writer")
        if workflow.get("consumer_imports_allowed") is not True:
            fail("consumer imports should be allowed for writer")
        if workflow.get("application_status", {}).get("sheets") != "ACCEPTED":
            fail("sheets status must be ACCEPTED in writer phase")
        if workflow.get("application_status", {}).get("writer") != "IN_PROGRESS":
            fail("writer status must be IN_PROGRESS in writer phase")
        for locked_app in ("present", "photo", "motion", "video", "studio", "encode"):
            if workflow.get("application_status", {}).get(locked_app) != "LOCKED":
                fail(f"application {locked_app} must remain LOCKED during writer phase")
    elif phase == "present":
        if foundation_status != "ACCEPTED":
            fail("present phase requires accepted foundation")
        if workflow.get("application_development_locked") is not False:
            fail("application development should be unlocked for present")
        if workflow.get("consumer_imports_allowed") is not True:
            fail("consumer imports should be allowed for present")
        if workflow.get("application_status", {}).get("sheets") != "ACCEPTED":
            fail("sheets status must be ACCEPTED in present phase")
        if workflow.get("application_status", {}).get("writer") != "ACCEPTED":
            fail("writer status must be ACCEPTED in present phase")
        if workflow.get("application_status", {}).get("present") != "IN_PROGRESS":
            fail("present status must be IN_PROGRESS in present phase")
        for locked_app in ("photo", "motion", "video", "studio", "encode"):
            if workflow.get("application_status", {}).get(locked_app) != "LOCKED":
                fail(f"application {locked_app} must remain LOCKED during present phase")
    else:
        fail(f"unrecognized active phase: {phase}")

    if workflow.get("application_order") != list(APPS):
        fail("serial application order drifted from AGENTS.MD")

allowed_root_markdown = {"AGENTS.MD", "README.md", "TRUTH.md"}
for path in ROOT.iterdir():
    if path.is_file() and path.suffix.lower() == ".md" and path.name not in allowed_root_markdown:
        fail(f"unauthorized root Markdown file: {path.name}")

for path in ROOT.rglob("*"):
    if not path.is_file() or path == AGENTS:
        continue
    if path.name.lower() == "agents.md":
        fail(f"nested agent authority is forbidden: {path.relative_to(ROOT)}")

for stale in (
    ROOT / ".superpowers",
    ROOT / "docs/plans",
    ROOT / "docs/superpowers",
):
    if stale.exists() and any(path.is_file() for path in stale.rglob("*")):
        fail(f"stale agent planning residue remains: {stale.relative_to(ROOT)}")

agents_text = AGENTS.read_text(encoding="utf-8") if AGENTS.is_file() else ""
truth_text = TRUTH.read_text(encoding="utf-8") if TRUTH.is_file() else ""
agents_lower = agents_text.lower()
truth_lower = truth_text.lower()
for phrase in (
    "highest-authority engineering instruction",
    "ACTIVE PHASE: UI FOUNDATION LOCK",
    "Serial application workflow",
    "Visual foundation gate",
    "commercially redistributable assets",
):
    if phrase.lower() not in agents_lower:
        fail(f"AGENTS.MD missing required constitutional clause: {phrase}")

if phase == "ui-foundation":
    truth_phrases = (
        "ACTIVE PHASE: UI FOUNDATION",
        "FOUNDATION STATUS: ACCEPTANCE_BLOCKED",
        "APPLICATION DEVELOPMENT: LOCKED",
        "Current complete-suite readiness remains approximately **29/100**",
    )
elif phase == "sheets":
    truth_phrases = (
        "ACTIVE PHASE: SHEETS",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: SHEETS",
        "Current complete-suite readiness remains approximately **29/100**",
    )
elif phase == "writer":
    truth_phrases = (
        "ACTIVE PHASE: WRITER",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: WRITER",
        "Current complete-suite readiness remains approximately **29/100**",
    )
elif phase == "present":
    truth_phrases = (
        "ACTIVE PHASE: PRESENT",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: PRESENT",
        "Current complete-suite readiness remains approximately **29/100**",
    )
else:
    truth_phrases = ()

for phrase in truth_phrases:
    if phrase.lower() not in truth_lower:
        fail(f"TRUTH.md missing required active-state statement: {phrase}")

for app in APPS:
    if app in ("sheets", "writer", "present") and phase in ("sheets", "writer", "present"):
        continue
    ui_root = ROOT / f"loom-{app}" / "crates" / f"loom-{app}-app" / "ui"
    if not ui_root.exists():
        continue
    for path in ui_root.rglob("*.slint"):
        text = path.read_text(encoding="utf-8")
        if "foundation.slint" in text or "/foundation/" in text:
            fail(f"locked application imports foundation: {path.relative_to(ROOT)}")

if errors:
    print("Loom governance audit: FAIL", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print("Loom governance audit: PASS")
