# Loom — Security Report

## Posture

Local-first by architecture: no mandatory account, no cloud service, no
telemetry by default, no hidden network requests, no remote inference.
Verified by the network-disabled gate: the suite builds and tests with
`--network none` (11/11 workspaces, 2026-08-04).

## Implemented controls

- **Offline operation** — all core workflows (create, edit, save, recover,
  export, encode, search) require no network. Offline gate PASS.
- **Package safety** — Loom package readers validate structure and content;
  the package verification step rejects unsafe archive entries (`../`,
  absolute paths, symlinks) and verifies integrity.
- **Plugin sandboxing** — plugin packages are validated defensively
  (manifest, capabilities, WASM bounds, time/output limits); optional
  Wasmtime execution is isolated. See `loom-plugin-sdk/SECURITY.md`.
- **Model packs** — checksum-verified manifests with provenance and license
  metadata; no implicit downloads.
- **Logging** — local diagnostic logs are understandable and redactable;
  no remote crash upload.
- **Dependency locking** — committed lockfiles, `--locked` builds, offline
  resolution verified.
- **Temporary files** — transactional write patterns with temp-file safety
  per `loom-bootstrap/SECURITY.md` and storage crates.
- **No secrets** — the suite contains no credentials; packaging excludes
  `.work/`, `target/`, and OS junk.

## Audit status (2026-08-04)

- All per-repo `SECURITY.md` documents present.
- Offline gate, package verification, and lockfile checks all PASS.
- No `unsafe` blocks were introduced by this audit's changes; existing
  `unsafe` usage is confined to required FFI boundaries with safety
  comments (see crate docs).

## Not yet complete (honest gaps)

- Fuzzing harnesses for package/import/plugin parsers are specified but not
  yet executed as a full campaign.
- Miri-style UB checks have not been run.
- Dependency vulnerability scanning (e.g. `cargo audit`) has not been
  executed in this audit; supply-chain policy is documented.
- Formal threat-model review is not yet published as a standalone document.