# Visual QA

This repository contains no UI code — it is a library and a CLI tool.

Visual quality applies to the CLI output contract, which is covered by
integration tests asserting exact stdout/stderr expectations:

- `loom-plugin validate`: summary lines (`manifest OK`, `id:`, `entry:`, ...)
  or `validation failed: <error>`.
- `loom-plugin install`: `Installed <id> <version> (sha256 <hex>) -> <dir>`.
- `loom-plugin list`: aligned table or `no plugins installed in <dir>`.
- `loom-plugin remove`: `Removed <id>`.

Any change to these strings must update `tests/cli_integration.rs` first.

When Loom's component gallery milestone lands, the CLI output examples in
`BUILDING.md` must be regenerated from real runs.
