#!/usr/bin/env python3
"""Strict evidence-based Loom UI and functionality scorecard.

Ten out of ten is deliberately difficult. Source tokens establish that an
architecture exists; native screenshots, packages, sample-open journeys,
conformance corpora, and interaction automation establish that it works.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8") if path.is_file() else ""


def contains_all(text: str, tokens: tuple[str, ...]) -> bool:
    return all(token in text for token in tokens)


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError, TypeError):
        return None


def native_report(evidence_root: Path | None) -> dict[str, Any] | None:
    if evidence_root is None or not evidence_root.exists():
        return None
    reports = sorted(evidence_root.rglob("native-ui-matrix.json"))
    return load_json(reports[0]) if reports else None


def native_packages(evidence_root: Path | None) -> list[Path]:
    if evidence_root is None or not evidence_root.exists():
        return []
    suffixes = (".msi", ".dmg", ".pkg", ".deb", ".appimage", ".zip", ".tar.gz")
    return [path for path in evidence_root.rglob("*") if path.is_file() and path.name.lower().endswith(suffixes)]


def score(evidence_root: Path | None) -> dict[str, object]:
    uis = {app: read(ROOT / f"loom-{app}/crates/loom-{app}-app/ui/app.slint") for app in APPS}
    mains = {app: read(ROOT / f"loom-{app}/crates/loom-{app}-app/src/main.rs") for app in APPS}
    cores = {app: read(ROOT / f"loom-{app}/crates/loom-{app}-core/src/lib.rs") for app in APPS}
    shared = read(ROOT / "loom-core/crates/loom-ui/ui/components.slint")
    theme = read(ROOT / "loom-core/crates/loom-ui/ui/theme.slint")
    native_workflow = read(ROOT / ".github/workflows/cross-platform.yml")
    packaging = read(ROOT / "loom-bootstrap/packaging/release.py")
    report = native_report(evidence_root)
    packages = native_packages(evidence_root)

    native_passed = bool(report and report.get("passed") is True)
    native_sizes = tuple(report.get("sizes", ())) if report else ()
    native_apps = report.get("applications", {}) if report else {}
    sample_open_count = sum(
        1
        for app in APPS
        if isinstance(native_apps, dict)
        and isinstance(native_apps.get(app), dict)
        and native_apps[app].get("sample_open")
    )

    ui_dimensions: list[dict[str, object]] = []

    def ui_point(name: str, value: float, evidence: str, blocker: str | None = None) -> None:
        item: dict[str, object] = {"name": name, "score": round(max(0.0, min(1.0, value)), 2), "evidence": evidence}
        if blocker:
            item["blocker"] = blocker
        ui_dimensions.append(item)

    shared_count = sum(1 for text in uis.values() if contains_all(text, ("AppHeader {", "StatusBar {", "Theme.palette()")))
    ui_point("shared design system", shared_count / len(APPS), f"{shared_count}/8 apps use shared creator chrome")

    semantic_count = sum(1 for text in uis.values() if not re.search(r"#[0-9a-fA-F]{6,8}", text))
    ui_point("semantic visual language", semantic_count / len(APPS), f"{semantic_count}/8 app UIs avoid local color literals")

    accessibility_tokens = ("accessible-role", "accessible-label", "accessible-action-default")
    accessibility_base = sum(token in shared for token in accessibility_tokens) / len(accessibility_tokens)
    accessibility_native = 0.2 if native_passed and "high-contrast" in tuple(report.get("themes", ())) else 0.0
    ui_point(
        "accessibility semantics",
        min(1.0, accessibility_base * 0.8 + accessibility_native),
        "shared semantic controls and native high-contrast evidence",
        None if accessibility_native else "screen-reader and native high-contrast journey evidence is incomplete",
    )

    key_handlers = shared.count("key-pressed(event)")
    ui_point(
        "keyboard and command interaction",
        min(0.75, key_handlers / 10.0),
        f"{key_handlers} shared keyboard handlers; no full keyboard journey recorder",
        "complete keyboard-only journeys and command-palette coverage are required",
    )

    theme_source = contains_all(theme, ("light", "dark", "high-contrast", "reduced-motion"))
    theme_native = native_passed and set(report.get("themes", ())) >= {"light", "dark", "high-contrast"}
    ui_point(
        "theme and motion coverage",
        (0.5 if theme_source else 0.0) + (0.5 if theme_native else 0.0),
        "source tokens plus native theme captures",
        None if theme_native else "native theme matrix has not passed",
    )

    ui_point(
        "native visual QA",
        1.0 if native_passed else 0.2 if contains_all(native_workflow, ("native-ui-matrix.py", "windows-2025", "macos-15")) else 0.0,
        "native screenshot matrix result" if native_passed else "workflow exists but completed native evidence is absent",
        None if native_passed else "all native screenshot jobs must pass",
    )

    adaptive_source = all(contains_all(text, ("min-width:", "min-height:", "horizontal-stretch", "vertical-stretch")) for text in uis.values())
    adaptive_native = native_passed and len(native_sizes) >= 3
    ui_point(
        "adaptive layout",
        (0.35 if adaptive_source else 0.0) + (0.65 if adaptive_native else 0.0),
        f"bounded stretch layouts and {len(native_sizes)} native capture sizes",
        None if adaptive_native else "compact, reference, and large native captures must pass",
    )

    positional_open = all("!other.starts_with('-') && args.open.is_none()" in text for text in mains.values())
    associations = contains_all(packaging, ("DOCUMENT_TYPES", "CFBundleDocumentTypes", "RegistryValue", "MimeType="))
    native_shell = 0.25 if positional_open else 0.0
    native_shell += 0.25 if associations else 0.0
    native_shell += 0.25 if packages else 0.0
    ui_point(
        "native shell integration",
        native_shell,
        f"positional open={positional_open}, associations={associations}, native packages={len(packages)}",
        "native menus, file pickers, drag/drop, recent documents, and OS services remain incomplete",
    )

    app_specific = {
        "writer": ("paper", "doc-content"),
        "sheets": ("formula", "selected-row"),
        "present": ("slide", "InspectorSurface"),
        "photo": ("layer", "preview-image"),
        "motion": ("timeline", "position-x"),
        "video": ("timeline", "preview-image"),
        "studio": ("track", "mixer"),
        "encode": ("queue", "preset"),
    }
    specific_count = sum(1 for app, tokens in app_specific.items() if contains_all(uis[app].lower(), tokens))
    ui_point("workflow-specific composition", specific_count / len(APPS), f"{specific_count}/8 apps expose domain-specific workspace composition")

    forbidden = ("coming soon", "placeholder ui", "placeholder control", "fake progress", "model preview")
    honest = all(not any(token in text.lower() for token in forbidden) for text in uis.values())
    ui_point("truthful states", 1.0 if honest else 0.0, "no fabricated product states in the application UI")

    functionality_dimensions: list[dict[str, object]] = []

    def fn_point(name: str, value: float, evidence: str, blocker: str | None = None) -> None:
        item: dict[str, object] = {"name": name, "score": round(max(0.0, min(1.0, value)), 2), "evidence": evidence}
        if blocker:
            item["blocker"] = blocker
        functionality_dimensions.append(item)

    recovered = sum(1 for text in mains.values() if contains_all(text, ("save", "load", "define_snapshot_recovery")))
    fn_point("persistence and recovery", recovered / len(APPS), f"{recovered}/8 apps use native package persistence and snapshot recovery")

    undo_apps = sum(1 for text in mains.values() if "on_undo" in text and "on_redo" in text)
    fn_point(
        "undo and redo",
        undo_apps / len(APPS) * 0.85,
        f"{undo_apps}/8 app front ends expose history",
        "operation-level undo/recovery coverage and crash replay tests are not yet complete",
    )

    export_apps = sum(1 for text in mains.values() if re.search(r"on_export|export_|encode_|execute_", text))
    import_apps = sum(1 for text in mains.values() if re.search(r"on_import|load_|decode_|from_csv|probe_media", text))
    fn_point(
        "import and export journeys",
        (export_apps + import_apps) / (2 * len(APPS)) * 0.85,
        f"exports={export_apps}/8, imports={import_apps}/8",
        "round-trip fidelity and destructive-loss reporting need broader conformance corpora",
    )

    media_tokens = {
        "photo": ("decode_raster", "encode_png", "save_photo_canvas"),
        "video": ("decode_preview_frame", "execute_timeline_export", "probe_media"),
        "studio": ("decode_wav", "AudioIo", "synthesize_notes"),
        "encode": ("discover_ffmpeg", "execute_job_with_cancel", "probe_duration"),
    }
    media_count = sum(1 for app, tokens in media_tokens.items() if contains_all(mains[app], tokens))
    fn_point(
        "media engines",
        media_count / len(media_tokens) * 0.8,
        f"{media_count}/4 media apps call real local engines",
        "GPU effects, synchronized low-latency playback, professional codecs, and plugin hosting are incomplete",
    )

    document_tokens = {
        "writer": ("export_pdf", "replace_paragraphs", "EditorHistory"),
        "sheets": ("evaluate", "from_csv", "CellEditTransaction"),
        "present": ("PresentationSession", "export_pdf", "save_presentation_session"),
    }
    document_count = sum(1 for app, tokens in document_tokens.items() if contains_all(mains[app], tokens))
    fn_point(
        "document engines",
        document_count / len(document_tokens) * 0.8,
        f"{document_count}/3 productivity apps call document engines",
        "professional typography, layout, recalculation breadth, and format fidelity are incomplete",
    )

    safety_count = sum(1 for text in list(mains.values()) + list(cores.values()) if any(token in text for token in ("cancel", "validate", "Result<", "map_err")))
    fn_point("failure, validation, and cancellation", min(0.9, safety_count / 14.0), "bounded errors, validation, and cancellation across engines")

    vision = read(ROOT / "loom-vision/crates/loom-vision-core/src/production.rs")
    vision_score = sum(token in vision.lower() for token in ("preprocess", "benchmark", "accelerat", "license", "model pack", "checksum")) / 6.0
    fn_point(
        "Vision productionisation",
        vision_score * 0.75,
        "model preprocessing, benchmark, acceleration, licensing, and package contracts",
        "redistributable production models and measured application integration remain incomplete",
    )

    plugin_files = "\n".join(read(path) for path in (ROOT / "loom-plugin-sdk").rglob("*.rs"))
    plugin_tokens = ("permission", "wasm", "signature", "trust", "rollback", "migration", "ui")
    plugin_score = sum(token in plugin_files.lower() for token in plugin_tokens) / len(plugin_tokens)
    fn_point(
        "plugin productionisation",
        plugin_score * 0.75,
        "permission, sandbox, trust, update, rollback, migration, and UI contracts",
        "production native CLAP/VST3 isolation and complete host UI ABI remain incomplete",
    )

    interop = read(ROOT / "loom-core/crates/loom-interop/src/lib.rs")
    interop_tokens = ("docx", "xlsx", "pptx", "psd", "odt", "ods", "odp", "fidelity")
    format_coverage = sum(token in interop.lower() for token in interop_tokens) / len(interop_tokens)
    fixture_count = len(list((ROOT / "loom-samples").rglob("*"))) if (ROOT / "loom-samples").exists() else 0
    fn_point(
        "interoperability fidelity",
        min(0.65, format_coverage * 0.45 + min(fixture_count, 20) / 100.0),
        f"declared format coverage={format_coverage:.0%}, sample/conformance entries={fixture_count}",
        "broad round-trip conformance, compatibility reports, and loss budgets remain incomplete",
    )

    journey_ratio = sample_open_count / len(APPS) if report else 0.0
    delivery_score = 0.35 if contains_all(native_workflow, ("windows-2025", "macos-15", "macos-15-intel")) else 0.0
    delivery_score += 0.25 if packages else 0.0
    delivery_score += 0.4 * journey_ratio
    fn_point(
        "native delivery and user-journey evidence",
        delivery_score,
        f"native packages={len(packages)}, sample-open journeys={sample_open_count}/8",
        None if journey_ratio == 1.0 and packages else "all native package and sample-open journeys must pass",
    )

    ui_score = round(sum(float(item["score"]) for item in ui_dimensions), 2)
    functionality_score = round(sum(float(item["score"]) for item in functionality_dimensions), 2)
    blockers = [
        {"area": "ui", "dimension": item["name"], "detail": item["blocker"]}
        for item in ui_dimensions if "blocker" in item
    ] + [
        {"area": "functionality", "dimension": item["name"], "detail": item["blocker"]}
        for item in functionality_dimensions if "blocker" in item
    ]
    return {
        "schema_version": 2,
        "target": 10.0,
        "ui": {"score": ui_score, "dimensions": ui_dimensions},
        "functionality": {"score": functionality_score, "dimensions": functionality_dimensions},
        "blockers": blockers,
        "evidence_root": str(evidence_root) if evidence_root else None,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", type=Path)
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--minimum-ui", type=float, default=0.0)
    parser.add_argument("--minimum-functionality", type=float, default=0.0)
    arguments = parser.parse_args()
    payload = score(arguments.evidence_root)
    rendered = json.dumps(payload, indent=2, sort_keys=True)
    print(rendered)
    if arguments.json:
        arguments.json.parent.mkdir(parents=True, exist_ok=True)
        arguments.json.write_text(rendered + "\n", encoding="utf-8")
    ui = float(payload["ui"]["score"])
    functionality = float(payload["functionality"]["score"])
    if ui < arguments.minimum_ui or functionality < arguments.minimum_functionality:
        print(
            f"product readiness below gate: UI {ui}/10 (minimum {arguments.minimum_ui}), "
            f"functionality {functionality}/10 (minimum {arguments.minimum_functionality})",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
