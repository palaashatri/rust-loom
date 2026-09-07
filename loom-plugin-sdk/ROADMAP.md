# Roadmap

## Status legend

COMPLETE / FUNCTIONAL_WITH_LIMITATIONS / EXPERIMENTAL / SCAFFOLDED /
NOT_STARTED / BLOCKED

## Current (milestone 1 — foundation)

| Item | Status | Evidence |
| --- | --- | --- |
| Manifest schema, validation, version compare | COMPLETE | 24 unit tests, error matrix |
| Safe zip installation, store, index | COMPLETE | 19 integration tests incl. hostile archives |
| Permission model + check API | COMPLETE | permission matrix tests |
| CLI validate/install/list/remove | COMPLETE | 8 integration tests against the real binary |
| Fixture generation from committed text sources | COMPLETE | `target/fixtures/demo.loomplugin` produced by tests |

## Next (milestone 2 — runtime, BLOCKED)

| Item | Status | Blocker |
| --- | --- | --- |
| WASI runtime execution (`loom_plugin_init`/`invoke`, `loom_host_*` imports) | NOT_STARTED | BLOCKED on wasmtime pinning decision (RFC-0009 open questions) |
| Resource-limit enforcement at runtime (memory, cpu watchdog, fs quotas) | NOT_STARTED | depends on runtime |
| Guest API version negotiation at instantiation | NOT_STARTED | depends on runtime |

## Later

| Item | Status |
| --- | --- |
| Plugin signing (Ed25519, local keyring) | NOT_STARTED (architecture in RFC-0009) |
| Process-per-plugin isolation | NOT_STARTED (phase 2 of RFC-0009) |
| Plugin sandbox benchmark harness | NOT_STARTED |
| CLI `bench` subcommand + perf budgets enforcement | NOT_STARTED |
| Component-model ABI adoption | NOT_STARTED |

## Honesty statement

Everything marked COMPLETE above has passing tests and real behavior.
Nothing in this repository executes WebAssembly, and no code path pretends
to. WASI execution and signing are documented in RFC-0009 but not
implemented, and must not be reported as done.
