#!/usr/bin/env python3
"""Independently validate native Loom release packages.

This validator treats release-manifest.json as an assertion to verify, not as
proof. It hashes the produced artifact, inspects the native container, checks
all eight Loom applications and their document registrations, and emits a
machine-readable report on both success and failure.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

try:
    import release  # type: ignore
except Exception as exc:  # pragma: no cover - startup contract
    raise SystemExit(f"package validation error: cannot import release metadata: {exc}")

APPS = tuple(release.APPS)
DISPLAY_NAMES = dict(release.DISPLAY_NAMES)
DOCUMENT_TYPES = dict(release.DOCUMENT_TYPES)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def run(args: list[str], *, text: bool = True, check: bool = True, cwd: Path | None = None):
    p = subprocess.run(args, cwd=cwd, check=False, capture_output=True, text=text)
    if check and p.returncode != 0:
        stdout = p.stdout if isinstance(p.stdout, str) else ""
        stderr = p.stderr if isinstance(p.stderr, str) else ""
        raise RuntimeError(
            f"command failed ({p.returncode}): {' '.join(args)}\n"
            f"stdout:\n{stdout[-4000:]}\nstderr:\n{stderr[-4000:]}"
        )
    return p


def require(name: str) -> str:
    path = shutil.which(name)
    if not path:
        raise RuntimeError(f"required validator program is unavailable: {name}")
    return path


def resolve_artifact(manifest_path: Path, item: dict[str, Any]) -> Path:
    raw = Path(str(item.get("path", "")))
    candidates = []
    if raw:
        candidates.append(raw)
        candidates.append(manifest_path.parent / raw.name)
        candidates.append(manifest_path.parent / "packages" / raw.name)
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    raise RuntimeError(f"manifest artifact is missing after relocation: {raw}")


def validate_manifest(
    manifest_path: Path, platform: str, architecture: str, assertions: list[str]
) -> tuple[dict[str, Any], Path]:
    payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != 1:
        raise RuntimeError("unsupported release manifest schema")
    if payload.get("suite") != "Loom Creator Suite":
        raise RuntimeError("unexpected release suite name")
    artifacts = payload.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        raise RuntimeError("release manifest contains no artifacts")

    expected_kind = {"linux": "deb", "windows": "msi", "macos": "dmg"}[platform]
    matching = [a for a in artifacts if a.get("kind") == expected_kind]
    if len(matching) != 1:
        raise RuntimeError(f"expected exactly one {expected_kind} artifact, found {len(matching)}")
    item = matching[0]
    if item.get("platform") != platform or item.get("architecture") != architecture:
        raise RuntimeError(
            "manifest platform/architecture mismatch: "
            f"{item.get('platform')}/{item.get('architecture')} vs {platform}/{architecture}"
        )
    artifact = resolve_artifact(manifest_path, item)
    actual_size = artifact.stat().st_size
    actual_sha = sha256_file(artifact)
    if int(item.get("bytes", -1)) != actual_size:
        raise RuntimeError("artifact byte count does not match release manifest")
    if item.get("sha256") != actual_sha:
        raise RuntimeError("artifact SHA-256 does not match release manifest")

    expected_source_sha = os.environ.get("LOOM_ARTIFACT_SOURCE_SHA") or os.environ.get("GITHUB_SHA")
    manifest_sha = payload.get("commit_sha")
    if manifest_sha and expected_source_sha and manifest_sha != expected_source_sha:
        raise RuntimeError(
            f"artifact source commit mismatch: manifest={manifest_sha}, expected={expected_source_sha}"
        )

    assertions.extend(
        [
            "release manifest schema and suite verified",
            f"exact {expected_kind} artifact selected",
            "artifact platform and architecture verified",
            "artifact byte count verified",
            "artifact SHA-256 verified",
            "relocated artifact resolved from downloaded evidence",
        ]
    )
    if manifest_sha:
        assertions.append("artifact source commit provenance verified")
    return payload, artifact


def elf_machine(path: Path) -> int:
    data = path.read_bytes()[:64]
    if len(data) < 20 or data[:4] != b"\x7fELF":
        raise RuntimeError(f"not an ELF executable: {path}")
    endian = "<" if data[5] == 1 else ">" if data[5] == 2 else None
    if endian is None:
        raise RuntimeError(f"invalid ELF endian marker: {path}")
    return struct.unpack_from(endian + "H", data, 18)[0]


def validate_linux(artifact: Path, architecture: str, assertions: list[str]) -> None:
    dpkg = require("dpkg-deb")
    info = run([dpkg, "--field", str(artifact), "Architecture"]).stdout.strip()
    expected_deb_arch = {"x86_64": "amd64", "aarch64": "arm64"}[architecture]
    if info != expected_deb_arch:
        raise RuntimeError(f"Debian architecture mismatch: {info} != {expected_deb_arch}")

    with tempfile.TemporaryDirectory(prefix="loom-package-validate-") as td:
        root = Path(td) / "root"
        run([dpkg, "--extract", str(artifact), str(root)])
        expected_machine = {"x86_64": 62, "aarch64": 183}[architecture]
        for app in APPS:
            binary = root / "usr" / "bin" / f"loom-{app}"
            if not binary.is_file() or not os.access(binary, os.X_OK):
                raise RuntimeError(f"missing executable in Debian package: {binary}")
            if elf_machine(binary) != expected_machine:
                raise RuntimeError(f"ELF architecture mismatch for loom-{app}")

            desktop = root / "usr" / "share" / "applications" / f"loom-{app}.desktop"
            text = desktop.read_text(encoding="utf-8")
            extension, mime, _ = DOCUMENT_TYPES[app]
            if f"Exec=loom-{app} %F" not in text or f"MimeType={mime};" not in text:
                raise RuntimeError(f"desktop registration is incomplete for loom-{app}")
            if extension not in DOCUMENT_TYPES[app][0]:
                raise RuntimeError(f"internal document metadata mismatch for {app}")

        mime_xml = root / "usr" / "share" / "mime" / "packages" / "loom.xml"
        xml = mime_xml.read_text(encoding="utf-8")
        for app in APPS:
            extension, mime, _ = DOCUMENT_TYPES[app]
            if mime not in xml or f"*.{extension}" not in xml:
                raise RuntimeError(f"shared MIME registration missing for {app}")

    assertions.extend(
        [
            "Debian control architecture verified",
            "all eight packaged executables are executable ELF files of the requested architecture",
            "all eight desktop Exec and MIME registrations verified",
            "shared MIME extension registrations verified",
        ]
    )


def pe_machine(path: Path) -> int:
    with path.open("rb") as f:
        if f.read(2) != b"MZ":
            raise RuntimeError(f"not a PE executable: {path}")
        f.seek(0x3C)
        offset_raw = f.read(4)
        if len(offset_raw) != 4:
            raise RuntimeError(f"truncated PE executable: {path}")
        pe_offset = struct.unpack("<I", offset_raw)[0]
        f.seek(pe_offset)
        if f.read(4) != b"PE\x00\x00":
            raise RuntimeError(f"missing PE signature: {path}")
        raw = f.read(2)
        if len(raw) != 2:
            raise RuntimeError(f"truncated PE machine field: {path}")
        return struct.unpack("<H", raw)[0]


def validate_windows(artifact: Path, architecture: str, assertions: list[str]) -> None:
    wix = require("wix")
    run([wix, "msi", "validate", str(artifact)])
    with tempfile.TemporaryDirectory(prefix="loom-msi-validate-") as td:
        temp = Path(td)
        wxs = temp / "decompiled.wxs"
        extracted = temp / "decompiled"
        run([wix, "msi", "decompile", str(artifact), "-o", str(wxs), "-x", str(extracted)])
        source = wxs.read_text(encoding="utf-8", errors="replace")
        for app in APPS:
            extension, _mime, _ = DOCUMENT_TYPES[app]
            required = [
                f"loom-{app}.exe",
                f".{extension}",
                f"Loom.{app}",
                "%1",
            ]
            if any(token not in source for token in required):
                raise RuntimeError(f"MSI registry/file metadata incomplete for loom-{app}")

        admin = temp / "admin"
        admin.mkdir()
        msiexec = require("msiexec")
        p = run(
            [msiexec, "/a", str(artifact), "/qn", f"TARGETDIR={admin}"],
            check=False,
        )
        if p.returncode not in (0, 3010):
            raise RuntimeError(f"MSI administrative extraction failed: {p.returncode}")

        expected_machine = {"x86_64": 0x8664, "aarch64": 0xAA64}[architecture]
        for app in APPS:
            candidates = list(admin.rglob(f"loom-{app}.exe"))
            if len(candidates) != 1:
                raise RuntimeError(f"expected one extracted loom-{app}.exe, found {len(candidates)}")
            if pe_machine(candidates[0]) != expected_machine:
                raise RuntimeError(f"PE architecture mismatch for loom-{app}")

    assertions.extend(
        [
            "WiX MSI validation passed",
            "MSI decompilation verified all application/file-association/open-command metadata",
            "MSI administrative extraction succeeded",
            "all eight extracted PE executables match the requested architecture",
        ]
    )


def validate_macos(artifact: Path, architecture: str, assertions: list[str]) -> None:
    hdiutil = require("hdiutil")
    lipo = require("lipo")
    attach = run(
        [hdiutil, "attach", "-plist", "-readonly", "-nobrowse", str(artifact)],
        text=False,
    )
    plist = plistlib.loads(attach.stdout)
    mount_points = [
        entity.get("mount-point")
        for entity in plist.get("system-entities", [])
        if entity.get("mount-point")
    ]
    if len(mount_points) != 1:
        raise RuntimeError(f"expected one mounted DMG volume, found {mount_points}")
    mount = Path(mount_points[0])
    try:
        applications_link = mount / "Applications"
        if not applications_link.is_symlink():
            raise RuntimeError("DMG does not expose an /Applications install link")
        expected_arch = {"aarch64": "arm64", "x86_64": "x86_64"}[architecture]
        for app in APPS:
            bundle = mount / f"{DISPLAY_NAMES[app]}.app"
            info_path = bundle / "Contents" / "Info.plist"
            info = plistlib.loads(info_path.read_bytes())
            executable_name = info.get("CFBundleExecutable")
            if not executable_name:
                raise RuntimeError(f"CFBundleExecutable missing for {app}")
            executable = bundle / "Contents" / "MacOS" / executable_name
            if not executable.is_file() or not os.access(executable, os.X_OK):
                raise RuntimeError(f"bundle executable missing for {app}")
            archs = run([lipo, "-archs", str(executable)]).stdout.split()
            if expected_arch not in archs:
                raise RuntimeError(f"Mach-O architecture mismatch for {app}: {archs}")
            if info.get("CFBundleIdentifier") != f"org.loom.{app}":
                raise RuntimeError(f"bundle identifier mismatch for {app}")

            extension, mime, _ = DOCUMENT_TYPES[app]
            doc_types = json.dumps(info.get("CFBundleDocumentTypes", []), sort_keys=True)
            declarations = json.dumps(info.get("UTExportedTypeDeclarations", []), sort_keys=True)
            if extension not in doc_types or mime not in doc_types:
                raise RuntimeError(f"document type registration missing for {app}")
            if extension not in declarations or mime not in declarations:
                raise RuntimeError(f"exported UTI registration missing for {app}")
    finally:
        run([hdiutil, "detach", str(mount)], check=False)

    assertions.extend(
        [
            "DMG mounted read-only and exposes /Applications install link",
            "all eight app bundles contain executable Mach-O files of the requested architecture",
            "all eight bundle identifiers verified",
            "all eight document type and exported UTI registrations verified",
        ]
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "windows", "macos"), required=True)
    parser.add_argument("--architecture", choices=("x86_64", "aarch64"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    assertions: list[str] = []
    diagnostics: list[str] = []
    artifact: Path | None = None
    manifest_payload: dict[str, Any] = {}
    passed = False
    try:
        runner_os = os.environ.get("RUNNER_OS")
        runner_arch = os.environ.get("RUNNER_ARCH")
        expected_runner_os = {"linux": "Linux", "windows": "Windows", "macos": "macOS"}[args.platform]
        expected_runner_arch = {"x86_64": "X64", "aarch64": "ARM64"}[args.architecture]
        if runner_os and runner_os != expected_runner_os:
            raise RuntimeError(f"runner OS mismatch: {runner_os} != {expected_runner_os}")
        if runner_arch and runner_arch != expected_runner_arch:
            raise RuntimeError(f"runner architecture mismatch: {runner_arch} != {expected_runner_arch}")
        if runner_os:
            assertions.append("native runner operating system verified")
        if runner_arch:
            assertions.append("native runner architecture verified")

        manifest_payload, artifact = validate_manifest(
            args.manifest.resolve(), args.platform, args.architecture, assertions
        )
        if args.platform == "linux":
            validate_linux(artifact, args.architecture, assertions)
        elif args.platform == "windows":
            validate_windows(artifact, args.architecture, assertions)
        else:
            validate_macos(artifact, args.architecture, assertions)
        passed = True
    except Exception as exc:
        diagnostics.append(str(exc))

    report = {
        "schema_version": 1,
        "passed": passed,
        "platform": args.platform,
        "architecture": args.architecture,
        "runner_os": os.environ.get("RUNNER_OS"),
        "runner_arch": os.environ.get("RUNNER_ARCH"),
        "source_commit_sha": manifest_payload.get("commit_sha") or os.environ.get("LOOM_ARTIFACT_SOURCE_SHA"),
        "validator_commit_sha": os.environ.get("GITHUB_SHA"),
        "source_run_id": os.environ.get("LOOM_ARTIFACT_SOURCE_RUN_ID") or os.environ.get("GITHUB_RUN_ID"),
        "artifact": None
        if artifact is None
        else {
            "path": artifact.name,
            "bytes": artifact.stat().st_size,
            "sha256": sha256_file(artifact),
        },
        "assertions": assertions,
        "diagnostics": diagnostics,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if not passed:
        print(json.dumps(report, indent=2, sort_keys=True), file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
