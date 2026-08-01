# RFC-0009: Plugin ABI and Sandboxing

- Status: **Accepted as architecture; runtime implementation BLOCKED** until
  the wasmtime pinning decision is made (see Open Questions).
- Author: Loom Plugin SDK lead.
- Created: 2026-08-01.
- Related RFCs: RFC-0001 (repository/versioning), RFC-0010 (Loom Vision
  provider model), RFC-0011 (model-pack format).

## Context

Loom is a local-first creative suite. Third-party extension points (commands,
importers, exporters, effects, generators, inspectors, vision providers,
document and media processors) must be safe to install and run on user
machines. The extension system must never require a network connection, must
crash-isolate misbehaving plugins, and must give users a clear, verifiable
permission model.

The current milestone delivers the package/manifest/host foundation: a
validated manifest schema, a defensive zip installer, a directory-backed
store, and a permission-checking API. Nothing in this milestone executes
plugin code.

## Goals

- Define the WASM32-WASI plugin module ABI used by Loom hosts.
- Define the capability negotiation model (api_min/api_max).
- Define resource limits (memory, fs, cpu, network) and their enforcement
  points.
- Define a sandboxing strategy with a clear evolution path from in-process
  WASM sandboxing to per-plugin process isolation.
- Deliver install-time security: archive guards, path validation, manifest
  validation, checksum recording.

## Non-Goals

- Shipping a WASI runtime in this milestone. `loom-plugin-host` must not
  execute wasm.
- Plugin signing/public-key trust in this milestone. The signing architecture
  is designed (see below) but not implemented.
- Remote plugin marketplaces, accounts, or network update checks.
- Native (non-WASM) plugin binaries in this milestone.

## Proposed design

### Module ABI

Plugins are WASM32-WASI modules (component-model style eventually; core-wasm
with the `wasi_snapshot_preview1` ABI initially). The host imports and the
guest exports the following symbols:

```text
guest exports:
  (func loom_plugin_init  (export "loom_plugin_init")  (param i32) (result i32))
  (func loom_plugin_invoke(export "loom_plugin_invoke")(param i32 i32) (result i32))

host imports (namespace "loom_host"):
  loom_host_log, loom_host_file_open/read/write, loom_host_read_dir,
  loom_host_clipboard_get/set, loom_host_vision_infer,
  loom_host_temp_dir, loom_host_state_dir, loom_host_http_request, ...
```

- `loom_plugin_init` receives a pointer to the serialized manifest context
  and returns a status code (`0` = ok).
- `loom_plugin_invoke` receives `(command_id, payload_ptr)` and returns a
  status code. All payload exchange is via linear memory plus an
  out-parameter size struct, never via guest-chosen host addresses.
- Every `loom_host_*` call validates the guest-provided pointer range before
  touching host state. This is the hard trust boundary inside the process.
- The host maps a plugin's declared capabilities to the set of import
  functions it links in. Capabilities are negotiated at instantiation from
  `api_min_version`/`api_max_version` in the manifest.

### Capability negotiation

`manifest.api_min_version..=manifest.api_max_version` must overlap the
host's `HOST_API_MIN_VERSION..=HOST_API_MAX_VERSION` (checked at install time
and again at load time). A mismatch blocks installation with a clear error.

### Resource limits

Enforced at three layers:

1. Install time (manifest-declared and host caps): entry count, total bytes,
   wasm size.
2. Runtime (in-process sandbox): guest memory via the runtime's memory limits,
   call duration via a watchdog timer, fs byte/entry quotas inside the
   `loom_host_*` wrappers.
3. Process boundary (later phase): OS-level rlimits, cgroup/quota for fs, and
   network namespace.

`resource_limits.network` gates the `loom_host_http_request` import; a plugin
without it cannot even link the import.

### Crash isolation

Phase 1: in-process WASM sandbox (wasmtime) — guest traps are caught and
reported; host state is protected by validated pointer ranges and the import
boundary. Phase 2: one OS process per plugin (or per plugin family), with a
Unix-socket/RPC-style command channel; guest crashes then cannot take down the
host. Phase 2 is the release target for running third-party code.

