# Performance

## Budgets

Measured on mainstream hardware (CI container, debug build):

| Operation | Budget |
| --- | --- |
| `parse_manifest` (10 KB doc) | < 1 ms |
| `install_zip` (1 MiB package) | < 100 ms |
| `list()` with 50 installed plugins | < 10 ms |
| `check_permission` | no filesystem syscalls when the path exists (canonicalize short-circuits); lexical normalization otherwise |

These budgets are not yet enforced by benchmarks; add them when the
`loom-plugin-cli` gains a `bench` subcommand.

## Design choices for speed

- Install pre-scans the central directory once (no per-entry re-open).
- Extraction streams (`io::copy` over `Read::take`), no whole-archive
  buffering; memory stays O(largest entry).
- Permission prefix checks compare path components after a single
  canonicalization; no repeated syscalls.
- `installed.json` regeneration only touches directory entries that look like
  `id@version`.

## Memory

- Archive-bomb limits cap worst-case disk and memory regardless of declared
  sizes (streaming caps are the truth).
- The zip test fixture is 8-byte wasm + small text; `cargo test` uses
  negligible memory.
