# License policy

## Project license

All original Loom Vision code is dual-licensed under:

- MIT
- Apache-2.0

This matches the Loom suite policy. License files will be added to each
crate before first publication; the workspace manifests already declare
`license = "MIT OR Apache-2.0"`.

## Direct dependency licenses (verified at 0.1.0)

| Crate | Version | License | Notes |
|---|---|---|---|
| serde / serde_derive | 1.0.229 | MIT OR Apache-2.0 | derive feature |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | SHA-256 |
| rqrr | 0.10.1 | (MIT OR Apache-2.0) AND ISC | QR decode; dual components |
| image | 0.25.10 | MIT OR Apache-2.0 | CLI only |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | dev/example only |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | dev only |

All direct dependencies are permissively licensed; nothing forces a
copyleft license on the workspace. Transitive dependencies are audited via
`cargo deny`-style review before release (the full transitive tree is in
`cargo tree` output; the largest subtree is the `image` crate's optional
AVIF/EXR machinery, all permissive).

## Rules

- **Never add** a dependency whose license is incompatible with MIT OR
  Apache-2.0 without isolating it behind a feature flag and documenting it.
- rqrr contains ISC-licensed code components: ISC is a permissive license
  compatible with our dual licensing; see
  [ADR-0001](docs/adrs/ADR-0001-qr-reference-provider.md).
- Model packs are user content: their `license` field is metadata only.
  Loom Vision ships no model files, so no model license questions apply to
  this repository itself.
- No proprietary assets (fonts, icons, sounds) are used.
