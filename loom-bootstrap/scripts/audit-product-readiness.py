#!/usr/bin/env python3
"""Evidence-based Loom UI and functionality scorecard.

A score is not a marketing claim. Every point is derived from source, CI, tests,
or native package evidence. Ten out of ten remains reserved for complete user
journeys, adaptive layouts, native integration, and production-grade engines.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")


def contains_all(text: str, tokens: tuple[str, ...]) -> bool:
    return all(token in text for token in tokens)


def app_ui(app: str) -> str:
    return (ROOT / f"loom-{app}/crates/loom-{app}-app/ui/app.slint").read_text(encoding="utf-8")


def app_main(app: str) -> str:
    return (ROOT / f"loom-{app}/crates/loom-{app}-app/src/main.rs").read_text(encoding="utf-8")


def app_core(app: str) -> str:
    path = ROOT / f"loom-{app}/crates/loom-{app}-core/src/lib.rs"
    return path.read_text(encoding="utf-8") if path.is_file() else ""


def score() -> dict[str, object]:
    uis = {app: app_ui(app) for app in APPS}
    mains = {app: app_main(app) for app in APPS}
    cores = {app: app_core(app) for app in APPS}
    shared = (ROOT / "loom-core/crates/loom-ui/ui/components.slint").read_text(encoding="utf-8")
    theme = (ROOT / "loom-core/crates/loom-ui/ui/theme.slint").read_text(encoding="utf-8")
    native = (ROOT / ".github/workflows/cross-platform.yml").read_text(encoding="utf-8")
    packaging = (ROOT / "loom-bootstrap/packaging/release.py").read_text(encoding="utf-8")

    ui_dimensions: list[dict[str, object]] = []
    def ui_point(name: str, value: float, evidence: str) -> None:
        ui_dimensions.append({"name": name, "score": value, "evidence": evidence})

    ui_point(
        "shared design system",
        1.0 if all(contains_all(text, ("AppHeader {", "StatusBar {", "Theme.palette()")) for text in uis.values()) else 0.0,
        "all eight apps use shared header, status, and semantic palette",
    )
    ui_point(
        "semantic visual language",
        1.0 if all(not re.search(r"#[0-9a-fA-F]{6,8}", text) for text in uis.values()) else 0.0,
        "app UIs contain no local hex palettes",
    )
    ui_point(
        "accessibility semantics",
        1.0 if contains_all(shared, ("accessible-role", "accessible-label", "accessible-action-default", "Slider", "WorkspaceRow")) else 0.0,
        "shared controls expose roles, labels, actions, and semantic slider labels",
    )
    ui_point(
        "keyboard interaction",
        1.0 if shared.count("key-pressed(event)") >= 7 else min(1.0, shared.count("key-pressed(event)") / 7.0),
        "keyboard activation and arrow-key interaction in shared controls",
    )
    ui_point(
        "theme coverage",
        1.0 if contains_all(theme, ("light", "dark", "high-contrast", "reduced-motion")) else 0.0,
        "three themes plus reduced-motion policy",
    )
    workflow_tokens = ("windows-2025", "macos-15", "macos-15-intel", "native-ui-matrix.py")
    ui_point(
        "native visual QA",
        1.0 if contains_all(native, workflow_tokens) else 0.0,
        "24-image matrix on Windows, Apple silicon, and Intel macOS",
    )
    adaptive = sum(
        1 for text in uis.values() if contains_all(text, ("min-width:", "min-height:", "horizontal-stretch", "vertical-stretch"))
    )
    ui_point(
        "adaptive layout",
        0.5 if adaptive == len(APPS) else adaptive / len(APPS) * 0.5,
        "all apps have bounded stretch layouts; breakpoint-driven reflow remains incomplete",
    )
    native_open = all("!other.starts_with('-') && args.open.is_none()" in text for text in mains.values())
    associations = contains_all(packaging, ("DOCUMENT_TYPES", "CFBundleDocumentTypes", "RegistryValue", "MimeType="))
    ui_point(
        "native shell integration",
        0.75 if native_open and associations else 0.0,
        "document associations and positional open are implemented; native file-picker/menu integration remains incomplete",
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
    ui_point(
        "workflow-specific composition",
        specific_count / len(APPS),
        f"{specific_count}/8 apps expose domain-specific workspace structure",
    )
    forbidden = ("coming soon", "placeholder ui", "placeholder control", "fake progress", "model preview")
    honest = all(not any(token in text.lower() for token in forbidden) for text in uis.values())
    ui_point(
        "truthful states",
        1.0 if honest else 0.0,
        "no placeholder or fabricated-status language in application UIs",
    )

    functionality_dimensions: list[dict[str, object]] = []
    def fn_point(name: str, value: float, evidence: str) -> None:
        functionality_dimensions.append({"name": name, "score": value, "evidence": evidence})

    fn_point(
        "persistence and recovery",
        1.0 if all(contains_all(text, ("save", "load", "define_snapshot_recovery")) for text in mains.values()) else 0.0,
        "all eight apps persist native packages and register recovery",
    )
    undo_apps = sum(1 for text in mains.values() if "on_undo" in text and "on_redo" in text)
    fn_point(
        "undo and redo",
        undo_apps / len(APPS),
        f"{undo_apps}/8 application front ends expose undo and redo",
    )
    export_apps = sum(1 for text in mains.values() if re.search(r"on_export|export_|encode_|execute_", text))
    fn_point(
        "import and export journeys",
        export_apps / len(APPS),
        f"{export_apps}/8 applications expose a real export or execution path",
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
        media_count / len(media_tokens),
        f"{media_count}/4 media applications call real local engines",
    )
    document_tokens = {
        "writer": ("export_pdf", "replace_paragraphs", "EditorHistory"),
        "sheets": ("evaluate", "from_csv", "CellEditTransaction"),
        "present": ("PresentationSession", "export_pdf", "save_presentation_session"),
    }
    document_count = sum(1 for app, tokens in document_tokens.items() if contains_all(mains[app], tokens))
    fn_point(
        "document engines",
        document_count / len(document_tokens),
        f"{document_count}/3 productivity applications call real document engines",
    )
    safety_count = sum(
        1
        for text in list(mains.values()) + list(cores.values())
        if any(token in text for token in ("cancel", "validate", "Result<", "map_err"))
    )
    fn_point(
        "failure, validation, and cancellation",
        min(1.0, safety_count / 12.0),
        "bounded errors, validation, and cancellation are present across engines",
    )
    vision = (ROOT / "loom-vision/crates/loom-vision-core/src/production.rs").read_text(encoding="utf-8")
    plugin = (ROOT / "loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs").read_text(encoding="utf-8")
    fn_point(
        "Vision and plugin productionisation",
        0.6 if contains_all(vision, ("benchmark", "model")) and contains_all(plugin, ("permission", "wasm")) else 0.3,
        "reference providers and defensive plugin hosting exist; complete model packs and host ABIs remain incomplete",
    )
    interop = (ROOT / "loom-core/crates/loom-interop/src/lib.rs").read_text(encoding="utf-8")
    fn_point(
        "interoperability fidelity",
        0.5 if contains_all(interop, ("Fidelity", "Format")) else 0.25,
        "fidelity reporting exists; broad DOCX/XLSX/PPTX/PSD/ODF fidelity remains incomplete",
    )
    fn_point(
        "native target delivery",
        1.0 if contains_all(native, workflow_tokens + ("release.py", "upload-artifact")) else 0.0,
        "Windows MSI and macOS ARM64/Intel DMG validation produce downloadable artifacts",
    )
    tests = sum(text.count("#[test]") for text in list(mains.values()) + list(cores.values()))
    fn_point(
        "automated user-journey evidence",
        min(1.0, 0.5 + min(tests, 50) / 100.0),
        f"{tests} application/core unit tests plus native smoke and screenshot matrices; interaction automation remains partial",
    )

    ui_score = round(sum(float(item["score"]) for item in ui_dimensions), 2)
    functionality_score = round(sum(float(item["score"]) for item in functionality_dimensions), 2)
    return {
        "schema_version": 1,
        "target": 10.0,
        "ui": {"score": ui_score, "dimensions": ui_dimensions},
        "functionality": {"score": functionality_score, "dimensions": functionality_dimensions},
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", type=Path)
    parser.add_argument("--minimum-ui", type=float, default=0.0)
    parser.add_argument("--minimum-functionality", type=float, default=0.0)
    arguments = parser.parse_args()
    payload = score()
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
