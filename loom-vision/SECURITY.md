# Security

Threat model: model packs and images are attacker-controlled inputs to
Loom Vision; the framework must never let such inputs escape their
directory, exhaust resources, or trigger network traffic.

## Guarantees

1. **No network.** There is no network code anywhere in the workspace.
   Providers and pack validation are purely local. Runtime behavior must
   not change if the machine is offline.

2. **Path traversal.** Manifest model paths must be relative and may only
   contain `Normal` path components — absolute paths, `.`, `..`, roots, and
   prefixes are rejected during validation. Pack `id`/`version` strings are
   sanitized to `[A-Za-z0-9._-]` (leading dots stripped) before they are
   used in destination directory names, so a hostile manifest cannot write
   outside `dest_dir`. Symlinked model files and symlinked destinations are
   refused.

3. **Checksums.** Every model file's size must match the manifest and its
   SHA-256 digest (streamed, 64 KiB chunks) must match. `install_pack`
   refuses to overwrite an existing pack with different checksums;
   `install_pack_force` is the explicit opt-out.

4. **Archive-bomb guard.** Validation takes a maximum total unpacked size
   (default 2 GiB via `DEFAULT_MAX_PACK_SIZE_BYTES`); packs whose declared
   sizes exceed it are rejected. Cumulative sizes are computed with checked
   arithmetic.

5. **No `unsafe`.** `#![forbid(unsafe_code)]` in both crates; the registry
   achieves thread-safe handle sharing with owned `Arc`s instead of
   borrowed references.

6. **Malformed input.** Image buffers are validated for channel count
   (1/3/4), exact length, and non-zero dimensions before any arithmetic.
   `RunContext` clamps progress to `[0, 1]`.

7. **Crash isolation by design.** Providers run behind the trait boundary;
   a provider crash cannot corrupt pack state because validation is
   side-effect free (only reads), and installs happen only after full
   validation.

## Out of scope (documented, not implemented)

- Plugin sandboxing / WASM isolation (future `loom-plugin-sdk`).
- Encrypted model files.
- Secure deletion.

## Review checklist

- New dependency → verify license (DEPENDENCIES.md), no network features,
  no `unsafe` in our usage.
- New path handling → run it against the traversal tests.
- New provider → keep it out of the pack-install path or validate first.
