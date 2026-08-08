#!/usr/bin/env python3
from __future__ import annotations

import base64
import gzip
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VALIDATOR_GZIP_B64 = "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"

release = ROOT / "loom-bootstrap/packaging/release.py"
release_text = release.read_text(encoding="utf-8")
needle = '        "version": version,\n        "artifacts": [asdict(item) for item in artifacts],'
replacement = '        "version": version,\n        "commit_sha": os.environ.get("GITHUB_SHA"),\n        "artifacts": [asdict(item) for item in artifacts],'
if replacement not in release_text:
    if release_text.count(needle) != 1:
        raise SystemExit("release.py: manifest payload target drifted")
    release_text = release_text.replace(needle, replacement, 1)
release.write_text(release_text, encoding="utf-8")

validator_path = ROOT / "loom-bootstrap/packaging/validate.py"
validator_path.write_bytes(gzip.decompress(base64.b64decode(VALIDATOR_GZIP_B64)))

readiness = ROOT / "loom-bootstrap/scripts/audit-product-readiness.py"
readiness_text = readiness.read_text(encoding="utf-8")
if "import hashlib" not in readiness_text:
    readiness_text = readiness_text.replace("import argparse\n", "import argparse\nimport hashlib\n", 1)
pattern = re.compile(r"def native_packages\(evidence_root: Path \| None\) -> list\[Path\]:.*?\n\ndef score", re.S)
replacement_fn = """def validated_native_packages(evidence_root: Path | None) -> list[Path]:
    if evidence_root is None or not evidence_root.exists():
        return []
    suffixes = (".msi", ".dmg", ".pkg", ".deb", ".appimage", ".zip", ".tar.gz")
    validated: list[Path] = []
    for path in evidence_root.rglob("*"):
        if not path.is_file() or not path.name.lower().endswith(suffixes):
            continue
        report = load_json(path.parent / "package-validation.json")
        if not report or report.get("passed") is not True:
            continue
        artifact = report.get("artifact")
        if not isinstance(artifact, dict) or artifact.get("path") != path.name:
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if artifact.get("sha256") != digest:
            continue
        validated.append(path)
    return validated


def score"""
if "def validated_native_packages(" not in readiness_text:
    readiness_text, count = pattern.subn(replacement_fn, readiness_text, count=1)
    if count != 1:
        raise SystemExit("audit-product-readiness.py: native package detector drifted")
readiness_text = readiness_text.replace(
    "    packages = native_packages(evidence_root)",
    "    packages = validated_native_packages(evidence_root)",
)
readiness.write_text(readiness_text, encoding="utf-8")

contracts = ROOT / "loom-bootstrap/scripts/audit-contracts.py"
contracts_text = contracts.read_text(encoding="utf-8")
block = '''
# Native package evidence must be independently validated before scoring.
release_source = (ROOT / "loom-bootstrap/packaging/release.py").read_text(encoding="utf-8")
validator_source = ROOT / "loom-bootstrap/packaging/validate.py"
ci_source = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
if '"commit_sha": os.environ.get("GITHUB_SHA")' not in release_source:
    errors.append("release manifests must record source commit provenance")
if not validator_source.is_file():
    errors.append("native package validator is missing")
if "Validate native package contents" not in cross_platform:
    errors.append("native workflow must independently validate package contents")
if "validated_native_packages" not in readiness or "package-validation.json" not in readiness:
    errors.append("readiness may only count independently validated native packages")
if "--minimum-ui" in ci_source or "--minimum-functionality" in ci_source:
    errors.append("source-only readiness diagnostics must not enforce native-evidence thresholds")
'''
if "Native package evidence must be independently validated before scoring." not in contracts_text:
    marker = "\nif errors:\n"
    if contracts_text.count(marker) != 1:
        raise SystemExit("audit-contracts.py: final error marker drifted")
    contracts_text = contracts_text.replace(marker, "\n" + block + marker, 1)
contracts.write_text(contracts_text, encoding="utf-8")

truth = ROOT / "TRUTH.md"
truth_text = truth.read_text(encoding="utf-8")
truth_note = '''### Native package validation baseline

Native package readiness now requires independent inspection of the produced DEB,
MSI, or DMG, including artifact hash/provenance, executable architecture, all eight
application payloads, and native document registrations. Merely producing a package
filename is not readiness evidence. This infrastructure improvement does **not** by
itself promote the complete-suite product score; four-platform evidence must pass.

'''
anchor = "### Present, Photo, and Motion re-audit\n"
if truth_note not in truth_text:
    if anchor not in truth_text:
        raise SystemExit("TRUTH.md: Phase 0 re-audit anchor missing")
    truth_text = truth_text.replace(anchor, truth_note + anchor, 1)
truth.write_text(truth_text, encoding="utf-8")
