# Dependencies

## Direct

| Crate | Version | Purpose | License |
| --- | --- | --- | --- |
| serde | 1.0.229 (locked) | schema derive | MIT OR Apache-2.0 |
| serde_json | 1.0.151 (locked) | manifest + index JSON | MIT OR Apache-2.0 |
| sha2 | 0.10.x | manifest_sha256 | MIT OR Apache-2.0 |
| zip | 0.6.6 (locked) | `.loomplugin` read/write | MIT |

## Feature trimming

`zip` is built with `default-features = false, features = ["deflate"]`,
removing aes-crypto/bzip2/zstd/time deps — the format used by Loom packages
is store/deflate only, and the host reads user-supplied zips with an
explicit allowlist of compression methods that the runtime milestone will
extend only with justification.

## Replacement strategy

- `zip`: active project (github.com/zip-rs/zip2); if unmaintained, a
  maintainer fork or hand-rolled zip reader (store/deflate only) is a viable
  fallback because our reader surface is small (name/size/mode + stream).
- `serde`/`serde_json`: ubiquitous; replacement only if licensing changes.
- `sha2`: RustCrypto, maintained; any CRC/SHA impl would do — the API is
  isolated in `install_zip`.

## Audit

`Cargo.lock` is committed. `cargo tree -d` shows no duplicate major
versions. A `cargo deny` workflow will be adopted with loom-core; until
then, dependency additions are reviewed per `CONTRIBUTING.md`.
