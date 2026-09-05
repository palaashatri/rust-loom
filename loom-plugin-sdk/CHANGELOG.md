# Changelog

## 0.1.0 (2026-08-01) — foundation milestone

### Added

- `loom-plugin-manifest`: `PluginManifest` schema (entry points, capabilities,
  permissions, resource limits), validation with structured error taxonomy
  (`Malformed`, `UnknownCapability`, `UnsupportedVersion`, `InvalidId`,
  `MissingField`, `TooLarge`), size-limited parsing, dotted-numeric
  `compare_versions` / `version_compatible`, serde round trips.
- `loom-plugin-host`: `PluginStore` (open/install/list/get/uninstall),
  defensive install (name pre-scan, archive-bomb guards, bounded streaming
  copies, manifest-first validation, wasm size cap, sha256 recording,
  cleanup-on-failure, informational `installed.json` regenerated from disk),
  `permissions_for` / `check_permission` with component-aware prefix
  matching and network-limit gating.
- `loom-plugin-cli`: `loom-plugin` binary with `validate`, `install`, `list`,
  `remove` (hand-rolled arg parsing); demo fixture generation producing
  `target/fixtures/demo.loomplugin` from committed text sources.
- RFC-0009 (plugin ABI and sandboxing): accepted as architecture; runtime
  implementation BLOCKED pending wasmtime pinning.

### Known limitations

- No WASM execution (by design, this milestone).
- No plugin signing (designed in RFC-0009, not implemented).
- `list()` silently skips corrupt installs rather than reporting them.
