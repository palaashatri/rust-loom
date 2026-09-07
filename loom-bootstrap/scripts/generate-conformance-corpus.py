#!/usr/bin/env python3
"""Generate the deterministic Loom interoperability conformance corpus.

The corpus is a golden fixture set stored in `loom-samples/conformance/`. Every
entry is a valid, minimal representation of its documented format:

* `.docx` — OOXML Word package with `word/document.xml`
* `.xlsx` — OOXML Excel package with `xl/workbook.xml`
* `.pptx` — OOXML PowerPoint package with `ppt/presentation.xml`
* `.odt`, `.ods`, `.odp` — OpenDocument packages with a stored `mimetype`
* `.psd`   — Photoshop document with the `8BPS` header and image data
* CSV / TSV / Markdown / plain-text reference documents

All archives use fixed timestamps so the bytes are reproducible. ODF bundles
store the `mimetype` member first and uncompressed as the specification
requires.

Run with:  python3 loom-bootstrap/scripts/generate-conformance-corpus.py
"""
from __future__ import annotations

import struct
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "loom-samples" / "conformance"

ODF = {
    "odt": "application/vnd.oasis.opendocument.text",
    "ods": "application/vnd.oasis.opendocument.spreadsheet",
    "odp": "application/vnd.oasis.opendocument.presentation",
}


def fixed_info(name: str, compress: bool) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED if compress else zipfile.ZIP_STORED
    info.external_attr = 0o644 << 16
    return info


def _entry(name: str, compress: bool) -> zipfile.ZipInfo:
    return fixed_info(name, compress)


def write_docx(name: str) -> None:
    parts = {
        "[Content_Types].xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>""",
        "_rels/.rels": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>""",
        "word/document.xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>Loom conformance document</w:t></w:r></w:p></w:body>
</w:document>""",
    }
    path = OUT / "docx" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as zf:
        for member, content in sorted(parts.items()):
            zf.writestr(_entry(member, True), content)


def write_xlsx(name: str) -> None:
    parts = {
        "[Content_Types].xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
</Types>""",
        "_rels/.rels": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>""",
        "xl/workbook.xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets>
</workbook>""",
        "xl/_rels/workbook.xml.rels": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>""",
        "xl/worksheets/sheet1.xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Loom</t></is></c></row></sheetData>
</worksheet>""",
    }
    path = OUT / "xlsx" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as zf:
        for member, content in sorted(parts.items()):
            zf.writestr(_entry(member, True), content)


def write_pptx(name: str) -> None:
    parts = {
        "[Content_Types].xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
</Types>""",
        "_rels/.rels": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>""",
        "ppt/presentation.xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldIdLst><p:sldId id="256" r:id="rId2" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></p:sldIdLst>
</p:presentation>""",
        "ppt/_rels/presentation.xml.rels": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>""",
        "ppt/slides/slide1.xml": b"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/></p:nvGrpSpPr></p:spTree></p:cSld>
</p:sld>""",
    }
    path = OUT / "pptx" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as zf:
        for member, content in sorted(parts.items()):
            zf.writestr(_entry(member, True), content)


def write_opendocument(name: str, mime: str, body: bytes) -> None:
    manifest = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">\n'
        f'<manifest:file-entry manifest:full-path="/" manifest:media-type="{mime}"/>\n'
        '<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>\n'
        "</manifest:manifest>\n"
    ).encode("ascii")
    parts = [
        ("mimetype", mime.encode("ascii"), False),
        ("META-INF/manifest.xml", manifest, True),
        ("content.xml", body, True),
    ]
    path = OUT / name.split(".")[-1] / name
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as zf:
        for member, content, compress in parts:
            zf.writestr(_entry(member, compress), content)


def write_psd() -> None:
    body = bytearray()
    body += b"8BPS"
    body += struct.pack(">H", 1)
    body += b"\x00" * 6
    body += struct.pack(">H", 1)
    body += struct.pack(">II", 1, 1)
    body += struct.pack(">HH", 8, 1)
    body += struct.pack(">I", 0)
    body += struct.pack(">I", 0)
    body += struct.pack(">I", 0)
    body += struct.pack(">H", 0)
    body += b"\x7f"
    path = OUT / "psd" / "one_pixel.psd"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(body))


def write_text_fixtures() -> None:
    for sub, name, payload in (
        ("csv", "accounts.csv", b"Item,Amount,Kind\nDesign,8000,chart\nTotal,=SUM(B2:B3),formula\n"),
        ("tsv", "measurements.tsv", b"Run\tMean\tSigma\n1\t1.02\t0.01\n2\t0.98\t0.02\n"),
        ("markdown", "notes.md", b"# Conformance\n\nA Markdown corpus document with a [link](https://example.test/x).\n"),
        ("plaintext", "catalog.txt", b"first line\nsecond line\nthird line\n"),
    ):
        directory = OUT / sub
        directory.mkdir(parents=True, exist_ok=True)
        (directory / name).write_bytes(payload)


def main() -> int:
    if not OUT.parent.is_dir():
        print(f"loom-samples directory missing: {OUT.parent}", file=__import__("sys").stderr)
        return 1
    write_docx("minimal.docx")
    write_xlsx("minimal.xlsx")
    write_pptx("minimal.pptx")
    for extension, mime in ODF.items():
        write_opendocument(
            f"minimal.{extension}",
            mime,
            b"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text><text:p>Loom conformance corpus</text:p></office:text></office:body>
</office:document-content>""",
        )
    write_psd()
    write_text_fixtures()
    generated = sorted(str(p.relative_to(OUT)) for p in OUT.rglob("*") if p.is_file())
    print(f"conformance corpus: {len(generated)} fixtures in {OUT}")
    for name in generated:
        print(f"  {name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())