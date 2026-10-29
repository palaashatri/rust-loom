# Loom Photo, Motion, Video, Studio and Encode icons

Five original vector application icons for the `apps/later` branch. Each uses a rounded enamel tile with a transparent safe area, an ivory dimensional mark, restrained upper-left illumination, and an app-specific color.

| App | Symbol | Color |
| --- | --- | --- |
| Photo | Stacked photographs and landscape | Terracotta |
| Motion | Orbiting arrows and central keyframe | Violet |
| Video | Filmstrip and playback triangle | Deep teal |
| Studio | Six-channel audio mixer | Magenta |
| Encode | Dual transcode arrows around a video frame | Copper |

The `loom-*.svg` files are the editable source of truth. To export the platform formats:

```sh
python -m pip install cairosvg pillow
python loom-bootstrap/scripts/generate-app-icons.py
python loom-bootstrap/scripts/generate-app-icons.py --check
python loom-bootstrap/scripts/audit-assets.py
```

The generation script emits 1024x1024 `.png`, 512x512 `.512.png`, multi-resolution Windows `.ico`, and macOS `.icns` files in this directory.

These icons complement the pre-existing Writer, Sheets, and Present family on `main`. They are original artwork and do not reproduce icons from other commercial suites.

## Provenance

Designed October 29, 2026. Authored directly as original SVG vectors for Loom. Rasterization uses CairoSVG and icon exports use Pillow. Artwork is under the repository's MIT OR Apache-2.0 license. All five sources retain transparent corners and no text or external artwork.
