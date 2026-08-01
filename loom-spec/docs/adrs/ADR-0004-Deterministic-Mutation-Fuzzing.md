# ADR-0004 — No cargo-fuzz on the Stable Toolchain; Deterministic Mutation Fuzz Tests Instead

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

The root directive requires fuzz targets for package readers, importers,
media metadata and subtitle parsers, formula/rich-text/manifest parsers,
clipboard input, and recovery journals. `cargo-fuzz` typically needs a
nightly toolchain, which conflicts with the MSRV-1.80 stable-only policy
and reproducible builds.

## Decision

- Do not require `cargo-fuzz`/nightly in the initial milestones.
- Implement **deterministic mutation fuzzing as normal `#[test]` targets**:
  seeded mutation generators over seed corpora (bit flips, truncations,
  block insertions, garbage bytes), run with fixed seeds so failures
  reproduce exactly and results are stable across CI runs.
- Mutation fuzz tests are part of `cargo test` and the release gates
  (`IMPLEMENTATION_GUIDE.md` §6).
- Revisit coverage-guided fuzzing (`cargo-fuzz` on a pinned nightly, or
  libFuzzer) at hardening phase as an optional enhancement — never as a
  hard gate on stable.

## Consequences

- Deterministic, reproducible, stable-toolchain fuzzing now; weaker
  coverage guidance than libFuzzer.
- Seed corpora grow from real bug findings; failures pin the exact seed
  and input.
- Fuzz targets live in the owning repositories with clear names
  (`*_fuzz_mutate`), not in a separate workspace.

## Verification

- Every parser listed above has a mutation fuzz test with a documented
  seed corpus; gate runs them with the suite
  (`RELEASE_CRITERIA.md` §2).
