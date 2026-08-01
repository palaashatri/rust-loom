# AGENTS.md — working rules for loom-bootstrap

## Scope

loom-bootstrap owns **orchestration only**: builds, tests, visual QA, offline
verification, packaging, and the cross-repo compatibility contract
(`COMPATIBILITY.toml`). It must never contain application code or duplicate
logic that belongs in `loom-core`.

## Boundaries

- Do **not** modify sibling repositories. If a change is required there, file
  the need in the task ledger / RFC and let that repo's owner apply it.
- Sibling repos are located one level up (`../loom-<name>`) and are located at
  runtime, never hard-coded to absolute paths.
- `COMPATIBILITY.toml` is the contract: MSRV, Slint version, per-repo
  status/rev. Changes require an RFC entry and a suite-wide gate re-run.
- Status classifications (from the suite directive): `COMPLETE`,
  `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`, `NOT_STARTED`,
  `BLOCKED`. Never mark something `COMPLETE` without test evidence.

## Script conventions

- Bash with `set -euo pipefail`. POSIX-safe idioms; no zsh-isms; no
  bash-3.2-incompatible syntax (scripts may run on macOS `/bin/bash` 3.2).
- Every script must fail gracefully when a sibling repo is absent:
  report `SKIP`/missing and continue, per README behavior table.
- Shared helpers live in `scripts/lib.sh` (paths, logging, timeout, app-binary
  discovery). New scripts must source it rather than re-implement.
- Every script writes logs to `.work/` (`$WORK`) — never to a repo it doesn't
  own.
- Exit codes: 0 = all executed steps passed; 1 = an executed step failed;
  2 = usage error.

## Quality gates (must pass before reporting done)

1. `bash -n scripts/*.sh`
2. `bash scripts/env-check.sh` (toolchain >= 1.80)
3. `bash scripts/fmt-all.sh` and `bash scripts/clippy-all.sh`
4. `bash scripts/test-all.sh`
5. `bash scripts/build-all.sh`

Do not claim a gate passed without having run it in this session. If a gate
cannot be run (missing tool, no binaries yet), say so explicitly with the
reason.

## Reporting

Completion reports must include: files created, `bash -n` results, which
scripts were executed (with real output), and an honest status for anything
not run or not verifiable (e.g. visual QA needs built binaries and baselines).
