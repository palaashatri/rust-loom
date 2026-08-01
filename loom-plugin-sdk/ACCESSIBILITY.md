# Accessibility

This repository contains no UI; accessibility applies to the CLI surface and
to the APIs that will back Loom's plugin-management UI.

## CLI

- Errors go to stderr with exit codes (0 ok, 1 operational, 2 usage) — safe
  for scripting and screen-reader friendly output redirection.
- `--help` documents every command; `--version` is machine-parseable.
- Output is plain text with no color or ANSI sequences, so terminal
  accessibility is preserved.

## API (for future plugin-management UI)

- Every error type implements `Display` with a human-readable, complete
  message (no truncation, no codes-only output).
- `ManifestError` and `HostError` carry structured variants, enabling
  accessible error UI that reads the failure reason aloud.
- Permission surfaces are exposed via `permissions_for(&InstalledPlugin)`
  so a management UI can render them as a plain list (screen-reader
  friendly) instead of raw JSON.

## Requirements inherited from Loom

When a plugin-management UI is built: keyboard navigation, visible focus,
non-color status indicators, and screen-reader labels for every control
(per the Loom design bible).
