# Architecture

## Layers

```text
loom-plugin-cli  (binary; arg parsing; fixture generation)
      |
      v
loom-plugin-host (store; safe zip install; permission checks)
      |
      v
loom-plugin-manifest (schema; validation; version compare)
      |
      v
serde / serde_json / sha2 / zip  (pinned, MIT/Apache-2.0)
```

Dependency direction is strictly downward: `manifest` depends on nothing from
this repo; `host` depends on `manifest`; `cli` depends on both.

## Data flow

1. A plugin package (`.loomplugin` zip) arrives as bytes.
2. `PluginStore::install_zip` pre-scans the archive: entry count <=
   `MAX_ENTRIES`, declared total <= `MAX_TOTAL_BYTES`, every name safe
   (relative, no `..`, no backslash), no symlink entries. Any violation
   aborts before a single byte is written.
3. `manifest.json` is stream-read (bounded) and parsed/validated by
   `loom-plugin-manifest`. The plugin API range must overlap the host's.
4. The wasm module named by `entry.wasm_module` must exist and be <
   `MAX_WASM_BYTES`.
5. All safe entries are extracted into `<store>/<id>@<version>/` with
   streaming caps. The manifest sha256 is recorded.
6. `installed.json` is an informational index regenerated from disk on every
   `open()`.

## Permission model

- `Capability` = coarse grant (must be in `manifest.capabilities`).
- `Permission` = fine-grained `(resource, mode, path_prefix)`.
- `check_permission(plugin, capability, path)` enforces both, resolving
  relative `path_prefix` values against the plugin install directory and
  comparing canonicalized path components (no partial-name prefix matches).
- `HttpRequest` additionally requires `resource_limits.network == true`.

## Error taxonomy

- `ManifestError`: parse/validation of the manifest document
  (Malformed, UnknownCapability, UnsupportedVersion, InvalidId, MissingField,
  TooLarge).
- `HostError`: store operations (Io, Zip, InvalidManifest, AlreadyInstalled,
  NotFound, UnsafePath, TooLarge, UnsupportedApi, Denied).

## Non-goals (this milestone)

- WASM execution (BLOCKED per RFC-0009).
- Plugin signing (designed, NOT_STARTED).
- Remote anything.
