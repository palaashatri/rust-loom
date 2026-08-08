#!/usr/bin/env python3
"""Build native Loom suite installers from already-built release binaries.

The script never labels an archive as a native installer. Linux emits a real
Debian package and, when appimagetool is installed, one AppImage per app.
Windows emits a WiX MSI. macOS emits code-signable .app bundles and a DMG.
Production signing is mandatory unless --allow-unsigned is explicitly passed
for development validation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable, NoReturn

APPS = ("writer", "sheets", "present", "photo", "motion", "video", "studio", "encode")
DISPLAY_NAMES = {
    "writer": "Loom Writer",
    "sheets": "Loom Sheets",
    "present": "Loom Present",
    "photo": "Loom Photo",
    "motion": "Loom Motion",
    "video": "Loom Video",
    "studio": "Loom Studio",
    "encode": "Loom Encode",
}
DOCUMENT_TYPES = {
    "writer": ("loomdoc", "application/x-loom-writer", "Loom Writer Document"),
    "sheets": ("loomtable", "application/x-loom-sheets", "Loom Sheets Workbook"),
    "present": ("loomdeck", "application/x-loom-present", "Loom Present Deck"),
    "photo": ("loomphoto", "application/x-loom-photo", "Loom Photo Project"),
    "motion": ("loommotion", "application/x-loom-motion", "Loom Motion Composition"),
    "video": ("loomvideo", "application/x-loom-video", "Loom Video Project"),
    "studio": ("loomstudio", "application/x-loom-studio", "Loom Studio Project"),
    "encode": ("loomencode", "application/x-loom-encode", "Loom Encode Queue"),
}
APP_CATEGORIES = {
    "writer": "public.app-category.productivity",
    "sheets": "public.app-category.productivity",
    "present": "public.app-category.productivity",
    "photo": "public.app-category.graphics-design",
    "motion": "public.app-category.video",
    "video": "public.app-category.video",
    "studio": "public.app-category.music",
    "encode": "public.app-category.video",
}


@dataclass(frozen=True)
class Artifact:
    """One generated release artifact."""

    path: str
    sha256: str
    bytes: int
    platform: str
    architecture: str
    signed: bool
    kind: str


def fail(message: str) -> NoReturn:
    raise SystemExit(f"release error: {message}")


def run(arguments: list[str], *, cwd: Path | None = None) -> None:
    process = subprocess.run(arguments, cwd=cwd, check=False)
    if process.returncode != 0:
        fail(f"command failed ({process.returncode}): {' '.join(arguments)}")


def run_with_retries(
    arguments: list[str],
    *,
    cwd: Path | None = None,
    attempts: int = 3,
    delay_seconds: float = 2.0,
) -> None:
    """Run a native packaging tool with bounded retries for transient OS errors."""
    if attempts < 1:
        fail("retry attempt count must be positive")
    last_code = 0
    for attempt in range(1, attempts + 1):
        process = subprocess.run(arguments, cwd=cwd, check=False)
        last_code = process.returncode
        if last_code == 0:
            return
        if attempt < attempts:
            time.sleep(delay_seconds * attempt)
    fail(
        f"command failed ({last_code}) after {attempts} attempts: "
        f"{' '.join(arguments)}"
    )


def require_program(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        fail(f"required program is not installed: {name}")
    return path


def binary_path(root: Path, app: str, platform: str) -> Path:
    suffix = ".exe" if platform == "windows" else ""
    candidate = root / f"loom-{app}" / "target" / "release" / f"loom-{app}{suffix}"
    if not candidate.is_file():
        fail(f"missing release binary: {candidate}")
    return candidate.resolve()


def collect_binaries(root: Path, platform: str) -> dict[str, Path]:
    return {app: binary_path(root, app, platform) for app in APPS}


def validate_version(version: str) -> str:
    parts = version.split(".")
    if len(parts) != 3 or any(not part.isdigit() for part in parts):
        fail("version must be numeric MAJOR.MINOR.PATCH")
    if any(int(part) > 65535 for part in parts):
        fail("version component exceeds native installer limits")
    return version


def validate_architecture(platform: str, architecture: str) -> None:
    supported = {
        "linux": {"x86_64", "aarch64"},
        "windows": {"x86_64", "aarch64"},
        "macos": {"x86_64", "aarch64", "universal2"},
    }
    if platform not in supported or architecture not in supported[platform]:
        fail(f"unsupported target: {platform}/{architecture}")


def write_text(path: Path, text: str, mode: int | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")
    if mode is not None:
        path.chmod(mode)


def loom_svg(app: str) -> str:
    initial = app[0].upper()
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#26282d"/><stop offset="1" stop-color="#111216"/></linearGradient></defs>
<rect width="512" height="512" rx="112" fill="url(#g)"/>
<path d="M128 356V156h48v154h208v46z" fill="#d59154"/>
<text x="256" y="292" text-anchor="middle" font-family="sans-serif" font-size="156" font-weight="700" fill="#f4f1ec">{initial}</text>
</svg>'''


def linux_mime_xml() -> str:
    entries = []
    for _app, (extension, mime, description) in DOCUMENT_TYPES.items():
        entries.append(
            f'''  <mime-type type="{mime}">\n'''
            f'''    <comment>{description}</comment>\n'''
            f'''    <glob pattern="*.{extension}"/>\n'''
            f'''  </mime-type>'''
        )
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">\n'
        + "\n".join(entries)
        + "\n</mime-info>\n"
    )


def macos_document_type(app: str) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    extension, mime, description = DOCUMENT_TYPES[app]
    identifier = f"org.loom.{app}-document"
    document_types = [
        {
            "CFBundleTypeName": description,
            "CFBundleTypeRole": "Editor",
            "LSHandlerRank": "Owner",
            "LSItemContentTypes": [identifier],
            "CFBundleTypeExtensions": [extension],
            "CFBundleTypeMIMETypes": [mime],
        }
    ]
    declarations = [
        {
            "UTTypeIdentifier": identifier,
            "UTTypeDescription": description,
            "UTTypeConformsTo": ["public.data"],
            "UTTypeTagSpecification": {
                "public.filename-extension": [extension],
                "public.mime-type": [mime],
            },
        }
    ]
    return document_types, declarations


def artifact(path: Path, platform: str, architecture: str, signed: bool, kind: str) -> Artifact:
    payload = path.read_bytes()
    return Artifact(
        path=str(path.resolve()),
        sha256=hashlib.sha256(payload).hexdigest(),
        bytes=len(payload),
        platform=platform,
        architecture=architecture,
        signed=signed,
        kind=kind,
    )


def package_linux(
    root: Path,
    output: Path,
    version: str,
    architecture: str,
    allow_unsigned: bool,
) -> list[Artifact]:
    del allow_unsigned
    binaries = collect_binaries(root, "linux")
    dpkg_deb = require_program("dpkg-deb")
    output.mkdir(parents=True, exist_ok=True)
    artifacts: list[Artifact] = []
    with tempfile.TemporaryDirectory(prefix="loom-deb-") as temporary:
        package_root = Path(temporary) / "loom-creator-suite"
        bin_dir = package_root / "usr" / "bin"
        applications = package_root / "usr" / "share" / "applications"
        icons = package_root / "usr" / "share" / "icons" / "hicolor" / "scalable" / "apps"
        mime_packages = package_root / "usr" / "share" / "mime" / "packages"
        bin_dir.mkdir(parents=True)
        applications.mkdir(parents=True)
        icons.mkdir(parents=True)
        mime_packages.mkdir(parents=True)
        installed_size = 0
        for app, source in binaries.items():
            destination = bin_dir / f"loom-{app}"
            shutil.copy2(source, destination)
            destination.chmod(0o755)
            installed_size += destination.stat().st_size
            write_text(icons / f"loom-{app}.svg", loom_svg(app))
            write_text(
                applications / f"loom-{app}.desktop",
                "\n".join(
                    [
                        "[Desktop Entry]",
                        "Type=Application",
                        f"Name={DISPLAY_NAMES[app]}",
                        f"Exec=loom-{app} %F",
                        f"Icon=loom-{app}",
                        f"MimeType={DOCUMENT_TYPES[app][1]};",
                        "Terminal=false",
                        "Categories=Graphics;AudioVideo;Office;",
                        "StartupNotify=true",
                        "",
                    ]
                ),
            )
        write_text(mime_packages / "loom.xml", linux_mime_xml())
        architecture_name = {"x86_64": "amd64", "aarch64": "arm64"}[architecture]
        write_text(
            package_root / "DEBIAN" / "control",
            "\n".join(
                [
                    "Package: loom-creator-suite",
                    f"Version: {version}",
                    "Section: graphics",
                    "Priority: optional",
                    f"Architecture: {architecture_name}",
                    "Maintainer: Loom Project",
                    "Depends: libasound2, libfontconfig1, libx11-6, libxkbcommon0, libwayland-client0, libgl1",
                    f"Installed-Size: {(installed_size + 1023) // 1024}",
                    "Description: Local-first professional creative suite",
                    " Loom Writer, Sheets, Present, Photo, Motion, Video, Studio, and Encode.",
                    "",
                ]
            ),
        )
        deb = output / f"loom-creator-suite_{version}_{architecture_name}.deb"
        run([dpkg_deb, "--root-owner-group", "--build", str(package_root), str(deb)])
        artifacts.append(artifact(deb, "linux", architecture, False, "deb"))

    appimagetool = shutil.which("appimagetool")
    if appimagetool:
        for app, source in binaries.items():
            with tempfile.TemporaryDirectory(prefix=f"loom-{app}-appdir-") as temporary:
                appdir = Path(temporary) / f"Loom-{app}.AppDir"
                usr_bin = appdir / "usr" / "bin"
                usr_bin.mkdir(parents=True)
                shutil.copy2(source, usr_bin / f"loom-{app}")
                (usr_bin / f"loom-{app}").chmod(0o755)
                write_text(
                    appdir / "AppRun",
                    f'#!/bin/sh\nHERE="$(dirname "$(readlink -f "$0")")"\nexec "$HERE/usr/bin/loom-{app}" "$@"\n',
                    0o755,
                )
                write_text(appdir / f"loom-{app}.svg", loom_svg(app))
                write_text(
                    appdir / f"loom-{app}.desktop",
                    "\n".join(
                        [
                            "[Desktop Entry]",
                            "Type=Application",
                            f"Name={DISPLAY_NAMES[app]}",
                            f"Exec=loom-{app}",
                            f"Icon=loom-{app}",
                            f"MimeType={DOCUMENT_TYPES[app][1]};",
                            "Terminal=false",
                            "Categories=Graphics;AudioVideo;Office;",
                            "",
                        ]
                    ),
                )
                appimage = output / f"Loom-{app}-{version}-{architecture}.AppImage"
                environment = os.environ.copy()
                environment["ARCH"] = architecture
                process = subprocess.run(
                    [appimagetool, str(appdir), str(appimage)],
                    env=environment,
                    check=False,
                )
                if process.returncode != 0:
                    fail(f"appimagetool failed for {app}")
                artifacts.append(artifact(appimage, "linux", architecture, False, "appimage"))
    return artifacts


def wix_source(binaries: dict[str, Path], version: str, architecture: str) -> str:
    del architecture  # Architecture is selected by `wix build -arch`.
    components: list[str] = []
    refs: list[str] = []
    for app, source in binaries.items():
        component_id = f"Component_{app}"
        association_id = f"Association_{app}"
        file_id = f"File_{app}"
        extension, _mime, description = DOCUMENT_TYPES[app]
        display = DISPLAY_NAMES[app]
        components.append(
            f'''<Component Id="{component_id}" Guid="*">
              <File Id="{file_id}" Source="{xml_escape(str(source))}" KeyPath="yes" />
              <Shortcut Id="Shortcut_{app}" Directory="ApplicationProgramsFolder"
                        Name="{xml_escape(display)}" Target="[INSTALLFOLDER]loom-{app}.exe"
                        WorkingDirectory="INSTALLFOLDER" />
            </Component>
            <Component Id="{association_id}" Guid="*">
              <RegistryValue Root="HKCR" Key=".{extension}" Value="Loom.{app}" Type="string" KeyPath="yes" />
              <RegistryValue Root="HKCR" Key="Loom.{app}" Value="{xml_escape(description)}" Type="string" />
              <RegistryValue Root="HKCR" Key="Loom.{app}\\shell\\open\\command"
                             Value="&quot;[INSTALLFOLDER]loom-{app}.exe&quot; &quot;%1&quot;" Type="string" />
            </Component>'''
        )
        refs.append(f'<ComponentRef Id="{component_id}" />')
        refs.append(f'<ComponentRef Id="{association_id}" />')
    upgrade_code = str(uuid.uuid5(uuid.NAMESPACE_URL, "https://loom.local/creator-suite")).upper()
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="Loom Creator Suite" Manufacturer="Loom Project" Version="{version}"
           UpgradeCode="{upgrade_code}" Scope="perMachine" Compressed="yes">
    <MajorUpgrade DowngradeErrorMessage="A newer version of Loom Creator Suite is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <StandardDirectory Id="ProgramFiles6432Folder">
      <Directory Id="INSTALLFOLDER" Name="Loom Creator Suite">
        {''.join(components)}
      </Directory>
    </StandardDirectory>
    <StandardDirectory Id="ProgramMenuFolder">
      <Directory Id="ApplicationProgramsFolder" Name="Loom Creator Suite" />
    </StandardDirectory>
    <Feature Id="MainFeature" Title="Loom Creator Suite" Level="1">
      {''.join(refs)}
    </Feature>
  </Package>
</Wix>
'''


def xml_escape(value: str) -> str:
    return (
        value.replace("&", "&amp;")
        .replace('"', "&quot;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
    )


def package_windows(
    root: Path,
    output: Path,
    version: str,
    architecture: str,
    allow_unsigned: bool,
) -> list[Artifact]:
    binaries = collect_binaries(root, "windows")
    wix = require_program("wix")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="loom-wix-") as temporary:
        source = Path(temporary) / "loom.wxs"
        write_text(source, wix_source(binaries, version, architecture))
        msi = output / f"Loom-Creator-Suite-{version}-{architecture}.msi"
        wix_arch = "x64" if architecture == "x86_64" else "arm64"
        run([wix, "build", "-arch", wix_arch, "-o", str(msi), str(source)])
    signed = False
    thumbprint = os.environ.get("LOOM_WINDOWS_CERT_SHA1", "").strip()
    if thumbprint:
        signtool = require_program("signtool")
        timestamp = os.environ.get("LOOM_TIMESTAMP_URL", "http://timestamp.digicert.com")
        run(
            [
                signtool,
                "sign",
                "/sha1",
                thumbprint,
                "/fd",
                "SHA256",
                "/tr",
                timestamp,
                "/td",
                "SHA256",
                str(msi),
            ]
        )
        signed = True
    elif not allow_unsigned:
        fail("LOOM_WINDOWS_CERT_SHA1 is required for a production MSI")
    return [artifact(msi, "windows", architecture, signed, "msi")]


def package_macos(
    root: Path,
    output: Path,
    version: str,
    architecture: str,
    allow_unsigned: bool,
) -> list[Artifact]:
    binaries = collect_binaries(root, "macos")
    hdiutil = require_program("hdiutil")
    output.mkdir(parents=True, exist_ok=True)
    identity = os.environ.get("LOOM_MACOS_CODESIGN_IDENTITY", "").strip()
    if not identity and not allow_unsigned:
        fail("LOOM_MACOS_CODESIGN_IDENTITY is required for production macOS packages")
    signed = bool(identity)
    with tempfile.TemporaryDirectory(prefix="loom-dmg-") as temporary:
        volume = Path(temporary) / "Loom Creator Suite"
        volume.mkdir()
        for app, source in binaries.items():
            display = DISPLAY_NAMES[app]
            bundle = volume / f"{display}.app"
            executable_dir = bundle / "Contents" / "MacOS"
            resources = bundle / "Contents" / "Resources"
            executable_dir.mkdir(parents=True)
            resources.mkdir(parents=True)
            executable = executable_dir / f"loom-{app}"
            shutil.copy2(source, executable)
            executable.chmod(0o755)
            document_types, exported_types = macos_document_type(app)
            with (bundle / "Contents" / "Info.plist").open("wb") as handle:
                plistlib.dump(
                    {
                        "CFBundleName": display,
                        "CFBundleDisplayName": display,
                        "CFBundleIdentifier": f"org.loom.{app}",
                        "CFBundleVersion": version,
                        "CFBundleShortVersionString": version,
                        "CFBundleExecutable": f"loom-{app}",
                        "CFBundlePackageType": "APPL",
                        "CFBundleDevelopmentRegion": "en",
                        "CFBundleDocumentTypes": document_types,
                        "UTExportedTypeDeclarations": exported_types,
                        "LSApplicationCategoryType": APP_CATEGORIES[app],
                        "LSMinimumSystemVersion": "13.0",
                        "NSHighResolutionCapable": True,
                    },
                    handle,
                    sort_keys=True,
                )
            if identity:
                run(
                    [
                        "codesign",
                        "--force",
                        "--options",
                        "runtime",
                        "--timestamp",
                        "--sign",
                        identity,
                        str(bundle),
                    ]
                )
        applications_link = volume / "Applications"
        if not applications_link.exists():
            os.symlink("/Applications", applications_link)
        dmg = output / f"Loom-Creator-Suite-{version}-{architecture}.dmg"
        if dmg.exists():
            dmg.unlink()
        run_with_retries(
            [
                hdiutil,
                "create",
                "-volname",
                "Loom Creator Suite",
                "-srcfolder",
                str(volume),
                "-format",
                "UDZO",
                "-ov",
                str(dmg),
            ],
            attempts=3,
            delay_seconds=3.0,
        )
        if identity:
            run(["codesign", "--force", "--timestamp", "--sign", identity, str(dmg)])
    return [artifact(dmg, "macos", architecture, signed, "dmg")]


def write_manifest(output: Path, artifacts: Iterable[Artifact], version: str) -> Path:
    manifest = output / "release-manifest.json"
    payload = {
        "schema_version": 1,
        "suite": "Loom Creator Suite",
        "version": version,
        "commit_sha": os.environ.get("GITHUB_SHA"),
        "artifacts": [asdict(item) for item in artifacts],
    }
    write_text(manifest, json.dumps(payload, indent=2, sort_keys=True) + "\n")
    return manifest


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True, help="rust-loom repository root")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "windows", "macos"), required=True)
    parser.add_argument("--architecture", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--allow-unsigned", action="store_true")
    parser.add_argument("--validate-only", action="store_true")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    root = arguments.root.resolve()
    output = arguments.output.resolve()
    version = validate_version(arguments.version)
    validate_architecture(arguments.platform, arguments.architecture)
    collect_binaries(root, arguments.platform)
    if arguments.validate_only:
        print(
            json.dumps(
                {
                    "valid": True,
                    "platform": arguments.platform,
                    "architecture": arguments.architecture,
                    "version": version,
                    "apps": list(APPS),
                },
                sort_keys=True,
            )
        )
        return 0
    builders = {
        "linux": package_linux,
        "windows": package_windows,
        "macos": package_macos,
    }
    artifacts = builders[arguments.platform](
        root,
        output,
        version,
        arguments.architecture,
        arguments.allow_unsigned,
    )
    manifest = write_manifest(output, artifacts, version)
    print(manifest)
    return 0


if __name__ == "__main__":
    sys.exit(main())
