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


def _resource_section(
    resources: dict[int, dict[int, dict[int, bytes]]],
) -> bytes:
    nodes: dict[object, list[tuple[int, tuple[str, object]]]] = {"root": []}
    root_entries = []
    for resource_type, names in resources.items():
        type_key = ("type", resource_type)
        nodes[type_key] = []
        root_entries.append((resource_type, ("directory", type_key)))
        for name_id, languages in names.items():
            name_key = ("name", resource_type, name_id)
            nodes[name_key] = []
            nodes[type_key].append((name_id, ("directory", name_key)))
            for language_id, payload in languages.items():
                nodes[name_key].append((language_id, ("data", payload)))
    nodes["root"] = root_entries

    directory_offsets = {}
    cursor = 0
    for key, entries in nodes.items():
        directory_offsets[key] = cursor
        cursor += 16 + len(entries) * 8
    cursor = (cursor + 3) & ~3
    data_entries = {}
    payload_records = []
    for key, entries in nodes.items():
        for index, (_identifier, (kind, payload)) in enumerate(entries):
            if kind == "data":
                data_entries[(key, index)] = cursor
                payload_records.append((cursor, payload))
                cursor += 16
    cursor = (cursor + 3) & ~3
    payload_offsets = {}
    for data_entry, payload in payload_records:
        payload_offsets[data_entry] = cursor
        cursor += len(payload)
        cursor = (cursor + 3) & ~3

    section = bytearray(cursor)
    for key, entries in nodes.items():
        offset = directory_offsets[key]
        struct.pack_into("<IIHHHH", section, offset, 0, 0, 0, 0, 0, len(entries))
        for index, (identifier, (kind, payload)) in enumerate(entries):
            target = (
                0x80000000 | directory_offsets[payload]
                if kind == "directory"
                else data_entries[(key, index)]
            )
            struct.pack_into("<II", section, offset + 16 + index * 8, identifier, target)
    for data_entry, payload in payload_records:
        payload_offset = payload_offsets[data_entry]
        struct.pack_into(
            "<IIII",
            section,
            data_entry,
            0x1000 + payload_offset,
            len(payload),
            0,
            0,
        )
        section[payload_offset:payload_offset + len(payload)] = payload
    return bytes(section)


def _pe_image_with_resource_section(section: bytes) -> bytes:
    pe_offset = 0x80
    optional_size = 0xF0
    section_offset = pe_offset + 24 + optional_size
    raw_offset = 0x200
    raw_size = (len(section) + 0x1FF) & ~0x1FF
    image = bytearray(raw_offset + raw_size)
    image[:2] = b"MZ"
    struct.pack_into("<I", image, 0x3C, pe_offset)
    image[pe_offset:pe_offset + 4] = b"PE\x00\x00"
    struct.pack_into(
        "<HHIIIHH", image, pe_offset + 4, 0x8664, 1, 0, 0, 0,
        optional_size, 0x2022,
    )
    optional = pe_offset + 24
    struct.pack_into("<H", image, optional, 0x20B)
    struct.pack_into("<I", image, optional + 108, 16)
    struct.pack_into(
        "<II", image, optional + 112 + 2 * 8, 0x1000, len(section)
    )
    section_header = section_offset
    image[section_header:section_header + 8] = b".rsrc\x00\x00\x00"
    struct.pack_into(
        "<IIII", image, section_header + 8, len(section), 0x1000, raw_size, raw_offset
    )
    struct.pack_into("<I", image, section_header + 36, 0x40000040)
    image[raw_offset:raw_offset + len(section)] = section
    return bytes(image)


def _pe_with_resources(resources: dict[int, dict[int, dict[int, bytes]]]) -> bytes:
    return _pe_image_with_resource_section(_resource_section(resources))


def _application_pe_icon_fixture(app: str, fault: str | None = None) -> bytes:
    ico = (ICONS / f"loom-{app}.ico").read_bytes()
    _reserved, _kind, count = struct.unpack_from("<HHH", ico)
    group = bytearray(ico[:6])
    icon_resources = {}
    for index in range(count):
        entry_offset = 6 + index * 16
        entry = ico[entry_offset:entry_offset + 16]
        size, image_offset = struct.unpack_from("<II", entry, 8)
        payload = ico[image_offset:image_offset + size]
        icon_id = index + 1
        if fault == "substituted" and index == 0:
            payload = b"substituted image resource"
        icon_resources[icon_id] = {0: payload}
        reference_id = 255 if fault == "misreferenced" and index == 0 else icon_id
        group.extend(entry[:12] + struct.pack("<H", reference_id))
    resources: dict[int, dict[int, dict[int, bytes]]] = {3: icon_resources}
    if fault != "missing_group":
        resources[14] = {1: {0: bytes(group)}}
    return _pe_with_resources(resources)


@pytest.mark.parametrize("app", release.PE_ICON_EMBEDDED_APPS)
@pytest.mark.parametrize(
    ("fault", "error"),
    [
        (None, None),
        ("missing_group", "group resource"),
        ("misreferenced", "referenced"),
        ("substituted", "substituted"),
    ],
)
def test_application_pe_icon_validator_checks_group_reference_and_payload(
    tmp_path: Path,
    app: str,
    fault: str | None,
    error: str | None,
) -> None:
    executable = tmp_path / f"loom-{app}.exe"
    executable.write_bytes(_application_pe_icon_fixture(app, fault))
    if error is None:
        validate.validate_application_pe_icon(executable, app)
    else:
        with pytest.raises(RuntimeError, match=error):
            validate.validate_application_pe_icon(executable, app)


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
