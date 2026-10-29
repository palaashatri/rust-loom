# Loom creative-suite application icons

Five original, self-contained SVG application icons for the `apps/later` branch.

## Visual design

These icons were redesigned on 2026-10-29 following visual review of Loom's
existing Writer, Sheets and Present identity on `main`. The first flat/toolbar
illustrations were rejected. These replacements use the approved direction:
softly lit, dimensional porcelain symbols on refined enamel rounded-square tiles.

| Application | Design | Palette |
| --- | --- | --- |
| Photo | Two offset translucent photograph cards, layered warm landscape | Coral / terracotta |
| Motion | Intersecting ivory orbital ribbons and a pearl sphere | Indigo / periwinkle |
| Video | Slightly angled pearl-framed filmstrip with triangular play symbol | Deep teal / cyan |
| Studio | Three inset audio fader tracks with pearl control knobs | Muted plum / magenta |
| Encode | Two thick looping conversion arrows framing a rounded tile | Cognac / copper |

All five use the same **1024×1024** viewBox, a **844px** rounded tile,
consistent transparent safe area, ivory material, and upper-left light source.
No Apple or third-party icon assets were traced or copied.

The SVG files are vector **redraws of the approved visual direction**, not
embedded/generated PNGs or exact pixel copies of the concept render. They
remain editable and reproducible in the repository.

## Generate native platform formats

```sh
python -m pip install cairosvg pillow
python loom-bootstrap/scripts/generate-app-icons.py
python loom-bootstrap/scripts/generate-app-icons.py --check
python loom-bootstrap/scripts/audit-assets.py
```

The exporter writes `loom-<app>.png` at 1024px,
`loom-<app>.512.png` at 512px, multi-resolution Windows `.ico`,
and macOS `.icns` into this directory. SVG sources are tracked in this PR;
generated raster/native files are not tracked here. Generate them before
running package builds that consume the new apps' icon assets.

## Provenance

- Created: 2026-10-29
- Origin: original Loom artwork, with the accepted AI-generated visual study used
  as a design reference; icons vector-redrawn for Loom
- License: MIT OR Apache-2.0 (the repository project license)
- Export tooling: CairoSVG and Pillow
- Asset audit policy: `loom-bootstrap/contracts/assets.toml`
