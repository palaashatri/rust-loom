# Loom Photo, Motion, Video, Studio and Encode icons

The five original generated PNG images approved by the owner are the **canonical artwork**.
All five original sources are committed without alteration as `loom-<app>.source.png`
(1254×1254 RGBA). The SHA-256 checksums in `approved-source-sha256.json` prove byte identity
with the approved image-generation outputs.

| App | Artwork | Canonical master |
| --- | --- | --- |
| Photo | Frosted photo panes on warm coral | `loom-photo.source.png` |
| Motion | Interlaced pearl orbital ribbons on indigo | `loom-motion.source.png` |
| Video | Pearl filmstrip on teal | `loom-video.source.png` |
| Studio | Three ceramic audio faders on plum | `loom-studio.source.png` |
| Encode | Porcelain conversion arrows on copper | `loom-encode.source.png` |

The PNG source masters, the 1024px/512px PNG derivatives, the Windows ICO files,
and the macOS ICNS files are included in the repository. The 1024px and smaller
exports are resampled versions of the original pixels; they are not identical-size
copies. The old approximation SVGs have been removed.

To rebuild and validate from the approved master images:

```bash
python -m pip install pillow
python loom-bootstrap/scripts/generate-app-icons.py
python loom-bootstrap/scripts/generate-app-icons.py --check
python loom-bootstrap/scripts/audit-assets.py
```

**Do not redraw the symbols** or replace the original masters with vectors.
Artwork was generated and approved on October 29, 2026. Its provenance is recorded
in the SHA-256 manifest and the repository asset policy. Original work, project
license MIT OR Apache-2.0.
