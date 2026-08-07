#!/usr/bin/env python3
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]


def replace_once(path: str, old: str, new: str) -> None:
    file = ROOT / path
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement target, found {count}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


# Sheets removed the old formatting command family; the native journey still
# queried it, guaranteeing an empty filtered palette.
replace_once(
    "loom-sheets/crates/loom-sheets-app/src/main.rs",
    'record_keyboard_palette_journey(&app, "sheets", Path::new(out_dir), "format")',
    'record_keyboard_palette_journey(&app, "sheets", Path::new(out_dir), "save")',
)

# A one-result query is valid. Down-arrow movement is only an invariant when
# the filtered list contains more than one row.
replace_once(
    "loom-core/crates/loom-test-support/src/journey.rs",
    '    if move_step.selected <= last_query.selected {\n        passed = false;\n        failures.push("down-arrow did not move the selection forward".to_string());\n    }',
    '    if last_query.commands > 1 && move_step.selected <= last_query.selected {\n        passed = false;\n        failures.push("down-arrow did not move the selection forward".to_string());\n    }',
)

# Handler counts can be diagnostic, but they cannot award readiness points.
replace_once(
    "loom-bootstrap/scripts/audit-product-readiness.py",
    "    source_ratio = min(1.0, key_handlers / 10.0)\n"
    "    # Current journeys programmatically open the palette because modifier-key\n"
    "    # injection is unavailable and do not assert the invoked command's domain\n"
    "    # side effect. Keep this dimension capped until both are evidenced.\n"
    "    keyboard_score = min(0.8, source_ratio * 0.5 + journey_ratio * 0.3)",
    "    # Source handler counts are diagnostic only and contribute no readiness points.\n"
    "    # Modifier-key opening and command domain side effects remain unproven.\n"
    "    keyboard_score = min(0.3, journey_ratio * 0.3)",
)
replace_once(
    "loom-bootstrap/scripts/audit-product-readiness.py",
    '        f"{key_handlers} shared keyboard handlers; post-open palette journeys {journeys_passed}/8",',
    '        f"post-open palette journeys {journeys_passed}/8; {key_handlers} source handlers diagnostic only (unscored)",',
)

# Make the native matrix language honest and add an explicit aggregate gate so
# package completion cannot be inferred from only a subset of targets.
cross = ROOT / ".github/workflows/cross-platform.yml"
cross_text = cross.read_text(encoding="utf-8")
old_step = "      - name: Record real command-palette keyboard journeys"
new_step = "      - name: Record palette input journeys (modifier-open unproven)"
if cross_text.count(old_step) != 1:
    raise SystemExit("cross-platform.yml: keyboard journey step target drifted")
cross_text = cross_text.replace(old_step, new_step, 1)
if "name: required-native-baseline" in cross_text:
    raise SystemExit("cross-platform.yml: native baseline aggregate already exists")
cross_text = cross_text.rstrip() + """

  native-baseline:
    name: required-native-baseline
    if: always()
    needs: native-build
    runs-on: ubuntu-24.04
    steps:
      - name: Require every declared native target to pass
        env:
          NATIVE_MATRIX_RESULT: ${{ needs.native-build.result }}
        shell: bash
        run: |
          set -euo pipefail
          if [ "$NATIVE_MATRIX_RESULT" != "success" ]; then
            echo "required native matrix did not fully pass: $NATIVE_MATRIX_RESULT" >&2
            exit 1
          fi
"""
cross.write_text(cross_text, encoding="utf-8")

# Re-audit Present, Photo, and Motion without score inflation.
truth = ROOT / "TRUTH.md"
truth_text = truth.read_text(encoding="utf-8")
old_epic = "Epic 1 is\nnot complete; six applications still use development-era path workflows."
new_epic = (
    "Epic 1 is\nnot complete; Video, Studio, and Encode still require the shared native "
    "desktop workflow migration."
)
if old_epic not in truth_text:
    raise SystemExit("TRUTH.md: Epic 1 workflow count target drifted")
truth_text = truth_text.replace(old_epic, new_epic, 1)

old_foundation = (
    "- Writer and Sheets use that shared contract for normal Open, Save/Save As, and\n"
    "  export destination workflows."
)
new_foundation = (
    "- Writer, Sheets, Present, Photo, and Motion use that shared contract for normal\n"
    "  native Open, Save/Save As, import/export destination workflows where applicable."
)
if old_foundation not in truth_text:
    raise SystemExit("TRUTH.md: desktop service foundation target drifted")
truth_text = truth_text.replace(old_foundation, new_foundation, 1)