### Install-time security (implemented)

Entry-name checks (`..`, absolute, backslash, symlink bits), `MAX_ENTRIES`,
`MAX_TOTAL_BYTES`, bounded streaming copies, manifest parse+validate before
extraction, wasm size cap, sha256 of the manifest recorded, `installed.json`
index regenerated from disk.

### Signing architecture (designed, not implemented)

Plugin packages may declare an optional `signature` block: an Ed25519
signature over the sorted manifest document + module bytes, with a public key
supplied in a sidecar `.sig` file or the manifest. Verification happens at
install time only when a keyring is configured; unsigned plugins are allowed
with an explicit trust prompt. No central authority; users add keys locally.

## Alternatives

- **Process-per-plugin only** (no in-process WASM): simpler isolation story
  but much higher per-call latency and memory cost; rejects the fast path for
  trusted plugins. Rejected as the sole strategy; adopted as the eventual
  default for third-party plugins.
- **wasmtime vs wasmtime-go vs custom interpreter**: wasmtime (Rust, WASI,
  cranelift, actively maintained, Apache-2.0) is the candidate runtime;
  wasmtime-go would force a Go dependency into the suite; a custom
  interpreter is unmaintainable. Decision is pending a dependency-review pass
  per the loom-core dependency policy, which is why runtime work is BLOCKED.
- **Native plugins (cdylib)**: rejected — no isolation without heavy
  machinery (seccomp/landlock) and worse portability across Linux distros.
- **POSIX seccomp sandboxing as the only mechanism**: rejected for the first
  release; revisit for process-isolation phase.

## Trade-offs

- In-process WASM has the best performance but a wider attack surface than
  process isolation; mitigated by the import boundary and by graduating to
  process isolation for third-party plugins.
- Manifest validation duplicates work between `loom-plugin-manifest` and
  `loom-plugin-host`; kept separate so the manifest crate is a pure,
  sandbox-safe library.
- Hand-rolled version comparison (no semver crate) is less precise than full
  semver but deliberately tolerant (missing parts = 0) for forward
  compatibility; documented in the manifest crate.

## Security

- No `unsafe` in any crate of this repository.
- All archive extraction is bounded, name-validated, and pre-scanned before
  any write.
- Permission checks are enforced in the host library, not delegated to the
  plugin.
- No network code exists in this repository.

## Performance

- Install is single-pass over the archive; extraction is streaming.
- Permission checks avoid filesystem syscalls when the path exists only
  lexically (canonicalize-on-exists).
- Budget: install of a 1 MiB package < 100 ms on mainstream hardware (to be
  measured in the runtime milestone).

## Compatibility

- `manifest_version` is versioned; unknown versions are rejected with a
  structured error, never guessed.
- Hosts must be able to run plugins built against older `api_min` versions
  within the supported window.

## Migration

- Store layout `id@version/manifest.json` is stable from this milestone.
- A future runtime milestone adds a `state/` and `temp/` directory per plugin
  without changing the manifest schema.

## Testing

- Manifest: validation matrix, error taxonomy, version-compatibility matrix,
  serde round trips (done).
- Host: safe install, hostile archives (traversal, absolute, symlink),
  archive-bomb limits, double install, uninstall, permission matrix,
  index regeneration (done).
- Future: wasm execution tests, trap recovery, cancellation, cross-version
  fixture corpus.

## Open questions

- wasmtime pinning: version, feature set (`wasi-common` vs `wasmtime-wasi`),
  and MSRV impact — BLOCKING the runtime milestone.
- Component-model adoption timeline vs core-wasm + `wasi_snapshot_preview1`.
- Whether plugins may host their own threads (wasmtime supports it) and how
  that interacts with cpu limits.

## Final status

**Accepted as architecture.** Manifest, host, and CLI implementations are
complete and tested. Runtime execution and signing are documented as
`NOT_STARTED` in `ROADMAP.md` and must not be represented as done.
