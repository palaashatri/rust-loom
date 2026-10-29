"""Check that Writer's Windows executable embeds its canonical application icon."""

from __future__ import annotations

import struct
from pathlib import Path

import release


def _u16(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 2 > len(data):
        raise RuntimeError("application PE icon resource is truncated")
    return struct.unpack_from("<H", data, offset)[0]


def _u32(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 4 > len(data):
        raise RuntimeError("application PE icon resource is truncated")
    return struct.unpack_from("<I", data, offset)[0]


def _layout(data: bytes) -> tuple[list[tuple[int, int, int]], int, int]:
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise RuntimeError("application PE icon executable has no DOS header")
    pe_offset = _u32(data, 0x3C)
    if data[pe_offset : pe_offset + 4] != b"PE\x00\x00":
        raise RuntimeError("application PE icon executable has no PE signature")
    coff = pe_offset + 4
    section_count = _u16(data, coff + 2)
    optional_size = _u16(data, coff + 16)
    optional = coff + 20
    if optional + optional_size > len(data):
        raise RuntimeError("application PE icon optional header is truncated")
    magic = _u16(data, optional)
    if magic == 0x10B:
        directory_count_offset, directory_offset = 92, 96
    elif magic == 0x20B:
        directory_count_offset, directory_offset = 108, 112
    else:
        raise RuntimeError("application PE icon executable has an unknown optional header")
    if optional_size < directory_offset + 3 * 8:
        raise RuntimeError("application PE icon data directories are truncated")
    if _u32(data, optional + directory_count_offset) <= 2:
        raise RuntimeError("application PE icon resource directory is missing")
    resource_rva, resource_size = struct.unpack_from(
        "<II", data, optional + directory_offset + 2 * 8
    )
    if resource_rva == 0 or resource_size == 0:
        raise RuntimeError("application PE icon resource directory is empty")
    sections_offset = optional + optional_size
    if sections_offset + section_count * 40 > len(data):
        raise RuntimeError("application PE icon section table is truncated")
    sections = []
    for index in range(section_count):
        section = sections_offset + index * 40
        virtual_address, raw_size, raw_offset = struct.unpack_from(
            "<III", data, section + 12
        )
        sections.append((virtual_address, raw_size, raw_offset))
    resource_offset = _rva_offset(data, sections, resource_rva, resource_size)
    return sections, resource_offset, resource_size


def _rva_offset(
    data: bytes,
    sections: list[tuple[int, int, int]],
    rva: int,
    size: int,
) -> int:
    for virtual_address, raw_size, raw_offset in sections:
        delta = rva - virtual_address
        offset = raw_offset + delta
        if 0 <= delta and delta + size <= raw_size and offset + size <= len(data):
            return offset
    raise RuntimeError("application PE icon RVA is not backed by file data")


def _directory_entries(
    data: bytes, resource_base: int, resource_size: int, relative_offset: int
) -> list[tuple[int | None, bool, int]]:
    directory_offset = resource_base + relative_offset
    if relative_offset < 0 or relative_offset + 16 > resource_size:
        raise RuntimeError("application PE icon resource directory is out of bounds")
    named_count, id_count = struct.unpack_from("<HH", data, directory_offset + 12)
    count = named_count + id_count
    entries_offset = directory_offset + 16
    if count > 4096 or entries_offset + count * 8 > resource_base + resource_size:
        raise RuntimeError("application PE icon resource entries are out of bounds")
    entries = []
    for index in range(count):
        name, target = struct.unpack_from("<II", data, entries_offset + index * 8)
        identifier = None if name & 0x80000000 else name
        entries.append((identifier, bool(target & 0x80000000), target & 0x7FFFFFFF))
    return entries


def _resource_payloads(
    data: bytes,
    sections: list[tuple[int, int, int]],
    resource_base: int,
    resource_size: int,
    resource_type: int,
) -> dict[int, set[bytes]]:
    type_entry = next(
        (
            entry
            for entry in _directory_entries(data, resource_base, resource_size, 0)
            if entry[0] == resource_type
        ),
        None,
    )
    if type_entry is None:
        return {}
    if not type_entry[1]:
        raise RuntimeError("application PE icon resource type is not a directory")
    payloads: dict[int, set[bytes]] = {}
    for name_id, name_is_directory, name_offset in _directory_entries(
        data, resource_base, resource_size, type_entry[2]
    ):
        if name_id is None:
            continue
        if not name_is_directory:
            raise RuntimeError("application PE icon resource name is not a directory")
        for _language_id, language_is_directory, language_offset in _directory_entries(
            data, resource_base, resource_size, name_offset
        ):
            if language_is_directory:
                raise RuntimeError("application PE icon resource tree exceeds three levels")
            if language_offset + 16 > resource_size:
                raise RuntimeError(
                    "application PE icon resource data entry is out of bounds"
                )
            data_entry = resource_base + language_offset
            payload_rva, payload_size = struct.unpack_from("<II", data, data_entry)
            payload_offset = _rva_offset(data, sections, payload_rva, payload_size)
            payloads.setdefault(name_id, set()).add(
                data[payload_offset : payload_offset + payload_size]
            )
    return payloads


def validate_application_pe_icon(path: Path, app: str) -> None:
    """Require an app's PE group icon to reference every canonical ICO frame."""
    data = path.read_bytes()
    sections, resource_base, resource_size = _layout(data)
    root = Path(release.__file__).resolve().parents[2]
    icon = release.icon_path(root, app, "ico").read_bytes()
    if len(icon) < 6:
        raise RuntimeError("canonical application ICO directory is truncated")
    reserved, icon_type, count = struct.unpack_from("<HHH", icon)
    if reserved != 0 or icon_type != 1 or count == 0:
        raise RuntimeError("canonical application ICO directory is invalid")
    if 6 + count * 16 > len(icon):
        raise RuntimeError("canonical application ICO entries are truncated")

    groups = _resource_payloads(data, sections, resource_base, resource_size, 14).get(
        1, set()
    )
    if len(groups) != 1:
        raise RuntimeError("application PE icon group resource 1 is missing or ambiguous")
    group = next(iter(groups))
    if group[:6] != icon[:6] or len(group) != 6 + count * 14:
        raise RuntimeError("application PE icon group differs from canonical ICO directory")

    expected_frames = []
    for index in range(count):
        ico_entry = 6 + index * 16
        image_size, image_offset = struct.unpack_from("<II", icon, ico_entry + 8)
        if image_offset + image_size > len(icon):
            raise RuntimeError("canonical application ICO frame extends beyond its payload")
        expected_frames.append(
            (
                icon[ico_entry : ico_entry + 8],
                image_size,
                icon[image_offset : image_offset + image_size],
            )
        )

    images = _resource_payloads(data, sections, resource_base, resource_size, 3)
    for index in range(count):
        group_entry = 6 + index * 14
        frame_metadata = group[group_entry : group_entry + 8]
        frame_size = _u32(group, group_entry + 8)
        icon_id = _u16(group, group_entry + 12)
        payloads = images.get(icon_id, set())
        if len(payloads) != 1:
            raise RuntimeError(
                f"application PE icon frame {index} is missing or ambiguously referenced"
            )
        referenced_frame = (frame_metadata, frame_size, next(iter(payloads)))
        try:
            expected_frames.remove(referenced_frame)
        except ValueError as exc:
            raise RuntimeError(
                f"application PE icon frame {index} is substituted or misreferenced"
            ) from exc
    if expected_frames:
        raise RuntimeError("application PE icon group omits canonical ICO frames")