bullets = {
    "Present": """- **Present:** deck/slide models, layouts, notes, transitions, scene generation,
  validation, persistence, history, PDF output, and native New/Open/Save/Save As/
  export-destination workflows are wired. The Phase 0 re-audit does **not** promote
  its score: semantic round-trip assertions remain narrow, writes are non-atomic,
  PDF output is not independently validated in the desktop journey, and recent
  documents/import, full direct manipulation, masters, mixed media, animation
  authoring, presenter workflows, recording, video export, and PPTX/ODP fidelity
  remain incomplete.""",
    "Photo": """- **Photo:** raster decode, pixel buffers, layers, blend modes, adjustment and
  mask foundations, compositing, crop/resize, persistence, history, native project
  Open/Save/Save As, raster import, and native PNG/JPEG destination workflows are
  wired. The Phase 0 re-audit does **not** promote its score: persistence/export
  writes are non-atomic, exported files are not independently decoded in the desktop
  journey, recent documents are absent, and tool selection still includes status-only
  modes rather than complete canvas interaction. Painting, production masks/selections,
  RAW/ICC, healing, warping, HDR/panorama, PSD fidelity, GPU effects, and production
  AI editing remain incomplete.""",
    "Motion": """- **Motion:** layer/keyframe models, interpolation, transform manipulation, ordering,
  validation, persistence, bounded history, frame sampling, SVG frame export, and
  native New/Open/Save/Save As/export destination workflows are wired. The repaired
  native slice has a genuinely blank New composition, exact model round-trip equality,
  repeated-open idempotence, Save→Save As path coverage, cancellation/error coverage,
  read-only and non-UTF-8 path coverage where supported, and responsive startup smoke
  checks. This still does **not** justify a score increase: writes remain non-atomic,
  recent documents and professional render validation are absent, and production
  compositing/playback, cameras/lights, particles, effects, tracking, stabilization,
  optical flow, and render-queue breadth remain incomplete.""",
}
for name, replacement in bullets.items():
    next_name = {"Present": "Photo", "Photo": "Motion", "Motion": "Video"}[name]
    pattern = rf"- \*\*{name}:\*\*.*?(?=\n- \*\*{next_name}:\*\*)"
    truth_text, count = re.subn(pattern, replacement, truth_text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"TRUTH.md: failed to replace {name} boundary")

audit_section = """### Present, Photo, and Motion re-audit

The Phase 0 re-audit confirms real native desktop file workflows in all three
applications, but none earns a readiness promotion from that fact alone. Present
still lacks complete semantic round-trip and independent PDF evidence. Photo still
has non-atomic persistence/export and status-only tool modes. Motion's repaired
native workflow passed its focused strict gate, while its professional playback,
compositing, and rendering engine remains incomplete. The complete-suite truth
score therefore remains approximately **24/100**.

"""
marker = "### Keyboard journeys\n"
if audit_section not in truth_text:
    if truth_text.count(marker) != 1:
        raise SystemExit("TRUTH.md: keyboard evidence marker drifted")
    truth_text = truth_text.replace(marker, audit_section + marker, 1)
truth.write_text(truth_text, encoding="utf-8")

# Reinstate durable truth checks.
contracts = ROOT / "loom-bootstrap/scripts/audit-contracts.py"
contracts_text = contracts.read_text(encoding="utf-8")
old_marker = 'journey_step = cross_platform.find("Record real command-palette keyboard journeys")'
new_marker = 'journey_step = cross_platform.find("Record palette input journeys (modifier-open unproven)")'
if contracts_text.count(old_marker) != 1:
    raise SystemExit("audit-contracts.py: keyboard marker drifted")
contracts_text = contracts_text.replace(old_marker, new_marker, 1)

insertion = r'''
# Phase 0 truth enforcement.
allowed_root_markdown = {"AGENTS.MD", "README.md", "TRUTH.md"}
for path in ROOT.iterdir():
    if path.is_file() and path.suffix.lower() == ".md" and path.name not in allowed_root_markdown:
        errors.append(f"unauthorized root Markdown file: {path.name}")

truth = (ROOT / "TRUTH.md").read_text(encoding="utf-8")
score_claims = re.findall(r"approximately\s+\*\*(\d{1,3})/100\*\*", truth)
if not score_claims:
    errors.append("TRUTH.md must contain an explicit approximate complete-suite score")
elif len(set(score_claims)) != 1:
    errors.append(f"TRUTH.md contains contradictory complete-suite scores: {sorted(set(score_claims))}")

for token in re.findall(r"`([^`\n]+)`", truth):
    if token.startswith("loom-") and "/" in token and not any(ch in token for ch in "*?{}"):
        if not (ROOT / token).exists():
            errors.append(f"TRUTH.md references missing evidence path: {token}")

if "source_ratio *" in readiness or "key_handlers / 10.0" in readiness:
    errors.append("readiness scoring must not award points from source keyboard-handler counts")

journey_source = (ROOT / "loom-core/crates/loom-test-support/src/journey.rs").read_text(encoding="utf-8")
if "open step uses the Ctrl+K host hook" not in journey_source:
    errors.append("palette journeys must disclose modifier-key opening as a host hook")
if "do not prove complete keyboard-only application operation" not in truth:
    errors.append("TRUTH.md must preserve the keyboard host-hook limitation")
if "exercise content-based format detection" not in truth or "They are not round-trip fidelity" not in truth:
    errors.append("detection-only fixtures must not be represented as conformance evidence")
if "name: required-native-baseline" not in cross_platform or "needs: native-build" not in cross_platform:
    errors.append("native package workflow must aggregate all required platform results")
'''
final_marker = "\nif errors:\n"
if contracts_text.count(final_marker) != 1:
    raise SystemExit("audit-contracts.py: final marker drifted")
contracts_text = contracts_text.replace(final_marker, "\n" + insertion + final_marker, 1)
contracts.write_text(contracts_text, encoding="utf-8")
