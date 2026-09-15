#!/usr/bin/env python3
"""Validate Loom's authority model and serial workflow lock."""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / "loom-bootstrap/contracts/workflow.toml"
AGENTS = ROOT / "AGENTS.MD"
TRUTH = ROOT / "TRUTH.md"
APPS = ("sheets", "writer", "present", "photo", "motion", "video", "studio", "encode")
errors: list[str] = []
workflow: dict = {}
phase = None


def fail(message: str) -> None:
    errors.append(message)


if not WORKFLOW.is_file():
    fail("missing loom-bootstrap/contracts/workflow.toml")
else:
    workflow = tomllib.loads(WORKFLOW.read_text(encoding="utf-8"))
    phase = workflow.get("phase")
    foundation_status = workflow.get("foundation_status")

    if phase == "audit-repair":
        if workflow.get("active_repair") != "shared-recovery":
            fail("audit-repair currently permits only shared-recovery; review a scope change explicitly")
        if workflow.get("application_development_locked") is not True:
            fail("application development must remain locked during audit-repair")
        if workflow.get("consumer_imports_allowed") is not False:
            fail("new consumer imports must remain locked during audit-repair")
        if workflow.get("next_application") != "sheets":
            fail("audit-repair must return to Sheets first")
        if foundation_status not in ("ACCEPTED", "ACCEPTANCE_BLOCKED"):
            fail("invalid foundation status during audit-repair")
        if workflow.get("foundation_gate", {}).get("status") != foundation_status:
            fail("foundation gate status disagrees with foundation_status")
        if workflow.get("existing_foundation_consumers") != list(APPS[:5]):
            fail("existing foundation consumer list changed without an adoption gate")
        repair_prefixes = {
            ".github/", "AGENTS.MD", "TRUTH.md", "README.md", "loom-bootstrap/",
            "loom-core/crates/loom-ui/", "loom-core/crates/loom-desktop/",
            "loom-core/crates/loom-production/", "loom-core/crates/loom-storage/",
            "loom-design-bible/contracts/", "loom-design-bible/tokens/",
        }
        prefixes = workflow.get("allowed_active_prefixes")
        if not isinstance(prefixes, list) or any(
            not isinstance(prefix, str) for prefix in prefixes
        ) or not set(prefixes).issubset(repair_prefixes):
            fail("shared-recovery allowed_active_prefixes changed beyond approved scope; review explicitly")
        for app in APPS:
            if workflow.get("application_status", {}).get(app) != "LOCKED":
                fail(f"application {app} must remain LOCKED during shared-recovery")
    elif phase == "ui-foundation":
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
    elif phase == "photo":
        if foundation_status != "ACCEPTED":
            fail("photo phase requires accepted foundation")
        if workflow.get("application_development_locked") is not False:
            fail("application development should be unlocked for photo")
        if workflow.get("consumer_imports_allowed") is not True:
            fail("consumer imports should be allowed for photo")
        if workflow.get("application_status", {}).get("sheets") != "ACCEPTED":
            fail("sheets status must be ACCEPTED in photo phase")
        if workflow.get("application_status", {}).get("writer") != "ACCEPTED":
            fail("writer status must be ACCEPTED in photo phase")
        if workflow.get("application_status", {}).get("present") != "ACCEPTED":
            fail("present status must be ACCEPTED in photo phase")
        if workflow.get("application_status", {}).get("photo") not in ("IN_PROGRESS", "ACCEPTED"):
            fail("photo status must be IN_PROGRESS or ACCEPTED in photo phase")
        for locked_app in ("motion", "video", "studio", "encode"):
            if workflow.get("application_status", {}).get(locked_app) != "LOCKED":
                fail(f"application {locked_app} must remain LOCKED during photo phase")
    elif phase == "motion":
        if foundation_status != "ACCEPTED":
            fail("motion phase requires accepted foundation")
        if workflow.get("application_development_locked") is not False:
            fail("application development should be unlocked for motion")
        if workflow.get("consumer_imports_allowed") is not True:
            fail("consumer imports should be allowed for motion")
        if workflow.get("application_status", {}).get("sheets") != "ACCEPTED":
            fail("sheets status must be ACCEPTED in motion phase")
        if workflow.get("application_status", {}).get("writer") != "ACCEPTED":
            fail("writer status must be ACCEPTED in motion phase")
        if workflow.get("application_status", {}).get("present") != "ACCEPTED":
            fail("present status must be ACCEPTED in motion phase")
        if workflow.get("application_status", {}).get("photo") != "ACCEPTED":
            fail("photo status must be ACCEPTED in motion phase")
        if workflow.get("application_status", {}).get("motion") not in ("IN_PROGRESS", "ACCEPTED"):
            fail("motion status must be IN_PROGRESS or ACCEPTED in motion phase")
        for locked_app in ("video", "studio", "encode"):
            if workflow.get("application_status", {}).get(locked_app) != "LOCKED":
                fail(f"application {locked_app} must remain LOCKED during motion phase")
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
    "Active workflow and audit repair gate",
    "Serial application workflow",
    "Visual foundation gate",
    "commercially redistributable assets",
):
    if phrase.lower() not in agents_lower:
        fail(f"AGENTS.MD missing required constitutional clause: {phrase}")

