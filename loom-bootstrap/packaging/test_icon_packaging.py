"""App artwork must reach native package identity without changing its bytes."""

from __future__ import annotations

import configparser
import plistlib
import shutil
import subprocess
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest

import release
import validate

ROOT = Path(__file__).resolve().parents[2]
ICONS = ROOT / "loom-core/crates/loom-ui/ui/app-icons"


def binaries(tmp_path: Path) -> dict[str, Path]:
    result = {}
    for app in release.APPS:
        path = tmp_path / f"loom-{app}"
        path.write_bytes(b"test executable")
        result[app] = path
    return result


def test_linux_package_installs_original_pngs_and_matching_desktop_keys(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(release, "collect_binaries", lambda *_: binaries(tmp_path))
    monkeypatch.setattr(release, "require_program", lambda name: name)
    monkeypatch.setattr(release.shutil, "which", lambda _: None)
    inspected = []
    theme = configparser.ConfigParser(interpolation=None)
    theme.read(Path(__file__).parent / "fixtures/hicolor-index.theme")
    indexed_directories = theme["Icon Theme"]["Directories"].split(",")

    def inspect_package(arguments: list[str], **_: object) -> None:
        package = Path(arguments[-2])
        hicolor = package / "usr/share/icons/hicolor"
        for app in release.APPS:
            icons = list(hicolor.rglob(f"loom-{app}.png"))
            assert len(icons) == 1
            icon = icons[0]
            directory = icon.parent.relative_to(hicolor).as_posix()
            assert (
                directory in indexed_directories
            ), f"hicolor does not index {directory}"
            size = int(theme[directory]["Size"]) * int(
                theme[directory].get("Scale", "1")
            )
            assert struct.unpack(">II", icon.read_bytes()[16:24]) == (size, size)
            assert icon.read_bytes() == (ICONS / f"loom-{app}.512.png").read_bytes()
            desktop = package / f"usr/share/applications/loom-{app}.desktop"
            assert f"Icon=loom-{app}\n" in desktop.read_text()
            inspected.append(app)
        Path(arguments[-1]).write_bytes(b"test deb")

    monkeypatch.setattr(release, "run", inspect_package)
    release.package_linux(ROOT, tmp_path / "output", "0.1.0", "x86_64", True)
    assert inspected == list(release.APPS)


def test_macos_bundles_declare_and_contain_original_icns(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(release, "collect_binaries", lambda *_: binaries(tmp_path))
    monkeypatch.setattr(release, "require_program", lambda name: name)
    monkeypatch.delenv("LOOM_MACOS_CODESIGN_IDENTITY", raising=False)
    inspected = []

    def inspect_volume(arguments: list[str], **_: object) -> None:
        volume = Path(arguments[arguments.index("-srcfolder") + 1])
        for app in release.APPS:
            bundle = volume / f"{release.DISPLAY_NAMES[app]}.app/Contents"
            info = plistlib.loads((bundle / "Info.plist").read_bytes())
            assert info["CFBundleIconFile"] == f"loom-{app}.icns"
            icon = bundle / "Resources" / info["CFBundleIconFile"]
            assert icon.read_bytes() == (ICONS / f"loom-{app}.icns").read_bytes()
            inspected.append(app)
        Path(arguments[-1]).write_bytes(b"test dmg")

    monkeypatch.setattr(release, "run_with_retries", inspect_volume)
    release.package_macos(ROOT, tmp_path / "output", "0.1.0", "aarch64", True)
    assert inspected == list(release.APPS)


def test_wix_shortcuts_reference_declared_original_icons(tmp_path: Path) -> None:
    source = release.wix_source(binaries(tmp_path), "0.1.0", "x86_64", ROOT)
    document = ET.fromstring(source)
    namespace = {"w": "http://wixtoolset.org/schemas/v4/wxs"}
    icons = {
        item.attrib["Id"]: item for item in document.findall(".//w:Icon", namespace)
    }
    for app in release.APPS:
        shortcut = document.find(f'.//w:Shortcut[@Id="Shortcut_{app}"]', namespace)
        assert shortcut is not None
        icon = icons[shortcut.attrib["Icon"]]
        assert (
            Path(icon.attrib["SourceFile"]).read_bytes()
            == (ICONS / f"loom-{app}.ico").read_bytes()
        )


def decompiled_icon_source(tmp_path: Path) -> tuple[str, Path]:
    """WiX 4 preserves Icon IDs and emits absolute -x/Icon/ID SourceFiles."""
    source = release.wix_source(binaries(tmp_path), "0.1.0", "x86_64", ROOT)
    extracted = tmp_path / "decompiled"
    (extracted / "Icon").mkdir(parents=True)
    for app in release.APPS:
        original = ICONS / f"loom-{app}.ico"
        destination = extracted / "Icon" / f"Icon_{app}.ico"
        shutil.copyfile(original, destination)
        source = source.replace(str(original), str(destination))
    return source, extracted


@pytest.mark.parametrize(
    "fault", [None, "missing_source", "payload", "outside", "index"]
)
def test_wix_validator_checks_extracted_icon_payloads_and_indices(
    tmp_path: Path,
    fault: str | None,
) -> None:
    source, extracted = decompiled_icon_source(tmp_path)
    icon = extracted / "Icon/Icon_writer.ico"
    if fault == "missing_source":
        icon.unlink()
    elif fault == "payload":
        icon.write_bytes(b"substituted ICO")
    elif fault == "outside":
        outside = tmp_path / "outside.ico"
        shutil.copyfile(icon, outside)
        source = source.replace(str(icon), str(outside))
    elif fault == "index":
        source = source.replace('IconIndex="0"', 'IconIndex="1"', 1)
    if fault:
        with pytest.raises(RuntimeError, match="application icon"):
            validate.validate_shortcut_icons(source, extracted)
    else:
        validate.validate_shortcut_icons(source, extracted)


@pytest.mark.parametrize("replacement", ["", "Icon_sheets.ico"])
def test_wix_validator_rejects_missing_or_swapped_shortcut_icon(
    tmp_path: Path,
    replacement: str,
) -> None:
    source, extracted = decompiled_icon_source(tmp_path)
    source = source.replace('Icon="Icon_writer.ico"', f'Icon="{replacement}"')
    with pytest.raises(RuntimeError, match="shortcut application icon"):
        validate.validate_shortcut_icons(source, extracted)


def test_appimage_stages_original_pngs(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(release, "collect_binaries", lambda *_: binaries(tmp_path))
    monkeypatch.setattr(release, "require_program", lambda name: name)
    monkeypatch.setattr(release.shutil, "which", lambda _: "appimagetool")
    monkeypatch.setattr(
        release, "run", lambda args: Path(args[-1]).write_bytes(b"test deb")
    )
    inspected = []

    def inspect_appdir(
        arguments: list[str], **_: object
    ) -> subprocess.CompletedProcess:
        appdir = Path(arguments[1])
        app = release.APPS[len(inspected)]
        assert (appdir / f"loom-{app}.png").read_bytes() == (
            ICONS / f"loom-{app}.png"
        ).read_bytes()
        assert f"Icon=loom-{app}\n" in (appdir / f"loom-{app}.desktop").read_text()
        inspected.append(app)
        Path(arguments[-1]).write_bytes(b"test appimage")
        return subprocess.CompletedProcess(arguments, 0)

    monkeypatch.setattr(release.subprocess, "run", inspect_appdir)
    release.package_linux(ROOT, tmp_path / "output", "0.1.0", "x86_64", True)
    assert inspected == list(release.APPS)


@pytest.mark.parametrize("fault", [None, "registration", "artwork"])
def test_linux_validator_checks_desktop_identity_and_original_artwork(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    fault: str | None,
) -> None:
    monkeypatch.setattr(validate, "require", lambda name: name)
    monkeypatch.setattr(validate, "elf_machine", lambda _: 62)

    def extract(arguments: list[str], **_: object) -> subprocess.CompletedProcess:
        if "--field" in arguments:
            return subprocess.CompletedProcess(arguments, 0, "amd64\n")
        root = Path(arguments[-1])
        for app in release.APPS:
            binary = root / f"usr/bin/loom-{app}"
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(b"test executable")
            binary.chmod(0o755)
            desktop = root / f"usr/share/applications/loom-{app}.desktop"
            desktop.parent.mkdir(parents=True, exist_ok=True)
            icon_name = (
                "wrong"
                if fault == "registration" and app == "writer"
                else f"loom-{app}"
            )
            desktop.write_text(
                f"Exec=loom-{app} %F\nMimeType={release.DOCUMENT_TYPES[app][1]};\nIcon={icon_name}\n"
            )
            icon = root / f"usr/share/icons/hicolor/512x512/apps/loom-{app}.png"
            icon.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ICONS / f"loom-{app}.512.png", icon)
            if fault == "artwork" and app == "writer":
                icon.write_bytes(b"substituted icon")
        mime = root / "usr/share/mime/packages/loom.xml"
        mime.parent.mkdir(parents=True)
        mime.write_text(release.linux_mime_xml())
        return subprocess.CompletedProcess(arguments, 0)

    monkeypatch.setattr(validate, "run", extract)
    if fault:
        with pytest.raises(RuntimeError, match="desktop registration|application icon"):
            validate.validate_linux(tmp_path / "test.deb", "x86_64", [])
    else:
        assertions = []
        validate.validate_linux(tmp_path / "test.deb", "x86_64", assertions)
        assert (
            "application PNG artwork and desktop icon registrations verified"
            in assertions
        )


def test_missing_original_icon_stops_packaging(tmp_path: Path) -> None:
    with pytest.raises(SystemExit, match="missing application icon"):
        release.icon_path(tmp_path, "writer", "png")


@pytest.mark.parametrize("extension", ["png", "512.png", "icns"])
def test_validator_rejects_replaced_or_missing_app_artwork(
    tmp_path: Path,
    extension: str,
) -> None:
    icon = tmp_path / f"loom-writer.{extension}"
    shutil.copyfile(ICONS / icon.name, icon)
    validate.validate_icon(icon, "writer", extension)
    icon.write_bytes(b"substituted artwork")
    with pytest.raises(RuntimeError, match="application icon"):
        validate.validate_icon(icon, "writer", extension)
    icon.unlink()
    with pytest.raises(RuntimeError, match="application icon"):
        validate.validate_icon(icon, "writer", extension)


@pytest.mark.parametrize("extension", ["ico", "icns"])
def test_asset_audit_requires_icon_provenance(
    tmp_path: Path,
    extension: str,
) -> None:
    script = tmp_path / "loom-bootstrap/scripts/audit-assets.py"
    script.parent.mkdir(parents=True)
    shutil.copyfile(ROOT / "loom-bootstrap/scripts/audit-assets.py", script)
    contract = tmp_path / "loom-bootstrap/contracts/assets.toml"
    contract.parent.mkdir()
    contract.write_text("schema_version = 1\n")
    icon = tmp_path / f"loom-writer.{extension}"
    icon.write_bytes(b"test asset")
    result = subprocess.run(
        [sys.executable, str(script)], capture_output=True, text=True
    )
    assert result.returncode == 1
    assert f"unregistered asset provenance: {icon.name}" in result.stderr
    contract.write_text(
        "schema_version = 1\n[[allow]]\n"
        f'glob = "{icon.name}"\norigin = "Original Loom artwork"\n'
        'license = "MIT OR Apache-2.0"\ncommercial = true\nredistributable = true\n'
    )
    result = subprocess.run(
        [sys.executable, str(script)], capture_output=True, text=True
    )
    assert result.returncode == 0, result.stderr
