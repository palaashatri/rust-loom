# Contributing

## Getting started

1. `cargo build --workspace` — must succeed before anything else.
2. Read [ARCHITECTURE.md](ARCHITECTURE.md) and
   [IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md).
3. Pick a task from [TASKS.md](TASKS.md) and announce ownership.

## Pull request checklist

- Code compiles with `cargo build --workspace` (debug and release).
- `cargo fmt --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- `cargo test --workspace` passes; new tests accompany new behavior.
- Docs updated where behavior or the provider surface changed
  (ROADMAP/TASKS statuses, DEPENDENCIES for new deps).
- No `unsafe`, no network code, no hardcoded absolute paths, no secrets.
- Commit `Cargo.lock` when dependencies change.

## Standards

- Public API items documented (`///`); `missing_docs` is enforced.
- Errors are typed and `Display`-able; never `panic!` on user input.
- Reference providers implement real algorithms with real tests — no
  placeholders, no `assert!(true)`.
- Behavior changes to accepted contracts (trait shapes, manifest schema)
  go through an ADR first; `FORMAT_VERSION` bumps are breaking.

## Reporting issues

Include: reproduction steps, `cargo test` output, environment (OS, rustc
version). Security issues: see [SECURITY.md](SECURITY.md) — do not include
credentials or personal paths.
