# Approved Loom creative-suite icons

The five generated PNG images approved by the owner are the **canonical source artwork**.
The earlier hand-redrawn SVG approximations in this branch are superseded and must not be shipped.

| Application | Required master | SHA-256 source of truth |
|---|---|---|
| Photo | `loom-photo.source.png` | `approved-source-sha256.json` |
| Motion | `loom-motion.source.png` | `approved-source-sha256.json` |
| Video | `loom-video.source.png` | `approved-source-sha256.json` |
| Studio | `loom-studio.source.png` | `approved-source-sha256.json` |
| Encode | `loom-encode.source.png` | `approved-source-sha256.json` |

Each approved image is an original 1254×1254 RGBA PNG. Their unchanged byte hashes are recorded in
`approved-source-sha256.json`. No re-creation via SVG, filters, upscaling, or generative rendering
is permitted when substituting a source master.

**The five required binary PNG masters still need to be committed.** GitHub has not received them
yet. Do not merge this PR until they are uploaded and the icon export workflow passes.

After placing the five exact original files beside this README, run:

```sh
python -m pip install pillow
python loom-bootstrap/scripts/generate-app-icons.py
python loom-bootstrap/scripts/generate-app-icons.py --check
```

The script validates the original SHA-256 before creating 1024px PNG, 512px PNG, Windows ICO and
macOS ICNS derivatives. Those derivatives are resampled/encoded from the originals and are **not
pixel-identical** to the originals, whereas the `.source.png` files are.

Original image source: owner-approved Loom icon-generation outputs, 2026-10-29.
Provenance: original artwork, repository MIT OR Apache-2.0 license.
