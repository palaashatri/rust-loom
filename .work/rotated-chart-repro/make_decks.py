import hashlib
import json
import pathlib
import sys
import zipfile

out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)

chart = {
    "id": "chart-repro",
    "title": "Quarterly net revenue",
    "chartType": "column",
    "widthPt": 620.0,
    "heightPt": 340.0,
    "series": [{"name": "Revenue", "values": [12.0, 24.0, 16.0, 31.0]}],
    "categories": ["First quarter", "Second quarter", "Third quarter", "Fourth quarter"],
    "categoryAxis": {"gridlines": True},
    "valueAxis": {"gridlines": True},
    "style": {"paletteId": "Office", "showLegend": True, "showDataLabels": False},
}
for name, rotation in (("upright", 0.0), ("rotated", 30.0)):
    presentation = {
        "id": f"deck-{name}",
        "title": "Rotated chart text repro",
        "author": "",
        "theme": "",
        "slides": [{
            "id": "slide-repro",
            "title": "Chart repro",
            "layout": "blank",
            "elements": [{
                "id": "chart-object",
                "element_type": "Chart",
                "content": json.dumps(chart, separators=(",", ":")),
                "x": 180.0,
                "y": 120.0,
                "width": 620.0,
                "height": 340.0,
                "rotation_deg": rotation,
            }],
            "speaker_notes": "",
            "bg_color": "#FFFFFF",
        }],
        "active_index": 0,
    }
    content = json.dumps(presentation, indent=2).encode()
    manifest = {
        "schema": "0.1.0",
        "kind": "present",
        "id": presentation["id"],
        "title": presentation["title"],
        "appVersion": "0.1.0",
        "entries": [{
            "path": "content/presentation.json",
            "mime": "application/vnd.loom.deck-content",
            "size": str(len(content)),
            "sha256": hashlib.sha256(content).hexdigest(),
        }],
    }
    path = out / f"{name}.loomdeck"
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as package:
        package.writestr("content/presentation.json", content)
        package.writestr("manifest.json", json.dumps(manifest, separators=(",", ":")))
    print(path)
