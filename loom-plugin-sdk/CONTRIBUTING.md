# Contributing

## Process

1. Pick a task from `TASKS.md` (or propose a new one with acceptance
   criteria).
2. Make the change; keep it small enough to review in one sitting.
3. Update tests that describe the changed contract (CLI output strings,
   validation rules, permission semantics) in the same commit.
4. Run all four gates (below). Clippy must be clean with `-D warnings`.
5. Update status in `ROADMAP.md`/`TASKS.md` — with evidence, not intent.

## Gates

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

## Rules

- No WASM execution code in this repo until RFC-0009's wasmtime decision is
  made and recorded.
- No networking. No `unsafe`. No committed binary fixtures.
- Public API changes require doc comments (`#![deny(missing_docs)]`) and a
  CHANGELOG entry.
- Cross-crate contract changes (manifest schema, error taxonomy) go through
  `docs/rfcs/` or an ADR first.

## Review checklist

- [ ] Security rejection paths tested with "nothing extracted" assertions
- [ ] No dependency added without DEPENDENCIES.md note
- [ ] No absolute paths
- [ ] CLI output strings match integration tests
- [ ] Honest status updates