if phase == "audit-repair":
    truth_phrases = (
        "APPLICATION DEVELOPMENT: LOCKED",
        "SUITE STATUS: ACCEPTANCE_BLOCKED",
        "ACTIVE REPAIR: SHARED-RECOVERY",
        "NEXT APPLICATION: SHEETS",
    )
elif phase == "ui-foundation":
    truth_phrases = (
        "ACTIVE PHASE: UI FOUNDATION",
        "FOUNDATION STATUS: ACCEPTANCE_BLOCKED",
        "APPLICATION DEVELOPMENT: LOCKED",
    )
elif phase == "sheets":
    truth_phrases = (
        "ACTIVE PHASE: SHEETS",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: SHEETS",
    )
elif phase == "writer":
    truth_phrases = (
        "ACTIVE PHASE: WRITER",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: WRITER",
    )
elif phase == "present":
    truth_phrases = (
        "ACTIVE PHASE: PRESENT",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: PRESENT",
    )
elif phase == "photo":
    truth_phrases = (
        "ACTIVE PHASE: PHOTO",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: PHOTO",
    )
elif phase == "motion":
    truth_phrases = (
        "ACTIVE PHASE: MOTION",
        "FOUNDATION STATUS: ACCEPTED",
        "ACTIVE APPLICATION: MOTION",
    )
else:
    truth_phrases = ()

for phrase in truth_phrases:
    if phrase.lower() not in truth_lower:
        fail(f"TRUTH.md missing required active-state statement: {phrase}")

# Check the live gate, not incidental words in historical prose. Readiness is
# evidence and explicit status, not an arbitrary number required by a regex.
if phase:
    phases = re.findall(r"^ACTIVE PHASE: (.+)$", truth_text, re.MULTILINE)
    expected_phase = "UI FOUNDATION" if phase == "ui-foundation" else phase.upper()
    if [value.strip() for value in phases] != [expected_phase]:
        fail("TRUTH.md active phase disagrees with workflow")
    foundations = re.findall(r"^FOUNDATION STATUS: (.+)$", truth_text, re.MULTILINE)
    if [value.strip() for value in foundations] != [workflow.get("foundation_status")]:
        fail("TRUTH.md foundation status disagrees with workflow")

if phase == "audit-repair":
    gate_fields = {
        "APPLICATION DEVELOPMENT": "LOCKED",
        "SUITE STATUS": "ACCEPTANCE_BLOCKED",
        "ACTIVE REPAIR": str(workflow.get("active_repair", "")).upper(),
        "NEXT APPLICATION": str(workflow.get("next_application", "")).upper(),
    }
    for field, expected in gate_fields.items():
        values = re.findall(rf"^{re.escape(field)}: (.+)$", truth_text, re.MULTILINE)
        if [value.strip() for value in values] != [expected]:
            fail(f"TRUTH.md {field} must appear once and match workflow")
    tables = re.findall(
        r"^\|[ \t]*Order[ \t]*\|[ \t]*Application[ \t]*\|"
        r"[ \t]*Product status[ \t]*\|[ \t]*Work status[ \t]*\|[^\n]*\n"
        r"((?:\|[^\n]*(?:\n|$))+)",
        truth_text, re.MULTILINE,
    )
    rows = re.findall(
        r"^\|\s*\d+\s*\|\s*(\w+)\s*\|\s*(\w+)\s*\|\s*(\w+)\s*\|",
        tables[0] if len(tables) == 1 else "", re.MULTILINE,
    )
    if len(tables) != 1 or [name.lower() for name, _, _ in rows] != list(APPS):
        fail("TRUTH.md must contain exactly one ordered live application status table")
    for name, quality, schedule in rows:
        if quality != "ACCEPTANCE_BLOCKED":
            fail(f"{name} product status must remain ACCEPTANCE_BLOCKED during shared-recovery")
        if schedule != workflow.get("application_status", {}).get(name.lower()):
            fail(f"{name} work status disagrees with workflow")
        section = re.search(
            rf"^### {re.escape(name)}\s*\n(.*?)(?=^## |^### |\Z)",
            truth_text, re.MULTILINE | re.DOTALL,
        )
        if section:
            for status in re.findall(
                r"^\s*(?:\*\*)?Status(?:\*\*)?:\s*(?:\*\*)?\s*`?(\w+)",
                section.group(1), re.MULTILINE,
            ):
                if status != quality:
                    fail(f"{name} section status disagrees with the live table")

for app in APPS:
    if phase == "audit-repair" and app in workflow.get("existing_foundation_consumers", []):
        continue
    if app in ("sheets", "writer", "present", "photo", "motion") and phase in ("sheets", "writer", "present", "photo", "motion"):
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
