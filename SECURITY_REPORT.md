# Loom Security Report

Status: initial posture for the current stage (two functional apps, shared platform).

## In place

- Archive safety: loom-package enforces extraction limits (count/size/depth), checksums
  for entries, and path traversal protection; fuzzable parser targets defined.
- Storage: safe-path validation (`is_safe_storage_path` rejects absolute and `..` paths),
  atomic writes via temp file + rename + dir sync.
- Logging: local diagnostic logs with redaction policy documented; no telemetry, no
  crash upload, no analytics, no network requests in any core workflow.
- Offline: verified working with `--network none` in a container (docker-offline-test.sh).
- Model packs: manifest validation with checksums and provenance fields; no auto-download.
- Clipboard/import inputs validated in core parsers (tests green).

## Policy

- `unsafe` Rust: none in application code; FFI-free so far. Any future `unsafe` requires a
  safety comment, tests, encapsulation, and Miri where applicable.
- No remote model inference; no mandatory update checks; no cloud architecture.
- Plugins (future WASM sandbox): declared capabilities, resource limits, signing without
  a central authority — architecture defined in loom-plugin-sdk, sandbox NOT yet implemented.

## Not yet done

- Automated vulnerability scanning (cargo audit) in CI — tooling not installed.
- Fuzzing runs in CI — targets defined, harness not automated.
- Untrusted-font handling strategy for the GUI text stack (uses system fonts).
- Secret scanning in the archive pipeline (none present; verified by exclusion rules).

## Honest status

The design eliminates the main web/cloud attack surface by construction (no network
surface, no embedded scripts). Remaining work is tooling (audit/fuzz) and the plugin
sandbox; neither blocks current offline desktop functionality.
