# RFC-0007 — Undo and Transaction System

- Status: **ACCEPTED (normative design; disk-backed history NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-history`, all applications

## Context

Every user action in Loom must be undoable. The root directive requires:
atomic edits, compound operations, coalescing, named undo actions, memory
budgets, disk-backed history for large media projects, branch handling
policy, autosave interaction, crash recovery interaction, plugin operation
integration, deterministic replay tests.

## Goals

- A transactional history framework in `loom-core/crates/loom-history`.
- Edits apply atomically: a document never exposes a partially applied
  edit.
- Compound operations undo as one unit; adjacent identical edits coalesce.
- Undo/redo determinism testable by replay.
- An architecture that can move history to disk for large projects.

## Non-goals

- Multi-user or networked history.
- Disk-backed history in the initial milestone.

## Proposed design

- History is a stack of transaction records. Each record carries a stable
  name (localization key), the command id that produced it, and the inverse
  operations needed to revert (or forward to redo) the document state.
- The `loom-history` crate provides the stack, transaction scope (begin/
  commit/cancel), compound grouping, coalescing policy, and a memory budget
  (oldest entries evicted with a documented policy once a threshold is
  reached). Implemented: in-memory transactions, 7 tests.
- Documents expose state-changing operations as mutations
  (`loom-document`'s `Mutation`/`TextEdit` types are the model for this);
  commands invoke mutations inside a transaction
  (`RFC-0002-UI-and-Engine-Separation.md`, `loom-command`).
- Autosave/recovery interplay: autosave snapshots capture committed
  history state; recovery reconciles the journal, and undo history beyond
  the snapshot is preserved where practical (see RFC-0018).
- Branch handling policy: when a recovered journal diverges from autosave,
  the newer revision wins and the divergent branch is offered in the
  recovery browser.
- Plugins participate through the same command/mutation interface; a plugin
  operation must register inverse operations with the same guarantees as
  built-ins.
- Disk-backed history (NOT_STARTED): journal snapshots in the package
  `history/` directory, written transactionally; designed but not
  implemented.

## Alternatives

- **Full document snapshots per edit**: simple but memory-heavy; rejected
  for large documents/media.
- **Per-app undo stacks**: duplication; rejected — one framework.

## Trade-offs

Inverse-operation records are compact but require every mutation to define
its inverse; enforced by tests (every mutation tested undo→redo identity).
Coalescing reduces memory but can confuse users if it merges distinct
actions; coalescing applies only to provably identical consecutive
operations with the same name.

## Security

History entries contain document content; memory budget prevents
unbounded growth; disk history respects package checksums and path rules
(`RFC-0006`).

## Performance

Undo/redo must be O(affected range), not O(document); editing hot paths
must not clone whole documents. Memory budget caps history growth.

## Compatibility

The transaction record schema is internal to `loom-history`; persisted
history entries are versioned with the package format.

## Migration

None needed yet; the crate is additive from 0.1.0.

## Testing

- Deterministic replay: record an edit sequence, replay forward/backward,
  assert document identity.
- Property tests: undo/redo invariants over random mutation sequences.
- Coalescing and compound tests; memory-budget eviction tests.
- Recovery-journal reconciliation tests with RFC-0018.

## Open questions

- Whether coalescing is time-bounded (e.g. 2 s window) — resolved at app
  integration; policy documented per app.

## Final status

ACCEPTED. In-memory transaction core implemented (`loom-history`, 7 tests);
disk-backed history NOT_STARTED.
