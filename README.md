# Loom

Loom is an experimental, local-first creative suite written in Rust with Slint.
The repository contains shared platform crates and early functional slices for
Writer, Sheets, Present, Photo, Motion, Video, Studio, Encode, Vision, and the
plugin SDK.

The project is **not a finished replacement** for established professional
creative applications. Several applications currently demonstrate project
models, persistence, commands, and interactive UI shells while their media,
rendering, audio, and editing engines remain incomplete. See [`TRUTH.md`](TRUTH.md)
for the current implementation boundary.

## Build and test

The suite is split into independent Cargo workspaces. The canonical orchestration
scripts live in `loom-bootstrap/scripts`:

```bash
bash loom-bootstrap/scripts/env-check.sh
bash loom-bootstrap/scripts/fmt-all.sh
bash loom-bootstrap/scripts/test-all.sh
bash loom-bootstrap/scripts/clippy-all.sh
bash loom-bootstrap/scripts/build-all.sh --release
```

Linux is the primary target. All core workflows are intended to work offline.

## Project rules

Read [`AGENTS.MD`](AGENTS.MD) before making changes. It is the single engineering
source of truth. Do not add scattered audit reports or speculative status files;
update [`TRUTH.md`](TRUTH.md) with reproducible evidence instead.

## License

Original Loom code is intended to be available under `MIT OR Apache-2.0`.
Third-party dependencies, codecs, fonts, models, and assets retain their own
licenses and must be audited before distribution.
