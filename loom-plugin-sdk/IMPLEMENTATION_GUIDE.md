# Implementation Guide

## loom-plugin-manifest

- `parse_manifest(json)` = size check + `serde_json` deserialize + `validate()`.
- Unknown capabilities surface as `UnknownCapability` via a custom
  `Deserialize` impl that emits a recognizable error message, reclassified in
  `classify_json_error` (serde_json exposes no typed error, so the message
  prefix is the contract — see the tests).
- `validate()` checks rules in a fixed, documented order
  (version, id, name/version, api range, module path, function, permission
  modes, resource limits, network rule).
- Version comparison is dotted-numeric with missing/non-numeric parts = 0;
  `version_compatible(api, host_min, host_max)` is the single
  negotiation entry point. Do not add a semver crate without an ADR.

## loom-plugin-host

- Install order matters: pre-scan names/sizes, read+validate manifest,
  check api overlap, check wasm presence/size, check AlreadyInstalled, then
  extract. On any error inside extraction, the install directory is removed.
- Declared `entry.size()` is advisory; every copy streams through
  `Read::take(limit + 1)` so lying archives are still bounded.
- `list()` skips corrupt installs and `installed.json` (informational).
- `check_permission` canonicalizes existing paths, lexically normalizes
  missing ones, and compares `Path::components()` — never string prefixes.

## loom-plugin-cli

- Hand-rolled arg parsing in `cli::run(args) -> i32` (0 ok, 1 operational,
  2 usage). Keep it that way; no clap.
- Fixture generation lives in `fixture.rs`; the zip is built from committed
  text sources, so tests never depend on committed binaries.

## Tests

- Unit tests live inside each `lib.rs`; integration tests in
  `tests/integration.rs` (host) and `tests/cli_integration.rs` (cli).
- Security tests must assert the store contains nothing after a rejected
  install.
- When editing validation rules, extend the manifest error-matrix test
  instead of relaxing assertions.
