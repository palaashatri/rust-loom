# Loom workflow and acceptance rules

## Owner overrides — summary

The dated owner overrides listed here authorized specific past runs. Their full text was moved verbatim to [docs/archive/AGENTS-ARCHIVE.md](../archive/AGENTS-ARCHIVE.md#archived-owner-overrides-moved-2026-10-25) on 2026-10-25 so that this file stays small. They remain the record of what was authorized; none of them is an open instruction for new work. The current phase and scope are in `loom-bootstrap/contracts/workflow.toml` and the current-truth section.

- Owner override — 2026-09-15 P0/P1 repair run
- Owner override — 2026-09-30 Windows and Linux production push
- Owner override — 2026-10-02 production finish (Sheets, Writer, Present)
- Owner override — 2026-09-18 Sheets then Writer completion run
- Owner override — 2026-09-25 Sheets continuation branch
- Owner override — 2026-09-29 reusable Linux desktop input skill

## Start here — one small repair at a time

The owner requested the 2026-09-14 code and UI/UX audit and these repair instructions. Scheduling lives in the [current-truth section](../workflow/current.md#current-truth); original findings and current card states are routed through the [repair index](../workflow/repairs.md); each card keeps its original finding and records its current repair result. New findings start OPEN.

For a small local coding model, including a model with a tiny context window:

1. Read this section, the active gate and repair order in the [current-truth section](../workflow/current.md#current-truth), and `loom-bootstrap/contracts/workflow.toml`. If they disagree, report the exact disagreement before editing product code.
2. Normally select **one** OPEN card allowed by the active gate and current repair order. CODE-01 is the original starting card; do not reopen a FIXED card without fresh evidence. During the dated owner overrides above, the owner has already authorized the listed P0/P1 cards and the active application's remaining cards, so keep the same one-card boundaries inside each milestone while working through that explicit list. Read only the current card and its named source functions, their callers, and applicable rules here. Do not ask a tiny model to repair the entire suite in one prompt.
3. Say what the user does, what goes wrong now, and what must happen instead. Copy the card's acceptance check into a focused regression test or a short manual interaction script. For a data-loss fix, first run the test and see it fail for the reported reason.
4. Make the smallest coherent change. A card with several steps may need several patches. Do not mix formatting, renaming, feature work, and a bug fix. Preserve unrelated uncommitted work.
5. Run the failing example again and the affected module's existing tests. For a UI change, also build the actual app, perform the interaction, save a fresh screenshot, and inspect it at the card's sizes/themes. A callback-only journey does not prove native focus or keyboard behavior.
6. Check the real result: compare saved/reopened data, inspect an independently opened export, or verify the changed visible state. A success message or a green build is not proof.
7. Record changed files, exact commands, observed result, evidence path, and remaining gaps under that card. Mark FIXED only when its check passes. Mark NEEDS_REVIEW when the code changed but a required check or expert review is missing. Do not delete the original finding.
8. Normally stop after this card. Do not change acceptance, unlock the next app, refresh visual baselines, or take the next card automatically unless the current task or the owner override above authorizes it.

Useful starting commands (run from the repository root; substitute the one named app/package):

```bash
rg -n '^### CODE-01|^### CODE-02' docs/archive/AGENTS-ARCHIVE.md
rg -n 'fn open|next_seq|checkpoint' loom-core/crates/loom-production/src/lib.rs
cargo test --manifest-path loom-core/Cargo.toml -p loom-production --locked
python3 loom-bootstrap/scripts/audit-governance.py
```

Read one complete card between its `###` heading and the next `###` heading in `docs/archive/AGENTS-ARCHIVE.md`; the [repair index](../workflow/repairs.md) lists every card and its state. The ignored `.work/` reports contain supporting evidence, not a second task queue. If an old probe is missing, rebuild the tiny fixture described in the card; do not mark it passed from an old log.

**Small models need bounded tasks, not weaker standards.** CODE-02, CODE-08, CODE-09, and CODE-15 involve atomic file operations, security boundaries, or concurrency. Split them into reproducer, implementation, failure-injection checks, and experienced review. If you cannot explain every failure path, leave NEEDS_REVIEW with the precise gap. Do not guess. Never “fix” data loss by dropping data, ignoring an error, deleting a journal, weakening a test, or turning a feature into a success-message stub.

## 3. Active workflow and audit repair gate

The current phase and active repair scope are recorded together in the [current-truth section](../workflow/current.md#current-truth) and `loom-bootstrap/contracts/workflow.toml`. Do not infer them from old acceptance prose or this file's history.
The dated owner overrides summarized at the top of this file (full text in the archive) are explicit exceptions for the 2026-09-15 P0/P1 run and the 2026-09-18 Sheets-then-Writer completion run. The latter authorizes the recorded phase transition and serial application edits while preserving the binary acceptance gates.

During `audit-repair`, new application features and new foundation consumers are locked. In the active `sheets` phase, product edits are limited to Sheets and the evidence required by its gate; later applications remain locked. Existing foundation imports may remain only for consumers enumerated in the workflow contract; retaining an import is not acceptance.

Finish the shared data-integrity repairs and their failure-path tests before application repairs. Recheck the shared visual foundation and obtain explicit human acceptance before advancing application design. Then repair applications serially, starting with Sheets. A later app's recorded defect is not permission to skip the queue. Security/runtime cards outside the app sequence need an explicitly recorded bounded repair scope before implementation.

Changing phase is a separate reviewed action: update the workflow contract and the active gate in the current-truth section together, with evidence. When adding a repair stage the validator does not yet support, update `loom-bootstrap/scripts/audit-governance.py` and its focused tests in that same reviewed change. The contract's allowed prefixes are an outer file boundary; the named active scope still limits which product behavior may change. An audit or a documentation change must never silently promote an app to ACCEPTED.

Always forbidden: app-local copies of generic controls; placebo or disabled future controls; baseline refreshes that hide defects; a second competing repair ledger. Existing application UI and `toolkit.slint` are compatibility code, not design authority.

## 4. Serial application workflow

After the shared UI foundation is explicitly accepted, application work proceeds **one application at a time** in this order:

1. Sheets
2. Writer
3. Present
4. Photo
5. Motion
6. Video
7. Studio
8. Encode

The next application remains locked until the current application passes its complete acceptance gate.

A gate is binary: `PASS` or `FAIL`. "Mostly done", "good enough", "94%", "compiles", or "the screenshot is close" are not pass states.

An agent must never start work on a later application because it is easier, more interesting, or parallelizable. Cross-application changes are permitted only when they fix an already-approved shared primitive or shared infrastructure required by the active application.

## 10. Testing and CI policy

CI exists to detect meaningful regressions, not to maximize job count.

### PR / push gate

Run only high-signal checks needed for the active phase:

- governance/contracts;
- code-structure ratchet;
- asset-license audit;
- format and Clippy for touched/active shared workspace;
- focused unit/integration tests;
- shared UI compilation and deterministic capture checks.

Do not build release packages or every application on every routine edit during the UI foundation lock.

### Acceptance gate

For the active foundation/application, generate the complete viewport/theme/state evidence required by its contract. Acceptance evidence must be reviewable by a human.

### Full product matrix

Cross-platform full builds, all-app journeys, packaging, installers, and compatibility/conformance matrices run manually, nightly when useful, or for a release candidate. They are not routine PR noise.

Tests must verify behavior. Source-string counts, screenshot existence, callback-name matching, or arbitrary computed readiness scores cannot substitute for product acceptance.

## 12. Current truth discipline

The marked current-truth section in [docs/workflow/current.md](../workflow/current.md) is the only live readiness ledger; root `AGENTS.md` routes readers to it.

Agents may update it only from verified evidence. Do not manufacture numerical readiness scores. Report concrete capabilities, open defects, tested flows, and missing acceptance evidence. Historical scores and agent-reviewed screenshots are not human acceptance.

Use these state labels exactly where applicable:

- `NOT_STARTED`
- `SCAFFOLDED`
- `FUNCTIONAL_WITH_LIMITATIONS`
- `ACCEPTANCE_BLOCKED`
- `ACCEPTED`

Card states are `OPEN`, `NEEDS_REVIEW`, and `FIXED`. Workflow scheduling uses `LOCKED` and `IN_PROGRESS`; these describe permission to work, not product quality. The live application quality table in the current-truth section must not contradict its detailed sections or the workflow contract.

Do not claim `production`, `complete`, `parity`, `100%`, or equivalent language unless every applicable product gate passes.

## 13. Definition of application acceptance

After the foundation is accepted and an application is unlocked, that application must pass all of the following before the next app begins:

1. shared-foundation adoption with no app-local generic control forks;
2. professional visual acceptance at all required viewports/themes;
3. core daily workflow executable end to end through the normal GUI;
4. correct selection/direct-manipulation model;
5. undo/redo and persistence for edits;
6. native open/save/export flows where applicable;
7. truthful errors/cancellation/recovery;
8. keyboard-only primary workflow;
9. accessibility semantics and focus order;
10. representative performance within documented budgets;
11. interoperability evidence proportional to format claims;
12. focused unit/integration/journey evidence;
13. no severity-1 or severity-2 known defect in the accepted workflow;
14. explicit `ACCEPTED` record in the current-truth section.

If any item fails, the application remains locked in `ACCEPTANCE_BLOCKED` and the next application must not begin.

## 14. Completion behavior for agents

Before claiming a task is finished, an agent must:

1. inspect the active workflow lock;
2. confirm the change is inside the allowed scope;
3. run the relevant focused checks;
4. inspect failures rather than weakening tests;
5. update the current-truth section only if verified product truth changed;
6. state remaining blockers explicitly.

The goal is not to make CI green at any cost. The goal is to make Loom genuinely excellent while keeping the repository small enough, clear enough, and strict enough that the next agent cannot accidentally turn it back into a jungle.


