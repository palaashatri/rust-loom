# Security

## Threat model

A plugin package is untrusted input. Attackers may try: zip-slip (path
traversal), archive bombs, symlink escapes, oversized manifests/wasm,
malformed JSON to confuse validation, capability confusion, and path-prefix
confusion. This milestone's host never executes plugin code, so remote-code
execution is out of scope until the WASI runtime milestone.

## Controls (implemented)

- **Path safety**: every entry name pre-scanned — must be relative, no `..`
  or `.` components, no backslashes, no leading `/`. Rejected before any
  write.
- **Symlink entries**: unix mode bits checked (`S_IFLNK`); real archivers'
  symlink entries are rejected at install.
- **Archive bombs**: `MAX_ENTRIES = 1024`, `MAX_TOTAL_BYTES = 256 MiB`
  declared-size checks plus streaming caps (`take(limit + 1)`) on every copy.
- **Manifest trust**: parsed and validated before extraction; unknown
  capabilities rejected; `manifest_version` must be exactly 1.
- **Wasm size**: `< 100 MiB`, presence verified before extraction.
- **Atomicity**: failed installs remove the partial install directory; the
  store is never left half-written.
- **Permissions**: enforced in the host library (never delegated to
  plugins); canonicalized component-wise path comparison prevents
  `/a/b` vs `/a/bc` prefix confusion; relative prefixes resolve against the
  plugin install dir.
- **API negotiation**: plugin api range must overlap the host's; mismatches
  block install.
- **Index**: `installed.json` is informational and regenerated from disk, so
  tampering with it changes nothing.

## Policies

- No `unsafe` (enforced by `#![forbid(unsafe_code)]`).
- No network code anywhere in the repo.
- Temp dirs are created via `std::env::temp_dir()` + unique names; test
  helpers always clean up via Drop.

## Future (see RFC-0009)

- WASM sandbox import boundary, trap recovery, watchdog timers.
- Per-plugin process isolation.
- Ed25519 plugin signing with local keyring; unsigned plugins gated by user
  consent.
