# LOOM ENGINEERING CONSTITUTION

This file is the **highest-authority engineering instruction in the repository**. Every coding agent, reviewer, automation, and human contributor must read it before modifying Loom. The owner's explicit instructions may change these repository rules; this file is not authority over the owner.

If another repository document, comment, old plan, screenshot, baseline, issue, model instruction, or generated report conflicts with this file, **this file wins**. If a requirement cannot be satisfied, report the blocker. Do not reinterpret, weaken, bypass, stub, hide, baseline, or redefine a requirement to make work appear complete.

## Owner override — 2026-09-15 P0/P1 repair run

The owner explicitly authorized this run to fix every recorded P0 and P1 audit card across the suite. For this run, the normal serial application lock is waived only for those named cards. Keep the safety, test, review, and evidence rules below. Finish each coherent milestone, run its focused checks, record the result in the [current-truth section](AGENTS.md#current-truth), and commit and push it before starting the next milestone.

For a tiny local model, use this recipe for each card: read the card; write down what the user does, what breaks, and what should happen; edit only the named functions; run the focused test; inspect the real output; then copy the command, result, commit, and remaining gap into the card's repair result. `FIXED` means the code and required focused checks passed. `NEEDS_REVIEW` means the code landed but an expert, native, cross-platform, or real-media check is still missing. Never turn a missing check into a success claim.

For a performance card, write down the exact workbook size and machine. Measure formula work, workbook copying, recovery writes, normal Save/Open, visible input delay, scrolling, and peak memory as separate checks. Run the two fixed workloads in the [Sheets performance record](AGENTS.md#source-loom-sheets-performance-md): 10,000 chained formulas and 1,000,000 unique pseudorandom cells. The scroll test measures CPU projection only; it is not proof of native 60 fps. A fast formula test does not prove the window stays responsive; an empty memory-limit field is still an open check.

**Sheets performance status (2026-09-25):** Formula-bar commits send one changed cell to a bounded worker and show “Calculating…” immediately. Picker-selected and startup Open parsing run on a background loader. Native Save and Save As use a worker barrier at revision N: the worker builds one package, atomically writes it, checkpoints the same bytes, and returns a separate FIFO completion. CSV/XLSX exports now wait for their accepted workbook revision, package and write on the worker, and return through an ordered completion queue; at most one export is accepted at a time. Close drains accepted worker/file operations, keeps the window visible on failures, and uses Save/Cancel for dirty work. A consumed worker input error is retained by document generation and revision; Save, Export, and Close stay blocked, New/Open require the existing replacement decision, and only a successful covering full replacement clears the barrier. Save completion updates the baseline only for its document generation and leaves newer revisions dirty. The UI now says newer edits remain unsaved and recovery may still be catching up. A crash before the later recovery batch completes can still lose N+1; that recovery-freshness gap remains open under PERF-01 and REC-02. Many non-cell full-workbook copies still happen on the UI thread. The last fixed million-cell run (2026-09-24, Intel Core i3-2350M) measured 2.56 seconds for evaluation, 12.72 seconds for package creation, and 43.05 seconds for recovery-journal writing; full file callbacks, visible-frame, export-scale, and current recovery-freshness measurements remain open. Do not mark PERF-01 or Sheets accepted until those paths, native scrolling, the reviewed app memory limit, accessibility, and interoperability have passing evidence. Use the running Linux window and native screenshot tool only while the desktop is unlocked; a renderer picture is not live-window proof.

**Sheets source-size status (2026-09-26):** The Sheets maintenance split now puts CSV interop, command dispatch, close handling, file completion ordering, and worker-failure recovery in named modules. `loom-sheets-app/src/main.rs` is 107,128 bytes against its 108,783-byte legacy ceiling; `ui/app.slint` is 39,629 bytes against its 39,732-byte ceiling. The new Rust and Slint modules remain below their source limits. The six generated renderer captures under `loom-sheets/docs/qa-renderer/` have a narrowly scoped provenance rule. The latest repo-wide code-structure audit reports 17 pre-existing findings outside active Sheets/REC-02 sources; changed recovery modules and loom-production/src/lib.rs remain within source limits. Do not edit locked applications or unrelated shared modules to clear them. The asset audit passes.

**Sheets XLSX export status (2026-09-24):** The rich XLSX exporter must link its generated `xl/styles.xml` part from `xl/_rels/workbook.xml.rels`. The exporter previously wrote the style part and cell style IDs but omitted this workbook relationship, so LibreOffice ignored exported cell formatting. `xlsx_export_links_the_styles_part_from_the_workbook` now guards the relationship. LibreOffice Calc 24.2.7.2 applied the tested fill, bold font, border, right alignment, and currency format to a converted fixture. This verifies one fixture and one LibreOffice version; it does not prove Microsoft Excel compatibility or full XLSX interoperability. Keep the Sheets acceptance gate blocked until the broader import/export matrix and remaining acceptance evidence pass.

**Sheets XLSX import status (2026-09-24):** Standard Excel workbooks use ZIP DEFLATE compression. The shared package reader used by Sheets previously accepted only uncompressed ZIP entries and rejected a normal Calc-generated `.xlsx` with `unsupported compression method 8`. It now reads stored and DEFLATE entries, including entries whose sizes follow the compressed data in a ZIP data descriptor. It checks uncompressed size limits before inflation, caps output, and checks descriptor values and CRC. `loom-package` has regression fixtures for both ZIP layouts. A Calc-generated workbook now imports four sheets, formulas, a styled cell, a chart, and a shape label; custom row/column sizes are still named by the existing import warning. The 29 package tests, 266 Sheets workspace tests, full-workspace all-target Clippy, and production app build pass after this change. The build took 3m07s and peaked at 2,985,580 KiB RSS with zero swaps. See CODE-25 for simple repair steps and remaining Excel/interoperability limits.

**Sheets data-safety status (2026-09-26):** REC-02 remains OPEN through stages 1-3, Stage 4A, Stage 4B, Stage 4C1, and Stage 4C2 (ordinary write-side capacity preflight; NEEDS_REVIEW). Sheets writes versioned cell-edit batches after a complete package checkpoint; startup validates and replays them, and legacy migration remains receipt/marker ordered under the shared recovery locks. Stage 4C2 preflights ordinary append and checkpoint operations against the approved package (256 MiB), retained encoded-file (640 MiB), and temporary-peak (1 GiB) ceilings before recovery mutation. Checkpoint publication validates its covered sequence under lock, compacts only covered records, preserves newer journal records, enforces the 10,000-entry and 32-level directory bounds including the two-entry atomic-write staging peak, and retains two generations. Cell-batch journal or aggregate-capacity refusal requests a complete checkpoint; unsupported or unknown recovery state fails closed. **Pending:** recovery errors still do not pause worker edit admission/publication, so visible edits can outpace durable recovery; production checkpoint cadence, truthful Retry/Save As, disk-full/interruption/restart comparison, p95 durability, and the approved performance measurements remain open. A process ignoring `.checkpoint.lock` may race path-based abandoned-tree cleanup; Windows/macOS lock and pointer-replacement behavior remain unverified. REC-02 and Sheets acceptance remain open.
**Sheets file workflow status (2026-09-26):** The export/close milestone retains consumed worker input errors and blocks Save, Export, and Close until a full worker replacement succeeds; New/Open preserve the existing dirty-workbook decision. CODE-23 detects known XLSX losses and asks before replacing a workbook in both the file picker and startup `--open` paths. The shared command dispatcher blocks native-menu and palette actions while the decision is open, including actions that were queued just before the warning appeared. Tests cover actual defined names and confirm that an empty `<definedNames/>` container does not warn, plus external links, conditional formatting, validation rules, PivotTables, frozen panes, custom row/column sizes, extra charts, missing drawing/media parts, a supported no-warning workbook, Cancel, Continue, startup recovery preservation, blocked command dispatch, and Escape after Tab navigation. A native screenshot captured the visible menu row and CODE-23 import-warning dialog on 2026-09-24, but native button, focus, menu-popup, and close interactions remain unverified; do not treat a still image as interaction proof. The XLSX warning is not proof of complete Excel compatibility. Read CODE-23 and REC-02 before changing these paths. When changing XLSX import, warn before replacing the open workbook; Cancel must leave both the workbook and recovery data untouched.

**REC-02 Stage 4A — bounded journal append (2026-09-25):** This scoped production API milestone passed its focused review and checks; REC-02 remains OPEN. RecoveryJournal::append_bounded enforces the complete JSONL line including newline (1 MiB), journal bytes (64 MiB), and record count (10,000) under .checkpoint.lock. It rejects an already over-limit file before parsing and repairs a torn tail only after all candidate bounds pass. Refusal tests verify the file bytes and next sequence are preserved; existing unrestricted append remains available for other callers. Review caught and fixed a mutation-on-refusal defect, then strengthened the ordering regression with an over-limit invalid-UTF-8 file and the specific early “already total” error; the follow-up review confirmed the assertion distinguishes preflight from parsing.

**REC-02 Stage 4B — bounded inspection and Sheets startup replay (2026-09-26) — NEEDS_REVIEW.** `RecoveryInspectionCursor` streams verified records under the approved package, metadata, journal, record-count, line-size, directory-entry, and depth ceilings. Sheets startup preflights both recovery stores while holding their locks, replays the cursor records into an unpublished workbook candidate, and finalizes that same cursor only after the candidate is valid. Checkpoint payload and metadata reads are bounded through opened file handles. The legacy journal decoder caps payload collection at 256 MiB; an oversized final committed record now fails closed and preserves the old journal bytes, while malformed final JSON retains torn-tail tolerance.

**Stage 4B verification:** RED `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets oversized_final_legacy_record --locked --offline -- --nocapture --test-threads=1` reproduced silent omission with `must fail closed: None`; GREEN `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets final_legacy_record --locked --offline -- --nocapture --test-threads=1` passed 2/2. `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets cell_edit_recovery_tests:: --locked --offline -- --nocapture --test-threads=1` passed 25/25; `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets recovery_policy::tests:: --locked --offline -- --nocapture --test-threads=1` passed 13/13. `cargo test --manifest-path loom-core/Cargo.toml -p loom-production --locked --offline` passed 62/62 with 0 doc tests. `cargo clippy --manifest-path loom-core/Cargo.toml -p loom-production --locked --offline --all-targets -- -D warnings` and `cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` passed. `cargo fmt --manifest-path loom-core/Cargo.toml --all -- --check`, `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check`, `python3 loom-bootstrap/scripts/audit-governance.py`, `python3 loom-bootstrap/scripts/audit-assets.py`, `cargo build --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --offline`, and native `git diff --check` passed. The code-structure audit reports 17 failures in locked or out-of-scope files; active recovery files pass, including `loom-production/src/lib.rs` at 63,714 bytes. Evidence logs: `.work/sheets-acceptance-2026-09-26/rec02-legacy-overlimit-red.log`, `rec02-legacy-overlimit-green.log`, `rec02-stage4b-focused-recovery-final.log`, `rec02-stage4b-policy-after-split.log`, `rec02-stage4b-production-tests-final.log`, `rec02-stage4b-production-clippy-after-split.log`, `rec02-stage4b-sheets-clippy-after-split.log`, `rec02-stage4b-app-build.log`, `rec02-stage4b-core-fmt.log`, `rec02-stage4b-sheets-fmt.log`, `rec02-stage4b-governance.log`, `rec02-stage4b-assets.log`, `rec02-stage4b-diff-check.log`, and `rec02-stage4b-structure-after-split.log`.

The full Sheets workspace command `cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline -- --test-threads=1` exits 1: 240 passed and the same seven accessibility/menu tests fail as in `.work/sheets-acceptance-2026-09-25/rec02-stage3-parent-app-tests.log`; names were compared and are identical. Current output: `.work/sheets-acceptance-2026-09-26/rec02-stage4b-sheets-workspace-tests-after-split.log`. Independent review confirmed same-cursor record digest validation and the legacy-record fix; report: `.work/sheets-acceptance-2026-09-26/rec02-stage4b-independent-review.log`. Remaining P2: a process that ignores the recovery lock can replace `operations.jsonl` after inspection and before torn-tail repair. Windows/macOS locking and replacement behavior remain unverified. REC-02 stays OPEN.
**Stage 4B source-size follow-up (2026-09-26):** extracted the policy unit tests into `recovery_policy_tests.rs`; `recovery_policy.rs` is 17,856 bytes / 498 lines and the test module is 13,903 bytes / 373 lines. The policy tests pass 13/13, Sheets workspace all-target Clippy and formatting pass. The code-structure audit still reports 17 pre-existing findings outside active Sheets/REC-02 sources; active recovery files remain within the source budgets. Evidence: `.work/sheets-acceptance-2026-09-26/rec02-stage4b-policy-module-split-tests-final.log`, `rec02-stage4b-policy-module-split-clippy.log`, `rec02-stage4b-policy-module-split-fmt-final.log`, and `rec02-stage4b-policy-module-split-structure.log`.
**Legacy recovery lock-lifetime follow-up (2026-09-26) — NEEDS_REVIEW.** `CellEditRecovery` now retains the legacy directory lock for its full lifetime, so a second recovery/migration session cannot acquire the same exclusive lock while this recovery object is active. Migration no longer tries to reacquire its own lock. Under WSL/Linux, `cell_edit_recovery_tests::` passed 19/19, app all-target Clippy with `-D warnings` passed, and workspace formatting passed. Native Windows lock behavior remains unverified.

**REC-02 Stage 4C1 — bounded cell batches and checkpoint fallback (2026-09-26) — NEEDS_REVIEW.** `CellEditRecovery::record_cells` now uses `RecoveryJournal::append_bounded` for cell batches within the record payload ceiling. A larger serialized batch or an append refusal caused by the record, journal-byte, or record-count ceiling requests a complete current-workbook package and publishes it as a checkpoint; it does not split the batch or increment the durable sequence. The regression covers a 300,000-character cell value after one durable edit: recovery contains the full edited checkpoint, its durable sequence stays at 1, and the next small edit is sequence 2. Independent review found a P1 policy gap: this ordinary checkpoint path reaches `RecoveryJournal::checkpoint` without `recovery_policy::preflight_checkpoint`, so it can publish a package over 256 MiB or exceed the 640 MiB retained and 1 GiB temporary-peak caps. Review also confirmed the worker applies edits to its candidate before recovery and continues formula evaluation after recovery failure; admission is not paused. The proposed package-limit regression was removed before commit because its migration-only test override did not cover this ordinary checkpoint path. Do not treat Stage 4C1 as cap enforcement or REC-02 completion.

**Stage 4C1 verification:** RED `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets oversized_cell_batch_uses_checkpoint_without_advancing_durable_sequence --locked --offline -- --nocapture --test-threads=1` reproduced loss of the oversized edit in the old append path; the same regression passed 1/1 after the fallback (`rec02-stage4c1-red.log`, `rec02-stage4c1-green.log`). After removing the invalid package-limit case, `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets cell_edit_recovery_tests:: --locked --offline -- --nocapture --test-threads=1` passed 26/26 and `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --bin loom-sheets workbook_worker::tests:: --locked --offline -- --nocapture --test-threads=1` passed 20/20 (`rec02-stage4c1-final-focused-recovery.log`, `rec02-stage4c1-final-worker-tests.log`). `cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings`, `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check`, `python3 loom-bootstrap/scripts/audit-governance.py`, and `python3 loom-bootstrap/scripts/audit-assets.py` passed (`rec02-stage4c1-clippy.log`, `rec02-stage4c1-final-fmt.log`, `rec02-stage4c1-final-governance.log`, `rec02-stage4c1-assets.log`). Native `git diff --check` exited 0; Git emitted LF-to-CRLF notices. The workspace test command `cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline -- --test-threads=1` passed 241 tests and failed seven existing accessibility/menu tests; exact names match the recorded baseline (`rec02-stage4c1-workspace-tests.log`, `rec02-stage4c1-baseline-failure-comparison.log`). The repo-wide code-structure audit has 17 findings in locked/out-of-scope files and none in the changed recovery modules (`rec02-stage4c1-structure.log`). Review findings and the missing cap/admission work keep this milestone NEEDS_REVIEW.

**Stage 4C1 checkpoint:** Code and verification above are committed as `d7561b58fa46c68f69bd9f3009512ac093796914` and pushed to `origin/main`. On 2026-09-26, `git ls-remote origin refs/heads/main` returned the same commit; the local worktree was clean. Stage 4C2 began on 2026-09-26; the current outcome is recorded below.

**REC-02 Stage 4C2 - ordinary write-side capacity preflight (2026-09-26) - NEEDS_REVIEW.** `RecoveryJournal::checkpoint_and_compact_with_preflight` now accepts the exact covered sequence, checks it against the verified journal frontier/current checkpoint under `.checkpoint.lock`, and compacts only covered records while retaining newer edits. Before the callback or mutation, the transaction projects retained and temporary encoded bytes plus final/peak directory entries, counting new generation payload/metadata, pointer/journal growth, exact two-generation pruning, and the two scanner-visible entries created by one atomic-write staging directory and child file. Generation discovery and legacy-predecessor scans are hard-clamped to 10,000 entries; abandoned atomic-write trees are bounded to 10,000 entries and depth 32. Sheets now invokes the package/retained/temp preflight for ordinary append and complete-checkpoint paths. Record/journal-limit or explicit aggregate-capacity refusal of a cell batch requests a whole-workbook checkpoint without splitting or advancing sequence; unsupported legacy entries and other inventory/I/O failures remain fail-closed. Tests cover refusal without callback/mutation, torn-tail and sequence preservation, fitting checkpoint fallback and recovery, two-generation retention, exact-fit/one-under directory bounds, discovery limits, and sequence races. An unrelated close-save journey fixture was corrected to keep its output outside recovery storage; production behavior is unchanged. Independent core review found no Stage 4C2 blocker under the documented cooperating-lock contract. Implementation is committed as `c6d36d6a20ff8689924788707ff0d71328adeedb` (`fix(sheets): preflight ordinary recovery writes`); this status record is committed and pushed with it. **Verification:** RED/GREEN entry-bound regressions are in `rec02-stage4c2-entry-bounds-red.log` and `rec02-stage4c2-entry-bounds-green.log`; the core production suite passed 72/72 (`rec02-stage4c2-production-tests-final.log`), Sheets cell recovery passed 31/31 (`rec02-stage4c2-sheets-recovery-tests-final.log`), and recovery policy passed 13/13 (`rec02-stage4c2-policy-tests-final.log`). The full Sheets workspace command passed 246 tests and failed the same seven accessibility/menu tests as the Stage 4C1 baseline; the failure names match exactly (`rec02-stage4c2-sheets-workspace-after-fixture.log`, `rec02-stage4c1-workspace-tests.log`). The former extra close-save failure was reproduced then passed 1/1 after the fixture correction (`rec02-stage4c2-close-fixture-red.log`, `rec02-stage4c2-close-fixture-green.log`). Production app build, strict production Clippy, Sheets all-target Clippy, formatting, governance, and asset audits passed; the code-structure audit still reports 17 findings in locked/out-of-scope files and none in active recovery sources. `loom-production/src/lib.rs` is 60,520 bytes, below its 65,536-byte limit. Evidence: `rec02-stage4c2-app-build-final.log`, `rec02-stage4c2-production-clippy-final.log`, `rec02-stage4c2-sheets-workspace-clippy-postfixture.log`, `rec02-stage4c2-core-fmt-final.log`, `rec02-stage4c2-close-fixture-fmt.log`, `rec02-stage4c2-governance-postdoc.log`, `rec02-stage4c2-assets-postdoc.log`, and `rec02-stage4c2-structure-postfixture.log`. The recursive `remove_dir_all` cleanup remains vulnerable if a process ignores `.checkpoint.lock` and mutates a planned tree; Windows/macOS locking and pointer replacement, disk-full/interruption injection, real restart data comparison, and measured durability latency remain unverified.

**Remaining Sheets/REC-02 work:** integrate recovery checkpoints at the approved 16 MiB, 2,000-record, or five-minute cadence; pause edits when bounded recovery cannot keep up; make Retry and Save As outcomes truthful; add disk-full/interruption and real restart/data-comparison coverage; measure the approved 250 ms p95 edit durability target and the remaining performance paths. Verify Windows/macOS lock and pointer-replacement behavior, native accessibility, and live-window interactions before evaluating the Sheets acceptance gate. REC-02, PERF-01, and Sheets acceptance remain OPEN.

**Sheets visual repairs (2026-09-25):** UI-28 is fixed. The XLSX warning grows to fit short messages, stops at the window limit, and scrolls only its warning text when long. Buttons stay visible after the scroll area, and Escape still means Cancel. Check the three `ui28-xlsx-warning-*.png` renderer captures and `ui28-xlsx-warning-live-linux.png` before changing this dialog.

UI-29 fixes the menu rows that looked centered and uneven. The reusable row is `LoomMenuItem` in `loom-core/crates/loom-ui/ui/foundation/overlays.slint`, exported by `loom-core/crates/loom-ui/ui/foundation.slint`. Other Loom apps must import this shared control instead of drawing their own menu row. Use separate `label` and `shortcut` properties; the component puts the label at the reading edge, the shortcut at the other edge, and the check mark in its own slot. Do not join label and shortcut into one string. To repair the popup, use `popup-layout.preferred-height` with a window-height cap instead of giving every small menu a fixed 520 px panel. Check the Overlays gallery example and `ui29-menu-popup-aligned-1024x720-linux.png`. Native popup interaction is still an open UI-01 check.

For a tiny coding model checking recovery: count the bytes kept after 1, 10, and 100 unsaved edits, then check how long the newest edit takes to reach durable recovery. Use the owner-approved REC-02 format and limits below. Save and verify a newer complete checkpoint before deleting only the older records it covers. Restart after each possible interruption and compare the recovered cells, formulas, tabs, active tab, and images with the user's last accepted workbook. A disk-full or journal error must be shown to the user. Follow the numbered REC-02 implementation stages; do not weaken the approved bound or failure guarantees.

For a tiny coding model fixing Sheets responsiveness: after the user commits a cell, show “Calculating…” right away. Do formula work, recovery-file writing, and normal file work on a background worker. Do not copy the whole workbook on the UI thread and call it fixed: time the copy too. Keep only one job running and one newest job waiting. Put a number on each edit; when a job finishes, show its result only if its number is still the newest and the same tab is open. Do not erase a formula the user is currently typing. Keep the old result visibly marked until the new result is ready. Measure the full input callback and the first visible frame separately; timing only edit preparation and queue submission is not enough. Test fast consecutive edits, tab switching, undo, recovery failure, and the real Linux window before marking the card fixed.

## Owner override — 2026-09-18 Sheets then Writer completion run

The owner explicitly authorized advancing the workflow to Sheets and completing the recorded Sheets findings and acceptance gate, then doing the same for Writer. Work remains serial: finish Sheets, prove its binary gate, commit and push each coherent milestone, then advance the workflow to Writer. This override includes the remaining Sheets P2/UI findings, focused regression coverage, acceptance evidence, and the source-size maintenance needed for a clean Sheets gate. Keep later applications locked until their turn. Do not mark an app `ACCEPTED` from a green unit test alone; record the required native, visual, accessibility, and persistence evidence.

## Owner override — 2026-09-25 Sheets continuation branch

The owner explicitly directed this continuation to use `main`, overriding the active-branch default below for this run. The checkout contains only `main` and `origin/main`; keep this Sheets work on `main`, and continue committing and pushing the authorized Sheets milestones serially. The workflow remains in the `sheets` phase and later applications remain locked.

## Start here — one small repair at a time

The owner requested the 2026-09-14 code and UI/UX audit and these repair instructions. Findings and exact repair cards live in the [current-truth section](AGENTS.md#current-truth); each card keeps its original finding and records its current repair result. New findings start OPEN.

For a small local coding model, including a model with a tiny context window:

1. Read this section, the active gate and repair order in the [current-truth section](AGENTS.md#current-truth), and `loom-bootstrap/contracts/workflow.toml`. If they disagree, report the exact disagreement before editing product code.
2. Normally select **one** OPEN card allowed by the active gate. Start with CODE-01. During the dated owner overrides above, the owner has already authorized the listed P0/P1 cards and the active application's remaining cards, so keep the same one-card boundaries inside each milestone while working through that explicit list. Read only the current card and its named source functions, their callers, and applicable rules here. Do not ask a tiny model to repair the entire suite in one prompt.
3. Say what the user does, what goes wrong now, and what must happen instead. Copy the card's acceptance check into a focused regression test or a short manual interaction script. For a data-loss fix, first run the test and see it fail for the reported reason.
4. Make the smallest coherent change. A card with several steps may need several patches. Do not mix formatting, renaming, feature work, and a bug fix. Preserve unrelated uncommitted work.
5. Run the failing example again and the affected module's existing tests. For a UI change, also build the actual app, perform the interaction, save a fresh screenshot, and inspect it at the card's sizes/themes. A callback-only journey does not prove native focus or keyboard behavior.
6. Check the real result: compare saved/reopened data, inspect an independently opened export, or verify the changed visible state. A success message or a green build is not proof.
7. Record changed files, exact commands, observed result, evidence path, and remaining gaps under that card. Mark FIXED only when its check passes. Mark NEEDS_REVIEW when the code changed but a required check or expert review is missing. Do not delete the original finding.
8. Normally stop after this card. Do not change acceptance, unlock the next app, refresh visual baselines, or take the next card automatically unless the current task or the owner override above authorizes it.

Useful starting commands (run from the repository root; substitute the one named app/package):

```bash
rg -n '^### CODE-01|^### CODE-02' AGENTS.md
rg -n 'fn open|next_seq|checkpoint' loom-core/crates/loom-production/src/lib.rs
cargo test --manifest-path loom-core/Cargo.toml -p loom-production --locked
python3 loom-bootstrap/scripts/audit-governance.py
```

Read one complete card between its `###` heading and the next `###` heading. The ignored `.work/` reports contain supporting evidence, not a second task queue. If an old probe is missing, rebuild the tiny fixture described in the card; do not mark it passed from an old log.

**Small models need bounded tasks, not weaker standards.** CODE-02, CODE-08, CODE-09, and CODE-15 involve atomic file operations, security boundaries, or concurrency. Split them into reproducer, implementation, failure-injection checks, and experienced review. If you cannot explain every failure path, leave NEEDS_REVIEW with the precise gap. Do not guess. Never “fix” data loss by dropping data, ignoring an error, deleting a journal, weakening a test, or turning a feature into a success-message stub.

## 1. Product and technology lock

Loom is an original, local-first, commercial-quality creative suite.

The product stack is locked to:

- Rust stable for product logic and native hosts.
- Slint for the application UI.
- Original Loom visual identity and interaction language.
- Local-first/offline-capable workflows; no mandatory account, telemetry, hidden upload, or remote inference.
- Cross-platform desktop targets: Linux x86-64, Windows x86-64, macOS arm64, and macOS x86-64.
- Commercially redistributable assets and dependencies only.

Do **not** introduce Electron, Qt, GTK, web UI, Tauri, Flutter, another GUI toolkit, or a second application language without explicit owner instruction.

The active integration branch is `cline-implementation`. Do not create long-lived feature branches, duplicate roadmaps, issue sprawl, or alternate sources of truth.

## 2. Authority and repository hygiene

`AGENTS.md` is the one authoritative Markdown document. Its constitution appears first, its live product state and repair cards are inside the marked current-truth section, and the remaining source-path sections preserve reference documents and history. Project and subproject `README.md` files stay in their folders as public-facing introductions.

Machine-readable contracts under `loom-bootstrap/contracts/` and `loom-design-bible/contracts/` are normative where this file delegates exact values to them.

Keep all non-README Markdown content in this file. Do not create another `.md` or `.markdown` document, including under `.work/`; use a log or other non-Markdown evidence file for temporary output. Never add another `AGENTS.md` below the repository root. Dated reports, prior truth snapshots, and old plans are preserved under their original-path headings but remain historical; update the current-truth section for live status.

Delete obsolete agent residue instead of preserving it "for context". Stale instructions are defects because they mislead future agents.

## 3. Active workflow and audit repair gate

The current phase and active repair scope are recorded together in the [current-truth section](AGENTS.md#current-truth) and `loom-bootstrap/contracts/workflow.toml`. Do not infer them from old acceptance prose or this file's history.
The dated owner overrides at the top of this file are explicit exceptions for the 2026-09-15 P0/P1 run and the 2026-09-18 Sheets-then-Writer completion run. The latter authorizes the recorded phase transition and serial application edits while preserving the binary acceptance gates.

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

## 5. Shared UI foundation acceptance

The shared component library must be designed and proven **before** it is adopted by any application.

The canonical entry point for new components is `loom-core/crates/loom-ui/ui/foundation.slint`. New shared components must be small, composable, token-driven, keyboard-operable, accessible, and demonstrated in the foundation gallery.

The gallery must demonstrate every reusable component across the states relevant to that component, including:

- light, dark, and high-contrast themes;
- normal, hover, pressed, focused, selected/checked, disabled, and destructive states where applicable;
- short and long labels;
- keyboard operation and visible focus;
- supported desktop viewports: 1024×720, 1280×800, 1440×900, and 1920×1200;
- text scaling at 1.0, 1.25, and 1.5 where supported by the host contract;
- right-to-left direction where layout direction matters;
- empty, loading, populated, error, and overflow states where applicable.

### Mechanical foundation gate

All of the following must pass:

- zero unintended overlap;
- zero clipped or ellipsized action/control labels;
- no inaccessible icon-only action without an accessible label and tooltip contract;
- no toolbar wrapping or horizontal scrolling;
- no application-local palette, spacing, radius, typography, or standard control geometry;
- keyboard activation for every interactive primitive;
- visible focus treatment;
- truthful disabled states;
- no placebo callbacks;
- responsive behavior at every required viewport;
- light/dark/high-contrast rendering;
- component source within code-quality budgets;
- no unlicensed visual assets.

### Visual foundation gate

Mechanical CI is necessary but **cannot certify visual quality**.

The foundation is accepted only after reviewed gallery screenshots are judged production-quality and the current-truth section records explicit human acceptance. A deterministic screenshot is not automatically a good screenshot.

Do not create or refresh a visual baseline until the represented design has been accepted. Never change a baseline merely to make a regression test pass.

## 6. Visual design rules

Loom must look like a professional desktop creative product, not an admin dashboard, demo app, component playground, web form, or collection of rounded cards.

Hard rules:

- Content/workspace is visually dominant; chrome is subordinate.
- Use one coherent hierarchy, not repeated title bars, nested cards, redundant labels, or duplicate document names.
- Standard controls use exact shared tokens. Do not invent one-off geometry.
- Action labels must remain readable; user content names may ellipsize when necessary.
- Avoid decorative borders, pills, shadows, and accent color unless they communicate hierarchy or state.
- Do not use strong zebra striping by default for spreadsheet cells, timelines, lists, or inspectors.
- Iconography must be stylistically consistent and legible at the canonical icon sizes.
- Toolbars contain only frequent commands. Rare commands belong in menus/overflow, not permanent chrome.
- Empty space is intentional workspace, not dead layout caused by a fixed-size editor surface.
- Canvas/timeline/spreadsheet status UI must not obscure editable content.
- Selection is clear but not visually louder than the content itself.
- Animations must be short, purposeful, cancellable by reduced-motion policy, and never required to understand state.

Exact geometry, token, and responsive values live in the machine-readable design contracts. Agents must use those values rather than approximate them from screenshots.

### Turn the audit into concrete design work

Apply the specific UI cards in the current-truth section, one at a time. These rules describe the required result:

- Protect work first. New/Open/Close must handle dirty documents explicitly. Save/Export/Recovery must preserve the actual content, not just show a reassuring message.
- Make basic actions findable in the app on every supported desktop. A memorized shortcut, toolbar icon, overflow button, or command palette is not a menu bar.
- Menu placement is simple: use a global menu only when the app really installs it and the desktop actually shows it. Otherwise draw a menu bar inside the app window.
  1. Never hide the local menu because a menu descriptor was created or DBusMenu data can be generated. Those facts do not prove a desktop displays a global menu.
  2. If there is no API that confirms a visible global menu, keep the local menu visible. In the current Sheets code, the native installer calls the OS menu API on macOS only; Linux DBusMenu is not connected to a desktop host.
  3. Keep the local menu connected to the same command IDs, enabled/checked state, and action handlers as the other controls. Show real commands only; hide empty menus and unsupported placeholder items.
  4. Check the local menu on a desktop without a global menu at 1024, 1280, and 1440 px. Check the global menu separately on a platform that really hosts it.
- Start with an honest empty document or clearly named example. Example data must make sense. Never start a real job from a made-up source path.
- Give the document the space promised by its contract. Close optional inspectors by default where required. Do not create a large blank panel just to fill the window.
- At narrow sizes, reflow template choices and move low-priority commands into a reachable overflow. Keep names and primary actions readable. Never solve clipping by making text tiny, hiding actions with no replacement, or making the whole toolbar scroll.
- Use foreground/background token pairs. Check selected cells, headers, fields, disabled controls, and error text in high contrast. A dark toolbar alone does not prove that the document surface has a working dark mode.
- Feedback must be visible. Render status/error state that the controller produces; keep it outside editable content. Say what failed and how to retry. Do not put developer types, placeholder paths, or internal implementation slogans in ordinary product copy.
- A preview must create the item it shows. To make a chart, ask the user for two columns and a range (for example `A1:B4`). Show that range, the value-column name, and its unit. Keep those same cells linked after Save/Open and redraw after edits or Undo. Include a Total row only when the user selected it. User-inserted objects may cover cells; temporary messages must not cover them.
- Template identity comes from a stable ID, never a card's list position. One shared descriptor must connect the displayed name, preview, and actual generated document. An unavailable template must not silently create something else.
- UI theme changes may recolor application chrome, not document artwork. Preview and export must share document geometry/content. Fit-to-window must show every image corner, with selection and pointer coordinates using the same rectangle.
- Compact windows must retain access to formatting and review controls. A hidden inspector needs an operable replacement route. Comments need visible anchors tied to their real ranges; a raw Markdown insertion must be labeled as text until visual table editing works.
- Precise properties need typed numeric entry with units, commit/cancel, and undo. Distinguish timeline zoom from scrolling. Show real sample peaks/time positions where an editor asks users to cut audio; never draw fictional media detail.
- Give each transport action one primary control. Show progress for actual jobs, derive readiness from real prerequisites, and provide a readable way to resolve missing local tools. Keep one authoritative editing location per setting.
- Keep visible keyboard focus and meaningful accessible names. Screenshots can reveal unreadable text; they cannot prove tab order, screen-reader output, or a complete keyboard workflow. Record untested accessibility work explicitly.

## 7. No facade-first development

A visible enabled control may exist only when its operation has real semantics.

A feature is not implemented because:

- a callback exists;
- a status string changes;
- a screenshot looks convincing;
- a model type or command enum exists;
- an exporter writes a syntactically valid but semantically incomplete file;
- a CLI path exercises less than the GUI implies;
- a test checks only launch, strings, source tokens, or element presence.

Visible functionality must be wired to the real domain operation, have correct selection/context semantics, participate in undo/redo when applicable, persist correctly, expose truthful failures, and have end-to-end evidence.

## 8. Code-quality constitution

Agent-generated verbosity is a defect. Prefer small modules with one responsibility over monolithic files, giant match statements, duplicated wrappers, compatibility aliases, and speculative abstractions.

Machine-enforced limits live in `loom-bootstrap/contracts/code-quality.toml`.

General rules:

- New Rust source files: maximum 64 KiB and should normally remain below 800 lines.
- New Slint files: maximum 32 KiB and should normally remain below 500 lines.
- New Python/shell QA utilities: maximum 32 KiB and should normally remain below 600 lines.
- Application `main.rs` is startup/composition code, not the application architecture.
- `lib.rs` is a module boundary, not a dumping ground.
- Prefer explicit domain modules over `utils`, `helpers`, or `common` dumping grounds.
- Avoid duplicate types representing the same concept.
- Avoid forwarding wrappers that add no semantics.
- Do not retain compatibility aliases once all call sites have migrated.
- Do not add dead code, speculative extension points, or feature scaffolding without an active requirement.
- Public APIs require a stable responsibility; do not make internals public for convenience.

Existing oversized files are registered legacy debt with a byte ceiling. They may not grow. A meaningful modification to an oversized file must either reduce it or extract coherent responsibility into smaller modules.

Warnings are failures in CI. New `allow(dead_code)`, `allow(unused_*)`, broad Clippy suppressions, and ignored failing tests require explicit justification in the same change and should normally be rejected.

## 9. Rust architecture

Dependency direction is:

```text
Slint view
  -> application controller / typed commands
  -> application domain engine
  -> narrow loom-core services
  -> platform adapters / optional backends
```

Rules:

- Slint does not own domain truth.
- UI callbacks dispatch typed operations; they do not perform complex file/media/audio/model work inline.
- Domain logic must be testable without creating a window.
- Platform-specific behavior sits behind narrow interfaces.
- Persistence, undo, recovery, jobs, interop, and media runtime must not be reimplemented independently in each app when a shared service already owns the responsibility.

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

## 11. Asset and licensing rules

Loom must remain safe to sell commercially.

Every shipped visual/audio/font/template asset must have explicit provenance recorded in `loom-bootstrap/contracts/assets.toml` or be produced by Loom itself under a repository-owned license.

Allowed sources include:

- original Loom-created assets;
- CC0/public-domain assets;
- SIL Open Font License fonts;
- MIT/BSD/Apache-2.0 or similarly permissive assets when their redistribution/attribution terms are satisfied.

Forbidden sources include:

- unknown-license downloads;
- "free for personal use" assets;
- non-commercial Creative Commons licenses;
- editorial-use-only assets;
- scraped icons, screenshots, photos, fonts, sounds, or templates;
- proprietary Apple, Adobe, Microsoft, Google, or other commercial product artwork copied from their applications;
- assets whose license cannot be documented.

When in doubt, do not add the asset. Original generated vector artwork is preferred over ambiguous third-party downloads.

## 12. Current truth discipline

The marked current-truth section in `AGENTS.md` is the only live readiness ledger.

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


## current-truth

<!-- CURRENT TRUTH START -->
# Loom — Current Truth

This is the live product ledger and repair queue. `AGENTS.md` defines the rules; `loom-bootstrap/contracts/workflow.toml` records the work gate. Updated 2026-09-25 from the code audit, fresh UI/UX inspection, owner-authorized P0/P1 repair run, native Sheets self-audit, Sheets acceptance follow-up, and a source audit of recovery retention and XLSX import behavior. Finding text stays here so a future repair can be checked against the original failure.

## Active gate — read this before choosing a card

```text
ACTIVE PHASE: SHEETS
FOUNDATION STATUS: ACCEPTED
SUITE STATUS: ACCEPTANCE_BLOCKED
APPLICATION DEVELOPMENT: UNLOCKED
ACTIVE APPLICATION: SHEETS
NEXT APPLICATION: WRITER
```

The foundation's ACCEPTED value is the pre-existing contract record, not a new approval from this audit. Existing consumer imports remain compatible; the active application phase is now Sheets. The current visual audit does not recertify the gallery or supply human acceptance. Recheck gallery evidence and human sign-off before advancing application design. Do not delete or refresh existing baselines to hide findings.

**Machine gate:** the workflow is now in the serial Sheets phase. **Owner override (2026-09-18):** the owner authorized completing Sheets to its binary acceptance gate, then advancing to Writer. The earlier P0/P1 repairs remain recorded below; this phase does not grant application acceptance. Keep native, visual, accessibility, and persistence evidence open until each gate is actually proven.

The contract's allowed prefixes are an outer file boundary for the active Sheets phase, not permission to edit later applications. A newly supported repair stage needs a reviewed update to `loom-bootstrap/scripts/audit-governance.py` and its focused tests; the validator now covers the recorded Sheets phase and its serial predecessors.

**Repair order:** finish the remaining Sheets findings and acceptance evidence, mark Sheets `ACCEPTED`, then advance the workflow to Writer. Within each app, data loss and broken output come before visual polish. No later app starts until the current app passes its full gate.

**How to use a card:** follow the numbered steps in `AGENTS.md` under “Start here.” Normally select one card, reproduce it, make a small repair, run its concrete check, and record evidence under that same card. The owner overrides authorize this dated P0/P1 run and the active Sheets completion run. States: OPEN, NEEDS_REVIEW, FIXED. FIXED does not mean the whole application is ACCEPTED. P2 and GOV cards remain OPEN unless their own record says otherwise.

## Current product state

Loom is a local-first Rust + Slint functional alpha. It has useful domain engines and real editing features. The audit found reproducible data loss, corrupt or incomplete exports, broken recovery, and misleading UI states. The owner-authorized repair run has fixed the recorded P1 code cards and UI-14. UI-25 compact recovery layout and action-matching guidance are fixed and tested in code; real playback/encode acceptance remains open because no media backend or sample media is installed here. Sheets has native evidence for saved/dirty titles, chooser focus, keyboard selection/create/cancel, the named export control, and visible formula/save-error/cancel feedback with one-time Orca announcements. The live Linux window shows the in-window File/Edit/View/Table/Help row and the CODE-23 warning dialog. The screensaver was inactive during that capture. Popup interaction and focus behavior remain unverified because this computer-use session exposes no native-app controls. The Sheets evaluator, array spill, XLSX-name collision, style-linked XLSX export, bounded DEFLATE XLSX import, revision-safe asynchronous cell-commit regressions, asynchronous picker/startup Open parsing, asynchronous CSV/XLSX export, and revision-safe close draining are fixed and tested in code. Worker input failures remain visible and protect unsaved work until a successful full resync. A LibreOffice Calc 24.2.7.2 XLSX→ODS→XLSX→Loom round-trip recovered four sheets, formulas, the tested cell formatting, a chart, and a shape label; custom row/column sizes were reported as an import warning. Microsoft Excel and the wider interchange matrix remain unverified. CODE-23 now reports known unsupported XLSX features and requires confirmation before import replaces the current workbook, including at startup; automated detection, cancel, continue, and recovery-preservation tests pass, while live dialog focus and action behavior remain unverified. REC-02's design and bounds were approved by the owner on 2026-09-25, but its implementation remains open and unsaved recovery packages can still accumulate without a storage bound. Sheets' full acceptance gate also remains blocked on native visual review and Open/close interaction and timing evidence, recovery freshness for very large workbooks, non-cell full-workbook copies, export-scale performance, wider interoperability, complete visual/accessibility coverage, native million-cell scrolling, and a reviewed memory budget. No application is certified by this audit as a professional replacement for mature creative software. The old 38/100 score and claims of complete Sheets acceptance are superseded; there is no defensible fresh numerical readiness score.

Quality and permission to work are different. The owner override permits the active Sheets completion work while the remaining applications stay locked until their turn. This is the single live application status table:

| Order | Application | Product status | Work status | Current blocking evidence |
|---:|---|---|---|---|
| 1 | Sheets | ACCEPTANCE_BLOCKED | IN_PROGRESS | Native menu-popup, warning-dialog, and close interactions; Open worker shutdown and live timing; slow and unbounded recovery storage; non-cell full-workbook copies and export-scale performance; full visual/accessibility coverage; broader interoperability; native million-cell scroll; reviewed app memory budget |
| 2 | Writer | ACCEPTANCE_BLOCKED | LOCKED | P1 code/UI repairs landed; CODE-17 and visual/manual checks remain |
| 3 | Present | ACCEPTANCE_BLOCKED | LOCKED | CODE-04 repaired; CODE-16/19 and visual checks remain |
| 4 | Photo | ACCEPTANCE_BLOCKED | LOCKED | CODE-04/UI-16 repaired; CODE-14, UI-14/18 and visual checks remain |
| 5 | Motion | ACCEPTANCE_BLOCKED | LOCKED | UI-17 still-frame parity verified; UI-14/18 and full video-render acceptance remain |
| 6 | Video | ACCEPTANCE_BLOCKED | LOCKED | UI-25 repaired in code; UI-19/20 and real-media checks remain |
| 7 | Studio | ACCEPTANCE_BLOCKED | LOCKED | CODE-10 repaired in code; audio and visual acceptance checks remain |
| 8 | Encode | ACCEPTANCE_BLOCKED | LOCKED | CODE-09/UI-23 repaired in code; filesystem/media checks remain |

The previous ledger listed Present/Photo both LOCKED and ACCEPTED and said Sheets had no known serious defects. Those statements are withdrawn. Historical test counts and agent-reviewed screenshots do not override the open findings below. The pre-update ledger is retained as an ignored audit backup, not a competing authority.

## Recorded audit and evidence

The first audit was a code/reliability audit; it explicitly did **not** certify UI/UX. The follow-up adds fresh rendered screenshots from all eight apps and isolated native Linux interactions in Sheets and Writer. The observations are durable in CODE-01 through CODE-19 and UI-01 through UI-29 below, including reproduction instructions so they remain usable if `.work/` is removed. UI cards distinguish reproduced failures from source-confirmed limitations and visual design recommendations.

- Original detailed code report: [.work/audit-2026-09-14/AUDIT.md](AGENTS.md#source-work-audit-2026-09-14-audit-md).
- Fresh visual report and screenshots: [.work/uiux-audit-2026-09-14/AUDIT.md](AGENTS.md#source-work-uiux-audit-2026-09-14-audit-md). Build/capture commands and limitations are recorded with that report.
- Portable audit evidence bundle, including the code and UI/UX reports, historical audit evidence, the 2026-09-22 Sheets acceptance report/UI-14 evidence, the expanded 2026-09-23 native screenshot/interaction report and screenshots (including UI-06 status/error captures), the UI-01 renderer preview, UI-17/UI-25 repair evidence, and the updated 2026-09-24 Sheets acceptance report with recovery-storage, XLSX-loss warning, and compressed-import findings; package and workspace tests; Clippy/build logs; viewport and XLSX-warning red/green regressions; the Sheets source-split audit and Save Changes Escape red/green logs; million-cell edit timings; formula benchmark; REC-02 proposal; renderer screenshot; LibreOffice fixtures; and the Calc-generated XLSX re-import probe. It also contains the 2026-09-25 UI-28/UI-29 report addendum and five warning/menu screenshots, including the shared reusable LoomMenuItem example. All non-README Markdown reports in the bundle are consolidated in its root AGENTS.md. It contains CODE-24 and CODE-25 evidence, including DEFLATE and data-descriptor fixtures, the imported workbook and probe log, and the current 267-test Sheets workspace / 29-test package results: [loom-bootstrap/audit-evidence-2026-09-14-and-22.zip](loom-bootstrap/audit-evidence-2026-09-14-and-22.zip) (SHA-256 `b6b5d0d2c5e17b6b643b19ccf6e23c2915736db28a7f4772cc55570c9920427a`; `unzip -t` passed). The AGENTS.md copy inside the archive records the bundle hash that was current when it was built; an archive cannot contain its own final hash. Fresh native evidence includes a screenshot of the live app with the visible in-window menu row and CODE-23 warning dialog at `loom-sheets/docs/qa-native/ui23-xlsx-import-warning-live-linux.png`; menu-popup activation and keyboard/focus behavior remain unverified.
- Native Sheets self-audit: [.work/sheets-acceptance-2026-09-23/REPORT.md](AGENTS.md#source-work-sheets-acceptance-2026-09-23-report-md) records live `gnome-screenshot` captures plus native keyboard, AT-SPI, and Orca checks at 1024×720. UI-06 status/error captures are also checked into `loom-sheets/docs/qa-native/`. The 1440×900 native capture remains unavailable on this 1366×768 desktop; other themes/viewports use existing renderer evidence. The latest shared ZIP reader change passes 266 Sheets workspace tests, 29 `loom-package` tests, full-workspace all-target Clippy, and the production app build. The build took 3m07 and peaked at 2,985,580 KiB RSS with zero swaps. The Calc-generated XLSX re-import probe is in the refreshed bundle. A fresh native capture shows the visible menu row and CODE-23 warning dialog; popup/menu actions, focus, and keyboard behavior remain unverified because no native-app control API is available in this session: [tracked report](AGENTS.md#source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md).
- Audit basis: commit `8fce782` plus the existing uncommitted Sheets implementation. That sentence describes the historical audit only; the owner-authorized repair commits listed below subsequently changed application behavior.
- Verified existing tests in the code audit: shared core 123, Sheets 98, Writer 77, Present 49, Photo 49 — **396 passing tests**. The four source/governance audits also passed before the documentation update. Three new focused recovery tests failed as intended, demonstrating CODE-01/02/18. Passing existing tests did not prevent these defects.
- Plugin and encode probes used controlled adapters, not real Wasmtime/codec runs. Source traces are labeled separately from executable probes. The original image-recovery probe tests payload transport; CODE-11 requires a real decoded-image regression too.
- Visual and accessibility evidence covers only the named states in the reports. The new run confirms selected-template Orca announcements, the focused chooser/grid groups, and a named compact export control. It does not establish complete screen-reader compliance, every keyboard command, every scale/direction, all dialog outcomes, or cross-platform acceptance. Uncaptured or untested states remain unknown.

**P0 findings:** none were recorded in the audit. The earlier P1 repair cards are fixed in code. CODE-23 is `NEEDS_REVIEW`: its dialog is visually captured, but focus and action behavior still need live interaction review. REC-02 remains `OPEN`; its recovery format and limits were explicitly approved by the owner on 2026-09-25, but implementation and measured guarantees remain outstanding. UI-01 is `NEEDS_REVIEW`: the menu row is visible in the live window, but popup and keyboard behavior remain unverified. UI-28's short/long warning layout is fixed and captured live; dialog focus and actions remain under CODE-23. UI-29's reusable shared menu row and compact aligned renderer layout are fixed; the native popup and keyboard behavior remain unverified under UI-01. UI-14 is fixed with an app-level keyboard regression check. UI-25 compact recovery layout and recovery guidance are fixed in code; real playback/encode acceptance remains open because the required media tools and fixture are absent. UI-17 is independently checked as a still-frame preview/export path.

- Repair milestones pushed: `68df596`, `1a51c6d`, `29cb024`, `68b2b6b`, `9e15c55`, `1540a20`, `458e9ec`, `d58202a`, and Motion still-frame parity with independent renderer evidence.
- Documentation handoff checks: 18 focused governance tests pass; the governance, asset, and UI-foundation source audits pass. The current code-structure audit reports six legacy byte-ratchet limits in locked apps: Encode, Motion, Video app/core, Writer core, and Photo. No Sheets source is listed and no ceiling was raised. The Sheets extraction is now complete; no byte limit was raised. The source audit recognizes pre-existing accepted baseline files; it does not supply new human visual approval. All 49 accepted screenshot paths/hashes and explicit source-file references were checked. Sixteen audited product source files still match the original code audit hashes. Verification details are saved beside the visual report.

## Existing capability inventory — preserve these while repairing

These capabilities describe the current implementation and historical work, not blanket acceptance. Do not remove working features to make a defect disappear. Old performance and cross-platform numbers are historical, not rerun measurements.

### Sheets

The implementation includes sparse multi-sheet workbooks, formulas and cross-sheet ranges, absolute references, lazy conditionals, lookup/text/aggregate/date/financial functions, dynamic-array spills, formula-backed summaries, cell style/formatting, freeze and row/column sizing, charts, anchored shapes/images, tab operations, templates, native packages, CSV and XLSX paths, a command palette, undo, and recovery. The P1 repair run covers lossless text/recovery, bounded workbook history, valid rich XLSX chart output, embedded recovery images, and live imported formulas. The 2026-09-24 follow-up fixes XLSX sheet-name collisions and array-spill orientation, removes formula-reader overhead when no array functions exist, reuses calculated values during view-only updates, and routes formula-bar commits through a revision-safe background worker that preserves the user's current viewport and formula draft. CODE-23 reports known lossy XLSX features and stages both picker and startup imports until confirmation; the shared command dispatcher blocks native-menu and palette actions during the decision, including queued commands, the empty `<definedNames/>` case does not warn, and Escape cancels after Tab navigation. The maintenance split moved CLI, headless rendering, and XLSX import flow into modules; it divided the app tests and Slint dialogs and split core XLSX handling into focused modules. The latest workspace run passes 330 tests: 206 app, 2 cache integration, 104 core unit, 13 formula, 1 performance fixture, and 4 range tests. Full-workspace all-target Clippy and the production app build pass on the async export/close changes. A fresh native capture shows the menu row and CODE-23 warning dialog; popup interactions, dialog focus, and keyboard behavior remain unverified because the available computer-use interface exposes no native-app controls. The latest optimized 10,000-formula run calculated in 88.5 ms on this low-end local host; a LibreOffice Calc 24.2.7.2 XLSX→ODS→XLSX→Loom round-trip imported four sheets, formulas, tested formatting, one chart, and one shape, with a custom row/column-size warning. The opt-in release test projected 60 viewports across 1,000,000 pseudorandomly placed cells in a 2,048×2,048 sheet at 0.17 ms p95 and 0.26 ms maximum; its test process peaked at 90,308 KiB RSS with no swap. A test-profile app run timed only edit preparation and mailbox submission (0.086 ms combined), not the full UI callback or a visible frame. It then took 2.56 seconds to evaluate, 12.72 seconds to create a recovery package, and 43.05 seconds to write the recovery journal; its process peaked at 2,612,252 KiB RSS with no swap. These are CPU/test-process measurements, not native frame rate or whole-app memory. This is not complete acceptance. UI-02/04/08 full visual evidence, live native menu/import-warning/close interactions, complete visual coverage, broader accessibility, very slow recovery after large edits, non-cell full-workbook copying, full file callback/export-scale performance, native million-cell scrolling, a numeric app memory budget, and wider interoperability remain. A fresh code-structure audit reports 18 findings outside the active Sheets source changes. Single-series charts and cached PivotTable import remain boundaries; unsupported OOXML must be disclosed. Do not label these boundaries as proof that all imports are safe.

### Writer

The implementation includes rich blocks/style runs, multi-page layout, selection, undo/coalescing, native packages, Markdown/PDF export, outline/metrics, page setup, list styles, comments, Markdown-native tables, an inspector, templates, and command/menu projection. The P1 repair run now covers complete PDF pagination, guarded Open, and launch recovery. Drifting comment anchors, pointer-driven table editing, visible comment highlighting, measured representative performance, and explicit human acceptance remain incomplete/unverified.

### Present

The implementation includes slides, scene objects, selection/manipulation helpers, snapping, notes, undo, native persistence, PPTX/PDF paths, themes, transitions, and native command/menu projection. CODE-04 New/Open replacement is guarded; duplicate identities (CODE-16) and non-undoable transitions (CODE-19) still block acceptance. Older source-cleanup and test-count claims are not evidence that these user workflows are correct.

### Photo

The implementation includes image/layer models, pixel and adjustment operations, selection/editing tools, native persistence, export paths, history, and a desktop canvas/inspector. CODE-04 New/Open replacement and the default fit geometry (UI-16) are repaired. Layer ID collisions, the remaining compact/precision checks, and a real import-edit-save-reopen-export journey still block approval.

### Motion

Layer/keyframe models, interpolation, transforms, timing/playback helpers, procedural utilities, render-queue primitives, persistence/history, and a composition shell exist. Professional scene manipulation, graph/timeline editing, compositing/effects, playback/render workflows, and interchange remain incomplete or unverified.

### Video

Timeline/track/clip models, trim/marker helpers, local processing, caption/audio/media helpers, persistence/history, and a timeline shell exist. Scalable interaction, media/source consistency, source/viewer workflows, effects/color/audio depth, export UX, and professional editing behavior remain incomplete or unverified.

### Studio

Tracks/regions, PCM/WAV support, synthesis/DSP, mixer/automation primitives, persistence/history, local device foundations, and a multitrack shell exist. CODE-10 precision persistence is repaired in code; production recording, realtime scheduling, comping/time/pitch, plugin isolation/UI, mixing/mastering, and scalable arrangement interaction remain incomplete or unverified.

### Encode

FFmpeg queue/preset planning, execution/progress/cancellation, persistence/recovery, probe/conformance helpers, hardware-codec planning, and destination primitives exist. CODE-09 final no-overwrite protection and the UI-23/25 readiness paths are repaired in code; filesystem review, real encode fixtures, queue/settings hierarchy, watch-folder experience, hardware policy, pause/resume guarantees, exhaustive format support, and perceptual conformance remain incomplete or unverified.

## Engineering and acceptance debt

Oversized `main.rs`/`lib.rs` files and duplicated generic UI remain maintenance debt. Follow the byte ratchet; extract coherent responsibilities when a touched legacy file cannot grow. Do not create a parallel framework or rewrite the suite as part of one repair.

A dedicated Sheets CI job is defined in `.github/workflows/ci.yml` for formatting, Clippy, and workspace tests, with the native UI build dependencies installed. **GOV-01 · FIXED:** on pushed commit `ce2374c75fad4ec3e112b0c3a852840c39c1f928`, the job passed formatting, Clippy, and `cargo test --workspace --locked`. The separate repo-wide governance job is still red at the code-structure ratchet for four untouched files: `loom-encode/crates/loom-encode-app/src/main.rs`, `loom-video/crates/loom-video-app/src/main.rs`, `loom-video/crates/loom-video-core/src/lib.rs`, and `loom-photo/crates/loom-photo-app/src/main.rs`. That unrelated debt is not a Sheets test failure. A focused chart-range regression was also run once with the intentional range bug restored and failed on the expected XLSX range assertion; the fixed-source focused and full cached test harnesses pass. Keep unrelated release/package builds manual. Do not replace behavior checks with callback-count or screenshot-exists checks.

Asset provenance and commercial redistribution rules remain in `AGENTS.md` and `loom-bootstrap/contracts/assets.toml`. Fresh screenshots are evidence generated by this project, not imported product artwork. No third-party assets were added for this audit.

## Code and workflow repair cards

Cards retain their original finding text. P1 means user data, trust, or a security boundary is at risk. P2 means a serious workflow defect. The owner-authorized P0/P1 repair results below use `FIXED` when code and focused tests passed, and `NEEDS_REVIEW` when implementation landed but an expert, native, or real-media check is still required. P2 and GOV cards remain OPEN. Paths are relative to this repository. Search for the named function; old line numbers can move. Evidence directory: `.work/audit-2026-09-14/`. Recreate a fixture from the instructions if ignored evidence is unavailable.

### CODE-01 — Keep edits after reopening a saved document

**P1 · Shared · FIXED.** Think of the recovery sequence as numbered pages. A new page must have a bigger number than every saved page. Currently a restart resets the number and hides later edits.

**Repair result (2026-09-15):** Commit `68df596` restores the recovery sequence from the larger on-disk value and keeps overflow/error paths explicit. Shared recovery tests and probes pass; a fresh cross-platform crash fixture is still a release check.

**Open:** `loom-core/crates/loom-production/src/lib.rs`, `RecoveryJournal::open`.

1. Read the checkpoint sequence and the last journal sequence.
2. Set the next sequence to one more than the larger value. Handle an empty journal and checked integer overflow.
3. Keep existing file compatibility. Do not delete a checkpoint to hide this error.

**Prove it:** Record `first`, checkpoint `saved`, close, reopen, record `new unsaved edit`, close, reopen. Recovered text must be exactly `new unsaved edit`. Repeat twice. Existing result is `saved contents`. Evidence: `shared-probes/src/lib.rs`, `shared-probes.log`.
### CODE-02 — Publish recovery checkpoints without destroying the last good copy

**P1 · Shared · FIXED · Recovery regression suite passed on Ubuntu and Windows in CI run 679.** The payload and its checksum are one thing. Replacing just one half makes the saved copy unreadable.

**Repair result (2026-09-22):** Replaced hand-written delete-then-rename writes with `atomicwrites::AtomicFile`. One cross-process lock now covers journal open/repair, append, checkpoint, compaction, record reads, checkpoint-plus-journal recovery reads, and recovery clearing. Clearing removes saved data while preserving the stable lock file so another process cannot create a competing lock during deletion. Append refreshes its sequence from disk, atomically publishes the first journal file, and drops any unterminated final record before another append. Checkpoint sequence values cannot move backward or skip past saved recovery data; compaction cannot remove edits beyond the durable checkpoint. Added failure-injection tests for replacement, compaction, and torn-tail repair; tests for stale handles, concurrent checkpoints, bounded compaction, recovery lock coverage, incomplete JSON records, clear-and-reopen, and failed publication. Corrected the corruption test to damage the authoritative Windows commit record. Added `atomicwrites` and `fs2` to both workspace lockfiles and a focused `loom-production` test job on Ubuntu and Windows.

**Verification:** `cargo fmt --manifest-path loom-core/Cargo.toml --all -- --check`, `cargo metadata --manifest-path loom-core/Cargo.toml --format-version 1 --locked`, `cargo metadata --manifest-path loom-sheets/Cargo.toml --format-version 1 --locked`, the governance unit tests, governance audit, asset audit, UI-foundation audit, and `git diff --check` passed locally. Focused test command: `cargo test --manifest-path loom-core/Cargo.toml -p loom-production --locked`; the Recovery journal jobs passed on `ubuntu-24.04` and `windows-2022` in CI run 679 (commit `0d3936d`). The same run passed Sheets workspace format, Clippy, and tests. A read-only expert review found no remaining recovery blocker by inspection. The general governance job reports the existing code-size ratchet on four unchanged application files: `loom-video/crates/loom-video-app/src/main.rs`, `loom-video/crates/loom-video-core/src/lib.rs`, `loom-photo/crates/loom-photo-app/src/main.rs`, and `loom-encode/crates/loom-encode-app/src/main.rs`; that unrelated limit remains open.

**Open:** None for this card.

1. Draw the old checkpoint, new checkpoint, and journal on paper. At every filesystem operation, identify which complete copy a restart can read.
2. Write a new generation into separate files; flush its payload and metadata. Verify both before publishing that generation through one atomic commit point.
3. Preserve the old generation and journal until the new generation is durably published. Never delete the destination before replacing it.
4. Make startup choose a complete valid generation. Report damage; do not silently claim that missing edits were saved.

**Prove it:** Inject failure before/after each write, sync, and publish step. Restart must recover the last committed data plus valid newer journal entries. The existing metadata-temp failure produces `checkpoint digest mismatch` despite a valid journal. Test Linux and Windows replacement semantics before acceptance. Evidence: `shared-probes/src/lib.rs`. A tiny model must not invent an atomic-file protocol alone.
### CODE-03 — Save and load Sheets text exactly

**P1 · Sheets · FIXED.** Saving a sentence must not change any of its letters.

**Repair result (2026-09-15):** Commit `29cb024` uses strict JSON DTOs with escaping and malformed-input tests. Sheets core and app suites pass (103 core, 97 app tests in the repair run).

**Open:** `loom-sheets/crates/loom-sheets-core/src/persistence.rs`, `sheet_from_json`, workbook parsing and object parsing.

1. Add round-trip examples with a tab, carriage return, newline, backslash, quote, Unicode, `before\"}after`, and the literal path `C:\new\notes`.
2. Replace delimiter searches and chained string replacements with a real JSON decoder for the complete structure, including sheet names and object fields.
3. Keep the supported legacy format by decoding its actual schema. Reject malformed input with an error; never silently truncate it.

**Prove it:** Save/reopen multiple tabs and recovery snapshots; every original string must compare equal byte for byte, including a sheet named `My "Sheet"`. Evidence: `sheets/repro.rs`, `sheets-repro.log`.
### CODE-04 — Ask before throwing away unsaved work

**P1 · Shared interaction, then one app at a time · FIXED.** New/Open must not silently erase the document being edited.

**Repair result (2026-09-15):** Commits `29cb024`, `68b2b6b`, `458e9ec`, and `d58202a` guard Sheets, Writer, Photo, and Present replacement flows. Save, Discard, Cancel/Escape, clean snapshots, candidate validation, and modal focus are wired; affected app suites pass.

**Open:** Sheets `src/main.rs` callbacks for New/Open; corresponding Present New, Photo New, Writer Open; all under `loom-<app>/crates/loom-<app>-app/`. Shared dialogs live under `loom-core/crates/loom-desktop/`.

1. Track whether the current document differs from its last successful save. An undo back to the saved state should clear dirty state.
2. Before replacing dirty work, show **Save changes?** with **Save**, **Discard**, **Cancel**. Name the document. Cancel/Escape must be the safe way out.
3. Save: complete the real save first. A cancelled chooser or failed write keeps the old document, path, selection, history, and recovery state. Discard: replace only after that explicit choice.
4. For Open, parse the candidate successfully before swapping it into the live session. Audit close/quit through the same decision helper.

**Prove it:** Type a unique value, invoke New/Open, exercise all three choices and a failed save/open. Cancel/failure preserves the value and undo history. Only successful Save or explicit Discard allows replacement. Writer's initial New opens a template chooser; put the guard at replacement, not at every chooser opening. Original evidence is a callback trace; the fresh Linux Sheets run also observed New clearing `audit123` without a decision and Undo not restoring it.
### CODE-05 — Export every Writer page

**P1 · Writer · FIXED.** Export must not quietly stop halfway through the document.

**Repair result (2026-09-15):** Commit `68b2b6b` paginates every Writer page and adds a multi-page PDF regression. Writer app/core suites pass (72/78 tests).

**Open:** `loom-writer/crates/loom-writer-core/src/export.rs`, `export_pdf`, and Writer pagination/layout code.

1. Use the same page setup, wrapping, and pagination data as the document layout.
2. Start a PDF page for each layout page. Render all fragments in order, with the correct page margins and text styling.
3. Wrap long paragraphs. Do not use a bottom-of-page `break` that drops the remaining document.

**Prove it:** Export 60 uniquely numbered paragraphs. Independently extract the PDF text: all 60 must appear in order, once each. Page count must match layout (the audit fixture lays out three pages but exports one and only 31 paragraphs). Add a long paragraph and non-ASCII text; inspect rendered pages for clipping. Evidence: `documents-repro.log`, `documents/writer-60-paragraphs.pdf`.
### CODE-06 — Write valid XLSX drawing and chart XML

**P1 · Sheets · FIXED.** A file with a chart must still open in another spreadsheet program.

**Repair result (2026-09-15):** Commit `29cb024` binds the drawing namespace and escapes chart formulas, with an `R&D` regression. Sheets core/app suites pass.

**Open:** `loom-sheets/crates/loom-sheets-core/src/xlsx.rs`, worksheet drawing insertion and chart formula/range serialization.

1. Declare the relationship namespace wherever an `r:id` attribute is written.
2. Escape XML text in chart range formulas. Keep spreadsheet quoting and XML escaping as two separate steps.
3. Use the existing `REL_NS` namespace and `xml_escape_text` helper at these two write sites. Escape the complete generated chart formula after spreadsheet quoting. Do not rewrite unrelated XML generation.

**Prove it:** Export charts, shapes, and an embedded image on a sheet named `R&D`. Unzip the XLSX and parse every XML part with an independent XML parser. Verify relationships resolve and chart data points to the intended cells. Then open the file in an independent spreadsheet app without a repair warning. Current parser failures: `unbound prefix` and unescaped `&`. Evidence: `sheets/validate_xml.py`, `sheets-xml.log`.
### CODE-07 — Stop undo history from copying itself

**P1 · Sheets · FIXED.** A history entry must contain document changes, not another complete history full of histories.

**Repair result (2026-09-15):** Commit `29cb024` stores bounded document transactions instead of recursive histories and proves 100 renames stay bounded. Sheets core/app suites pass.

**Open:** `loom-sheets/crates/loom-sheets-app/src/main.rs`, `WorkbookUndoState::capture`/`restore`, `SheetTransaction`, and `commit_workbook_transaction`.

1. Separate document state from undo stacks. A document snapshot must not contain workbook transactions.
2. Store document-only before/after snapshots or a bounded delta. Preserve active-tab and per-tab edit behavior explicitly.
3. Apply an actual byte budget as well as an entry count. Releasing an old entry must release its owned data.

**Prove it:** Rename a tab 9 times, then 100 times; count retained transactions and bytes. Growth must be bounded/linear in the retained edits, not 1, 4, 13, 40… (9 edits currently contain 9,841 nested transactions). Undo/redo across rename, delete, switch, and cell edits must still work. Evidence: `sheets/history.rs`, `sheets-history.log`.
### CODE-08 — Keep plugin writes inside the allowed folder

**P1 · Plugin host · FIXED · Linux, Windows Server 2022, Apple Silicon macOS, and Intel macOS write-boundary tests pass in CI run `35709282018` at commit `7a220c879a935db55eaac6c51750fb52689646fd`.** A shortcut folder must not let a plugin write outside its permission boundary.

**Repair result (2026-09-15):** Commit `1a51c6d` canonicalizes and validates plugin storage paths and rejects traversal/symlink escapes in focused tests. An experienced cross-platform security review was still required at that point.

**Repair result (2026-09-22):** Authorization now opens the permission root and each parent folder as a directory handle without following links. Writes go to a new sibling file and are atomically renamed over the target, so writing through an existing hard link cannot change the outside name's contents. Windows keeps each ancestor handle open while publishing because its directory rename path is resolved from names. The independent security review found no remaining escape for an untrusted plugin limited to the host's `write_file` API. It identified a temp-name race only against a separate local process with the same OS permissions; that process is outside this plugin-only boundary and already has direct filesystem access. The SDK forbids unsafe code, so the wider same-user-process case is not addressed with raw Windows FFI.

**Verified:** On Linux, `cargo fmt --manifest-path loom-plugin-sdk/Cargo.toml --all -- --check`, `cargo test --manifest-path loom-plugin-sdk/Cargo.toml --workspace --locked` (72 passed), `cargo clippy --manifest-path loom-plugin-sdk/Cargo.toml --workspace --all-targets --locked -- -D warnings`, `cargo build --manifest-path loom-plugin-sdk/Cargo.toml --workspace --release --locked`, `cargo check --manifest-path loom-plugin-sdk/Cargo.toml -p loom-plugin-host --tests --target x86_64-pc-windows-gnu --locked`, and `git diff --check` passed. CI run `35709282018` passed the dedicated write-boundary test on `ubuntu-24.04`, `windows-2022`, `macos-15`, and `macos-15-intel`. The overall workflow run also had an unrelated `Code structure ratchet` failure on unchanged legacy application files; see the cross-cutting CI notes near the top of this ledger.

**Original repair checklist (retained for audit traceability):** `loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs`, `canonicalize_or_normalize`, write authorization and the actual write operation.

1. Reproduce with an allowed directory containing a symlink to an outside directory and a target file that does not exist yet.
2. Resolve existing parent directories for create targets. Compare the resolved parent with the allowed root.
3. Enforce the boundary when opening/creating the file, using directory handles or equivalent race-resistant platform APIs. Checking a string and writing later is insufficient.
4. Handle link swaps between check and write; fail closed with an actionable permission error.

**Prove it:** Existing-file, new-file, nested-link, traversal, and link-swap attempts cannot create or change any outside file. Normal allowed writes still work. The audit proves the host permission API escape, not a running Wasmtime exploit. Evidence: `plugin-permission.log`, `media-plugins/src/main.rs`.

### CODE-09 — Respect Encode's no-overwrite choice at the final write

**P1 · Encode · FIXED · Native Linux, Windows, and macOS publication tests pass in CI run `35712055760` at commit `6840076faf18e3023f6dd5e335c646cd5648ee22`.** Another file may appear while encoding. It still belongs to its owner.

**Repair result (2026-09-15):** Commit `1a51c6d` rechecks the destination immediately before the final publish and adds no-overwrite races. Focused Encode tests pass; filesystem semantics on Windows/network filesystems remain a review item.

**Repair result (2026-09-22):** `loom-encode/crates/loom-encode-core/src/output_publish.rs` now publishes no-overwrite results by creating a hard link from the completed same-folder temporary file to the final name. The operating system rejects the operation if another file already owns that name. Loom reports the collision, keeps the other file's bytes unchanged, and the temporary-output guard cleans up the encode result. The explicit Overwrite path remains separate. Added helper tests for a collision, a missing destination, and explicit overwrite; added a controlled-encoder race test for Unix and Windows; and added Linux, Windows, and macOS CI coverage. The independent filesystem review confirms the create-if-absent operation is appropriate; unsupported filesystems fail closed without replacing the destination.

**Verified:** Locally, `cargo fmt --manifest-path loom-encode/Cargo.toml --all -- --check`; `cargo test --manifest-path loom-encode/Cargo.toml -p loom-encode-core --locked --offline` (51 unit tests and 1 controlled-encoder integration test passed); `cargo clippy --manifest-path loom-encode/Cargo.toml -p loom-encode-core --all-targets --locked --offline -- -D warnings`; `cargo check --manifest-path loom-encode/Cargo.toml -p loom-encode-core --tests --target x86_64-pc-windows-gnu --locked --offline`; `python3 loom-bootstrap/scripts/audit-governance.py`; and `git diff --check`. CI run `35712055760` passed the helper and controlled-encoder collision tests on `ubuntu-24.04`, `windows-2022`, and `macos-15`. The same workflow's separate code-structure ratchet still fails on unchanged legacy application files; the dedicated Encode matrix is green. A full Encode app compile could not complete on this machine because the system `fontconfig` development package is absent; the app was not changed by this card.

**Remaining boundary:** Remote and network filesystems are not exercised. If the platform cannot create a hard link, Encode fails safely and does not replace the destination.

1. Keep encoding into a temporary file.
2. When overwrite is false, publish with an atomic **create only if absent** operation. An earlier `exists()` check does not solve the race.
3. On collision, preserve the existing destination, report the conflict, and clean up or offer the completed temporary result under a new name.
4. Keep explicit overwrite=true behavior separate and test platform differences.

**Prove it:** The controlled encoder creates `IMPORTANT_OTHER_FILE` at the destination midway through the job. The job reports a collision, leaves those exact bytes at the destination, and removes its temporary result. The helper tests also prove that an absent destination is created and that explicit Overwrite still replaces an existing file. Regression coverage: `loom-encode/crates/loom-encode-core/tests/output_publish.rs` and `loom-encode/crates/loom-encode-core/src/output_publish.rs`.
### CODE-10 — Preserve audio precision in Studio projects

**P1 · Studio · FIXED.** Saving the project must not make quiet sounds disappear or lower every sample a little.

**Repair result (2026-09-15):** Commit `1a51c6d` preserves full-fidelity audio samples in native persistence and adds quiet-signal/round-trip tests. Studio persistence tests pass.

**Open:** `loom-studio/crates/loom-studio-core/src/lib.rs`, `save_studio_bundle`, audio asset encoding/decoding.

1. Store native audio assets losslessly at their source/internal precision. Version the package representation and retain old-file loading.
2. Keep PCM16 quantization in explicit export options only. Native Save is not an audio conversion command.
3. Reuse unchanged asset payloads where possible.

**Prove it:** Round-trip samples `0.000001`, `0.75`, negative values, and supported extrema through three native saves. Samples must be bit-exact at the supported internal precision, with unchanged sample rate/channels/frame count. Currently the first becomes zero and 0.75 keeps decreasing. Evidence: `studio-roundtrip.log`, `media-plugins/src/bin/studio_roundtrip.rs`.
### CODE-11 — Recover Sheets images together with their cells

**P1 · Sheets · FIXED.** An image's name is not the image. Recovery needs its actual bytes.

**Repair result (2026-09-15):** Commit `29cb024` stores complete validated workbook packages with embedded image bytes and deduplicated assets. Sheets recovery/export tests pass; a native decoded-image fixture remains a visual release check.

**Open:** `loom-sheets/crates/loom-sheets-app/src/main.rs`, `record_workbook_snapshot`; `loom-sheets/crates/loom-sheets-app/src/assets.rs`, `prepare_workbook` and `attach_workbook_assets`; `loom-sheets/crates/loom-sheets-core/src/persistence.rs`, object parsing.

1. Encode workbook data and package-owned asset bytes in one recoverable snapshot/package.
2. Restore object references to those recovered bytes; do not depend on the original import path.
3. Preserve deduplication and integrity checks. Make missing/corrupt assets an explicit recoverable error.

**Prove it:** Import a valid PNG, save, remove only the test PNG's original file, edit, crash/recover, then render and export XLSX. The image must remain visible and exportable. The original probe used arbitrary bytes to isolate transport loss, so add the real-image case. Evidence: `sheets-repro.log`.
### CODE-12 — Keep imported shared formulas live

**P1 · Sheets · FIXED.** A displayed number calculated by a formula must keep recalculating after import.

**Repair result (2026-09-15):** Commit `29cb024` indexes shared formula masters and translates relative, mixed, and quoted references. Live recalculation regressions pass.

**Open:** `loom-sheets/crates/loom-sheets-core/src/lib.rs`, XLSX formula extraction; move coherent parser work into the existing interop module rather than growing this oversized file.

1. Index each shared formula master by worksheet and shared-formula ID.
2. Translate its formula to each member's relative row/column. Respect absolute `$` references, mixed references, ranges, and quoted sheet names.
3. Preserve a live expression. If a formula cannot be supported, show an explicit import warning and preserve its source; do not silently turn it into an ordinary constant.

**Prove it:** B1 has `A1*2`; B2 is a shared member. After import, change A2 to 50. B2 must become 100, not stay at its cached 30. Add mixed/absolute references and save/reopen. Evidence: `sheets-repro.log`; format reference: Microsoft's Open XML `CellFormula` documentation linked in the original audit.
### CODE-13 — Turn on Writer recovery when opening a file at launch

**P1 · Writer · FIXED.** Opening a document from the command line must not disable its safety net.

**Repair result (2026-09-22):** Writer initializes recovery for every editing launch, including `--open`. The requested file wins over an older draft, and new edits are recorded as usual. If recovery cannot initialize, the editor still opens and shows a short warning. Recovery write/checkpoint failures keep Save available, print the detailed cause to stderr, and show a short status message that fits. Refreshed `loom-writer/Cargo.lock` so the Writer workspace records the recovery dependencies used by its shared core.

**Evidence:** Added a two-process recovery test: open a saved `.loomdoc`, apply a unique unsaved edit through Writer state, exit without cleanup, and recover that edit on an ordinary launch. The original file bytes stay unchanged. Added invalid-`--open`, startup initialization failure, and recovery-write/checkpoint failure tests. All four focused tests and the full Writer app suite pass on Linux. Captured and inspected the visible warnings at `/tmp/loom-writer-recovery-init-failure.png` and `/tmp/loom-writer-recovery-write-failure.png`.

**Verification:** `cargo test --manifest-path loom-writer/Cargo.toml -p loom-writer-app --locked --offline` — 76 passed. `cargo fmt --manifest-path loom-writer/Cargo.toml --all -- --check`, `git diff --check`, and `audit-governance.py` pass. Clippy exits 0 with existing warnings in `main.rs` table setup and `actions_tests.rs:1446`. `audit-code-structure.py` still reports four unrelated legacy overages in Encode, Video, and Photo; it reports no Writer file. The permission-denied injection is Unix-only; Windows recovery restart coverage was not run locally.

**Open:** `loom-writer/crates/loom-writer-app/src/main.rs`, `run_gui_with_dialogs` startup; `loom-core/crates/loom-production/src/snapshot.rs` recovery macro.

1. Initialize the recovery store for every editing session, including `--open`.
2. Separately choose whether to restore an older recovery payload or open the requested document.
3. Surface initialization/write errors in interactive sessions. Preserve the recovery macro's intentional no-op for nonediting headless capture paths; enforce the editing-session requirement at interactive initialization/call sites.

**Prove it:** Launch with a saved test `.loomdoc`, edit a unique sentence, terminate the test app without a save, and recover. The sentence must return. Also test ordinary launch and an invalid launch path. Original evidence is source tracing, not a live forced-crash test. This depends on CODE-01/02.
### CODE-14 — Give every Photo layer a unique ID

**P2 · Photo · OPEN.** Deleting a layer does not make its old number safe to reuse if another layer already has that number.

**Open:** `loom-photo/crates/loom-photo-app/src/main.rs`, pixel and adjustment layer insertion near `layers.len() + 1`; layer identity helpers in the core.

1. Use a persisted monotonic allocator or collision-checked unique ID helper. Apply it to every layer creation path.
2. Keep displayed layer names separate from internal identity. Never replace an existing asset because a new layer got the same ID.

**Prove it:** Add two layers, delete the earlier one, add another, save/reopen, undo/redo. IDs stay unique and each layer keeps its original image. Current result includes `layer-3` twice and save fails. Evidence: `documents-repro.log`.

### CODE-15 — Enforce plugin timeouts while sending input

**P2 · Plugin host · OPEN · Needs concurrency review.** A child that refuses to read must not freeze the host while the host fills its input pipe.

**Open:** `loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs`, `invoke`.

1. Start the deadline before process I/O. Supervise stdin, stdout, stderr, and child completion concurrently.
2. On timeout or I/O failure, close pipes, stop and reap the child, and finish the invocation once. Bound buffered output.
3. Avoid waiting forever on the input writer after killing the process; test child descendants holding pipes too.

**Prove it:** A nonreading child with 1 MiB input and a 50 ms limit returns a timeout within a documented scheduling tolerance (for example 500 ms on the test host), without a leaked child. Also test full stdout/stderr, normal completion, and cancellation. Current controlled adapter takes about 2,014 ms and returns Broken pipe. No real Wasmtime runtime was exercised. Evidence: `media-timeout.log`.

### CODE-16 — Give every Present slide a unique ID

**P2 · Present · OPEN.** Two slides must never share the same identity.

**Open:** `loom-present/crates/loom-present-core/src/lib.rs`, `add_slide` and the existing unique-slide-ID helper used for duplication.

1. Reuse the collision-free allocation strategy for new slides as well as duplicates.
2. Preserve transitions, notes, navigation, and references when deleting/recreating slides.

**Prove it:** Add two slides, delete the earlier added slide, add again. IDs must be unique; select each slide, assign a different transition, save/reopen, and verify each keeps its own state. Current IDs include `slide-3` twice. Evidence: `documents-repro.log`.

### CODE-17 — Move Writer comments with the text they describe

**P2 · Writer · OPEN.** Inserting words before a comment must not attach it to different words.

**Open:** `loom-writer/crates/loom-writer-core/src/lib.rs`, `replace_paragraphs`, comment anchors, text-edit mapping.

1. Describe each edit as removed range plus inserted text. Rebase comment endpoints alongside style runs.
2. Define what happens when commented text is partly or entirely deleted: shrink, collapse, or mark orphaned explicitly; never silently point at unrelated text.
3. Handle paragraph split/merge and valid UTF-8 boundaries. Include anchors in undo/redo and persistence.

**Prove it:** Comment on `world` in `Hello world`, insert `New ` at the start; the anchor must still select `world` (10..15), not `llo w` (6..11). Add emoji, splits, merges, deletion, undo/redo, save/reopen. Evidence: `documents-repro.log`.

### CODE-18 — Do not consume a storage sequence when append fails

**P2 · Shared storage · OPEN.** A failed write is not a completed journal entry.

**Open:** `loom-core/crates/loom-storage/src/lib.rs`, journal `append`.

1. Reserve the next sequence locally. Advance in-memory sequence state only after the entry is durably written.
2. Define and implement partial-write recovery/truncation so a retry cannot append behind a broken tail.
3. Preserve the original error and keep the journal recoverable. Do not waive sequence validation on load.

**Prove it:** Force the first open/write to fail, remove the fixture obstruction, append successfully, reopen. The entry must load at sequence 1. Also inject partial-write and sync failures. Current retry writes sequence 2 and reopen fails. This storage journal is distinct from desktop production recovery. Evidence: `shared-probes/src/lib.rs`, `shared-probes.log`.

### CODE-19 — Undo Present transitions with the rest of the slide

**P2 · Present · OPEN.** Undo must reverse a visible transition change.

**Open:** Present app `src/main.rs`, transition callback; `loom-present/crates/loom-present-core/src/lib.rs`, session history and transition map.

1. Include transitions in the session state saved by undo history. A document-only checkpoint is insufficient.
2. Route transition edits through the same transaction/checkpoint mechanism as other edits.
3. A fresh edit after undo must clear redo. Keep transition state attached to unique slide IDs (CODE-16).

**Prove it:** Set Dissolve, undo to the original transition, redo to Dissolve. Mix this with slide deletion/duplication, then save/reopen. Current isolated transition change leaves `undo()` false. Evidence: `documents-repro.log`.

## UI/UX repair cards — fresh 2026-09-14/15 evidence

Each card shows its current state. A screenshot proves only the visible state; native observations are marked separately. Original audit evidence lives in `.work/uiux-audit-2026-09-14/`. A cut-off control inside an otherwise complete app capture is a product finding, not an accidentally cropped evidence image.

### UI-01 — Give every desktop a visible application menu

**P1 · Sheets first; check each app when active · NEEDS_REVIEW.** The original Linux audit saw no menu bar because Loom only installed the native global menu on macOS. A toolbar route, overflow item, and Ctrl+K palette did not satisfy the user's need for a normal File/Edit/View menu on Linux Mint.

**Repair result (2026-09-24):** Sheets draws File, Edit, View, Table, and Help menus inside every non-macOS window. macOS keeps its native global menu. The local menu is projected from the installed menu model, uses the same command IDs and enabled/checked state, and dispatches through the existing guarded menu action sink. Help > Keyboard Shortcuts opens the existing command palette. Unsupported items and empty menus are hidden. The popup now contains only the selected menu's rows; hidden entries no longer leave a blank gap above Edit. The native `NativeMenuBar` installer only calls the OS menu API under `target_os = "macos"`; generating Linux DBusMenu layout data does not install or display a Linux global menu.

**Verification:** The non-macOS accessibility-tree test finds File, Edit, View, and Table in the application window. Projection/dispatch and keyboard-navigation tests pass. The latest CI-equivalent Sheets workspace suite passes 245 tests with Slint debug metadata enabled; the native build result is in the [tracked follow-up report](AGENTS.md#source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md). A 2026-09-24 native capture shows the visible File/Edit/View/Table/Help row in the live window; see `loom-sheets/docs/qa-native/ui23-xlsx-import-warning-live-linux.png`. The screen is unlocked and the screensaver is inactive. The popup was not opened and menu keyboard actions were not tested because this session provides no native-app control API. Keep `NEEDS_REVIEW` until those live interactions are checked.

**Evidence:** Source in `loom-sheets/crates/loom-sheets-app/ui/app.slint`, `src/local_menu.rs`, `src/local_menu_tests.rs`, and `src/main.rs`. Tests: `non_macos_window_exposes_a_local_application_menu_bar` and `local_application_menu_supports_keyboard_navigation_and_activation`. The current renderer preview is `loom-sheets/docs/qa-renderer/ui01-local-menu-current-1024-linux.png`; it is not native evidence. The before-fix live Edit capture is `loom-sheets/docs/qa-native/ui01-edit-menu-open-before-fix-linux.png`. Detailed current commands and remaining native check are in [the tracked report](AGENTS.md#source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md) and the portable audit archive.

**Remaining check:** With the desktop unlocked, use a native-app control surface to inspect the real application window. Confirm the menu row is visible at 1024, 1280, and 1440 px, open each menu, and use only the pointer to create, open, and save a test workbook. Confirm keyboard access with Alt+F/E/V/T/H and Escape. Check macOS separately to confirm its native global menu remains available. CODE-04 must protect the file transitions.
### UI-28 — Fit the XLSX loss warning to its message

**P2 · Sheets warning dialog · FIXED.** The one-feature native warning capture left a large empty block between the warning copy and its buttons. The dialog had a fixed 456 px height and stretched the warning text area to fill the spare space. Long loss lists also needed readable warnings and visible buttons.

**Reproduce:** Start Sheets with an XLSX that has exactly one unsupported feature, such as custom row or column sizes, and inspect the native warning at the device's current 1018×744 window size. Compare the short message with a fixture containing many warnings.

**For a tiny coding model:** (1) Read `xlsx_import_warning.slint` and the test named `xlsx_import_warning_fits_short_and_long_messages`. (2) Let the box grow from its title, warning text, buttons, and padding instead of always using 456 px. (3) Cap its height to the available window. Put only the warning copy in the scroll area; keep both buttons after it. (4) Keep Escape mapped to Cancel. (5) Render a short warning and a long warning at 1024×720 and 2× text size. Check that the short warning has no large blank block and that long text scrolls without hiding either button.

**Repair result (2026-09-25):** The dialog now sizes itself to its contents, caps its height to the window, and scrolls only the warning text when needed. Text uses the app's template scale. The action row stays outside the scroll area. The Escape-to-Cancel focus scope and callback are unchanged.

**Verification:** The focused warning layout regression failed before the repair because the short dialog was 390 px high. It now checks the short message, 2× text, and a 30-line warning in a 1024×720 render; it confirms buttons remain visible while the warning text area is capped. The live Linux capture at 1018×744 shows the real startup warning at 2× text scale with both actions visible. This closes the layout finding; live focus, Escape, and button effects remain unverified under CODE-23.

**Evidence:** Live capture `loom-sheets/docs/qa-native/ui28-xlsx-warning-live-linux.png` (SHA-256 `3e78058c6ddf36c690816bf54480b4ad97f7b9a7bb33fbb7e49f560ed3a33e02`). Renderer captures: `loom-sheets/docs/qa-renderer/ui28-xlsx-warning-short-1024x720-linux.png`, `ui28-xlsx-warning-short-2x-1024x720-linux.png`, and `ui28-xlsx-warning-long-2x-1024x720-linux.png`. Test source: `loom-sheets/crates/loom-sheets-app/src/main_tests/workbook_interop_tests.rs`.

### UI-29 — Align and size desktop menu popup items

**P2 · Shared Loom UI, first used by Sheets · FIXED in rendered layout; native interaction needs review.** The Sheets dropdown entries were center-aligned, so labels and shortcuts looked unprofessional and hard to scan. The popup also retained a large blank area because its panel always reserved space for many rows.

**Reproduce:** Open the File menu at 1024×720. Labels and shortcut text appear together as one centered string; a three-row menu uses a much taller panel than its content needs.

**For a tiny coding model:** (1) Read `LoomMenuItem` in `loom-core/crates/loom-ui/ui/foundation/overlays.slint`. (2) Keep it in the shared UI foundation and export it from `loom-core/crates/loom-ui/ui/foundation.slint`; importing apps must reuse it. (3) Give the row separate `label` and `shortcut` properties. Put the label at the left for left-to-right text, put the shortcut at the right, and reserve a small separate slot for a check mark. (4) In each popup, size the panel to its visible rows and cap it to the window. Do not put the label and shortcut into one centered text string. (5) Check the shared Overlays gallery, run the menu layout regression, and inspect the 1024×720 renderer capture. Leave native menu behavior in NEEDS_REVIEW until it is tested in the actual window.

**Repair result (2026-09-25):** Added shared, exported `LoomMenuItem` to Loom UI Foundation. It supports separate label/shortcut fields, checked and selected states, disabled state, activation, accessible list-item labeling, theme tokens, and reading-direction-aware alignment. The Foundation Overlays gallery demonstrates normal, checked/selected, and disabled rows. Sheets now uses this component instead of one centered button string, and sizes the popup to the selected menu's preferred content height with a window cap.

**Verification:** The focused red test found the first menu label at x=110 instead of near the popup's reading edge; a second red check found a 487 px blank vertical run in the three-item menu. The full Sheets workspace passes **267 tests** (143 app, 2 evaluation-cache integration, 104 core, 13 formula, 1 performance fixture, and 4 range tests). The renderer capture shows aligned labels, right-side shortcuts, selected/disabled states, and a compact menu. The production app builds. This does not prove native popup appearance, pointer activation, or keyboard interaction; those remain under UI-01.

**Reusable component:** Import `LoomMenuItem` from `foundation.slint` in Loom Slint applications. The canonical export is `loom-core/crates/loom-ui/ui/foundation.slint`; the shared source is `loom-core/crates/loom-ui/ui/foundation/overlays.slint`. This repairs the common menu-row control, not each application's command model or popup behavior.

**Evidence:** `loom-sheets/docs/qa-renderer/ui29-menu-popup-aligned-1024x720-linux.png` (SHA-256 `8227e21a9175e531b2681f596e8f1d54d3a679ec8233ef7fdcc6b483abf3ad5d`), `loom-sheets/crates/loom-sheets-app/src/local_menu_tests.rs`, and the Loom UI Overlays gallery in `loom-core/crates/loom-ui/ui/foundation/sections-b.slint`.

### UI-02 — Stop opening a mostly empty Sheets inspector by default

**P2 · Sheets · FIXED.** The startup inspector consumed 320 px for a few properties and a large empty panel. At the reference 1280 px width, only 960 px remained for the central area (75%, before inner padding). The contract requires at least 78% and a closed inspector by default.

**Repair result (2026-09-18):** Commit `fix: close Sheets inspector by default` starts the Format inspector closed, adds a separate remembered preference, and restores that preference after compact mode instead of reopening the panel. The focused breakpoint test and all 97 Sheets app tests pass. The 1024/1280/1440 visual acceptance captures remain a gate item.

**Evidence:** `sheets/01-start-light-1440.png`, `02-compact-light-1024.png`, `17-current-native-start.png`. Compact mode already gives the sheet more room; keep that strength.

**Open for acceptance evidence:** Sheets `ui/app.slint` (`show-inspector`), `src/main.rs` (`INSPECTOR_WIDTH`, breakpoint handling); `loom-design-bible/contracts/desktop-ui.toml` `[app.sheets]`.

1. Start with the inspector closed, matching `right-inspector-default = 0`.
2. Keep a labeled Format toggle; opening it must preserve selection and show properties for that selection.
3. Respect the user's explicit open/closed choice across resizing. Use shared width tokens instead of an app-only 320 px constant.

**Done when:** A fresh 1280×800 window gives the central work surface at least 78% of available width. Toggle Format, resize below and above the compact breakpoint, and verify useful controls remain reachable without reopening a panel the user closed. Test 1024×720 and 1440×900 too.

### UI-03 — Reflow the template chooser and show truthful Recents

**P2 · Sheets · FIXED.** At 1024×720 the rightmost template previews and names were cut off. The chooser also repeated Blank under Recents and Basic, and Recents was hard-coded instead of showing actual recent choices.

**Repair result (2026-09-22):** The chooser now uses a bounded vertical scroll region with full card names, shows only session-created templates in Recents, and exposes all category sections in All Templates. Left/Right keyboard selection follows the visible category with wrap-around; Return/Create uses the selected card; Escape/Cancel leaves the current workbook unchanged. The selected Invoice & Expenses card remains fully visible at 1.25× and 1.5× text scale after the scroll target is adjusted for the larger card heights. Chooser navigation moved to `template_navigation.rs` so the active Sheets source-size ceiling remains satisfied.

**Evidence:** `.work/sheets-acceptance-2026-09-22/REPORT.md`; fresh captures under `.work/sheets-acceptance-2026-09-22/screenshots/`; focused test `keyboard_selection_scrolls_to_the_selected_template_in_all_templates`; full Sheets app suite (109 passed).

**Native result (2026-09-23):** On the rebuilt Linux app, AT-SPI reports the `Template chooser` group focused and exposes the selected template plus Left/Right/Return/Escape instructions. Right selects Checklist; Orca speaks the selection and instructions; Return creates the Checklist workbook; Escape preserves the opened workbook. Both close paths restore focus to the named worksheet-grid group. Native captures are under `loom-sheets/docs/qa-native/` and the detailed results are in `.work/sheets-acceptance-2026-09-23/REPORT.md`.

**Still open for overall Sheets acceptance:** test the full template catalog at all required scales/viewports and broaden screen-reader/keyboard coverage beyond this chooser workflow.

1. Compute how many full cards fit in the content area after the category sidebar and padding. Move excess cards to another row; allow vertical scrolling of this content region.
2. Keep the entire template name and preview visible. Do not shrink the text or make the whole modal horizontally scroll.
3. Show Recents only from actual stored template choices. If there are none, omit that section or say `No recent templates`.
4. Keep keyboard selection visible as it moves; selection must refer to the intended template after reflow or category filtering.

**Done when:** At 1024×720 and 1.25/1.5 text scale, every template can be reached, its full name is readable, and Create produces the selected item. Escape/Cancel preserves the current workbook. No right-edge card is sliced in half.

### UI-04 — Use honest starter data and a clear empty-document path

**P2 · Sheets · FIXED.** Startup looked like a user's Budget but contained demo values. Rent/Transport said monthly, Food said weekly, yet Total simply added all three. No common period or currency was stated.

**Repair result (2026-09-18; visual recheck 2026-09-22):** The normal GUI and a plain headless screenshot start with a blank `Untitled` workbook. Explicit `--example`/smoke/chart/object paths use a named `Example Budget` whose visible amount header is `Amount (USD/month)`, whose period column is `Monthly`, and whose Total/Average cells remain live formulas. New/recovery fallback behavior stays blank unless `--example` is requested. The refreshed 1024×720 and 1440×900 chart captures show the complete amount header.

**Evidence:** `sheets/01-start-light-1440.png`, `17-current-native-start.png`; `starter_workbook` assigns the mixed period labels.

**Open for acceptance evidence:** `loom-sheets/crates/loom-sheets-app/src/main.rs`, `starter_workbook`, startup selection, and template data.

1. Make a new document blank, with an obvious path to templates/examples. If opening an example is desired, name it `Example Budget` and make that choice explicit.
2. Give the example a single stated period and unit. Either all amounts are monthly or convert them with a visible formula and explanation before totaling.
3. Keep the sample's totals as real formulas. Never mark example data as a saved user document or silently overwrite user recovery with it.

**Done when:** First launch and New have clear, distinct blank/example behavior. Every Budget amount uses the same stated unit and time period, and changing an amount recalculates its total. Add a small test of example values/formulas, not a test that merely searches for the word Budget.

### UI-05 — Make Sheets header cells readable in high contrast

**P1 · Sheets accessibility · FIXED.** High-contrast mode hides the actual header text `Item`, `Amount`, and `Note`: the header background and text are both black. A1's formula field still says Item, confirming the text exists.

**Repair result (2026-09-15):** Commit `29cb024` uses high-contrast header foreground/background tokens. Sheets source and UI checks pass.

**Evidence:** `sheets/08-high-contrast.png` compared with `02-compact-light-1024.png`. Source uses `paper-line` for header fill and `paper-ink` for text; both are black in the high-contrast palette.

**Open:** Sheets `ui/components.slint`, cell background/text in `SheetGridSurface`; shared `loom-core/crates/loom-ui/ui/theme.slint`; corresponding design tokens/contracts.

1. Use a foreground/background pair for header cells. Do not use a line/separator color as a text background without its paired foreground.
2. Preserve the distinct active-cell border and readable user-specified fills. Check headers, selected ranges, and ordinary cells separately.
3. If a new semantic token is necessary, define it centrally and demonstrate it in the gallery; do not add an isolated color literal.

**Done when:** Item/Amount/Note are readable at 1024×720 and 1440×900 in all three themes, with and without selection/fills. Measure the contract's required contrast for each pair and inspect the actual screenshot. This repairs one visible accessibility defect; it is not full accessibility certification.
### UI-06 — Render the status and errors the controller produces

**P1 · Sheets trust/feedback · FIXED.** The window has no visible status bar or saved/unsaved indicator. Source declares and updates status strings, imports status components, but never instantiates those components. Users cannot rely on messages that only exist in memory.

**Repair result (2026-09-15):** Commit `29cb024` renders controller status/error state in the visible status bar. Sheets UI tests cover the projection.

**Repair result (2026-09-23):** The native window title now follows the saved workbook filename and appends `*` while the workbook differs from its saved snapshot. Undoing back to the saved content or completing a save recalculates that marker. A focused unit test covers clean/dirty filename and template titles. A live edit to A2 changed the title from `loom-sheets-native-acceptance-2026-09-23.loomtable` to the same filename with `*`; the opened and edited native screenshots are under `loom-sheets/docs/qa-native/`.

**Repair result (2026-09-23, UI-06 completion):** Formula errors now appear both in formula feedback and in the visible status area, which is a polite live region. The cell/formula count has its own status property so a grid reprojection cannot erase an error message. Read-only save failures use concise actionable feedback. Native captures show an invalid formula, a read-only Save failure with the workbook still marked `*`, and Save As cancellation with the dirty marker intact. Anchored Orca log records contain one speech event each for `Formula error in A1: #DIV/0!`, `Save failed: destination is read-only`, and `Save cancelled`.

**Verification:** Focused regressions `formula_errors_are_visible_in_a_polite_live_region` and `readonly_save_error_feedback_is_short_and_actionable`; the complete Sheets app suite passed 112 tests, and the native app build succeeded. The save-cancel visual and announcement were captured immediately before the status-summary layout follow-up; the cancellation handler was unchanged by that follow-up.

**Evidence:** `sheets/17-current-native-start.png`, `18-current-native-edit.png`, `20-current-native-undo.png`; source check of `status-left`, `status-right`, and the complete `ui/app.slint` layout. Native title behavior is also captured in `opened-saved-workbook-linux.png` and `unsaved-edit-title-linux.png`. UI-06 outcome captures are `loom-sheets/docs/qa-native/ui06-formula-error-live-status-linux.png`, `ui06-readonly-save-failure-live-status-linux.png`, and `ui06-save-cancel-dirty-workbook-linux.png`. Full details: `.work/sheets-acceptance-2026-09-23/REPORT.md`.

**Original acceptance target:** Exercise save cancel, a forced write failure, invalid formula feedback, and their one-time screen-reader announcements in the native app. Keep the recorded controller/status paths intact. The 2026-09-23 repair result above records these checks; broader accessibility acceptance remains open.

1. Render the existing status properties in the shared status bar using the contract height, outside the grid.
2. Show a clear unsaved state tied to actual document dirtiness. Clear it only after a successful save, not after merely opening a save chooser.
3. Show actionable error messages persistently enough to read; do not bury a failed save in an invisible property. Announce relevant messages through the existing accessibility mechanism without repeating every frame.

**Done when:** Edit, save, cancel save, force a write failure in a test folder, and trigger an invalid formula. Visible state must distinguish these outcomes. The grid must remain usable at 1024×720. A screen reader must receive the meaningful error once; record that check separately from screenshots.
### UI-07 — Clear edited fields when their document value changes

**P1 · Shared text input, reproduced in Sheets · FIXED.** After typing `audit123` and creating a new blank workbook, the formula field still paints that old text over its empty-field placeholder. The grid is blank. This makes the visible value disagree with the current document.

**Repair result (2026-09-15):** Commit `29cb024` clears stale formula input when the workbook changes and keeps the placeholder truthful. The focused Sheets regression passes.

**Evidence:** `sheets/18-current-native-edit.png` → `19-current-native-new.png` → `20-current-native-undo.png` → `21-current-native-palette.png`. Reproduced with the freshly built current-source native app in an isolated display and state directory. Source points to the shared input's one-way `text: root.value` binding and its independently drawn placeholder; confirm the precise binding fix with a focused reproducer.

**Open:** `loom-core/crates/loom-ui/ui/foundation/controls.slint`, `LoomTextField` (also check `LoomSearchField`); Sheets `FormulaNameBar` and buffer reset paths.

1. Reproduce typing, then programmatically replacing the field's value with a different string and with empty text.
2. Make the editable text and public value stay synchronized, using Slint's appropriate two-way binding or explicit update path. The placeholder must depend on the actual displayed text being empty.
3. Ensure New/Open, selection changes, commit, and cancel reset the buffer consistently without writing stale text into the new document.

**Done when:** Type `audit123`, commit, start a blank document, then select another cell. No old text remains; exactly one placeholder appears in an empty field. Repeat using Undo and switching tabs. Verify the shared gallery case and native Sheets; a headless initial-state screenshot cannot expose this defect.
### UI-08 — Make chart ranges and comparisons understandable

**P2 · Sheets · FIXED.** The default Budget chart compared Rent, Food, Transport, Total, and Average as peer categories, without explaining its source range or unit.

**Repair result (2026-09-22):** Chart creation now uses the selected cell range and stores the exact row bounds. The example chart selects `A1:B4`, labels the series `Amount`, shows `USD/month`, and plots only Rent/Food/Transport. Source edits redraw the chart; Undo restores the previous value. The workbook save/load path and XLSX export/import preserve chart bounds. A user can explicitly select `A1:B5` and include Total. To replace the range, select the new two-column range and choose Insert Chart; the chart shows this instruction.

**Evidence:** App suite: 101 passed, including `chart_insert_requires_range_and_can_replace_source` (explicit range, unit, live edit, Undo, native Save/Open, explicit Total row). Core suite: 87 passed, including `chart_explicit_range_roundtrips_without_total_row` (XLSX round-trip). Both were run directly from prebuilt test harnesses in this acceptance pass; no local rebuild was performed. Refreshed 1024×720 and 1440×900 chart captures: `.work/sheets-acceptance-2026-09-22/screenshots/1024x720-chart.png` and `1440x900-chart.png`.

**Open:** Visual evidence is headless. Native pointer/keyboard editing, screen-reader output, and a fresh local build remain part of the overall Sheets acceptance gate.

### UI-09 — Distinguish a workbook from one sheet in its commands

**P1 · Sheets · FIXED.** The palette says `New Sheet`, but the command replaces the entire workbook and clears its tabs/history. This label suggests a much smaller action than it performs.

**Repair result (2026-09-15):** Commit `29cb024` renames whole-file commands to Workbook and tab commands to Sheet, including palette/menu labels.

**Evidence:** `sheets/21-current-native-palette.png`, the New callback in CODE-04, and `19-current-native-new.png` showing the blank workbook after New.

**Open:** Sheets command labels in `src/main.rs`/`src/palette.rs`, menus, tab-add control, and user-facing status strings.

1. Use `New Workbook`, `Open Workbook`, `Save Workbook`, and `Save Workbook As` for whole-file actions.
2. Use `Add Sheet`, `Rename Sheet`, and `Delete Sheet` for operations on a tab inside that file.
3. Update labels, tooltips, accessible names, and palette search synonyms together; retain the established shortcuts.

**Done when:** A workbook with two tabs remains intact when Add Sheet is chosen and gains one tab. New Workbook invokes CODE-04's dirty-work decision and replaces the whole file only after consent. Every visible label accurately names its scope.
### UI-10 — Remove the contradictory permission to truncate action captions

**P2 · Shared design contract · OPEN · Source-confirmed design debt.** `AGENTS.md` requires readable complete action labels, but `desktop-ui.toml` permits 10 px ellipsized toolbar captions. A small model can obey one rule and violate the other.

**Open:** `loom-design-bible/contracts/desktop-ui.toml`, `[component.labeled-toolbar-item]`, `[component.icon-over-label-toolbar-item]`, toolbar/ellipsis clauses; shared toolbar components and their contract audits.

1. Make the contract agree with the readable-action rule: full captions or an accessible icon-only action with a tooltip and a reachable fully labeled menu item.
2. Use the existing `[typography]` `caption = 11` and matching caption line height from this contract, through shared tokens. Do not invent a separate 10 px caption rule or shrink text to make it fit.
3. Update component measurement/overflow behavior and contract checks together. Preserve optional ellipsis for user content names where explicitly allowed.

**Done when:** Long translated action labels and 1.25/1.5 text scale remain operable at the boundary widths 1179/1180, 1279/1280, and 1319/1320. The contract and UI tests reject clipped action captions. This is a source-confirmed rule conflict, not a claim that every captured toolbar currently clips.

### UI-11 — Keep product help out of the user's document

**P2 · Writer · OPEN · Rendered and source-confirmed.** The default sample page contains product claims about an “inspectable” package, deterministic PDF export, and fully wired undo. It looks like editable document content because it is document content. This is a poor starting point for writing and makes unverified engineering claims part of the user's file.

**Evidence:** `writer/01-1440x900-light.png`, `02-1024x720-light.png`. Native launch opens a chooser; the screenshot path shows the sample. Do not claim every native launch bypasses the chooser.

**Open:** `loom-writer/crates/loom-writer-app/src/main.rs`, the sample document builder containing `Loom Writer is a calm`; `ui/template_chooser.slint` in that app.

1. Keep Blank genuinely blank, with the caret ready for text. Keep deliberate sample content available only as a clearly named sample/template.
2. Put shortcuts and help in application help or an empty-state hint outside the saved document. Do not serialize that hint into user content.
3. Remove unsupported readiness claims from the sample/help copy. Describe available actions in ordinary words.

**Done when:** Create Blank, type one sentence, save/reopen, and export. Only that sentence appears. Opening a named sample still works and never replaces a dirty document without CODE-04's decision.

### UI-12 — Make each Writer template create what its card says

**P1 · Writer · FIXED · Native failure reproduced.** Selecting Executive Report and clicking Create Document produces a letter beginning “Your Name” and “Dear Recipient”; the status says “Created letter document.” The UI exposes six positional choices while the callback maps only four template kinds.

**Repair result (2026-09-15):** Commit `68b2b6b` maps stable Writer template IDs to their descriptors and generated documents. Writer app tests pass.

**Evidence:** `writer/09-native-start.png` → `10-native-report-selected.png` → `11-native-created-letter.png`. These use the current-source executable.

**Open:** `loom-writer/crates/loom-writer-app/ui/template_chooser.slint`, card IDs/order; `loom-writer/crates/loom-writer-app/src/main.rs`, `on_create_template`; the existing core template enum and builders reached from that callback.

1. Make a small table of every visible card and the document it must create. Current cards are Blank, Blank Black, Executive Report, Business Letter, Curriculum Vitae, and Newsletter.
2. Give cards stable template IDs and dispatch by ID. Do not use the card's position as its meaning. Use the same descriptor for the name and preview.
3. Show only genuinely implemented template choices. If a displayed style lacks a generator, record that missing choice explicitly; never silently create a different template. Preserve the existing working generators.
4. Generate the selected document, then replace the current one through the dirty-work guard. An invalid ID must return an understandable error without changing the open document.

**Done when:** Click every visible template card in the native chooser and create it. Its heading, content, page style, and status agree with the selected card. A test that reorders the descriptors must not change which document an ID creates. Save/reopen one nonblank result. Check pointer and keyboard selection independently.
### UI-13 — Make the table command honest about its editing model

**P2 · Writer · OPEN · Rendered limitation.** Insert Table renders pipe-separated Markdown text on the page instead of a table with aligned cells and borders. The generic “Insert Table” command promises a visual editing object the current renderer does not provide.

**Evidence:** `writer/05-inspector.png`, `08-inspector-wide.png`; the wide capture shows populated rows as literal pipes. This audit did not verify pointer-based cell editing.

**Open:** `loom-writer/crates/loom-writer-app/src/main.rs`, `writer_render_projection`, `writer_render_markup`, `on_insert_table`, and command label `writer.table.insert`; `loom-writer/crates/loom-writer-app/ui/writer_components.slint`; core `parse_table_markdown` and table model.

1. For the first bounded repair, label the current operation “Insert Markdown Table (text)” consistently in the menu, palette, and help. Explain that it inserts editable table text. Preserve its content and round-trip behavior.
2. Keep visual table editing explicitly incomplete in the capability inventory. Do not draw decorative cell borders over unrelated text and claim cell editing works.
3. When visual table editing is separately authorized, split it into model-to-cell projection, measured row/column layout, cell selection/editing, and pagination/export checks. Reuse the existing table parser/model; each piece needs its own behavioral evidence before removing the “text” label.

**Done when:** The current command accurately describes its result, and a 3×3 table survives edit/save/reopen without loss. That closes the misleading-label repair only; a future visual-table feature is accepted only after actual cell editing and multi-page export are verified.

### UI-14 — Keep formatting and comments reachable in compact windows

**P1 · Writer; shared compact-layout rule also applies to Sheets/Photo/Motion · FIXED.** Writer's inspector disappears at 1024×720, and `inspector_available` disables toggling it. A comment visible in the wide inspector has no visible anchor highlight on the page. The compact capture therefore removes the visible route to that review context. Photo/Motion also hide their inspectors at compact width; their complete alternate command reachability remains untested.

**Repair result (2026-09-22):** Compact inspector access has visible labels at 1024×720 in all four affected apps: Writer and Sheets show a `Format` action, Photo shows `Format`, and Motion shows `Inspector`. Writer's anchored comments rebase with text edits, project a marker/range using the same comment ID, and `Show on page` selects the anchored text, opens the review inspector, and scrolls to it. Sheets remembers the user's inspector show/hide choice across responsive layout changes. Writer now keeps the compact inspector mounted while hidden so opening it triggers its focus handler. A regression test drives comment navigation, formatting, Escape, Space on the restored Format trigger, adding a follow-up comment, and returning to the original anchor at 1024×720 through Slint keyboard events. Existing Sheets app coverage verifies focus on open and return to its Format trigger on close. Photo/Motion compact breakpoint screenshots and tests verify the alternate action remains visible; their equivalent full keyboard flow and physical desktop keyboard input remain outside the verified evidence.

**Evidence:** Fresh screenshots: `.work/audit-2026-09-22/ui14/writer-1024x720.png`, `writer-1440x900.png`, `sheets-1024x720.png`, `sheets-1440x900.png`, `photo-1024x720.png`, `photo-1440x900.png`, `motion-1024x720.png`, and `motion-1440x900.png`. They are preserved with command-level verification notes in `loom-bootstrap/audit-evidence-2026-09-14-and-22.zip` under `.work/audit-2026-09-22/ui14/`. At 1024×720, the Photo capture visibly shows `Format`; Motion visibly shows `Inspector`. Tests passed: Sheets app `102 passed`; Writer app `80 passed` including the new keyboard regression; Writer core `82 passed`; focused Photo and Motion responsive breakpoint tests `1 passed` each. Rust formatting check for changed Rust files and `git diff --check` passed. The full Photo app suite was started but its deep image-processing test saturated CPU for over 13 minutes; it was interrupted, so only its focused responsive regression is recorded as verified. The Writer keyboard path is verified through app-level Slint key events, not through physical desktop keyboard input; Photo/Motion keyboard paths remain unverified.

**Open:** Writer `src/main.rs`, `apply_layout_breakpoints`, `on_toggle_inspector`, `writer_render_projection`; full base path `loom-writer/crates/loom-writer-app/`; `ui/app.slint`, `ui/inspector.slint`, and `ui/writer_components.slint` there. Use the corresponding app's breakpoint/inspector paths only when that app's stage is active.

1. First repair compact access: collapse the side panel, but keep a labeled Format/Review action available. Open the same controls in a dismissible overlay or drawer at compact width. Do not build a second set of editing callbacks.
2. Opening the panel must move focus inside; Escape/Close must restore focus to its trigger. Keep the document selection while choosing a property or comment.
3. In a separate patch under this card, display a comment marker/range on the page and connect it to the same comment ID in the panel. Fix CODE-17's anchor drift before relying on the displayed range.
4. Recheck each other app's compact inspector when its stage starts. Record missing alternate actions; do not infer reachability just because a palette exists.

**Done when:** At 1024×720, select text, change its style, add a comment, close/reopen the review panel, and navigate between the comment and its anchored text using pointer and keyboard. No forced window enlargement is needed. Repeat at 1440×900 with the docked panel, without duplicate controls or lost selection.
### UI-15 — Put Present's navigator in the expected reading position

**P2 · Present · OPEN · Visual recommendation and contract mismatch.** In the captured left-to-right layout, the slide filmstrip sits to the right of the canvas, beside another right-hand inspector. The contract specifies a left navigator and right inspector. This makes navigation and properties compete in one region. The empty inspector also repeats “No element selected” above several meaningless dashes.

**Evidence:** Present `01-1440x900-light.png`, `02-1024x720-light.png`, `03-1440x900-dark.png`.

**Open:** `loom-present/crates/loom-present-app/ui/app.slint`, the `root.rtl`/`!root.rtl` branches placing `PresentSlideStrip`; `ui/inspector.slint`; Present workspace settings in `loom-design-bible/contracts/desktop-ui.toml`.

1. Move the existing filmstrip before the canvas for LTR and after it for RTL. Keep the inspector on its contract-defined side. Change placement, not slide state or selection callbacks.
2. Show one useful empty inspector message: “Select an object to edit its position and style.” Hide inapplicable geometry rows until an object is selected.
3. Preserve filmstrip scrolling, add/select/reorder behavior, and the compact panel policy.

**Done when:** The active slide is easy to locate at 1024×720 and 1440×900. Selecting, adding, and reordering a slide updates the same canvas and selection. Check RTL separately; do not accidentally put both panels on the same side again. Keyboard focus order follows the visual arrangement.

### UI-16 — Fit Photo's whole image inside its viewport

**P1 · Photo · FIXED · Rendered defect with a source-level geometry cause.** The initial image loses its right and bottom portions inside a clipped viewport even though the surrounding canvas has space. The selection outline is also only partially visible. The image-fit properties use the enclosing stage's size rather than the smaller viewport's size.

**Repair result (2026-09-15):** Commit `1540a20` sizes and centers the Photo image from the actual viewport and uses the same rectangle for overlays and pointer mapping. Photo app tests pass.

**Evidence:** Photo `01-1440x900-light.png`, `02-1024x720-light.png`, `03-1440x900-dark.png`.

**Open:** `loom-photo/crates/loom-photo-app/ui/photo_components.slint`, `viewport`, `scaled-width`, `scaled-height`, and the four `image-content-*` properties.

1. Create a test image with a different labeled marker in each corner. Start at the app's default fit view.
2. Compute fitted image width/height from the actual viewport width/height and image aspect ratio. Compute its centered x/y from those same viewport dimensions. Do not use `parent` when it refers to the larger stage.
3. Use the same image rectangle for selection overlays and pointer-to-image coordinates. Do not remove clipping globally; deliberate zoom/pan still needs viewport clipping.

**Done when:** All four corners are visible at fit in 1024×720 and 1440×900, for wide, tall, and square images. A click on each corner targets that image corner. Zoom/pan and reset-to-fit work, and export retains the complete original image. Check light and dark themes.
### UI-17 — Make Motion's preview represent the exported frame

**P1 · Motion · FIXED · Rendered/source mismatch.** The stage and SVG export previously represented different colors/text/geometry, so users could not trust the stage as the output preview.

**Repair result (2026-09-22):** The repair samples the composition once into a frame projection and uses the same SVG serialization for the stage image and exported SVG. Composition dimensions, sampled text, visibility, transforms, opacity, and document colors are shared; stage guides and selection handles stay outside the output.

**Evidence:** `.work/audit-2026-09-22/ui17/REPAIR-VERIFICATION.txt` and four 1024×720/1440×900 light/dark screenshots, preserved under `.work/audit-2026-09-22/ui17/` in the evidence ZIP. Motion app tests (33) and core tests (46) pass. Independent `resvg` rendering verified a 640×360 fixture, document color samples, text, and exact preview restoration after transform Undo. No complete video render was tested. The archive SHA-256 is `ddd0be8f7eeeb4cd387ce60a65438df7102273839e9cf590b8cc915090047a95`.
### UI-18 — Let users type precise transform values

**P2 · Photo and Motion · OPEN · Visual design limitation.** Transform inspectors show numbers beside sliders, but the numbers are static text. A slider alone is a poor way to place a layer at an exact coordinate or angle.

**Evidence:** Photo/Motion wide light captures and their inspector source. This is a precision-entry recommendation, not a claim that all keyboard nudging is absent.

**Open:** `loom-photo/crates/loom-photo-app/ui/inspector.slint` and `loom-motion/crates/loom-motion-app/ui/inspector.slint`, transform value text/slider rows; existing transform callbacks in each app's `src/main.rs`.

1. Repair one app at its permitted stage. Replace the static value label for one transform row with the shared numeric input, leaving the slider for rough adjustment.
2. Route typed and dragged values through the same existing mutation function. Show units: px, degrees, or percent. Use the model's legal range; reject nonfinite/invalid input without silently changing the layer.
3. Commit a typed edit once on Enter or focus completion, cancel on Escape, and preserve one undo transaction. Extend the verified pattern to the remaining transform rows.

**Done when:** Type x=123 px, rotation=12.5 degrees, and scale=75% where supported; the model and visible value agree after Save/Open. Enter, Escape, invalid text, drag, Undo, and Redo behave predictably. Typed controls remain reachable through UI-14's compact panel.

### UI-19 — Distinguish Video timeline zoom from scrolling

**P2 · Video · OPEN · Rendered and source-confirmed.** The timeline toolbar reads “Zoom out timeline,” a slider, “Zoom in timeline,” then another slider. The second slider actually scrolls. Long button captions make the pair look like two zoom controls and obscure the different action.

**Evidence:** Video `01-1440x900-light.png`, `02-1024x720-light.png`; `components.slint` has a zoom slider and a separate `Timeline scroll` slider. This is not two zoom sliders in the implementation.

**Open:** `loom-video/crates/loom-video-app/ui/components.slint`, timeline `ToolbarButton` and `Slider` rows; shared toolbar rendering used by that component.

1. Group minus button, one visibly labeled Zoom control, and plus button together. Use shared icon buttons with accessible names and tooltips for minus/plus.
2. Show timeline position with a distinct horizontal scroll control attached to the track area. Preserve the existing scroll value and callback.
3. Keep zoom anchored to the playhead or a clearly specified viewport anchor so changing magnification does not unexpectedly lose the working location.

**Done when:** Zoom changes displayed duration while scroll changes the visible start time. A native user can do both at 1024×720 using pointer and keyboard. Labels/tooltips/accessibility names distinguish the operations, and the playhead remains understandable.

### UI-20 — Keep clip and region captions readable on their fills

**P2 · Video and Studio · OPEN · Visible accessibility risk.** Small secondary clip text is difficult to distinguish from the colored Video clips; Studio's light captions on cyan regions are also at risk. A screenshot establishes the risk, not a measured contrast ratio or full accessibility verdict.

**Evidence:** Video/Studio `01-1440x900-light.png` and `03-1440x900-dark.png`.

**Open:** `loom-video/crates/loom-video-app/ui/components.slint`, clip fill/secondary text; `loom-studio/crates/loom-studio-app/ui/studio_arrangement.slint`, region fill/name; shared semantic color tokens and contrast contract.

1. At the permitted app stage, list the actual background/foreground pairs for normal, selected, muted, and disabled clips. Measure each pair against the contract.
2. Use a shared semantic foreground paired with each region fill. Do not apply a generic muted-text color over arbitrary saturated backgrounds.
3. Preserve clip type/selection meaning through icon, border, and text as well as color. Keep readable labels when a region is short; expose the full name through focus/tooltip.

**Done when:** Measured pairs meet the contract in light, dark, and high-contrast modes. Real screenshots at 1024×720 and larger show readable name, duration/source status, and selected state. Keyboard focus and screen-reader naming get separate checks.

### UI-21 — Give Studio one transport control for each action

**P2 · Studio · OPEN · Rendered/source-confirmed duplication.** Loop and Metronome each appear twice across the main toolbar. This consumes scarce width and makes users wonder whether the copies control different things.

**Evidence:** Studio `01-1440x900-light.png`, `02-1024x720-light.png`; duplicate instances in `ui/app.slint`. Actual audio device operation was not tested.

**Open:** `loom-studio/crates/loom-studio-app/ui/app.slint`, Loop/Metronome toolbar instances; `ui/studio_transport.slint`; their callbacks in `src/main.rs`.

1. Keep one Loop toggle and one Metronome toggle in the transport group next to playback. Remove only the duplicate toolbar instances, not the command or keyboard shortcut.
2. Make the controller state authoritative. A single activation must change the state once; avoid both the UI and callback independently inverting it.
3. Use the same checked state, accessible name, and shortcut in any menu projection. Keep transport visible at compact width without wrapping.

**Done when:** Each control appears once in the main toolbar. Clicking once toggles once; its keyboard command and menu show the same state. Save/reopen behavior follows the existing model policy. Verify playback with an actual configured device separately; headless “device unavailable” messages are expected capture behavior.

### UI-22 — Show Studio users what is inside an audio region

**P2 · Studio · OPEN · Visible capability gap/design recommendation.** Audio regions are mostly solid colored rectangles with an icon and filename. There is no waveform to locate speech, silence, or a transient, and the arrangement lacks a useful sequence of time ticks. The screen gives little help deciding where to cut.

**Evidence:** Studio wide/compact light captures; `studio_arrangement.slint` renders icon/text but no sample peaks. Empty room below tracks is working space and is not itself a defect.

**Open:** `loom-studio/crates/loom-studio-app/ui/studio_arrangement.slint`, region rendering and time header; existing region/audio-asset projection in `src/main.rs`; decoded PCM access in the core.

1. Start with one bounded audio-waveform patch: compute min/max sample peaks for visible time buckets from the actual decoded asset. Cache by asset revision and scale; do not fabricate decorative wave shapes.
2. Draw those peaks inside the existing region rectangle. Respect channel count, region offset, trim, and zoom. Missing audio gets an explicit unavailable state.
3. In a separate patch, add measured time ticks aligned with region positions and the playhead. Label seconds or beats according to the actual project time base; do not mix them silently.
4. Keep MIDI previews as a separately scoped event-data projection if supported; do not render audio peaks for MIDI or claim note editing exists.

**Done when:** A fixture with sound/silence/sound visibly matches its actual waveform; trimming and zooming preserve alignment. A click at a labeled tick lands at that time. Work remains responsive for a long file and multiple tracks. Record native audio verification separately from a rendered peak display.

### UI-23 — Make Encode's empty queue and readiness truthful

**P1 · Encode · FIXED · Rendered/source-confirmed state mismatch.** The initial screen contains `sample-input.mov`/`sample-output.mp4` as a queued job, says “Queue ready,” and simultaneously reports “Encoder unavailable.” Start is correctly disabled in the captured state; the contradictory status and fabricated-looking job are the defects.

**Repair result (2026-09-15):** Commit `9e15c55` starts Encode with an honest empty queue and derives readiness/start state from real prerequisites. Encode app tests pass (16 tests).

**Evidence:** Encode `01-1440x900-light.png`, `02-1024x720-light.png`; `sample_queue`, `refresh`, and `run_gui` in the app source. No real media was encoded in this visual run.

**Open:** `loom-encode/crates/loom-encode-app/src/main.rs`, `sample_queue`, startup status, readiness calculation; `ui/encode_queue.slint`, `ui/encode_job_card.slint`, and `ui/encode_progress.slint` in that app.

1. Start an ordinary fresh user session with an empty queue and one clear Add Files action. Keep synthetic jobs only in an explicit demo/capture fixture, identified as examples and never runnable as user jobs.
2. Derive summary status and Start availability from the same readiness state. Distinguish no jobs, missing input, missing encoder, invalid destination, ready, running, failed, and finished.
3. Show the first actionable blocker beside the affected job or setup control. Do not display “Queue ready” merely because a queue object exists.

**Done when:** Fresh native launch shows no invented source file. Add a real test source, remove/restore it, use an invalid destination, and test missing/available encoder. Visible status and Start agree in every case. A completed controlled encode produces a validated output file before showing success. Keep CODE-09's no-overwrite protection as a separate prerequisite.
### UI-24 — Make Encode's job settings fit the editing task

**P2 · Encode · OPEN · Visual hierarchy recommendation.** Queue, a large mostly empty “Active Job” card, and a seven-tab inspector compete for space. The selected source is repeated, tabs wrap into several rows, and destination appears under both Source & Destination and Destination. At 1024×720, substantial space still goes to idle progress instead of useful settings.

**Evidence:** Encode wide/compact light captures and dark variant.

**Open:** `loom-encode/crates/loom-encode-app/ui/app.slint`, `ui/encode_progress.slint`, `ui/encode_inspector.slint`, `ui/encode_job_card.slint`; Encode workspace contract in `loom-design-bible/contracts/desktop-ui.toml`.

1. First remove duplicated selected-job headings and show the large progress detail only for an actual running/completed/failed job. Idle state should direct the user to add/select a job.
2. Give Destination one canonical editing location. If another view summarizes it, make that view read-only and link it to the same setting.
3. Group settings under Source, Output, and Advanced using the shared navigation/disclosure pattern. Keep every existing setting and callback; do not hide required settings behind an unlabeled icon or wrapped tab strip.
4. At compact width, prioritize queue plus the selected job's settings. Open secondary detail on demand. Use an accurate destination action such as “Choose output folder/file”; only promise dropping a destination if that action is actually implemented.

**Done when:** At 1024×720 and 1440×900, add two jobs, select each, set distinct destinations/presets, and verify they remain independent after Save/Open. Settings labels never overlap or wrap into ambiguous tab rows. Progress becomes visible for the running job without stealing the user's selected-job edits.

### UI-25 — Help users recover from missing media tools

**P1 · Video and Encode · NEEDS_REVIEW · Compact recovery code fixed; real-media acceptance open.** The apps detect missing media backends, but recovery guidance is terse and Video truncates its installation instruction in a narrow status area. Telling a desktop user to put several tools “on PATH” is not a complete setup flow. Missing software on this machine is an environment fact; the unclear recovery path is the product finding.

**Repair result (2026-09-15):** Commit `9e15c55` adds readable missing-tool setup, choose/check-again actions, bounded probes, and truthful blocked states in Video and Encode. App tests pass; a real configured playback/encode fixture remains.

**Repair result (2026-09-22):** Video’s blocked-start recovery card now stays tall enough for the complete explanation and both actions at 1024×720. Encode’s unavailable status now tells the user to choose an FFmpeg executable or use Check again. Added compact snapshot and guidance regression tests. Video app tests pass (34); Encode app tests pass (17); shared setup-probe tests cover valid identity, wrong executable, missing executable, and bounded timeout (2). The repair report and fresh compact screenshots are preserved under `.work/audit-2026-09-22/ui25/` in the portable evidence ZIP. Encode formatting and `git diff --check` pass; Video formatting still reports five unchanged pre-existing one-line `let-else` statements, left alone per the small-repair rule.

**Evidence:** Fresh repaired compact light captures: `.work/audit-2026-09-22/ui25/video-1024x720-fixed.png` and `encode-1024x720-fixed.png`, plus command-level results in `REPAIR-VERIFICATION.txt`. No claim is made that codecs or playback worked during this run.

**Remaining acceptance check:** On a machine with a real local FFmpeg/FFprobe installation, select it in Video and Encode, use Check again without restarting, confirm the media action becomes available, and run a real playback/export/encode fixture. Do not install tools automatically as part of this repair.

1. Repair the active app first. Put one readable setup message near the blocked action, naming the exact missing tools. Keep status-bar text short and let the user open full details.
2. Provide Choose executable/folder and Check again actions using the existing file chooser and backend detection. Validate the selected tool with a bounded version/probe call; store a successful local path through existing settings.
3. Explain which tasks are unavailable and leave unaffected document editing usable. A failed check preserves the current project/queue and displays its actual error.
4. Keep optional installation instructions outside the main editing canvas. Do not automatically download or install tools as part of this repair.

**Done when:** With tools absent, the complete explanation is readable at 1024×720. Select a valid local installation, check again without restarting, and verify the formerly blocked action becomes available. Wrong path, wrong executable, and a hanging probe produce bounded, actionable errors. Run one real playback/export/encode fixture once the backend is configured.

### UI-26 — Name the compact Export CSV action for screen readers

**P1 · Sheets accessibility · FIXED.** At compact width the toolbar shows Export as an icon with no visible text. Its AT-SPI button name and description were both empty, so a screen-reader user could not tell what the action does.

**Reproduce:** Enable the Linux screen reader, launch Sheets at 1024×720, and inspect or navigate the compact toolbar after Chart. The export button is unnamed.

**Repair result (2026-09-23):** The export button now exposes `Export CSV` as its accessible name and `Export the active worksheet as CSV` as its description while keeping the compact visual layout.

**Evidence:** The rebuilt app's AT-SPI tree reports the exact name and description. Native window captures and the accessibility run are recorded in `.work/sheets-acceptance-2026-09-23/REPORT.md`.

**Done when:** The compact export icon has a meaningful AT-SPI name and description, and its existing export action remains reachable. Broader screen-reader command coverage remains part of the Sheets gate.

### UI-27 — Keep chooser focus and announcements inside the dialog

**P1 · Sheets accessibility/keyboard · FIXED.** At launch the template chooser looked like Blank was selected, but focus belonged to an unnamed window-level scope outside the dialog. AT-SPI showed the app frame focused and the named chooser group unfocused; Orca did not announce the selected template. After Return, focus also remained on the frame instead of the new worksheet.

**Reproduce:** Enable Orca, launch with `--template-chooser`, and inspect the AT-SPI focused object. Press Right and Return, then inspect focus again. Before repair, the chooser group was not focused and the newly created workbook had no focused grid group.

**Repair result (2026-09-23):** The keyboard scope now lives inside the named chooser group. Opening the chooser focuses it; its description follows the selected template and its keys. Closing by Escape or Return restores focus to the worksheet grid, unless a save-changes dialog is active.

**Evidence:** AT-SPI reports `Template chooser` focused on open and `Checklist worksheet grid` focused after Return. Orca spoke `Checklist selected. Use Left and Right to choose; Return creates it; Escape cancels.` Native screenshots cover selected Checklist, canceled chooser with the original workbook, and the created Checklist workbook in `loom-sheets/docs/qa-native/`. Full details: `.work/sheets-acceptance-2026-09-23/REPORT.md`.

**Done when:** On the native Linux chooser, focus enters the named dialog, selection changes are announced, Return creates the visible selected template and returns focus to its grid, and Escape preserves the current workbook and returns focus to its grid. Other Sheets workflows still need broader accessibility checks.

### CODE-20 — Keep exported worksheet names distinct

**P1 · Sheets · FIXED.** Excel sheet names cannot contain some characters and have a 31-character limit. If two Loom tabs become the same name after those rules are applied, export must not silently merge their references or show the wrong tab.

**Reproduce:** Make tabs named `A/B` and `AB`, export to XLSX, and inspect the workbook's sheet list and a formula that uses both tabs. Before repair, both tabs became `AB`.

**Repair result (2026-09-24):** XLSX export now gives each sanitized name a unique suffix such as `AB (2)`, shortens names before adding the suffix, and rewrites formulas and chart references to the exported names in one pass. Duplicate source names are rejected because formulas cannot identify which duplicate tab they mean.

**Verification:** `xlsx_export_keeps_sanitized_sheet_names_unique_and_rewrites_references` passes. LibreOffice Calc 24.2.7.2 opened and converted the fixture; it kept `AB`, `AB (2)`, and the shortened long tab, recalculated the cross-sheet formula to `80`, and kept the quoted text `A/B!B2` as text. Exact commands and results are in [the tracked report](AGENTS.md#source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md) and the portable audit bundle. Microsoft Excel was not tested.

### CODE-21 — Put spilled array results in the right cells

**P1 · Sheets · FIXED.** `SEQUENCE(2,3)` means two rows and three columns. If the app swaps those numbers, the visible grid gets wrong values, and formulas that read spill cells can show stale results.

**Reproduce:** Put `=D2+1` in A1 and `=SEQUENCE(2,3)` in B1. Before repair, the spill used the wrong width/height and A1 did not update after D2 was filled.

**Repair result (2026-09-24):** Spill placement now uses `rows` for row movement and `columns` for column movement, keeps values in row-major order, and reruns formulas whose earlier reads changed. It checks all target cells before writing any value, so a blocked spill leaves no partial cells behind.

**Verification:** `sequence_spills_rows_and_columns_and_recalculates_earlier_readers` and `blocked_sequence_spill_does_not_leave_partial_values` pass. Core suite: 109 passed.

### CODE-22 — Reuse calculated values while the user navigates

**P2 · Sheets · FIXED for view-only updates.** Clicking a cell, scrolling, or resizing does not change formulas. Recalculate only when the workbook changes or the active tab changes; otherwise navigation needlessly repeats the full workbook calculation.

**Repair result (2026-09-24):** The app keeps the active sheet's calculated values and reuses them for selection, scroll, and resize. A cell/workbook change refreshes the cache, and switching to a different active tab forces fresh values. Values are shared by reference so reuse does not copy the whole result map.

**Verification:** The evaluation-cache regression checks same-tab reuse, tab switching, and refresh after an edit. `rename_rewrites_qualifiers_and_rejects_collisions` verifies the cross-sheet value remains `25` after a real tab rename/switch. App suite: 116 passed plus the cache test.

**Remaining performance gate:** Formula-bar commits, picker/startup Open parsing, and native Save/Save As now use background workers. Save N has a revision barrier, one package build, an atomic file write, a checkpoint of the same bytes, a separate FIFO completion, and dirty-state preservation for later revisions. CSV/XLSX exports and many full-workbook copies still run synchronously or on the UI thread. Recovery can lag behind accepted edits and has no reviewed storage bound. Callback and visible-frame timings, native million-cell scrolling, app memory, accessibility, and broader interoperability remain unverified; passing unit tests do not close those checks.

### CODE-23 — Warn before importing XLSX features Loom cannot keep

**P1 · Sheets data integrity · NEEDS_REVIEW.** Opening an Excel file that contains features Loom does not model can throw those features away. The import message only said `Imported`; it did not tell the user what would be lost.

**Original source evidence (2026-09-24):** `extract_xlsx_sheets` maps cells, styles, alignments, and supported drawings into Loom's workbook model. It does not inspect workbook defined names or external-link parts, or worksheet conditional-formatting and data-validation parts. The model holds only one chart per sheet, so each additional imported chart replaces the preceding chart. Freeze panes and custom row/column sizes have fields in `Sheet` but the importer does not populate them. Missing drawing relationships and media can also make `import_drawings` return success while skipping the object. Neither `open_workbook_from_picker` nor startup `--open` reported these omissions before loading the workbook. Normal Save does not overwrite the original `.xlsx`: import clears the Loom save target, which limits damage. The imported Loom copy can still be degraded, and any later `.loomtable` save cannot restore omitted features.

**Repair result (2026-09-24):** `import_xlsx_sheets` now returns a warning list for defined names, external links, conditional formatting, data validation, PivotTables, frozen panes, custom row/column sizes, additional charts on a sheet, and missing drawing/media parts. It checks actual `definedName` entries so an empty `<definedNames/>` container does not raise a false warning. The picker stages a warning modal before replacing the open workbook. Startup `--open` stages the same modal and keeps a recovered workbook visible until the owner accepts the import; cancel does not replace its recovery snapshot. The modal names the losses, says that the source `.xlsx` is unchanged, offers `Cancel import` or `Import with feature loss`, and routes Escape to Cancel from a focus scope that contains the dialog controls. The shared command dispatcher blocks native-menu and palette actions while the warning is open, and a second guard drops native commands already queued when the warning appears. Continue replaces the workbook and reports the dropped feature list. The original `.xlsx` remains unchanged and imported spreadsheets still need Save As.

Focused verification: all 121 Sheets core tests and 142 Sheets app tests pass. The full workspace passes **265 tests** (142 app, 2 cache integration, 103 core unit, 13 formula, 1 performance fixture, and 4 range). Coverage includes one fixture per known warning, a supported workbook with no warnings, the empty-definedNames no-warning fixture, file-loader reporting, cancel-preserves-state including an on-disk recovery reopen, continue-replaces-state, startup-recovery preservation, native/palette dispatcher blocking, rendered warning, and Tab then Escape cancellation. Logs: `.work/sheets-acceptance-2026-09-24/xlsx-import-review-red-app.log`, `xlsx-import-review-red-core.log`, `xlsx-import-review-green-app.log`, `xlsx-import-review-green-core.log`, and `xlsx-import-warning-workspace-tests-final.log`, `workspace-tests-split.log`, `clippy-split.log`, and `build-split-time.log`, `save-changes-escape-red.log`, and `save-changes-escape-green.log`. The fresh production build took 3m20s and peaked at 2,983,908 KiB RSS with zero swap; binary SHA-256 is `616fc22b33bd9ead122b2f00729de417d503fe0940bd4f3f538dfa8d608345f6`. A native live-window screenshot was captured at `loom-sheets/docs/qa-native/ui23-xlsx-import-warning-live-linux.png` while the screensaver was inactive. It shows the CODE-23 warning and visible application menu row. Dialog focus/actions and menu-popup behavior remain unverified because native-app controls are unavailable in this session. Broader Excel/LibreOffice interoperability remains separate. Keep this card `NEEDS_REVIEW` until the live dialog is inspected and captured while the desktop is unlocked.

**For a tiny coding model:** (1) Make one short list of XLSX parts Loom keeps and parts it does not keep. Include the command-line `--open` path. (2) Before replacing the current workbook, check the XLSX for known dropped parts. Read XML element names and relationship types; do not search arbitrary text, because cell text can contain the same words. Count actual `<definedName>` children; an empty `<definedNames/>` container contains nothing to lose. (3) If a dropped part exists, show its name and offer Continue or Cancel before changing the workbook. Cancel must keep the current cells, tab, undo history, save target, and recovery data. Block all shared, keyboard, palette, and native-menu commands until the user decides; check queued native actions again when they reach the UI. (4) Apply the same warning during startup `--open`; if a warning cannot be shown, stop with a clear error instead of silently importing. Put the Escape handler around the actual dialog controls so Escape still cancels after Tab moves focus to a button. (5) Add one small test file for every known omission, plus a supported file that shows no warning. Test Cancel and Continue. The original `.xlsx` must stay unchanged. Do not claim full Excel compatibility. Preserve extra XML only when it can be safely round-tripped through edits and saves; never guess where unknown XML belongs.

**Done when:** Users see a clear warning before Loom drops each known unsupported feature, cancellation leaves the open workbook unchanged, supported imports do not show a false warning, command-line startup cannot bypass the warning, and the documented feature list matches tested behavior. Broader Excel and LibreOffice feature coverage remains a separate interoperability gate. Keep `NEEDS_REVIEW` until the warning's live focus and button behavior are checked; the visual capture alone does not confirm them.

### CODE-24 — Link XLSX cell styles so other spreadsheet apps apply them

**P1 · Sheets data integrity · FIXED.** An exported workbook can contain formatted cells and `xl/styles.xml` yet show those cells without formatting in another spreadsheet app if the workbook does not link to the style part.

**Reproduce:** Export a sheet with a styled total cell and open it in LibreOffice Calc. Before repair, the XLSX contained the style table, the correct cell style index, and a content-type entry, but the workbook relationship file did not point to `styles.xml`. Calc therefore imported the total as a default-style cell. Adding the `apply*` flags alone did not fix it; adding the missing workbook relationship did.

**Repair result (2026-09-24):** `export_xlsx_sheets` now adds a unique workbook relationship from `xl/workbook.xml` to `xl/styles.xml`. The relationship ID follows the worksheet and shared-string relationships generated by the base workbook exporter.

**Verification:** `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-core --lib xlsx_export_links_the_styles_part_from_the_workbook -- --nocapture` failed against the old exporter and passes after the fix; red/green output is in `.work/sheets-acceptance-2026-09-24/interop/style-link-red.log` and `style-link-green.log`. `libreoffice --headless -env:UserInstallation=file:///tmp/loom-sheets-interop-verified-20260924 --convert-to ods --outdir .work/sheets-acceptance-2026-09-24/interop/lo-verified .work/sheets-acceptance-2026-09-24/interop/loom-interop-matrix-fixed.xlsx` succeeded; `python3 .work/sheets-acceptance-2026-09-24/interop/assert_ods.py .work/sheets-acceptance-2026-09-24/interop/lo-verified/loom-interop-matrix-fixed.ods` passed. The assertion checks the yellow fill, bold font, border, right alignment, and two-decimal currency format on B4; cross-sheet formula results `80`, `30`, and `60`; renamed sheet tabs; literal text; chart frame; and shape label. The source fixture, converted workbook, logs, and assertion script are in the portable audit evidence bundle. Full workspace tests pass (266), workspace Clippy passes, and the app builds; exact commands and output are in the tracked follow-up report. This does not verify Microsoft Excel or the full XLSX feature matrix.

**Commit:** `ba76d16` (`fix(sheets): link exported XLSX styles`), pushed to `origin/main`. **Remaining gap:** Microsoft Excel and the broader XLSX import/export feature matrix still need separate validation.

**For a tiny coding model:** (1) Export a workbook with one formatted cell. (2) Check that the workbook relationship file links to `styles.xml` and that the worksheet cell points to a style index. (3) Add the relationship without reusing an existing ID. (4) Run the focused regression and open the output in another spreadsheet app. (5) Check the actual fill, font, border, alignment, and number format. A style XML file inside the ZIP is not enough if the workbook does not link to it.

**Done when:** The regression passes, LibreOffice applies each tested cell format to the exported workbook, and remaining Excel and feature-matrix gaps stay documented separately.

### CODE-25 — Import standard DEFLATE-compressed XLSX packages

**P1 · Sheets import integrity · FIXED.** Normal Excel workbooks compress their ZIP entries with DEFLATE. Loom's shared ZIP reader accepted only uncompressed entries, so a Calc-generated workbook failed before the Sheets importer could inspect it, with `unsupported compression method 8`.

**Reproduce:** Convert a valid Loom XLSX fixture through LibreOffice Calc so it is written as a standard compressed `.xlsx`, then run `import_xlsx_sheets` on the result. Before repair, the shared ZIP reader returned `UnsupportedMethod(8)` before parsing any workbook entries.

**Repair result (2026-09-24):** `loom-package` now reads ZIP methods 0 (stored) and 8 (DEFLATE), plus entries that set the data-descriptor flag and write sizes after their compressed data. The central-directory sizes are authoritative when that flag is set; the optional descriptor signature, descriptor values, flags, methods, and local/central names are checked. Declared expanded sizes are checked against the existing per-entry and total limits before inflation. The decoder can emit at most one byte beyond the declared size, so a false small size cannot trigger unbounded output allocation. CRC is checked against the expanded bytes. The writer remains stored-only; the reader continues to reject encryption and unsupported compression methods.

**Verification:** Two small fixtures from Python's standard ZIP writer cover normal DEFLATE and a streamed DEFLATE entry with a signed data descriptor. Tests also remove the optional descriptor signature to exercise the unsigned form, reject corrupt descriptors, verify CRC after inflation, and enforce both per-entry and total uncompressed-size limits. `cargo test --manifest-path loom-core/Cargo.toml --locked --offline -p loom-package` passes **29 tests**. Package all-target Clippy and fresh Sheets workspace all-target Clippy both pass; the full workspace test run passes **266 tests**. A production Sheets app build passes in **3m07**, peaking at **2,985,580 KiB RSS** with zero swaps; its binary SHA-256 is `bbfb0c4c42d018137eef43ae905c0928424cc11d16ee13992d5ff3504d90c9f4`. Clippy and build logs are in `.work/sheets-acceptance-2026-09-24/`. LibreOffice Calc **24.2.7.2** converted the rich Loom XLSX fixture to ODS and back to XLSX; Loom then imported four sheets, formulas, the styled B4 total, a chart, and a shape label. It reported `CustomRowColumnSizes`, which the existing import warning already names. Probe source, output, and logs are in the portable evidence bundle. Microsoft Excel and the wider XLSX matrix remain unverified.

**Commits:** `82e3ca0` (`fix(package): read deflated ZIP entries`) and `bbda477` (`test(package): cover unsigned ZIP descriptors`), both pushed to `origin/main`. The direct dependency was recorded in all nine active app/workspace lockfiles that use `loom-package`.

**For a tiny coding model:** (1) Find the compression method and sizes in the ZIP central directory. (2) Keep method 0 as a plain byte copy and use a raw DEFLATE decoder for method 8. (3) If bit 3 is set, use central-directory sizes because the local header can say zero; then check the data descriptor after the compressed bytes. (4) Before decoding, reject any declared uncompressed size over the per-entry limit or remaining total limit. Read no more than the declared size plus one byte. (5) Compare the produced byte count and CRC with the central directory. (6) Add one ordinary compressed fixture and one data-descriptor fixture, then import the Calc-generated workbook and inspect cells, formatting, formulas, charts, and drawings. Return a clear import error for corrupt or unsupported entries; never replace the current workbook on a failed import.

**Done when:** Both fixtures and all limit/corruption checks pass, the Calc-generated workbook reaches the Sheets importer with its supported values and objects intact, and the warning names any feature Loom drops. This fixes standard ZIP DEFLATE import; it does not certify Microsoft Excel compatibility or the full XLSX feature matrix.

### PERF-01 — Keep large workbook work from freezing the window

**P2 · Sheets performance · NEEDS_REVIEW.** A fast calculation benchmark is only one part of performance. Users also need immediate feedback while editing, smooth scrolling, and a known memory limit.

**What is measured:** The latest optimized 10,000-chained-formula test passed at 88.5 ms on this device's Intel Core i3-2350M (2 cores, 7.7 GiB RAM). The opt-in release test fills a 2,048×2,048 sheet with 1,000,000 pseudorandomly placed numeric cells and projects 60 viewports, including both workbook corners, at 0.17 ms p95 and 0.26 ms maximum; `/usr/bin/time -v` measured 90,308 KiB peak RSS and no swap for that test process. The app-level test-profile run measured 1.39 ms p95 / 1.43 ms maximum for CPU-side viewport projection. For a cell edit, only UI preparation (0.049 ms) and mailbox submission (0.037 ms) were timed; the complete callback, Slint model/status projection, and input-to-visible-frame delay were not measured. Worker evaluation took 2,561.653 ms, recovery package creation 12,721.371 ms, and recovery journal writing 43,046.444 ms. The test process peaked at 2,612,252 KiB RSS with no swap. The test fixture and harness are included; this is not interactive-app RSS. These measurements exclude native rendering and presentation. Recovery at this size can lag far behind the visible edit and is a durability blocker. Non-cell edits still copy `Vec<Sheet>` on the UI thread. A 2026-09-24 source audit mapped the then-synchronous Open, Save, Save As, CSV/XLSX export, and `record_workbook_snapshot` paths; it found Save packaged the workbook twice, copied the saved baseline on the UI thread, then blocked on `checkpoint().recv()`. The 2026-09-25 Open milestone moved picker-selected and startup `--open` parsing to a background loader with one newest waiting request; the Save milestone added a worker barrier and FIFO Save results. The async export/close milestone now queues CSV/XLSX work at a target worker revision, atomically writes on the worker, limits accepted export concurrency to one, and returns results through ordered file completions. Close waits for accepted work, offers Save/Cancel for dirty documents, and keeps full file/error outcomes in the accessible status text. Consumed worker input errors block Save/Export/Close and remain dirty until a successful full replacement covers them. Full callbacks, native presentation, export throughput at workbook scale, the native picker path, Open cancellation during shutdown, and interactive-app memory remain unmeasured. The native picker remains on the UI thread. Many `apply_sheet` callers still clone `Vec<Sheet>` for whole-workbook replacements, Undo/Redo, tab/row/column changes, formatting, ranges, objects, and sizing. No real-window run has measured input response or million-cell frame rate, and Sheets has no reviewed numeric app memory budget.

**For a small coding model:** (1) Run the fixed commands in `AGENTS.md#source-loom-sheets-performance-md` without changing either workload. (2) Keep formula-bar commits as one-cell messages to the worker; do not copy every sheet on the UI thread. Keep at most one calculation running and one newest combined update waiting. (3) The million-cell measurements show that `workbook_package_bytes` and `SnapshotRecovery::record` are the slowest steps. Implement the owner-approved REC-02 journal and checkpoint plan, preserve each accepted edit before formula work, then rerun the same workload and measure durability, retained bytes, replay time, and temporary peak. Do not hide slow recovery behind a fast input field. (4) Keep Open, Save, and CSV/XLSX export on revision barriers with ordered file completion results; clone and serialize on workers, preserve newer edits across checkpoints, and report file-write and recovery-checkpoint results separately. (5) Keep the revision check: show results only for the newest edit and active tab, and do not erase a formula draft being typed. (6) Measure native scrolling and full app peak memory on the same million-cell workbook; ask the owner to set the numeric memory limit before marking the card fixed.

**Done when:** Formula work stays below the mainstream-profile budget; each input shows feedback within 16.7 ms; no calculation or file operation blocks the UI thread; one million randomly placed cells scroll at 60 fps; and peak RSS is below a reviewed numeric limit. Record machine, exact workbook, commands, measurements, and any remaining gap in `AGENTS.md#source-loom-sheets-performance-md`. Current gate details: [the tracked follow-up report](AGENTS.md#source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md).

**Open milestone result (2026-09-25) — NEEDS_REVIEW.** Picker-selected and startup `--open` workbook parsing now runs off the UI thread after the native picker/window is available. One loader may run and one newest request may wait; every file result uses a FIFO completion channel separate from the calculation result slot. Operation IDs, document generations, and target edit revisions reject stale completions. A workbook candidate is installed only after its unsaved-changes decision and any XLSX-loss warning are accepted. Formula drafts remain available through Open preflight and asynchronous completion; Save/Discard/Cancel resolve the pending replacement. Repeated New/Open requests preserve the active Save Changes decision. Startup parse failure preserves the fallback workbook, save path, and recovery payload.

**Open verification:** `SLINT_EMIT_DEBUG_INFO=1 cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --offline --bin loom-sheets` passes **160 tests**. The `open_operation` filter passes **16 tests**; the added XLSX-held-candidate cancellation journey passes separately. `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check` passes. `cargo clippy --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --offline --all-targets -- -D warnings` passes with no Rust warnings from the Sheets app. `cargo build --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --offline --bin loom-sheets` succeeds; Slint emits existing exported-component warnings from the unchanged `loom-ui` crate. Logs: `.work/sheets-open-perf-2026-09-25/perf01-followup-red.log`, `perf01-followup-green.log`, `root-open-app-full-final.log`, `root-open-app-build.log`, and `root-open-app-clippy-final.log`.

**Remaining Open gaps:** The loader cannot be cooperatively stopped during parsing and its JoinHandle detaches on shutdown. Native picker/dialog/close interaction, current live-window screenshot, complete startup wiring plus recovery journey, and full input-to-visible-frame timing remain unverified. Native Save/Save As and asynchronous exports now have focused ordering coverage, but a crash can still lose N+1 before its recovery batch completes; the UI warns that newer edits remain unsaved and recovery may still be catching up. Export throughput at workbook scale, non-cell full-workbook copies, recovery latency/storage bounds, native million-cell scrolling, reviewed app memory limit, visual/accessibility coverage, and broader interoperability remain open. The Sheets acceptance gate stays blocked.

**Save and Save As milestone result (2026-09-25) — NEEDS_REVIEW.** Save/Save As now use a worker barrier tagged with operation ID, document generation, edit revision, and path. The worker packages once, writes atomically, and checkpoints the same bytes; file-write and checkpoint results stay separate on a FIFO queue. UI path/baseline changes require a matching generation. A Save Changes replacement resumes only after a successful write and only when no newer dirty edit or formula draft remains. The interface keeps the replacement choice open for N+1 and reports that newer edits remain unsaved while recovery may still be catching up. An Open candidate that read a path before a successful Save overwrite is reloaded before its warning/installation decision; a user Discard authorizes only the exact revision discarded.

**Save/Open verification:** The controlled overwrite regression was observed RED, then passed. The test that rejects automatic replacement after N+1 also failed first, then passed. The newer-edit status regression failed with the previous “Saved” status, then passed after the status update. Focused Save journey filter: 14 passed; focused Open-operation filter: 17 passed. Full Sheets workspace: 301 tests passed (177 app, 2 evaluation-cache integration, 104 core, 13 formula, 1 performance fixture, 4 range). `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check` passed. `cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` passed; Slint code generation still reports existing exported-component warnings from `loom-ui`. The production app build passed in 1m03s, peaking at 3,055,420 KiB RSS with zero swaps; this is build-process memory, not interactive-app memory. Governance audit passed. Fresh code-structure and asset audits failed: two active Sheets sources exceed their recorded limits, 18 additional code-size findings are outside Sheets, and six generated QA captures lack provenance entries.

**Commit:** `244e1eb` (`fix(sheets): save workbooks asynchronously`).

**Review disposition:** A fresh read-only review found that Save N can complete before an accepted N+1 edit is journaled; a crash in that interval can lose N+1. The Save status now identifies newer unsaved edits and warns recovery may still be catching up, but this milestone does not make N+1 durable at Save N completion. Keep this as an open PERF-01/REC-02 recovery-freshness blocker and do not change the recovery format without its reviewed design. A minor path-scope issue is deferred: the native-write epoch is global, so Save As to B can unnecessarily reload a candidate opened from A and read a newer version of A.

**Sheets source-size/provenance milestone (2026-09-25) — FIXED for active Sheets scope.** CSV interop moved from `loom-sheets-core/src/lib.rs` into `interop.rs` with explicit root re-exports; menu dispatch moved from `actions.rs` into `command_dispatch.rs` without changing call signatures. All six tracked generated renderer screenshots are covered by the `loom-sheets/docs/qa-renderer/*.png` rule in `loom-bootstrap/contracts/assets.toml`. The original audit findings are resolved without touching locked applications.

**Source-size verification:** `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --offline --bin loom-sheets command_dispatch -- --nocapture` passed 14 tests; `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-core --locked --offline --lib csv -- --nocapture` passed 4 tests; `cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline -q` passed 301 tests. `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check`, `cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings`, and `cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` passed. The build completed in 5m59s; Slint emitted existing exported-component warnings from unchanged `loom-ui`. `python3 loom-bootstrap/scripts/audit-governance.py` and `python3 loom-bootstrap/scripts/audit-assets.py` passed. `python3 loom-bootstrap/scripts/audit-code-structure.py` still fails on 18 locked, out-of-scope findings and lists no Sheets findings. `git diff --check` passed. The Sheets audit portion is clear; the repository-wide audit is not.

**Commit:** `c2bdb8b` (`refactor(sheets): split oversized source modules`).

**Next milestone:** REC-02 crash-safe checkpoint retention: preserve the previous valid recovery state through interrupted publication and cleanup, then retain only two complete generations. Keep the overall Sheets gate blocked.

**Save ordering reproducer — RED (2026-09-25):** The focused WSL command was `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --offline --bin loom-sheets workbook_worker::tests::checkpoint_at_revision_n_preserves_later_n_plus_one_recovery -- --exact --nocapture` with `SLINT_EMIT_DEBUG_INFO=1` and the repo toolchain environment. At the time of that RED run, recovered A1 was `Some("saved N")` instead of `Some("unsaved N+1")`; the later N+1 journal write had not completed before the test restarted.

**Async CSV/XLSX export and close-safety milestone (2026-09-25) — NEEDS_REVIEW.** CSV and XLSX exports wait for their accepted workbook revision, package and atomically write on the background worker, and deliver operation results through the FIFO file-completion channel. At most one export is accepted at once. Save and export completions retain their worker sequence and full outcomes, including same-tick success/failure pairs. A close request keeps the window shown while accepted work drains; dirty close offers Save or Cancel, and failures keep the document open. Current-generation worker input failures remain dirty and block Save, Export, and Close until a successful full-workbook resync result covers the failed revision; New/Open route through the existing dirty-workbook decision. The status text keeps full details in its accessibility label while the visual single-line label may elide long history. Two app-specific Slint components hold the close shield and accessible operation status; `main.rs` and `app.slint` remain below their registered legacy ceilings.

**Async export/close verification:** Test-first regressions first reproduced stale worker failure aborting a close that waited for a newer accepted revision; dirty-close prompts and worker failures hiding file results; idle-consumed input failure being lost before Save/Close; New/Open bypassing dirty protection; and a same-tick export result being duplicated. The final app journey filters passed: `idle_worker_input_failure` 3 passed, `close_operation_journeys` 14 passed, and `export_operation_journeys` 9 passed. Full workspace command `cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline` passed 330 tests (206 app, 2 cache integration, 104 core, 13 formula, 1 performance fixture, 4 range). Workspace all-target Clippy with `-D warnings`, production app build, `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check`, `git diff --check`, governance audit, and asset audit passed. The code-structure audit reports 18 findings outside active Sheets source changes and no Sheets source-size finding. Test, Clippy, build, and audit logs are under `.work/sheets-acceptance-2026-09-25/async-export-close-*`. A read-only review found the stale-error and status-overwrite defects; the follow-up latch fix and exact-once outcome correction were reviewed with no remaining finding. This WSL session has no native screenshot utility or running desktop/window process, so no live close interaction or fresh native screenshot was captured for this change. No full callback/frame or representative export-scale timing was measured.

**Commit:** `5852be2` (`fix(sheets): move exports off UI thread and drain before close`).

**Remaining gaps:** Native menu, dialog, and close interaction; live-window visual evidence; full input callback/frame timing; representative CSV/XLSX throughput; full replacement copy costs; recovery latency and storage bounds; native million-cell scrolling; app memory limit; complete visual/accessibility coverage; and broad spreadsheet interoperability remain open. PERF-01 and the Sheets acceptance gate remain `NEEDS_REVIEW` / `ACCEPTANCE_BLOCKED` respectively.

### REC-02 — Keep unsaved recovery safe and bounded

**P1 · Sheets persistence and storage · OPEN.** Each changed workbook can add another complete workbook package to the local recovery journal. The journal is compacted only by the explicit Save checkpoint, so an unsaved session has no automatic disk-space bound. The append path also rereads and validates the growing journal for every new record. On this personal computer, a long editing session must not be allowed to quietly consume all available storage. The owner approved the versioned format and numeric limits in [the REC-02 recovery design review](AGENTS.md#source-loom-sheets-docs-qa-reports-rec-02-recovery-design-review-md) on 2026-09-25. The first cell-edit journal milestone is implemented; the remaining storage, retention, admission, failure-path, and performance requirements are outstanding.

**Source evidence (2026-09-24):** `WorkbookWorker::run_worker` evaluates a changed workbook, serializes all sheets with `workbook_package_bytes`, then calls `SnapshotRecovery::record`. `SnapshotRecovery::record` deduplicates only byte-for-byte identical full packages. `SnapshotRecovery::checkpoint` writes a checkpoint and compacts, but the worker calls it only for an explicit save. `RecoveryJournal::append` reads and validates existing records before appending. The million-cell test measured 2.56 seconds for evaluation, 12.72 seconds to build a recovery package, and 43.05 seconds to append recovery; it did not measure retained journal size. A crash during that work can also leave the newest visible edit unrecoverable.

**Stage 1 result (2026-09-25) — IMPLEMENTED; REC-02 remains OPEN.** Commit `fde9cd1` adds a versioned cell-edit journal in a sibling `*.sheets-recovery-v1` directory. It stores complete native workbook packages as checkpoints and exact raw cell assignments, active-tab changes, and explicit deletions as ordered batches. Startup validates format version, session/workbook/baseline identities, sequence continuity, predecessor links, sheet indexes, and duplicate assignments before returning the replayed candidate. Full replacements and Save barriers publish complete packages. Legacy recovery is read-only during fallback; its checksums, sequence order, checkpoint forms, and torn-tail behavior are preserved. A Unix-only regression caught a stale Windows commit pointer overriding authoritative Unix `checkpoint.json`; pointer selection now follows the shared reader's platform behavior. Batches are appended before calculation of their own revision, but the single calculation worker means later accepted edits can wait behind an earlier slow calculation. Windows-target compilation was unavailable in this environment; only `x86_64-unknown-linux-gnu` was installed.

**Stage 1 verification:** `cargo test --workspace --locked --offline` passed 335 tests (211 app, 2 cache integration, 104 core, 13 formula, 1 performance fixture, and 4 range). Workspace all-target Clippy with `-D warnings`, the production app build, `cargo fmt --all -- --check`, governance audit, asset audit, and native `git diff --check` passed. The code-structure audit still reports 18 unrelated locked/out-of-scope source-size findings and lists no Sheets files. The stale-pointer regression failed before the platform-selection fix and passed after it; legacy preservation, exact replay, Open, Save Changes, and XLSX-cancel journeys pass in the workspace run. Evidence logs: `.work/sheets-acceptance-2026-09-25/rec02-stage1-workspace-tests.log`, `rec02-stage1-clippy.log`, `rec02-stage1-app-build.log`, `rec02-stage1-governance.log`, `rec02-stage1-assets.log`, and `rec02-stage1-code-structure.log`.

**Stage 2 result (2026-09-25) — IMPLEMENTED; REC-02 remains OPEN.** Commit `e5a8312` extracts checkpoint pointer/generation handling into `checkpoint_generations.rs` and its regressions into `checkpoint_generation_tests.rs`. New pointers record the exact predecessor generation. Recovery considers both `checkpoint.json` and `checkpoint-commit-N.json` pointer formats on every platform, rejects conflicting pointers for one generation, and fails closed if the highest committed generation or its predecessor is damaged. Startup validates the checkpoint and journal before cleanup; compaction replaces the journal before pruning. Cleanup retains the current generation and exact predecessor (or the highest lower complete generation for an older pointer), removes abandoned atomic-write temporary paths, and refuses to recurse into checkpoint-generation directory obstructions. Before each checkpoint allocation, cleanup removes an orphan from a prior failed attempt, so repeated blocked same-process retries do not accumulate complete packages. On Unix, a versioned commit record is used while `checkpoint.json` still contains direct legacy metadata, preserving the legacy `checkpoint.bin` and metadata byte-for-byte until the migration stage.

**Stage 2 verification:** `cargo test --manifest-path loom-core/Cargo.toml -p loom-production --locked` passed 32 tests; `cargo clippy --manifest-path loom-core/Cargo.toml -p loom-production --locked --all-targets -- -D warnings`, `cargo fmt --manifest-path loom-core/Cargo.toml --all -- --check`, `python3 loom-bootstrap/scripts/audit-governance.py`, `python3 loom-bootstrap/scripts/audit-assets.py`, and `git diff --check` passed. The independent source review found no remaining stage 2 issues. Evidence logs: `.work/sheets-acceptance-2026-09-25/rec02-stage2-tests.log`, `rec02-stage2-clippy.log`, `rec02-stage2-fmt.log`, `rec02-stage2-governance.log`, `rec02-stage2-assets.log`, `rec02-stage2-diff-check.log`, `rec02-stage2-red.log`, `rec02-stage2-retry-red.log`, and `rec02-stage2-review.log`. The code-structure audit remains red on 17 pre-existing locked/shared files and reports no touched recovery file or Sheets source finding; exact output is in `rec02-stage2-code-structure.log`. Only `x86_64-unknown-linux-gnu` is installed, so Windows-target compilation remains unverified.

**Stage 3 result (2026-09-25) — IMPLEMENTED AND INDEPENDENTLY REVIEWED; REC-02 remains OPEN.** Adds the shared `.checkpoint.lock` RAII API and a separate Sheets legacy-migration module. Migration records a prepared receipt and manifest, enforces the approved 256 MiB package, 640 MiB retained, and 1 GiB temporary-peak limits before publication, verifies the selected checkpoint package/hash/schema/sequence, then records verified state and removes only unchanged listed regular files while holding the legacy lock. Startup can retry verified partial cleanup; a complete marker rejects reappearing or unknown legacy entries. Unsafe receipt paths and mismatched manifests fail closed. Temporary startup imports do not write a receipt or checkpoint; accepted replacements migrate only after their replacement checkpoint is published. An unreceipted mix of versioned and known or unsupported legacy entries fails closed before the versioned journal can repair a torn tail. The migration fixture verifies formulas, two worksheets, active tab, and embedded image bytes. The old startup test now checks that legacy records are removed only after the complete v1 baseline is published and the marker is complete.

**Stage 3 verification:** Behavioral RED command `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app legacy_migration` ran 9 tests with 8 failures and 1 temporary-initialization preservation pass before implementation; exact output is in `.work/sheets-acceptance-2026-09-25/rec02-stage3-red.log`. Final focused command `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app cell_edit_recovery_tests:: --locked -- --nocapture --test-threads=1` passed 18 tests, including fail-closed recovery before torn-tail repair for known and unsupported legacy entries; output: `rec02-stage3-focused-recovery.log`. `cargo test --manifest-path loom-core/Cargo.toml -p loom-production --locked` passed 33 tests, including lock contention/release. Sheets workspace all-target Clippy and production all-target Clippy with `-D warnings`, `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check`, `python3 loom-bootstrap/scripts/audit-governance.py`, and `python3 loom-bootstrap/scripts/audit-assets.py` passed; logs are `rec02-stage3-sheets-clippy.log`, `rec02-stage3-production-clippy.log`, `rec02-stage3-governance.log`, and `rec02-stage3-assets.log`. The full serial app command `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app --locked --quiet -- --test-threads=1` passed 220 tests and failed the same seven accessibility-label/menu-visibility tests as clean stage-2 parent commit `6b8a2f7468a0123cee4f08366cb4bfdbfec6090a`, which passed 204 and failed those same seven; current output is `rec02-stage3-final-app-tests.log` and parent output is `rec02-stage3-parent-app-tests.log`. A direct failure-name comparison passed. Governance passes; the repo-wide code-structure audit remains red on 17 out-of-scope files and reports no finding in changed Sheets/production files; output: `rec02-stage3-structure.log`. The independent review confirmed the lost-receipt and torn-tail guard, including the final unknown-entry case; actual Windows interruption testing remains unverified, and only `x86_64-unknown-linux-gnu` is installed, so Windows-target compilation is also unverified.

**Remaining REC-02 gaps:** Stage 4C2 contains an unverified draft of ordinary append/checkpoint preflight; the latest focused Sheets command failed compilation, and the narrow error mapping added afterward has not been rebuilt. Therefore cap enforcement and mutation-free refusal are not yet proven. Checkpoint cadence exists only in test scaffolding; the approved 16 MiB / 2,000-record / 5-minute triggers are not wired into production. Recovery failure does not yet pause later edit admission or provide the approved Retry/Save As recovery actions. The worker applies edits to its candidate before journaling and continues into calculation/publication after recovery errors; fix ordering so failed durability cannot be reported as accepted. Add cap, disk-full, interruption-at-each-publication-step, two-instance, recovery restart, and worker-admission tests. Measure 250 ms p95 and worst-case durability, retained bytes, temporary peak, replay, and recent work lost after forced restart on the unchanged workload. The same-snapshot finalizer still trusts the path under the cooperating-lock contract; non-cooperating replacement and Windows/macOS behavior remain open. Keep REC-02 OPEN and Sheets `ACCEPTANCE_BLOCKED`.

**Next bounded milestone plan — finish Stage 4C2, then Stage 4C3:** (1) Rebuild the current draft, run the focused refusal tests, and inspect exact-byte preservation for package, retained-storage, and temporary-peak refusals before accepting the ordinary checkpoint/append preflight. (2) Ensure recovery durability gates evaluation and subsequent accepted edits; a failed append/checkpoint must latch the revision, stop admission, and surface Retry/Save As through the existing Sheets controller and UI. (3) Promote cadence accounting to production and trigger a complete checkpoint at 16 MiB, 2,000 records, or five minutes, and after explicit Save; retain the current and previous complete generations. Test each threshold and failure branch, then run focused suites, all-target Clippy, and source/governance audits. Keep disk-full/interruption matrix, two-instance/restart cases, and measured performance as separate later milestones.
**For a tiny coding model:** Follow the approved stages in order: (1) implement versioned cell-edit records and exact replay from a complete checkpoint; (2) make checkpoint publication crash-safe and retain only two complete generations; (3) migrate legacy package journals without modifying or deleting them until the new baseline and pointer are verified; (4) enforce the approved storage, journal, package, temporary-peak, and checkpoint limits with truthful Retry/Save As feedback and edit admission pause; (5) add interruption, corruption, two-instance, disk-full, and cap tests, then measure the unchanged million-cell workload. Make each accepted edit durable before slow formula evaluation. Preserve formulas, explicit blank-cell deletions, active tab, added/deleted tabs, and embedded image bytes. If an append or replay fails, preserve the last valid recovery state and do not claim the edit is protected. Record retained bytes, temporary peak, p95 and worst-case durability latency, replay time, and recent work lost after forced restart.

**Done when:** A long unsaved session stays within the owner-approved 640 MiB retained and 1 GiB temporary-peak bounds; the 256 MiB package, 64 MiB / 10,000-record journal, 1 MiB record, 16 MiB / 2,000-record / 5-minute checkpoint, and two-generation rules are enforced; the newest accepted edit meets the measured 250 ms p95 durability target; restart reconstructs the exact workbook; and failure tests prove the prior valid checkpoint survives every interrupted compaction step. These values are approved policy limits and targets, not achieved measurements. REC-02 remains OPEN until the implementation, failure-path evidence, and performance measurements pass.
<!-- CURRENT TRUTH END -->


# Consolidated Markdown reference

The sections below preserve the non-README Markdown files that used to live in separate paths. The original path is shown under each section heading. Current instructions and active project state are the constitution and marked truth section above. Dated reports, plans, and prior snapshots remain historical evidence; they do not override the active gate. Requirements and specifications remain useful within their stated scope when they do not conflict with the current constitution.


## Index

- [`.work/audit-2026-09-14/AUDIT.md`](#source-work-audit-2026-09-14-audit-md)
- [`.work/sheets-100-plan.md`](#source-work-sheets-100-plan-md)
- [`.work/sheets-acceptance-2026-09-22/REPORT.md`](#source-work-sheets-acceptance-2026-09-22-report-md)
- [`.work/sheets-acceptance-2026-09-23/REPORT.md`](#source-work-sheets-acceptance-2026-09-23-report-md)
- [`.work/sheets-acceptance-2026-09-24/REPORT.md`](#source-work-sheets-acceptance-2026-09-24-report-md)
- [`.work/sheets-acceptance-2026-09-24/SCROLL-ACCEPTANCE-PLAN.md`](#source-work-sheets-acceptance-2026-09-24-scroll-acceptance-plan-md)
- [`.work/sheets-acceptance-2026-09-24/async-cell-commit-plan.md`](#source-work-sheets-acceptance-2026-09-24-async-cell-commit-plan-md)
- [`.work/sheets-acceptance-2026-09-24/interop/styles-relationship-diagnostic.md`](#source-work-sheets-acceptance-2026-09-24-interop-styles-relationship-diagnostic-md)
- [`.work/uiux-audit-2026-09-14/AUDIT.md`](#source-work-uiux-audit-2026-09-14-audit-md)
- [`.work/uiux-audit-2026-09-14/code-repair-cards.md`](#source-work-uiux-audit-2026-09-14-code-repair-cards-md)
- [`.work/uiux-audit-2026-09-14/prior-TRUTH.md`](#source-work-uiux-audit-2026-09-14-prior-truth-md)
- [`.work/uiux-audit-2026-09-14/ui-repair-cards.md`](#source-work-uiux-audit-2026-09-14-ui-repair-cards-md)
- [`loom-bootstrap/BOOTSTRAP.md`](#source-loom-bootstrap-bootstrap-md)
- [`loom-bootstrap/CHANGELOG.md`](#source-loom-bootstrap-changelog-md)
- [`loom-bootstrap/DEPENDENCIES.md`](#source-loom-bootstrap-dependencies-md)
- [`loom-bootstrap/LICENSE_POLICY.md`](#source-loom-bootstrap-license-policy-md)
- [`loom-bootstrap/SECURITY.md`](#source-loom-bootstrap-security-md)
- [`loom-bootstrap/docs/adrs/ADR-0001-orchestration-layout.md`](#source-loom-bootstrap-docs-adrs-adr-0001-orchestration-layout-md)
- [`loom-design-bible/ACCESSIBILITY.md`](#source-loom-design-bible-accessibility-md)
- [`loom-design-bible/ANTI_PATTERNS.md`](#source-loom-design-bible-anti-patterns-md)
- [`loom-design-bible/CANVAS.md`](#source-loom-design-bible-canvas-md)
- [`loom-design-bible/COLOR.md`](#source-loom-design-bible-color-md)
- [`loom-design-bible/COMMAND_PALETTE.md`](#source-loom-design-bible-command-palette-md)
- [`loom-design-bible/COMPONENTS.md`](#source-loom-design-bible-components-md)
- [`loom-design-bible/DESIGN_BIBLE.md`](#source-loom-design-bible-design-bible-md)
- [`loom-design-bible/DESIGN_PRINCIPLES.md`](#source-loom-design-bible-design-principles-md)
- [`loom-design-bible/DESIGN_REVIEW.md`](#source-loom-design-bible-design-review-md)
- [`loom-design-bible/DESIGN_TOKENS.md`](#source-loom-design-bible-design-tokens-md)
- [`loom-design-bible/DIALOGS.md`](#source-loom-design-bible-dialogs-md)
- [`loom-design-bible/DOCUMENT_EDITOR.md`](#source-loom-design-bible-document-editor-md)
- [`loom-design-bible/DRAG_AND_DROP.md`](#source-loom-design-bible-drag-and-drop-md)
- [`loom-design-bible/ICONOGRAPHY.md`](#source-loom-design-bible-iconography-md)
- [`loom-design-bible/INSPECTORS.md`](#source-loom-design-bible-inspectors-md)
- [`loom-design-bible/KEYBOARD.md`](#source-loom-design-bible-keyboard-md)
- [`loom-design-bible/LAYOUT.md`](#source-loom-design-bible-layout-md)
- [`loom-design-bible/MECHANICAL_DESIGN_STANDARD.md`](#source-loom-design-bible-mechanical-design-standard-md)
- [`loom-design-bible/MENUS.md`](#source-loom-design-bible-menus-md)
- [`loom-design-bible/MOTION.md`](#source-loom-design-bible-motion-md)
- [`loom-design-bible/NOTIFICATIONS.md`](#source-loom-design-bible-notifications-md)
- [`loom-design-bible/PERFORMANCE.md`](#source-loom-design-bible-performance-md)
- [`loom-design-bible/POINTER_AND_PEN.md`](#source-loom-design-bible-pointer-and-pen-md)
- [`loom-design-bible/SELECTION.md`](#source-loom-design-bible-selection-md)
- [`loom-design-bible/SIDEBARS.md`](#source-loom-design-bible-sidebars-md)
- [`loom-design-bible/SPACING.md`](#source-loom-design-bible-spacing-md)
- [`loom-design-bible/SPREADSHEET.md`](#source-loom-design-bible-spreadsheet-md)
- [`loom-design-bible/THEMING.md`](#source-loom-design-bible-theming-md)
- [`loom-design-bible/TIMELINE.md`](#source-loom-design-bible-timeline-md)
- [`loom-design-bible/TOOLBARS.md`](#source-loom-design-bible-toolbars-md)
- [`loom-design-bible/TYPOGRAPHY.md`](#source-loom-design-bible-typography-md)
- [`loom-design-bible/UX_ACCEPTANCE_CHECKLIST.md`](#source-loom-design-bible-ux-acceptance-checklist-md)
- [`loom-design-bible/VISUAL_QA.md`](#source-loom-design-bible-visual-qa-md)
- [`loom-design-bible/WINDOWS.md`](#source-loom-design-bible-windows-md)
- [`loom-design-bible/docs/adrs/ADR-0001-design-tokens.md`](#source-loom-design-bible-docs-adrs-adr-0001-design-tokens-md)
- [`loom-plugin-sdk/ACCESSIBILITY.md`](#source-loom-plugin-sdk-accessibility-md)
- [`loom-plugin-sdk/ARCHITECTURE.md`](#source-loom-plugin-sdk-architecture-md)
- [`loom-plugin-sdk/BUILDING.md`](#source-loom-plugin-sdk-building-md)
- [`loom-plugin-sdk/CHANGELOG.md`](#source-loom-plugin-sdk-changelog-md)
- [`loom-plugin-sdk/CONTRIBUTING.md`](#source-loom-plugin-sdk-contributing-md)
- [`loom-plugin-sdk/DEPENDENCIES.md`](#source-loom-plugin-sdk-dependencies-md)
- [`loom-plugin-sdk/IMPLEMENTATION_GUIDE.md`](#source-loom-plugin-sdk-implementation-guide-md)
- [`loom-plugin-sdk/LICENSE_POLICY.md`](#source-loom-plugin-sdk-license-policy-md)
- [`loom-plugin-sdk/PERFORMANCE.md`](#source-loom-plugin-sdk-performance-md)
- [`loom-plugin-sdk/ROADMAP.md`](#source-loom-plugin-sdk-roadmap-md)
- [`loom-plugin-sdk/SECURITY.md`](#source-loom-plugin-sdk-security-md)
- [`loom-plugin-sdk/TASKS.md`](#source-loom-plugin-sdk-tasks-md)
- [`loom-plugin-sdk/TESTING.md`](#source-loom-plugin-sdk-testing-md)
- [`loom-plugin-sdk/VISUAL_QA.md`](#source-loom-plugin-sdk-visual-qa-md)
- [`loom-plugin-sdk/docs/rfcs/RFC-0009-plugin-abi-and-sandboxing.md`](#source-loom-plugin-sdk-docs-rfcs-rfc-0009-plugin-abi-and-sandboxing-md)
- [`loom-samples/conformance/markdown/notes.md`](#source-loom-samples-conformance-markdown-notes-md)
- [`loom-sheets/PERFORMANCE.md`](#source-loom-sheets-performance-md)
- [`loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md`](#source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md)
- [`loom-sheets/docs/qa-reports/REC-02-recovery-design-review.md`](#source-loom-sheets-docs-qa-reports-rec-02-recovery-design-review-md)
- [`loom-spec/ARCHITECTURE.md`](#source-loom-spec-architecture-md)
- [`loom-spec/COMPATIBILITY_POLICY.md`](#source-loom-spec-compatibility-policy-md)
- [`loom-spec/CROSS_APP_WORKFLOWS.md`](#source-loom-spec-cross-app-workflows-md)
- [`loom-spec/FEATURE_MATRICES.md`](#source-loom-spec-feature-matrices-md)
- [`loom-spec/FILE_FORMAT_FAMILY.md`](#source-loom-spec-file-format-family-md)
- [`loom-spec/IMPLEMENTATION_GUIDE.md`](#source-loom-spec-implementation-guide-md)
- [`loom-spec/PRODUCT_SPEC.md`](#source-loom-spec-product-spec-md)
- [`loom-spec/RELEASE_CRITERIA.md`](#source-loom-spec-release-criteria-md)
- [`loom-spec/ROADMAP.md`](#source-loom-spec-roadmap-md)
- [`loom-spec/TERMINOLOGY.md`](#source-loom-spec-terminology-md)
- [`loom-spec/docs/adrs/ADR-0001-Slint-Licensing-and-Distribution.md`](#source-loom-spec-docs-adrs-adr-0001-slint-licensing-and-distribution-md)
- [`loom-spec/docs/adrs/ADR-0002-Path-Based-Crate-Pinning.md`](#source-loom-spec-docs-adrs-adr-0002-path-based-crate-pinning-md)
- [`loom-spec/docs/adrs/ADR-0003-Headless-Screenshots.md`](#source-loom-spec-docs-adrs-adr-0003-headless-screenshots-md)
- [`loom-spec/docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`](#source-loom-spec-docs-adrs-adr-0004-deterministic-mutation-fuzzing-md)
- [`loom-spec/docs/adrs/ADR-0005-Internal-PDF-Writer.md`](#source-loom-spec-docs-adrs-adr-0005-internal-pdf-writer-md)
- [`loom-spec/docs/adrs/ADR-0006-Image-Codec-Backend.md`](#source-loom-spec-docs-adrs-adr-0006-image-codec-backend-md)
- [`loom-spec/docs/adrs/ADR-0007-No-FFmpeg-in-Initial-Milestone.md`](#source-loom-spec-docs-adrs-adr-0007-no-ffmpeg-in-initial-milestone-md)
- [`loom-spec/docs/rfcs/RFC-0001-Repository-and-Versioning-Strategy.md`](#source-loom-spec-docs-rfcs-rfc-0001-repository-and-versioning-strategy-md)
- [`loom-spec/docs/rfcs/RFC-0002-UI-and-Engine-Separation.md`](#source-loom-spec-docs-rfcs-rfc-0002-ui-and-engine-separation-md)
- [`loom-spec/docs/rfcs/RFC-0003-Slint-Integration-Model.md`](#source-loom-spec-docs-rfcs-rfc-0003-slint-integration-model-md)
- [`loom-spec/docs/rfcs/RFC-0005-Text-Shaping-and-Layout.md`](#source-loom-spec-docs-rfcs-rfc-0005-text-shaping-and-layout-md)
- [`loom-spec/docs/rfcs/RFC-0006-File-Package-Format.md`](#source-loom-spec-docs-rfcs-rfc-0006-file-package-format-md)
- [`loom-spec/docs/rfcs/RFC-0007-Undo-and-Transaction-System.md`](#source-loom-spec-docs-rfcs-rfc-0007-undo-and-transaction-system-md)
- [`loom-spec/docs/rfcs/RFC-0008-Async-Job-Framework.md`](#source-loom-spec-docs-rfcs-rfc-0008-async-job-framework-md)
- [`loom-spec/docs/rfcs/RFC-0010-Vision-Provider-Model.md`](#source-loom-spec-docs-rfcs-rfc-0010-vision-provider-model-md)
- [`loom-spec/docs/rfcs/RFC-0011-Model-Pack-Format.md`](#source-loom-spec-docs-rfcs-rfc-0011-model-pack-format-md)
- [`loom-spec/docs/rfcs/RFC-0013-Color-Management.md`](#source-loom-spec-docs-rfcs-rfc-0013-color-management-md)
- [`loom-spec/docs/rfcs/RFC-0015-Visual-Regression-System.md`](#source-loom-spec-docs-rfcs-rfc-0015-visual-regression-system-md)
- [`loom-spec/docs/rfcs/RFC-0018-Autosave-and-Recovery.md`](#source-loom-spec-docs-rfcs-rfc-0018-autosave-and-recovery-md)
- [`loom-vision/ACCESSIBILITY.md`](#source-loom-vision-accessibility-md)
- [`loom-vision/ARCHITECTURE.md`](#source-loom-vision-architecture-md)
- [`loom-vision/BUILDING.md`](#source-loom-vision-building-md)
- [`loom-vision/CHANGELOG.md`](#source-loom-vision-changelog-md)
- [`loom-vision/CONTRIBUTING.md`](#source-loom-vision-contributing-md)
- [`loom-vision/DEPENDENCIES.md`](#source-loom-vision-dependencies-md)
- [`loom-vision/IMPLEMENTATION_GUIDE.md`](#source-loom-vision-implementation-guide-md)
- [`loom-vision/LICENSE_POLICY.md`](#source-loom-vision-license-policy-md)
- [`loom-vision/PERFORMANCE.md`](#source-loom-vision-performance-md)
- [`loom-vision/ROADMAP.md`](#source-loom-vision-roadmap-md)
- [`loom-vision/SECURITY.md`](#source-loom-vision-security-md)
- [`loom-vision/TASKS.md`](#source-loom-vision-tasks-md)
- [`loom-vision/TESTING.md`](#source-loom-vision-testing-md)
- [`loom-vision/VISUAL_QA.md`](#source-loom-vision-visual-qa-md)
- [`loom-vision/docs/adrs/ADR-0001-qr-reference-provider.md`](#source-loom-vision-docs-adrs-adr-0001-qr-reference-provider-md)




## source-work-audit-2026-09-14-audit-md

Original path: `.work/audit-2026-09-14/AUDIT.md`

**Loom audit — 14 September 2026**

The current tree contains reproducible data-loss, export, recovery, and plugin-boundary defects. The passing repository gates do not substantiate the current acceptance claims. Fix recovery and native persistence first, then destructive document transitions and interoperability.

Reviewed commit `8fce782` plus the existing uncommitted Sheets implementation. This audit includes existing defects as well as defects in that implementation. Product source and the existing working changes were not modified; audit evidence lives in this ignored `.work/` directory.

**Verified baseline**

| Check | Result |
|---|---|
| Governance, code structure, assets, UI foundation source audits | All four PASS |
| Shared core: 13 selected crates | 123 tests PASS |
| Sheets core and integration tests | 98 tests PASS |
| Writer core | 77 tests PASS |
| Present core | 49 tests PASS |
| Photo core | 49 tests PASS |
| Additional shared recovery regression tests | 3 FAIL, reproducing defects below |
| Focused application and plugin probes | Reproductions below |

That is 396 passing existing tests, alongside the newly demonstrated failures. Commands, probe source, generated files, and logs are retained here. Tests ran on Linux with Rust 1.98.1. The test results are not a claim that all workspaces, native windows, platforms, or acceptance journeys pass.

**Findings, in priority order**

1. **[P1] Recovery loses edits made after a saved document is reopened.** [RecoveryJournal::open](loom-core/crates/loom-production/src/lib.rs#L148) derives the next sequence only from the remaining journal. Saving compacts that journal to empty while retaining the checkpoint's sequence. After restart, the next edit receives sequence 1 again, and recovery filters it out as older than the checkpoint. Reproduction: record → checkpoint → reopen → record new edit → reopen; the returned payload is `saved contents`, not `new unsaved edit`. Initialize the sequence from both the checkpoint and journal. Evidence: [failing regression](.work/audit-2026-09-14/shared-probes/src/lib.rs#L14), [log](.work/audit-2026-09-14/shared-probes.log).

2. **[P1] A partially written checkpoint makes valid recovery data inaccessible.** [checkpoint](loom-core/crates/loom-production/src/lib.rs#L211) replaces `checkpoint.bin` and `checkpoint.json` separately. If metadata creation fails, the new payload remains paired with the old checksum; startup returns `checkpoint digest mismatch` before examining the valid journal. The probe injects a real filesystem failure at metadata-temp creation. The underlying [replacement helper](loom-core/crates/loom-production/src/lib.rs#L366) also deletes the old file before renaming the new one. Publish a complete checkpoint generation atomically and preserve a recoverable generation on failure. Evidence: [failing regression](.work/audit-2026-09-14/shared-probes/src/lib.rs#L32).

3. **[P1] Sheets changes cell text and sheet names during native save/reopen.** [sheet_from_json](loom-sheets/crates/loom-sheets-core/src/persistence.rs#L223) searches for delimiters inside JSON strings and applies chained substitutions instead of decoding JSON. Tabs and carriage returns become literal escapes; `C:\new\notes` gains newline characters; `before"}after` truncates to `before\`; a quoted sheet name truncates at the escaped quote. This affects the native workbook and recovery representations. Use a proper JSON parser for the complete structure. Evidence: [probe](.work/audit-2026-09-14/sheets/repro.rs), [output](.work/audit-2026-09-14/sheets-repro.log).

4. **[P1] New/Open replace unsaved documents without a save/discard decision.** [Sheets New](loom-sheets/crates/loom-sheets-app/src/main.rs#L2352) overwrites all tabs and clears history immediately; [Open](loom-sheets/crates/loom-sheets-app/src/main.rs#L2459) also replaces the workbook. `apply_sheet` then records the replacement to recovery. The same direct New replacement exists in [Present](loom-present/crates/loom-present-app/src/main.rs#L1512) and [Photo](loom-photo/crates/loom-photo-app/src/main.rs#L1992); [Writer Open](loom-writer/crates/loom-writer-app/src/main.rs#L2272) similarly replaces the current document. Add a dirty-document transition decision before replacement. Verified by tracing callbacks and state/history replacement; this was not a live window click test. Writer's initial New action opens a template chooser, so it is not included in the immediate-New claim.

5. **[P1] Writer PDF export silently omits later content.** [export_pdf](loom-writer/crates/loom-writer-core/src/export.rs#L125) always creates one PDF page and stops when the bottom margin is reached. A 60-paragraph document that Writer paginates into three pages exports only 31 paragraphs; paragraph 60 is absent, and `pdfinfo` confirms one page. Long ordinary paragraphs are also passed as a single line. Drive export from the document's pagination/wrapping model. Evidence: [probe log](.work/audit-2026-09-14/documents-repro.log), [generated PDF](.work/audit-2026-09-14/documents/writer-60-paragraphs.pdf).

6. **[P1] Sheets XLSX files containing drawings contain malformed XML.** [drawing insertion](loom-sheets/crates/loom-sheets-core/src/xlsx.rs#L620) writes `<drawing r:id="rId1"/>` into a worksheet without declaring `xmlns:r`. Python's independent XML parser rejects the exported worksheet with `unbound prefix`. There is a second XML defect in [chart ranges](loom-sheets/crates/loom-sheets-core/src/xlsx.rs#L866): a sheet named `R&D` injects an unescaped `&` into chart formula text. Add the namespace and XML-escape formula ranges; validate exported parts with an independent XML parser. Evidence: [XLSX](.work/audit-2026-09-14/sheets/chart.xlsx), [validation](.work/audit-2026-09-14/sheets-xml.log).

7. **[P1] Sheets workbook history grows exponentially despite its 200-entry limit.** [commit_workbook_transaction](loom-sheets/crates/loom-sheets-app/src/main.rs#L1693) stashes live history, then copies it into both workbook snapshots. Those snapshots include earlier workbook transactions recursively. Nine renames create 9,841 nested transactions while the top-level stack has only nine entries; the observed counts are 1, 4, 13, 40, … . The cap never bounds this nested allocation, which also clones workbook content. Store nonrecursive workbook history or references to immutable snapshots. Evidence: [source-extraction probe](.work/audit-2026-09-14/sheets/history.rs), [counts](.work/audit-2026-09-14/sheets-history.log). The three extracted production functions were compared byte-for-byte with the current source.

8. **[P1] Plugin write authorization can escape its allowed directory through a symlink.** [canonicalize_or_normalize](loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs#L527) falls back to lexical normalization when the requested file does not yet exist. With `allowed/linked` pointing outside the allowed root, authorization for `allowed/linked/new.txt` returns `Ok(())`, and writing that path creates the file outside the boundary. Resolve existing ancestors for create targets and enforce the boundary during the actual operation. Evidence: [permission probe](.work/audit-2026-09-14/media-plugins/src/main.rs), [result](.work/audit-2026-09-14/plugin-permission.log). This proves the host permission API defect; it does not claim a completed exploit through a running Wasmtime guest.

9. **[P1] Encode's final commit overwrites a newly created destination despite no-overwrite policy.** [commit_encode_output](loom-encode/crates/loom-encode-core/src/lib.rs#L2163) renames the temporary output over the destination even when `overwrite` is false. A deterministic encoder adapter creates another file at the final destination during encoding: execution returns `Ok(())` and replaces `IMPORTANT_OTHER_FILE` with `NEWENCODE`. The encoder's `-n` flag only protects the temporary path. Enforce no-replace semantics at final commit. Evidence: [probe](.work/audit-2026-09-14/media-plugins/src/bin/encode_collision.rs), [output](.work/audit-2026-09-14/encode-overwrite.log). Verified on Linux; the adapter tests orchestration, not codec output.

10. **[P1] Studio native persistence irreversibly changes audio on every save/reopen.** [save_studio_bundle](loom-studio/crates/loom-studio-core/src/lib.rs#L1383) encodes every floating-point asset as PCM16. A sample of `0.000001` becomes zero after one native round trip. The encoder scales by 32767 while the decoder divides by 32768, so `0.75` becomes `0.7499695`, then `0.74993896`, then `0.74990845` across three rounds. Preserve source/sample precision in native storage and reserve lossy conversion for explicit export. Evidence: [probe](.work/audit-2026-09-14/media-plugins/src/bin/studio_roundtrip.rs), [output](.work/audit-2026-09-14/studio-roundtrip.log).

11. **[P1] Sheets recovery drops package-owned image payloads.** [record_workbook_snapshot](loom-sheets/crates/loom-sheets-app/src/main.rs#L1148) records only workbook JSON. That JSON retains an asset name but not its bytes; [object parsing](loom-sheets/crates/loom-sheets-core/src/persistence.rs#L609) restores every object with `embedded: None`. After recovering a portable workbook whose original image source is unavailable, the image cannot be reconstructed; the probe's XLSX export fails trying to read the missing original path. Snapshot the package-owned assets with the workbook. Evidence: [Sheets output](.work/audit-2026-09-14/sheets-repro.log). The probe uses a small arbitrary byte payload to isolate transport loss; it does not test image decoding.

12. **[P1] Sheets imports shared XLSX formulas as frozen numbers.** [formula extraction](loom-sheets/crates/loom-sheets-core/src/lib.rs#L4311) only retains explicit formula text, falling back to the cached value for `<f t="shared" si="0"/>`. In the fixture, B1 defines `A1*2`, B2 shares it, and B2 imports as raw `30`. Changing A2 to 50 leaves B2 at 30 instead of 100. Shared formula members derive their expression from the master and relative position, as documented by [Microsoft's CellFormula reference](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cellformula?view=openxml-3.0.1). Expand these formulas correctly or report unsupported import explicitly. Evidence: [Sheets probe/output](.work/audit-2026-09-14/sheets-repro.log).

13. **[P1] Launching Writer with `--open` disables recovery for that session.** [startup](loom-writer/crates/loom-writer-app/src/main.rs#L3034) calls `take_snapshot_recovery()` only when no explicit file was supplied. That call is also the sole initializer of the recovery slot. The shared macro makes record/checkpoint calls silently succeed when the slot is `None`, so subsequent edits in a `--open` session are never journaled. Initialize the store unconditionally; separately decide whether to restore its previous payload. Verified by tracing startup and the [macro's no-op branch](loom-core/crates/loom-production/src/snapshot.rs#L181); no forced crash of a live Writer window was performed.

14. **[P2] Photo add/delete/add produces duplicate layer IDs and breaks saving.** [pixel insertion](loom-photo/crates/loom-photo-app/src/main.rs#L1080) and adjustment insertion derive IDs from the current layer count. Add two pixel layers, remove the earlier one, then add another: IDs become `layer-bg, layer-3, layer-3`; save returns `duplicate layer id 'layer-3'`. Assigning the transparent image under that ID can also replace the surviving layer's payload. Use a collision-free ID allocator. Evidence: [document probe output](.work/audit-2026-09-14/documents-repro.log); the probe executes the core operations used by the callback.

15. **[P2] Plugin input delivery bypasses the invocation deadline.** [invoke](loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs#L870) synchronously writes all stdin before starting output readers or checking elapsed time. A nonreading child can fill the pipe and block the host before timeout enforcement. A valid minimal module and a controlled nonreading runtime adapter, given 1 MiB input and a 50 ms limit, returned only after roughly 2,014 ms with `Broken pipe`. Run input/output and deadline supervision concurrently, and terminate/reap on I/O failures. Evidence: [probe](.work/audit-2026-09-14/media-plugins/src/main.rs), [output](.work/audit-2026-09-14/media-timeout.log). No real Wasmtime executable was needed or exercised.

16. **[P2] Present reuses slide IDs after deletion.** [add_slide](loom-present/crates/loom-present-core/src/lib.rs#L1136) uses `slides.len() + 1`. The GUI calls it directly. Add two slides, delete the earlier one, then add: the deck contains `slide-1, slide-3, slide-3`. ID-keyed transitions and navigation can no longer distinguish those slides, and validation reports duplicate identity. Reuse the existing unique-slide-ID allocation strategy already used by duplication. Evidence: [document probe output](.work/audit-2026-09-14/documents-repro.log).

17. **[P2] Writer comment anchors point to the wrong text after edits.** [replace_paragraphs](loom-writer/crates/loom-writer-core/src/lib.rs#L424) remaps character styles but not comment offsets. Anchor a comment to `world` in `Hello world`, then insert `New ` at the start: the comment still spans 6..11 and now selects `llo w`. Rebase or invalidate anchors using the text edit, including paragraph split/merge and UTF-8 boundaries. Evidence: [document probe output](.work/audit-2026-09-14/documents-repro.log).

18. **[P2] A failed storage-journal append consumes its sequence and poisons a successful retry.** [append](loom-core/crates/loom-storage/src/lib.rs#L239) increments `next_seq` before opening or writing the journal. The probe fails the first open, removes that obstruction, then successfully appends; reopening rejects sequence 2 where it expects 1. Commit the sequence only after the durable append and handle partial writes explicitly. Evidence: [failing regression](.work/audit-2026-09-14/shared-probes/src/lib.rs#L50). This is the storage primitive, distinct from the production journal used by desktop snapshot recovery.

19. **[P2] Present transition changes do not participate in undo.** [GUI callback](loom-present/crates/loom-present-app/src/main.rs#L2274) directly calls `set_transition`, which modifies a separate map without checkpointing it. Session history contains only the document, so adding a checkpoint alone would still omit transitions. In the probe, setting Dissolve leaves `undo()` returning false and the transition unchanged. Include transitions in the undoable session state. Evidence: [document probe output](.work/audit-2026-09-14/documents-repro.log).

**Acceptance and CI observations**

- [Routine CI](.github/workflows/ci.yml#L54) only runs Clippy/tests for `loom-ui` (plus formatting for the core workspace and source audits). It does not compile or test active Writer or the extended Sheets work. The full native workflow is manual. Add focused active-application checks and behavioral regression coverage for the failures above.
- [TRUTH's serial gate](AGENTS.md#current-truth) lists Present and Photo as LOCKED, but their later sections say ACCEPTED. Its Sheets section says there are no known severity-1/2 defects, which these findings contradict. The governance script still passes because it checks selected phrases and contract fields rather than consistency of the application sections. Reconcile the ledger from verified evidence before using it to unlock further application work. I left the user's already-modified ledger intact.

**Coverage and limits**

The deepest review covered Sheets working changes; shared storage, package and recovery services; Writer/Present/Photo persistence and export; plugin authorization/invocation; Encode output commit; and Studio audio persistence. Motion, Video, Vision, desktop services, and the shared runtime received source spot checks. Their complete feature sets were not exhaustively tested. This audit does not certify visual quality, accessibility, cross-platform behavior, dependency vulnerability status, real codecs, or real Wasmtime execution. Known broad product limitations were not counted as newly discovered defects.

The extracted Sheets history harness preserves production function bodies exactly but replaces the GUI state container with a minimal equivalent. Other probes call the production core APIs. The three failing regression tests intentionally express the desired recovery behavior; their failure is evidence of the defects, not an audit infrastructure failure.

**Reproducing the evidence**

From the repository root, put the installed Rust toolchain on `PATH` (in this environment, `/home/patri/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin`). Use the supplied lockfiles and an external `CARGO_TARGET_DIR` if desired. Dependency downloads may be needed when changing Cargo caches.

```bash
cargo test --manifest-path .work/audit-2026-09-14/shared-probes/Cargo.toml --locked
cargo run --manifest-path .work/audit-2026-09-14/sheets/Cargo.toml --locked --bin sheets-repro
cargo run --manifest-path .work/audit-2026-09-14/sheets/Cargo.toml --locked --bin history-repro
python3 .work/audit-2026-09-14/sheets/validate_xml.py
cargo run --manifest-path .work/audit-2026-09-14/documents/Cargo.toml --locked
cargo run --manifest-path .work/audit-2026-09-14/media-plugins/Cargo.toml --locked --bin loom-audit-media
cargo run --manifest-path .work/audit-2026-09-14/media-plugins/Cargo.toml --locked --bin loom-audit-media -- stdin
cargo run --manifest-path .work/audit-2026-09-14/media-plugins/Cargo.toml --locked --bin encode_collision
cargo run --manifest-path .work/audit-2026-09-14/media-plugins/Cargo.toml --locked --bin studio_roundtrip
```

The shared regression command is expected to exit 101 with three failures. Application probes print their observed results; XML validation prints the parser errors. The media probes use dedicated `/tmp/loom-audit-media/` and `/tmp/loom-audit-media/evidence-encode/` fixtures and controlled process adapters. They do not access real user documents or media.


## source-work-sheets-100-plan-md

Original path: `.work/sheets-100-plan.md`

# Sheets 100% extension plan

Goal: remove the remaining Sheets limitations recorded in `TRUTH.md` while
preserving legacy `.loomtable` files and the existing accepted workflow.

Scope:

1. Add package-owned image assets to `.loomtable`; load them independently of
   the original local path and render them in the native UI.
2. Add object selection, anchor dragging, resize handles, and undoable
   object geometry changes through the existing Slint callback route.
3. Make CSV formula/value export explicit and extend XLSX import/export for
   styles and supported chart/image metadata, with safe preservation of
   unknown external parts where the model cannot edit them.
4. Add focused core/app tests, native Linux captures for object states, and
   update README/TRUTH only from passing evidence.

Verification per slice: write a failing regression test, run it, implement the
smallest behavior, rerun focused tests, then run the full workspace tests,
Clippy, format, structure/governance/asset/UI audits, and native screenshots.


## source-work-sheets-acceptance-2026-09-22-report-md

Original path: `.work/sheets-acceptance-2026-09-22/REPORT.md`

# Sheets acceptance evidence — 2026-09-22

This report records the repaired Sheets template chooser, the current viewport/theme captures, and the existing data-integrity evidence. It does not claim acceptance for checks that require a physical desktop pointer or a screen reader.

## Repaired chooser behavior

The template chooser now:

- wraps template cards into a vertically scrollable content region;
- keeps Recents tied to templates created in the current session and says `No recent templates` when empty;
- exposes all categories in All Templates, including Personal, Business, and Education;
- lets Left/Right select the visible category's cards with wrap-around;
- moves the scroll target with the selected card, including at 1.25× and 1.5× text scale;
- uses Return/Create for the selected card and Escape/Cancel without replacing the current workbook.

The focused Slint interaction test renders selected Invoice & Expenses cards at 1.0×, 1.25×, and 1.5×. At the two required accessibility sizes, the complete preview, title, section heading, and footer actions are visible in the 1024×720 capture.

Evidence:

- `screenshots/1024x720-template-chooser-1.25.png`
- `screenshots/1024x720-template-chooser-1.5.png`
- `screenshots/1024x720-template-chooser-text-200-current.png` (exploratory 2.0× capture)
- `/tmp/loom-sheets-template-chooser-keyboard-selection-1.0.png`
- `/tmp/loom-sheets-template-chooser-keyboard-selection-1.25.png`
- `/tmp/loom-sheets-template-chooser-keyboard-selection-1.5.png`

## Viewport and theme evidence

The fresh debug binary was built from the current working tree after the chooser repair and split. Its SHA-256 is `383ff06676dfda18ad835d81cd0f0cf205ebd61a6324891b4965808e17fd4d5c`.

Fresh headless captures were generated at 1024×720, 1280×800, 1440×900, and 1920×1200 in light, dark, and high-contrast themes. All 12 commands exited 0 and produced PNGs with the requested dimensions and plausible file sizes. The contact sheet is `screenshots/sheets-viewport-theme-contact.png`.

The captures show the blank workbook, readable chrome, a closed inspector by default, the grid, formula bar, tabs, and status bar. The dark and high-contrast variants remain visually distinct and keep the active-cell outline visible.

## Tests and checks

- `PKG_CONFIG_PATH=/tmp/loom-writer-fontconfig-deps/pkgconfig-dynamic cargo test --locked --offline -p loom-sheets-app`: **109 passed, 0 failed**.
- `cargo test --locked --offline -p loom-sheets-core`: **87 core tests, 13 formula tests, 1 performance test, and 4 range tests passed**.
- `cargo fmt --manifest-path loom-sheets/Cargo.toml --all`: passed.
- `git diff --check`: passed.
- `python3 loom-bootstrap/scripts/audit-governance.py`: passed.
- `python3 loom-bootstrap/scripts/audit-ui-foundation.py`: passed.
- The active Sheets source-size ceiling passes after moving chooser navigation to `template_navigation.rs`; the structure audit still reports pre-existing over-budget locked apps (Encode, Motion, Video, Writer core, and Photo).

The app tests cover template creation, real Recents, Escape cancellation, keyboard focus paths, workbook Save/Open and XLSX round trips, chart ranges, undo/redo, formula editing, and inspector state. The compact inspector regression is recorded in `inspector-keyboard-tests.log`; it covers Escape dismissal, focus restoration to Format, Tab/Enter Close, and selection preservation. The chooser keyboard path is verified through Slint window key events and rendered output. A physical desktop keyboard/pointer run and screen-reader output remain unverified in this environment, so the overall Sheets application stays acceptance-blocked until those checks are performed.


## source-work-sheets-acceptance-2026-09-23-report-md

Original path: `.work/sheets-acceptance-2026-09-23/REPORT.md`

# Native Sheets self-audit — 2026-09-23

This report records visual captures and keyboard/accessibility checks from the live Linux Sheets window. The screenshots were taken with `/usr/bin/gnome-screenshot`, not rendered by the headless preview path.

## Capture setup

- Binary: `loom-sheets/target/debug/loom-sheets`
- Binary SHA-256: `a19bd5a2994b3f5fbf511001b134352564af9a409192654db7fa8e55889207b3`
- Backend: `SLINT_BACKEND=software` on `DISPLAY=:0`
- Screenshot command: `/usr/bin/gnome-screenshot -w -f <path>`
- Requested window: 1024×720; PNG captures include the native title bar and are 1024×741.
- Orca 46.1 and the AT-SPI tree were enabled for the accessibility checks. The temporary accessibility settings were restored after the run.

## Results

- The reopened `.loomtable` shows the saved A2 value `groceries` and the correct filename in its title. The earlier native save/open flow created and reopened this file through the desktop file picker.
- A native keyboard edit changed A2 to `vegetables`; the window title gained `*`. AT-SPI reported `A2 selected; value: vegetables; formula: vegetables` on the focused `Example Budget worksheet grid`. The unsaved test edit was not written to the sample file.
- The compact toolbar export icon now has the accessible name `Export CSV` and description `Export the active worksheet as CSV`.
- On chooser launch, AT-SPI reports the named `Template chooser` group focused with the selected-template instructions. Right changes the description to `Checklist selected. Use Left and Right to choose; Return creates it; Escape cancels.` Orca's speech log confirms it spoke that text.
- Return creates the selected Checklist workbook, whose sample rows and formulas are visible. Focus returns to the named `Checklist worksheet grid` group.
- Escape closes the chooser without changing the existing workbook and restores focus to its worksheet grid. The post-cancel screenshot matches the opened-workbook screenshot by SHA-256.
- The chooser and workbook layouts fit in the native 1024×720 window at 1.25× text scale. Older native captures still document the light, dark, and high-contrast blank/chooser states.

## Native screenshots

These app-only captures are also checked into `loom-sheets/docs/qa-native/`:

- `opened-saved-workbook-linux.png` — reopened saved file, including A2 `groceries`.
- `unsaved-edit-title-linux.png` — edited A2 and visible `*` dirty marker.
- `template-chooser-checklist-selected-linux.png` — selected Checklist card.
- `template-cancel-preserves-workbook-linux.png` — Escape returns to the unchanged saved workbook.
- `template-created-checklist-linux.png` — Return creates the Checklist workbook.

## Limits

This is one Linux desktop at 1024×720 in the light theme. A native 1440×900 capture is not possible on the 1366×728 display; larger viewports and the other themes remain covered only by existing renderer evidence. These checks do not cover every dialog failure/cancellation path, representative large-workbook performance, all keyboard commands, or complete screen-reader coverage. They do not by themselves accept Sheets.

## UI-06 status and error feedback — 2026-09-23

The status feedback follow-up was checked in the live Linux app. The final build keeps formula errors visible after grid projection, keeps the cell/formula count in a separate status field, and makes the visible error text a polite accessibility live region. Read-only save errors are shortened to a direct message.

- `loom-sheets/docs/qa-native/ui06-formula-error-live-status-linux.png` — invalid `=1/0` formula, visible `#DIV/0!`, status `Formula error in A1: #DIV/0!`, and separate cell/formula count. Native window capture: 1024×752.
- `loom-sheets/docs/qa-native/ui06-readonly-save-failure-live-status-linux.png` — read-only destination, concise `Save failed: destination is read-only`, and the dirty `*` retained. Native window capture: 1024×752.
- `loom-sheets/docs/qa-native/ui06-save-cancel-dirty-workbook-linux.png` — Save As canceled, status `Save cancelled`, and dirty `*` retained. This capture and screen-reader check are from the native run immediately before the status-summary separation; the Save-cancel callback was unchanged by that separation.

Verification recorded for the final status build: `cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline -q -p loom-sheets-app` passed 112 tests, and `cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` succeeded. The focused checks are `formula_errors_are_visible_in_a_polite_live_region` and `readonly_save_error_feedback_is_short_and_actionable`. Anchored Orca log records contain exactly one speech event for the formula error, one for the read-only save error, and one for the canceled Save As message. The raw temporary logs are not retained; the filtered counts and native captures are the evidence. Orca and the speech-dispatcher backend were stopped and temporary accessibility settings restored after the checks.

## UI-01 in-window menu fallback — 2026-09-23

Linux Mint showed no visible application menu because Loom's production native menu installer only calls the operating-system menu API on macOS. A DBusMenu layout serializer exists, but the app does not publish it to a Linux global-menu host. A toolbar, overflow item, or Ctrl+K palette does not replace a menu bar.

The rebuilt Sheets app now shows File, Edit, View, Table, and Help inside every non-macOS window. The row uses the installed menu descriptors, current command enablement, and the existing guarded action dispatcher. Unsupported entries and empty menus are omitted. Help > Keyboard Shortcuts opens the existing command palette. macOS continues using its native menu.

Verification on 2026-09-23:

- `cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline -q -p loom-sheets-app` passed **115 tests**.
- `cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` succeeded.
- `cargo fmt --manifest-path loom-sheets/Cargo.toml --all --check` and `git diff --check` passed.
- `non_macos_window_exposes_a_local_application_menu_bar` finds File, Edit, View, and Table in the Slint accessibility tree. Projection and guarded-dispatch tests cover enabled state and command routing.
- The headless software-renderer image at `loom-sheets/docs/qa-renderer/ui01-local-menu-1024-linux.png` shows the in-window menu at 1024×720. This preview is not a native screenshot.

Native visual acceptance is still pending. `/usr/bin/gnome-screenshot` was tried against the live app, but `cinnamon-screensaver-command --query` reported `The screensaver is active`; the full-screen Cinnamon screensaver and backup-locker windows were above the app. The capture therefore showed the desktop overlay, not Loom. No screen-lock, accessibility, or display settings were changed. Repeat the native capture and pointer/keyboard menu interaction checks after the desktop session is unlocked.


## source-work-sheets-acceptance-2026-09-24-report-md

Original path: `.work/sheets-acceptance-2026-09-24/REPORT.md`

# Loom Sheets acceptance follow-up — 2026-09-24

## Status

Sheets is **not 100% accepted**. After the shared ZIP-reader change, the Sheets workspace passes 266 tests, `loom-package` passes 29 tests, full-workspace all-target Clippy passes, and the production app builds. The latest build took 3m07 and peaked at 2,985,580 KiB RSS with zero swaps. CODE-25 fixes a real import blocker: a LibreOffice-generated `.xlsx` round-trip now imports four sheets, formulas, the styled total, a chart, and a shape label. CODE-23 warns before a known lossy XLSX import replaces the current workbook, including when started with `--open`. REC-02 remains open because full recovery packages accumulate without an automatic storage bound. The formal gate stays `ACCEPTANCE_BLOCKED`: the corrected live menu and import-warning dialog are still awaiting inspection, recovery on a million-cell workbook takes tens of seconds, ordinary file work and full workbook replacements remain synchronous on the UI thread, and native frame rate, app memory, full visual/accessibility coverage, and broader interoperability are not proven.

## What changed

- The in-window File/Edit/View/Table/Help menu now gives the popup only the selected menu's rows. The old code let hidden rows from other menus take up space, so Edit appeared far below its menu label. The old native Edit capture is preserved as `loom-sheets/docs/qa-native/ui01-edit-menu-open-before-fix-linux.png`; it is before-fix evidence.
- XLSX export now gives sanitized sheet names unique names and rewrites formula and chart references to the names actually written to the file. For example, source tabs `A/B` and `AB` export as `AB` and `AB (2)` instead of silently colliding.
- Dynamic-array spill placement now uses row and column dimensions in the right order. A 2×3 `SEQUENCE` fills B1:D2 and formulas that read those cells before the spill are recalculated. A blocked spill leaves no partial values behind.
- The formula evaluator skips spill-reader bookkeeping when a workbook has no array formulas. The active sheet's calculated values are also reused for view-only updates (selection, scrolling, and resize) and recalculated after edits or a tab change.
- Formula-bar cell commits now send one revision-tagged cell delta to the workbook worker instead of cloning the workbook, recalculating, and writing recovery on the UI thread. The UI immediately shows `Calculating…`, keeps the previous calculated values visible, rejects stale/wrong-tab results, and preserves a newer formula draft.
- XLSX import now reports known losses from OOXML before replacing the current workbook. The warning covers defined names, external links, conditional formatting, validation rules, PivotTables, frozen panes, custom row/column sizes, additional charts on one sheet, and drawing/media parts that are missing. Both the file picker and startup `--open` use the same warning. Cancel leaves the current workbook and recovery data untouched; Continue imports supported content and names the dropped features. Automated fixtures cover every warning and a supported file that should not warn; an empty `<definedNames/>` container also correctly produces no warning. The shared command dispatcher blocks menu and palette actions while the decision is pending and rechecks queued native actions. Escape cancels after Tab moves focus to a dialog button. Live native dialog review is still pending.
- The shared ZIP reader now accepts ordinary DEFLATE-compressed entries and ZIP data descriptors. Before this, a Calc-generated `.xlsx` failed before import with `unsupported compression method 8`. A real Calc-generated workbook now reaches the Sheets importer with four tabs, formulas, supported formatting, a chart, and a shape label. Expanded-size limits and CRC checks remain enforced; this fixes standard compression support, not the whole XLSX compatibility matrix.
- The source-size cleanup extracts CLI parsing, headless rendering, and XLSX import flow from the app entry point; splits app tests and Slint dialogs into focused files; and divides core XLSX handling into import, export, XML, style, relationship, and warning modules. During extraction, a Tab-then-Escape regression showed the Save Changes dialog lost its Escape handler when focus moved to a button. The dialog's focus scope now contains its overlay and controls.
- `loom-sheets/PERFORMANCE.md`, this report, the app README, `TRUTH.md`, and the tiny-model performance instructions in `AGENTS.MD` were updated with measured results and remaining limits.

## Automated and local verification

- Latest workspace tests: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline -q` — **266 passed, 0 failed**, covering 142 app tests, 2 cache integration tests, 104 core unit tests, 13 formula tests, 1 performance fixture, and 4 range tests. The scale test skips its million-cell setup unless `LOOM_ENFORCE_SCROLL_BUDGET=1` is set. Exact output: `.work/sheets-acceptance-2026-09-24/zip-deflate-workspace-tests.log`.
- Shared ZIP-package tests: `cargo test --manifest-path loom-core/Cargo.toml --locked --offline -p loom-package` — **29 passed, 0 failed**. Fresh package-only all-target Clippy: `cargo clippy --manifest-path loom-core/Cargo.toml --locked --offline -p loom-package --all-targets -- -D warnings` — passed. Logs: `.work/sheets-acceptance-2026-09-24/interop/zip-deflate-package-tests.log` and `zip-deflate-clippy.log`.
- Fresh full-workspace all-target Clippy: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig RUSTFLAGS='-L native=/tmp/loom-fontconfig' SLINT_EMIT_DEBUG_INFO=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` — passed in **2m15s**. Exact log: `.work/sheets-acceptance-2026-09-24/zip-deflate-workspace-clippy.log`.
- Reverse import smoke check: LibreOffice Calc **24.2.7.2** converted the expanded Loom workbook from ODS back to XLSX. `import_xlsx_sheets` then imported four sheets, formulas, B4 bold/yellow/bordered/right-aligned currency formatting, one chart, and the “Quarterly target” shape label. The only warning was `CustomRowColumnSizes`. Probe source and exact output: `.work/sheets-acceptance-2026-09-24/interop/import_calc_roundtrip.rs` and `import-calc-roundtrip-deflate.log`; the Calc-generated XLSX is in the portable bundle.
- Fresh production app build: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig SLINT_EMIT_DEBUG_INFO=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 /usr/bin/time -v nice -n 19 ionice -c3 cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` — succeeded in **3m07**. Peak RSS was **2,985,580 KiB** with zero swaps. Binary SHA-256: `bbfb0c4c42d018137eef43ae905c0928424cc11d16ee13992d5ff3504d90c9f4`. Exact output: `.work/sheets-acceptance-2026-09-24/zip-deflate-app-build-time.log`.
- CODE-23 TDD logs show the native-command bypass, empty-definedNames false warning, and focused-Escape path failing before their fixes and passing after them: `.work/sheets-acceptance-2026-09-24/xlsx-import-review-red-app.log`, `xlsx-import-review-red-core.log`, `xlsx-import-review-green-app.log`, `xlsx-import-review-green-core.log`, `xlsx-import-focus-red-probe.log`, and `xlsx-import-focus-green-probe.log`.
- The native warning-dialog screenshot and interaction review are still pending because Cinnamon reports that the desktop is locked. No image of the lock screen was captured or presented as app evidence.
- The current all-target workspace Clippy run and production app build both passed after CODE-25 with Slint debug metadata. The separate repo-wide code-structure script confirms all four previously oversized Sheets sources now meet their existing limits; it still reports six legacy byte-limit failures in locked apps, recorded in `.work/sheets-acceptance-2026-09-24/code-structure-after-split.log`.
- Renderer screenshot of the UI-01 menu at 1024×720: `loom-sheets/docs/qa-renderer/ui01-local-menu-current-1024-linux.png` (SHA-256 `4e556f6d3aeed49c2c62c5361ac29effb6f3dbea37ba4eb163ed7b1ec11744b`). It is software-rendered and does not replace the missing native screenshot.
- The focused tab-rename/tab-switch regression `rename_rewrites_qualifiers_and_rejects_collisions` passes. It checks the cross-sheet result stays 25 after the source tab is renamed.
- The evaluation-cache test checks that the same tab reuses one result, switching tabs recalculates, and an edit refreshes the values.
- Optimized 10,000-formula budget: `LOOM_ENFORCE_PERF_BUDGET=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline --release -p loom-sheets-core --test perf_measure -- --nocapture` — passed the 200 ms calculation threshold at **88.5 ms** on an Intel Core i3-2350M, 2 cores, 7.7 GiB RAM. JSON output was 319,175 bytes and took **20.4 ms**; parse took **10.4 ms**. Raw output: `.work/sheets-acceptance-2026-09-24/formula-perf-rerun.log`. This is one low-end local machine result, not the specified mainstream desktop profile.
- Million-cell CPU projection: the opt-in test projected 60 viewports over 1,000,000 numeric cells at unique pseudorandom addresses in a 2,048×2,048 address space, including both workbook corners and balanced cell coverage across all four regions. The debug run measured **1.27 ms p95 / 7.02 ms maximum**. The standalone release test measured **0.17 ms p95 / 0.26 ms maximum**. `/usr/bin/time -v` measured **90,308 KiB peak RSS** for the release test process, with zero swaps. These exclude native rendering and frame presentation; the observed RSS is not the whole interactive application's memory use. Exact release output: `.work/sheets-acceptance-2026-09-24/scroll-random-release-time.log`.
- Million-cell app edit sample: the opt-in test-profile run projected 60 viewports at **1.39 ms p95 / 1.43 ms maximum**, prepared a committed cell edit in **0.049 ms**, and queued it in **0.037 ms**. Worker evaluation took **2,561.653 ms**, recovery package creation **12,721.371 ms**, and recovery journal writing **43,046.444 ms**. Peak RSS for the test process was **2,612,252 KiB** with zero swaps; this includes the million-cell fixture and harness and is not an interactive-app RSS measurement. Exact output: `.work/sheets-acceptance-2026-09-24/cell-commit-perf-time.log`.
- Async result viewport regression: the test first failed because a current worker result reset the user's scroll offsets from `(-180, -672)` to `(0, 0)`. Results now update visible values without revealing the old selected cell; the regression passes and verifies scroll offsets and a concurrently typed formula draft remain unchanged. Red/green logs: `.work/sheets-acceptance-2026-09-24/viewport-preservation-red.log` and `.work/sheets-acceptance-2026-09-24/viewport-preservation-green.log`.
- Independent spreadsheet check: LibreOffice Calc **24.2.7.2** opened `interop/loom-name-collision-v2.xlsx` and converted it to `interop/lo-output/loom-name-collision-v2.ods`. The tabs remained `AB`, `AB (2)`, `Very Long Sheet Name That Excee`, and `Consumer`; the rewritten cross-sheet formula recalculated to `80`; and the quoted text `A/B!B2` remained text. The fixture, converter input/output, and export source are in the portable audit bundle under `.work/sheets-acceptance-2026-09-24/interop/`. This does not prove Excel or other spreadsheet applications behave identically.
- `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check` and `git diff --check` passed on the current source before this documentation refresh; they will be rerun after the report and archive are updated.
- The repo-wide `python3 loom-bootstrap/scripts/audit-code-structure.py` now reports only six legacy byte-limit failures in locked apps (Encode, Motion, Video app/core, Writer core, and Photo). It reports no Sheets source. Exact output: `.work/sheets-acceptance-2026-09-24/code-structure-after-split.log`. No app-wide size limit was relaxed.

An earlier app run omitted `SLINT_EMIT_DEBUG_INFO=1`; two tests that inspect Slint's accessibility tree could not see elements. That run was invalid for those assertions. The current rerun with Slint debug metadata passed all 142 app tests. Test-first regressions caught stale “Calculating…” feedback after a newer workbook revision, a viewport jump when a worker result arrived after scrolling, native commands reaching through the XLSX warning, false warnings for empty defined-name containers, and Escape failing after focus moved to a button in both the XLSX warning and Save Changes dialogs. The regressions now pass. Save Changes red/green logs: `.work/sheets-acceptance-2026-09-24/save-changes-escape-red.log` and `save-changes-escape-green.log`.

## Still blocking a truthful acceptance claim

- **Native menu and XLSX warning check:** UI-01 and CODE-23 behavior pass source-level and app tests, but neither corrected live interaction has been inspected. On this latest attempt, `cinnamon-screensaver-command -q` reported that the screensaver is active, so I left the locked desktop untouched and did not save a false app screenshot. A native capture while unlocked plus pointer/keyboard inspection is still required; the renderer preview is not a substitute.
- **Visual scope:** all required viewports, themes, text sizes, and RTL states still need a complete inspected matrix. This device's live display is 1366×768, so a native 1440×900 capture is not available here.
- **Responsiveness:** selection, scroll, and resize reuse calculated values; a tab change recalculates. The benchmark timed only cell-edit preparation (0.049 ms) and mailbox submission (0.037 ms); it did not time the complete formula-bar callback, Slint model/status updates, or input-to-visible-frame delay. The million-cell worker then took 2,561.653 ms to evaluate, 12,721.371 ms to create a recovery package, and 43,046.444 ms to write the recovery journal. Recovery can lag far behind the visible edit and must be made fast enough to protect recent work. Non-cell edits still copy `Vec<Sheet>` on the UI thread. Ordinary Open, Save, Save As, CSV, and XLSX callbacks still parse/package/write synchronously.
- **File-work source audit:** `open_workbook_from_picker` reads and imports the full file before replacing the workbook; startup `--open` does the same before the event loop. `save_current_sheet` performs native serialization and atomic write on the UI thread, clones the saved baseline, serializes the package a second time, then blocks on `checkpoint().recv()`. CSV and XLSX exports also serialize and write on the UI thread. `record_workbook_snapshot` calls `workbook_sheets`, cloning every tab before queuing many non-cell changes; `apply_sheet` covers New/Open/template replacement, undo/redo, tab and row/column operations, range edits, formatting, objects, and sizing. Tab changes and transaction/dirty-state handling add further full-sheet/workbook clones. This audit confirmed source paths; it did not run new builds, tests, or GUI sessions.
- **Async file-ordering rule:** the current save blocks the UI, so edits cannot race its recovery checkpoint. Once Save is asynchronous, a checkpoint for saved revision N must not clear a durable recovery entry for later edit N+1. File commands need operation/document identifiers and a target edit revision; they must wait for queued edits through that revision, clone/serialize off the UI thread, and send completions through a dedicated queue that does not coalesce results. Open must parse a candidate and install only if that operation is still current and any import-loss warning was accepted. Save completion may update path/baseline only for its original document. File-write success and recovery-checkpoint failure must be reported separately. Moving an existing callback to a thread after it has cloned the full workbook on the UI thread does not fix the blocking cost.
- **Recovery storage and freshness:** source inspection confirms the worker appends a complete workbook package for each changed batch. `SnapshotRecovery::record` removes exact duplicates only; automatic compaction does not run, and the save checkpoint is the only caller path that compacts. The shared journal rereads and validates its existing records on each append. Therefore an unsaved session has no automatic storage bound, while each new record can cost more than the previous one. The million-cell test measured 2.56 seconds of evaluation, 12.72 seconds of package creation, and 43.05 seconds of journal writing; it did not measure retained bytes. REC-02 now tracks the missing storage bound, durable freshness, and interruption tests.
- **Scale and memory:** CPU projection of the pseudorandom million-cell fixture measured 1.39 ms p95 / 1.43 ms maximum in the test profile, but no real-window test proves native rendering and frame presentation sustain 60 fps. The full test process peaked at 2,612,252 KiB and used no swap; this includes its million-cell fixture and test harness, not an interactive app. Earlier standalone release projection RSS was 90,308 KiB and measured only that different test process. The app's peak-memory limit still needs an owner-reviewed numeric target.
- **Interoperability:** LibreOffice now verifies Loom XLSX export to ODS and back to XLSX, and Loom imports the resulting four-sheet workbook with its supported values, formatting, chart, and shape. Custom row/column sizes are warned about and dropped. Microsoft Excel and the wider import/export feature matrix remain unverified. CODE-23 detects listed known losses and requires confirmation in both the picker and startup `--open` path. Its warning fixtures, supported no-warning fixture, Cancel, Continue, and startup recovery preservation tests pass. Live native inspection of the warning dialog remains pending; this evidence does not prove full Excel compatibility, and imported Loom workbooks still omit features named in the warning after Continue.
- **Accessibility:** existing native evidence covers the named Linux flows in the 2026-09-23 report. Full keyboard command, screen-reader, dialog error/cancel, and cross-platform coverage remains open.

Keep the app gate `ACCEPTANCE_BLOCKED` until each of these items has its own observed evidence. Do not report absolute 100% based on unit tests or the single local benchmark.


## source-work-sheets-acceptance-2026-09-24-scroll-acceptance-plan-md

Original path: `.work/sheets-acceptance-2026-09-24/SCROLL-ACCEPTANCE-PLAN.md`

# Sheets Million-Cell Scroll Acceptance Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure the CPU work needed to project a visible Sheets viewport over one million sparse cells, then separate that result from native frame-rate and memory evidence.

**Architecture:** Use an opt-in release-mode app test with a deterministic one-million-cell worksheet and 60 scroll offsets. Assign addresses with a fixed-key Feistel permutation so the cells are unique and random-looking without allocating a second million-entry index. Time only `project_sheet_grid_with_values`; a passing result proves its CPU projection is below one 60 Hz frame on the recorded device, but does not prove rendering, presentation, or whole-app memory acceptance.

**Tech Stack:** Rust, Slint app projection, `SheetViewport`, Cargo release test profile.

**Spec:** `TRUTH.md`, PERF-01; `loom-sheets/PERFORMANCE.md`.

## Global Constraints

- Keep the 10,000-formula calculation budget below 200 ms on the mainstream desktop profile.
- Visible input feedback must be within 16.7 ms, and calculation or file operations must not block the UI thread.
- The acceptance workload contains 1,000,000 randomly placed values and must scroll at 60 fps on the mainstream profile.
- Peak resident memory must be measured on the same workbook; no numeric Sheets limit is approved yet.
- Keep long builds single-job and low-priority on the owner's personal Linux machine.
- A software-rendered projection test is not native window or frame-presentation evidence.

## Review Focus

- Address generation must be unique and random-looking; assert all 1,000,000 cells were inserted and each of the four workbook regions has cells.
- The 60 offsets must cover different rows and columns, including the origin and clamped far corner.
- Each projection must return exactly the visible-cell count even at the far workbook boundary.
- The test must be opt-in so routine workspace tests do not build a million-cell fixture.
- Report p95 and maximum time; do not use the CPU projection result as a claim about native frames or peak RSS.

---

### Task 1: Add the deterministic projection workload

**Files:**
- Create: `loom-sheets/crates/loom-sheets-app/src/perf_tests.rs`
- Modify: `loom-sheets/crates/loom-sheets-app/src/main.rs`

**Interfaces:**
- Consumes: `project_sheet_grid_with_values(&Sheet, &HashMap<CellRef, Value>, SheetViewport)`.
- Produces: the opt-in test `million_sparse_cells_project_one_viewport_within_one_frame`.

- [x] Add a fixed-key Feistel permutation over a 2,048×2,048 address range and insert exactly one million numeric strings; verify uniqueness and spread across all four regions.
- [x] Measure 60 viewport projections, including both workbook corners, and assert the maximum is below 16.7 ms when `LOOM_ENFORCE_SCROLL_BUDGET` is set.
- [x] Confirm the opt-in test builds and passes in debug; it measures 1.27 ms p95 and 7.02 ms maximum in this unoptimized profile.

Run: `LOOM_ENFORCE_SCROLL_BUDGET=1 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app million_sparse_cells_project_one_viewport_within_one_frame`.

### Task 2: Run the optimized CPU projection measurement

**Files:**
- Test: `loom-sheets/crates/loom-sheets-app/src/perf_tests.rs`
- Record: `loom-sheets/PERFORMANCE.md`

**Interfaces:**
- Consumes: the opt-in test from Task 1.
- Produces: 60-frame p95 and maximum CPU projection time for this machine and exact fixture.

- [x] Run the exact release test binary with `LOOM_ENFORCE_SCROLL_BUDGET=1` and record its machine, exact command, p95, maximum, and process RSS: p95 0.17 ms, maximum 0.26 ms, 90,308 KiB peak RSS, zero swaps on an Intel Core i3-2350M.
- [x] The corrected release workload stays below 16.7 ms; no visible-cell optimization is needed for this CPU-only check. Native rendering and presentation remain unmeasured.
- [x] Run `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check` and the full workspace tests after the fixture correction; 227 passed, 0 failed.

Build the release test harness with `LOOM_ENFORCE_SCROLL_BUDGET=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline --release -p loom-sheets-app million_sparse_cells_project_one_viewport_within_one_frame --no-run`, then run the built test under `/usr/bin/time -v`. On this host the exact invocation was `/usr/bin/time -v env LOOM_ENFORCE_SCROLL_BUDGET=1 loom-sheets/target/release/deps/loom_sheets-d845744821bda674 --exact perf_tests::million_sparse_cells_project_one_viewport_within_one_frame --nocapture`; it passed in 1.62 seconds with 0.17 ms p95, 0.26 ms maximum, and 90,308 KiB peak RSS. The Cargo package test command also started compiling the ordinary GUI binary before running the filtered test, so that unnecessary build was stopped after the test harness existed; the test binary itself was run and measured directly.

### Task 3: Close the native and memory evidence gaps

**Files:**
- Record: `loom-sheets/PERFORMANCE.md`
- Evidence: `loom-sheets/docs/qa-native/`

**Interfaces:**
- Consumes: the one-million-cell fixture specification from Task 1.
- Produces: native scrolling/frame-presentation and peak-RSS results for the exact same workbook.

- [ ] After the desktop lock is cleared, open the actual Linux app, load the million-cell fixture, scroll through 60 positions, and capture the live window with the native screenshot tool. A fresh attempt still showed the lock/saver overlay even though `cinnamon-screensaver-command -q` reported inactive; no lock setting was changed.
- [x] Measure peak RSS for the exact million-cell fixture and record `/usr/bin/time -v`: test-process peak was 90,308 KiB with zero swaps. This does not establish full interactive-app RSS; that measure remains open.
- [ ] Ask the product owner to review a numeric Sheets memory limit after the measured result is visible; keep the gate open until the limit is recorded.
- [ ] Re-run the 10,000-formula benchmark and workspace tests after any optimization.

The current automated test reports CPU projection and test-process RSS only. The native display capture showed the desktop lock/saver overlay, so native app scrolling and screenshot evidence cannot be completed from this session unless the desktop is accessible.


## source-work-sheets-acceptance-2026-09-24-async-cell-commit-plan-md

Original path: `.work/sheets-acceptance-2026-09-24/async-cell-commit-plan.md`

# Sheets Async Cell Commit Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a committed formula-bar cell edit return control to the UI within one frame while a worker recalculates and records crash recovery without losing newer edits.

**Architecture:** One worker owns the recovery journal and a mirror of the workbook. The UI sends revision-tagged cell deltas; a one-running/one-pending mailbox merges repeated writes to the same cell, while preserving distinct cell changes. A UI-thread timer accepts only results for the newest revision and active tab. Non-cell mutations send a full workbook replacement as a correctness fallback; its UI-side copy cost remains a measured blocker until those paths are converted to small deltas.

**Tech Stack:** Rust, Slint 1.17, `std::thread`, `Mutex`/`Condvar`, `SnapshotRecovery`, existing Sheets workbook/package APIs.

**Spec:** `AGENTS.MD` responsiveness instructions; `TRUTH.md` PERF-01; `loom-sheets/PERFORMANCE.md`.

## Global Constraints

- Show visible “Calculating…” feedback immediately after a cell commit.
- Keep old calculated values visible and marked stale until the current result arrives.
- Do calculation and recovery serialization/journal writes on one background worker.
- Do not clone the whole workbook on the UI thread for a committed cell edit.
- Allow at most one running calculation and one coalesced newest pending input state.
- Accept a result only when both its revision and active tab still match the UI.
- An async result must not reset or overwrite a formula draft the user is typing.
- Preserve every committed edit, undo, redo, tab change, and recovery failure signal.
- Run builds single-job and low-priority; no additional worker job queue may grow without bound.
- Keep this plan and temporary logs under ignored `.work/`; do not add an implementation plan to source control.

## Review Focus

- Two fast commits to the same cell: the worker must calculate the newest formula and keep one pending cell entry.
- Fast commits to different cells: the worker must preserve both changes even when it coalesces evaluation.
- Undo/redo while a calculation is running: stale results must not overwrite the latest visible state.
- Switching tabs before a result arrives: discard the wrong-tab result and request or use the current tab's values.
- Typing a new formula while an old calculation finishes: preserve the draft byte-for-byte.
- Recovery package/journal failure: keep the edited workbook in memory and show a visible failure.
- Large non-cell workbook mutation: preserve all document data; report any remaining UI-side copy cost instead of claiming the whole responsiveness gate passed.

---

### Task 1: Remove full serialization from the ordinary dirty-title check

**Files:**
- Modify: `loom-sheets/crates/loom-sheets-app/src/main.rs` (`GuiState`, `mark_saved`, `is_dirty`, `sync_window_title`, undo/redo callbacks)
- Test: `loom-sheets/crates/loom-sheets-app/src/main_tests.rs`

**Interfaces:**
- Consumes: the existing saved workbook snapshot and existing `SheetTransaction` undo/redo path.
- Produces: `mark_content_dirty()`, `recompute_dirty_from_saved()`, and an O(1) `is_dirty()` for ordinary edits and title refreshes.

- [x] Add a regression that saves a workbook, marks a cell edit dirty, restores the saved content, calls the undo dirty-state refresh, and verifies the dirty marker clears.
- [x] Run the focused test and confirm it fails because the cached dirty-state API is absent.
- [x] Add a cached dirty flag; set it on content edits, clear it in `mark_saved`, and recompute full equality only after undo/redo where exact restoration must be detected.
- [x] Keep active-tab dirty state explicit and O(1), matching the serialized workbook's active-tab behavior.
- [x] Run the focused dirty-state tests and existing undo/redo tests.

### Task 2: Add a bounded worker mailbox for workbook mutations

**Files:**
- Create: `loom-sheets/crates/loom-sheets-app/src/workbook_worker.rs`
- Modify: `loom-sheets/crates/loom-sheets-app/src/main.rs` (module declaration and worker ownership in `GuiState`)
- Test: `workbook_worker.rs` unit tests

**Interfaces:**
- Consumes: owned startup workbook, active tab, and the existing recovery application ID/schema.
- Produces: `WorkbookWorker`, `CellUpdate { revision, sheet, cell, raw }`, `WorkbookResult { revision, active_sheet, values, recovery_error }`, and a coalesced pending mailbox.

```rust
struct CellUpdate {
    revision: u64,
    active_sheet: usize,
    cell: CellRef,
    raw: Option<String>,
}

struct WorkbookResult {
    revision: u64,
    active_sheet: usize,
    values: HashMap<CellRef, Value>,
    recovery_error: Option<String>,
}
```

- [x] Test that two updates to A1 keep only the newest raw value, while a pending B1 update remains present and the reported revision is newest.
- [x] Test that a full workbook replacement discards older pending cell updates but preserves later cell deltas.
- [x] Run those tests and confirm they fail before implementing the mailbox.
- [x] Implement a single worker thread with one active batch, one pending coalesced batch, and a latest-only result slot; keep workbook mutation order intact.
- [x] Let the worker apply cell deltas, call `evaluate_workbook`, package the current workbook, and append the recovery record on its own thread.
- [x] Add a real temporary-directory test that replaces the worker's recovery directory with a regular file after startup, then verifies the write failure is returned without losing the in-memory edited result.
- [x] Run worker tests, including shutdown with one pending edit.

### Task 3: Preserve the correct workbook model through worker startup and replacements

**Files:**
- Modify: `loom-sheets/crates/loom-sheets-app/src/main.rs` (recovery initialization, startup, `apply_sheet`, `record_workbook_snapshot`)
- Modify: `loom-sheets/crates/loom-sheets-app/src/workbook_io.rs` only if the worker needs a small pure startup helper
- Modify: `loom-sheets/crates/loom-sheets-app/src/evaluation_cache.rs`
- Test: `main_tests.rs` and `workbook_worker.rs`

**Interfaces:**
- Consumes: initial open path, recovered payload, starter/blank workbook, and every existing non-cell `apply_sheet` mutation.
- Produces: one recovery-writer owner on the worker thread, an initial evaluated cache value, and an async full-replacement fallback for mutations not yet expressed as deltas.

- [x] Add a startup test proving the worker returns the recovered or loaded workbook and its active values match `evaluate_workbook`; keep the sole `SnapshotRecovery` value inside the worker thread by construction.
- [x] Replace GUI-thread recovery record calls with worker requests; do not open a second journal writer on another thread.
- [x] Start the worker with the workbook model and have it create any UI copy on its own thread before sending initialization data back.
- [x] Change `apply_sheet` to project the last good values immediately and submit a revision-tagged full replacement for non-cell changes; do not recalculate or serialize the package on the UI thread.
- [x] Run replacement, recovery, tab-switch, and persistence tests; record the remaining UI-side full-copy cost as an open gate.

### Task 4: Make cell commits asynchronous and revision-safe

**Files:**
- Modify: `loom-sheets/crates/loom-sheets-app/src/cell_actions.rs`
- Modify: `loom-sheets/crates/loom-sheets-app/src/main.rs` (`apply_sheet`, projection, `GuiState`, `run_gui_with_dialogs`)
- Modify: `loom-sheets/crates/loom-sheets-app/src/evaluation_cache.rs`
- Modify: `loom-sheets/crates/loom-sheets-app/ui/app.slint` only if an accessible stale/calculating status cannot use current status fields
- Test: `main_tests.rs`, app UI journey tests, and `perf_tests.rs`

**Interfaces:**
- Consumes: a successfully committed raw cell delta and the worker result slot.
- Produces: immediate `Calculating…` feedback, preserved previous values, and newest-revision-only projection with no formula-draft reset.

- [x] Add tests for a newer revision rejecting an old result and for a changed active tab rejecting a result from another tab.
- [x] Add a UI-state regression that keeps a newer formula draft byte-for-byte unchanged when an older completion arrives.
- [x] Run the regressions. The stale-revision/tab guards were introduced with Task 3 result plumbing, so their Task 4 tests passed before a new Task 4 implementation; the cell-commit regression did fail first (`FullReplacement` instead of `CellDelta`). This is recorded as a TDD-ordering ruling in the SDD ledger.
- [x] Route formula-bar cell commits through a cell delta; remove `apply_sheet`'s active-sheet clone, synchronous formula evaluation, and per-edit recovery write from that path.
- [x] Replace the extra `evaluate(&state.current)` feedback call with the accepted worker result.
- [x] Use a values-only completion projection that does not call `reset_formula_edit_buffer`.
- [x] Clear obsolete “Calculating…” formula feedback when a newer workbook revision replaces a pending cell commit; preserve the user's current formula draft.
- [x] Keep the user's scrolled viewport in place when a worker result refreshes visible values; an async completion must not reveal the old selected cell.
- [x] Poll the latest result from a persistent UI-thread Slint timer; discard it unless revision and active tab both match.
- [x] Extend the opt-in 1,000,000-cell performance test to measure UI-side cell-commit preparation and mailbox submission separately from worker evaluation and recovery time; assert preparation is below 16.7 ms.
- [x] Run focused tests, workspace tests, formatting, and all-target app Clippy.

### Task 5: Record honest completion evidence for this milestone

**Files:**
- Modify: `loom-sheets/PERFORMANCE.md`
- Modify: `loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md`
- Modify: `TRUTH.md`
- Modify: `loom-bootstrap/audit-evidence-2026-09-14-and-22.zip`

**Interfaces:**
- Consumes: raw worker/cell-preparation measurements and test logs.
- Produces: updated acceptance evidence that names remaining synchronous replacement clones, Save/Open operations, native frame rate, accessibility, and memory-target gaps.

- [x] Record the exact workload, machine, commands, and separate UI preparation, evaluation, recovery package, and recovery journal times; clarify that preparation plus mailbox timings exclude the rest of the callback and visible frame.
- [x] Keep Sheets `ACCEPTANCE_BLOCKED` until every required acceptance item has evidence.
- [x] Validate the portable archive, update its SHA-256 in `TRUTH.md`, and run `git diff --check`.
- [x] Commit and push the code milestone `124afc8` and evidence milestone `25a8b8c` under the owner's existing instruction.

The regular Save/Open/import/export callbacks remain a separate required follow-up after this cell-commit milestone. This plan does not certify the application as 100% complete.


## source-work-sheets-acceptance-2026-09-24-interop-styles-relationship-diagnostic-md

Original path: `.work/sheets-acceptance-2026-09-24/interop/styles-relationship-diagnostic.md`

# XLSX style interoperability probe

LibreOffice Calc 24.2.7.2 converted Loom's exported XLSX fixture to ODS. The original package contains `xl/styles.xml`, a content-type override, and a worksheet style index on B4, but `xl/_rels/workbook.xml.rels` has no relationship to the styles part. The converted ODS therefore gave B4 no cell style.

A diagnostic copy with `applyFont`, `applyFill`, `applyBorder`, and `applyNumberFormat` added to the XF still lost the style. A second diagnostic copy with only a workbook relationship to `styles.xml` retained the cell style and currency value. This isolated the package relationship as the cause.

After the exporter fix, the full generated fixture converted successfully. `assert_ods.py` verifies all four sheet names, the B4 yellow fill/bold/border/right alignment/two-decimal currency format, three cross-sheet formula results (80, 30, 60), literal `A/B!B2` text, the chart frame, and the shape label. This is evidence for this fixture and Calc version only; it is not a full Excel or interoperability certification.


## source-work-uiux-audit-2026-09-14-audit-md

Original path: `.work/uiux-audit-2026-09-14/AUDIT.md`

# Loom UI/UX audit — 14–15 September 2026

Completed a visual audit of all eight current-source desktop applications, plus native Sheets and Writer interactions. **The inspected experience is not ready for acceptance:** it includes unsafe document replacement, misleading template/preview/readiness states, inaccessible high-contrast text, and controls that become difficult to find or use. This is a functional alpha with useful foundations; passing builds do not resolve these failures.

The durable repair queue is [TRUTH.md](AGENTS.md#current-truth); the one-card-at-a-time instructions are [AGENTS.MD](AGENTS.md). There are **19 code findings, 25 UI/UX cards, and one CI coverage item**. All remain OPEN. No product repair was implemented in this audit.

## Scope and evidence

- Eight apps were rebuilt from commit `8fce782` plus the existing uncommitted Sheets work. Every final build and scripted capture exited successfully; warnings are retained in logs.
- Scripted captures show real application-rendered states at 1440×900 light/dark, 1024×720 light, and an open palette. Sheets and Writer have additional template, document, and high-contrast variants. These are rendered checkpoints, not claims of completed native journeys.
- Native interactions used an isolated Xephyr display `:99` (1500×1000) with Metacity and isolated application state. Screenshots show that private display; no personal files or user desktop were captured. Sheets used 1280×800; Writer used a 1024×720 client.
- Each of the 49 accepted images below was saved and visually inspected. The earlier Sheets executable captures and transient frames are excluded from this final set; fresh current-source captures supersede them.
- Screen readers, complete tab order, RTL, all text scales, Windows/macOS, all dialog failures, real media backends/audio devices, performance at production scale, and full end-to-end workflows remain unverified. Source traces and recommendations are identified explicitly.

## Ordered walkthrough

### 1. Sheets

**Health:** Blocked: unsafe New, unreadable high-contrast headers, missing feedback and file-menu discoverability.

**Repair references:** UI-01–10; CODE-04. Read the complete card in TRUTH for files, steps, and acceptance checks.

**1.1 — Wide startup.** The grid is orderly and selected cells are clear. The inspector occupies substantial width for few properties. Budget mixes weekly and monthly amounts without normalization; no period/unit is explained. UI-02/04.

![Sheets: Wide startup](.work/uiux-audit-2026-09-14/sheets/01-start-light-1440.png)

**1.2 — Compact startup.** Collapsing the inspector gives the grid more space, a useful behavior to preserve. Formatting still needs an accessible compact route; the complete alternate route was not verified. UI-02/14.

![Sheets: Compact startup](.work/uiux-audit-2026-09-14/sheets/02-compact-light-1024.png)

**1.3 — Command search.** Search results are readable, and export formats are named clearly. This scripted palette state does not prove initial pointer discoverability, focus return, or execution.

![Sheets: Command search](.work/uiux-audit-2026-09-14/sheets/03-palette-light.png)

**1.4 — Choose a template.** Cancel and Create remain visible, but right-hand previews and names are cut off. Recents duplicates a hard-coded Blank entry. Reflow full cards and use real recent choices. UI-03.

![Sheets: Choose a template](.work/uiux-audit-2026-09-14/sheets/04-template-chooser.png)

**1.5 — Inspect the Budget chart.** Bars have readable value labels. Total and Average are plotted alongside individual expenses with no visible source range or unit. Require an explicit range; do not automatically suppress user-selected formula rows. UI-08.

![Sheets: Inspect the Budget chart](.work/uiux-audit-2026-09-14/sheets/05-chart.png)

**1.6 — Inspect worksheet objects.** The selected shape and resize handle are visible. User-inserted objects legitimately overlap cells. This seeded capture does not establish native dragging, image recovery, or XLSX fidelity.

![Sheets: Inspect worksheet objects](.work/uiux-audit-2026-09-14/sheets/06-objects.png)

**1.7 — Dark theme.** Dark chrome retains readable document cells and selected-cell borders. The light paper surface is not itself a defect. Dark mode does not establish high-contrast correctness.

![Sheets: Dark theme](.work/uiux-audit-2026-09-14/sheets/07-dark.png)

**1.8 — High contrast.** Item, Amount, and Note disappear into black header fill, while the formula field still contains Item. The text exists but the chosen foreground/background pair hides it. UI-05.

![Sheets: High contrast](.work/uiux-audit-2026-09-14/sheets/08-high-contrast.png)

**1.9 — Native launch, isolated Linux desktop.** Fresh current-source build at 1280×800 shows Budget and no visible New/Open/Save menu. There is no global menu host in this test environment. Title/status do not identify document save state. UI-01/02/04/06.

![Sheets: Native launch, isolated Linux desktop](.work/uiux-audit-2026-09-14/sheets/17-current-native-start.png)

**1.10 — Type and commit audit123 into A1.** The grid confirms the edit. The native title stays Untitled and there is no visible dirty/status indicator. This is an actual pointer/keyboard interaction, not a seeded screenshot. UI-06.

![Sheets: Type and commit audit123 into A1](.work/uiux-audit-2026-09-14/sheets/18-current-native-edit.png)

**1.11 — Press Ctrl+N after committing.** The workbook immediately becomes blank with no save/discard/cancel decision. The formula field still paints audit123 over the empty placeholder. CODE-04 and UI-07.

![Sheets: Press Ctrl+N after committing](.work/uiux-audit-2026-09-14/sheets/19-current-native-new.png)

**1.12 — Try Ctrl+Z after New.** The blank workbook remains and the stale field persists. Undo does not restore the discarded Budget edit. These results were reproduced after rebuilding current source.

![Sheets: Try Ctrl+Z after New](.work/uiux-audit-2026-09-14/sheets/20-current-native-undo.png)

**1.13 — Open commands with Ctrl+K.** The palette works and lists shortcuts. Its New Sheet command actually replaces the workbook; name whole-file actions accurately. UI-09. Full keyboard navigation and screen-reader output were not tested.

![Sheets: Open commands with Ctrl+K](.work/uiux-audit-2026-09-14/sheets/21-current-native-palette.png)

**1.14 — Inspect the overflow menu.** Overflow contains Export CSV only, so it does not solve pointer access to New/Open/Save in this environment. The captured app window is complete; the black surrounding area is the private display. UI-01.

![Sheets: Inspect the overflow menu](.work/uiux-audit-2026-09-14/sheets/22-current-native-overflow.png)

### 2. Writer

**Health:** Blocked: selected template creates a different document; compact review and table presentation are incomplete.

**Repair references:** UI-11–14; CODE-04/05/13/17. Read the complete card in TRUTH for files, steps, and acceptance checks.

**2.1 — Wide document.** The page is visually dominant and the status bar is visible. Sample prose advertises implementation details instead of helping the user write; keep it out of Blank and label samples explicitly. UI-11.

![Writer: Wide document](.work/uiux-audit-2026-09-14/writer/01-1440x900-light.png)

**2.2 — Compact document.** The page remains usable, but the inspector is unavailable at this breakpoint. Do not remove a control category without another reachable route. UI-14.

![Writer: Compact document](.work/uiux-audit-2026-09-14/writer/02-1024x720-light.png)

**2.3 — Dark theme.** Light paper stays readable against dark chrome. Document colors remain stable here, a useful behavior. Full contrast and accessibility were not measured.

![Writer: Dark theme](.work/uiux-audit-2026-09-14/writer/03-1440x900-dark.png)

**2.4 — Command palette.** Commands, search field, and keyboard hints are readable. This seeded state does not verify all command enablement or focus transitions.

![Writer: Command palette](.work/uiux-audit-2026-09-14/writer/04-1440x900-light-palette.png)

**2.5 — Request comments/table at compact width.** The capture seeds a comment and table and requests the inspector. The breakpoint hides that panel; the table appears as Markdown text. UI-13/14.

![Writer: Request comments/table at compact width](.work/uiux-audit-2026-09-14/writer/05-inspector.png)

**2.6 — Template chooser at 1024×720.** All six cards fit in two rows, and the selection/action area is clear. This is better reflow than Sheets. Card identity still needs the native check below.

![Writer: Template chooser at 1024×720](.work/uiux-audit-2026-09-14/writer/06-chooser.png)

**2.7 — High contrast with review fixture.** Paper and body text remain readable in this captured state. The inspector is still hidden at compact width. This is not screen-reader or complete contrast certification.

![Writer: High contrast with review fixture](.work/uiux-audit-2026-09-14/writer/07-high-contrast.png)

**2.8 — Comments/table at wide width.** The inspector now shows the seeded comment, proving it exists. No visible anchor highlights it on the page; the populated table still shows literal pipes. UI-13/14; CODE-17 covers anchor drift.

![Writer: Comments/table at wide width](.work/uiux-audit-2026-09-14/writer/08-inspector-wide.png)

**2.9 — Native chooser.** Fresh current-source native launch shows the template chooser in the private display. No real user document is open in this fixture.

![Writer: Native chooser](.work/uiux-audit-2026-09-14/writer/09-native-start.png)

**2.10 — Select Executive Report.** The card is visibly selected. The following Create Document action should construct this report.

![Writer: Select Executive Report](.work/uiux-audit-2026-09-14/writer/10-native-report-selected.png)

**2.11 — Create the selected report.** The result is a letter with Your Name and Dear Recipient; status explicitly says Created letter document. Six UI positions and four callback mappings disagree. UI-12. Save/export of this result was not exercised.

![Writer: Create the selected report](.work/uiux-audit-2026-09-14/writer/11-native-created-letter.png)

### 3. Present

**Health:** Needs repair: navigator placement and repetitive empty inspector; reliability blockers remain.

**Repair references:** UI-15; CODE-04/16/19. Read the complete card in TRUTH for files, steps, and acceptance checks.

**3.1 — Wide deck.** The canvas is large and readable. The LTR filmstrip is on the right beside the inspector, contrary to the left-navigator contract. No element selected is repeated with empty geometry fields. UI-15.

![Present: Wide deck](.work/uiux-audit-2026-09-14/present/01-1440x900-light.png)

**3.2 — Compact deck.** The optional inspector disappears and the slide thumbnails remain visible, preserving basic orientation. The filmstrip is still on the right. Full compact property access was not tested.

![Present: Compact deck](.work/uiux-audit-2026-09-14/present/02-1024x720-light.png)

**3.3 — Dark theme.** The slide paper remains readable against dark chrome. Filmstrip and inspector placement issues persist. Toolbar icons need native tooltip/name checks before accessibility acceptance.

![Present: Dark theme](.work/uiux-audit-2026-09-14/present/03-1440x900-dark.png)

**3.4 — Command palette.** The modal and its entries are legible. These captures do not establish slide creation, transitions, undo, or exported deck correctness; see the separate code findings.

![Present: Command palette](.work/uiux-audit-2026-09-14/present/04-1440x900-light-palette.png)

### 4. Photo

**Health:** Blocked preview trust: image clipping; precision entry and compact properties need repair.

**Repair references:** UI-14/16/18; CODE-04/14. Read the complete card in TRUTH for files, steps, and acceptance checks.

**4.1 — Wide image workspace.** Image content dominates, but its right/bottom portions and selection edges are clipped inside the viewport. The source fits content against the larger stage instead of the viewport. Transform values are static labels beside sliders. UI-16/18.

![Photo: Wide image workspace](.work/uiux-audit-2026-09-14/photo/01-1440x900-light.png)

**4.2 — Compact image workspace.** Clipping remains and the inspector disappears. An alternate compact property route needs native verification. Canvas whitespace alone is not a defect. UI-14/16.

![Photo: Compact image workspace](.work/uiux-audit-2026-09-14/photo/02-1024x720-light.png)

**4.3 — Dark theme.** The artwork remains visible, but changing chrome does not solve the clipped image rectangle. Verify all corners and coordinate mapping after repair. UI-16.

![Photo: Dark theme](.work/uiux-audit-2026-09-14/photo/03-1440x900-dark.png)

**4.4 — Export command search.** PNG and JPEG choices are explicit and readable. A palette image is not proof of actual export, color fidelity, layer persistence, or selection manipulation.

![Photo: Export command search](.work/uiux-audit-2026-09-14/photo/04-1440x900-light-palette.png)

### 5. Motion

**Health:** Blocked preview trust: artwork changes with UI theme and differs from SVG output.

**Repair references:** UI-14/17/18. Read the complete card in TRUTH for files, steps, and acceptance checks.

**5.1 — Wide composition.** Layers, stage, timeline, and transforms are visible. Stage labels include type suffixes such as (Text); these are not the text written by the SVG exporter. Precise transform entry needs typed values. UI-17/18.

![Motion: Wide composition](.work/uiux-audit-2026-09-14/motion/01-1440x900-light.png)

**5.2 — Compact composition.** The inspector disappears and the composition-layers heading truncates. Keep a reachable compact editing route; do not infer missing timeline/keyframe features from this screenshot. UI-14.

![Motion: Compact composition](.work/uiux-audit-2026-09-14/motion/02-1024x720-light.png)

**5.3 — Dark composition.** Stage background and title color change with the UI theme. Source export colors are fixed, so the visible artwork cannot serve as a trustworthy output preview. UI-17.

![Motion: Dark composition](.work/uiux-audit-2026-09-14/motion/03-1440x900-dark.png)

**5.4 — Export command search.** Export SVG Frame describes the implemented output more honestly than promising a video render. The native keyframe, playback, and export workflow was not completed in this visual audit.

![Motion: Export command search](.work/uiux-audit-2026-09-14/motion/04-1440x900-light-palette.png)

### 6. Video

**Health:** Blocked setup experience; timeline zoom/scroll labeling and clip contrast need repair.

**Repair references:** UI-19/20/25. Read the complete card in TRUTH for files, steps, and acceptance checks.

**6.1 — Wide timeline.** The viewer is large, and synthetic media is explicitly labeled Offline sample, which is honest. Missing-tool guidance is truncated; zoom button captions make the adjacent scroll slider confusing. Small secondary clip text has weak visible separation from the fill. UI-19/20/25.

![Video: Wide timeline](.work/uiux-audit-2026-09-14/video/01-1440x900-light.png)

**6.2 — Compact timeline.** The timeline and viewer remain available, but the backend instruction is even harder to read fully. Provide readable setup details and a retry route. UI-25.

![Video: Compact timeline](.work/uiux-audit-2026-09-14/video/02-1024x720-light.png)

**6.3 — Dark timeline.** Clip color coding remains clear; secondary text still needs measured foreground/background contrast. No quantitative contrast ratio or screen-reader compliance is claimed. UI-20.

![Video: Dark timeline](.work/uiux-audit-2026-09-14/video/03-1440x900-dark.png)

**6.4 — Command palette.** Commands are readable. A source check distinguishes the single zoom slider from the separate scroll slider. Actual media decoding, playback, caption/export fidelity, and backend recovery were not exercised.

![Video: Command palette](.work/uiux-audit-2026-09-14/video/04-1440x900-light-palette.png)

### 7. Studio

**Health:** Needs substantial editing UX repair: duplicate transport controls and little region detail.

**Repair references:** UI-20/21/22; CODE-10. Read the complete card in TRUTH for files, steps, and acceptance checks.

**7.1 — Wide arrangement.** Tracks and regions are identifiable, but Loop and Metronome each appear twice. Audio regions show generic fills/icons/filenames without sample peaks or a useful sequence of time ticks. UI-21/22.

![Studio: Wide arrangement](.work/uiux-audit-2026-09-14/studio/01-1440x900-light.png)

**7.2 — Compact arrangement.** Library, arrangement, and mixer compete for width while repeated transport actions consume toolbar space. Keep the arrangement primary and verify existing mixer/inspector contract behavior during the app stage. Empty space below tracks is intentional working room.

![Studio: Compact arrangement](.work/uiux-audit-2026-09-14/studio/02-1024x720-light.png)

**7.3 — Dark arrangement.** Bright cyan regions are distinct; light text on them needs a contrast measurement. Device-unavailable/headless-deterministic messages belong to the capture harness, so this image cannot diagnose native device failure. UI-20.

![Studio: Dark arrangement](.work/uiux-audit-2026-09-14/studio/03-1440x900-dark.png)

**7.4 — Command palette.** Search and commands are readable. No real recording, playback, MIDI device, or plugin workflow was tested. Waveform recommendations require real PCM data, not decorative shapes. Native persistence loss is separately documented in CODE-10.

![Studio: Command palette](.work/uiux-audit-2026-09-14/studio/04-1440x900-light-palette.png)

### 8. Encode

**Health:** Blocked readiness clarity: contradictory queue status, sample paths, and crowded settings.

**Repair references:** UI-23/24/25; CODE-09. Read the complete card in TRUTH for files, steps, and acceptance checks.

**8.1 — Wide queue.** Start is correctly disabled while Encoder unavailable is shown, yet the status says Queue ready and a sample-input.mov job is queued. A large idle progress card repeats the selected job; settings tabs wrap. UI-23/24.

![Encode: Wide queue](.work/uiux-audit-2026-09-14/encode/01-1440x900-light.png)

**8.2 — Compact queue.** Three competing panels leave little room for job settings. Destination appears in multiple contexts and Drop destination file here gives unclear guidance. Provide one authoritative destination editor and readable setup recovery. UI-24/25.

![Encode: Compact queue](.work/uiux-audit-2026-09-14/encode/02-1024x720-light.png)

**8.3 — Dark queue.** The warning remains visible, but contradictory readiness and idle progress hierarchy remain. Derive labels and action enablement from the same real prerequisites. UI-23.

![Encode: Dark queue](.work/uiux-audit-2026-09-14/encode/03-1440x900-dark.png)

**8.4 — Command palette.** The palette is readable. The capture does not prove that all listed actions are enabled or that a real encode succeeds. The output overwrite race was reproduced separately with a controlled adapter, not a real codec. CODE-09.

![Encode: Command palette](.work/uiux-audit-2026-09-14/encode/04-1440x900-light-palette.png)

## Repair order and small-model handoff

First repair shared recovery/storage: CODE-01, CODE-02, CODE-18. Then recheck foundation evidence and proceed through the application sequence recorded in TRUTH. Within an app, protect data and truthful output before polishing layout. This report does not unlock later stages.

A local model should receive one card, its named functions/callers, and the applicable AGENTS rules. Each card says what fails, what files to open, the next small actions, and what result proves it works. Atomic persistence, filesystem boundaries, and subprocess concurrency still require experienced review; simplified prose is not evidence that an extremely small model can safely solve those areas alone.

## Reproduction and provenance

Final commands/results are in [capture-manifest.json](.work/uiux-audit-2026-09-14/capture-manifest.json) and [sheets-capture-manifest.json](.work/uiux-audit-2026-09-14/sheets-capture-manifest.json). [accepted-screenshots.json](.work/uiux-audit-2026-09-14/accepted-screenshots.json) records the exact image hashes and notes. Build logs are in `builds/`; native logs and isolated state are in `native-state/`. The native input helper is `native_input.py`, which targets only display `:99`. The audit display and its applications were closed after capture.

Additional Writer variants cover compact comments/table with inspector requested (05), compact template chooser (06), compact high contrast (07), and wide comments/table with inspector requested (08). The app supports `--comment`, `--table`, `--inspector`, `--template-chooser`, `--theme`, and `--size` for reproducing these seeded states. Screenshot switches do not simulate a user completing those actions.

Builds used the existing temporary Rust toolchain/cache at `/tmp/loom-cargo` and `/tmp/loom-rustup`, shared target `loom-sheets/target`, two cargo jobs, and temporary pkg-config/linker paths. Studio needed ALSA development headers: the distribution package was downloaded and extracted under this ignored evidence folder, with no system installation. An earlier Sheets build failed on local dependency configuration; the corrected current-source rebuild succeeded.

```bash
cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app
loom-sheets/target/debug/loom-sheets --screenshot /absolute/output.png --size 1024x720 --theme high-contrast
python3 -m unittest discover -s loom-bootstrap/scripts -p test_audit_governance.py
python3 loom-bootstrap/scripts/audit-governance.py
```

The original [code audit](AGENTS.md#source-work-audit-2026-09-14-audit-md) records 396 existing tests passing and three new shared-recovery regressions failing as expected. Those historical test runs are not rerun or rebranded as UI validation here. Governance validation for the documentation handoff is recorded in `final-verification.json`. The authoritative findings survive removal of this ignored evidence directory because they are included in TRUTH.


## source-work-uiux-audit-2026-09-14-code-repair-cards-md

Original path: `.work/uiux-audit-2026-09-14/code-repair-cards.md`

## Code and workflow repair cards

All cards below are **OPEN**. P1 means user data, trust, or a security boundary is at risk. P2 means a serious workflow defect. These are findings, not implemented fixes. Paths are relative to this repository. Search for the named function; old line numbers can move. Evidence directory: `.work/audit-2026-09-14/`. Recreate a fixture from the instructions if ignored evidence is unavailable.

### CODE-01 — Keep edits after reopening a saved document

**P1 · Shared · OPEN.** Think of the recovery sequence as numbered pages. A new page must have a bigger number than every saved page. Currently a restart resets the number and hides later edits.

**Open:** `loom-core/crates/loom-production/src/lib.rs`, `RecoveryJournal::open`.

1. Read the checkpoint sequence and the last journal sequence.
2. Set the next sequence to one more than the larger value. Handle an empty journal and checked integer overflow.
3. Keep existing file compatibility. Do not delete a checkpoint to hide this error.

**Prove it:** Record `first`, checkpoint `saved`, close, reopen, record `new unsaved edit`, close, reopen. Recovered text must be exactly `new unsaved edit`. Repeat twice. Existing result is `saved contents`. Evidence: `shared-probes/src/lib.rs`, `shared-probes.log`.

### CODE-02 — Publish recovery checkpoints without destroying the last good copy

**P1 · Shared · OPEN · Needs experienced review.** The payload and its checksum are one thing. Replacing just one half makes the saved copy unreadable.

**Open:** `loom-core/crates/loom-production/src/lib.rs`, `checkpoint`, atomic replacement helper.

1. Draw the old checkpoint, new checkpoint, and journal on paper. At every filesystem operation, identify which complete copy a restart can read.
2. Write a new generation into separate files; flush its payload and metadata. Verify both before publishing that generation through one atomic commit point.
3. Preserve the old generation and journal until the new generation is durably published. Never delete the destination before replacing it.
4. Make startup choose a complete valid generation. Report damage; do not silently claim that missing edits were saved.

**Prove it:** Inject failure before/after each write, sync, and publish step. Restart must recover the last committed data plus valid newer journal entries. The existing metadata-temp failure produces `checkpoint digest mismatch` despite a valid journal. Test Linux and Windows replacement semantics before acceptance. Evidence: `shared-probes/src/lib.rs`. A tiny model must not invent an atomic-file protocol alone.

### CODE-03 — Save and load Sheets text exactly

**P1 · Sheets · OPEN.** Saving a sentence must not change any of its letters.

**Open:** `loom-sheets/crates/loom-sheets-core/src/persistence.rs`, `sheet_from_json`, workbook parsing and object parsing.

1. Add round-trip examples with a tab, carriage return, newline, backslash, quote, Unicode, `before\"}after`, and the literal path `C:\new\notes`.
2. Replace delimiter searches and chained string replacements with a real JSON decoder for the complete structure, including sheet names and object fields.
3. Keep the supported legacy format by decoding its actual schema. Reject malformed input with an error; never silently truncate it.

**Prove it:** Save/reopen multiple tabs and recovery snapshots; every original string must compare equal byte for byte, including a sheet named `My "Sheet"`. Evidence: `sheets/repro.rs`, `sheets-repro.log`.

### CODE-04 — Ask before throwing away unsaved work

**P1 · Shared interaction, then one app at a time · OPEN.** New/Open must not silently erase the document being edited.

**Open:** Sheets `src/main.rs` callbacks for New/Open; corresponding Present New, Photo New, Writer Open; all under `loom-<app>/crates/loom-<app>-app/`. Shared dialogs live under `loom-core/crates/loom-desktop/`.

1. Track whether the current document differs from its last successful save. An undo back to the saved state should clear dirty state.
2. Before replacing dirty work, show **Save changes?** with **Save**, **Discard**, **Cancel**. Name the document. Cancel/Escape must be the safe way out.
3. Save: complete the real save first. A cancelled chooser or failed write keeps the old document, path, selection, history, and recovery state. Discard: replace only after that explicit choice.
4. For Open, parse the candidate successfully before swapping it into the live session. Audit close/quit through the same decision helper.

**Prove it:** Type a unique value, invoke New/Open, exercise all three choices and a failed save/open. Cancel/failure preserves the value and undo history. Only successful Save or explicit Discard allows replacement. Writer's initial New opens a template chooser; put the guard at replacement, not at every chooser opening. Original evidence is a callback trace; the fresh Linux Sheets run also observed New clearing `audit123` without a decision and Undo not restoring it.

### CODE-05 — Export every Writer page

**P1 · Writer · OPEN.** Export must not quietly stop halfway through the document.

**Open:** `loom-writer/crates/loom-writer-core/src/export.rs`, `export_pdf`, and Writer pagination/layout code.

1. Use the same page setup, wrapping, and pagination data as the document layout.
2. Start a PDF page for each layout page. Render all fragments in order, with the correct page margins and text styling.
3. Wrap long paragraphs. Do not use a bottom-of-page `break` that drops the remaining document.

**Prove it:** Export 60 uniquely numbered paragraphs. Independently extract the PDF text: all 60 must appear in order, once each. Page count must match layout (the audit fixture lays out three pages but exports one and only 31 paragraphs). Add a long paragraph and non-ASCII text; inspect rendered pages for clipping. Evidence: `documents-repro.log`, `documents/writer-60-paragraphs.pdf`.

### CODE-06 — Write valid XLSX drawing and chart XML

**P1 · Sheets · OPEN.** A file with a chart must still open in another spreadsheet program.

**Open:** `loom-sheets/crates/loom-sheets-core/src/xlsx.rs`, worksheet drawing insertion and chart formula/range serialization.

1. Declare the relationship namespace wherever an `r:id` attribute is written.
2. Escape XML text in chart range formulas. Keep spreadsheet quoting and XML escaping as two separate steps.
3. Use one XML-writing path for all exported parts. Do not fix only the sample name.

**Prove it:** Export charts, shapes, and an embedded image on a sheet named `R&D`. Unzip the XLSX and parse every XML part with an independent XML parser. Verify relationships resolve and chart data points to the intended cells. Then open the file in an independent spreadsheet app without a repair warning. Current parser failures: `unbound prefix` and unescaped `&`. Evidence: `sheets/validate_xml.py`, `sheets-xml.log`.

### CODE-07 — Stop undo history from copying itself

**P1 · Sheets · OPEN.** A history entry must contain document changes, not another complete history full of histories.

**Open:** `loom-sheets/crates/loom-sheets-app/src/main.rs`, `commit_workbook_transaction`, `stash_live_history`, workbook snapshot/transaction types.

1. Separate document state from undo stacks. A document snapshot must not contain workbook transactions.
2. Store document-only before/after snapshots or a bounded delta. Preserve active-tab and per-tab edit behavior explicitly.
3. Apply an actual byte budget as well as an entry count. Releasing an old entry must release its owned data.

**Prove it:** Rename a tab 9 times, then 100 times; count retained transactions and bytes. Growth must be bounded/linear in the retained edits, not 1, 4, 13, 40… (9 edits currently contain 9,841 nested transactions). Undo/redo across rename, delete, switch, and cell edits must still work. Evidence: `sheets/history.rs`, `sheets-history.log`.

### CODE-08 — Keep plugin writes inside the allowed folder

**P1 · Plugin host · OPEN · Needs experienced security review.** A shortcut folder must not let a plugin write outside its permission boundary.

**Open:** `loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs`, `canonicalize_or_normalize`, write authorization and the actual write operation.

1. Reproduce with an allowed directory containing a symlink to an outside directory and a target file that does not exist yet.
2. Resolve existing parent directories for create targets. Compare the resolved parent with the allowed root.
3. Enforce the boundary when opening/creating the file, using directory handles or equivalent race-resistant platform APIs. Checking a string and writing later is insufficient.
4. Handle link swaps between check and write; fail closed with an actionable permission error.

**Prove it:** Existing-file, new-file, nested-link, traversal, and link-swap attempts cannot create or change any outside file. Normal allowed writes still work. The audit proves the host permission API escape, not a running Wasmtime exploit. Evidence: `plugin-permission.log`, `media-plugins/src/main.rs`.

### CODE-09 — Respect Encode's no-overwrite choice at the final write

**P1 · Encode · OPEN · Needs filesystem review.** Another file may appear while encoding. It still belongs to its owner.

**Open:** `loom-encode/crates/loom-encode-core/src/lib.rs`, `commit_encode_output`.

1. Keep encoding into a temporary file.
2. When overwrite is false, publish with an atomic **create only if absent** operation. An earlier `exists()` check does not solve the race.
3. On collision, preserve the existing destination, report the conflict, and clean up or offer the completed temporary result under a new name.
4. Keep explicit overwrite=true behavior separate and test platform differences.

**Prove it:** Have a controlled encoder create `IMPORTANT_OTHER_FILE` at the destination midway through the job. The job must report a collision and that file's bytes must remain unchanged. Current result replaces it with `NEWENCODE`. Evidence: `encode-overwrite.log`, `media-plugins/src/bin/encode_collision.rs`.

### CODE-10 — Preserve audio precision in Studio projects

**P1 · Studio · OPEN.** Saving the project must not make quiet sounds disappear or lower every sample a little.

**Open:** `loom-studio/crates/loom-studio-core/src/lib.rs`, `save_studio_bundle`, audio asset encoding/decoding.

1. Store native audio assets losslessly at their source/internal precision. Version the package representation and retain old-file loading.
2. Keep PCM16 quantization in explicit export options only. Native Save is not an audio conversion command.
3. Reuse unchanged asset payloads where possible.

**Prove it:** Round-trip samples `0.000001`, `0.75`, negative values, and supported extrema through three native saves. Samples must be bit-exact at the supported internal precision, with unchanged sample rate/channels/frame count. Currently the first becomes zero and 0.75 keeps decreasing. Evidence: `studio-roundtrip.log`, `media-plugins/src/bin/studio_roundtrip.rs`.

### CODE-11 — Recover Sheets images together with their cells

**P1 · Sheets · OPEN.** An image's name is not the image. Recovery needs its actual bytes.

**Open:** Sheets app `src/main.rs`, `record_workbook_snapshot`; Sheets core `src/persistence.rs` object parsing and native package asset code.

1. Encode workbook data and package-owned asset bytes in one recoverable snapshot/package.
2. Restore object references to those recovered bytes; do not depend on the original import path.
3. Preserve deduplication and integrity checks. Make missing/corrupt assets an explicit recoverable error.

**Prove it:** Import a valid PNG, save, remove only the test PNG's original file, edit, crash/recover, then render and export XLSX. The image must remain visible and exportable. The original probe used arbitrary bytes to isolate transport loss, so add the real-image case. Evidence: `sheets-repro.log`.

### CODE-12 — Keep imported shared formulas live

**P1 · Sheets · OPEN.** A displayed number calculated by a formula must keep recalculating after import.

**Open:** `loom-sheets/crates/loom-sheets-core/src/lib.rs`, XLSX formula extraction; move coherent parser work into the existing interop module rather than growing this oversized file.

1. Index each shared formula master by worksheet and shared-formula ID.
2. Translate its formula to each member's relative row/column. Respect absolute `$` references, mixed references, ranges, and quoted sheet names.
3. Preserve a live expression. If a formula cannot be supported, show an explicit import warning and preserve its source; do not silently turn it into an ordinary constant.

**Prove it:** B1 has `A1*2`; B2 is a shared member. After import, change A2 to 50. B2 must become 100, not stay at its cached 30. Add mixed/absolute references and save/reopen. Evidence: `sheets-repro.log`; format reference: Microsoft's Open XML `CellFormula` documentation linked in the original audit.

### CODE-13 — Turn on Writer recovery when opening a file at launch

**P1 · Writer · OPEN.** Opening a document from the command line must not disable its safety net.

**Open:** `loom-writer/crates/loom-writer-app/src/main.rs`, `run_gui_with_dialogs` startup; `loom-core/crates/loom-production/src/snapshot.rs` recovery macro.

1. Initialize the recovery store for every editing session, including `--open`.
2. Separately choose whether to restore an older recovery payload or open the requested document.
3. Surface initialization/write errors. A missing recovery slot must not silently pretend that recording succeeded.

**Prove it:** Launch with a saved test `.loomdoc`, edit a unique sentence, terminate the test app without a save, and recover. The sentence must return. Also test ordinary launch and an invalid launch path. Original evidence is source tracing, not a live forced-crash test. This depends on CODE-01/02.

### CODE-14 — Give every Photo layer a unique ID

**P2 · Photo · OPEN.** Deleting a layer does not make its old number safe to reuse if another layer already has that number.

**Open:** `loom-photo/crates/loom-photo-app/src/main.rs`, pixel and adjustment layer insertion near `layers.len() + 1`; layer identity helpers in the core.

1. Use a persisted monotonic allocator or collision-checked unique ID helper. Apply it to every layer creation path.
2. Keep displayed layer names separate from internal identity. Never replace an existing asset because a new layer got the same ID.

**Prove it:** Add two layers, delete the earlier one, add another, save/reopen, undo/redo. IDs stay unique and each layer keeps its original image. Current result includes `layer-3` twice and save fails. Evidence: `documents-repro.log`.

### CODE-15 — Enforce plugin timeouts while sending input

**P2 · Plugin host · OPEN · Needs concurrency review.** A child that refuses to read must not freeze the host while the host fills its input pipe.

**Open:** `loom-plugin-sdk/crates/loom-plugin-host/src/lib.rs`, `invoke`.

1. Start the deadline before process I/O. Supervise stdin, stdout, stderr, and child completion concurrently.
2. On timeout or I/O failure, close pipes, stop and reap the child, and finish the invocation once. Bound buffered output.
3. Avoid waiting forever on the input writer after killing the process; test child descendants holding pipes too.

**Prove it:** A nonreading child with 1 MiB input and a 50 ms limit returns a timeout within a documented scheduling tolerance (for example 500 ms on the test host), without a leaked child. Also test full stdout/stderr, normal completion, and cancellation. Current controlled adapter takes about 2,014 ms and returns Broken pipe. No real Wasmtime runtime was exercised. Evidence: `media-timeout.log`.

### CODE-16 — Give every Present slide a unique ID

**P2 · Present · OPEN.** Two slides must never share the same identity.

**Open:** `loom-present/crates/loom-present-core/src/lib.rs`, `add_slide` and the existing unique-slide-ID helper used for duplication.

1. Reuse the collision-free allocation strategy for new slides as well as duplicates.
2. Preserve transitions, notes, navigation, and references when deleting/recreating slides.

**Prove it:** Add two slides, delete the earlier added slide, add again. IDs must be unique; select each slide, assign a different transition, save/reopen, and verify each keeps its own state. Current IDs include `slide-3` twice. Evidence: `documents-repro.log`.

### CODE-17 — Move Writer comments with the text they describe

**P2 · Writer · OPEN.** Inserting words before a comment must not attach it to different words.

**Open:** `loom-writer/crates/loom-writer-core/src/lib.rs`, `replace_paragraphs`, comment anchors, text-edit mapping.

1. Describe each edit as removed range plus inserted text. Rebase comment endpoints alongside style runs.
2. Define what happens when commented text is partly or entirely deleted: shrink, collapse, or mark orphaned explicitly; never silently point at unrelated text.
3. Handle paragraph split/merge and valid UTF-8 boundaries. Include anchors in undo/redo and persistence.

**Prove it:** Comment on `world` in `Hello world`, insert `New ` at the start; the anchor must still select `world` (10..15), not `llo w` (6..11). Add emoji, splits, merges, deletion, undo/redo, save/reopen. Evidence: `documents-repro.log`.

### CODE-18 — Do not consume a storage sequence when append fails

**P2 · Shared storage · OPEN.** A failed write is not a completed journal entry.

**Open:** `loom-core/crates/loom-storage/src/lib.rs`, journal `append`.

1. Reserve the next sequence locally. Advance in-memory sequence state only after the entry is durably written.
2. Define and implement partial-write recovery/truncation so a retry cannot append behind a broken tail.
3. Preserve the original error and keep the journal recoverable. Do not waive sequence validation on load.

**Prove it:** Force the first open/write to fail, remove the fixture obstruction, append successfully, reopen. The entry must load at sequence 1. Also inject partial-write and sync failures. Current retry writes sequence 2 and reopen fails. This storage journal is distinct from desktop production recovery. Evidence: `shared-probes/src/lib.rs`, `shared-probes.log`.

### CODE-19 — Undo Present transitions with the rest of the slide

**P2 · Present · OPEN.** Undo must reverse a visible transition change.

**Open:** Present app `src/main.rs`, transition callback; `loom-present/crates/loom-present-core/src/lib.rs`, session history and transition map.

1. Include transitions in the session state saved by undo history. A document-only checkpoint is insufficient.
2. Route transition edits through the same transaction/checkpoint mechanism as other edits.
3. A fresh edit after undo must clear redo. Keep transition state attached to unique slide IDs (CODE-16).

**Prove it:** Set Dissolve, undo to the original transition, redo to Dissolve. Mix this with slide deletion/duplication, then save/reopen. Current isolated transition change leaves `undo()` false. Evidence: `documents-repro.log`.


## source-work-uiux-audit-2026-09-14-prior-truth-md

Original path: `.work/uiux-audit-2026-09-14/prior-TRUTH.md`

# Loom — Current Truth

`AGENTS.MD` defines what Loom must become. This file records what is verified today and which work is currently allowed.

## Current product state

Loom is a local-first Rust + Slint creative-suite **functional alpha** with eight desktop applications, substantial domain engines, persistence/history infrastructure, native project formats, cross-platform packaging machinery, and a large amount of unfinished product/UI code.

It is **not** yet a professional replacement for mature office, image, motion, video, audio, or encoding software. One application (Sheets) passes the acceptance definition in `AGENTS.MD`; Writer has verified evidence toward it; the remaining six do not.

Current complete-suite readiness is approximately **38/100**, raised from 30 for Sheets' verified acceptance: the first application to pass all 14 gate items — real multi-sheet persistence, undoable tab operations, 41-function formula engine, persisted live charts, real zoom, honest templates, keyboard-complete palette, measured performance, and a judge-reviewed 18-capture viewport/theme pass.

The score is intentionally frozen during the UI-foundation reset unless verified user-facing capability materially changes. Repository cleanup, smaller code, better CI, and stronger governance are valuable, but they do not by themselves increase professional product readiness.

## Active gate

```text
ACTIVE PHASE: WRITER
FOUNDATION STATUS: ACCEPTED
ACTIVE APPLICATION: WRITER (IN_PROGRESS)
LOCKED APPLICATIONS: PRESENT, PHOTO, MOTION, VIDEO, STUDIO, ENCODE
```

Loom Sheets is recorded `ACCEPTED` below (verified 2026-09-11), satisfying the user directive that paused Writer until Sheets reached 100% genuine functionality. Per owner directive 2026-09-11, Sheets remains the active workstream for an extended scope — removing every documented limitation in its section — and no Writer implementation work has started. Present, Photo, Motion, Video, Studio, and Encode remain strictly LOCKED.

### Writer acceptance evidence (verified 2026-09-06)

- **Features now real (previously facade controls):** per-document page setup (paper/orientation/margins — persisted, undoable, drives layout, canvas geometry, and export PDF page size), bulleted/numbered list styles (block-kind edits with hanging markers, restart-after-interrupt numbering, markdown/PDF export), anchored comments (block-id + byte-range threads, resolve/reopen/delete, undoable, persisted in `.loomdoc`), and markdown-native tables (block text is the table source of truth; insertion at the caret, verbatim markdown export). All are reachable through the command registry / palette keyboard path.
- **End-to-end journey:** `loom-writer/.work/qa-round8/journey/` — type, select, format, lists, comments, tables, page setup, undo/redo, zoom/scroll, save `.loomdoc`, reopen, export PDF, and cancel/failure paths all assert real state (block kinds, marker projection, comment threads, table parse-back, page-style switch, package round-trip) and PASS.
- **Viewport/theme acceptance:** 18 deterministic captures covering 1024×720, 1280×800, 1440×900, 1920×1200 × light/dark/high-contrast, plus inspector (seeded with comment + table) and template-chooser states, in `loom-writer/.work/acceptance/`. A full judge review passed all 18 with no clipping, no ellipsized action labels, and no contrast defects; three defects it found first (status-bar clipping under the inspector, unseeded headless captures, left-heavy chooser grid) were fixed and re-verified.
- **Native macOS visual QA:** live-GUI window captures via `screencapture -l` across nine states in `loom-writer/.work/qa-native/` (light/dark/high-contrast, inspector light/dark seeded with comments and a table, template chooser, palette).
- **Tests/gates:** 149 tests green (72 app, 77 core), `loom-writer-core` and `loom-writer-app` clippy-clean, code-structure and governance audits PASS (legacy byte ceilings enforced).

### Remaining Writer gate blockers

1. **Human visual acceptance** (section 5): the judge-reviewed captures above are agent-reviewed evidence; `ACCEPTED` additionally requires explicit human sign-off of the represented design.
2. **Representative performance evidence** (section 13 item 10): no measured interaction/pagination budget runs exist yet.
3. **Documented limitations** (not severity-1/2): table cells are edited as markdown text rather than a pointer-driven cell grid; commented ranges are listed in the inspector but not yet visually highlighted in the canvas.

## Why the reset is necessary

The repository has accumulated useful engines together with excessive agent-generated structure and UI duplication. Several application entrypoints and core modules are extremely large; generic controls exist both in shared and application-local Slint files; old plans and reports compete for agent attention; and CI has been spending substantial compute on all-workspace release builds and cross-platform packaging even when the active work is a shared UI edit.

The visible result is below the desired product bar. Current captures have demonstrated clipping, weak hierarchy, dense or redundant chrome, dead space, inconsistent control grammar, fixed-size workspaces, and prototype-like composition. Passing deterministic screenshot tests does not make those designs acceptable.

## Strict readiness scorecard

| Dimension | Current | Current truth |
|---|---:|---|
| Core/backend engineering | 67/100 | Sheets formula engine (41 functions, lazy IF, error propagation, absolute refs), chart geometry, and workbook persistence added to the existing models. |
| Architecture & persistence | 72/100 | Versioned multi-tab `.loomtable` format with back-compat, workbook undo states, crash-recovery of all tabs; new code lives in small modules (`functions`, `persistence`, `style`, `formatting`) while legacy cores stay under byte ceilings. |
| Functionality reachable through GUI | 52/100 | Sheets' full daily workflow (cells, ranges, 41-function formulas, formatting, charts, zoom, sort, tabs, templates, save/reopen, CSV/XLSX export) is wired end to end with journey evidence; Writer's document workflow slice from 2026-09-06 stands; other apps unchanged. |
| Interaction design | 44/100 | Sheets: every mutation undoable (incl. tab add/delete/rename, chart ops, and anchored-object move/resize), keyboard/palette reachability for all primary commands, truthful enablement and status announcements, responsive toolbar/inspector breakpoints, viewport-filling grid, and Esc hierarchy. |
| Visual design & polish | 40/100 | Sheets passed a judge-reviewed 18-capture viewport/theme acceptance pass (no clipping, no ellipsized labels, no contrast defects; two defects found and fixed: stale zoom label, fixed-size grid void); Writer's earlier pass stands; other applications' UI remains frozen legacy reference. |
| Professional workflow depth | 46/100 | Sheets now covers cross-sheet formulas, dynamic-array spills, formula-backed pivot summaries, basic cell styling, and anchored shape/image objects in a coherent daily spreadsheet workflow. External office-format feature parity and richer direct manipulation remain limited. Other apps unchanged. |
| **Overall product readiness** | **38/100** | Functional alpha with the first accepted application; professional-suite parity is not established. |

The overall score is not an arithmetic mean. User-visible workflow completion, reliability, and acceptance evidence dominate.

## Shared UI foundation status

Status: `ACCEPTED`

The shared UI foundation (`loom-core/crates/loom-ui/ui/foundation.slint`) is accepted with approved baselines and full mechanical CI passing. Consumer imports are now unlocked for the active application (Sheets). Approved screenshot baselines are recorded under `loom-core/crates/loom-ui/baselines/foundation`.

## Legacy UI status

`loom-core/crates/loom-ui/ui/toolkit.slint` and existing application-local component files are compatibility code for the current applications. They are **not** the design source for the new foundation.

They remain in the tree only to avoid breaking existing application builds during the reset. New applications/components must not copy from them. They will be removed or reduced as each application migrates after foundation acceptance.

## Code-structure debt

The following classes of debt are verified and must be reduced by the code-structure ratchet:

- monolithic application `main.rs` files;
- monolithic application-core `lib.rs` files;
- oversized legacy shared Slint files;
- application-local generic component libraries;
- compatibility aliases and forwarding wrappers that no longer add semantics;
- QA scripts that combine governance, source heuristics, visual auditing, and product scoring in one large program.

Existing oversized files are legacy exceptions with fixed byte ceilings. They may not grow. New source files must obey the smaller general budgets in `loom-bootstrap/contracts/code-quality.toml`.

## CI truth

Routine CI is being reduced to high-signal checks appropriate for the active phase: governance, structure, asset provenance, shared UI compilation, format/Clippy, focused tests, and deterministic foundation capture.

The full cross-platform application/package matrix remains useful release evidence, but it is not a routine PR gate during the UI foundation lock.

Source inspection, callback counts, control counts, screenshot existence, or a generated numeric "readiness" score are not product evidence and must not be used to raise this file's score.

## Asset/licensing truth

No third-party visual asset should enter the product without explicit provenance and a license compatible with commercial redistribution.

Current policy prefers original Loom-generated assets, CC0/public-domain material, SIL OFL fonts, and clearly permissive licenses whose terms are satisfied. Unknown, personal-use, non-commercial, editorial-only, or scraped assets are forbidden.

Existing product screenshots and test fixtures are project-generated evidence/fixtures rather than shipped third-party artwork. New external assets must be registered in the asset manifest before use.

## Serial application gate

After the shared foundation becomes `ACCEPTED`, application migration proceeds only in this order:

| Order | Application | Status |
|---:|---|---|
| 1 | Sheets | ACCEPTED |
| 2 | Writer | IN_PROGRESS |
| 3 | Present | LOCKED |
| 4 | Photo | LOCKED |
| 5 | Motion | LOCKED |
| 6 | Video | LOCKED |
| 7 | Studio | LOCKED |
| 8 | Encode | LOCKED |

A later application remains locked until the immediately preceding application is explicitly recorded `ACCEPTED` here.

## Current application boundaries

### Sheets

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13; extended Sheets scope verified 2026-09-13)

Verified capabilities (each gate item in parentheses):
- Shared-foundation adoption, no app-local generic forks (§13.1): zero `toolkit.slint` imports, zero app-local `Loom*` controls; 100% token discipline; native palette and AppKit/DBus menu-bar reflection with live enablement.
- Daily workflow end to end through the GUI (§13.3): cell selection, ranges, Shift/Arrow navigation, Select All, formula-bar input/cancel/commit, Fill Down, Copy/Cut/Paste (cells and matrices), Delete/Backspace clearing — all keyboard-reachable.
- Selection/direct manipulation (§13.4): anchor/focus ranges, marquee, fill handle, floating live chart overlay, anchored-object selection with pointer move/resize handles, and Esc hierarchy (palette/template/overflow/chart/edit).
- Undo/redo and persistence (§13.5): every mutation undoable — cells, styles, alignments, decimals, sort, freeze, row/column sizing, chart insert/kind, anchored shapes/images, object move/resize, tab add/delete/rename (workbook-level transactions with per-tab history discipline); `.loomtable` persists all tabs, active index, styles, alignments, freeze panes, chart specs, anchored object metadata, and package-owned image assets with legacy single-sheet back-compat; crash recovery restores the full workbook; each history stack is bounded at 200 entries.
- Native open/save/export (§13.6): `.loomtable` open/save/save-as, formula-preserving CSV import/export with delimiter sniffing, and multi-sheet XLSX import/export preserving worksheet names, formulas, cached values, cell styles/alignments, basic charts, shapes, and embedded images; all cancellations/dialog failures report truthfully (§13.7).
- Keyboard-only primary workflow (§13.8): full shortcut map (Ctrl+N/O/S/E/Z/C/X/V/A/B/I/U/K, Ctrl+=/-/0 zoom, arrows/Tab/Del/Esc) plus a 49-command palette covering every primary command incl. decimals, sort-by-column, fill, chart ops, pivot summaries, and object insertion.
- Accessibility (§13.9): table role with polite live-region announcements, per-cell accessible labels/values, labeled icon-only controls with tooltips, managed focus (grid/palette/template/overflow), status confirmations for toggles.
- Performance (§13.10): 10k-cell chained-formula sheet evaluates in ~167 ms (debug, Apple Silicon), 30 KB workbook serializes in ~1 ms and parses in ~2 ms; projection renders the visible window only.
- Interop (§13.11): formula-preserving CSV round-trip with delimiter sniffing and RFC 4180 multiline quotes; multi-sheet XLSX import/export validity (ZIP magic, shared strings, worksheet names, formulas, cached values, cell styles/alignments, basic charts, shapes, and embedded images); package-owned `.loomtable` image assets; and legacy `.loomtable` load path — each with tests.
- Evidence (§13.12): 191 tests green (93 app, 80 core lib, 13 formula integration, 1 performance, 4 sheet-range integration); keyboard + sparse + two-tab-save/reopen headless journeys PASS; 18 judge-reviewed captures (4 viewports × 3 themes + chooser/palette/chart/zoom states) plus native Linux X11 window captures in `loom-sheets/docs/`, including the anchored-object state; 4 audits PASS; Clippy `-D warnings` clean; `cargo fmt --check` clean; byte ceilings hold (`loom-sheets-core/src/lib.rs` 198,356 < 199,529; rich XLSX module 57,278 < 65,536; app `main.rs` 105,336 < 108,783; `actions.rs` 65,443 < 65,536).
- Visual acceptance (§13.2): judge-reviewed pass over all 18 deterministic captures — no overlap, no clipped/ellipsized action labels, no toolbar wrapping, truthful disabled states, visible grid fills every viewport, light/dark/high-contrast coherent; native Linux screenshots independently inspected from the live X11 window.
- Defects (§13.13): no known severity-1/2 defect in the accepted workflow.
- Formula engine depth: 41 functions (arithmetic, comparison, SUM/AVERAGE/COUNT/COUNTA/MIN/MAX/IF(lazy)/AND/OR/NOT, ROUND/ABS/SQRT/POWER/MOD/FLOOR/CEILING/MEDIAN, CONCAT/CONCATENATE/TEXTJOIN, VLOOKUP/HLOOKUP/INDEX/MATCH, LEFT/RIGHT/MID/LEN/UPPER/LOWER/TRIM, SUMIF/COUNTIF/AVERAGEIF with criteria + wildcards, PMT/FV/PV, TODAY/NOW, IFERROR), absolute `$` refs, preserved error codes, cycle detection, cross-sheet references/ranges, and dynamic-array spill/error handling.
- Facade removal: Category/Pivot/Shape/Media/Note placebo controls, non-undoable pivot summary, label-only zoom, and dead template cards all replaced with real semantics or removed; eleven template cards each create their advertised sheet with live formulas; shapes and image attachments render as anchored worksheet objects, support pointer move/resize, and persist package-owned image payloads.

Documented interoperability boundaries (not severity-1/2): XLSX PivotTable caches import as their visible cached worksheet cells rather than as a native refreshable pivot object; unsupported vendor-specific OOXML extensions are ignored; and the supported chart model is single-series. These boundaries are explicit and do not block the accepted Sheets workflow.

### Writer

Status: `IN_PROGRESS` (unlocked 2026-09-11 after Sheets acceptance; no implementation work started yet — evidence below is preserved from the 2026-09-06 pass)

Verified capabilities:
- Shared UI foundation adopted: zero legacy `toolkit.slint` imports; 100% token discipline, native palette & AppKit menu bar reflection.
- UI debt reduction: `app.slint` reduced to 15,269 bytes (from 36,518 bytes); `writer_components.slint` reduced to 12,785 bytes (from 32,542 bytes); `main.rs` reduced to 141,585 bytes (from 194,401 bytes).
- Full document model & multi-page layout: RichBlock structure with character style runs, paragraph styles, headings H1-H6, multi-page layout engine with zoom and scroll projection.
- Text selection & grapheme-safe navigation: Collapsed caret, range selection, UTF-8 and extended grapheme boundary clamping, word boundary detection.
- Undo/redo isolation & coalescing: Typing coalescence within time windows, discrete formatting actions creating undoable snapshots, memory-bounded history cache.
- Native persistence & export: Native `.loomdoc` package format saving and loading with integrity verification, Markdown export, deterministic PDF export.
- Document metrics & table of contents: Word count, character count, sentence count, reading time estimation, hierarchical outline/TOC generation from headings.
- Format Inspector: Dedicated side panel with Style/Layout/More tabs, paragraph style selector, character style toggles (Bold/Italic/Underline), alignment, and live document statistics.
- Template Chooser: Modal sheet with category filtering (All Templates, Basic, Letters, Curricula Vitae) and deterministic template initialization.
- Native macOS Global Menu Bar: AppKit reflection for File, Edit, View, and Format menus with live command state synchronization.
- Test and audit verification:
  - 125 unit/integration tests passing (64 in `loom-writer-app`, 61 in `loom-writer-core`).
  - 4 automated bootstrap audits passing (governance, code structure, asset provenance, UI foundation).
  - 0 warnings with Clippy (`-D warnings`).
  - Native screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200), themes (light, dark), and states (default, inspector, template chooser).

### Present

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13)

Verified capabilities:
- Shared UI foundation adopted: zero legacy `toolkit.slint` imports; 100% token discipline, native palette & AppKit menu bar reflection.
- Complete removal of placebo and fake controls: clean toolbar (`LoomToolbar`, `LoomIconButton`, `LoomOverflowButton`) and inspector (`LoomPanel`, `LoomSegmentedControl`, `LoomSectionHeader`, `LoomButton`).
- Native macOS AppKit `NSMenu` and Linux DBusMenu reflection (`MenuBarService`) with command projection (`file.new`, `file.open`, `file.save`, `file.export_pdf`, `edit.undo`, `edit.redo`, `slide.new`, `slide.duplicate`, `slide.delete`, `slide.prev`, `slide.next`, `view.inspector`).
- Dynamic menu enablement synchronization matching document state, history, selection, and viewport constraints.
- Deep audit test suite passing (scene graph, shapes, selection, marquee, snapping, notes, undo/redo, persistence, PPTX/PDF export, macOS AppKit menu bar reflection).
- File byte size ceilings enforced and reduced:
  - `main.rs`: 95,635 bytes (ceiling 123,901 bytes; reduced by 28,266 bytes).
  - `app.slint`: 20,432 bytes (ceiling 42,883 bytes; reduced by 22,451 bytes).
  - `present_components.slint`: 23,544 bytes (ceiling 34,385 bytes; reduced by 10,841 bytes).
  - `desktop_tests.rs`: 25,607 bytes (under 65,536 limit).
  - `audit_tests.rs`: 10,009 bytes (under 65,536 limit).
  - `theme_chooser.slint`: 20,491 bytes (under 32,768 limit).
  - `inspector.slint`: 7,521 bytes (under 32,768 limit).
  - `toolbar.slint`: 3,248 bytes (under 32,768 limit).
- Test and audit verification:
  - 78 unit/integration tests passing (29 in `loom-present-app`, 49 in `loom-present-core`).
  - 4 automated bootstrap audits passing (governance, code structure, asset provenance, UI foundation).
  - 0 warnings with Clippy (`-D warnings`).
  - Native screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200), themes (light, dark), and states (default, theme chooser, command palette).

### Photo

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13)

Verified capabilities:
- Shared UI foundation adopted: zero legacy `toolkit.slint` imports; 100% token discipline, native palette & AppKit menu bar reflection.
- Complete removal of placebo and fake controls: clean toolbar (`PhotoActionToolbar`), canvas with subtle drop shadows (`PhotoCanvas`), right format inspector (`PhotoInspector`), status bar (`PhotoStatusBar`), and command palette (`CommandPalette`).
- Native macOS AppKit `NSMenu` and Linux DBusMenu reflection (`MenuBarService`) with command projection (`file.new`, `file.open`, `file.save`, `file.export_png`, `file.export_jpeg`, `edit.undo`, `edit.redo`, `layer.new_pixel`, `layer.new_adjustment`, `layer.delete`, `layer.move_up`, `layer.move_down`, `view.inspector`, `view.zoom_in`, `view.zoom_out`).
- Layer stack lifecycle & reordering: pixel layers, adjustment layers, visibility toggling, layer selection, move up/down, delete.
- Compositing & blend modes: Normal, Multiply, Screen, Overlay blend modes, per-layer opacity adjustments (0-100%).
- Color adjustments: live brightness, contrast, and saturation adjustments on dedicated adjustment layers.
- Affine transforms & cropping: position X/Y nudging, scale X/Y, rotation (-180° to 180°), document bounds calculation, canvas cropping to selection, layer cropping to selection.
- Raster payloads, persistence, and export: native `.loomphoto` project saving and loading, deterministic PNG export, JPEG export, and OpenRaster stack manifest emission.
- File byte size ceilings enforced and ratcheted down:
  - `main.rs`: 82,220 bytes (legacy debt ceiling reduced from 111,647 to 82,500 bytes; reduced by 29,427 bytes).
  - `desktop_tests.rs`: 26,005 bytes (< 65,536 limit).
  - `audit_tests.rs`: 7,860 bytes (< 65,536 limit).
  - `app.slint`: 18,380 bytes (< 32,768 limit).
  - `inspector.slint`: 21,491 bytes (< 32,768 limit).
  - `photo_components.slint`: 10,101 bytes (< 32,768 limit).
  - `toolbar.slint`: 3,171 bytes (< 32,768 limit).
- Test and audit verification:
  - 74 unit/integration tests passing (25 in `loom-photo-app`, 49 in `loom-photo-core`).
  - 4 automated bootstrap audits passing (governance, code structure, asset provenance, UI foundation).
  - 0 warnings with Clippy (`-D warnings`).
  - Native macOS screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200), themes (light, dark), and command palette.

### Motion

Foundation strengths include layer/keyframe models, interpolation, transforms, timing/playback helpers, procedural motion utilities, render-queue primitives, persistence/history, and a composition shell.

Current product limitation: professional scene manipulation, graph/timeline editing, compositing, effects, playback/render workflows, and interchange remain incomplete.

### Video

Foundation strengths include timeline/track/clip models, trim/marker helpers, local processing pieces, captions/audio/media helpers, persistence/history, and a timeline shell.

Current product limitation: scalable timeline interaction, trimming/direct manipulation, media/source consistency, source/viewer workflows, effects/color/audio depth, export UX, and professional NLE behavior remain incomplete.

### Studio

Foundation strengths include tracks/regions, PCM/WAV support, synthesis/DSP helpers, mixer/automation primitives, persistence/history, local device foundations, and a multitrack shell.

Current product limitation: production recording, low-latency realtime scheduling, editing/comping, time/pitch workflows, plugin hosting/isolation/UI, mixing/mastering depth, and scalable arrangement interaction remain incomplete.

### Encode

Foundation strengths include FFmpeg queue/preset planning, command execution/progress/cancellation, persistence/recovery, probe/conformance helpers, hardware-codec planning, and batch/destination primitives.

Current product limitation: queue/settings hierarchy, watch-folder experience, hardware policy, pause/resume guarantees, exhaustive format support, and perceptual conformance remain incomplete.

## Evidence rules

A capability receives full product credit only when the normal GUI exposes real semantics, editing is selection/context aware, undo/persistence are correct where applicable, failures are truthful and recoverable, realistic user content completes the workflow, UI passes the mechanical contract, format claims have proportional evidence, and the claimed platforms have been validated.

Core-only functionality earns foundation credit, not parity credit.

Do not mark this project `complete`, `production`, `100%`, or equivalent while any active acceptance gate remains blocked.


## source-work-uiux-audit-2026-09-14-ui-repair-cards-md

Original path: `.work/uiux-audit-2026-09-14/ui-repair-cards.md`

## UI/UX repair cards — fresh 2026-09-14 evidence

All cards are OPEN. A screenshot proves only the visible state. Native observations are marked separately. Evidence paths below are relative to `.work/uiux-audit-2026-09-14/`. The full report embeds each accepted screenshot with its flow step. A cut-off control inside an otherwise complete app capture is a product finding, not an accidentally cropped evidence image.

### UI-01 — Give Linux users a visible route to basic file commands

**P1 · Sheets first; check each app when active · OPEN.** In the isolated Linux desktop with no global menu host, the window has no visible New/Open/Save menu or palette button. At 1280 px, overflow offers only Export CSV. A new user has to know Ctrl+K before discovering the commands.

**Evidence:** `sheets/09-native-start.png`, `14-native-after-new.png`, `16-native-palette.png`. Ctrl+K successfully opens a readable palette; retain that working shortcut. This observation is specific to the captured Linux environment, not a claim about every desktop menu host.

**Open:** `loom-sheets/crates/loom-sheets-app/ui/app.slint`, `SheetActionToolbar` in `ui/components.slint`, app command registry/palette, and menu-host detection in `loom-core/crates/loom-desktop/`.

1. Detect whether the desktop actually hosts the native/global menu. Publishing DBus menu data alone is not proof that users can see it.
2. When no host exists, render a shared local menu or clearly named menu button. Include New Workbook, Open, Save, Save As, Export, and Search Commands. Route every entry to the existing typed command and its enablement state.
3. Keep this entry visible at 1024, 1280, and 1440 px. Put secondary items inside the menu; do not squeeze more unlabeled icons into the toolbar.

**Done when:** A person using only the pointer can create, open, and save a test workbook without knowing a shortcut; a keyboard user can reach the same menu and return focus to the sheet. Check a Linux desktop without a global menu and one platform with native menu hosting. CODE-04 must protect the transitions.

### UI-02 — Stop opening a mostly empty Sheets inspector by default

**P2 · Sheets · OPEN.** The startup inspector consumes 320 px for a few properties and a large empty panel. At the reference 1280 px width, only 960 px remain for the central area (75%, before inner padding). The contract requires at least 78% and a closed inspector by default.

**Evidence:** `sheets/01-start-light-1440.png`, `02-compact-light-1024.png`, `09-native-start.png`. Compact mode already gives the sheet more room; keep that strength.

**Open:** Sheets `ui/app.slint` (`show-inspector`), `src/main.rs` (`INSPECTOR_WIDTH`, breakpoint handling); `loom-design-bible/contracts/desktop-ui.toml` `[app.sheets]`.

1. Start with the inspector closed, matching `right-inspector-default = 0`.
2. Keep a labeled Format toggle; opening it must preserve selection and show properties for that selection.
3. Respect the user's explicit open/closed choice across resizing. Use shared width tokens instead of an app-only 320 px constant.

**Done when:** A fresh 1280×800 window gives the central work surface at least 78% of available width. Toggle Format, resize below and above the compact breakpoint, and verify useful controls remain reachable without reopening a panel the user closed. Test 1024×720 and 1440×900 too.

### UI-03 — Reflow the template chooser and show truthful Recents

**P2 · Sheets · OPEN.** At 1024×720 the rightmost template previews and names are cut off. The chooser also repeats the selected Blank item under Recents and Basic; the Recents section is hard-coded rather than driven by real recent choices.

**Evidence:** `sheets/04-template-chooser.png`; source uses fixed 156 px cards in long horizontal rows. Cancel and Create remain visible — preserve that.

**Open:** `loom-sheets/crates/loom-sheets-app/ui/template_chooser.slint`, `SheetsTemplateCard`, category rows and Recents; template selection callbacks in `src/main.rs`.

1. Compute how many full cards fit in the content area after the category sidebar and padding. Move excess cards to another row; allow vertical scrolling of this content region.
2. Keep the entire template name and preview visible. Do not shrink the text or make the whole modal horizontally scroll.
3. Show Recents only from actual stored template choices. If there are none, omit that section or say `No recent templates`.
4. Keep keyboard selection visible as it moves; selection must refer to the intended template after reflow or category filtering.

**Done when:** At 1024×720 and 1.25/1.5 text scale, every template can be reached, its full name is readable, and Create produces the selected item. Escape/Cancel preserves the current workbook. No right-edge card is sliced in half.

### UI-04 — Use honest starter data and a clear empty-document path

**P2 · Sheets · OPEN.** Startup looks like a user's Budget but contains demo values. Rent/Transport say monthly, Food says weekly, yet Total simply adds all three. No common period or currency is stated.

**Evidence:** `sheets/01-start-light-1440.png`, `09-native-start.png`; `starter_workbook` assigns the mixed period labels.

**Open:** `loom-sheets/crates/loom-sheets-app/src/main.rs`, `starter_workbook`, startup selection, and template data.

1. Make a new document blank, with an obvious path to templates/examples. If opening an example is desired, name it `Example Budget` and make that choice explicit.
2. Give the example a single stated period and unit. Either all amounts are monthly or convert them with a visible formula and explanation before totaling.
3. Keep the sample's totals as real formulas. Never mark example data as a saved user document or silently overwrite user recovery with it.

**Done when:** First launch and New have clear, distinct blank/example behavior. Every Budget amount uses the same stated unit and time period, and changing an amount recalculates its total. Add a small test of example values/formulas, not a test that merely searches for the word Budget.

### UI-05 — Make Sheets header cells readable in high contrast

**P1 · Sheets accessibility · OPEN.** High-contrast mode hides the actual header text `Item`, `Amount`, and `Note`: the header background and text are both black. A1's formula field still says Item, confirming the text exists.

**Evidence:** `sheets/08-high-contrast.png` compared with `02-compact-light-1024.png`. Source uses `paper-line` for header fill and `paper-ink` for text; both are black in the high-contrast palette.

**Open:** Sheets `ui/components.slint`, cell background/text in `SheetGridSurface`; shared `loom-core/crates/loom-ui/ui/theme.slint`; corresponding design tokens/contracts.

1. Use a foreground/background pair for header cells. Do not use a line/separator color as a text background without its paired foreground.
2. Preserve the distinct active-cell border and readable user-specified fills. Check headers, selected ranges, and ordinary cells separately.
3. If a new semantic token is necessary, define it centrally and demonstrate it in the gallery; do not add an isolated color literal.

**Done when:** Item/Amount/Note are readable at 1024×720 and 1440×900 in all three themes, with and without selection/fills. Measure the contract's required contrast for each pair and inspect the actual screenshot. This repairs one visible accessibility defect; it is not full accessibility certification.

### UI-06 — Render the status and errors the controller produces

**P1 · Sheets trust/feedback · OPEN.** The window has no visible status bar or saved/unsaved indicator. Source declares and updates status strings, imports status components, but never instantiates those components. Users cannot rely on messages that only exist in memory.

**Evidence:** `sheets/09-native-start.png`, `11-native-commit.png`, `15-native-undo-after-new.png`; source check of `status-left`, `status-right`, and the complete `ui/app.slint` layout. Native title remains `Untitled` after the test edit. Save/error dialogs were not exhaustively exercised in this visual run.

**Open:** `loom-sheets/crates/loom-sheets-app/ui/app.slint`, status properties and layout; status-setting paths and document dirty state in `src/main.rs`; shared `LoomStatusBar`/`LoomStatusText`.

1. Render the existing status properties in the shared status bar using the contract height, outside the grid.
2. Show a clear unsaved state tied to actual document dirtiness. Clear it only after a successful save, not after merely opening a save chooser.
3. Show actionable error messages persistently enough to read; do not bury a failed save in an invisible property. Announce relevant messages through the existing accessibility mechanism without repeating every frame.

**Done when:** Edit, save, cancel save, force a write failure in a test folder, and trigger an invalid formula. Visible state must distinguish these outcomes. The grid must remain usable at 1024×720. A screen reader must receive the meaningful error once; record that check separately from screenshots.

### UI-07 — Clear edited fields when their document value changes

**P1 · Shared text input, reproduced in Sheets · OPEN.** After typing `audit123` and creating a new blank workbook, the formula field still paints that old text over its empty-field placeholder. The grid is blank. This makes the visible value disagree with the current document.

**Evidence:** `sheets/11-native-commit.png`, `15-native-undo-after-new.png`, `16-native-palette.png`. Reproduce with a freshly built native app before implementation; the first native captures used the existing local executable. Source points to the shared input's one-way `text: root.value` binding and its independently drawn placeholder.

**Open:** `loom-core/crates/loom-ui/ui/foundation/controls.slint`, `LoomTextField` (also check `LoomSearchField`); Sheets `FormulaNameBar` and buffer reset paths.

1. Reproduce typing, then programmatically replacing the field's value with a different string and with empty text.
2. Make the editable text and public value stay synchronized, using Slint's appropriate two-way binding or explicit update path. The placeholder must depend on the actual displayed text being empty.
3. Ensure New/Open, selection changes, commit, and cancel reset the buffer consistently without writing stale text into the new document.

**Done when:** Type `audit123`, commit, start a blank document, then select another cell. No old text remains; exactly one placeholder appears in an empty field. Repeat using Undo and switching tabs. Verify the shared gallery case and native Sheets; a headless initial-state screenshot cannot expose this defect.

### UI-08 — Make chart ranges and comparisons understandable

**P2 · Sheets · OPEN.** The default Budget chart compares Rent, Food, Transport, Total, and Average as if they were five peer categories. It provides no visible range/unit explanation. This turns summary values into misleading bars.

**Evidence:** `sheets/05-chart.png`; `src/analysis.rs` `plan_chart` currently collects populated rows for the selected columns.

**Open:** `loom-sheets/crates/loom-sheets-app/src/analysis.rs`, chart command/controller, `ui/chart.slint`, persisted chart specification.

1. Build a chart from an explicit selected range, with clear category/value columns. Preserve that range in the model.
2. Show/edit the range and series label in chart properties. For the starter example, select only the actual expense rows; do not silently guess that any formula row must be excluded in arbitrary user data.
3. Include a clear unit/period when the data supplies one. Keep charts live when source cells change.

**Done when:** Select A1:B4 in the example and create a chart: exactly Rent/Food/Transport appear. Show that source range. Changing Food changes the chart, Undo reverses the change, and Save/Open preserves the range. A deliberately selected Total row must still be chartable as an explicit user choice.

### UI-09 — Distinguish a workbook from one sheet in its commands

**P1 · Sheets · OPEN.** The palette says `New Sheet`, but the command replaces the entire workbook and clears its tabs/history. This label suggests a much smaller action than it performs.

**Evidence:** `sheets/16-native-palette.png`, the New callback in CODE-04, and the native blank workbook after New.

**Open:** Sheets command labels in `src/main.rs`/`src/palette.rs`, menus, tab-add control, and user-facing status strings.

1. Use `New Workbook`, `Open Workbook`, `Save Workbook`, and `Save Workbook As` for whole-file actions.
2. Use `Add Sheet`, `Rename Sheet`, and `Delete Sheet` for operations on a tab inside that file.
3. Update labels, tooltips, accessible names, and palette search synonyms together; retain the established shortcuts.

**Done when:** A workbook with two tabs remains intact when Add Sheet is chosen and gains one tab. New Workbook invokes CODE-04's dirty-work decision and replaces the whole file only after consent. Every visible label accurately names its scope.

### UI-10 — Remove the contradictory permission to truncate action captions

**P2 · Shared design contract · OPEN · Source-confirmed design debt.** `AGENTS.MD` requires readable complete action labels, but `desktop-ui.toml` permits 10 px ellipsized toolbar captions. A small model can obey one rule and violate the other.

**Open:** `loom-design-bible/contracts/desktop-ui.toml`, `[component.labeled-toolbar-item]`, `[component.icon-over-label-toolbar-item]`, toolbar/ellipsis clauses; shared toolbar components and their contract audits.

1. Make the contract agree with the readable-action rule: full captions or an accessible icon-only action with a tooltip and a reachable fully labeled menu item.
2. Set one canonical minimum caption size from the shared typography scale. Do not solve a tight toolbar by shrinking captions to 10 px.
3. Update component measurement/overflow behavior and contract checks together. Preserve optional ellipsis for user content names where explicitly allowed.

**Done when:** Long translated action labels and 1.25/1.5 text scale remain operable at the boundary widths 1179/1180, 1279/1280, and 1319/1320. The contract and UI tests reject clipped action captions. This is a source-confirmed rule conflict, not a claim that every captured toolbar currently clips.


## source-loom-bootstrap-bootstrap-md

Original path: `loom-bootstrap/BOOTSTRAP.md`

# BOOTSTRAP.md — full bootstrap procedure

This document describes how to build, test, visually verify, package, and
deliver the complete Loom suite from `loom-bootstrap`.

## 0. Toolchain requirements

| Tool | Version | Required | Purpose |
|------|---------|----------|---------|
| `rustup` + stable rustc/cargo | >= 1.80 (MSRV) | yes | all cargo workspaces |
| `git` | any | yes | VCS (dev only; excluded from packages) |
| `just` | any recent | optional | nicer task runner over scripts/ |
| `docker` + compose plugin | any recent | optional | visual QA + offline containers |
| `zip`, `unzip` | any | packaging + verification | |
| python3 + PIL | any | contract-aligned RGBA visual comparison | |
| `timeout` (coreutils) | any | recommended | smoke/screenshot timeouts (fallback built in) |

Verify with:

```sh
bash scripts/env-check.sh
```

It checks rustc/cargo against MSRV 1.80 and reports optional tools as WARN
(docker, just, zip, unzip, timeout). It exits 1 only if the toolchain is
missing or below MSRV.

## 1. Layout

```
<suite-root>/                    (parent of loom-bootstrap)
├── loom-bootstrap/              this repo — orchestration
├── loom-core/                   shared crates (crates/loom-*)
├── loom-writer/ ... loom-encode/  applications
├── loom-vision/  loom-plugin-sdk/ loom-design-bible/ loom-spec/ loom-samples/
```

Repos are path-pinned siblings during development: app workspaces reference
`../loom-core/crates/*` via relative paths. `COMPATIBILITY.toml` records
`rev = "local"` for every repo; before release, revs are pinned to tags and
the manifest updated.

## 2. Build

```sh
bash scripts/build-all.sh            # cargo build --release, per existing repo
bash scripts/build-all.sh --debug    # debug profile
```

Repos without `Cargo.toml` are reported as `SKIP` (not a failure). Logs:
`.work/build-<repo>.log`. Exit 1 if any repo that *is* a workspace fails.

## 3. Test

```sh
bash scripts/fmt-all.sh      # cargo fmt --check per repo
bash scripts/clippy-all.sh   # clippy -D warnings; fails only on Loom crates
bash scripts/test-all.sh     # cargo test --workspace per repo, aggregate PASS/FAIL
```

`clippy-all.sh` reports diagnostics in third-party code without failing;
warnings/errors in Loom's own crates fail the run.

## 4. Visual QA

Prerequisites: app binaries built (release or debug). The harness captures the
default light and dark screens for each app. It compares when a committed
baseline exists, reports missing baselines as incomplete, and never writes a
baseline automatically. This is only the default light/dark slice; it does
not run the design-bible high-contrast, text-scale, reduced-motion, locale,
component/state, or error-state matrix.

```sh
bash scripts/visual-qa-all.sh
```

Per app: runs `<bin> --screenshot <work>/screenshots/<app>-light.png
--size 1280x800`, attempts the dark variant with `--theme dark`, then
compares each available screenshot against its baseline with
`scripts/img-compare.sh` (Python 3 + PIL, RGBA mean absolute error < 1.0 and
one-pixel-eroded differing-pixel ratio < 0.01). ImageMagick alone is not a
valid fallback because it cannot evaluate both contract gates. Result table →
`../visual-qa-report.txt`. Exit 1 for a diff, missing baseline, missing binary,
screenshot failure, or size mismatch; exit 2 when comparison tooling or input
is unavailable. Override the size with `--size WIDTHxHEIGHT`.

Inside Docker:

```sh
bash scripts/docker-build.sh
bash scripts/docker-visual-qa.sh     # xvfb-run inside the visual service
```

## 5. Offline test

Mode A (host, no network): unset proxy variables, run the test suite with
`cargo --offline`:

```sh
bash scripts/offline-test.sh
```

Mode B (docker, hard network isolation):

```sh
bash scripts/offline-test.sh --mode-b   # docker run --network none
# or
bash scripts/docker-offline-test.sh     # compose 'offline' service (network_mode: none)
```

Note: `--offline` can only resolve dependencies that are already in the local
cargo registry or vendored. Repos without a `Cargo.lock` are reported
explicitly. A core workflow failing without network is a release-blocking
defect.

## 6. Cleanup

Inspect generated output before cleanup, especially when preserving visual
evidence:

```sh
bash scripts/cleanup-targets.sh --dry-run
bash scripts/cleanup-targets.sh
```

The allowlist covers sibling Cargo `target/` directories, the documented
plugin fixture target, and package-verification temporary paths. Logs,
screenshots, reports, historical run directories, and arbitrary
`CARGO_TARGET_DIR` paths are preserved. Pass `--visual-diffs` only when the
current diff images are no longer needed.

## 7. Smoke launch

```sh
bash scripts/run-apps.sh   # each existing binary runs --smoke for 5s
```

A binary that exits cleanly or stays alive for the full 5 seconds counts as
launched. No binary → reported as missing, not a failure.

## 8. Packaging (ZIP delivery)

```sh
bash scripts/package.sh
```

Creates `../Loom-Complete.zip` from the suite root:

- includes every `loom-*` repo,
- excludes `target/`, `.git/`, `.DS_Store`, `.work/`, `__pycache__`,
- excludes symbolic links deliberately; `find -P ... -type f` never follows
  untrusted link targets,
- deterministic ordering: sorted regular-file list piped to `zip -X -q -@`,
- writes `../Loom-Complete.zip.sha256`.

Checksum generation is fatal. A previous zip and checksum sidecar are removed
before packaging so a failed run cannot leave a stale `.sha256` paired with a
new or partial archive.

Limitation: zip embeds file mtimes, so two runs at different times are not
byte-identical even with identical content; ordering is deterministic.

## 9. Verification of extraction

```sh
bash scripts/verify-package.sh
```

Extracts the zip into `.work/verify-extract/` and re-runs from the extracted
tree: `env-check.sh`, `cargo metadata --no-deps` for each expected workspace,
and the full `cargo test --locked --offline --workspace` suite for each
expected workspace. The temporary extracted tree and verification target are
removed on startup and exit, using exact allowlisted paths only. Reports
success or the failing step.

## 10. Status reporting

```sh
bash scripts/generate-status-report.sh   # → ../VERIFICATION_REPORT.txt
```

Walks every `loom-*` repo and records: exists / cargo / builds / tests-pass
(from the last `.work/test-*.log`) / status keywords from `TASKS.md` or
`FEATURE_STATUS.md` / missing items.

## 11. Docker environment

```sh
bash scripts/docker-build.sh [service]   # build ci, visual, offline images
bash scripts/docker-test.sh              # fmt + clippy + test in the ci service
bash scripts/docker-visual-qa.sh         # visual QA headlessly
bash scripts/docker-offline-test.sh      # offline test with network_mode: none
```

`docker/compose.yaml` mounts the suite root at `/workspace`; logs and
screenshots written under `.work/` land on the host. Images:

- `Dockerfile.ci` — ubuntu:24.04, rustup stable, X11/font/GL dev libraries,
  fonts-noto-core, xvfb, imagemagick + python3-pil, zip/unzip; locale
  C.UTF-8; `RUSTFLAGS=""` default.
- `Dockerfile.dev` — ci + git/bash/curl/vim/jq.
- `Dockerfile.visual` — ci + xdotool, mesa-utils.

## 12. GitHub Actions

`.github/workflows/ci.yml` runs on ubuntu-24.04: single checkout (this
monorepo workspace), Rust stable with rustfmt+clippy, cargo cache, then
env-check, fmt, clippy, test, release build. Test logs are uploaded as
artifacts (14-day retention). In real deployment, swap the single checkout
for per-repo checkouts pinned via `COMPATIBILITY.toml` revs.


## source-loom-bootstrap-changelog-md

Original path: `loom-bootstrap/CHANGELOG.md`

# Changelog

All notable changes to this repository (loom-bootstrap) are recorded here.
Format follows [Keep a Changelog](https://keepachangelog.com/); versioning
follows Semantic Versioning for the orchestration contract
(`COMPATIBILITY.toml` schema_version and script behavior).

## [Unreleased]

### Added

- Initial bootstrap repository for the Loom suite.
- Orchestration scripts: env-check, build-all, test-all, fmt-all, clippy-all,
  run-apps, visual-qa-all, offline-test, package, verify-package,
  generate-status-report, docker-build/test/visual-qa/offline-test.
- Image comparison helper (`scripts/img-compare.sh`) with ImageMagick or
  python3+PIL backends.
- `COMPATIBILITY.toml` cross-suite manifest (schema_version 1, MSRV 1.80,
  Slint 1.17.1, per-repo status/rev pins).
- `justfile` task recipes (just is optional; scripts run standalone).
- Docker compose environment with `ci`, `dev`, `visual`, and `offline`
  (`network_mode: none`) services.
- GitHub Actions workflow: fmt, clippy, test, release build, artifact upload.
- Documentation: README, AGENTS, BOOTSTRAP, LICENSE_POLICY, DEPENDENCIES,
  SECURITY, ADR-0001.


## source-loom-bootstrap-dependencies-md

Original path: `loom-bootstrap/DEPENDENCIES.md`

# Loom Bootstrap — dependency report

This repository itself has no Rust dependencies (it contains no crates). It
orchestrates repositories that do. The information below describes how the
suite manages dependencies and what is currently pinned.

## Toolchain

| Tool | Version | Required by |
|------|---------|-------------|
| Rust (rustc/cargo) | stable, MSRV 1.80 | all cargo workspaces |
| Slint | 1.17.1 | app UI layer (pinned via COMPATIBILITY.toml) |
| just | any recent | optional task runner |
| docker / compose | any recent | optional visual-QA + offline containers |
| zip / unzip | any recent | packaging and verification |

## Runtime dependencies by repo

Per-repo dependency lists live in each repository's `DEPENDENCIES.md` and
`Cargo.lock`. The authoritative cross-suite pins live in `COMPATIBILITY.toml`
in this repository.

### Shared platform (loom-core, crates/)

| Crate | License | Notes |
|-------|---------|-------|
| loom-package | MIT OR Apache-2.0 | container/package format |
| loom-document | MIT OR Apache-2.0 | document model |
| loom-color | MIT OR Apache-2.0 | color pipeline |
| loom-jobs | MIT OR Apache-2.0 | async jobs |
| loom-command | MIT OR Apache-2.0 | command system |
| loom-history | MIT OR Apache-2.0 | undo/redo |
| loom-text | MIT OR Apache-2.0 | text foundation |
| loom-storage | MIT OR Apache-2.0 | storage |

External third-party dependencies are resolved by each repo's `Cargo.lock`.
A complete inventory (cargo-deny / cargo-license output) is generated at audit
time; see `LICENSE_POLICY.md` for the release gate.

## Dependency audit process

1. `cargo tree -e all` per repo to enumerate direct and transitive deps.
2. `cargo-deny check licenses` (or equivalent) to verify license compliance.
3. Review security advisories via `cargo audit`.
4. Record findings in the suite-wide `DEPENDENCY_REPORT.md` at the parent
   level before release.

## Replacement strategy

Critical dependencies must have a documented replacement strategy in the
consuming repo's ADR, per the suite directive. This repository's role is to
enforce the pins and to validate lockfiles at package time.


## source-loom-bootstrap-license-policy-md

Original path: `loom-bootstrap/LICENSE_POLICY.md`

# Loom Bootstrap — license policy

## Original Loom code

All code authored for the Loom suite (including this repository) is released
under the permissive dual license:

- MIT
- Apache-2.0

Every Cargo package in the suite declares `license = "MIT OR Apache-2.0"`.
New code must not change this without an RFC and a suite-wide license review.

## Dependency policy

- Every direct and transitive dependency must carry a license compatible with
  MIT/Apache-2.0 distribution, or be isolated behind a feature flag with
  documented packaging consequences.
- A dependency whose license cannot be identified is release-blocking
  (`BLOCKED` in COMPATIBILITY.toml).
- Codecs, system media frameworks, and model runtimes are license-sensitive:
  they must be feature-gated, documented in `../LICENSE_POLICY.md` per repo,
  and covered by the dependency audit before any release.
- Pinned versions are recorded in each repository's `Cargo.lock`; lockfiles are
  mandatory and part of the deliverable.

## Model and asset policy

- No model whose license forbids redistribution may be bundled.
- Local model packs (see loom-vision) must carry a manifest with license and
  provenance; installing a pack is a user action.
- All icons, illustrations, sample media, fonts, and sounds in the suite must
  be original or under verified permissive licenses. See the design bible for
  asset provenance requirements.

## Process

- Before release, run the dependency audit (cargo-deny or equivalent) and
  produce the suite-wide `LICENSE_REPORT.md` at the parent level.
- The bootstrap `verify-package.sh` step includes a license check of the
  extracted archive.

## Disclaimer

This document states policy; it is not legal advice. A factual dependency and
license audit must accompany every release.


## source-loom-bootstrap-security-md

Original path: `loom-bootstrap/SECURITY.md`

# Security

This repository orchestrates the Loom suite; it contains no application code,
but it defines security-relevant build, packaging, and verification behavior.

## Principles

- **Local-first:** no telemetry, no account systems, no hidden network calls.
  The offline test (`scripts/offline-test.sh`) enforces that core workflows
  work with the network disabled.
- **Supply chain:** every repo pins dependencies via `Cargo.lock`; the package
  step verifies extraction from a clean tree (`scripts/verify-package.sh`).
- **No secrets:** packaging excludes VCS metadata and build artifacts; before
  any release the archive must be scanned for credentials and absolute
  personal paths.
- **Sandboxing:** plugins (loom-plugin-sdk) and model packs are sandboxed and
  capability-declared; model packs must pass checksum and provenance
  validation before loading (enforced in loom-vision).

## Archive hardening

- `scripts/package.sh` excludes `target/`, `.git/`, `.DS_Store` and produces a
  deterministic file ordering plus a SHA-256 checksum.
- Archive extraction in `scripts/verify-package.sh` is limited to the suite
  package; extraction of untrusted archives (document/plugin/model packages)
  must enforce size/entry limits in the consuming crate (loom-package).

## Reporting

Security issues in Loom should be reported privately to the maintainers —
do not open a public issue with exploit details. Reports should include the
affected repository, version/commit, and a minimal reproduction.

## CI

The GitHub Actions workflow runs fmt, clippy, and the full test suite on every
push; it is the first gate for security-relevant changes (unsafe code,
filesystem access, parser code).


## source-loom-bootstrap-docs-adrs-adr-0001-orchestration-layout-md

Original path: `loom-bootstrap/docs/adrs/ADR-0001-orchestration-layout.md`

# ADR-0001: Orchestration layout and development dependency strategy

- Status: Accepted
- Date: 2026-08-01
- Owner: build-and-CI lead (loom-bootstrap)

## Context

The Loom suite is a multi-repository project: shared crates live in
`loom-core`, each application is its own repo, and the specification/design
repos are documentation-only. The suite directive requires a single
orchestration repository that builds, tests, visually verifies, and packages
all repos, and it requires that application repos build against pinned or
versioned shared crates without source copying or circular dependencies.

Two questions needed a decision:

1. Where does orchestration live, and how is it executed?
2. How do application workspaces depend on `loom-core` crates during
   development?

## Decision

### 1. Orchestration lives in a sibling repository, `loom-bootstrap`, executed via scripts

- `loom-bootstrap` is a normal sibling repo (its own git repo, no cargo
  workspace) containing `scripts/*.sh`, `docker/`, `COMPATIBILITY.toml`, and
  `.github/workflows/ci.yml`.
- The execution interface is **portable bash scripts** (`set -euo pipefail`,
  POSIX-safe) that invoke `cargo`/`docker` per repo. `just` is an optional
  thin wrapper over the same scripts.
- CI and Docker compose call the scripts directly; there is no central
  build server, no Makefile magic, and no dependency on `just` being installed.

Alternatives considered:

- **A Makefile in each repo**: duplicated orchestration, no single gate.
- **CI-only orchestration (GitHub Actions as the only driver)**: impossible
  for local/offline development; the directive requires local runs.
- **A cargo workspace spanning all repos**: forbidden by the directive
  (independent workspaces with own lockfiles; avoids a shared-target
  monolith and enables independent rev pins).
- **just-only**: just is not guaranteed installed on all hosts; scripts are
  the floor, just is the ergonomic layer.

### 2. Relative path dependencies during development; tag-pinned versions at release

- During development, application workspaces reference shared crates via
  relative paths, e.g. `loom-document = { path = "../loom-core/crates/loom-document" }`.
- `COMPATIBILITY.toml` records `rev = "local"` for every repo while
  development is path-pinned.
- Before a release, `rev` entries switch to tagged versions, applications
  move to `git`/registry dependencies on released shared crates, and
  `verify-package.sh` re-runs the gates from the packaged tree to prove the
  packaging works without the development path pins.

Alternatives considered:

- **Published crates from day one**: publication friction blocks lockstep
  changes across the suite and slows iteration.
- **Vendored source copies**: forbidden — no unversioned copying between
  repos; would create divergence.
- **git dependencies on unpublished repos**: equivalent to path deps but
  requires network and complicates offline builds.

## Consequences

Positive:

- One command per gate (`bash scripts/<x>-all.sh`), identical locally, in
  Docker, and in CI.
- Missing repos degrade gracefully (SKIP + report) instead of failing hard,
  which matches the suite's incremental build order.
- Path deps keep the whole suite compiling in lockstep during development.
- The suite stays fully offline-capable: no registry publication is needed to
  develop.

Negative / risks:

- Path deps make each repo's `Cargo.lock` tied to sibling paths; the package
  + verify step is therefore mandatory before any delivery.
- Scripts must be kept POSIX-safe (no zsh-isms, no bash-3.2-incompatible
  syntax) so they run on macOS and in the Ubuntu containers alike.
- Drift risk between `COMPATIBILITY.toml` statuses and reality — mitigated by
  `generate-status-report.sh` and the CI gates.

## References

- Suite directive §6 (repository architecture), §11 (docs), §23 (quality gates),
  §26 (ZIP delivery).


## source-loom-design-bible-accessibility-md

Original path: `loom-design-bible/ACCESSIBILITY.md`

# Accessibility

Accessibility is release-blocking. A feature that fails any requirement in
this document is not complete. This is the single contract all applications
and components implement against.

## 1. Keyboard navigation

* Every control, tool, panel, and command is reachable and operable with
  the keyboard alone — including canvas objects, timeline clips, grid
  cells, and audio regions.
* Tab order is logical (visual reading order): title bar → toolbar →
  document/canvas → sidebars/panels in the order they were opened →
  inspector → status bar controls. Each major region is a Tab group;
  Tab moves between groups, arrows move within a group
  (`KEYBOARD.md` §4).
* No keyboard trap except where a dialog is open (`DIALOGS.md` §4) and in
  explicit modal contexts; Esc always exits the trap.
* Every drag-and-drop has a keyboard path (nudge, cut/paste, menu
  commands) — `DRAG_AND_DROP.md` §5.
* Canvas/timeline/grid keyboard navigation is specified per surface:
  `CANVAS.md` §6, `TIMELINE.md` §6, `SPREADSHEET.md` §3/§7,
  `DOCUMENT_EDITOR.md` §6.

## 2. Focus visibility

* Focus is always visible: 2 px focus ring (`border-width-strong`,
  `color-accent-default`), offset 2 px outside the control bounds
  (`COMPONENTS.md` state model).
* Ring contrast: the ring must hold ≥ 3:1 against every adjacent surface
  in every theme; in the high-contrast theme the ring is black-on-white or
  white-on-black per surface.
* Focus visibility is unconditional: no "focus until mouse is used"
  suppression. Hover states never replace focus states; focus and hover
  states combine (focused control that is also hovered shows both, ring
  wins visually).
* When focus moves, the target is announced (screen reader) and the
  visible ring moves within 120 ms.

## 3. Screen-reader labels

* Every interactive control has an `accessible-description` — a human
  name (not an icon name): "Undo", "Layer 3: rectangle", "Play",
  "Filter clips".
* Grouping: panels and sections expose a group role with a title; toolbar
  groups, property rows, and table headers announce their context.
* Text alternatives for all meaningful graphics (icons carry their
  control's label; charts and previews expose a summary + data table
  where feasible).
* State is announced: checked, disabled, selected, expanded/collapsed,
  value (sliders announce value changes — debounced 120 ms), and
  completion ("Export finished").
* Live regions: errors, progress completion, search results count, and
  status-bar transitions announce without stealing focus; error messages
  use the alert role.
* Custom controls (canvas objects, timeline clips) implement the
  accessible object model of the toolkit with roles mapped per surface;
  see §7.

## 4. Focus order and management

* Focus order is stable: opening a panel places focus on its first
  control; closing returns focus to the opener (dialogs, popovers,
  palettes — `WINDOWS.md`, `DIALOGS.md`).
* The command palette returns focus to the prior element; the inspector
  returns focus to the canvas object it edited when focus was there.
* Popover focus policy per `WINDOWS.md` §4: keyboard-activatable popovers
  take focus; mouse-only conveniences do not steal it.
* Focus never jumps randomly (no focus "helpfully" moving to a spinner);
  background job completion never steals focus.

## 5. High-contrast theme

* A built-in high-contrast theme (true black/white, doubled-contrast
  accents — `COLOR.md` §4) is always available and follows the OS
  high-contrast preference by default; manually switchable.
* In high contrast: all hairlines become solid 1 px white or black;
  selection = white outline on black / black outline on white (never
  accent-only); focus rings are 2 px solid with 2 px offset; icons render
  at full ink; hover = inverted fills (white box + black glyph).
* No feature may depend on theme internals; themes are token swaps
  (`THEMING.md`).

## 6. Text scaling, reduced motion, non-color indicators

* Text scale 1.0/1.25/1.5 (`TYPOGRAPHY.md` §8); at 1.5 no clipping, no
  unusable layouts (visual-QA gate).
* Reduced-motion mode (`MOTION.md` §4): all translation/scale animations
  disabled, opacity only at ≤ 120 ms; applied automatically from the OS
  preference and toggleable.
* Non-color status indicators (`COLOR.md` §6): every status has text/icon/
  shape/position; charts disambiguate series by pattern/dash/label.

## 7. Canvas and media accessibility

* Canvas objects expose: name, type, bounds, role, and actionable
  commands (select, move, resize, edit text) to assistive technology —
  mapped to the platform accessibility tree.
* Timeline: clips are navigable objects with timecode names; transport is
  keyboard-driven (Space/J/K/L); waveform/thumbnail channels are
  decorative for AT (the timecode and metadata carry meaning).
* Spreadsheet: cells announce "A1: 42" with row/column headers; ranges
  announce start/end; formulas announce their formula text in edit mode
  (`SPREADSHEET.md` §7).
* Charts: every chart exposes a data summary ("Bar chart: sales by
  quarter, Q1 12k, Q2 18k…") and keyboard navigation of data points with
  value announcement; scopes (video) expose numeric readouts as text.
* Document text: caret/selection/composition reported per
  `DOCUMENT_EDITOR.md` §6.
* Media playback: play/pause/mute/volume all keyboard-reachable; closed
  captions are first-class text content.

## 8. Errors and announcements

* Errors announce via live regions with the alert role, in plain language
  (`NOTIFICATIONS.md` §4 shape), and repeat the actionable path.
* Validation errors focus the offending control and describe the fix
  ("End date is before start date — swap them").
* Toast announcements are polite; error announcements are assertive only
  for fatal conditions.
* All announcements respect reduced-motion (announce once, no re-announce
  loops) and are not throttled into silence during rapid typing
  (debounce 300 ms, never drop the final state).

## 9. Verification gates (every app, every release)

1. Full keyboard walkthrough of every feature (scripted checklist per
   `UX_ACCEPTANCE_CHECKLIST.md`).
2. Focus ring visible test: automated screenshots at each focus stop.
3. Screen-reader pass with at least one major desktop screen reader on
   Linux (Orca); label lint: every control has
   `accessible-description` (CI assertion).
4. High-contrast visual QA pass (`VISUAL_QA.md`).
5. Text-scale 1.5× layout stress pass (screenshots at 1280 × 800 × 1.5).
6. Reduced-motion pass: assert no translation/scale animations active.
7. Contrast verification: CI computes WCAG contrast for every
   token-on-token pair in use (`COLOR.md` §7).

A gate failure is release-blocking regardless of feature completeness.


## source-loom-design-bible-anti-patterns-md

Original path: `loom-design-bible/ANTI_PATTERNS.md`

# Anti-Patterns

Twenty-two named anti-patterns. Each is a defect regardless of how good it
looks in isolation. Reviewers reject them; implementers never introduce them.

1. **The Ribbon** — a permanently expanded multi-row strip of every command.
   *Example:* 40 buttons across three rows, toolbar height 120 px. *Why it
   fails:* content loses; nothing is discoverable because everything is
   visible. *Instead:* single-row contextual toolbar (`TOOLBARS.md`).

2. **Modal Dialog Abuse** — dialogs for properties, formatting, or
   confirmations of undoable actions. *Example:* a "Text Color" dialog with
   OK/Cancel. *Instead:* inspector + toolbar; dialogs only for
   consequential decisions (`DIALOGS.md`).

3. **Hover-Only Functionality** — features that exist only while the pointer
   hovers. *Example:* row action buttons that vanish without hover; sidebar
   auto-peek. *Fails:* keyboard users, touch/pen, focus-based access.
   *Instead:* actions appear on hover AND focus, and exist in menus/palette.

4. **Tiny Unlabeled Icons** — 16 px icon-only buttons with no tooltip,
   no accessible name, no menu equivalent. *Instead:* ≥ 44 px targets,
   tooltips with name + shortcut, `accessible-description`
   (`POINTER_AND_PEN.md`, `ACCESSIBILITY.md`).

5. **Decorative Animation** — motion that answers no usability question.
   *Example:* logo bounce, buttons that pulse on hover, entire dialogs that
   fly in. *Instead:* the motion grammar (`MOTION.md`) — every animation
   maps to a token and a question.

6. **Destructive Default Button** — Enter activates "Delete permanently"
   instead of Cancel. *Instead:* safe path is always the default
   (`DIALOGS.md` §3).

7. **Color-Only Status** — green/red dots with no text or icon. *Fails:*
   color blindness, high contrast, screen readers. *Instead:* text + icon +
   color (`COLOR.md` §6).

8. **Invisible Focus** — focus rings removed "for polish". *Instead:*
   focus is always visible (`ACCESSIBILITY.md` §2). Release-blocking.

9. **Mouse-Only Surfaces** — a timeline or grid with no keyboard path.
   *Instead:* complete keyboard model per surface (`KEYBOARD.md`).

10. **UI-Thread Blocking** — synchronous open/save/decode/thumbnail on the
    main thread. *Example:* opening a project freezes the window for 4 s.
    *Instead:* jobs with progress and cancellation (`PERFORMANCE.md` §2).

11. **The Fake Progress Bar** — indeterminate bar where a determinate value
    exists, or progress that jumps 0→100 at the end. *Instead:* truthful
    determinate progress or, when unknowable, phase text
    (`COMPONENTS.md` §18).

12. **Uncancellable Work** — long operations with no cancel path.
    *Instead:* every long job is cancellable and announces it
    (`NOTIFICATIONS.md` §6).

13. **Stealing Focus** — background completion popping a window to front or
    moving the caret. *Instead:* status bar + toast
    (`WINDOWS.md` §5, `NOTIFICATIONS.md`).

14. **The Empty Dead End** — an empty state with no action path. *Example:*
    "No projects" with no "New Project" button. *Instead:* EmptyState always
    has an action (`COMPONENTS.md` §14).

15. **Silent Failure** — an action that fails without feedback. *Example:*
    export writes a partial file and reports success. *Instead:* truthful
    completion with atomic writes and error reporting
    (`NOTIFICATIONS.md` §4).

16. **Nested Scroll Traps** — scroll regions inside scroll regions inside
    scroll regions. *Instead:* one scrolling column per panel
    (`SIDEBARS.md` §3).

17. **The Confetti Welcome** — onboarding that demands attention before the
    user can work. *Instead:* immediate, useful default document; help is
    on demand.

18. **Per-App Drift** — the same component re-implemented differently per
    app (checkbox styles, inspector layouts, spacing). *Instead:* shared
    components and tokens; drift is a review-blocking defect
    (`AGENTS.md` §3, `DESIGN_BIBLE.md` §6).

19. **Ribbon-Numbered Shortcuts** — shortcuts assigned by "next free key"
    with no hierarchy. *Instead:* the key map in `KEYBOARD.md`; assignment
    policy with conflict detection.

20. **Magnetic Drag** — dragging that eases, lags, or snaps decoratively.
    *Instead:* 1:1 pointer-follow with zero lag; snap only as drop-assist
    (`DRAG_AND_DROP.md` §1–2).

21. **The Gradient Everywhere** — gradients, glows, and drop shadows used
    for "depth". *Instead:* depth by color steps; `shadow-popover` only for
    popovers (`DESIGN_TOKENS.md` §10).

22. **Silent Autosave Failure** — autosave failing while the UI pretends
    otherwise. *Instead:* autosave is observable in the status bar and
    failures are errors (`NOTIFICATIONS.md` §6).

Review guidance: if a design contains any of these, it is rejected without
argument. If a workaround is needed, it must be an ADR — not an exception
smuggled into a component.


## source-loom-design-bible-canvas-md

Original path: `loom-design-bible/CANVAS.md`

# Canvas

The canvas is where the work happens: pages, images, timelines' previews,
slides, compositions. This document fixes the chrome and interaction around
it; surface-specific rules live with the surface (`TIMELINE.md`,
`SPREADSHEET.md`, `DOCUMENT_EDITOR.md`).

## 1. Canvas chrome

* The canvas fills the space between sidebar, toolbar, and status bar
  (`LAYOUT.md`). It has no default chrome of its own beyond:
* **Rulers** (`[goal]` v1, `[future]` full): optional top/left rulers
  (toggle `Cmd+R`), 20 px strips, tick marks in tabular figures at
  `space-8`-aligned intervals; rulers are hidden by default in canvas apps
  (Photo: always hidden; Writer: visible in page mode; Motion/Video:
  hidden — time ruler lives in the timeline).
* **Scrollbars**: overlay-style scrollbars (auto-hide) in canvas apps;
  always-visible thin scrollbars (8 px) in document/grid surfaces;
  scrollbar color: ink at 30% on hover 45%; keyboard scrolling via arrows
  with Shift=fast (`KEYBOARD.md`).
* **Zoom readout**: status bar right (`LAYOUT.md` §7), tabular figures,
  click opens zoom menu; `Cmd+0` fit, `Cmd+1` 100%, `Cmd+Plus/Minus` step.

## 2. Zoom

* Range: **25% – 800%**, stepping: 25/33/50/66/75/100/125/150/200/300/400/
  600/800%; free zoom in between allowed via pinch/wheel, snapped readout
  shows the actual value.
* Zoom anchored at the pointer (wheel/pinch zooms toward the cursor);
  control-driven zoom (menu, buttons) anchors at the canvas center.
* Zoom animation: 200 ms in-out when triggered by controls; **instant while
  actively zooming** (wheel/pinch) — never animate behind an active gesture.
* Zoom is a content property: zoom state persists per document; text-scale
  (1.0/1.25/1.5) is separate and never affects canvas zoom
  (`TYPOGRAPHY.md` §8).
* At any zoom, minimum readable canvas feedback: panning is smooth at 60 fps;
  at zoom < 50% large objects may render simplified only if visually
  equivalent (no detail flaking).

## 3. Pan

* Space+drag or middle-mouse drag pans; two-finger scroll pans (canvas apps
  map scroll to pan when the document fits, scroll-to-zoom with modifier);
  scroll wheel scrolls (vertical/horizontal per platform), Shift+wheel
  horizontal.
* Hand tool (`H`) pans; double-click hand tool fits content.
* Panning is instant, 1:1, no easing; inertia is `[future]` (off by default
  when added — predictable panning is the baseline).
* Bounds: content may be panned freely; a grid dot pattern
  (`color-ink-primary` at 5%) fills the canvas beyond content so emptiness
  is legible; the pattern only appears when content is smaller than the
  viewport.

## 4. Guides and snapping

* Guides: drag from rulers (when visible); vertical/horizontal lines,
  `color-accent-default` at 60% opacity, 1 px; guides are per-document,
  movable, deletable (drag off-canvas), lockable.
* Snapping: toggles — snap to guides, snap to grid, snap to object bounds,
  snap to center/midlines (per app default documented in PRODUCT_SPEC:
  Photo/Motion default on; Writer page-flow does not snap).
* Snap feedback: during a drag, a snap indicator line (accent, 1 px) shows
  the snapped alignment plus a 40 px snap halo around the pointer; snap
  activates within 8 px (halo), snaps instantly (no magnetic animation).
* Grid: `space-16` default grid (documented per app), dots at 6% ink, togglable.

## 5. Direct manipulation targets

* Every selectable object offers handles (per `SELECTION.md`); targets are
  ≥ 8 px visual with 48 px effective hit area; small objects get an
  invisible expanded hit box (never silently overlapping neighbors — hit
  priority: topmost object first, then by z-order).
* Rotate: handle above top-center; Option/Alt rotates around center.
* Nudge: arrows (1 px, Shift 10 px) — undoable in one command per gesture
  series (coalesced undo).
* Constrain: Shift constrains aspect/direction during drags (move: axis
  lock after initial vector; resize: preserve aspect; rotate: 15° steps
  from 0°/90°).
* Alt/Option duplicates on drag (Photoshop-style convention) only in
  Photo/Motion; duplicate is always undoable.
* Live feedback during manipulation: geometry readout near the pointer
  (width × height, rotation, x/y in tabular figures, `space-8` from the
  pointer, `type-size-11`, contrast-backed chip).

## 6. Canvas accessibility strategy

* Canvas surfaces expose an object model to accessibility: each object has
  a name, type, bounds, and role (per `ACCESSIBILITY.md` §canvas).
* Keyboard navigation of canvas objects: Tab moves between top-level
  objects in z-order; arrows nudge; Enter opens the object's inspector
  section; context-menu key opens object actions.
* Text objects are editable by keyboard (caret entry); media objects expose
  play/pause via keyboard.
* The canvas never depends on pointer-only operations; any drag operation
  has a keyboard path (nudge, arrow-based resize, or numeric inspector
  fields) — verified by the acceptance checklist.
* Reduced motion: all canvas feedback (snap halos, zoom animation, selection
  morphs) goes instant; scrubbing stays functional.


## source-loom-design-bible-color-md

Original path: `loom-design-bible/COLOR.md`

# Color

The color contract for Loom: the three palettes, usage rules, contrast
requirements, non-color status indicators, and the data-visualization palette.

## 1. Role of color

Color has four jobs in Loom, in priority order:

1. **Grounding content** — canvas vs. surface vs. ink separation.
2. **Signal** — selection, focus, status, destructive intent.
3. **Meaning in data** — categorical series in charts.
4. **Warmth** — the warm-neutral material tone of the whole suite.

Color is never decorative, never the only channel for meaning, and never used
for hierarchy (hierarchy is size and weight).

## 2. Light palette (default)

| Token | Value | Usage |
|---|---|---|
| `color-surface-canvas` | `#FAF9F7` | Window backdrop, canvas surround, page-free areas |
| `color-surface-raised` | `#FFFFFF` | Panels, dialogs, controls, cards |
| `color-surface-sunken` | `#F1EFEA` | Input wells, media beds, disabled regions |
| `color-ink-primary` | `#26221C` | Primary text |
| `color-ink-secondary` | `#5C564C` | Secondary text, captions, placeholders |
| `color-accent-default` | `#B4552D` | Selection, focus, primary actions (terracotta) |
| `color-accent-ink` | `#FFFFFF` | Text/icon on accent fills |
| `color-accent-hover` | `#C9643A` | Hover/pressed accent states |
| `color-status-success` | `#3E6B4F` | Success signals |
| `color-status-warning` | `#A8681E` | Warning signals |
| `color-status-danger` | `#A43424` | Errors, destructive actions |
| `color-status-info` | `#3B5E7A` | Informational signals |

## 3. Dark palette

| Token | Value |
|---|---|
| `color-surface-canvas` | `#201D19` |
| `color-surface-raised` | `#2A2621` |
| `color-surface-sunken` | `#171512` |
| `color-ink-primary` | `#F2EFE9` |
| `color-ink-secondary` | `#B9B2A6` |
| `color-accent-default` | `#D97A4A` |
| `color-accent-ink` | `#1F1710` |
| `color-accent-hover` | `#E88B5E` |
| `color-status-success` | `#7FA98C` |
| `color-status-warning` | `#D5A14A` |
| `color-status-danger` | `#E0654F` |
| `color-status-info` | `#7FA3C4` |

## 4. High-contrast palette

True black/white with doubled-contrast accents (all values verified ≥ 7:1
against pure black; see §7 for the method):

| Token | Value |
|---|---|
| `color-surface-canvas` | `#000000` |
| `color-surface-raised` | `#000000` |
| `color-surface-sunken` | `#000000` |
| `color-ink-primary` | `#FFFFFF` |
| `color-ink-secondary` | `#E6E6E6` |
| `color-accent-default` | `#FF8A3C` |
| `color-accent-ink` | `#000000` |
| `color-accent-hover` | `#FFA266` |
| `color-status-success` | `#4ADE80` |
| `color-status-warning` | `#FFC24D` |
| `color-status-danger` | `#FF6B5E` |
| `color-status-info` | `#6CB2FF` |

In high contrast, surfaces lose all gray steps (everything is black); depth
is carried by white borders (`border-width-default` becomes white) and text
tiers (`ink-secondary` is only for tertiary labels).

## 5. Usage rules

* **Content first**: the canvas (`color-surface-canvas`) is the lightest
  (darkest in dark theme) ground. Content surfaces sit on it. Chrome sits on
  `color-surface-raised`. Wells sit in `color-surface-sunken`.
* **Ink**: primary text uses `color-ink-primary` on every surface; secondary
  uses `color-ink-secondary` — no third text tier on light/dark (captions are
  secondary, never a lighter gray).
* **Accent is a tool, not a theme**: accent is for selection, focus, and
  primary actions. It does not paint entire windows, toolbars, or panels.
  Accent fills (buttons) use `color-accent-ink` for text.
* **Danger**: destructive actions use `color-status-danger` for the action's
  label or icon and confirm buttons; never for the window chrome.
* **Borders**: `border-width-default` borders use a 12% opacity ink
  (`color-ink-primary` at 12% alpha) on light/dark; high contrast uses pure
  white.
* **Depth without shadows**: surface separation comes from canvas→raised→sunken
  steps only. `shadow-none` everywhere except `shadow-popover` on popovers
  (see `DESIGN_TOKENS.md` §10).

## 6. Status and non-color indicators

Color is never the sole channel for status. Every status has at least two
channels:

* Success: green + check icon + affirmative text ("Saved").
* Warning: ochre + triangle icon + explanation text.
* Error: red + octagon/alert icon + description text; the message also
  reaches a screen-reader live region.
* Info: blue + circle-info icon + text.

In high contrast, icons and text carry the meaning at full strength; the
color channels are secondary. Progress is a value (bar fill or percent), not
a color.

## 7. Contrast requirements

Floors (WCAG 2.x relative-luminance contrast, all four themes):

* **4.5:1** — body text, captions, placeholders, icon labels.
* **3:1** — large text (≥ 24 px or ≥ 18.66 px bold), UI component boundaries
  (focus rings, control outlines, toggle tracks), and iconographic indicators
  when the icon is the only label.
* **3:1** — text on accent fills at large sizes; body-size text on accent
  fills must be ≥ 4.5:1 (hence `color-accent-ink` on `color-accent-default`
  = 4.9:1 in light).

Verified values in the light theme (computed from the sRGB relative-luminance
formula): ink-on-canvas 15.0:1, ink-secondary-on-canvas 6.9:1,
accent-ink-on-accent 4.9:1, danger-on-white 6.8:1, success-on-white 6.1:1,
warning-on-white 4.5:1, info-on-white 6.9:1. Dark theme: ink-on-canvas 14.6:1,
ink-secondary-on-canvas 8.0:1, accent-ink-on-accent 5.7:1. High contrast:
accent-on-black 8.95:1, ink 21:1. CI verifies every committed palette value
against these floors; a value below its floor fails the build.

## 8. Data-visualization palette

Categorical series (charts, scopes, timelines), proposed set, values valid in
light and dark themes:

| Name | Value | Notes |
|---|---|---|
| terracotta | `#B4552D` | Suite accent — charts may use it as series 1 |
| teal | `#2E7D6E` | Series 2 |
| ochre | `#C98A2E` | Series 3 (≥ 3:1 as large marks; avoid small text) |
| slate | `#4A6B8A` | Series 4 |
| plum | `#7A5C8A` | Series 5 |
| moss | `#6B8A4A` | Series 6 |

Rules:

* Max 6 categorical series; beyond that, group and use a legend pattern.
* Series are always disambiguated by pattern, dash, or label in addition to
  color (colorblind-safe: teal/ochre and plum/slate pairs are distinguished by
  luminance as well as hue).
* Never pair `terracotta` with another series color that is visually
  identical to it; never use `color-status-*` colors as series colors in the
  same chart as status indicators.
* Chart backgrounds use `color-surface-sunken`; series must hold ≥ 3:1
  against it in both themes. Where a value fails (ochre on light), use larger
  marks with white or black outlines and verify in the theme's visual QA.

## 9. Color management

* All palette values are specified in sRGB; the render pipeline is
  color-managed (see `loom-core`'s `loom-color` contract and `loom-spec`).
* Screen colors are display-profile adjusted at runtime only; tokens are the
  canonical sRGB values for deterministic screenshots in the software renderer.
* Never sample a token from a screenshot; never tune a token "until it looks
  right" without re-running the contrast verification in §7.


## source-loom-design-bible-command-palette-md

Original path: `loom-design-bible/COMMAND_PALETTE.md`

# Command Palette

The command palette is the fastest path to every command, in every app, with
the keyboard only. It is the fourth layer's power surface.

## 1. Invocation and anatomy

* Open: `Cmd+Shift+P` (Windows: `Ctrl+Shift+P`); also the title bar's
  `commands` icon button and the Help menu. Focus goes into the palette on
  open; typing starts searching immediately (no "click to search" step).
* Popover surface centered top-third of the window, width 480 px, max height
  480 px, `shadow-popover`, `radius-8`; entrance: out-back 320 ms (the one
  sanctioned overshoot), reduced motion: fade 120 ms.
* Anatomy: search field (magnifier icon, placeholder "Search commands,
  files, help…"), result list, footer row with hints ("↑↓ navigate · ↵
  run · ⌫ history").

## 2. Scope

The palette searches, in order:

1. **Commands** — every command in the app (the command registry:
   `loom-core` command system), filtered by current context: commands that
   apply now rank above commands that cannot run; disabled commands show at
   40% with a reason and remain selectable-and-blocked (never silently
   removed).
2. **Recently opened documents** — the app's recent files, when the query
   matches their name.
3. **Help topics** — bundled help entries, when the query matches (help
   search per `loom-core` search contract).
4. **Settings** — named settings reachable via the palette ("theme",
   "shortcuts", "autosave").

A single result list, grouped by these scopes with tiny group labels
(`type-size-11` caps, `space-8` spacing).

## 3. Fuzzy search

* Fuzzy substring matching with per-character skip, case-insensitive;
  matches are highlighted in the result label with accent.
* Ranking: exact-prefix > word-prefix > contiguous substring > scattered
  matches; tie-break by most-recently-used.
* Minimum query: empty query shows **most-recent-first command history** +
  recently opened documents (see §4); one character filters.
* All results render in under one frame of typing (search is over an
  in-memory index; no disk I/O in the palette path).

## 4. Ordering

* History ordering: commands the user has run most recently rank above
  alphabetically-equal peers; a persistent MRU list (per app, bounded at
  20 entries, reset-able in settings).
* When the query matches nothing in a scope, that scope hides; when nothing
  matches at all, show the empty state: "No command found — press Esc to
  dismiss" plus a "Search in help" action.

## 5. Keyboard model

| Key | Action |
|---|---|
| Type | Filter |
| ↑ / ↓ | Move selection (wraps) |
| Enter | Run selected command |
| Cmd+Enter | Run without closing (sticky) |
| Esc | Close (cancels) |
| Backspace at empty | Return to history view |
| Tab | Cycle scope groups (commands → files → help → settings) |

* The palette is keyboard-first but mouse-usable (click item runs it; click
  outside closes).
* Running a command closes the palette (except `Cmd+Enter`); the command's
  undo description feeds the undo menu as usual.
* Screen reader: opening announces "Command palette"; result selection is
  announced; Enter confirms.

## 6. Behavior rules

* The palette is non-modal in the popover sense (it does not lock the
  window), but while open, focus stays inside it; Esc always returns focus
  to where it was.
* Palette state (last query) is not persisted across sessions; MRU is.
* Commands that require context run with the existing selection — the
  palette never silently changes the selection.
* Every command in every app must be in the palette; the acceptance gate is:
  for each command in the app's command registry, assert its palette entry
  exists with the documented label (`UX_ACCEPTANCE_CHECKLIST.md`).


## source-loom-design-bible-components-md

Original path: `loom-design-bible/COMPONENTS.md`

# Loom Shared Components

This file defines component **semantics and ownership**. Exact measurements live in [`contracts/desktop-ui.toml`](loom-design-bible/contracts/desktop-ui.toml); token values live in [`tokens/loom.toml`](loom-design-bible/tokens/loom.toml). Do not duplicate or override those values in an application.

## Canonical object kit

The toolkit exports one canonical object for each shared shell role:
`TitleChrome`, `IconOnlyToolbarItem`, `IconOverLabelToolbarItem`,
`SheetTabStrip`, `FormulaBar`, `InspectorSection`, `PropertyRow`, `Field`,
`SegmentedControl`, `StatusBar`, and `Overflow`. `ContextToolbar` and
`LabeledToolbar` make the two toolbar slot heights explicit. Existing names
such as `DocumentChrome`, `AppleToolbarItem`, `TabStrip`, `TextField`,
`ToolkitStatusBar`, and `ToolbarOverflowButton` remain compatibility aliases
to those owners, not second implementations.

The same kit is exported through `components.slint` for source compatibility.
An audit checks declaration ownership and rejects duplicate implementations.

## Ownership rule

`loom-core/crates/loom-ui` owns every generic desktop control and shell primitive. Applications compose them and supply domain state/commands. They do not fork visual implementations.

Shared ownership includes at minimum:

- document/title chrome;
- context toolbar and toolbar groups/overflow;
- buttons, icon buttons, toggles, checkboxes, radio/segmented controls;
- text/search/numeric fields;
- sliders and steppers;
- menus, popovers, tooltips, dialogs;
- sidebars, inspectors, section headers, status bars;
- tabs and list/tree rows;
- empty/loading/error states;
- command palette;
- canvas backdrop, selection overlay, transform handles, rulers and guides;
- layer/object lists;
- timeline ruler, track header, clips, playhead and trim handles.

If two Loom applications need the same interaction pattern, it belongs in `loom-ui` unless the shared-platform owner documents why their semantics differ.

## Universal state model

Every interactive component implements these states where applicable:

`default → hover → pressed → keyboard-focus → disabled`

Selectable components additionally implement `unselected` and `selected`. Inputs additionally implement `valid` and `invalid`. Async actions additionally implement `idle`, `running`, `success`, `failure`, and `cancelled` when those states are observable.

State rules:

- hover/press/focus never change component geometry;
- keyboard focus uses the shared focus token and remains visible in every theme;
- disabled controls are not clickable, keyboard-activatable, or accessibility-activatable;
- selected/checked state is not represented by color alone;
- error state includes text or icon semantics, not only a red border;
- every icon-only control has an accessible name and tooltip;
- every action resolves to the same typed command used by menus and shortcuts.

## Standard controls

### Button / ToolButton

Use the contract's standard control height, horizontal padding, radius, typography, and state durations. A button label never ellipsizes. If there is not enough width, change toolbar composition or move the action to overflow.

Toolbar controls are visually quiet. Routine toolbar items do not need a permanent raised bezel; hover, checked, pressed, and focus states provide affordance.

### IconButton

Uses the standard square target and standard icon size. Tooltip is mandatory. An unambiguous symbol can replace a text label only where the corresponding command remains discoverable in menus/palette.

### Text/Search/Numeric fields

Use shared field geometry. Placeholder text is secondary, not disabled. Validation does not resize the control; error/help text occupies a separate row below or an inspector message region.

### Segmented control

Use only for a small mutually-exclusive set. Six segments is the absolute maximum; larger sets become a menu/combobox/list. Segment labels never truncate.

### Slider

The visual handle may be smaller than the desktop target only when its hit region remains at least the contract target. Arrow keys adjust values and the accessible value is exposed.

### List/tree row

User-authored names may ellipsize with full tooltip/accessibility value. Command labels, badges with semantic meaning, and row actions may not collide with the name.

## Chrome components

### DocumentChrome / AppHeader compatibility surface

The long-term shared component is document-oriented chrome, not a branded app banner. Existing `AppHeader` call sites are a migration compatibility surface.

Rules:

- document/project title is primary;
- app name is not repeated as a large in-window title;
- neutral facts such as "Local" or package type are not permanent chrome;
- no decorative app-logo tile;
- title may ellipsize because it is user content;
- important modified/sync/error state may appear only when it changes a user decision.

### WorkspaceToolbar

Implements the algorithm in `TOOLBARS.md` and the machine contract: one line, maximum three groups, deterministic priority collapse, no scrolling, no clipping.

Use `ContextToolbar` for a 40 px context row and `LabeledToolbar` for the
48–52 px icon-over-label slot. The host must select one explicitly; children
never stretch the other slot implicitly.

### Sidebar / Inspector

Flush work surfaces, not card stacks. One body scroll surface. Sections use spacing and headers rather than decorative containers. Property labels do not truncate; rows stack when localization/text scale requires it.

### StatusBar

Contains transient status, progress/cancel affordance, and compact readouts. It is not a second toolbar and never competes with content hierarchy.

## Editor primitives

Shared editor primitives are as important as shared buttons. A professional suite cannot obtain coherent behavior if Photo, Present, Motion, Video, and Studio each invent selection, drag, snapping, zoom, and timeline semantics.

### Canvas

The shared canvas family owns viewport pan/zoom, hit testing, selection overlay, transform handles, snapping, guides, keyboard nudging, gesture cancellation, and direct-manipulation state. Domain engines provide objects and operations; the UI layer does not own document truth.

### Selection overlay

Selection handles use exact visual/hit sizes from the contract. Selection itself is not an undoable edit. Drag operations coalesce into one operation; Escape restores the pre-gesture state.

### Timeline

Motion, Video, and Studio share ruler/playhead/track/clip primitives. Application-specific track content can differ, but pointer capture, selection, trim hit regions, scrolling/zoom, snapping, and keyboard semantics are shared.

### Grid/document viewport

Writer and Sheets use specialized editor surfaces built from shared scrolling, selection, keyboard-routing, ruler/header, and inspector primitives. A generic `TextEdit` or fixed demo grid is not a professional editor architecture.

### Geometry evidence

`loom-bootstrap/scripts/audit-product-ui.py` emits and validates a logical
geometry manifest at the six responsive boundary probes, required viewport
matrix, 150% text scale, and LTR/RTL directions. The manifest checks bounds,
overlap, clipping, toolbar line count, and primary-surface starvation; a stable
PNG hash alone is not component evidence.

## Component completion gate

A shared component is complete only when:

1. its geometry comes from the contract/tokens;
2. all required states render and behave;
3. keyboard and accessibility activation work;
4. it appears in the deterministic component reference surface;
5. realistic, empty, disabled, and error states have fixtures as applicable;
6. required viewport/theme/text-scale captures pass approved-baseline comparison;
7. no known overlap/clipping defect remains;
8. at least one application uses it in a real workflow;
9. the previous app-local implementation is deleted after migration.

Passing a source-string audit or producing a stable screenshot alone is insufficient.


## source-loom-design-bible-design-bible-md

Original path: `loom-design-bible/DESIGN_BIBLE.md`

# Loom Design Bible

Master document for the Loom design language. This file is the index of the
contract; each section names the owning document and summarizes the binding
rules. Where this file and a section document disagree, the section document
holds — except for token values, where `tokens/loom.toml` wins.

## 1. Design language

Loom is **calm, precise, minimal, warm, professional, fast, predictable,
capable, timeless.**

* Calm — nothing shouts. Restrained chrome, low noise, generous silence.
* Precise — snapping, exact alignment, truthful progress, deterministic layout.
* Minimal — content is the focus; chrome is supporting furniture.
* Warm — a warm-neutral palette (canvas `#FAF9F7`, terracotta accent
  `#B4552D`) rather than cold grays and saturated blues.
* Professional — depth through progressive disclosure, not clutter.
* Fast — input feedback within one frame; no UI-thread blocking, ever.
* Predictable — same action, same place, same result in every application.
* Capable — professional features exist; they are simply revealed on demand.
* Timeless — no decorative trends, no visual noise that will age.

## 2. Principles

The ten+ principles in `DESIGN_PRINCIPLES.md` govern all decisions:

1. Content first. 2. Calm over busy. 3. Direct manipulation first. 4. Progressive
disclosure. 5. Predictability across the suite. 6. Truthful feedback. 7. Motion
with meaning. 8. Accessibility is release-blocking. 9. Professional depth,
never featurelessness. 10. Performance is a feature. 11. Warmth without
decoration. 12. Every default is a deliberate choice.

## 3. Layout system

* Window anatomy (see `LAYOUT.md`): title bar, single-row contextual toolbar
  (**40 px**), collapsible sidebar (**240 px**), contextual inspector
  (**280 px**), status bar (**28 px**), content canvas fills the remainder.
* Spacing is drawn from the scale 2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48, 64 px.
  Only these values, with one exception: components may use
  `0.5 × space-2` = 1 px only for internal hairline offsets, documented locally.
* Chrome heights are fixed and DPI-scaled; content surfaces flex.

## 4. Typography

* Default sans: **Noto Sans** (per `TYPOGRAPHY.md`), with a documented fallback
  chain and a future variable-font path.
* Type scale: 11, 12, 13, 14 (body), 16, 20, 24, 32, 40 px; leading ratios
  1.4–1.5; weights 400/500/600/700.
* Body line length 45–75 characters per line (prefer 60–72 in documents).
* Tabular figures for all data columns, timestamps, and numeric inspectors.
* UI scales with a text scale factor of 1.0 / 1.25 / 1.5.

## 5. Color

* Full palettes (light, dark, high-contrast) in `COLOR.md` and
  `tokens/loom.toml`. Accent is warm terracotta `#B4552D` (light).
* Contrast floors: 4.5:1 body text, 3:1 large text, 3:1 UI components and
  iconographic indicators (WCAG 2.x relative luminance).
* Status is never conveyed by color alone; every status has a text, icon,
  shape, or position component.
* Depth is conveyed by color steps, not shadows (`shadow-none`); popovers are
  the only allowed exception (`shadow-popover`).

## 6. Components

* Component inventory and full state matrices (default/hover/active/focus/
  disabled/checked) in `COMPONENTS.md`.
* All components use the same tokens; there are no per-application forks of a
  component. Applications may combine components; they may not redefine them.
* Every interactive component has an `accessible-description`, a focus ring,
  a disabled visual, and a keyboard path.

## 7. Interaction rules

* Direct manipulation first, then context toolbar, then inspector, then menus
  and command palette, then advanced workspace, then scripting — six layers of
  progressive disclosure (`DESIGN_PRINCIPLES.md` §4).
* Selection visuals: accent 2 px outline plus overlay (`SELECTION.md`).
* Every long operation is a job: cancellable, observable, recoverable. Nothing
  blocks the UI thread.
* Dragging, reordering, zooming, scrubbing all have specified feedback and
  motion (`MOTION.md`, `DRAG_AND_DROP.md`).

## 8. Motion

* Durations: instant 0, fast 120 ms, standard 200 ms, deliberate 320 ms,
  slow 500 ms.
* Easings: out-quad `(0.33, 1, 0.68, 1)`, in-out `(0.65, 0, 0.35, 1)`,
  out-back `(0.34, 1.56, 0.64, 1)`.
* Every animation answers a usability question. All animations are
  interruptible. Reduced motion disables translation/scale and keeps opacity
  changes at 120 ms (`MOTION.md`).

## 9. Accessibility baseline

Release-blocking (`ACCESSIBILITY.md`):

* Complete keyboard navigation; every control reachable and operable by keyboard.
* Visible focus at all times (accent 2 px ring, min 2 px offset).
* `accessible-description` on every control; logical focus order.
* High-contrast theme (true black/white, doubled-contrast accents).
* Text scaling 1.0/1.25/1.5 without layout breakage.
* Reduced-motion mode; non-color status indicators.
* Accessible canvas/timeline/grid navigation strategies per application.
* Configurable shortcuts; errors announced via live regions.

## 10. Theming

* Four built-in configurations: **light**, **dark**, **high-contrast**, and the
  orthogonal **reduced-motion** mode (`THEMING.md`).
* Themes are pure token swaps. Components are theme-agnostic; they consume
  tokens only.
* Custom/third-party themes are `[future]` and must go through the same token
  interface.

## 11. Visual QA

* In-app screenshots with the software renderer at fixed **1280×800**, stored
  as `baselines/<app>/<name>.png`.
* Perceptual diff with two gates: mean absolute error `< 1.0` and
  differing-pixel ratio `< 0.01`. No auto-approval. Baselines are generated
  only in the Docker visual environment (`VISUAL_QA.md`).

## 12. Performance

* Input feedback within one frame; animations at 60 fps; no allocations per
  frame in scroll paths; warm launch `< 1 s` for lightweight apps; bounded
  memory via documented cache policies; cancellation feedback immediate
  (`PERFORMANCE.md`).

## 13. Grammar for writing requirements

Requirements must be written so a less-capable agent can implement them
without inference. Prefer: "The toolbar is a single row, 40 px tall, left
aligned, with the primary action first." over "The toolbar should feel
efficient." See `AGENTS.md` §4.

## 14. Governance

* Token changes: ADR + TOML + prose in one change.
* New component: component spec in `COMPONENTS.md` + states matrix +
  `DESIGN_REVIEW.md` checklist entry.
* New motion: duration/easing from the token set; no bespoke values.
* Any exception to this Bible must be an ADR. Exceptions without an ADR are
  defects in the application, not precedents.


## source-loom-design-bible-design-principles-md

Original path: `loom-design-bible/DESIGN_PRINCIPLES.md`

# Design Principles

Twelve principles govern every Loom design decision. Each has a rationale and
anti-examples — concrete things that violate the principle. If two principles
conflict, lower-numbered principles win, except that **principle 8
(accessibility) always wins.**

## 1. Content first

The document, canvas, spreadsheet, timeline, or composition is the product.
Chrome exists to support the work, not to display itself.

*Rationale:* Professionals spend hours inside one surface. Every pixel of
chrome is time stolen from content. Calm interfaces earn trust because the
work is always the most prominent thing on screen.

*Anti-examples:* A ribbon permanently occupying a third of the window height;
toolbar rows that wrap when the window narrows; a sidebar that cannot be
collapsed; decorative headers on panels; watermark backgrounds behind content.

## 2. Calm over busy

Reduction until further reduction would cost capability, then progressive
disclosure for what remains.

*Rationale:* Visual noise increases decision time and error rate. A quiet
interface signals mastery; a noisy one signals fear of missing features.

*Anti-examples:* Every tool visible at once; status badges on top of badges;
persistent scroll indicators; pulsing "new" dots everywhere; dense borders
around every control; gradients and drop shadows as decoration.

## 3. Direct manipulation first

When the user can point at an object and change it, they should: drag, resize,
scrub, nudge, rotate, reorder. Properties belong in the inspector; the
essential shape of the action belongs on the object.

*Rationale:* Direct manipulation is the fastest path from intent to result and
the strongest mental model. It is also how experts judge a tool's quality.

*Anti-examples:* A color picker that requires opening a dialog to change a
fill; resizing only via numeric fields; reordering only via up/down buttons;
no drag feedback; invisible handles that only appear on hover.

## 4. Progressive disclosure

Six layers, in order: (1) direct manipulation, (2) context toolbar, (3)
contextual inspector, (4) menus and command palette, (5) advanced workspace or
panel, (6) scripting and plugins.

*Rationale:* A beginner must be able to produce useful work immediately; an
expert must reach any feature in seconds. Layering lets both happen without
either paying for the other.

*Anti-examples:* Dumping every feature into the first layer; burying a common
feature in the fifth layer; the same feature exposed in three layers at once;
modal dialogs used as a disclosure mechanism.

## 5. Predictability across the suite

Eight applications, one behavior. Same shortcut does the same thing; same
component looks and behaves identically; same gesture has the same result.

*Rationale:* Users move between Writer and Motion. Muscle memory built in one
application must transfer. Predictability is the cheapest power users have.

*Anti-examples:* `Cmd+S` meaning something different in two apps; two
differently-styled checkboxes; three applications with three different undo
models; per-app inspector layouts that contradict the suite convention.

## 6. Truthful feedback

Every action produces observable, accurate, timely feedback: hover states,
press states, progress, completion, cancellation, error.

*Rationale:* Truthful feedback is the foundation of trust. A user must never
wonder whether their click registered or whether a save completed.

*Anti-examples:* Fake progress bars; instant "export complete" before the
file is flushed; a disabled button that gives no reason; destructive actions
that complete silently; hover states with no press state.

## 7. Motion with meaning

Every animation answers a usability question: where did it come from? where is
it going? is the state changing? is it done? An animation that answers no
question is decoration.

*Rationale:* Motion encodes continuity and hierarchy — it is cognition, not
embellishment. The motion grammar (see `MOTION.md`) makes the answer legible
and consistent.

*Anti-examples:* Bounce-in logos; rotating progress spinners where a value
matters; slides that animate content in after a 400 ms delay; everything
pulsing on hover; modal windows that fly in from off-screen.

## 8. Accessibility is release-blocking

Full keyboard navigation, visible focus, screen-reader labels, logical focus
order, high contrast, scalable UI, reduced motion, non-color status, and
configurable shortcuts are requirements, not enhancements. This principle
always wins over aesthetics, and aesthetics must accommodate it.

*Rationale:* A suite for professional creative work is used by everyone. An
editor inaccessible from the keyboard is not a professional tool. This
principle wins over principle 1–7: a beautiful surface that fails it is
defective.

*Anti-examples:* A timeline only usable with a mouse; focus rings suppressed
for "cleanliness"; color-only status dots; text that cannot scale to 1.5×
without breaking; animations that cannot be disabled.

## 9. Professional depth, never featurelessness

Minimal does not mean shallow. Every professional workflow exists — pagination,
compound clips, pivot tables, color management, comping — and is reachable
through disclosure. Depth is hidden, never absent.

*Rationale:* Minimal-without-depth is a demo; depth-without-disclosure is a
dungeon. The suite must be a tool professionals keep, not a toy they abandon.

*Anti-examples:* An "easy" mode that cannot do the real job; hiding destructive
but necessary controls entirely; an empty panel labeled "Advanced" with nothing
in it; keyboard shortcuts that only exist in menus nobody opens.

## 10. Performance is a feature

Input feedback within one frame; 60 fps interaction; zero UI-thread blocking;
no synchronous file, decode, or inference on the UI thread. Budgets in
`PERFORMANCE.md`.

*Rationale:* Latency breaks flow. A tool that stutters reads as broken even
when the output is correct; cancellation that takes seconds reads as a lie.

*Anti-examples:* Autosave freezing the UI; thumbnails generated on the main
thread; scrolling that allocates per frame; an un-cancellable export; progress
that only updates when the work is done.

## 11. Warmth without decoration

The palette is warm (canvas `#FAF9F7`, terracotta accent `#B4552D`), type
feels human, spacing breathes. Warmth comes from material choices, not from
decorative flourish.

*Rationale:* Warm neutrals are approachable; they photograph, print, and age
well. Decoration is what makes software look dated; material warmth is what
makes it timeless.

*Anti-examples:* Skeuomorphic leather or wood textures; gradients layered on
every button; cartoon mascots; saturated "friendly" color spam; emoji as UI.

## 12. Every default is a deliberate choice

Default document settings, default zoom, default shortcuts, default colors,
default window sizes: each is designed and documented for the target workflow.

*Rationale:* Most users never change a default. Defaults are the most powerful
design decisions in the suite, and delegated defaults are how suites drift.

*Anti-examples:* "Default zoom 100% because it's round"; a default paper size
nobody uses; keyboard shortcuts assigned by proximity instead of by convention;
sample content that exists only because a placeholder was needed.


## source-loom-design-bible-design-review-md

Original path: `loom-design-bible/DESIGN_REVIEW.md`

# Design Review

The review process for any change to the design contract (this repository)
or to application UI (implemented against it). Reviews are checklists, not
opinions; every item is verifiable.

## 1. When a review happens

* Contract change: any edit to this repository (tokens, components,
  motion, accessibility, windows, surfaces) — reviewed before merge.
* Feature UI: any user-visible change in an application — reviewed by the
  app's design owner against the checklist below before CI's visual gate.
* New component or state: reviewed against `COMPONENTS.md` state matrices
  before implementation starts (design review precedes code).

## 2. Review checklist

**Contract integrity**

- [ ] Token values match `tokens/loom.toml` exactly (names, values, order).
- [ ] No new token names without the ADR path; no bespoke durations,
      easings, sizes, or colors.
- [ ] `DESIGN_TOKENS.md` and `THEMING.md` agree with the TOML; docs list
      every token they reference.
- [ ] Contrast floors verified (CI gate) for any new color or color pair.

**Visual and layout**

- [ ] Spacing drawn from the scale; no arbitrary values.
- [ ] Fixed chrome heights honored (toolbar 40, sidebar 240, inspector
      280, status 28; timeline header 160, ruler 24).
- [ ] No shadows outside `shadow-popover`; depth by color.
- [ ] Type from the scale; tabular figures for data; line length within
      45–75 ch where text is body-like.
- [ ] Iconography: 20 px grid, 1.5 px stroke, no text glyphs, labels
      present.

**Interaction and motion**

- [ ] Direct manipulation first; disclosure order respected.
- [ ] Motion uses tokens; animation answers a question; interruptible;
      reduced-motion behavior specified (default: opacity 120 ms).
- [ ] Selection visuals per `SELECTION.md` (accent 2 px outline +
      overlay); focus ring visible.
- [ ] No anti-patterns from `ANTI_PATTERNS.md` (scan the list explicitly).

**Accessibility**

- [ ] Full keyboard path (Tab order, arrows, Esc, Enter semantics).
- [ ] `accessible-description` on every control; state announced.
- [ ] Focus order and focus return specified.
- [ ] Text scale 1.25/1.5 layout check; high-contrast behavior specified;
      non-color status.

**Truthfulness and performance**

- [ ] Progress is determinate where possible; cancellation path exists.
- [ ] No UI-thread blocking in the feature's critical path; async work
      specified.
- [ ] Empty state and error state specified; no silent failures.

**Verification plan**

- [ ] Unit/property tests named; visual-QA captures listed (default,
      selected, error, reduced-motion); perf budgets named.

## 3. Process

1. Author completes the checklist with evidence (screenshots, tests,
   token diffs) — claims without evidence are treated as unverified.
2. Review by the design-system lead (contract changes) or the app design
   owner (feature UI). A second reviewer is required for any accessibility
   impact.
3. Violations are fixed or explicitly waived via ADR. A waived item stays
   listed in `KNOWN_LIMITATIONS.md` of the app.
4. Visual gate runs in Docker; the review closes only with a green gate
   for the changed captures plus its themes.
5. Every review result is recorded (short review note appended to the PR):
   verdict, checklist items, evidence links, follow-ups with IDs.

## 4. Review cadence

* Design reviews happen at design time (before implementation) and at
  merge time (verification) — never only after implementation.
* Suite-wide design reviews (cross-app consistency pass) run per release
  cycle: compare the same component across all eight applications in the
  gallery, reconcile drift, update this checklist if the review found a
  gap in it.


## source-loom-design-bible-design-tokens-md

Original path: `loom-design-bible/DESIGN_TOKENS.md`

# Loom Design Tokens

Loom has one token authority and one behavioral geometry authority:

- [`tokens/loom.toml`](loom-design-bible/tokens/loom.toml) — primitive and semantic visual values.
- [`contracts/desktop-ui.toml`](loom-design-bible/contracts/desktop-ui.toml) — component metrics, layout rules, responsive behavior, forbidden patterns, and acceptance thresholds.

[`MECHANICAL_DESIGN_STANDARD.md`](AGENTS.md#source-loom-design-bible-mechanical-design-standard-md) explains how to apply both. If a prose document duplicates a value and disagrees with TOML, the TOML value is authoritative and the prose must be corrected in the same change.

## Rules

1. Application UI consumes semantic roles or shared `loom-ui` components; it does not define local palette values.
2. Standard control sizes, radii, toolbar geometry, panel geometry, status geometry, focus treatment, and typography are shared-system values, not application choices.
3. A new token is added only when an existing semantic role cannot express the requirement. Do not add a token merely to preserve arbitrary legacy geometry.
4. Theme variants change semantic values, never token names.
5. Content-specific roles such as paper, media stage, grid, waveform, or chart series are valid only for actual content surfaces; they must not be used to decorate chrome.
6. The Loom accent is reserved for focus, selection, active/checked state, primary actions, and meaningful emphasis. It is not a background-decoration color.
7. Shadows are absent from routine chrome. The only general elevation token is for menus/popovers/tooltips. Document/media content may define content-specific elevation only through a reviewed contract extension.
8. Interactive state changes never alter layout geometry.
9. Runtime `theme.slint`, this TOML source, and the desktop contract are checked together by `loom-bootstrap/scripts/audit-product-ui.py`.

## Core numeric system

The exact values are intentionally not repeated as tables here; duplication previously allowed the documentation and runtime implementation to diverge. Agents and contributors must read the TOML sources directly.

The current system is built around:

- compact pointer/keyboard desktop controls;
- 13 logical-pixel UI labels;
- a finite spacing scale;
- 1 px separators and 2 px keyboard focus treatment;
- neutral light/dark chrome;
- high-contrast semantic variants;
- deterministic motion durations with reduced-motion replacement;
- a single original Loom icon family.

## Token-generation direction

`tokens/loom.toml` is the input contract. A generator may emit Slint constants, Rust constants, documentation tables, or design-tool exports, but generated outputs are never edited manually. Until generation fully replaces `theme.slint`, CI verifies that the runtime Slint values match the TOML contract.

A generator is considered complete only when:

- every palette role is emitted for light, dark, and high contrast;
- type, spacing, radius, border, icon, motion, and component metrics are emitted;
- invalid or missing semantic references are compile/test failures;
- applications no longer need raw design literals for standard UI;
- generated output is deterministic and checked for repository cleanliness.

## Governance

A design-token change is a product change. The commit must state:

- which semantic role changes;
- which component/application workflows are affected;
- accessibility/contrast impact;
- required reference-baseline updates;
- whether the desktop UI contract changes too.

Do not change a token solely to make one screenshot pass. Fix the component or layout unless the shared product rule itself is wrong.


## source-loom-design-bible-dialogs-md

Original path: `loom-design-bible/DIALOGS.md`

# Dialogs

Dialogs are rare by policy (`WINDOWS.md` §3). This document fixes when they
exist, how destructive actions behave, and the keyboard defaults.

## 1. When a dialog is justified

A modal dialog appears only when: the user must decide before the workflow
can continue, and the decision has consequences that cannot be inferred or
reversed cheaply. Accepted uses:

* Confirm destructive, irreversible actions (delete project, discard
  unsaved document, replace file).
* Resolve conflicts with no safe default (version incompatibility,
  media relink ambiguity).
* Enter values that require full attention and have no canvas analogue
  (print settings, export options are utility windows or panels where
  possible).

Not accepted: properties editing (inspector), formatting (toolbar), settings
(settings panel), confirmations for undoable actions (undo is the
confirmation), progress (status bar/notifications), error details
(notifications + diagnostics log).

## 2. Anatomy and behavior

* Anatomy per `COMPONENTS.md` §16: `radius-8`, `space-24` padding, min
  width 420 px, title, body, right-aligned action row.
* Backdrop dim: ink at 20% over the parent window; backdrop clicks do not
  dismiss (deliberate — a modal you must read, not swipe away).
* Only one dialog at a time; requests queue.
* Resize: dialogs may resize within 420–640 px width; body scrolls if the
  window is short; the action row never scrolls.
* Motion: fade + 4 px scale, out-quad 200 ms; exit 160 ms; reduced motion:
  instant in, fade-only out.

## 3. Destructive action confirmation

* Destructive button is **never the default**: the safe path (Cancel, or
  the constructive primary) is the default; Enter always activates the
  default (safe) button.
* Destructive confirmations state the consequence in the button itself
  ("Delete project") and repeat the consequence in the body ("This deletes
  8 media files and 120 hours of history. This cannot be undone.").
* Double-confirmation (type-to-confirm or second dialog) is used only for
  truly irreversible suite-level actions (delete project file, wipe
  recovery data); ordinary destructive edits rely on undo.
* Destructive buttons use `color-status-danger` fill with white text; the
  action row order is [Cancel] [Destructive], destructive rightmost when
  Cancel is default — and when the destructive is genuinely the only action
  (delete confirmation with "Cancel" present), Cancel stays left.
* Undoable destructive actions do not require a dialog at all (delete layer,
  delete clip: undoable, no modal) — dialog policy respects the undo system.

## 4. Keyboard defaults

| Key | Behavior |
|---|---|
| Enter | Activate default (safe) button |
| Esc | Cancel (always present unless no cancellation is possible, e.g. fatal error) |
| Tab | Move focus within dialog, wraps; focus traps while open |
| Shift+Tab | Reverse |
| Ctrl+Enter / Cmd+Enter | Same as Enter |
| Ctrl+W / Cmd+W | Cancel-and-close when the dialog can cancel |

* On open, focus goes to the first focusable control (usually the default
  button or the first field); never to the backdrop or title.
* On close, focus returns to the element that opened the dialog.
* Screen reader: dialog role announced with title; backdrop described as
  "inactive".

## 5. Recurring decision dialogs

* "Don't ask again" is allowed only when the decision is genuinely
  repeatable and reversible (checkbox in the dialog, saved to settings);
  the setting is discoverable in Preferences.
* A dialog that users dismiss via Esc more than twice in a session logs a
  diagnostics hint (privacy-preserving; no telemetry — the hint is local).

## 6. Error dialogs

* Fatal, unrecoverable errors (corrupt project at open, unrecoverable save
  failure) use a dialog because work must stop; recoverable errors use
  notifications (`NOTIFICATIONS.md`).
* Error dialogs: title states what failed in plain language, body explains
  what was lost and what the user can do (restore from recovery, relink
  media), actions offer the recovery path as default when it exists.
* Error details are available ("Show details" expands a monospace log
  block, redacted per `loom-core` diagnostics privacy rules), never a raw
  panic or stack dump by default.


## source-loom-design-bible-document-editor-md

Original path: `loom-design-bible/DOCUMENT_EDITOR.md`

# Document Editor

The document surface of Writer (and the text surfaces of Present and Photo).
This document fixes the page canvas chrome, cursor behavior, and IME rules.

## 1. Page canvas chrome

* Pages render on `color-surface-canvas`; the page itself is
  `color-surface-raised` with a 1 px hairline border (dark theme) — depth by
  color, never a drop shadow (`DESIGN_TOKENS.md` §10).
* **Page gaps**: consecutive pages separated by `space-16` vertical gaps
  (continuous mode: no gap, one flowing column); page mode shows a page
  break glyph (2 px line + "Page 2" label, `type-size-11` secondary ink)
  only when gaps are hidden.
* **Margins and text bounds**: editable text shows a subtle bounds inset
  (1 px accent at 25% when the caret or selection is inside the paragraph);
  margins visible via rulers (see §2).
* **Rulers**: visible by default in page mode; 20 px top + left ruler
  strips (`CANVAS.md` §1): page edge, margins, indents, tab stops, column
  guides; tab-stop drag handles on the top ruler; margin markers draggable
  (Option+drag moves both margins).
* **Text cursor**: 2 px wide (`border-width-strong` / 2 px), accent color,
  height = line height; blinks 530 ms on / 270 ms off while idle, **stops
  blinking while the user is typing or moving the cursor** (focus
  stability); caret color switches to a contrasting color when the caret
  rests on accent-colored text (visibility rule).
* **Selection in text**: accent fill at 25% behind glyphs (never obscures
  the glyph); selection across pages stays continuous in the logical text
  flow; screen-reader selection announcements per `ACCESSIBILITY.md`.

## 2. Cursor movement rules

* Movement is by **logical character** (grapheme cluster), honoring
  bidirectional text: arrow keys move visually in RTL paragraphs (visual
  movement with logical storage — the standard modern editor model); Home/
  End = start/end of visual line; Ctrl/Cmd+Left/Right = word; Ctrl/Cmd+Up/
  Down = paragraph; PageUp/PageDown = viewport with caret following.
* Double-click selects word; triple-click selects paragraph; Shift+arrows
  extends selection; the selection anchor never moves on its own.
* Cursor mapping between logical (UTF-16/UTF-32) offsets and visual
  positions is a tested contract (`loom-core` `loom-text`); property tests
  cover RTL and combining characters (`loom-spec` testing section).
* The caret never scrolls out of view: typing near the edge auto-scrolls
  with a 24 px margin; scroll is instant (no animation while typing).

## 3. Input and editing

* Input is composition-aware (see §5); everything typed goes through the
  document model as text operations — undoable, journaled, replayable
  (undo/recovery contract in `loom-core`).
* Typing latency budget: keystroke → glyph on screen within one frame
  (16.7 ms); layout of the visible paragraph only (incremental layout —
  never re-layout the whole document per keystroke); heavy documents
  (10,000+ paragraphs) must keep the caret line fluid while background
  layout catches up.
* Autocorrect/auto-capitalization: off by default in Loom documents
  (professional control); the features, when enabled, show a transient
  underline (ink at 40%) and a non-modal suggestion popover (Option+click
  or palette to accept) — never silent replacement.
* Smart quotes/dashes: on by default in writing apps, per locale, with an
  "undo auto-replacement" affordance (the replaced text is undoable as one
  step); straight quotes preserved in code-like contexts (formulas, HTML).

## 4. Page, layout, and long documents

* Pagination and layout run off-thread; the visible page renders from the
  committed layout; repagination is observable via a status-bar chip
  ("Paginating…") only when it exceeds 250 ms.
* Footnotes, endnotes, headers/footers, and TOC markers are layout objects:
  they render inline in the page flow; selection of a marker selects its
  content ("footnote 3").
* Find & replace: `Cmd+F` opens a non-modal find bar (in-toolbar,
  32 px): matches highlighted with accent outline (current match: accent
  fill 25% + 2 px outline); Replace panel extends the same surface; find
  never creates a dialog (`DIALOGS.md`).
* Word count and document stats live in the status bar (word count,
  page/line position of the caret, tabular figures), updated on idle.

## 5. IME composition

* Composition is a first-class state: the composing region shows a 2 px
  accent underline under the current segment; candidate windows appear
  near the caret, sized to candidate text, styled per Menu/popover rules.
* Composition state is preserved across cursor moves within the segment;
  committing a composition (Enter/Space per IME) inserts the text as one
  undoable unit; cancelling (Esc) restores the pre-composition text.
* The editor never reorders or re-layouts a composing segment mid-
  composition except for caret-driven scrolling; composition with
  bidirectional text and RTL locales is tested per `loom-spec`'s IME test
  matrix.
* On window blur with active composition: composition commits (per platform
  convention); on app crash, composition state is journaled with the
  document (recovery contract).

## 6. Accessibility

* Caret and selection are fully reported to assistive technologies: caret
  position (character + line), selection ranges, composing text.
* Typeahead/autocomplete surfaces announce options as a listbox with
  selected-state announcements.
* Text scaling: at 1.25×/1.5× the page zoom stays at document zoom — text
  scale applies to UI chrome and the editing surface's minimum text size,
  never silently to document formatting (document text scale is a document
  property, user-controlled).
* All cursor movements are keyboard-native by construction (this surface IS
  the keyboard surface); pointer users get the same model via click/drag.
* Spell-check and grammar suggestions: never modal, never color-only
  (underline + context-menu/palette action + screen-reader announcement of
  the suggestion count).


## source-loom-design-bible-drag-and-drop-md

Original path: `loom-design-bible/DRAG_AND_DROP.md`

# Drag and Drop

Drag and drop is a first-class interaction in Loom: moving clips, layers,
files, assets, and rows. The rules below make it predictable, reversible,
and never the only path.

## 1. Drag feedback

* **Origin**: the dragged item lifts within 120 ms — scale 1.02 (canvas
  objects) or a lifted row (lists), opacity 0.95, `shadow-popover` for
  lifted rows; the origin leaves a ghost (ink at 20% outline) marking the
  drop-away position.
* **Follow**: the dragged representation follows the pointer 1:1 with zero
  lag and no easing; a small chip under the cursor shows the count for
  multi-drag ("3 clips"); cursor changes to `grabbing` (custom Loom grab
  cursor, `POINTER_AND_PEN.md`).
* **Snapshots**: canvas objects drag as a live-rendered thumbnail of the
  object (Photo/Motion/Video); library assets drag as their thumbnail +
  name chip; rows drag as the row itself.
* **Cancellation**: Esc during drag cancels, animating the item back
  (out-quad 200 ms); releasing outside any target cancels with the same
  return animation; nothing is modified until the drop commits.
* **Reduced motion**: lift scale and return animation are disabled; the
  drag stays 1:1 and the item snaps back instantly.

## 2. Drop targets

* Targets highlight **before** the pointer is over them when the drag
  crosses their boundary: a 2 px accent outline + accent fill at 8% over
  the target's bounds (non-color reinforcement: the target also enlarges
  its affordance by 4 px and shows a drop-role chip: "insert after",
  "replace", "merge").
* Drop roles are explicit: each target declares what a drop means — insert,
  replace, reorder, link, copy (Option/Alt to force copy), move (default
  within a project). The role is shown in the drop chip and announced.
* Invalid targets reject visually (danger-colored "not allowed" cursor,
  no highlight) — never a silent no-op.
* Snap-to-drop: when the pointer is within 12 px of a target edge, the
  dragged item snaps to the target position (visual snap, out-quad
  120 ms) — this is drop-assist, not magnetic inertia.

## 3. Reorder

* List/panel reorder (sidebar items, sheets tabs, track headers): the
  displaced rows animate out of the way (out-quad 200 ms) as the dragged
  row passes; the landing position is marked by a 2 px accent insertion
  line (not by the dragged row "hovering" ambiguously).
* Timeline reorder: clips slide for the displaced span; the insertion
  caret (accent vertical line at the snap point) marks where the clip will
  land.
* Reorder commits on release; every reorder is undoable as one step
  (reorder history is a tested contract in `loom-core`).
* Keyboard reorder alternative: cut/paste and arrow-move commands exist for
  every reorder surface (`SIDEBARS.md` §6, `CANVAS.md` §6).

## 4. Cross-app drag

* **Files**: dragging files into any app (from the file manager) opens the
  app's import path: media into media beds, `.loomdoc` into Writer,
  `.loomphoto` into Photo — the target highlights with the accepted format
  ("Open as project", "Import media"); unsupported formats reject with the
  reason chip ("No importer for .xyz").
* **Assets between Loom apps**: shared clipboard/drag contract in
  `loom-core`: dragging an image from Photo's library into Writer drops it
  as an embedded or linked image (role chosen in the drop chip: "Link" vs
  "Copy"); dragging a Motion composition into Video inserts it as a
  compound clip; dragging an Encode preset into Video's export panel
  applies the preset.
* **Link vs copy semantics**: default is Link for project-internal and
  Media-Linked (asset stays in its library, referenced); Option/Alt forces
  Copy; the drop chip always shows which will happen. Links are relinkable
  (`loom-core` media contract).
* **Cross-app drag never requires a running second app**: files are the
  transport (drag a `.loomdoc` file to the desktop = the document).

## 5. Drag and accessibility

* Drag is never the only way to accomplish a move: every drag operation
  has a keyboard equivalent or a menu/palette command
  (`ACCESSIBILITY.md` §keyboard).
* Screen readers announce drag initiation and drop results ("Moved layer 3
  after layer 5"); during drag, the drop role and target name are
  announced when focus is in the drag source's context.
* Drag initiation requires a deliberate press-move: no drag on click; a
  pure click with a 0 px drag never lifts the item (click-to-select stays
  intact).
* Multi-select drag: dragging any selected item drags the whole selection;
  the count chip announces.

## 6. Drag data and safety

* Drag payloads are validated on drop, not on drag start: dropping a
  corrupt file shows the standard import-error toast, never a crash.
* Dragging over other applications is standard OS drag (mime types per
  the clipboard contract); dragging into Loom from any app follows the
  file rules above.
* All drags are cancellable (Esc), reversible (undo), and observable
  (status bar chip shows the operation: "Moving 3 clips…").


## source-loom-design-bible-iconography-md

Original path: `loom-design-bible/ICONOGRAPHY.md`

# Iconography

The original Loom icon family. All icons are drawn for this suite; no
proprietary symbol sets, no traced commercial icons, no OS glyph sets.

## 1. Grid and geometry

* **ViewBox**: 20 × 20 px (`icon-viewbox-20`). All icons are authored on this
  grid and rendered at integer multiples of 20 (20, 40, 60… at 1×, 2×, 3×).
* **Stroke**: 1.5 px (`icon-stroke-width-15`), round joins and caps, uniform
  across the family. Filled icons are the exception and must be approved
  (e.g. status dots are 4 px fills, never strokes).
* **Corner radius**: 2 px (`icon-corner-radius-2`) for shapes with corners
  (e.g. tool-tile, photo frame).
* **Optical alignment**: no ink within 1.5 px of the viewBox edge on any side;
  vertical/horizontal visual centering is by optical mass, not bounding box.
  Alignment guides at the 20, 40, 60, 80, 100% marks of the grid (i.e. at
  4, 8, 12, 16 px) for consistent placement of common elements.
* **Stroke width on interaction**: icons never change stroke weight by state;
  states are color, rotation (spinner only for pending), and check overlay.

## 2. Style rules

* Geometric, not sketched: icons are built from lines, arcs, and rectangles
  with rational centerlines; no freehand shapes, no gradients, no drop shadows
  on icons.
* Optical weight is uniform: a toolbar icon at 20 px should not read heavier
  than its neighbor. Design-time check: at 16 px rendering (0.8×), all icons
  must remain legible (test in gallery snapshot set).
* Metaphor discipline: use the suite's own metaphors where they exist (Loom
  canvas, loom threads in empty states); keep generic metaphors (open, save,
  undo) conventional so users recognize them instantly.
* Localization: icons carry no text (no letters in icons) except where the
  glyph is universal (e.g. play triangle, print glyph is avoided — use a
  printer-shape icon instead of the letter glyph).

## 3. Required icon inventory

Shared (all applications): new, open, save, save-as, export, import, undo,
redo, cut, copy, paste, delete, duplicate, search, filter, zoom-in,
zoom-out, zoom-fit, full-screen, help, settings, shortcuts, commands, close,
minimize, maximize, restore, collapse-panel, expand-panel, back, forward,
chevron-down/left/right/up, overflow-ellipsis, menu, more, history, lock,
unlock, warning, error, info, success, spinner, link, unlink, external,
plus, minus, reset, refresh, check, check-circle, eye, eye-off, grid,
list, sort-ascending, sort-descending, pin, pin-off, window, tabs, alert,
bookmark, star, trash, folder, folder-open, file, file-text, calendar, clock.

Writer: paragraph, character-style, page, section, column, table, insert-row,
insert-column, footnote, endnote, header, footer, toc, citation, comment,
track-changes, hyphenation, text-wrap, master-page, template, mail-merge,
field, cross-reference, image-anchor.

Sheets: cell, formula, function, sum, average, filter-row, freeze-pane,
merge-cells, split-cells, sort, pivot-table, chart-bar, chart-line,
chart-pie, conditional-format, data-validation, named-range, goal-seek,
audit-trace, error-trace.

Present: slide, new-slide, layout, theme, master-slide, transition,
animate, presenter-mode, rehearsal, notes, align, distribute, group,
ungroup, order-front, order-back, guides, shapes, equation.

Photo: crop, rotate-ccw, rotate-cw, flip-h, flip-v, brush, eraser, heal,
clone, gradient, eyedropper, levels, curves, exposure, white-balance, mask,
selection-rect, lasso, magic-wand, perspective, liquify, warp, layer,
layer-group, adjustment, opacity, blend-mode, before-after.

Motion: composition, keyframe, keyframe-hold, keyframe-ease, playhead,
loop, ping-pong, play, pause, stop, record, parent, constrain, path,
bezier, camera, light, particle, replicate, graph-editor, motion-blur,
render-queue.

Video: timeline, blade, ripple, roll, slip, slide, trim-in, trim-out,
overwrite, insert, connected-clip, compound-clip, multicam, sync, proxy,
optimize, transcode, waveform, caption, subtitle, scene-detect, track,
audio-role, title, generator, stabilize, color-wheel, scopes, luts.

Studio: note, piano, drum, metronome, tempo, marker, loop-region, record-arm,
solo, mute, fader, pan, send, bus, sidechain, plugin, instrument, sampler,
automation, comp, take, flex-time, pitch, mixer, master, loudness, key,
scale, chord, drummer.

Encode: queue, job, preset, pause-job, resume-job, retry, format, codec,
bitrate, frame-rate, resolution, hdr, subtitle-track, audio-map, watch-folder,
cli, hardware-accel, software-fallback, quality-meter, comparison.

Vision: scan, ocr, document-detect, barcode, qr, face, pose, segment,
matting, track-point, plane, flow, search-image, model-pack, model, provider.

## 4. Icon states and use

* Toolbar and tool icons: 20 px, default `color-ink-secondary`, hover and
  active `color-ink-primary`, selected/tool-active `color-accent-default`.
* Status icons: 16 px where inline, 20 px where standalone; always paired
  with text per `COLOR.md` §6.
* Menu icons: 16 px.
* Disabled icons: 40% opacity of their default color (never a different color).

## 5. Accessibility

* Every icon has an `accessible-description` (via its control's label or a
  dedicated accessibility name). Purely decorative icons are marked
  decorative and skipped by the screen reader.
* Toolbar icon buttons must have a visible tooltip name (label) and shortcut
  hint; the tooltip appears within 400 ms of hover and is keyboard-reachable
  (focus shows the same hint in the status bar).
* Icon-only controls must never be the only path to a command: the same
  command exists in a menu or the command palette with a text name.

## 6. Source and review

* Icons are authored as editable SVG sources (20 px grid), committed in
  `loom-core`'s asset crate with per-icon test fixtures.
* Every icon ships with a snapshot test (render at 20 px and 16 px, software
  renderer) and a label-convention lint (accessibility name present, no text
  glyphs).
* New icons are reviewed against §1–§2 by the design-system lead before
  merging; a new icon without a snapshot test is not accepted.


## source-loom-design-bible-inspectors-md

Original path: `loom-design-bible/INSPECTORS.md`

# Inspectors

The contextual inspector is the third layer of progressive disclosure: it
shows properties of the current selection in a structured, searchable panel.
One model across the suite; applications do not fork it.

## 1. Placement and anatomy

* Right dock by default; width 280 px, resizable 240–360 px; persists per
  app. Never coexists with the sidebar on the same side.
* Toggle: toolbar `inspector` IconButton; shortcut `Cmd+Option+I`
  (Windows: `Ctrl+Alt+I`); collapses/expands the panel (out-quad 200 ms,
  reduced motion: instant).
* Anatomy: header 40 px (title = current selection summary, e.g. "Text box
  — 2 selected", plus collapse); body is a single scrolling column of
  **sections**.

## 2. Sections

Order per selection type, fixed within an app and documented in the app's
PRODUCT_SPEC:

1. **Object** — geometry, position, name, transform; the manipulation
   primitives of the selection.
2. **Style** — visual properties: fill, stroke, type style, effects.
3. **Document** — properties that affect the document/page/project context
   (page size, margins, master, layers settings).
4. **Metadata** — name, description, tags, dates, source, sync/relink state.
5. **Advanced** — everything else: scripting hooks, format-specific options,
   hidden format controls. Collapsed by default; expanded via its header.

Rules:

* Empty selections show **no** sections (the inspector shows an empty state
  with a hint: "Select an object to edit its properties").
* Sections are shown only if applicable: a text layer shows Object + Style
  + Metadata; a project-wide selection shows Document + Metadata.
* Each section has a summary line of the current key value when collapsed
  (e.g. Style: "Fill #B4552D · Weight 1.5").
* Sections never reorder on selection change; the scroll position and
  expanded sections persist across selection changes (stability rule).
* No tab bar inside the inspector; one scrolling column, search above it.

## 3. Property rows

* Two-column rows: label (left, `type-size-13` secondary ink) and control
  (right, aligned to a shared control column edge).
* Row group headers (`type-size-11` caps) only where a section is large
  (transform → position/size/rotation).
* Rows update live from the selection: changing an object's position in the
  canvas updates the row within 120 ms — the inspector is a view of the
  document, not a buffer (no "apply" buttons).
* Numeric rows: SpinBox with tabular figures; scrubby labels (drag label to
  adjust, Shift = ×10, Option = ×0.1) — direct manipulation in the
  inspector.
* Color rows: swatch + hex; swatch opens a color-well popover
  (`COMPONENTS.md`, popover rules) with eyedropper.

## 4. Search

* A search field pinned above the sections. Typing filters property rows by
  label and value across all sections (fuzzy substring, case-insensitive);
  matching sections open, non-matching collapse; matches are highlighted
  with accent.
* Search never hides sections permanently: clearing the query restores the
  prior expanded state.
* Keyboard: search is reachable via `Cmd+F` while the inspector has focus
  or via the palette; Esc clears search, then closes.
* Search is discoverable: the field shows "Search properties…" placeholder
  and a magnifier icon; it is one of the inspector's three persistent
  affordances (search, pin, collapse).

## 5. Pinning

* `pin` IconButton in the section header pins a section: the pinned section
  stays expanded and scrolls to remain visible (or the inspector splits into
  two columns: pinned section above, scrolling remainder below — the
  simpler v1 behavior: pinned sections rise to the top and never collapse
  until unpinned).
* Pinning is per-app-persistent and shows in the section header as an accent
  pin glyph; pinned sections are announced to screen readers ("Section
  pinned").

## 6. Keyboard operation

* Inspector is a focus group: Tab enters, arrows move rows, Space/Enter
  activate the focused control, Esc leaves.
* Arrow keys move focus between rows in a section; collapsed sections are
  skipped; Home/End jump to top/bottom of the panel.
* All rows are reachable: sliders with arrows, SpinBox with typed input,
  ComboBox with arrows+Enter; nothing requires a mouse.
* Focus ring per the state model; row focus shows the row's label read by
  the screen reader ("Position, X, 120").

## 7. Updating and performance

* The inspector updates on selection change within 200 ms (standard token);
  updates animate as 120 ms fade for value text (reduced motion: same, it's
  opacity).
* Never block input while refreshing; refresh is incremental, not a panel
  rebuild.
* At 1.5× text scale the inspector floor width rises to 320 px so
  label+control rows remain usable.


## source-loom-design-bible-keyboard-md

Original path: `loom-design-bible/KEYBOARD.md`

# Keyboard

Keyboard operation is a release requirement. This document fixes the shared
key map, per-app additions, and the shortcut configuration policy.

## 1. Convention

* Primary modifier is `Cmd` on macOS, `Ctrl` on Linux/Windows (written
  `Cmd`/`Ctrl` below as `Mod`).
* `Mod+Shift+P` opens the command palette everywhere; `F1` opens help;
  `F10`/`Alt` reveals menu mnemonics on Linux/Windows.
* Shortcut hints render in menus, tooltips, and the palette footer
  (`MENUS.md`, `COMMAND_PALETTE.md`).
* All shortcuts are **configurable** (§5); defaults below are the contract.

## 2. Standard editing keys (all apps)

| Action | Key |
|---|---|
| Undo / Redo | `Mod+Z` / `Mod+Shift+Z` (Linux also Ctrl+Y) |
| Cut / Copy / Paste | `Mod+X` / `Mod+C` / `Mod+V` |
| Paste without formatting | `Mod+Shift+V` |
| Delete selection | `Backspace` / `Delete` |
| Select all | `Mod+A` |
| Find | `Mod+F` (surface-specific: document find, palette find, inspector search) |
| Preferences | `Mod+,` |
| Command palette | `Mod+Shift+P` |
| Zoom in/out/fit/100% | `Mod+Plus` / `Mod+Minus` / `Mod+0` / `Mod+1` |
| Toggle sidebar / inspector | `Mod+\` / `Mod+Option+I` (Win: `Ctrl+Alt+I`) |
| Full screen | `F11` (Linux/Win), `Ctrl+Cmd+F` (macOS) |
| Focus search | `Mod+Shift+F` |
| Close window/doc | `Mod+W` |
| Save | `Mod+S` (Save As: `Mod+Shift+S`) |
| New / Open | `Mod+N` / `Mod+O` |
| Show shortcuts help | `Mod+?` (also Help menu) |

## 3. Per-app keys

**Writer**: arrows/Home/End/PageUp/PageDown per `DOCUMENT_EDITOR.md` §2;
`Mod+B/I/U` bold/italic/underline; `Mod+K` insert link; `Mod+Shift+S`
save-as; `Mod+E` center-aligned text? — no: align via `Mod+Shift+L/C/R`
(par. align left/center/right); headings: `Mod+Alt+1..6`; lists
`Mod+Shift+7`/`Mod+Shift+8` (bulleted/numbered); footnote `Mod+Alt+F`;
comment `Mod+Option+M` (macOS `Mod+Alt+M`); track changes toggle
`Mod+Shift+E`; find next/prev `Enter`/`Shift+Enter` in find bar.

**Sheets** (`SPREADSHEET.md` §3 table is the core): cell editing `F2`,
go-to `Mod+G`; insert row/col `Mod+Shift+K` / `Mod+Shift+K` (col variant
`Mod+Option+K`); delete row/col `Mod+Delete`; formula entry `=`;
autofill: select + fill-handle drag (pointer); keyboard fill `[goal]`;
filter toggle `Mod+Shift+L`; freeze toggle `Mod+Alt+F`; sheet cycle
`Ctrl+PageUp/PageDown`; recalc manual `F9` (auto by default).

**Present**: next/prev slide `→`/`←` (or Space/Shift+Space in edit mode:
`→` steps slides, `Shift+→` selects next object); present mode
`Mod+Shift+Enter`; add slide `Mod+Enter` (or `Mod+M`); duplicate slide
`Mod+Shift+D`; notes panel `Mod+Option+N`; rehearse `Mod+Shift+R`.

**Photo**: tool shortcuts (single letters, non-modifier): V select, M
marquee/lasso, C crop, B brush, E eraser, H hand, R rotate, G gradient,
I eyedropper, T text, Z zoom (with Mod = zoom out), A magic-wand,
L levels, W white-balance; bracket keys `[`/`]` brush-size; `X` swap
foreground/background colors; `Mod+Shift+N` new layer; `Mod+J` duplicate
layer; `Mod+G` group; `Mod+E` merge layers; `Mod+Shift+E` flatten; `/`
toggle mask overlay; `Mod+Shift+M` show/hide mask.

**Motion/Video**: Space play/pause; J/K/L scrub (`TIMELINE.md` §4);
`Mod+Right/Left` next/prev edit point (Shift adds 1-frame step); `Mod+B`
blade at playhead; `Mod+Shift+B` ripple delete at playhead; `Mod+D`
duplicate clip; `Mod+[`/`]` trim to playhead; `Mod+Shift+E` open export
queue; `Mod+1..9` jump to marker 1–9 (Option+1..9 set marker); `N`
toggle snapping; `Mod+Option+B` set loop range; `G` toggle graph editor
(Motion); `P` pick parent (Motion); `K` pause.

**Studio**: Space play/pause; `Mod+T` metronome; `Mod+R` record-arm
toggled; `Mod+K` create automation point; `Mod+Shift+A` show automation;
`Mod+1..8` mute track 1–8 (add Option for solo); `[`/`]` zoom time;
`Mod+Plus/Minus` zoom; `F` follow playhead; `Shift+Space` half-speed;
`Option+Arrow` nudge note 1 semitone/1 grid step (in piano roll).

**Encode**: Space start/pause selected job; `Mod+Return` start job;
`Mod+Shift+Return` start all; `Esc` stop selected job (confirm only if
mid-write); `Tab` moves focus between queue columns; `Mod+O` add sources;
`Mod+P` open presets panel.

**Common canvas keys**: `V` selection tool in canvas apps; `H` hand;
`Space` temporary hand (hold); `Z` zoom tool (Mod inverts); `Mod+Alt` + drag
duplicate (Photo/Motion); arrows nudge (Shift ×10).

## 4. Focus and scope

* Shortcuts apply to the focused surface: a text field consumes typing keys;
  JKL transport only applies when the timeline or canvas has focus (typing
  in a text field never scrubs); the rule is: surface-local keys are active
  when their surface has focus, global keys (`Mod+S`, palette) always.
* The focused surface is visually indicated (focus ring); the status bar
  shows the active surface ("Timeline").
* Tab navigation order is per `ACCESSIBILITY.md` §focus-order: chrome →
  toolbar → canvas/editor surface → panels in the order they were opened;
  Tab groups per surface (toolbar as one group, sidebar as one group).
* No key is reserved twice for different actions in the same surface
  (conflict detection is a config-time validation, §5).

## 5. Shortcut configuration policy

* All shortcuts are user-configurable via a shortcuts panel (searchable
  list, per-app and global sections, import/export as JSON — the exported
  file is user-owned data).
* Configuration applies on the fly; conflicts are reported with the
  conflicting action and the user resolves or cancels; configurable
  shortcuts never disable accessibility-critical keys (Tab, Space in text
  fields, Enter, Esc, arrows) — these are not overridable.
* Defaults are reset-able per app and globally; the config file lives in
  user settings with the same redaction/privacy rules as all Loom data.
* Layout variants: a keyboard layout (e.g. Dvorak) maps by key position;
  AZERTY users get mnemonic-safe defaults (`[goal]` — first release ships
  US/QWERTY defaults + configuration, layout-aware remapping is a goal).
* Screen-reader note: shortcut hints are read to screen-reader users; the
  shortcuts panel is fully keyboard-operable and announced.


## source-loom-design-bible-layout-md

Original path: `loom-design-bible/LAYOUT.md`

# Loom Layout Contract

Exact values and responsive thresholds are machine-readable in [`contracts/desktop-ui.toml`](loom-design-bible/contracts/desktop-ui.toml). This document defines composition. Application layouts may not invent a second geometry system.

## Canonical shell objects

Every shell region has one named Slint owner in `loom-ui/ui/toolkit.slint`:

| Role | Canonical object | Compatibility names |
|---|---|---|
| title/document chrome | `TitleChrome` | `DocumentChrome` |
| 40 px context toolbar | `ContextToolbar` | `Toolbar { labeled-slot: false }` |
| 48–52 px labeled toolbar | `LabeledToolbar` | `Toolbar` (legacy default) |
| compact icon action | `IconOnlyToolbarItem` | `ToolbarIconButton` |
| icon-over-label action | `IconOverLabelToolbarItem` | `AppleToolbarItem` |
| Sheets tabs | `SheetTabStrip` | `TabStrip` |
| formula/name row | `FormulaBar` | — |
| inspector section | `InspectorSection` | — |
| property row / field | `PropertyRow` / `Field` | `TextField` |
| status / overflow | `StatusBar` / `Overflow` | `ToolkitStatusBar` / `ToolbarOverflowButton` |

Compatibility names are inheritance or re-export shims only. They do not own
geometry, colours, focus, or interaction state.

## Shared vertical chrome

A normal Loom document window is composed in this order when those regions apply:

```text
Document/title chrome
Context toolbar or app-specific persistent control row
Primary work area
Status bar
```

The title region and toolbar are separate semantic regions even when a platform later integrates them visually.

`ContextToolbar` is exactly 40 px high. A toolbar containing an
`IconOverLabelToolbarItem` must explicitly use `LabeledToolbar` (48–52 px);
hosts may not rely on child content to stretch a context row.

Rules:

- chrome is compact and neutral;
- the document/project name is the useful title;
- the application name is not repeated as a large banner inside the window;
- persistent "Local", package-format, prototype, or implementation-status badges are forbidden unless the state changes a user decision;
- the primary work area stretches before any decorative region;
- no persistent overlay covers a canvas, stage, viewer, grid, or document page;
- below a comfortable width, optional panels collapse before primary content violates its minimum size.

## Shared horizontal work-area grammar

When all regions exist:

```text
optional left sidebar | primary work surface | optional right inspector
```

The sidebar and inspector use contract widths and resize ranges. They are flush surfaces separated by hairlines. They are not floating rounded cards.

The primary surface must meet the per-app `primary-share-min` rule at the reference viewport. If it cannot, collapse optional chrome rather than squeeze controls or content.

## Application shells

### Writer

```text
Title 40
Formatting/context toolbar 40
Document canvas: flexible, dominant
Status 28
```

No sidebar or inspector is open by default. Pages are centered inside the flexible canvas with contract fit margins. Formatting chrome must not consume enough width to clip; lower-priority commands overflow.

### Sheets

```text
Title 40
Formula bar 32
Virtualized grid: fills width and height
Sheet tabs 30
Status 28
```

The visible grid expands to the viewport. Empty space after an arbitrary fixed column count is a defect, not intentional whitespace.

### Present

```text
Title 40
Context toolbar 40
Slide navigator 220 default | Stage flexible | Inspector 280 optional
Status 28
```

Navigator and inspector collapse before the stage becomes unusably narrow.

### Photo

```text
Title 40
Context toolbar 40
Tool rail 40 | Image canvas flexible | Layers/Inspector 280-ish
Status 28
```

Tool/mode/status hints are chrome or transient gesture feedback, never permanent image overlays.

### Motion / Video

```text
Title 40
Context toolbar 40
Browser 240 | Viewer flexible | Inspector 280 optional
Shared timeline >= 220 high
Status 28
```

Transport/timecode is placed in chrome. Timeline controls do not occupy the ruler's content lane. Viewer, media browser, and timeline must agree about loaded project/media state.

### Studio

```text
Title/transport chrome
Track headers 180 | Arrangement flexible
Optional mixer/inspector
Status 28
```

Track names may ellipsize with tooltip. Transport and command labels never truncate.

### Encode

```text
Title 40
Queue toolbar / batch controls
Queue 280 | Selected-job settings flexible/300
Status 28
```

Job identity and destination/settings have greater hierarchy than progress percentage. Progress is a property of the selected job or batch, not a hero element.

## Responsive algorithm

At every required viewport:

1. Reserve mandatory title/status regions.
2. Reserve the primary work surface minimum.
3. Allocate required fixed chrome.
4. Allocate optional panels at preferred width.
5. Shrink optional panels toward their minimums.
6. Collapse optional panels if the primary surface would otherwise violate its minimum.
7. Apply toolbar priority collapse/overflow.
8. Never reduce action controls below their contract dimensions.
9. Never wrap a toolbar.
10. Never overlap or clip a control to satisfy width.

The shared `ResponsivePolicy` owns the two transitions. Below 1180 px P1
actions use compact icon-only targets; below 1320 px P2 actions move to the
single overflow command. The required transition probes are 1179, 1180, 1279,
1280, 1319, and 1320 px. 1279/1280 are stability probes, not an additional
breakpoint.

The bootstrap UI audit builds a geometry manifest at every probe, required
viewport, direction, and 1.0/1.5 text scale. It asserts rectangle bounds,
positive-area overlap, toolbar line count, label-fit budget, and primary-surface
minimums independently of PNG hashes.

This ordering is mandatory. Arbitrary `compact-layout` branches that merely change paddings without following this ordering do not satisfy the contract.

## Scaling

Required release matrix: all contract viewports at text scale 1.0, plus reference/stress captures at 1.25 and 1.5. 2.0 is the accessibility stress target.

Larger text does **not** automatically multiply every chrome dimension. Controls remain coherent desktop controls; when labels no longer fit, responsive composition changes, rows stack where allowed, and lower-priority toolbar actions overflow. Content zoom is independent of UI text scale.

RTL is a first-class direction probe. `DirectionalLayout` keeps stable
`HorizontalLayout` geometry while swapping logical leading/trailing insets;
applications that need distinct side-panel placement must provide an explicit
`root.rtl` branch. The shared probe preserves command order, focus order, and
the same geometry budgets, and no app-local padding or colour override is
permitted.

## Alignment

- sibling controls align to the same baseline/center line;
- panel and toolbar edges align to shared boundaries;
- no arbitrary floating margins in chrome;
- text baselines, not box tops, govern mixed control/label alignment;
- hairlines are one logical pixel;
- every persistent 1 px separator must land on a deterministic logical boundary under the software renderer.


## source-loom-design-bible-mechanical-design-standard-md

Original path: `loom-design-bible/MECHANICAL_DESIGN_STANDARD.md`

# Loom Mechanical Design Standard

This document is the normative implementation procedure for Loom UI. It exists so a coding agent with **no image understanding** can produce and validate the same interface geometry as a visual designer.

Machine-readable values live in [`contracts/desktop-ui.toml`](loom-design-bible/contracts/desktop-ui.toml). Primitive and semantic theme values live in [`tokens/loom.toml`](loom-design-bible/tokens/loom.toml). If prose and TOML disagree, TOML wins and CI must report the drift.

The target is not an imitation of proprietary assets or source. Loom uses the desktop interaction discipline associated with first-class macOS creative applications: useful document titles, restrained chrome, content-first hierarchy, compact pointer/keyboard controls, predictable panels, deliberate toolbar grouping, direct manipulation, and exhaustive state polish. Loom keeps its own iconography, accent, branding, and application identity.

## 1. Definition of polished

"Polished" is not a subjective acceptance criterion. A surface passes only when all of the following are true:

1. Every dimension belongs to the contract or is content-derived.
2. No control, action label, section heading, or persistent status indicator overlaps or clips at any required viewport/text-scale combination.
3. No toolbar wraps or scrolls. Lower-priority actions move to overflow before available width becomes insufficient.
4. User-authored names may ellipsize; action labels and control labels may not. Ellipsized user content exposes its full value by tooltip/accessibility description.
5. Primary work content receives the minimum share specified for the application in `desktop-ui.toml`.
6. Every interactive control has default, hover, pressed, keyboard-focus, and disabled behavior; selectable controls add selected/unselected; inputs add valid/invalid.
7. Every icon-only action has an accessible name and tooltip and is reachable through the same command registry as menus/keyboard shortcuts.
8. No visible control is a placebo. Disabled functionality is visibly disabled and explains why.
9. No application draws its own generic buttons, text fields, segmented controls, panel shells, title chrome, toolbar shells, inspector shells, status bars, menus, or popovers.
10. A human-approved baseline exists for realistic content, empty state, and error state. Baselines containing known defects are invalid evidence.

A screenshot matching an unreviewed baseline is **not** proof of quality.

## 2. Geometry authority

All measurements are logical desktop pixels.

### Window and chrome

- Title/document chrome: 40 px.
- Context toolbar: 40 px.
- Status bar: 28 px.
- Panel header: 32 px.
- Standard control: 28 px high.
- Compact control: 24 px high.
- Prominent control: 32 px high.
- Standard icon: 16 px.
- Default pointer target: 28 × 28 px.
- Hairline/separator: 1 px.

The app name is not useful document hierarchy and must not be repeated as a large title inside the window. The document/project name is the primary title. Neutral implementation facts such as "Local", "Editing locally", or package type do not occupy permanent title chrome. Surface them only when they change a decision or explain a state.

### Spacing

Allowed chrome/component spacing values are exactly:

`2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48, 64`.

No 5 px, 7 px, 9 px, 10 px, 11 px, 14 px, 15 px, 18 px, 26 px, 28 px padding/gap values may appear in application chrome merely because they "look right". If a missing value is genuinely necessary, change the contract and justify it once.

### Typography

- UI labels: 13 px / medium (500).
- Section headings: 13 px / semibold (600).
- Caption/status: 11 px.
- Small metadata: 12 px.
- Body/help prose: 14 px.
- Subtitles: 16 px.
- Content headings: 20 or 24 px only where content hierarchy requires them.
- Numeric readouts use tabular figures.
- Bold 700 is reserved for document content or exceptional hierarchy; routine chrome should not look shouted.

Text size changes must not alter button height, reorder controls, or create a second toolbar line. At larger text scales the responsive policy moves lower-priority controls into overflow.

## 3. Surface hierarchy

Use only these hierarchy levels:

1. **Canvas/work area** — user content and the background immediately around it.
2. **Chrome** — title/toolbar/status regions.
3. **Panel** — sidebar/inspector/timeline headers and bodies.
4. **Raised control** — buttons, fields, selected rows, menus.
5. **Overlay** — menu, popover, modal, tooltip.

Do not create a rounded card for each section. Sidebars and inspectors are flush work surfaces separated by hierarchy, hairlines, spacing, and section headers. Rounded containers are reserved for controls, real card-like objects, menus/popovers, and modal surfaces.

Content owns contrast. Chrome is deliberately neutral. The Loom terracotta accent is restricted to focus, selection, active/checked state, primary action, and meaningful emphasis. It is not decoration.

## 4. Toolbar grammar

Toolbars are the highest-risk source of current Loom defects, so they follow an algorithm rather than free placement.

### 4.1 Maximum structure

A toolbar has at most three groups:

- leading: navigation/document structure;
- center: commands for current content/selection;
- trailing: view/search/export/overflow.

Groups are separated by 12 px. Items within a group use 4 px. A toolbar never wraps and never scrolls.

### 4.2 Priority algorithm

Every toolbar item has priority 0, 1, or 2.

- **P0:** always visible at every supported width.
- **P1:** visible with label at wide width; icon-only below 1180 px when the icon is unambiguous and a tooltip exists.
- **P2:** moves to the overflow menu below 1320 px.

If P0 still cannot fit, remove redundant toolbar actions and keep them in menus/command palette. Never solve width pressure by clipping text, shrinking controls below contract size, negative spacing, or drawing over adjacent controls.

### 4.3 Command placement

Every toolbar action exists in the command registry and therefore can also appear in the menu/command palette. Save As, infrequent import variants, advanced export settings, and similar secondary commands normally belong in menus rather than permanent toolbar slots.

Text-labeled actions are separated from unrelated icon-only groups. Actions never ellipsize.

## 5. Panels and inspectors

Sidebar default: 240 px, range 200–360 px.

Inspector default: 280 px, range 240–360 px.

Panel padding: 12 px. Section gap: 16 px. Property rows are at least 28 px high. Property labels reserve 84–112 px; values consume remaining width. If a localized property label cannot fit, the row becomes stacked rather than truncating the label.

Only the panel body scrolls. Nested section scrollbars are forbidden. Sections may collapse, but the user must not encounter scroll-within-scroll for normal property editing.

User-authored layer, track, file, and item names may ellipsize. Inspector commands, field labels, and section headings may not.

## 6. Canvas and direct manipulation

Persistent status UI must not cover a canvas. Play/pause state, mode name, zoom, timecode, snapping, and transport state live in chrome unless they are transient feedback tied to a direct manipulation gesture.

Canvas invariants:

- minimum useful viewport: 480 × 320 px;
- zoom-to-fit leaves 24 px minimum margin;
- selection outline: 1 px;
- visible transform handle: 8 px, hit target 20 px;
- snap distance: 6 px;
- guides: 1 px;
- pan/zoom never mutates document geometry;
- selection changes never create undo entries;
- a drag is one coalesced undoable operation;
- Escape restores pre-gesture geometry;
- keyboard nudge follows the same snapping/undo model.

No canvas is considered complete because it can draw content. It needs selection, hit testing, transform, keyboard operation, context commands, accessibility semantics, and persistence-aware undo.

## 7. Timeline grammar

Motion, Video, and Studio share timeline primitives rather than drawing three unrelated timelines.

- minimum timeline height: 220 px;
- track header: 180 px;
- ruler: 24 px;
- default track: 44 px; compact: 32 px;
- clip corner radius: 4 px;
- playhead: 1 px visual with a wider hit region;
- trim handle: 4 px visual / 12 px minimum hit region.

Track names may ellipsize only with tooltip/accessibility text. Tool names, transport commands, and ruler labels cannot collide with tracks. Timeline controls live outside the ruler row unless they are ruler interactions.

## 8. Application shell contracts

### Writer

Persistent structure: document title → context formatting toolbar → document canvas → status bar. No sidebar or inspector by default. The page occupies the visual focus. Formatting applies to text selection/caret state, never "all paragraphs" unless explicitly invoked as a document-wide command.

### Sheets

Persistent structure: document title → formula bar → grid → sheet tabs → status bar. The grid fills available viewport width and height and virtualizes beyond visible cells. A fixed eight-column demo grid with unused canvas is a contract violation.

### Present

Persistent structure: title/toolbar → 220 px slide navigator → flexible stage → optional 280 px inspector → status. Slide thumbnails and inspector are collapsible before the stage falls below its minimum useful width.

### Photo

Persistent structure: title/toolbar → compact tool rail + flexible image canvas + layers/inspector → status. Tool/mode hints cannot permanently overlay the image. Direct manipulation is primary; inspector controls mirror selection state.

### Motion / Video

Persistent structure: title/toolbar → media/browser + viewer + inspector → shared timeline → status. Transport/timecode is chrome, not a canvas overlay. The project cannot claim media sources are empty while sample timeline clips exist.

### Studio

Persistent structure: title/transport → track headers + arrangement → optional mixer/inspector → status. Track names may ellipsize; all transport/action labels remain complete.

### Encode

Persistent structure: title/queue controls → queue + selected-job settings → status. Progress is subordinate to job identity and output settings; a giant isolated percentage is not the visual hierarchy.

## 9. Accessibility and platform behavior

Loom is a pointer/keyboard desktop suite. Standard controls use 28 × 28 px targets; specialized dense affordances may render as small as 20 px only when their actual hit target remains at least 28 px. Body/control text maintains at least 4.5:1 contrast; non-text controls and focus indication maintain at least 3:1.

Every operation available from a toolbar has a keyboard/menu path. Focus order follows visual reading order. Focus indication is 2 px and must remain visible in all themes. State is never communicated by color alone.

Text-scale tests are 1.0, 1.25, 1.5 for every release; 2.0 is the accessibility stress target. High contrast and reduced motion are first-class variants rather than afterthoughts.

## 10. Motion

- hover: 0 ms onset;
- press: 0 ms;
- micro state transition: 120 ms;
- ordinary transition: 180 ms;
- panel transition: 220 ms;
- routine interaction maximum: 250 ms;
- reduced motion: 0 ms for spatial/decorative motion.

Repeated professional controls must never make users wait for animation. No bounce is used for routine chrome.

## 11. Non-visual QA procedure

An agent without vision follows this exact sequence:

1. Read `contracts/desktop-ui.toml` and `tokens/loom.toml` before editing UI.
2. Use only shared `loom-ui` components for standard chrome and controls.
3. Run the design-system contract audit. Any literal palette or component metric drift fails.
4. Build the component gallery/smoke surface for light, dark, and high-contrast.
5. Capture required viewport/text-scale matrix.
6. Run deterministic image-diff against **approved** baselines. A changed image requires explicit baseline review; the agent may not bless its own change by simply replacing the baseline.
7. Run geometry/state assertions: no overlap, action clipping, toolbar wrap, or primary-work-surface violation.
8. Run keyboard/accessibility journeys for each changed component.
9. Run realistic-content, empty-state, and error-state fixtures.
10. Only then migrate an application to the new toolkit.

A no-vision agent is allowed to know that its screenshot differs; it is not allowed to infer that a difference is attractive. It must satisfy the contract and approved baseline or escalate the baseline for review.

## 12. Migration order

Application feature expansion pauses where it would create more legacy UI. The productization reset proceeds in this order:

1. Synchronize token sources and runtime theme.
2. Make `loom-ui` the sole owner of standard controls/chrome.
3. Build a complete component gallery/state matrix.
4. Add strict contract and baseline validation.
5. Replace title chrome and toolbar composition across all apps.
6. Migrate Writer and Sheets document/grid primitives.
7. Migrate Photo canvas/layer/inspector primitives.
8. Migrate Present/Motion/Video onto shared scene/timeline/direct-manipulation primitives.
9. Migrate Studio timeline/mixer primitives.
10. Migrate Encode queue/settings primitives.
11. Resume broad feature work only through toolkit components.

A migration step is complete only when old app-local implementations are deleted, not merely hidden behind a new component.


## source-loom-design-bible-menus-md

Original path: `loom-design-bible/MENUS.md`

# Menus

Menus are the fourth layer of progressive disclosure. They are complete —
every command exists in a menu — but calm: restrained grouping, no redundant
nests, mnemonics only where useful.

## 1. Menu bar model

* Menu bar row (24 px) below the title bar: File, Edit, View, (App-specific
  — e.g. Insert in Writer/Sheets/Present, Layers in Photo, Clip in Video,
  Track in Studio), Selection, and the suite's Help.
* On platforms with a global menu bar (macOS), the same commands live in the
  platform menu; the in-app menu bar is hidden. Linux/Windows: in-app menu
  bar visible, hideable per user setting (default visible; palette and
  `Alt` still reach everything).
* The menu bar is the last-chance discoverability surface: every command
  that has a shortcut shows it right-aligned in the menu item
  (`color-ink-secondary`, tabular figures).

## 2. Menu item anatomy

* Item rows 28 px: icon 16 px (optional, aligned left), label
  `type-size-13`, shortcut hint right, checkmark for toggles (checked =
  accent check), submenu chevron right.
* Separators: hairline, only between groups that genuinely differ (undo/
  redo vs clipboard vs find is over-separated; group by action class).
* Disabled items: 40% opacity, never removed (consistency), with reason
  tooltip where non-obvious.
* Destructive items (Delete Clip, Discard): label in danger color only when
  the command is destructive and irreversible without undo — most
  destructive commands have undo and stay neutral; see `DIALOGS.md`.

## 3. Menu interactions

* Open: click or `Alt+Underline` mnemonic; opens within 120 ms (out-quad,
  4 px slide; reduced motion: fade).
* Keyboard: arrows move; Enter/Space activate; Esc closes one level;
  Left/Right navigate submenus; mnemonics activate directly.
* Hover over another top-level item while a menu is open switches to it
  immediately (no hover delay, no animation between top-level switches).
* Mouse-up on the trigger closes the menu without activating; clicking an
  item activates on mouse-up.
* Menus never cascade more than one submenu level (two levels total).
  Deeper structure is a redesign signal.

## 4. Context menus

* Right-click context menus show the commands applicable to the hovered
  object/selection, in the object's command order, plus: undo/redo (top,
  when relevant), Copy/Cut/Paste, and a "Properties…" entry opening the
  inspector section.
* Context menus never contain disabled-only commands; irrelevant commands
  are absent.
* Keyboard access to context menus: the context-menu key (or Shift+F10)
  opens at the focused object; keyboard-opened context menus have focus and
  are navigable.
* Context menus open instantly (no delay); dismiss on outside click, Esc,
  or click-away, 120 ms fade.

## 5. Mnemonics policy

* Mnemonics (underlined letters) are provided on the Linux/Windows menu bar;
  macOS uses platform conventions.
* Mnemonic letters are assigned from a policy: first letter, then unique
  consonant, then unique letter — deterministic, never auto-assigned by
  "find first unused".
* UI labels never encode mnemonic letters in the visible string (no
  parentheses); the mnemonic is a separate localization string.

## 6. Accessibility

* Menu bar and all menus are fully keyboard-operable; focus is visible on
  menu items (accent row fill 15% + primary ink).
* Every menu item announces its state (checked/disabled) to screen readers.
* Menu labels are localized; keyboard equivalents follow the shortcut
  configuration policy (`KEYBOARD.md` §5) — configurable, never hard-coded
  in menus.
* No menu item is reachable only by mouse; no command exists only in a
  context menu (palette covers all commands; see `COMMAND_PALETTE.md`).

## 7. Per-app menu contract

File: New, Open, Open Recent, Save, Save As, Export, (Import), Close, Quit.
Edit: Undo, Redo, Cut, Copy, Paste, Paste Special, Delete, Select All,
Find, Preferences. View: toggle toolbar/sidebar/inspector/status bar,
zoom controls, full screen, theme. App menu: the application's insert/
create surface (Insert, Layers, Clip, Track, Slide, Song). Selection:
Select All, Deselect, Invert, (Expand/Contract). Help: Documentation,
Shortcuts, Diagnostics Log, About.

Every app's menu inventory is enumerated in its PRODUCT_SPEC with its
`FEATURE_MATRIX`; the Bible fixes the pattern above.


## source-loom-design-bible-motion-md

Original path: `loom-design-bible/MOTION.md`

# Motion

Every animation in Loom must answer a usability question: origin, destination,
state change, hierarchy, completion, cancellation, relationship. Motion that
answers no question is decoration and is prohibited.

## 1. Tokens

Durations (ms):

| Token | Value | Use |
|---|---|---|
| `motion-duration-instant` | 0 | Hover on/off, press feedback, focus visibility changes |
| `motion-duration-fast` | 120 | Tiny state changes: checkmarks, icon swaps, toggles, progress pulses |
| `motion-duration-standard` | 200 | **Default**: panel transitions, selection moves, tool state changes |
| `motion-duration-deliberate` | 320 | Larger surfaces: sidebar expand/collapse, dialog fade, drag reorder |
| `motion-duration-slow` | 500 | Emphasis moments only: undo reveal, achievement of a long action; never per-frame UI |

Easings (cubic-bezier):

| Token | Control points | Use |
|---|---|---|
| `motion-easing-out-quad` | (0.33, 1.00, 0.68, 1.00) | **Default**: entrances, exits, state changes — quick start, calm finish |
| `motion-easing-in-out` | (0.65, 0.00, 0.35, 1.00) | Value transitions: progress fills, cross-fades, scrolling to a snap |
| `motion-easing-out-back` | (0.34, 1.56, 0.64, 1.00) | Spring-ish arrival, sparingly: command palette open, panel "landing" |

Rules: 200 ms default, 120 ms minimum for non-instant feedback, 500 ms maximum
for any single interaction animation. No custom durations or easings anywhere
in the suite — bespoke values require an ADR.

## 2. Motion grammar

* **Entrance** (panel opens, palette appears): out-quad, 200 ms; slight
  overshoot via out-back only for the command palette and popover "landing"
  (320 ms). Elements never fly from off-screen.
* **Exit** (panel closes): out-quad, 160 ms — exits are 20% faster than
  entrances (attention is already on the object; exit is confirmatory).
* **State change** (toggle, mode switch, checkmark): fast, 120 ms, usually
  opacity + small scale (1.0 → 1.05 → 1.0) on the state glyph only.
* **Selection** (selection moves across items, marquee completes): standard,
  200 ms, out-quad. The selection outline and overlay animate from the
  previous selection rect to the new one (see `SELECTION.md`).
* **Progress** (job progress, export, render): in-out, fill animations only,
  value changes snap the bar to the new fraction within fast (120 ms) —
  progress never bounces or overshoots, and never loops decoratively.
* **Drag feedback** (object, clip, row): the dragged item scales to 1.02 and
  lifts (opacity 0.95) within 120 ms; it follows the pointer 1:1 with zero
  lag — no easing on drag-follow. Drop landing animates 200 ms out-quad.
* **Reorder** (list row, track order): displaced items slide out-quad 200 ms;
  the dropped item settles 320 ms with a 2 px settle-overshoot via out-back
  only for timeline/arranger reorders.
* **Zoom** (canvas zoom, spreadsheet zoom): zoom follows the pointer; the
  content transform animates in-out 200 ms when triggered by a control, but
  is **instant** while the user is actively zooming (wheel/pinch).

## 3. Interruption

* Every animation is interruptible on the frame it is interrupted: a new
  gesture starts the new animation from the current state — no "finish
  current animation first" delays, no queued animations.
* Hover-based motion (tooltip, preview) is cancellable and never traps the
  pointer.
* Input latency is never sacrificed to finish an animation: if a 120 Hz
  pointer stream arrives during a 200 ms panel animation, the panel snaps to
  the target and input wins.
* Keyboard interaction interrupts animations the same way pointer input does.

## 4. Reduced motion

In reduced-motion mode (user preference, applied automatically and
switchable in settings):

* All translation and scale animations are disabled: panels appear and
  disappear instantly (`motion-duration-instant`), no slides, no overshoot,
  no lifts, no spring.
* Opacity changes remain, capped at `motion-duration-fast` (120 ms): fades
  convey state change without motion.
* Progress continues to animate fill changes at 120 ms (value feedback, not
  decoration).
* Hover feedback is instant color/opacity only.
* Reduced motion is a release gate in every app: verified with a dedicated
  visual-QA pass and automated assertion that no transform-animated element
  translates or scales in this mode.

## 5. Frame-drop policy

* Animations are driven per-frame from the compositor; at 60 Hz target, a
  frame that takes longer than 16.7 ms drops rather than stalling the next
  frame — animation time is wall-clock based, so dropped frames never
  accumulate delay.
* Below 45 fps sustained (e.g. software renderer in CI), animations still
  complete in wall-clock time; the UI never depends on frame count.
* The software-renderer visual QA path uses wall-clock timing and captures
  final states (reduced motion: instant), so baselines are deterministic.

## 6. Motion checklist (every feature)

1. Does the motion answer a usability question? If not — cut it.
2. Which token duration/easing is it? (No bespoke values.)
3. Is it interruptible and input-responsive?
4. What happens in reduced motion? (Default: opacity 120 ms or instant.)
5. Does it scale? (UI text scaling never animates; panel heights adjust
   instantly.)


## source-loom-design-bible-notifications-md

Original path: `loom-design-bible/NOTIFICATIONS.md`

# Notifications

Notifications report outcomes without stealing focus. Two surfaces share the
job: **toasts** for transient, attention-worthy outcomes; the **status bar**
for ongoing and background work. Both obey the non-color status rules in
`COLOR.md` §6.

## 1. Surface selection

| Condition | Surface |
|---|---|
| Task completed while user is idle in another area (export done, render finished) | Toast |
| Task failed; retry or recovery available | Toast (error style) + entry in Diagnostics |
| Ongoing background work with progress (import, proxy, encode queue) | Status bar progress + cancel |
| Transient status (autosave, relink, zoom reset) | Status bar message, auto-clears |
| Fatal, work-stopping error | Dialog (`DIALOGS.md` §6) |

Rules: no more than one toast visible per app at a time (queued); status bar
shows one primary progress job (others collapse to a "3 jobs" entry opening
the jobs panel); toasts never stack with dialogs.

## 2. Toast anatomy and behavior

* Top-right, width 320 px, `radius-8`, raised fill, `shadow-popover`;
  icon (16 px) + message (`type-size-13`, primary ink) + optional action
  button + close button.
* Severity: info (default), success, warning, error — icon + text per
  `COLOR.md` §6; never color-only.
* Auto-dismiss: info 6 s, success 4 s, warning 10 s, error persists until
  dismissed (errors also live in the Diagnostics log with full details).
* Action buttons in toasts: single action ("Open", "Retry", "View log");
  clicking dismisses the toast after running.
* Motion: entrance out-quad 200 ms (fade + 4 px slide from the edge);
  exit 160 ms; auto-dismiss fades 160 ms; reduced motion: fades only.
* Accessibility: toasts announce via live region (polite); focus does not
  move on toast arrival; toast action buttons are keyboard-reachable while
  visible; Esc dismisses the focused toast.

## 3. Status bar reporting

* Left region: primary status/progress line. Text `type-size-11`, secondary
  ink; progress uses the mini ProgressBar (80 × 6 px) + fraction in tabular
  figures + cancel button (always, for cancellable jobs).
* Right region: readouts (zoom, snapping, transport). Never more than 3
  readout groups.
* Status messages auto-clear after 4 s or when superseded; progress rows
  persist until the job ends.
* The status bar never shows errors in red alone; error text uses danger
  ink + alert icon and offers "Details" opening the Diagnostics log.

## 4. Error reporting

* Recoverable errors (import failure, file locked, model missing) go to a
  warning/error toast with a retry/recovery action; the full message lives
  in Diagnostics.
* Error messages follow a fixed shape: what failed, what was affected, what
  to do next. Example: "Import failed: 'clip.mov' is corrupt. Nothing was
  imported. Try re-exporting the file." Never: "Error 0x4F2 — internal
  error."
* Unexpected internal errors are logged to Diagnostics with a stable error
  ID and shown as a compact toast ("Unexpected error — see Diagnostics").
* No network-dependent behavior: error reporting is local and offline
  (`loom-spec` privacy contract).

## 5. Recovery prompts

* After a crash or interrupted session, the app opens the Recovery browser
  (per `loom-core` recovery contract). Presentation: a modal-style entry
  surface (recovery list) — designed as a dialog by default, listing
  recovered documents with timestamps and "Keep / Discard / Review"
  actions.
* Recovery prompts are explicit, never silent: the user chooses what to
  keep; nothing is deleted automatically at session start.
* If no recovery data exists, no prompt appears (no dead-end "welcome
  back" screens).

## 6. Autosave and background-job notifications

* Autosave is silent by default: the status bar shows a brief "Saved
  HH:MM" (4 s) after the first save of a session; failures are errors
  (toast + persist).
* Background jobs (proxy generation, thumbnails, indexing) never toast on
  success; they report progress in the status bar and toast only on
  failure with a retry action.
* Cancellation is immediate and announced in the status bar ("Import
  cancelled") — never a toast for user-initiated cancellations.


## source-loom-design-bible-performance-md

Original path: `loom-design-bible/PERFORMANCE.md`

# Performance

UI performance budgets and the discipline around them. These are design
contracts: the design language assumes them (motion, selection, scrubbing,
typing all specify one-frame latency), so the budgets are binding.

## 1. Hardware tiers

Budgets are defined per tier; applications report against the tier they
target (mainstream is the default gate).

| Tier | Example | UI target |
|---|---|---|
| Baseline integrated | Intel/AMD iGPU laptop, 1080p | Functional 30 fps UI, responsive input |
| Mainstream desktop | Mid-range dGPU, 1440p | **60 fps UI, one-frame input** |
| High-performance workstation | Top dGPU, 4K HDR | 60 fps at 4K, 120 Hz-friendly architecture |
| CPU-only compatibility | Software rendering in CI/VM | Deterministic renders, animations complete in wall-clock time |

## 2. Input and interaction budgets

* **Input feedback within one display frame** (16.7 ms @ 60 Hz, 8.3 ms @
  120 Hz): hover, press, selection change, slider thumb, caret move.
* **Animations run at 60 fps**: 200 ms standard animations render at ≤
  16.7 ms/frame on mainstream; no frame-budget overrun may cause visible
  stutter on the budget tier (frame-drop policy per `MOTION.md` §5).
* **No UI-thread blocking, ever**: file I/O, media decode, parsing,
  inference, indexing, autosave, export, thumbnail/waveform generation run
  off-thread (jobs framework, `loom-core`). A blocking path that exceeds
  16.7 ms in a known critical path is a release-blocking defect
  (`loom-spec` release criteria).
* **Scrolling and virtualized surfaces allocate zero per frame**:
  timelines, spreadsheets, lists, and canvases reuse buffers; GC/alloc
  pauses are not acceptable in scroll paths (alloc-profile gate).
* **Cancellation feedback appears immediately**: cancel input → visible
  acknowledgement within one frame; the underlying job stops within 100 ms
  of the acknowledgement (best effort) and reports cancellation
  (`NOTIFICATIONS.md` §6).

## 3. Startup budgets

* **Warm launch < 1 s** for lightweight apps (Sheets, Writer, Encode) from
  app start to usable main window; **< 2 s cold launch where feasible**
  (cold = no caches).
* Heavy apps (Video, Studio, Photo, Motion): main window interactive
  within 1.5 s; project/media loading continues asynchronously with
  progress (never a blocking splash).
* Startup must not require network access; offline startup is a test gate
  (`loom-bootstrap` offline suite).

## 4. Memory budgets

* Bounded memory via documented cache policies: thumbnail caches (LRU with
  byte budget), waveform caches, decoded-frame pools, undo history budgets
  (memory + disk-backed modes per `loom-core` history contract).
* Representative workloads define the budgets per app (`PERFORMANCE.md`
  benchmark projects in `loom-samples`): e.g. a 1 M-cell sheet, a 10,000-
  clip timeline, a 4 GB photo folder — each app documents its workload
  budget in its PERFORMANCE.md.
* Leak detection: long-session tests (open/close 100 documents, run 500
  undo cycles) assert RSS returns to baseline within 5%.
* GPU memory: bounded by cache policies; the renderer reports GPU memory
  use in diagnostics.

## 5. Background-task interference

* Background tasks (autosave, thumbnails, proxies, indexing, model
  inference) yield to direct manipulation: interactive-frame budget wins;
  tasks are cancellable and prioritized (`loom-core` jobs contract).
* Autosave never visibly interrupts editing (silent path,
  `NOTIFICATIONS.md` §6); typing latency is unaffected during autosave
  (verified in the perf suite).
* Background-task progress is observable: status bar + jobs panel; a task
  that starves input is a defect.

## 6. Measurement and gates

* Benchmarks run in the Docker environment on the mainstream tier profile
  (and CI-software tier for determinism): cold/warm launch, window
  creation, document open, large-document scroll, recalc, scrub, waveform
  generation, thumbnail generation, export, save, undo, search index,
  memory, GPU memory, background interference.
* Regressions beyond configured thresholds fail CI or require a reviewed
  waiver (`loom-bootstrap` perf gate).
* Budgets are declared per workload in each application's
  `PERFORMANCE.md`; the Bible fixes the UI-level budgets above, which
  override nothing and are overridden by nothing less strict.


## source-loom-design-bible-pointer-and-pen-md

Original path: `loom-design-bible/POINTER_AND_PEN.md`

# Pointer and Pen

Pointer targeting, cursors, and pen-input goals for Loom.

## 1. Targeting

* **Minimum interactive target: 44 × 44 px** (logical px at 1.0 scale) of
  effective hit area for every pointer target — buttons, handles, sliders,
  list rows, tabs, scrollbar thumbs.
* Effective area = visual area + invisible grace zones: a 32 px toolbar
  button gets 6 px grace on each side; a 8 px canvas handle gets 18 px
  grace (48 px effective, per `SELECTION.md` §1).
* Grace zones must not overlap a neighboring target's grace zone in a way
  that makes the wrong target win: when they would overlap, the visual
  target grows or spacing increases (`space-8` minimum between adjacent
  icon buttons).
* Small-but-prevalent objects (8 px handles, 9 px keyframe diamonds) are
  exempt from the 44 px rule for *positioning* but must keep ≥ 24 px
  effective hit areas and work at 1.5× scale; the exemption is documented
  per object type in `COMPONENTS.md`/`TIMELINE.md`.
* Hover activation: click target = the object; hover activation of
  sub-actions (row action buttons) requires the sub-target itself to be
  ≥ 44 px effective and to appear with hover AND focus
  (`ANTI_PATTERNS.md` #hover-only).
* Precision: at high zoom, canvas targets scale with zoom — hit boxes stay
  1:1 with the rendered object, so precision improves naturally; UI chrome
  targets never scale.

## 2. Cursors

The Loom cursor set (original designs, per `ICONOGRAPHY.md` art rules):

| Cursor | Use |
|---|---|
| arrow | Default over chrome and non-interactive canvas |
| text (I-beam) | Text fields, document text, in-cell editing |
| hand-open | Pan available (Space held) |
| hand-grabbing | While panning |
| crosshair | Marquee, draw tools, pen tools |
| move | Dragging objects, reorder |
| resize-e/w/ne-sw/nw-se/ew/ns | Canvas handles, panel edges, row/col resize |
| cell-cross | Spreadsheet grid (over cells) |
| not-allowed | Invalid drop target, disabled action |

Rules: cursors are small (16–20 px), stroke-weight 1.5 px family,
high-contrast variant inverts to white-on-black; cursor hot-spot at the
logical tip; cursors never animate (spinning state cursors prohibited —
use progress UI instead).

## 3. Pointer buttons and modifiers

* Left: select/manipulate. Middle: pan (canvas), autoscroll (document
  surfaces). Right: context menu.
* Shift: constrain (aspect, axis, 15° rotate steps, straight lines).
* Option/Alt: duplicate-on-drag (Photo/Motion), fine-tune (0.1 px nudge,
  scrub ×0.1), alternative drop role (`DRAG_AND_DROP.md` §4).
* Mod: additive selection, zoom out (with zoom tool).
* Wheel: scroll; Shift+wheel horizontal; Mod+wheel zoom (canvas apps);
  plain wheel zooms in Present edit mode only.
* Trackpad: scroll pans, pinch zooms (anchor at pinch center), two-finger
  scroll honors platform natural direction; edge-scroll while dragging
  (rate proportional to distance from edge, 50 px/frame max).

## 4. Pen input (goals)

* Pen support is a `[goal]` across canvas apps (Photo first): hover
  preview (pen-over-canvas shows the tool stroke cursor), pressure maps
  to brush size/opacity per tool, tilt/rotation supported where the
  backend reports them.
* Pen vs mouse: when a pen is in range, the UI shows pen-optimized cursors
  (no resize-cursor conflicts), palm rejection (ignore touches within
  400 ms of pen-down when the platform reports palm events), and the
  drawing tool activates without a click-to-focus step.
* Pen targeting: pen hit areas are the same as pointer (§1); small drawing
  tools scale the stroke preview with pressure, never the hit area.
* Erase/side-switch: pen barrel buttons toggle eraser where the platform
  exposes them; configurable in the pen settings section of Preferences.
* Reduced motion: pen strokes are instantaneous (they ARE the input);
  hover previews fade only (120 ms).
* Verification: pen goals are tracked in `FEATURE_STATUS.md` as
  `[goal]` until a device-backed test exists in CI (`[future]` hardware).


## source-loom-design-bible-selection-md

Original path: `loom-design-bible/SELECTION.md`

# Selection

Selection is the primary state of direct manipulation: what the user means
by their next action. The visuals and rules below are suite-wide; canvas
apps, grids, and lists adapt the primitives but never invent their own.

## 1. Visual language

* **Canvas selection** (objects, clips, cells, slides): accent outline
  2 px (`border-width-strong`, `color-accent-default`) around the object
  bounds, plus a soft overlay: accent fill at 8% inside the outline for
  filled objects, accent at 15% behind the outline for wireframes.
* **Outline offsets**: 0 px for objects with their own border (snaps to
  bounds); 1 px outward otherwise; the outline never obscures the object's
  own content.
* **Handles**: 8 × 8 px squares (`color-surface-raised` fill, 1 px accent
  border) at the 8 handles of the bounds (4 corners, 4 edges); rotation
  handle above top-center with a 24 px arm. Handles appear on selection and
  are always ≥ 6 px of interactive area (48 px target per
  `POINTER_AND_PEN.md` hit rules — 8 px visual + 20 px grace each side).
* **List/grid selection** (sidebar items, sheets cells, timeline clips):
  row fill accent 15% + accent text; never full accent fill.
* **Focus vs selection**: selection = accent outline/overlay; keyboard focus
  = focus ring (2 px accent ring, 2 px offset). A selected object that is
  not focused shows outline only; the ring appears when the object or its
  handles receive keyboard focus.

## 2. Selection model

* Click selects; Shift+click adds/toggles; drag on empty canvas marquee-selects
  (see §5); Cmd/Ctrl+click toggles without clearing (selection additive);
  clicking empty canvas clears (with undo-able state — selection changes are
  recorded as a discrete command where the app model supports it).
* Parent/group selection: clicking a group member selects the member; double-
  click selects the group (and re-click drills in); pressing Esc selects the
  parent, Esc again clears.
* Locked/hidden objects are excluded from hit-testing; locked objects are
  unselectable but remain visible; a locked object in a selection shows
  lock glyphs on its handles.
* Selection persists across mode switches (select a clip, switch to the
  scissors tool: selection remains, tool applies to it).

## 3. Multi-select

* Multi-selection outline: a single bounding box around all selected
  objects, with per-object outlines at 40% opacity under it; the bounding
  box carries the handles.
* Inspector shows aggregate values: identical properties show the value;
  mixed properties show a dash ("—") and editing applies to all.
* Multi-select ordering: selection order is preserved (Shift+click order)
  and exposed to alignment/distribution commands; the last-clicked object is
  the "anchor" for alignment and scale operations.

## 4. Keyboard selection

* Arrows nudge the selected object(s) 1 px (Shift: 10 px); Option/Alt+arrows
  nudge 0.1 px at high zoom.
* Tab / Shift+Tab moves selection to next/previous sibling (object order);
  arrows also move the caret in text; per-surface key maps in `KEYBOARD.md`.
* Select All (`Cmd+A`), Deselect (Esc or `Cmd+Shift+A`), Invert
  (`Cmd+Shift+I` where supported).
* Keyboard-selected objects announce their name and type via
  accessible-description ("Layer 3, rectangle, selected").

## 5. Marquee

* Drag on empty canvas (with the selection tool) draws a marquee: 1 px
  accent outline, ink 10% fill; live preview of what will be selected
  (objects the marquee intersects highlight at 60% opacity as it grows);
  release commits.
* Intersection rule: an object is selected when the marquee contains its
  bounds center (common default) — for canvas apps, contained-by-marquee for
  shapes (design intent), intersect-for-clips (timeline precedent); the rule
  is fixed per surface in the app's PRODUCT_SPEC.
* Marquee is keyboard-reachable: Shift+arrows extend selection by marquee
  equivalent (`[goal]` in canvas apps; mandatory in Spreadsheet where cell
  range selection uses Shift+arrows natively).

## 6. Selection motion and feedback

* Selection change animates: the outline morphs from previous bounds to new
  bounds, out-quad 200 ms (reduced motion: instant). During a marquee drag,
  outline follows the marquee instantly (no animation while dragging).
* The animation is skipped when many objects (≥ 50) change selection at once
  (batch rule: animate ≤ 50, snap beyond).
* Selection is announced to screen readers as a concise summary ("3 layers
  selected") — announcements are debounced 300 ms to avoid chatter.

## 7. Non-visual selection channels

* Status bar shows the selection summary ("3 layers · 2 shapes, 1 text").
* Inspector reflects the selection per `INSPECTORS.md`.
* Where color-blindness affects selection visibility (accent on accent
  fills), the overlay + outline double-channel (fill + outline + handles)
  keeps selection distinguishable; high-contrast theme uses black outline
  with white handles inside an accent outline.


## source-loom-design-bible-sidebars-md

Original path: `loom-design-bible/SIDEBARS.md`

# Sidebars

The collapsible sidebar hosts the application's navigational and asset
surfaces: media libraries, project panels, layers, clips, pages, sheets,
scenes, tracks, and jobs.

## 1. Placement and anatomy

* Left dock by default (right dock allowed per user setting; never both at
  once with the inspector on the same side).
* Default width 240 px; resizable 180–400 px; width persists per app.
* Anatomy: column surface (`color-surface-raised`) with panel stack; each
  panel has a 40 px header (panel title, collapse chevron, optional pin
  button) and a body. The stack's top panel is the primary navigator for
  the app; the rest are contextual.
* A hairline separates the sidebar from the canvas.

## 2. Collapse behavior

* Toggle: toolbar `collapse-panel` IconButton; shortcut `Cmd+\` (Windows:
  `Ctrl+\`). Collapse animates the width to 0 (out-quad 200 ms); the canvas
  expands. Reduced motion: instant.
* Collapsed state: the sidebar is fully hidden; the toggle remains in the
  toolbar. No auto-peek on hover (hover-reveal prohibited).
* Restoring returns the last width, not a default.
* Auto-collapse at minimum window size (`LAYOUT.md` §2): when the window
  cannot fit sidebar + minimum content, the sidebar collapses and shows a
  transient toast ("Sidebar hidden to fit window"), restorable.
* Per-panel collapse: each panel header collapses its body to a 40 px header
  row; multiple panels can be collapsed; the last collapsed panel state
  persists per app.

## 3. Panel stacking

* Panels stack vertically with hairline separators; no overlap, no floating
  panels inside the sidebar (floating panels are `[future]` via WINDOWS.md
  utility windows).
* Stack order persists per app; users may reorder panels by drag within the
  sidebar (reorder feedback per `DRAG_AND_DROP.md` — 200 ms settle).
* Panel bodies scroll independently, but the sidebar does not nest scroll
  regions inside scroll regions: a panel body scrolls, the sidebar column
  itself never scrolls.

## 4. Resize

* Drag handle: the sidebar's inner edge (toward the canvas), 4 px hit area
  (2 px visual + 2 px grace), resize cursor; width clamps to 180–400 px with
  live redraw (instant, no animation during drag; settle at release).
* During drag, the canvas reflows continuously at 60 fps; no re-layout
  flicker, no async relayout.
* Keyboard resize (`[goal]`): `Alt+Left/Right` adjusts width by `space-8`
  steps when the sidebar has focus; announced via status bar.

## 5. Content patterns

* **Media/asset libraries** (Photo layers, Video clips, Studio tracks,
  Sheets sheets, Writer pages): ListItems with thumbnail rows (thumbnail
  32 × 32 `radius-4`, name `type-size-13`, secondary metadata line
  `type-size-11`), selection per `SELECTION.md`, drag out per
  `DRAG_AND_DROP.md`.
* **Search** inside libraries: a search field pinned at the panel top
  (filter-as-you-type, 120 ms debounce, results keep selection context).
* **Empty states** in panels: compact EmptyState variant with an action
  ("Import media", "Add layer").
* **Progress**: long-running library jobs (imports, thumbnails, proxies)
  show a compact progress row at the panel bottom with cancel; never block
  browsing (jobs are async; `loom-core` jobs contract).

## 6. Accessibility

* Sidebar toggle is keyboard-reachable from everywhere (toolbar shortcut);
  focus moves into the sidebar on open, returns to the canvas on close.
* Panel headers are focusable; chevron + title + pin have labels; collapsed
  panels are announced.
* Sidebar item drag is keyboard-alternative-able: items can be moved via
  cut/paste or a context-menu move command — drag is never the only path.
* At 1.5× text scale, sidebar width floor rises to 260 px so labels remain
  legible.


## source-loom-design-bible-spacing-md

Original path: `loom-design-bible/SPACING.md`

# Spacing

Spacing is the cheapest way to communicate structure. This document defines
the spacing scale, its roles, and the patterns that use it.

## 1. The scale

| Token | Value | Primary use |
|---|---|---|
| `space-2` | 2 px | Internal hairlines, focus-ring offsets (≥ 2 px from control edge) |
| `space-4` | 4 px | Dense control insets, dot markers, icon-to-icon gaps in compact rows |
| `space-6` | 6 px | Compact gaps between inline controls in one control group |
| `space-8` | 8 px | **Standard gap**: control-to-control, icon-to-label, list rows |
| `space-12` | 12 px | Control-to-field-group, list padding, table cell padding |
| `space-16` | 16 px | **Panel padding**, dialog padding, card padding |
| `space-20` | 20 px | Section spacing inside panels, between field groups |
| `space-24` | 24 px | Window/chrome padding, sidebar panel padding |
| `space-32` | 32 px | Page-level grouping, between major regions |
| `space-40` | 40 px | Generous whitespace: hero states, onboarding |
| `space-48` | 48 px | Between primary regions (toolbar → content) in large windows |
| `space-64` | 64 px | Maximum required gap; window-scale separation |

Rules: sizes below `space-8` never separate independent controls; sizes above
`space-24` never appear inside a single control. If a layout "needs" 10 px,
use `space-12` — the scale is a discipline, not a suggestion.

## 2. Patterns

* **Inset pattern**: control content inset = control padding; control padding
  = 4 px within a 32 px-tall control (toolbar buttons are 32 px with 4 px
  inset, yielding a 24 px icon/action area).
* **Label pattern**: label to control `space-8`; label column to control
  column `space-12`; field-group to field-group `space-20`.
* **List pattern**: row content padding `space-8` vertical, `space-12`
  horizontal; icon to text `space-8`; section label to list `space-8`.
* **Panel pattern**: panel padding `space-16`; header to body `space-8`;
  header icon to title `space-8`; panel to panel `space-0` (stacked flush)
  separated by a hairline; panel-group to panel-group `space-20`.
* **Dialog pattern**: dialog padding `space-24`; content to actions
  `space-24`; action button to action button `space-8` (`DIALOGS.md`).
* **Toolbar pattern**: control-to-control `space-8`; group-to-group
  `space-16`; toolbar left inset `space-16`; right inset `space-16`.

## 3. Anti-patterns

* Padding values "slightly more than 16": the scale is fixed; a 20 px
  requirement inside a panel means the layout is wrong, not the token.
* Centering arbitrary content to "fill space" (use `space-32`/`space-48`
  groupings instead of random margins).
* Negative margins and absolute-positioned layout gymnastics to defeat the
  scale: components are authored to the scale; if a component cannot be
  authored to the scale, the component spec is wrong — fix `COMPONENTS.md`,
  not the layout.
* Different apps using different paddings for the same element: panel padding
  is `space-16` everywhere. Per-app drift in spacing is a review-blocking
  defect.

## 4. Spacing and text

* Line-height ratios in `TYPOGRAPHY.md` are per text token, not derived from
  the spacing scale; UI rows stack with `space-4`–`space-8` gaps above
  line-height breathing room.
* Vertical rhythm in document content (Writer) is a document-style property
  (paragraph spacing), governed by `loom-spec`, not by UI spacing tokens.
* At 1.25×/1.5× text scale, spacing tokens are unchanged (text grows inside
  its box); only chrome heights may grow (`TYPOGRAPHY.md` §8).


## source-loom-design-bible-spreadsheet-md

Original path: `loom-design-bible/SPREADSHEET.md`

# Spreadsheet

The grid is Sheets' primary surface: a virtualized, formula-driven, fully
keyboard-navigable workspace.

## 1. Grid chrome

```
┌────────────┬──────────────────────────────┐
│ formula bar│ fx =SUM(A1:A12)              │  (32 px)
├────────────┼──────────────────────────────┤
│  row       │  column headers (28 px)      │
│  headers   ├──────────────────────────────┤
│  (40 px)   │                              │
│            │         cell grid            │
│            │                              │
└────────────┴──────────────────────────────┘
```

* **Formula bar**: 32 px row above the headers: cell reference label (left,
  tabular figures) + formula input (the same TextField affordances as cell
  editing; see §5). The formula bar is always visible in Sheets.
* **Column headers**: 28 px, letters (A, B, …, Z, AA…), freeze-capable;
  selected columns highlight with accent-tinted header fill (15%).
* **Row headers**: 40 px wide, numbers, freeze-capable; height 28 px default
  (rows are resizable 16–512 px via the header edge drag).
* **Cell grid**: the largest surface; gridlines ink at 8%; cells
  `color-surface-canvas` background with `color-surface-raised` content
  wells only where a cell is in edit mode.
* **Freeze panes**: freeze rows/columns via a drag handle at the header
  intersection (crosshair cursor); frozen regions get a 2 px accent divider
  line; unfreeze via the same handle (drag back) or a header menu command.
* **Sheet tabs**: bottom tab bar (28 px) with sheet names, + button,
  scroll arrows; tab underline 2 px accent for the active sheet (TabBar
  component, `COMPONENTS.md` §19). Reorder by drag (200 ms settle);
  keyboard: Ctrl+PageUp/PageDown cycles sheets.
* **Status bar**: cell-mode indicator (Ready/Enter/Edit), selected-range
  summary ("=SUM 3 cells"), zoom readout; per `LAYOUT.md` §7.

## 2. Cell selection

* Click selects a cell; drag extends the range; Shift+arrows extend;
  Shift+click sets the range end; Cmd/Ctrl+click adds disjoint selections
  (`[goal]` in v1 — disjoint selection is a pro feature, tracked).
* Selected range: accent outline 2 px around the whole range; active cell
  (top-left anchor) shows the white fill + accent outline; fill-handle
  (bottom-right 8 × 8 px square, accent) drags to autofill.
* The selection outline stays inside the viewport edges (clamped); moving
  the active cell past the edge auto-scrolls (edge-scroll rate 50 px per
  frame at 60 fps, proportional to distance from edge).
* Selection is announced to screen readers on change ("A1 through C5
  selected").

## 3. Navigation (keyboard)

| Key | Behavior |
|---|---|
| Arrows | Move active cell 1 cell (Shift: extend range) |
| Tab / Shift+Tab | Right / left one cell (enters or exits selection per mode setting) |
| Enter / Shift+Enter | Down / up one cell, committing edit |
| Home | Column A (Cmd: A1) |
| Ctrl/⌘+Home | A1 |
| PageUp/PageDown | Viewport-height jump, active cell moves with view |
| Ctrl/⌘+Arrow | Jump to the edge of the data block (hold Shift to extend) |
| Ctrl/⌘+G | Go To (dialog: cell reference) |
| F2 | Enter edit mode on active cell |
| Esc | Exit edit / cancel edit / (in selection) move to cell-move mode |

Numeric entry starts editing and committing on Enter — a full cell editor
(`[goal]`: input-box overlay with formula highlighting).

## 4. Virtualization and performance

* The grid is virtualized: only visible rows/columns render; cell values
  are text fragments, styles resolved on demand; scrolling reuses buffers
  (zero allocations per frame in the scroll path).
* Recalculation is incremental and off-thread (formula engine contract in
  `loom-core`); the grid never blocks on `=SUM` chains; a recalculation
  banner (status bar chip: "Calculating…") appears only when a recalculation
  exceeds 250 ms and is cancellable.
* Benchmark gates: 1,000,000-cell random data sheet scrolls at 60 fps;
  recalc of 10,000 formulas < 200 ms on the mainstream tier
  (`PERFORMANCE.md`).
* Freeze panes + virtual scrolling compose: frozen region is a separate
  buffer overlaid on the virtual grid.

## 5. Editing

* Cell entry: type = replace value; F2/double-click = in-cell caret edit;
  the formula bar edits the same buffer (edits from either surface stay
  synchronized, commit on Enter/Esc).
* Formula entry: `=` prefix opens formula context — token coloring
  (functions accent, references ink-primary, errors danger), reference
  highlighting: when the caret touches a reference, the referenced range
  is outlined with the reference's color (per reference, cycling the
  data-viz palette for multi-reference formulas).
* Commit semantics: Enter commits and moves down; Tab commits and moves
  right; Esc cancels; invalid input (bad formula, wrong type) blocks commit
  with an inline error message under the cell (never a dialog —
  `DIALOGS.md` policy).
* IME input: in-cell editing supports IME composition with the standard
  composition underline (`DOCUMENT_EDITOR.md` §5 rules apply to the grid).

## 6. Rows, columns, structure

* Header context menus: insert/delete row(s)/column(s), hide/unhide, freeze,
  width/height, sort, filter — every entry also available via
  keyboard (`Cmd+Shift+K` insert, `Cmd+Delete` delete) and palette.
* Row/column resize: drag header edge, live redraw, snap to content on
  double-click (auto-fit); width readout in the header chip while dragging.
* Grouping/outlining: group rows/columns via header menu; the group rail
  (12 px) shows collapse chevrons with a non-color state (chevron
  orientation + disclosure); groups collapse/expand with animation 200 ms
  out-quad, reduced motion instant.
* Filters: header dropdown (ComboBox-list popover) with filter state shown
  as a funnel glyph + tinted header fill (15% accent) — never color-only.

## 7. Accessibility

* The grid is fully keyboard-operable (table semantics per cell: row
  header, column header announced on navigation).
* Screen reader announcements: cell reference + value + formula when the
  cell has one; range selections summarized; frozen state announced.
* Cell target size: minimum 24 px height at default zoom with 44 px
  effective row hit targets for pointer when rows are at minimum height
  (`POINTER_AND_PEN.md`).
* At 1.5× text scale, headers grow to fit; the grid keeps 1:1 cell-to-value
  mapping (cell content scales, geometry grows).


## source-loom-design-bible-theming-md

Original path: `loom-design-bible/THEMING.md`

# Theming

Loom ships four built-in configurations: three themes (light, dark,
high-contrast) and one orthogonal mode (reduced motion). Themes are pure
token swaps; components are theme-agnostic.

## 1. Theme matrix

| Configuration | Purpose | Selection |
|---|---|---|
| Light (default) | Standard professional work | Default; manual |
| Dark | Low-light work, media review | Manual; follows OS dark preference by default (can be fixed) |
| High-contrast | Accessibility requirement | Manual; follows OS high-contrast preference by default |
| Reduced motion | Motion accessibility | OS preference; manual toggle; orthogonal to theme |

Rules:

* A theme change applies instantly (token swap, no restart), animates as a
  200 ms in-out cross-fade at most, and is reduced-motion-safe (instant).
* Theme choice persists per app in settings; document content (page
  backgrounds, document colors) is **not** re-themed — only UI chrome
  re-themes. The page always renders in its document color space.
* Third-party/custom themes are `[future]`: the theme API will be a token
  document (the same TOML schema) loaded from user settings with
  validation against the contrast floors; not a plugin API surface yet.

Responsive geometry is theme-independent. `ResponsivePolicy` owns the 1180 px
P1 and 1320 px P2 transitions, while the selected `Theme` supplies the
semantic colours and type metrics. The 40 px context-toolbar slot and 48–52 px
icon-over-label slot remain distinct in every theme and at 150% text scale.

## 2. Token mapping (identical in all three themes)

Each theme supplies values for the same semantic tokens; names never vary.
Full values in `tokens/loom.toml` and `DESIGN_TOKENS.md` §4–§11. Summary
map (light shown; dark/high-contrast replace values only):

| Semantic token | Light value | Role |
|---|---|---|
| `color-surface-canvas` | `#FAF9F7` | Window/canvas ground |
| `color-surface-raised` | `#FFFFFF` | Panels, dialogs, controls |
| `color-surface-sunken` | `#F1EFEA` | Wells, inputs, beds |
| `color-ink-primary` | `#26221C` | Primary text |
| `color-ink-secondary` | `#5C564C` | Secondary text |
| `color-accent-default` | `#B4552D` | Selection, focus, primary actions |
| `color-accent-ink` | `#FFFFFF` | Text on accent |
| `color-accent-hover` | `#C9643A` | Accent hover/pressed |
| `color-status-success` | `#3E6B4F` | Success signals |
| `color-status-warning` | `#A8681E` | Warning signals |
| `color-status-danger` | `#A43424` | Error/destructive signals |
| `color-status-info` | `#3B5E7A` | Info signals |
| `space-2 … space-64` | 2…64 px | Spacing scale |
| `radius-2 … radius-12` | 2…12 px | Corner radii |
| `border-width-hairline/default/strong` | 1 / 1 / 2 px | Borders |
| `type-size-11 … 40` | 11…40 px | Type scale |
| `type-leading-compact/body/relaxed` | 1.4 / 1.45 / 1.5 | Leading |
| `type-weight-regular/medium/semibold/bold` | 400/500/600/700 | Weights |
| `motion-duration-instant/fast/standard/deliberate/slow` | 0/120/200/320/500 ms | Durations |
| `motion-easing-out-quad/in-out/out-back` | (0.33,1,0.68,1) / (0.65,0,0.35,1) / (0.34,1.56,0.64,1) | Easings |
| `shadow-none` / `shadow-popover` | none / 0 1px 3px rgba(38,34,28,0.12) | Elevation |
| `icon-viewbox-20` / `icon-stroke-width-15` / `icon-corner-radius-2` | 20 / 1.5 / 2 | Icons |

Theme-independent tokens (identical in all themes): spacing, radii, border
widths, type scale/leading/weights, durations, easings, shadows, icon grid.
Only palette tokens vary by theme.

## 3. Dark-theme specifics

* Same contrast floors as light (`COLOR.md` §7); surfaces darken
  canvas→sunken instead of lightening.
* Ink hierarchy inverts; accent lightens (`#D97A4A`) to keep ≥ 4.5:1 with
  `accent-ink` (`#1F1710`); media wells are darker than the canvas
  (`#171512`) so previews pop.
* Borders: ink at 18% alpha (brighter than light's 12% — needed on dark);
  page edges gain a 1 px hairline to separate page from canvas.

## 4. High-contrast specifics

* Surfaces collapse to pure black; separation via white borders and text
  tiers only (`COLOR.md` §4).
* Selection: 2 px white outline + white handles on black; focus ring
  black-on-white (or white-on-black) by surface; hover = inverted.
* Accents carry doubled contrast (≥ 7:1 vs black) so they remain
  distinguishable from pure ink.
* Icons: full ink, no opacity tiers below 80% except disabled (40%).

## 5. Reduced motion as a mode

* Applies over any theme; disabled animations: translation, scale,
  overshoot, spring — replaced per `MOTION.md` §4 (opacity ≤ 120 ms only).
* Not a theme: there is no separate "reduced motion theme"; the setting
  switches animation behavior globally.

RTL is likewise orthogonal to colour theme. The release matrix exercises both
directions with the same focus, accessible-name, and geometry assertions; a
direction change must not introduce app-local padding, colour, or control-size
overrides.

## 6. Theme integrity

* Components consume tokens only — a hard-coded literal in a component is
  a defect that fails CI (token lint).
* Every theme must pass: the full visual-QA baseline set, the contrast
  gate (§`COLOR.md` 7), the 1.5× text-scale gate, and the reduced-motion
  assertion suite. A theme that fails a gate does not ship.
* `tokens/loom.toml` carries all three palettes; adding a theme = adding a
  palette table + its QA passes, never new components.

## 7. Theme application

* The runtime theme is selected from settings at startup and applied at
  the UI root; theme is user settings, not document data.
* Future custom themes will load through the token document schema with
  validation (contrast floors enforced) — noted here so the current
  architecture (token swap at the root) is not contradicted later.


## source-loom-design-bible-timeline-md

Original path: `loom-design-bible/TIMELINE.md`

# Timeline

The timeline is the primary surface of Motion, Video, and Studio's
arrangement view. This document fixes its anatomy and scrubbing behavior;
per-app details (track types, connect semantics) live in each app's
PRODUCT_SPEC.

## 1. Anatomy

```
┌───────────────┬────────────────────────────────────────────┐
│ track header  │ ruler (24 px)                              │
│ (160 px)      ├────────────────────────────────────────────┤
│               │  track lanes                               │
│               │  ┌────────────────────────────────────┐    │
│               │  │  clip   [keyframe ◆ ◆]             │    │
│               │  └────────────────────────────────────┘    │
│               │  waveform lane (audio clips)               │
└───────────────┴────────────────────────────────────────────┘
```

* **Track header**: 160 px fixed, resizable 96–320 px. Contains track name,
  type glyph, mute/solo/record-arm controls (Studio), lock/visibility
  toggles (Video/Motion), track height control. Header rows align
  ʹ1:1 with lanes; horizontal scrolling of the timeline never detaches the
  header.
* **Ruler**: 24 px strip above lanes; time in tabular figures
  (`type-size-11`, secondary ink), tick density adapts to zoom (major ticks
  with labels, minor ticks unlabeled); timecode format per project
  (SMPTE drop/non-drop for video, bars/beats for Studio, frames for
  Motion — the ruler honors the project timebase, never mixed units).
* **Lanes**: horizontal tracks; clip blocks `radius-4`, raised fill,
  1 px border; selected clip = accent outline (`SELECTION.md` §1 adapted:
  outline stays 2 px, overlay 15%).
* **Playhead**: 1 px accent line, full lane height, triangular head in the
  ruler; current time shown in the ruler at the playhead
  (tabular figures, chip-backed). Playhead is draggable (scrub), reaches
  via keyboard (see §4).
* **Keyframes**: diamonds 9 × 9 px (`color-accent-default` fill, white
  center dot) on their value track; selected keyframe = white fill +
  accent ring; adjacent keyframes connect with hairline value lines only in
  the graph editor view.
* **Waveform lane**: audio clips render a waveform (min/max peaks) at
  `color-ink-primary` 55%, on sunken bed; generated off-thread
  (`loom-core` media contract), cached per clip+zoom, rendered within one
  frame from cache; waveform never blocks scrubbing.
* **Clip edges**: trim handles 6 px at each clip end (drag to trim, ripple
  by default in Video); speed glyph at clip top-right when ≠ 100%.

## 2. Timeline chrome

* Horizontal scrollbar at the bottom of the timeline area (overlay style);
  vertical scrollbar right of the lanes; header and ruler are sticky.
* **Zoom (time)**: `Cmd+Plus/Minus` zooms time centered on the playhead;
  wheel with `Option/Alt` zooms around pointer (anchor-at-pointer rule from
  `CANVAS.md`); range: 1 frame per 2 px to 1 hour per 10 px (per app
  limits).
* **Zoom (track height)**: `Cmd+Shift+Plus/Minus` or drag on header bottom
  edge; per-track height override persists.
* **Auto-scroll**: during playback, the view follows the playhead; during
  scrubbing, follows with a 10% margin trigger (never fights the pointer:
  if the user drags against the auto-scroll, the pointer wins).
* **Fit**: double-click in the ruler empty area fits the whole sequence
  (or `Cmd+Shift+F`).

## 3. Selection and editing

* Click clip = select; Shift+click = add; drag on empty lane = marquee
  (`SELECTION.md` §5); clicking the lane header selects all clips in that
  track.
* Trim/roll/slip/slide operate per `loom-spec` editing model; feedback:
  ghost of the affected region during the drag (ink at 15% overlay,
  live value readout in the status bar), commit on release — undoable.
* Ripple mode (Video): trimming or deleting ripples downstream; non-ripple
  leaves gaps (indicated with a hatch pattern in the gap, never a
  fake clip).
* Clip snapping: clip edges, playhead, markers, 1 s time divisions —
  snap halo 8 px, accent indicator (`CANVAS.md` §4).
* Track reorder: drag the track header (lift + 1.02 scale, 120 ms;
  displaced tracks slide 200 ms out-quad per `MOTION.md` grammar).
* Keyboard delete: Backspace/Delete deletes selected clips (ripple per mode
  setting); undo restores exactly.

## 4. Scrubbing

* **Pointer scrub**: drag on the ruler or the playhead; time follows the
  pointer 1:1, zero lag, no easing; preview updates at the display rate —
  target 60 fps, acceptable ≥ 24 fps for heavy compositions with an
  explicit "resolution scrub" indicator (a small "HD/1/4" chip) showing
  degraded preview quality during heavy scrubs.
* **Space = play/pause** (`KEYBOARD.md`); spacebar during playback stops
  instantly (no fade-out tail).
* **J/K/L** transport: J = reverse, K = pause, L = forward; J/L hold
  accelerates (1× → 2× → 4× after 700 ms per press repeat, shown as a chip
  near the playhead); Shift+J/L = 1-frame step. JKL works during scrubbing
  and over any focused surface except text fields (where they type).
* **Frame stepping**: arrows with Shift when timeline focused (Left/Right =
  1 frame; Up/Down = previous/next edit point or marker).
* **Audio scrub**: playing region is silence-suppressed only in Studio's
  scrub-chips mode (`[goal]`); default scrub is silent in Video/Motion,
  audible-but-stable in Studio (audio continues at 1× pitch).
* **Reduced motion**: scrubbing is instantaneous by nature (no animation);
  playhead follows frame-exact. No reduced-motion changes needed beyond
  disabling the (optional) playhead travel animation — there is none.

## 5. Markers, ranges, loops

* Markers: diamond flags in the ruler (drag to move, Option+drag to
  duplicate); marker menu (rename, color per marker category with text
  label — never color-only).
* Loop range: drag in the ruler to select a range (accent-tinted band,
  in/out handles); loop toggle (`Cmd+Shift+L`); loop range persists with
  the project.
* Work area (Encode/Video export): distinct band style (hatched), separate
  from loop range; exports honor the work area by default.

## 6. Performance and accessibility

* The timeline is a virtualized view: visible range renders only
  (clips/keyframes/waveforms fetched on demand, cached); 10,000+ clip
  projects stay fluid; nothing on the UI thread except compositing.
* Timeline keyboard operation is complete: arrows navigate, Tab moves
  between clips and controls, every clip action has a shortcut or palette
  entry (`KEYBOARD.md`, `COMMAND_PALETTE.md`).
* Screen reader: timeline exposes clip names/positions via the
  accessible object model; selected clip announces "Clip 3 of 12, 00:02:10 –
  00:05:00".
* At 1.5× text scale the ruler height grows to 32 px; lane heights grow
  proportionally; nothing clips.


## source-loom-design-bible-toolbars-md

Original path: `loom-design-bible/TOOLBARS.md`

# Loom Toolbar Contract

Toolbars are single-line, contextual command surfaces. Exact dimensions and breakpoints are defined in [`contracts/desktop-ui.toml`](loom-design-bible/contracts/desktop-ui.toml).

## Explicit toolbar slots

`ContextToolbar` owns the 40 px context row. `LabeledToolbar` owns the
48–52 px row required by `IconOverLabelToolbarItem`; `IconOnlyToolbarItem`
remains the 28 px compact target. `Toolbar` keeps a legacy labeled-slot
default for existing hosts, but new surfaces must choose an explicit slot.
Content cannot silently increase a context row's height.

## Structure

A toolbar contains at most three logical groups:

1. **Leading** — navigation/document structure or the primary tool family.
2. **Center** — commands for the active content, tool, or selection.
3. **Trailing** — view/search/export and the overflow menu.

More than three visible groups is a design failure. Do not solve command abundance with more separators or another toolbar row.

Items within a group use the contract item gap; groups use the contract group gap. Toolbars never scroll and never wrap.

## Priority algorithm

Every toolbar item declares one priority:

- `P0`: visible at all supported widths.
- `P1`: may collapse from labeled form to icon-only below the contract breakpoint, but only if the symbol is unambiguous and a tooltip/accessibility label exists.
- `P2`: moves to overflow below the contract breakpoint.

The implementation applies this order whenever width decreases:

1. move P2 actions to overflow;
2. convert eligible P1 actions to icon-only;
3. remove redundant toolbar exposure while retaining menu/palette access;
4. never clip, ellipsize, overlap, wrap, scroll, or shrink below control minimums.

The shared `ResponsivePolicy` evaluates these transitions at one canonical
set of boundaries: P1 icon-only below 1180 px, P2 overflow below 1320 px.
Validation exercises 1179, 1180, 1279, 1280, 1319, and 1320 px, plus 150%
text and both LTR/RTL directions. 1279/1280 must produce the same policy
state; a host-defined 1280 px breakpoint is a contract violation.

If P0 still does not fit, the toolbar contains too many P0 actions and must be redesigned.

## Command placement

A toolbar is not the command inventory. Every toolbar command exists in the shared command registry and therefore has a menu/command-palette/keyboard representation where applicable.

Permanent toolbar placement is reserved for frequently used or context-critical actions. Secondary commands such as Save As, uncommon import variants, detailed export settings, or one-off maintenance commands normally live in menus/palette.

A text-labeled action must not visually merge with an adjacent icon action. Keep labeled actions in their own group or give them the group spacing required by the contract.

## Visual treatment

Routine Mac-class desktop toolbar controls are visually quiet. An unchecked toolbar icon normally has no permanent heavy bezel. Hover, pressed, selected, disabled, and keyboard-focus states provide affordance.

Rules:

- action labels never truncate;
- toolbar controls use shared `loom-ui` components only;
- no app-local rounded rectangles pretending to be buttons;
- no decorative vertical rule after every small group;
- no application logo tile in the toolbar;
- no neutral "Local"/"Editing" badge occupying permanent toolbar width;
- export/render may be visually emphasized only when it is genuinely the primary current action;
- destructive commands are not promoted merely for symmetry.

## Context behavior

Toolbars reflect the active selection/tool while preserving stable command locations where possible. Context changes must not cause unrelated controls to jump between arbitrary positions.

Examples:

- Writer text selection → character/paragraph commands.
- Photo selected layer → transform/mask/adjustment commands.
- Present selected object → arrange/style commands.
- Video selected clip → trim/clip commands.

No selection exposes document/surface commands rather than a set of inexplicably disabled object controls.

## Accessibility

Every toolbar item has:

- accessible name;
- keyboard focus path;
- command registry identity;
- tooltip for icon-only representation;
- shortcut hint where one exists;
- disabled reason when the cause is not obvious.

Tab enters/leaves the toolbar as a logical region; arrow-key movement within a toolbar group is preferred when the shared component provides roving focus.

## Acceptance

A toolbar is accepted only when all required viewports and text scales satisfy:

- one line exactly;
- zero action-label clipping;
- zero overlap;
- no control below minimum size;
- at most three visible groups;
- deterministic overflow membership;
- every overflowed action remains reachable;
- keyboard focus order matches visual order;
- realistic localized/pseudolocalized labels do not break the layout.

The source audit records a geometry manifest for each state and rejects
positive-area rectangle overlap, label clipping, a second toolbar line, and a
primary-surface width below the shared minimum. Screenshot hashes remain useful
regression evidence but cannot substitute for these assertions.

Replacing a screenshot baseline to hide a toolbar collision is a release-process defect.


## source-loom-design-bible-typography-md

Original path: `loom-design-bible/TYPOGRAPHY.md`

# Loom Typography

Exact type roles and values live in [`tokens/loom.toml`](loom-design-bible/tokens/loom.toml) and [`contracts/desktop-ui.toml`](loom-design-bible/contracts/desktop-ui.toml). This file defines usage.

## Desktop UI hierarchy

Loom is a dense pointer/keyboard creative suite. Routine UI uses compact desktop typography rather than mobile-sized labels or oversized dashboard headings.

Roles:

- **UI label:** 13 logical px, medium 500. Buttons, toolbar labels, fields, tabs, inspector values.
- **Section label:** 13 logical px, semibold 600. Inspector/panel sections.
- **Caption/status:** 11 logical px. Status bar, timestamps, tertiary readouts.
- **Small metadata:** 12 logical px.
- **Body/help prose:** 14 logical px.
- **Subtitle:** 16 logical px.
- **Content heading:** 20 or 24 logical px only when the content hierarchy actually requires it.

Routine chrome does not use 20–40 px display typography. The work itself, not application branding, owns visual hierarchy.

Bold 700 is not the default chrome weight. Use regular/medium for routine controls and semibold for section/title emphasis. Excessive bold weight is treated as hierarchy noise.

## Font stack

Default UI family is `Noto Sans` with the fallback chain defined in `tokens/loom.toml`. Document applications may use `Noto Serif`, `Noto Sans Mono`, embedded/document fonts, and language-specific shaping for user content.

Rules:

- fonts are vector text, never bitmap UI labels;
- fallback is per glyph where shaping infrastructure supports it;
- missing UI fonts fall back rather than preventing launch;
- UI family choice is a token/system decision, not an application choice;
- document typography is domain content and remains independent of chrome typography.

## Numeric text

Coordinates, timecode, durations, spreadsheet values, percentages, progress, media timestamps, mixer values, and inspector numeric readouts use tabular figures when the renderer/font path supports them. Scrubbing a value must not cause surrounding UI to jitter horizontally.

## Truncation

Text falls into two categories.

**User content may ellipsize:** document/project title, file name, layer name, track name, media name, user-created style/name. Full value must remain available through tooltip/accessibility text.

**Interface language may not ellipsize:** action labels, button labels, inspector property labels, section headings, severity labels, command names, menu items.

If interface language does not fit, the layout must recompose: overflow toolbar commands, widen/stack inspector rows within bounds, or collapse optional panels. Do not hide a design failure behind `…`.

## Localization

All UI strings must survive pseudolocalization and bidirectional text. Logical leading/trailing layout is preferred over left/right assumptions.

- UI labels do not hyphenate.
- Long translated control labels trigger responsive composition, not clipping.
- Dates, numbers, currency, and units use locale-aware formatting where implemented.
- Text shaping and IME behavior are part of editor correctness, not visual polish only.

## Text scaling

Release tests require 1.0, 1.25, and 1.5 text-scale behavior; 2.0 is the accessibility stress target.

Scaling text does not blindly scale every panel and toolbar dimension. The responsive system keeps controls usable by moving lower-priority actions into overflow, stacking property rows where allowed, and collapsing optional panels before content is harmed.

No action/control label may clip at any required scale. User content may ellipsize only under the explicit truncation policy.

Content zoom is independent of UI text scale. A Writer page, Photo image, Present stage, or Video viewer can change zoom without making chrome text larger.

## Contrast

Routine UI/body text must meet the contrast floor defined in `tokens/loom.toml`. Focus and non-text control boundaries must meet their UI contrast floor. High-contrast mode is a semantic palette swap, not application-specific restyling.


## source-loom-design-bible-ux-acceptance-checklist-md

Original path: `loom-design-bible/UX_ACCEPTANCE_CHECKLIST.md`

# UX Acceptance Checklist

Per-app acceptance checklist. An application milestone is not done until
every applicable item passes with evidence. Items are grouped; each app
fills the app-specific rows (shortcuts, surfaces) from its PRODUCT_SPEC.

## 1. Keyboard

- [ ] Every command reachable by keyboard (Tab/arrows/Enter/Esc + its
      shortcut); no mouse-only feature anywhere.
- [ ] Every shortcut in `KEYBOARD.md` works in the app; app-specific keys
      per its section behave as documented.
- [ ] Tab order is logical and stable; focus returns to the opener after
      dialogs/popovers/palette.
- [ ] No keyboard trap except sanctioned modal contexts.
- [ ] Space/Enter/Esc semantics correct in every surface (transport keys
      don't fire while typing in text fields).

## 2. Focus visibility

- [ ] Focus ring visible at every focus stop in every theme (captured).
- [ ] Ring ≥ 3:1 contrast on all adjacent surfaces; high-contrast variant.
- [ ] Focus not suppressed by mouse use; hover never replaces focus.
- [ ] Focus moves announced (screen reader) and visible within 120 ms.

## 3. Screen-reader labels

- [ ] Every control has an `accessible-description` (CI lint green).
- [ ] Groups, panels, sections, toolbars announce names and roles.
- [ ] States announced: checked, disabled, selected, expanded, value
      changes (debounced), completion, errors (live region, alert role).
- [ ] Canvas/timeline/grid expose the accessible object model per surface
      document (`ACCESSIBILITY.md` §7).

## 4. Contrast and color

- [ ] All token pairs in use pass the contrast floors (CI gate, `COLOR.md`
      §7).
- [ ] No color-only status anywhere; each status has text/icon/shape.
- [ ] High-contrast theme passes its dedicated capture set.

## 5. Reduced motion

- [ ] All animations from the grammar; no bespoke values.
- [ ] Reduced-motion mode: no translation/scale animations (asserted);
      opacity ≤ 120 ms; surfaces instant.
- [ ] Scrubbing and direct manipulation unaffected by reduced motion.

## 6. Text scaling

- [ ] UI usable at 1.25× and 1.5× (capture set); no clipped labels, no
      unusable layouts.
- [ ] Chrome heights accommodate scale (toolbar ≤ 56 px at 1.5×); canvas
      zoom independent of text scale.

## 7. Empty states

- [ ] Every empty state has a primary action and a clear explanation; no
      dead ends.
- [ ] Empty states for: no document, no media, no layers, no results,
      no recovery data, no model packs.

## 8. Error states

- [ ] Recoverable errors: toast with the failure, affected items, and next
      action; details in Diagnostics.
- [ ] Validation errors focus the offending control and state the fix.
- [ ] Fatal errors: dialog with recovery path; never a silent failure.
- [ ] Error messages in plain language; no raw codes as the headline.

## 9. Cancellation and progress

- [ ] Every long job: progress observable (status bar/jobs), cancellable,
      cancellation acknowledged within one frame, job stops promptly.
- [ ] Progress determinate where a value exists; phase text otherwise.
- [ ] Autosave silent and non-blocking; autosave failure is an error.

## 10. Unsaved-work handling

- [ ] Closing a document with unsaved changes prompts (save / discard /
      cancel) per `DIALOGS.md`; discard is never the default.
- [ ] Crash recovery: recovered documents listed with timestamps; user
      chooses keep/discard; nothing deleted silently.
- [ ] Save failure (disk full, permissions) reported with options, and
      the document remains editable with recovery available.

## 11. Selection and direct manipulation

- [ ] Selection visuals per `SELECTION.md` in all themes; multi-select,
      marquee, keyboard selection work.
- [ ] Drag/drop: feedback per `DRAG_AND_DROP.md`; keyboard alternative
      exists for every drag operation.
- [ ] Inspector reflects selection truthfully (live updates, no apply
      buttons).

## 12. Per-app specifics (fill from each app's PRODUCT_SPEC)

- Writer: page canvas chrome, caret behavior, IME composition.
- Sheets: grid navigation table, freeze panes, formula entry, cell
  announcements.
- Present: slide navigation, presenter display, rehearsal timing.
- Photo: tool shortcuts, mask overlay accessibility.
- Motion/Video: JKL transport, timeline keyboard model, marker jump.
- Studio: transport, piano-roll keyboard, mixer focus order.
- Encode: queue keyboard model, job control keys.

## 13. Evidence requirement

Every checked item ships evidence: scripted walkthrough output, CI lint
results, capture files with gate metrics, or a test name. A checklist with
unverifiable claims is not accepted as evidence of completion — mark the
item "not yet verified" and keep it open.

## 14. Exit criteria

The app milestone passes when: all applicable items above are checked with
evidence; the visual gate is green for its theme/locale/scale matrix; the
perf gates are green on the mainstream tier; and no open release-blocking
known limitations remain (`loom-spec` release criteria).


## source-loom-design-bible-visual-qa-md

Original path: `loom-design-bible/VISUAL_QA.md`

# Visual QA

Visual regression is a release gate. This document fixes the process, the
capture contract, the comparison method, and the artifact rules.

## 1. Capture contract

* All baselines and actuals are captured **in-app** using the software
  renderer (deterministic CPU rasterization — the documented
  software-rendering path; never a screenshot of a GPU-presented window).
* Fixed capture size: **1280 × 800 logical pixels**, 1.0 text scale,
  fixed deterministic fonts, fixed locale (en-US default for baselines;
  pseudolocale/RTL sets are separate capture suites), fixed window chrome.
* Capture happens only in the **Docker visual-QA environment** (pinned
  Ubuntu base, software Vulkan, Xvfb — `loom-bootstrap` docker scripts).
  Baselines generated on contributor machines are rejected.
* Determinism requirements: fixed seed for any procedural content
  (sample documents, patterns), wall-clock animations disabled by
  advancing to final states (reduced-motion capture mode), no network
  anywhere in the pipeline.

## 2. Baseline storage

* Baselines live in this repository: `baselines/<app>/<name>.png`,
  where `<app>` is one of writer, sheets, present, photo, motion, video,
  studio, encode, vision, components (gallery).
* Each baseline ships with metadata (JSON sidecar, `<name>.meta.json`):
  app, capture date, commit id, renderer id, font configuration, theme,
  text scale, locale, reduced-motion flag, window layout, and the seed.
* The baseline set is versioned with the repository; a baseline changes
  only when the contract intentionally changes (token change, layout
  change), and the change requires the ADR/review path — never silent
  auto-updates.
* Component-gallery baselines (from the gallery milestone) cover the
  component state matrix; application baselines cover the application
  screens listed in §4.

## 3. Comparison method

Perceptual diff on RGBA pixels (software-renderer output is deterministic,
so the diff measures contract drift, not rasterizer noise):

* Metric 1 — **mean absolute error** across all pixels (0–255 scale):
  gate `mean < 1.0`.
* Metric 2 — **differing-pixel ratio**: pixels with per-channel abs diff
  > 8/255 (after 1 px erosion to ignore 1-px shifts) divided by total
  pixels: gate `ratio < 0.01`.
* Both gates must pass. A run that fails either produces artifacts and is
  reported; it does not update the baseline.
* Tolerances are fixed contract values; adjusting them requires an ADR.

## 4. Baseline coverage (minimum)

Per application:

* Empty state (no document), a representative sample project open, and a
  complex project open.
* Each component state that appears in the app's chrome (toolbar hover,
  button focus, dialog open, menu open, popover open, inspector sections).
* Selection states: single, multi, marquee in progress, text caret.
* Theme set: light, dark, high-contrast.
* Text scale 1.25 and 1.5 layouts (layout-stress set).
* Reduced-motion final states.
* RTL locale stress screens and pseudolocale screens (localization set).
* Error states: import failure toast, recovery browser, relink dialog.
* The full component-state matrix once the gallery milestone exists.

A new feature adds at minimum: its default state, its selected state, its
error state, and its reduced-motion final state.

## 5. Process

1. Application team adds/updates feature; runs the app's visual suite
   locally (actuals only — no baseline writes).
2. CI (Docker visual environment) renders actuals, diffs against committed
   baselines, and fails on gate violation.
3. On failure: artifacts — actual PNG, diff PNG (green = equal, red =
   differing, blue = missing region), metadata JSON — are collected into
   `artifacts/<run-id>/<app>/<name>.{png,meta.json}` and made available in
   the CI artifact store.
4. The design-system lead inspects severe diffs, classifies them:
   intentional contract change (→ baseline update through review) or
   defect (→ fix in the application, new baseline NOT created).
5. No auto-approval exists: no tool may accept a new baseline without
   human review in a PR that also updates the contract documents if the
   change is intentional.

## 6. Diff inspection rules

* Diffs beyond tolerance are classified: layout shift, color drift, missing
  element, extra element, text change, antialiasing noise.
* Diff magnitude: `mean` and `ratio` values are printed per capture with
  the region bounding box of the largest differing area — engineers fix by
  region, not by guessing.
* Screenshots must come from the real application binary in the container:
  no compositing screenshots from mockups, no hand-placed images.

## 7. Reporting

* `VISUAL_QA_REPORT.md` per run: pass/fail per capture, metrics table,
  artifact links, classifier notes, and the commit ids of baseline vs.
  actual. Shipped with the release artifacts (`loom-bootstrap`).
* A release ships only with a green visual gate for its supported theme
  and locale matrix.


## source-loom-design-bible-windows-md

Original path: `loom-design-bible/WINDOWS.md`

# Windows

Window types, anatomy, and multi-window policy for Loom applications.

## 1. Window types

| Type | Purpose | Modality | Count |
|---|---|---|---|
| Main window | The application surface | — | 1 per app instance |
| Dialog | Short decisions (save, confirm, preferences section) | Modal, rare | 0–1 at a time |
| Popover | Non-modal surface anchored to a control (menus, palettes, inspectors pop, color wells) | Non-modal, dismisses on outside click | 0–n, one per anchor |
| Utility window | Long-lived secondary surfaces: render queue, media browser, scopes, mixer | Non-modal | 0–3 |
| Floating panel | Detached inspector/canvas area | Non-modal | `[future]` |

## 2. Main window

* One main window per app; document tabs within it (Writer/Sheets/Present may
  open multiple documents in tabs; canvas apps open one document, project
  sessions in a library window).
* Anatomy per `LAYOUT.md`: title bar 40 px, context toolbar 40 px, sidebar
  240 px collapsible, inspector 280 px, status bar 28 px, canvas fills.
* The main window is resizable with minimums per `LAYOUT.md` §2; below the
  minimum, chrome collapses (sidebar auto-collapses) before content.
* Window state (size, position, sidebar/inspector state, theme) persists per
  app in user settings; document state persists per document (recovery per
  `loom-core` autosave contract).

## 3. Dialog windows

Policy: **dialogs are rare**. A modal appears only when the user must make a
decision before continuing and the decision has consequences (destructive
confirmation, file-overwrite confirmation, incompatible-version save). Do not
use dialogs for: properties (inspector), formatting (context toolbar),
navigation (sidebar/palette), or settings pages (utility window or panel).

Rules (`COMPONENTS.md` §16 for anatomy):

* Focus moves into the dialog on open (first focusable control); Tab order
  loops within the dialog; Esc = cancel; Enter = primary/default action.
* Default action button is always the safe one (primary "Save", or "Cancel"
  where the primary action is destructive); destructive buttons require
  explicit confirmation semantics (`DIALOGS.md`).
* A dialog opens over a dimmed, non-interactive backdrop (dim = ink at 20%);
  backdrop click does not dismiss (deliberate decision over dismissal).
* Only one dialog per app; a second dialog request is queued behind it.
* Dialog positions center on the parent window; sizes 420 px min width,
  640 px max width, height ≤ 80% of window; scroll inside body if needed.
* Motion: fade + 4 px scale-in, out-quad 200 ms; exit 160 ms; reduced motion:
  instant appearance, fade-only exit.

## 4. Popovers

* Anchored to their trigger control, offset 4 px; 320 px default width
  (menus/color wells may be smaller/larger per component spec).
* Non-modal: clicking outside dismisses; Esc dismisses; the trigger toggles
  state (arrow indicator shows open).
* Focus behavior: menus and combobox popovers move focus in (keyboard-first
  lists); color wells and preview popovers keep focus on the trigger.
* Popovers never open on hover alone (only after a deliberate click/keyboard
  action) — hover-reveal is prohibited (`ANTI_PATTERNS.md`).
* Motion: entrance out-quad 120 ms with 4 px slide from anchor direction;
  exit 120 ms; reduced motion: fade 120 ms only.
* Popovers are the only surfaces allowed `shadow-popover`
  (`DESIGN_TOKENS.md` §10).

## 5. Multi-window policy

* All windows are in the same process; a single app session owns them.
* Secondary windows (utility windows) are non-modal and move with the main
  window's state; they never steal focus on their own (no pop-to-front on
  background completion; status bar + notification covers that).
* Opening a document type handled by another Loom app (`.loomdoc` from
  Photo) opens that app — or, if the app is not installed, shows an install
  hint dialog; no in-app emulation of another app's surface.
* Cross-app drag and drop of assets/files uses the shared clipboard/drag
  contract (`DRAG_AND_DROP.md`).
* Presenter/secondary-display windows (Present, Video): follow
  `PRESENTER.md`-level rules — the presenter display is a dedicated full-
  screen window with no chrome, timing, and a rehearsal clock; it is
  considered a utility window variant.

## 6. Window chrome

* Title bar: app name on the right (or platform convention), document title
  centered-left, window controls per platform; on Linux, standard WM
  decorations are acceptable, but the in-app title bar must still exist for
  consistent theming and command access.
* Window icons: original Loom app icons per application (drawn to the icon
  family rules in `ICONOGRAPHY.md`), provided in PNG/ICO/SVG source
  (assets in `loom-core`, spec here).


## source-loom-design-bible-docs-adrs-adr-0001-design-tokens-md

Original path: `loom-design-bible/docs/adrs/ADR-0001-design-tokens.md`

# ADR-0001: Design Token Architecture

Status: **Accepted**
Date: 2026-08-01
Owner: Design-system lead

## Context

Eight applications and a shared platform must render a single visual
language. Early sketches showed three failure modes: (1) values copy-pasted
per app and drifting, (2) a single monolithic "theme" struct mixing raw
colors with layout decisions, and (3) prose-only contracts that agents and
teams interpret differently. The suite also requires four theme
configurations (light, dark, high-contrast, reduced-motion mode) that must
not multiply component code.

## Decision

Adopt a two-layer token architecture with one canonical machine-readable
file.

1. **Primitive layer**: raw values (hex colors, px sizes, ms durations,
   cubic-bezier points). Versioned; changed only via ADR.
2. **Semantic layer**: purpose-named tokens (`color-surface-canvas`,
   `space-8`, `motion-duration-fast`) consumed by components. Semantic
   tokens are the only thing components may reference.

Rules:

* Canonical source: `tokens/loom.toml` in `loom-design-bible`.
* Naming: `category-role-scale`, lowercase with hyphens. Categories:
  `color`, `space`, `radius`, `border`, `type`, `motion`, `shadow`, `icon`.
* Mapping to Slint: a generator (owned by `loom-core`) emits
  `Tokens.slint` `export const` declarations with native types (`color`,
  `length`, `duration`, `easing`) so wrong-typed usage fails at compile
  time. Theme variants are structs of the same token names per theme.
* Documentation (`DESIGN_TOKENS.md`, `THEMING.md`) must match the TOML
  exactly; the TOML wins on conflict.
* Themes are token-value swaps only; components are theme-agnostic.
* No component may hard-code a literal; a token lint fails CI.
* Adding/changing/deleting a token is an ADR-gated change
  (`DESIGN_TOKENS.md` §13).

## Alternatives considered

* **Slint-only tokens**: define everything in a `Tokens.slint`. Rejected:
  no machine-readable form for CI checks, no single source for docs and
  generator, poor diff-ability.
* **One giant theme struct**: single struct per theme containing colors,
  sizes, and motion. Rejected: conflates categories, no compile-time
  guidance, hard to diff, encourages per-app forks.
* **YAML/JSON source**: rejected for determinism of ordering and
  commentability; TOML chosen (sorted, commented, diff-friendly).
* **CSS-variable-style runtime strings**: rejected — no type safety, no
  generator, and the toolkit consumes typed values.

## Consequences

* Pro: one source of truth; compile-time enforcement; themes become data;
  CI can verify contrast floors by reading the TOML.
* Pro: agents and teams consume identical names everywhere; doc drift is a
  mechanical defect, not an interpretation issue.
* Con: token changes are heavier (ADR + TOML + docs + generator emit);
  accepted as intentional friction.
* Con: the generator is a new `loom-core` dependency; mitigated by
  specifying the emit contract now (see `DESIGN_TOKENS.md` §3) so the
  gallery milestone consumes a stable format.

## Migration

No prior tokens exist; this is the initial contract. Future migrations
(adding themes, custom user themes) are value additions to the same file
schema (`THEMING.md` §6–7).

## Verification

* Consistency check: script compares token names/values across TOML and
  both documents (CI).
* Contrast check: CI computes WCAG contrast for palette pairs
  (`COLOR.md` §7).
* Emit check: generated `Tokens.slint` compiles in the `loom-core` UI
  crate fixture (gallery milestone).


## source-loom-plugin-sdk-accessibility-md

Original path: `loom-plugin-sdk/ACCESSIBILITY.md`

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


## source-loom-plugin-sdk-architecture-md

Original path: `loom-plugin-sdk/ARCHITECTURE.md`

# Architecture

## Layers

```text
loom-plugin-cli  (binary; arg parsing; fixture generation)
      |
      v
loom-plugin-host (store; safe zip install; permission checks)
      |
      v
loom-plugin-manifest (schema; validation; version compare)
      |
      v
serde / serde_json / sha2 / zip  (pinned, MIT/Apache-2.0)
```

Dependency direction is strictly downward: `manifest` depends on nothing from
this repo; `host` depends on `manifest`; `cli` depends on both.

## Data flow

1. A plugin package (`.loomplugin` zip) arrives as bytes.
2. `PluginStore::install_zip` pre-scans the archive: entry count <=
   `MAX_ENTRIES`, declared total <= `MAX_TOTAL_BYTES`, every name safe
   (relative, no `..`, no backslash), no symlink entries. Any violation
   aborts before a single byte is written.
3. `manifest.json` is stream-read (bounded) and parsed/validated by
   `loom-plugin-manifest`. The plugin API range must overlap the host's.
4. The wasm module named by `entry.wasm_module` must exist and be <
   `MAX_WASM_BYTES`.
5. All safe entries are extracted into `<store>/<id>@<version>/` with
   streaming caps. The manifest sha256 is recorded.
6. `installed.json` is an informational index regenerated from disk on every
   `open()`.

## Permission model

- `Capability` = coarse grant (must be in `manifest.capabilities`).
- `Permission` = fine-grained `(resource, mode, path_prefix)`.
- `check_permission(plugin, capability, path)` enforces both, resolving
  relative `path_prefix` values against the plugin install directory and
  comparing canonicalized path components (no partial-name prefix matches).
- `HttpRequest` additionally requires `resource_limits.network == true`.

## Error taxonomy

- `ManifestError`: parse/validation of the manifest document
  (Malformed, UnknownCapability, UnsupportedVersion, InvalidId, MissingField,
  TooLarge).
- `HostError`: store operations (Io, Zip, InvalidManifest, AlreadyInstalled,
  NotFound, UnsafePath, TooLarge, UnsupportedApi, Denied).

## Non-goals (this milestone)

- WASM execution (BLOCKED per RFC-0009).
- Plugin signing (designed, NOT_STARTED).
- Remote anything.


## source-loom-plugin-sdk-building-md

Original path: `loom-plugin-sdk/BUILDING.md`

# Building

## Requirements

- Rust stable >= 1.80 (developed against 1.97.1). Verify: `rustc --version`.
- No system libraries, no network access needed to build, test, or run.
  `cargo` needs network only for the first dependency download.

## Build

```sh
cargo build                       # debug
cargo build --release             # release (LTO thin)
```

## Test fixtures

The demo plugin package is generated, never committed:

```sh
cargo test -p loom-plugin-cli      # writes target/fixtures/demo.loomplugin
```

This creates `crates/loom-plugin-cli/target/fixtures/demo.loomplugin` from
the committed sources under `fixtures/demo/`
(`manifest.json`, 8-byte `module.wasm`, `assets/notes.txt`).

## Using the CLI

```sh
cargo run -p loom-plugin-cli -- validate crates/loom-plugin-cli/fixtures/demo/manifest.json
cargo run -p loom-plugin-cli -- install crates/loom-plugin-cli/target/fixtures/demo.loomplugin --dir /tmp/loom-store
cargo run -p loom-plugin-cli -- list --dir /tmp/loom-store
cargo run -p loom-plugin-cli -- remove demo-actions --dir /tmp/loom-store
```

## Offline operation

The binaries make zero network calls. For a fully offline build, prime the
cargo cache once (`cargo fetch` online), then build with
`cargo build --offline`.


## source-loom-plugin-sdk-changelog-md

Original path: `loom-plugin-sdk/CHANGELOG.md`

# Changelog

## 0.1.0 (2026-08-01) — foundation milestone

### Added

- `loom-plugin-manifest`: `PluginManifest` schema (entry points, capabilities,
  permissions, resource limits), validation with structured error taxonomy
  (`Malformed`, `UnknownCapability`, `UnsupportedVersion`, `InvalidId`,
  `MissingField`, `TooLarge`), size-limited parsing, dotted-numeric
  `compare_versions` / `version_compatible`, serde round trips.
- `loom-plugin-host`: `PluginStore` (open/install/list/get/uninstall),
  defensive install (name pre-scan, archive-bomb guards, bounded streaming
  copies, manifest-first validation, wasm size cap, sha256 recording,
  cleanup-on-failure, informational `installed.json` regenerated from disk),
  `permissions_for` / `check_permission` with component-aware prefix
  matching and network-limit gating.
- `loom-plugin-cli`: `loom-plugin` binary with `validate`, `install`, `list`,
  `remove` (hand-rolled arg parsing); demo fixture generation producing
  `target/fixtures/demo.loomplugin` from committed text sources.
- RFC-0009 (plugin ABI and sandboxing): accepted as architecture; runtime
  implementation BLOCKED pending wasmtime pinning.

### Known limitations

- No WASM execution (by design, this milestone).
- No plugin signing (designed in RFC-0009, not implemented).
- `list()` silently skips corrupt installs rather than reporting them.


## source-loom-plugin-sdk-contributing-md

Original path: `loom-plugin-sdk/CONTRIBUTING.md`

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


## source-loom-plugin-sdk-dependencies-md

Original path: `loom-plugin-sdk/DEPENDENCIES.md`

# Dependencies

## Direct

| Crate | Version | Purpose | License |
| --- | --- | --- | --- |
| serde | 1.0.229 (locked) | schema derive | MIT OR Apache-2.0 |
| serde_json | 1.0.151 (locked) | manifest + index JSON | MIT OR Apache-2.0 |
| sha2 | 0.10.x | manifest_sha256 | MIT OR Apache-2.0 |
| cap-fs-ext | 3.4.6 (locked) | no-follow directory operations for plugin writes | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| cap-std | 3.4.6 (locked) | anchored directory handles and atomic plugin-write publication | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| zip | 0.6.6 (locked) | `.loomplugin` read/write | MIT |

## Feature trimming

`zip` is built with `default-features = false, features = ["deflate"]`,
removing aes-crypto/bzip2/zstd/time deps — the format used by Loom packages
is store/deflate only, and the host reads user-supplied zips with an
explicit allowlist of compression methods that the runtime milestone will
extend only with justification.

## Replacement strategy

- `zip`: active project (github.com/zip-rs/zip2); if unmaintained, a
  maintainer fork or hand-rolled zip reader (store/deflate only) is a viable
  fallback because our reader surface is small (name/size/mode + stream).
- `serde`/`serde_json`: ubiquitous; replacement only if licensing changes.
- `sha2`: RustCrypto, maintained; any CRC/SHA impl would do — the API is
  isolated in `install_zip`.

## Audit

`Cargo.lock` is committed. `cargo tree -d` shows no duplicate major
versions. A `cargo deny` workflow will be adopted with loom-core; until
then, dependency additions are reviewed per `CONTRIBUTING.md`.


## source-loom-plugin-sdk-implementation-guide-md

Original path: `loom-plugin-sdk/IMPLEMENTATION_GUIDE.md`

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


## source-loom-plugin-sdk-license-policy-md

Original path: `loom-plugin-sdk/LICENSE_POLICY.md`

# License Policy

## Code

All original code in this repository is dual-licensed **MIT OR Apache-2.0**
(workspace-level `license = "MIT OR Apache-2.0"`, matching the Loom suite).

Each file carries no individual headers by convention; the workspace
manifest is authoritative.

## Dependencies (direct)

| Crate | Version | License | Notes |
| --- | --- | --- | --- |
| serde | 1.x | MIT OR Apache-2.0 | feature `derive` |
| serde_json | 1.x | MIT OR Apache-2.0 | |
| sha2 | 0.10 | MIT OR Apache-2.0 | |
| zip | 0.6.x | MIT | `default-features = false`, `deflate` only (drops bzip2/zstd/aes deps) |

All are permissive and compatible with MIT OR Apache-2.0. No dependency
forces a copyleft or viral license. Run `cargo deny` (when adopted by
loom-core) as a CI gate.

## Fixtures and assets

- The demo manifest, notes asset, and 8-byte wasm header are original,
  created within the project.
- No commercial, proprietary, or sample media is included.

## Constraints

- Adding a dependency with a non-permissive license requires an ADR and
  isolation behind a feature flag, per Loom's licensing policy.
- Model packs (future) must not bundle models whose licenses forbid
  redistribution (Loom Vision policy, RFC-0011).

## Verification

`cargo tree -d` + the pinned `Cargo.lock` are the dependency inventory.
Rebuilds are reproducible via the committed lockfile.


## source-loom-plugin-sdk-performance-md

Original path: `loom-plugin-sdk/PERFORMANCE.md`

# Performance

## Budgets

Measured on mainstream hardware (CI container, debug build):

| Operation | Budget |
| --- | --- |
| `parse_manifest` (10 KB doc) | < 1 ms |
| `install_zip` (1 MiB package) | < 100 ms |
| `list()` with 50 installed plugins | < 10 ms |
| `check_permission` | no filesystem syscalls when the path exists (canonicalize short-circuits); lexical normalization otherwise |

These budgets are not yet enforced by benchmarks; add them when the
`loom-plugin-cli` gains a `bench` subcommand.

## Design choices for speed

- Install pre-scans the central directory once (no per-entry re-open).
- Extraction streams (`io::copy` over `Read::take`), no whole-archive
  buffering; memory stays O(largest entry).
- Permission prefix checks compare path components after a single
  canonicalization; no repeated syscalls.
- `installed.json` regeneration only touches directory entries that look like
  `id@version`.

## Memory

- Archive-bomb limits cap worst-case disk and memory regardless of declared
  sizes (streaming caps are the truth).
- The zip test fixture is 8-byte wasm + small text; `cargo test` uses
  negligible memory.


## source-loom-plugin-sdk-roadmap-md

Original path: `loom-plugin-sdk/ROADMAP.md`

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


## source-loom-plugin-sdk-security-md

Original path: `loom-plugin-sdk/SECURITY.md`

# Security

## Threat model

A plugin package is untrusted input. Attackers may try: zip-slip (path
traversal), archive bombs, symlink escapes, oversized manifests/wasm,
malformed JSON to confuse validation, capability confusion, and path-prefix
confusion. This milestone's host never executes plugin code, so remote-code
execution is out of scope until the WASI runtime milestone.

## Controls (implemented)

- **Path safety**: every entry name pre-scanned — must be relative, no `..`
  or `.` components, no backslashes, no leading `/`. Rejected before any
  write.
- **Symlink entries**: unix mode bits checked (`S_IFLNK`); real archivers'
  symlink entries are rejected at install.
- **Archive bombs**: `MAX_ENTRIES = 1024`, `MAX_TOTAL_BYTES = 256 MiB`
  declared-size checks plus streaming caps (`take(limit + 1)`) on every copy.
- **Manifest trust**: parsed and validated before extraction; unknown
  capabilities rejected; `manifest_version` must be exactly 1.
- **Wasm size**: `< 100 MiB`, presence verified before extraction.
- **Atomicity**: failed installs remove the partial install directory; the
  store is never left half-written.
- **Permissions**: enforced in the host library (never delegated to
  plugins); canonicalized component-wise path comparison prevents
  `/a/b` vs `/a/bc` prefix confusion; relative prefixes resolve against the
  plugin install dir.
- **API negotiation**: plugin api range must overlap the host's; mismatches
  block install.
- **Index**: `installed.json` is informational and regenerated from disk, so
  tampering with it changes nothing.

## Policies

- No `unsafe` (enforced by `#![forbid(unsafe_code)]`).
- No network code anywhere in the repo.
- Temp dirs are created via `std::env::temp_dir()` + unique names; test
  helpers always clean up via Drop.

## Future (see RFC-0009)

- WASM sandbox import boundary, trap recovery, watchdog timers.
- Per-plugin process isolation.
- Ed25519 plugin signing with local keyring; unsigned plugins gated by user
  consent.


## source-loom-plugin-sdk-tasks-md

Original path: `loom-plugin-sdk/TASKS.md`

# Tasks

Small, independently verifiable tasks. Format: ID — Title (status).

## Manifest

- M-01 Immutable `PluginManifest` schema with serde (COMPLETE).
- M-02 Hand-rolled plugin-id regex check (COMPLETE).
- M-03 `version_compatible` dotted-numeric compare + matrix tests (COMPLETE).
- M-04 Error taxonomy incl. UnknownCapability via custom Deserialize (COMPLETE).
- M-05 Size-limited `parse_manifest_with_limit` (COMPLETE).
- M-06 Serde round-trip + canonical kebab-case output tests (COMPLETE).

## Host

- H-01 `PluginStore::open` + index regeneration (COMPLETE).
- H-02 Install pre-scan: names, count, declared total (COMPLETE).
- H-03 Manifest read-bounded + validated before extraction (COMPLETE).
- H-04 Api-range overlap check (COMPLETE).
- H-05 Bounded streaming extraction with cleanup on failure (COMPLETE).
- H-06 Symlink-entry rejection via unix mode bits (COMPLETE).
- H-07 `check_permission` with component-aware prefix matching (COMPLETE).
- H-08 Hostile-archive integration tests with "nothing extracted" asserts (COMPLETE).

## CLI

- C-01 Hand-rolled subcommand parsing with usage exit code 2 (COMPLETE).
- C-02 validate/install/list/remove against the real binary (COMPLETE).
- C-03 Fixture generation test writing `target/fixtures/demo.loomplugin` (COMPLETE).

## Backlog (blocked or future)

- R-01 wasmtime pinning decision (BLOCKED: needs dependency review).
- R-02 WASI instantiation + `loom_host_*` import boundary (NOT_STARTED).
- R-03 Watchdog + memory-limit enforcement (NOT_STARTED).
- S-01 Ed25519 package signing + local keyring (NOT_STARTED).
- P-01 Process-per-plugin isolation (NOT_STARTED).
- P-02 CLI `bench` subcommand with enforced budgets (NOT_STARTED).
- P-03 Cross-version plugin fixture corpus (NOT_STARTED).

## Definition of done

`cargo fmt --check`, clippy `-D warnings`, `cargo test --workspace`,
`cargo build --release` all green; tests assert real behavior; status
reflects reality.


## source-loom-plugin-sdk-testing-md

Original path: `loom-plugin-sdk/TESTING.md`

# Testing

## Commands

```sh
cargo fmt --check                                  # formatting
cargo clippy --all-targets -- -D warnings          # zero warnings required
cargo test --workspace                             # all unit + integration tests
cargo build --release                              # release gate
```

## Test inventory

| Crate | Scope | What it proves |
| --- | --- | --- |
| manifest (unit) | 24 tests | validation error matrix (each rule -> its error), version matrix, serde round trip, id/mode/path helpers |
| host (unit) | 4 tests | safe-name rejection, path normalization, component-aware prefix matching |
| host (integration) | 19 tests | install+sha, double-install, traversal/absolute/symlink rejection with "nothing extracted" assertions, bad manifest, missing manifest/wasm, api mismatch, archive-bomb limit, uninstall, corrupt-dir skip, permission matrix, index regeneration |
| cli (unit) | 2 tests | fixture sources valid, generated zip installs cleanly |
| cli (integration) | 8 tests | real binary: validate ok/bad/missing, install/list/remove round trip, malicious zip rejection, usage errors |

Total: 57 tests.

## Conventions

- Security rejections assert the store is untouched afterwards.
- No test may require network, a GPU, or a committed binary fixture.
- The symlink-entry fixture is crafted as raw zip bytes (zip 0.6.6's writer
  masks file-type bits); see `make_symlink_zip` in host integration tests.
- Temp dirs are hand-rolled with Drop cleanup; never leave files behind.


## source-loom-plugin-sdk-visual-qa-md

Original path: `loom-plugin-sdk/VISUAL_QA.md`

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


## source-loom-plugin-sdk-docs-rfcs-rfc-0009-plugin-abi-and-sandboxing-md

Original path: `loom-plugin-sdk/docs/rfcs/RFC-0009-plugin-abi-and-sandboxing.md`

# RFC-0009: Plugin ABI and Sandboxing

- Status: **Accepted as architecture; runtime implementation BLOCKED** until
  the wasmtime pinning decision is made (see Open Questions).
- Author: Loom Plugin SDK lead.
- Created: 2026-08-01.
- Related RFCs: RFC-0001 (repository/versioning), RFC-0010 (Loom Vision
  provider model), RFC-0011 (model-pack format).

## Context

Loom is a local-first creative suite. Third-party extension points (commands,
importers, exporters, effects, generators, inspectors, vision providers,
document and media processors) must be safe to install and run on user
machines. The extension system must never require a network connection, must
crash-isolate misbehaving plugins, and must give users a clear, verifiable
permission model.

The current milestone delivers the package/manifest/host foundation: a
validated manifest schema, a defensive zip installer, a directory-backed
store, and a permission-checking API. Nothing in this milestone executes
plugin code.

## Goals

- Define the WASM32-WASI plugin module ABI used by Loom hosts.
- Define the capability negotiation model (api_min/api_max).
- Define resource limits (memory, fs, cpu, network) and their enforcement
  points.
- Define a sandboxing strategy with a clear evolution path from in-process
  WASM sandboxing to per-plugin process isolation.
- Deliver install-time security: archive guards, path validation, manifest
  validation, checksum recording.

## Non-Goals

- Shipping a WASI runtime in this milestone. `loom-plugin-host` must not
  execute wasm.
- Plugin signing/public-key trust in this milestone. The signing architecture
  is designed (see below) but not implemented.
- Remote plugin marketplaces, accounts, or network update checks.
- Native (non-WASM) plugin binaries in this milestone.

## Proposed design

### Module ABI

Plugins are WASM32-WASI modules (component-model style eventually; core-wasm
with the `wasi_snapshot_preview1` ABI initially). The host imports and the
guest exports the following symbols:

```text
guest exports:
  (func loom_plugin_init  (export "loom_plugin_init")  (param i32) (result i32))
  (func loom_plugin_invoke(export "loom_plugin_invoke")(param i32 i32) (result i32))

host imports (namespace "loom_host"):
  loom_host_log, loom_host_file_open/read/write, loom_host_read_dir,
  loom_host_clipboard_get/set, loom_host_vision_infer,
  loom_host_temp_dir, loom_host_state_dir, loom_host_http_request, ...
```

- `loom_plugin_init` receives a pointer to the serialized manifest context
  and returns a status code (`0` = ok).
- `loom_plugin_invoke` receives `(command_id, payload_ptr)` and returns a
  status code. All payload exchange is via linear memory plus an
  out-parameter size struct, never via guest-chosen host addresses.
- Every `loom_host_*` call validates the guest-provided pointer range before
  touching host state. This is the hard trust boundary inside the process.
- The host maps a plugin's declared capabilities to the set of import
  functions it links in. Capabilities are negotiated at instantiation from
  `api_min_version`/`api_max_version` in the manifest.

### Capability negotiation

`manifest.api_min_version..=manifest.api_max_version` must overlap the
host's `HOST_API_MIN_VERSION..=HOST_API_MAX_VERSION` (checked at install time
and again at load time). A mismatch blocks installation with a clear error.

### Resource limits

Enforced at three layers:

1. Install time (manifest-declared and host caps): entry count, total bytes,
   wasm size.
2. Runtime (in-process sandbox): guest memory via the runtime's memory limits,
   call duration via a watchdog timer, fs byte/entry quotas inside the
   `loom_host_*` wrappers.
3. Process boundary (later phase): OS-level rlimits, cgroup/quota for fs, and
   network namespace.

`resource_limits.network` gates the `loom_host_http_request` import; a plugin
without it cannot even link the import.

### Crash isolation

Phase 1: in-process WASM sandbox (wasmtime) — guest traps are caught and
reported; host state is protected by validated pointer ranges and the import
boundary. Phase 2: one OS process per plugin (or per plugin family), with a
Unix-socket/RPC-style command channel; guest crashes then cannot take down the
host. Phase 2 is the release target for running third-party code.

### Install-time security (implemented)

Entry-name checks (`..`, absolute, backslash, symlink bits), `MAX_ENTRIES`,
`MAX_TOTAL_BYTES`, bounded streaming copies, manifest parse+validate before
extraction, wasm size cap, sha256 of the manifest recorded, `installed.json`
index regenerated from disk.

### Signing architecture (designed, not implemented)

Plugin packages may declare an optional `signature` block: an Ed25519
signature over the sorted manifest document + module bytes, with a public key
supplied in a sidecar `.sig` file or the manifest. Verification happens at
install time only when a keyring is configured; unsigned plugins are allowed
with an explicit trust prompt. No central authority; users add keys locally.

## Alternatives

- **Process-per-plugin only** (no in-process WASM): simpler isolation story
  but much higher per-call latency and memory cost; rejects the fast path for
  trusted plugins. Rejected as the sole strategy; adopted as the eventual
  default for third-party plugins.
- **wasmtime vs wasmtime-go vs custom interpreter**: wasmtime (Rust, WASI,
  cranelift, actively maintained, Apache-2.0) is the candidate runtime;
  wasmtime-go would force a Go dependency into the suite; a custom
  interpreter is unmaintainable. Decision is pending a dependency-review pass
  per the loom-core dependency policy, which is why runtime work is BLOCKED.
- **Native plugins (cdylib)**: rejected — no isolation without heavy
  machinery (seccomp/landlock) and worse portability across Linux distros.
- **POSIX seccomp sandboxing as the only mechanism**: rejected for the first
  release; revisit for process-isolation phase.

## Trade-offs

- In-process WASM has the best performance but a wider attack surface than
  process isolation; mitigated by the import boundary and by graduating to
  process isolation for third-party plugins.
- Manifest validation duplicates work between `loom-plugin-manifest` and
  `loom-plugin-host`; kept separate so the manifest crate is a pure,
  sandbox-safe library.
- Hand-rolled version comparison (no semver crate) is less precise than full
  semver but deliberately tolerant (missing parts = 0) for forward
  compatibility; documented in the manifest crate.

## Security

- No `unsafe` in any crate of this repository.
- All archive extraction is bounded, name-validated, and pre-scanned before
  any write.
- Permission checks are enforced in the host library, not delegated to the
  plugin.
- No network code exists in this repository.

## Performance

- Install is single-pass over the archive; extraction is streaming.
- Permission checks avoid filesystem syscalls when the path exists only
  lexically (canonicalize-on-exists).
- Budget: install of a 1 MiB package < 100 ms on mainstream hardware (to be
  measured in the runtime milestone).

## Compatibility

- `manifest_version` is versioned; unknown versions are rejected with a
  structured error, never guessed.
- Hosts must be able to run plugins built against older `api_min` versions
  within the supported window.

## Migration

- Store layout `id@version/manifest.json` is stable from this milestone.
- A future runtime milestone adds a `state/` and `temp/` directory per plugin
  without changing the manifest schema.

## Testing

- Manifest: validation matrix, error taxonomy, version-compatibility matrix,
  serde round trips (done).
- Host: safe install, hostile archives (traversal, absolute, symlink),
  archive-bomb limits, double install, uninstall, permission matrix,
  index regeneration (done).
- Future: wasm execution tests, trap recovery, cancellation, cross-version
  fixture corpus.

## Open questions

- wasmtime pinning: version, feature set (`wasi-common` vs `wasmtime-wasi`),
  and MSRV impact — BLOCKING the runtime milestone.
- Component-model adoption timeline vs core-wasm + `wasi_snapshot_preview1`.
- Whether plugins may host their own threads (wasmtime supports it) and how
  that interacts with cpu limits.

## Final status

**Accepted as architecture.** Manifest, host, and CLI implementations are
complete and tested. Runtime execution and signing are documented as
`NOT_STARTED` in `ROADMAP.md` and must not be represented as done.


## source-loom-samples-conformance-markdown-notes-md

Original path: `loom-samples/conformance/markdown/notes.md`

# Conformance

A Markdown corpus document with a [link](https://example.test/x).


## source-loom-sheets-performance-md

Original path: `loom-sheets/PERFORMANCE.md`

# Loom Sheets performance gate

These checks use the fixed budgets in [`loom-design-bible/PERFORMANCE.md`](AGENTS.md#source-loom-design-bible-performance-md) and [`SPREADSHEET.md`](AGENTS.md#source-loom-design-bible-spreadsheet-md). A passing formula-engine test does not prove that scrolling, typing, or the live window stays responsive.

## Required budgets

- Recalculate 10,000 chained formulas in under 200 ms on the mainstream desktop profile.
- Give the user visible input feedback within one 60 Hz frame (16.7 ms). No calculation or file operation may block the UI thread.
- Scroll a worksheet containing 1,000,000 randomly placed values at 60 fps on the mainstream profile.
- Measure peak resident memory on that same representative workbook. The repository's higher-level performance contract requires each app to name a memory budget, but it does not provide a numeric Sheets limit yet. Until that limit is reviewed and recorded, the memory part of this gate is open.

## Formula benchmark

The `perf_measure` integration test creates 500 rows with 20 chained formulas per row. It checks that the formula count is exactly 10,000 and that the last value is correct.

Run a quick correctness and timing sample without a budget assertion:

```sh
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked \
  -p loom-sheets-core --test perf_measure -- --nocapture
```

Run the 200 ms budget check in the optimized test profile:

```sh
LOOM_ENFORCE_PERF_BUDGET=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked --release \
  -p loom-sheets-core --test perf_measure -- --nocapture
```

### Latest local result — 2026-09-24

On the local Intel Core i3-2350M (2 cores, 7.7 GiB RAM), the latest optimized 10,000-formula test passed: calculation **88.5 ms**, JSON save preparation **20.4 ms** for 319,175 bytes, and JSON parse **10.4 ms**. The command used the release profile and the explicit 200 ms assertion. Raw output is `.work/sheets-acceptance-2026-09-24/formula-perf-rerun.log`. This is a recorded result on this machine; it does not establish the separate mainstream desktop profile.

## Million-cell projection sample — 2026-09-24

The opt-in app test `million_sparse_cells_project_one_viewport_within_one_frame` fills a 2,048×2,048 address space with 1,000,000 sparse numeric cells. A fixed-key Feistel permutation assigns each cell a unique, random-looking address without allocating a second million-entry index. It checks all four workbook regions, then projects the visible cells at 60 positions for a 1,024×720 viewport, including the origin and clamped far corner. It asserts the visible-cell count is exact and each CPU projection finishes below 16.7 ms.

Run the optimized assertion with:

```sh
LOOM_ENFORCE_SCROLL_BUDGET=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline --release \
  -p loom-sheets-app million_sparse_cells_project_one_viewport_within_one_frame -- --nocapture
```

On the same Intel Core i3-2350M, the standalone release test measured **0.17 ms p95** and **0.26 ms maximum** across the 60 CPU-side projections. `/usr/bin/time -v` measured **90,308 KiB peak RSS** for the test process, which included the one-million-cell fixture and test harness; it reported **0 swaps**. This is a test-process measurement, not the complete interactive app's memory use.

This is not a real-window scroll result: it excludes Slint rendering, native frame scheduling, compositing, and frame presentation. The native one-million-cell scroll check and a numeric owner-reviewed app memory cap remain open.

## App-level million-cell edit sample — 2026-09-24

The opt-in app test also commits one cell in the same one-million-cell workbook. It records the UI's edit preparation, mailbox submission, worker evaluation, package creation, and recovery-journal write as separate timings. Run it with:

```sh
LOOM_ENFORCE_SCROLL_BUDGET=1 SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline \
  -p loom-sheets-app million_sparse_cells_project_one_viewport_within_one_frame -- --nocapture
```

On the Intel Core i3-2350M (2 cores, 7.7 GiB RAM), the test-profile run projected 60 CPU-side viewports at **1.39 ms p95 / 1.43 ms maximum**. For the edit, the test timed only `commit_formula_edit` preparation (**0.049 ms**) and worker mailbox submission (**0.037 ms**; 0.086 ms combined). Those two steps are below 16.7 ms, but the test does **not** time the full formula-bar callback, selection/workbook synchronization, menu/status updates, Slint model projection, repaint, or native frame presentation. It therefore does not establish end-to-end input feedback or a visible frame within 16.7 ms. The worker then spent **2,561.653 ms** evaluating formulas, **12,721.371 ms** creating the recovery package, and **43,046.444 ms** writing the recovery journal. `/usr/bin/time -v` measured **2,612,252 KiB peak RSS** and zero swaps for the test process. This test-profile number includes the million-cell fixture and test harness; it is not interactive-app RSS. The test removes its temporary recovery directory when it exits. Exact raw output: `.work/sheets-acceptance-2026-09-24/cell-commit-perf-time.log`.

The result proves only that the two measured cell-commit steps are quick. Recovery can lag badly on this million-cell workbook, and that lag creates a crash-recovery window; package and journal costs are still too high at this scale. Regular Save/Open and XLSX/CSV work still run synchronously, and non-cell edits still copy the workbook to send a replacement to the worker. Keep those as separate blockers. The CPU projection and test-process RSS do not prove native 60 fps or the full app's memory limit. Sheets remains `ACCEPTANCE_BLOCKED`.

## Recovery storage and freshness gate — 2026-09-24

The current recovery path saves a complete workbook package after each changed worker batch. It appends those packages to the journal and compacts them only when the user explicitly saves. There is no automatic record-count or byte limit, so unsaved work can grow recovery storage without bound. The journal append also rereads and validates previous records each time, making later edits more expensive as the journal grows. This matters on a personal device with limited free space.

The one-million-cell app test measured **2.56 seconds** of evaluation, **12.72 seconds** of package creation, and **43.05 seconds** of journal writing after the edit was queued. It did not retain or report the test journal's byte size. The newest visible edit can therefore wait tens of seconds before its recovery record is durable. The app must measure both bytes retained during a long unsaved session and the time from accepted edit to durable recovery.

Acceptance requires a reviewed finite storage bound, exact restart recovery, and interruption/failure tests proving that automatic compaction never removes edits newer than a verified checkpoint. Include disk-full, torn-write, corrupt-record, and forced-restart cases. Preserve the existing full-package journal until a compatible replay design is reviewed; see `TRUTH.md` REC-02. Do not count a quick formula-bar response as proof that the edit is safely recoverable.


## source-loom-sheets-docs-qa-reports-2026-09-24-acceptance-follow-up-md

Original path: `loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md`

# Loom Sheets acceptance follow-up — 2026-09-24

## 2026-09-25 UI repair addendum

UI-28's empty warning-dialog space is fixed. The dialog now grows to its content, caps at the window limit, and scrolls only long warning text while keeping both actions visible. Text scale is inherited from the app. A regression checks a short warning, 2× text, and a 30-line warning at 1024×720; the live 1018×744 Linux screenshot shows the warning at 2× text scale. Live focus, Escape, and button effects remain open under CODE-23.

UI-29's centered menu rows and oversized short-menu popup are fixed in the renderer. Sheets now uses the shared Loom UI Foundation component `LoomMenuItem`; its canonical public import is `foundation.slint`. The row exposes separate label and shortcut fields, with check, selected, and disabled states. It is demonstrated in the shared Overlays gallery for other Loom applications to reuse. Popup height now follows visible content and remains capped to the window. The regression first found the label beginning at x=110 and a 487 px blank run; the corrected renderer capture shows a left-aligned label, right-side shortcut, and compact panel. Native popup interaction and keyboard behavior remain open under UI-01.

Fresh verification: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline -q` passes **267 tests** (143 app, 2 evaluation-cache integration, 104 core, 13 formula, 1 performance fixture, and 4 range tests). The production app builds. Live warning capture: `loom-sheets/docs/qa-native/ui28-xlsx-warning-live-linux.png`, SHA-256 `3e78058c6ddf36c690816bf54480b4ad97f7b9a7bb33fbb7e49f560ed3a33e02`. Renderer captures: `ui28-xlsx-warning-short-1024x720-linux.png`, `ui28-xlsx-warning-short-2x-1024x720-linux.png`, `ui28-xlsx-warning-long-2x-1024x720-linux.png`, and `ui29-menu-popup-aligned-1024x720-linux.png` under `loom-sheets/docs/qa-renderer/`. The menu screenshot is renderer evidence only. Native popup activation, dialog focus/actions, and the remaining Sheets acceptance checks are still open.

## Status

Sheets is **not 100% accepted**. After the shared ZIP-reader change, the Sheets workspace passes 266 tests, `loom-package` passes 29 tests, full-workspace all-target Clippy passes, and the production app builds. The latest build took 3m07 and peaked at 2,985,580 KiB RSS with zero swaps. CODE-25 fixes a real import blocker: a LibreOffice-generated `.xlsx` round-trip now imports four sheets, formulas, the styled total, a chart, and a shape label. CODE-23 still warns before a known lossy XLSX import replaces the current workbook, including when started with `--open`. CODE-24 fixes exports whose style table was not linked from the workbook. REC-02 remains open because full recovery packages accumulate without an automatic storage bound. The formal gate stays `ACCEPTANCE_BLOCKED`: a fresh native capture now shows the live menu row and the XLSX import-warning dialog, but the popup and focus/action behavior were not exercised. Recovery on a million-cell workbook takes tens of seconds, ordinary file work and full workbook replacements remain synchronous on the UI thread, and native frame rate, app memory, full visual/accessibility coverage, and broader interoperability are not proven.

## What changed

- The in-window File/Edit/View/Table/Help menu now gives the popup only the selected menu's rows. The old code let hidden rows from other menus take up space, so Edit appeared far below its menu label. The old native Edit capture is preserved as `loom-sheets/docs/qa-native/ui01-edit-menu-open-before-fix-linux.png`; it is before-fix evidence.
- XLSX export now gives sanitized sheet names unique names and rewrites formula and chart references to the names actually written to the file. For example, source tabs `A/B` and `AB` export as `AB` and `AB (2)` instead of silently colliding.
- XLSX export now links the generated style table from the workbook relationships. Before this repair, LibreOffice ignored the emitted cell styles because the relationship to `xl/styles.xml` was missing even though the XML part and content-type entry were present.
- Dynamic-array spill placement now uses row and column dimensions in the right order. A 2×3 `SEQUENCE` fills B1:D2 and formulas that read those cells before the spill are recalculated. A blocked spill leaves no partial values behind.
- The formula evaluator skips spill-reader bookkeeping when a workbook has no array formulas. The active sheet's calculated values are also reused for view-only updates (selection, scrolling, and resize) and recalculated after edits or a tab change.
- Formula-bar cell commits now send one revision-tagged cell delta to the workbook worker instead of cloning the workbook, recalculating, and writing recovery on the UI thread. The UI immediately shows `Calculating…`, keeps the previous calculated values visible, rejects stale/wrong-tab results, and preserves a newer formula draft.
- XLSX import now reports known losses from OOXML before replacing the current workbook. The warning covers defined names, external links, conditional formatting, validation rules, PivotTables, frozen panes, custom row/column sizes, additional charts on one sheet, and drawing/media parts that are missing. Both the file picker and startup `--open` use the same warning. Cancel leaves the current workbook and recovery data untouched; Continue imports supported content and names the dropped features. Automated fixtures cover every warning and a supported file that should not warn; an empty `<definedNames/>` container also correctly produces no warning. The shared command dispatcher blocks menu and palette actions while the decision is pending and rechecks queued native actions. Escape cancels after Tab moves focus to a dialog button. A fresh native capture shows the visible menu row and warning dialog; the popup, focus, and button interactions remain unverified.
- The shared ZIP reader now accepts ordinary DEFLATE-compressed entries and ZIP data descriptors. Before this, a Calc-generated `.xlsx` failed before import with `unsupported compression method 8`. A real Calc-generated workbook now reaches the Sheets importer with four tabs, formulas, supported formatting, a chart, and a shape label. Expanded-size limits and CRC checks remain enforced; this fixes standard compression support, not the whole XLSX compatibility matrix.
- The source-size cleanup extracts CLI parsing, headless rendering, and XLSX import flow from the app entry point; splits app tests and Slint dialogs into focused files; and divides core XLSX handling into import, export, XML, style, relationship, and warning modules. During extraction, a Tab-then-Escape regression showed the Save Changes dialog lost its Escape handler when focus moved to a button. The dialog's focus scope now contains its overlay and controls.
- `loom-sheets/PERFORMANCE.md`, this report, the app README, `TRUTH.md`, and the tiny-model performance instructions in `AGENTS.MD` were updated with measured results and remaining limits.

## Automated and local verification

- Latest workspace tests: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline -q` — **266 passed, 0 failed**, covering 142 app tests, 2 cache integration tests, 104 core unit tests, 13 formula tests, 1 performance fixture, and 4 range tests. The scale test skips its million-cell setup unless `LOOM_ENFORCE_SCROLL_BUDGET=1` is set. Exact output: `.work/sheets-acceptance-2026-09-24/zip-deflate-workspace-tests.log`.
- Shared ZIP-package tests: `cargo test --manifest-path loom-core/Cargo.toml --locked --offline -p loom-package` — **29 passed, 0 failed**. Fresh package-only all-target Clippy: `cargo clippy --manifest-path loom-core/Cargo.toml --locked --offline -p loom-package --all-targets -- -D warnings` — passed. Logs: `.work/sheets-acceptance-2026-09-24/interop/zip-deflate-package-tests.log` and `zip-deflate-clippy.log`.
- Fresh full-workspace all-target Clippy: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig RUSTFLAGS='-L native=/tmp/loom-fontconfig' SLINT_EMIT_DEBUG_INFO=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` — passed in **2m15s**. Exact log: `.work/sheets-acceptance-2026-09-24/zip-deflate-workspace-clippy.log`.
- Reverse import smoke check: LibreOffice Calc **24.2.7.2** converted the expanded Loom workbook from ODS back to XLSX. `import_xlsx_sheets` then imported four sheets, formulas, B4 bold/yellow/bordered/right-aligned currency formatting, one chart, and the “Quarterly target” shape label. The only warning was `CustomRowColumnSizes`. Probe source and exact output: `.work/sheets-acceptance-2026-09-24/interop/import_calc_roundtrip.rs` and `import-calc-roundtrip-deflate.log`; the Calc-generated XLSX is in the portable bundle.
- Fresh production app build: `PKG_CONFIG_PATH=/tmp/loom-fontconfig LIBRARY_PATH=/tmp/loom-fontconfig SLINT_EMIT_DEBUG_INFO=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 /usr/bin/time -v nice -n 19 ionice -c3 cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` — succeeded in **3m07**. Peak RSS was **2,985,580 KiB** with zero swaps. Binary SHA-256: `bbfb0c4c42d018137eef43ae905c0928424cc11d16ee13992d5ff3504d90c9f4`. Exact output: `.work/sheets-acceptance-2026-09-24/zip-deflate-app-build-time.log`.
- CODE-23 TDD logs show the native-command bypass, empty-definedNames false warning, and focused-Escape path failing before their fixes and passing after them: `.work/sheets-acceptance-2026-09-24/xlsx-import-review-red-app.log`, `xlsx-import-review-red-core.log`, `xlsx-import-review-green-app.log`, `xlsx-import-review-green-core.log`, `xlsx-import-focus-red-probe.log`, and `xlsx-import-focus-green-probe.log`.
- Native capture (2026-09-24): `gnome-screenshot -w -f loom-sheets/docs/qa-native/ui23-xlsx-import-warning-live-linux.png`. The inspected 1018×744 image shows the real Loom window, the File/Edit/View/Table/Help row, the feature-loss warning, its source-file reassurance, and both decision buttons. SHA-256: `d2736f235b6eb463b49a93d89d086ff960460613eab6d294193417150a2f38d9`. The screensaver was inactive and no lock-screen image was taken. The warning text has large unused vertical space in this single-feature case. No menu/button/Tab/Escape interaction was performed: the available CUA interface exposes browser controls but no native-app controls, so popup, focus, and action behavior remain unverified.
- The current all-target workspace Clippy run passed after CODE-25 with Slint debug metadata. The production app build also passes after CODE-25. The separate repo-wide code-structure script confirms all four previously oversized Sheets sources meet their existing limits; it still reports six legacy byte-limit failures in locked apps, recorded in `.work/sheets-acceptance-2026-09-24/code-structure-after-split.log`.
- Renderer screenshot of the UI-01 menu at 1024×720: `loom-sheets/docs/qa-renderer/ui01-local-menu-current-1024-linux.png` (SHA-256 `4e556f6d3aeed49c2c62c5361ac29effb6f3dbea37ba4eb163ed7b1ec11744b`). It is software-rendered; the fresh native screenshot above confirms the row is visible but does not show the popup open.
- The focused tab-rename/tab-switch regression `rename_rewrites_qualifiers_and_rejects_collisions` passes. It checks the cross-sheet result stays 25 after the source tab is renamed.
- The evaluation-cache test checks that the same tab reuses one result, switching tabs recalculates, and an edit refreshes the values.
- Optimized 10,000-formula budget: `LOOM_ENFORCE_PERF_BUDGET=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline --release -p loom-sheets-core --test perf_measure -- --nocapture` — passed the 200 ms calculation threshold at **88.5 ms** on an Intel Core i3-2350M, 2 cores, 7.7 GiB RAM. JSON output was 319,175 bytes and took **20.4 ms**; parse took **10.4 ms**. Raw output: `.work/sheets-acceptance-2026-09-24/formula-perf-rerun.log`. This is one low-end local machine result, not the specified mainstream desktop profile.
- Million-cell CPU projection: the opt-in test projected 60 viewports over 1,000,000 numeric cells at unique pseudorandom addresses in a 2,048×2,048 address space, including both workbook corners and balanced cell coverage across all four regions. The debug run measured **1.27 ms p95 / 7.02 ms maximum**. The standalone release test measured **0.17 ms p95 / 0.26 ms maximum**. `/usr/bin/time -v` measured **90,308 KiB peak RSS** for the release test process, with zero swaps. These exclude native rendering and frame presentation; the observed RSS is not the whole interactive application's memory use. Exact release output: `.work/sheets-acceptance-2026-09-24/scroll-random-release-time.log`.
- Million-cell app edit sample: the opt-in test-profile run projected 60 viewports at **1.39 ms p95 / 1.43 ms maximum**, prepared a committed cell edit in **0.049 ms**, and queued it in **0.037 ms**. Worker evaluation took **2,561.653 ms**, recovery package creation **12,721.371 ms**, and recovery journal writing **43,046.444 ms**. Peak RSS for the test process was **2,612,252 KiB** with zero swaps; this includes the million-cell fixture and harness and is not an interactive-app RSS measurement. Exact output: `.work/sheets-acceptance-2026-09-24/cell-commit-perf-time.log`.
- Async result viewport regression: the test first failed because a current worker result reset the user's scroll offsets from `(-180, -672)` to `(0, 0)`. Results now update visible values without revealing the old selected cell; the regression passes and verifies scroll offsets and a concurrently typed formula draft remain unchanged. Red/green logs: `.work/sheets-acceptance-2026-09-24/viewport-preservation-red.log` and `.work/sheets-acceptance-2026-09-24/viewport-preservation-green.log`.
- Independent spreadsheet check: LibreOffice Calc **24.2.7.2** converted the expanded XLSX fixture to ODS. It applied the yellow fill, bold font, border, right alignment, and two-decimal currency format to B4; retained the four sanitized sheet names, chart frame, and shape label; recalculated cross-sheet formulas to `80`, `30`, and `60`; and kept `A/B!B2` as literal text. `assert_ods.py` checks these properties against the converted workbook. The earlier name-collision fixture also converted with the expected tab names and formula result. These results verify the listed fixture only, not Microsoft Excel or unrestricted XLSX compatibility. Fixture, source, converted output, assertion, and diagnostic probes are in the portable audit bundle.
- CODE-24 test-first evidence: `style-link-red.log` shows the relationship regression failing against the old exporter; `style-link-green.log` shows the same command passing after the repair. The red/green test command is the focused `loom-sheets-core` test for `xlsx_export_links_the_styles_part_from_the_workbook`.
- `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check` and `git diff --check` passed on the current source before this documentation refresh; they will be rerun after the report and archive are updated.
- The repo-wide `python3 loom-bootstrap/scripts/audit-code-structure.py` now reports only six legacy byte-limit failures in locked apps (Encode, Motion, Video app/core, Writer core, and Photo). It reports no Sheets source. Exact output: `.work/sheets-acceptance-2026-09-24/code-structure-after-split.log`. No app-wide size limit was relaxed.

An earlier app run omitted `SLINT_EMIT_DEBUG_INFO=1`; two tests that inspect Slint's accessibility tree could not see elements. That run was invalid for those assertions. The current rerun with Slint debug metadata passed all 142 app tests. Test-first regressions caught stale “Calculating…” feedback after a newer workbook revision, a viewport jump when a worker result arrived after scrolling, native commands reaching through the XLSX warning, false warnings for empty defined-name containers, and Escape failing after focus moved to a button in both the XLSX warning and Save Changes dialogs. The regressions now pass. Save Changes red/green logs: `.work/sheets-acceptance-2026-09-24/save-changes-escape-red.log` and `save-changes-escape-green.log`.

## Still blocking a truthful acceptance claim

- **Native menu and XLSX warning check:** UI-01 and CODE-23 source-level and app tests pass. The 2026-09-24 live capture verifies that the non-macOS menu row is visible and the warning dialog is readable with both actions. Popup menu layout/activation, dialog focus, Tab/Escape, and button effects remain unverified because the available CUA surface has no native-app controls. The screensaver is inactive; the remaining gap is interaction access, not the lock screen.
- **Visual scope:** all required viewports, themes, text sizes, and RTL states still need a complete inspected matrix. This device's live display is 1366×768, so a native 1440×900 capture is not available here.
- **Responsiveness:** selection, scroll, and resize reuse calculated values; a tab change recalculates. The benchmark timed only cell-edit preparation (0.049 ms) and mailbox submission (0.037 ms); it did not time the complete formula-bar callback, Slint model/status updates, or input-to-visible-frame delay. The million-cell worker then took 2,561.653 ms to evaluate, 12,721.371 ms to create a recovery package, and 43,046.444 ms to write the recovery journal. Recovery can lag far behind the visible edit and must be made fast enough to protect recent work. Non-cell edits still copy `Vec<Sheet>` on the UI thread. Ordinary Open, Save, Save As, CSV, and XLSX callbacks still parse/package/write synchronously.
- **File-work source audit:** `open_workbook_from_picker` reads and imports the full file before replacing the workbook; startup `--open` does the same before the event loop. `save_current_sheet` performs native serialization and atomic write on the UI thread, clones the saved baseline, serializes the package a second time, then blocks on `checkpoint().recv()`. CSV and XLSX exports also serialize and write on the UI thread. `record_workbook_snapshot` calls `workbook_sheets`, cloning every tab before queuing many non-cell changes; `apply_sheet` covers New/Open/template replacement, undo/redo, tab and row/column operations, range edits, formatting, objects, and sizing. Tab changes and transaction/dirty-state handling add further full-sheet/workbook clones. This audit confirmed source paths; it did not run new builds, tests, or GUI sessions.
- **Async file-ordering rule:** the current save blocks the UI, so edits cannot race its recovery checkpoint. Once Save is asynchronous, a checkpoint for saved revision N must not clear a durable recovery entry for later edit N+1. File commands need operation/document identifiers and a target edit revision; they must wait for queued edits through that revision, clone/serialize off the UI thread, and send completions through a dedicated queue that does not coalesce results. Open must parse a candidate and install only if that operation is still current and any import-loss warning was accepted. Save completion may update path/baseline only for its original document. File-write success and recovery-checkpoint failure must be reported separately. Moving an existing callback to a thread after it has cloned the full workbook on the UI thread does not fix the blocking cost.
- **Recovery storage and freshness:** source inspection confirms the worker appends a complete workbook package for each changed batch. `SnapshotRecovery::record` removes exact duplicates only; automatic compaction does not run, and the save checkpoint is the only caller path that compacts. The shared journal rereads and validates its existing records on each append. Therefore an unsaved session has no automatic storage bound, while each new record can cost more than the previous one. The million-cell test measured 2.56 seconds of evaluation, 12.72 seconds of package creation, and 43.05 seconds of journal writing; it did not measure retained bytes. REC-02 now tracks the missing storage bound, durable freshness, and interruption tests.
- **Scale and memory:** CPU projection of the pseudorandom million-cell fixture measured 1.39 ms p95 / 1.43 ms maximum in the test profile, but no real-window test proves native rendering and frame presentation sustain 60 fps. The full test process peaked at 2,612,252 KiB and used no swap; this includes its million-cell fixture and test harness, not an interactive app. Earlier standalone release projection RSS was 90,308 KiB and measured only that different test process. The app's peak-memory limit still needs an owner-reviewed numeric target.
- **Interoperability:** LibreOffice now verifies Loom XLSX export to ODS and back to XLSX, and Loom imports the resulting four-sheet workbook with its supported values, formatting, chart, and shape. Custom row/column sizes are warned about and dropped. Microsoft Excel and the wider import/export feature matrix remain unverified. CODE-23 detects listed known losses and requires confirmation in both the picker and startup `--open` path. Its warning fixtures, supported no-warning fixture, Cancel, Continue, and startup recovery preservation tests pass. The warning dialog has a fresh native visual capture, but its interactions remain unverified; this evidence does not prove full Excel compatibility, and imported Loom workbooks still omit features named in the warning after Continue.
- **Accessibility:** existing native evidence covers the named Linux flows in the 2026-09-23 report. Full keyboard command, screen-reader, dialog error/cancel, and cross-platform coverage remains open.

Keep the app gate `ACCEPTANCE_BLOCKED` until each of these items has its own observed evidence. Do not report absolute 100% based on unit tests or the single local benchmark.


## source-loom-sheets-docs-qa-reports-rec-02-recovery-design-review-md

Original path: `loom-sheets/docs/qa-reports/REC-02-recovery-design-review.md`

# REC-02 recovery format and limits — owner review

**Status: owner-approved design (2026-09-25); implementation outstanding.** The owner approved the ordered edit-batch format, limits, checkpoint policy, error wording, and pause-on-backlog behavior below. REC-02 remains open until implementation and required evidence pass. These values are policy ceilings and performance targets, not measurements or guarantees already achieved. Existing recovery data must remain readable while the new design is introduced.

The measured million-cell test spent 12.72 seconds creating a full recovery package and 43.05 seconds appending it to the journal. It did not measure how many bytes a long editing session retains. The values below are the approved policy and target; their enforcement and measured performance remain unverified.

| Decision | Approved value | What it means |
|---|---:|---|
| Recovery format | One complete package, followed by ordered, versioned cell-edit batches | Cell edits are small journal records. Structural changes initially write a new complete package. |
| Total retained recovery | 640 MiB | Sum of checkpoints, journal, metadata, and any legacy data kept during migration. |
| Temporary disk peak | 1 GiB | Includes old and new checkpoints, replacement journal, and temporary files while publishing safely. |
| Largest recoverable complete package | 256 MiB encoded | A larger workbook remains editable and can be saved by the user, but automatic recovery must say it is unavailable until the policy is changed. |
| Journal size | 64 MiB | Reaching the cap starts checkpointing; it must never delete the only valid recovery state. |
| Journal records | 10,000 total, 1 MiB per record | One oversized transaction uses the full-checkpoint path. A transaction must not be silently split. |
| Checkpoint trigger | 16 MiB, 2,000 records, or 5 minutes of edits | Use whichever limit is reached first, and checkpoint after a successful explicit save. |
| Checkpoint retention | Two complete generations; one extra only during publication | Publish and verify the new generation before removing the one it replaces. |
| Edit durability target | 250 ms p95 on the measured host | Sync the accepted edit batch before waiting for slow formula evaluation. Measure and report the worst case too. |

Each batch should contain a format version, workbook/session identity, baseline identity, predecessor durable sequence, and exact cell assignments or deletions. Keep UI revision numbers separate from recovery sequence numbers. Validate a whole batch before replaying it. Store formulas as formulas, and preserve complete packages for images and workbook structure.

Use one writer for each recovery session, with distinct session and workbook identifiers. Do not allow two app instances to write into the same ordered journal. A checkpoint for sequence N must never remove data for an accepted sequence greater than N.

Publish checkpoints in this order:

1. Choose the exact durable sequence covered by the new checkpoint.
2. Write its package and metadata, sync both, and verify them.
3. Atomically publish and sync the checkpoint pointer.
4. Replace the journal with records newer than the covered sequence.
5. Remove obsolete generations and temporary files only after the new state is safe.

On disk-full, cap, append, sync, or compaction failure, preserve the previous valid recovery data. Keep the in-memory workbook. Show **“Recent changes are not protected by recovery”** with **Retry** and **Save As** actions. Do not advance the durable sequence, discard a failed edit, or claim that the edit is recoverable. If bounded pending changes cannot be retained, stop accepting edits until a complete checkpoint succeeds.

Read legacy full-package journals without changing their format. To migrate, first publish and verify a complete baseline in the new versioned session directory; only then make the new format active. Count legacy bytes against the storage limit. If migration cannot fit, leave the old data untouched and report the limitation. Older binaries must not write into the new journal.

Required failure and correctness tests include exact restart reconstruction after cell edits, blank-cell deletions, formulas, tabs, undo/redo, replacements, and images; interruption after every write, sync, pointer change, compaction, and cleanup; torn tails and corrupt middle records; unsupported versions and wrong lineage; duplicate batches and failed writes; disk-full and cap enforcement during migration; two app instances; repeated failed compactions; and the same million-cell workload measuring durability latency, replay time, retained bytes, peak temporary bytes, and RSS.

## Owner decision (2026-09-25)

Approved as written: the ordered versioned cell-batch format with structural changes initially stored as complete packages; the 640 MiB retained limit, 1 GiB temporary peak, 256 MiB package limit, 64 MiB / 10,000-record journal limits, and 1 MiB record limit; the 16 MiB / 2,000-record / 5-minute checkpoint triggers, two-generation retention, and 250 ms p95 durability target; the recovery-error wording and pausing edit admission when bounded recovery cannot keep up.

Implement in small changes: versioned record encoding and replay; crash-safe checkpoint publication; legacy migration; bounded storage and truthful error UI; then failure injection and the unchanged performance workload. Do not mark REC-02 fixed until every step has measured, passing evidence.


## source-loom-spec-architecture-md

Original path: `loom-spec/ARCHITECTURE.md`

# Loom System Architecture

## 1. Repository map and dependency direction

Loom is a set of independent repositories with a strict dependency direction.
Versioned contracts flow from the bottom up; nothing above may be depended on
from below.

```text
loom-bootstrap   orchestration: builds, tests, Docker visual QA, packaging,
                 COMPATIBILITY.toml  → depends on all other repos
                     ▲
apps: loom-writer, loom-sheets, loom-present, loom-photo, loom-motion,
      loom-video, loom-studio, loom-encode
                     ▲ depends on: loom-core, loom-vision, loom-plugin-sdk
                     │             (never on each other)
loom-plugin-sdk  plugin manifest/host/sandbox  → depends on loom-core only
loom-vision      vision core + CLI (self-contained workspace)
loom-core        shared platform crates (loom-package, loom-document,
                 loom-color, loom-jobs, loom-command, loom-history,
                 loom-text, loom-storage) — depends on nothing in Loom
```

Reference-only repositories (no runtime dependency, no code depends on them):

- `loom-spec` — this repository; product and engineering specification.
- `loom-design-bible` — visual, motion, interaction, accessibility spec.
- `loom-samples` — original sample content for every application.

Rules:

- `loom-core` depends on nothing within Loom (only external crates).
- Applications never depend on each other; cross-app exchange uses packages,
  clipboard, or shared contracts.
- `loom-vision` and `loom-plugin-sdk` must never depend on an application.
- `loom-bootstrap` is the only repository that knows every repository.
- Repo-level pinning during development: path dependencies into `loom-core`
  (see `docs/adrs/ADR-0002-Path-Based-Crate-Pinning.md`); tagged releases
  later (see `COMPATIBILITY_POLICY.md`).

## 2. Command architecture

Every user action maps to a command with a stable identifier
(`loom-core/crates/loom-command`). Commands are the single path for menus,
shortcuts, command palette, context menus, accessibility invocation, and
plugin/scripting entry. A command declares:

- stable id, enablement state, checked state, undo description, localization
  key;
- execute/revert operations used by the history system
  (`loom-core/crates/loom-history`).

The UI layer sends commands; engines are headless and never know about Slint.
This separation is specified in `docs/rfcs/RFC-0002-UI-and-Engine-Separation.md`
and keeps every engine fully testable without a display.

## 3. Job framework

Long-running work runs as jobs (`loom-core/crates/loom-jobs`), never on the UI
thread. Jobs support progress, cancellation, priority, dependencies, error
reporting, retry where safe, and cleanup. Model and media work must be
cancellable and observable; cancellation feedback must appear immediately.
See `docs/rfcs/RFC-0008-Async-Job-Framework.md`. Jobs are the mechanism behind
autosave, recovery, export, transcoding, indexing, thumbnails, and model
inference.

## 4. Storage and package format

All Loom documents are ZIP packages with a versioned `manifest.json`
(`loom-core/crates/loom-package` implements manifest and ZIP layer, including
checksums and archive-bomb limits). One extension per application:
`.loomdoc`, `.loomtable`, `.loomdeck`, `.loomphoto`, `.loommotion`,
`.loomvideo`, `.loomstudio`, `.loomencode`; plugins are `.loomplugin`.
Container layout, versioning, forward compatibility, and corruption handling:
`FILE_FORMAT_FAMILY.md`; `docs/rfcs/RFC-0006-File-Package-Format.md`.

Persistence behavior per app: Writer and Sheets embed content as JSON inside
the package today; media-heavy apps (Video, Motion, Photo, Studio, Encode)
will reference external media by default with embedded assets as an option.
Autosave and recovery are specified in `docs/rfcs/RFC-0018-Autosave-and-Recovery.md`.

## 5. Rendering

- Application UI: Slint (pinned 1.17.1), a `.slint` component library in
  `loom-core`'s UI crate (`loom-ui`) consumed via `library_paths` by every
  app. The shared library and populated showcase UIs are implemented; full
  editing surfaces remain app-specific work. See
  `docs/rfcs/RFC-0003-Slint-Integration-Model.md`.
- Custom GPU rendering (canvas, timeline, grid) will use `wgpu` with Vulkan as
  the primary Linux target (`docs/rfcs/RFC-0004-GPU-Renderer.md` — not yet
  drafted; GPU work is not started).
- Deterministic rendering for tests and visual QA: Slint software renderer in
  headless mode with a custom platform, screenshots captured and compared
  against baselines committed in `loom-design-bible`
  (`docs/rfcs/RFC-0015-Visual-Regression-System.md`,
  `docs/adrs/ADR-0003-Headless-Screenshots.md`).
- Color: sRGB pipeline first in `loom-core/crates/loom-color`, ICC/BTO later
  (`docs/rfcs/RFC-0013-Color-Management.md`).
- Text: Slint text rendering plus a shaping/layout architecture decision in
  `docs/rfcs/RFC-0005-Text-Shaping-and-Layout.md`; paginated mode layout is
  NOT_STARTED.

## 6. Vision provider model

Loom Vision exposes capabilities, not models. `loom-vision-core` defines
`CapabilityId`, `ProviderDescriptor` (capability, input/output schema, media
formats, languages, memory, latency, backends, license, provenance,
determinism, batch/streaming/cancel/progress), `CapabilityProvider`
(Send + Sync), `RunContext` (cancellation and progress), and a
`ProviderRegistry`/`CapabilityRegistry` with best-provider selection. Model
packs install from files with checksum validation. Implemented reference
providers: QR decode, image statistics. See
`docs/rfcs/RFC-0010-Vision-Provider-Model.md`, `RFC-0011-Model-Pack-Format.md`,
and `../loom-vision/ARCHITECTURE.md`.

## 7. UI/engine separation (normative)

All application engines are headless libraries with CLI harnesses, and each
currently has a limited Slint showcase (Writer, Sheets, Present, Photo,
Motion, Video, Studio, and Encode). Engines own the document model,
persistence, and logic; the Slint UI consumes the current foundation through
callbacks, while a command-driven editing layer remains future work. This keeps unit,
property, fuzz, and integration tests display-free and deterministic, and
makes Docker visual QA feasible without GPUs.

## 8. Security posture

Path traversal protection and checksum verification in package and model-pack
readers; archive extraction limits (entry count, total size, compression
ratio) to prevent archive bombs; plugin installation validation and permission
checks in `loom-plugin-sdk`; no `unsafe` without justification, safety
comments, tests, and Miri where applicable. See `RELEASE_CRITERIA.md` and
`../loom-bootstrap/` quality gates.


## source-loom-spec-compatibility-policy-md

Original path: `loom-spec/COMPATIBILITY_POLICY.md`

# Loom Compatibility Policy

## 1. Scope

This policy governs versioning of shared crates and cross-repository
compatibility across the Loom suite. It is enforced by
`../loom-bootstrap/` via `COMPATIBILITY.toml` (the suite compatibility
manifest; not yet created in `loom-bootstrap`) and the bootstrap validation
scripts.

## 2. Semantic versioning for shared crates

All shared crates (`loom-core`, `loom-vision`, `loom-plugin-sdk`) follow
SemVer 2.0:

- `MAJOR` bump: breaking API or contract change.
- `MINOR` bump: backward-compatible feature addition.
- `PATCH` bump: backward-compatible fix.

Current versions: all shared crates and application workspaces are `0.1.0`
(edition 2021, MSRV 1.80). While at `0.x`, any contract change is permitted
to bump `MINOR` but must be announced in `CHANGELOG.md` and reflected in
`COMPATIBILITY.toml`; breaking changes still require an ADR/RFC note per
`AGENTS.md` §4.

Contract changes that require a version bump (not just a patch):

- manifest or package schema changes (`FILE_FORMAT_FAMILY.md` §3);
- command identifier removals/renames;
- provider trait signature changes (`RFC-0010-Vision-Provider-Model.md`);
- plugin manifest schema or permission model changes;
- behavior changes in serialization (file format compatibility).

## 3. MSRV policy

- Minimum supported Rust version: **1.80** (verified in workspace
  `rust-version` of `loom-core`, `loom-writer`, and others; enforced by CI).
- No code may require a newer toolchain without a documented MSRV bump ADR
  covering all repositories in the same release.
- CI verifies builds on the MSRV toolchain in addition to the latest stable.

## 4. Cross-repo version pinning

During development:

- Applications and consumers pin shared crates by **path dependency**
  (`loom-writer/Cargo.toml` depends on `../loom-core/crates/loom-document`
  etc.). This is the documented development mode
  (`docs/adrs/ADR-0002-Path-Based-Crate-Pinning.md`).
- `COMPATIBILITY.toml` records the pinned revision (commit) of every
  repository plus the expected crate versions, so bootstrap can validate a
  consistent workspace (see `../loom-bootstrap/`).
- Path-pinned builds are for local dev and CI only; they are never published.

For tagged releases (future, NOT_STARTED):

- `loom-core`, `loom-vision`, `loom-plugin-sdk` publish tagged crate releases
  to a registry; applications depend on version requirements with lockfiles.
- A release manifest maps app version → shared crate versions; bootstrap
  verifies the manifest and the lockfiles agree.
- The first tagged release also pins the Slint version (1.17.1) and all
  external dependencies in the lockfiles (`RFC-0001-Repository-and-Versioning-Strategy.md`).

Rules:

- Applications never depend on each other (no circular pins;
  `ARCHITECTURE.md` §1).
- A shared-crate minor release must not break any consumer on the
  documented test matrix; consumer CI runs against path pins in the
  bootstrap workspace.
- Dependency upgrades are validated across the whole suite by
  `loom-bootstrap` before being accepted.

## 5. Compatibility manifest (`COMPATIBILITY.toml`)

Owned by `loom-bootstrap` (planned, not yet created). Contents:

- `[repositories]` — repo name → pinned commit/ref;
- `[crates]` — crate name → allowed version range per repository;
- `[toolchain]` — MSRV and CI toolchain;
- `[formats]` — supported package `format_version` ranges per app;
- `[features]` — optional-backend compatibility matrix.

Bootstrap validation fails when a repository's lockfile or manifest violates
the ranges. The manifest is regenerated by the release script and included in
the release archive.

## 6. Compatibility guarantees

- A package written by version X opens in version X+1 (forward compatibility
  strategy in `FILE_FORMAT_FAMILY.md` §3), with read-only handling for
  unknown future versions.
- Import/export formats are never promised to be lossless; converters must
  emit import reports (`TERMINOLOGY.md`).
- Visual baselines are only valid for the pinned Docker images they were
  generated in (`docs/adrs/ADR-0003-Headless-Screenshots.md`).

## 7. Enforcement

- CI runs `cargo build`/`test` on the full pinned workspace
  (`../loom-bootstrap/`).
- License and dependency audits run against lockfiles at each release
  checkpoint.
- Any accepted contract change updates `COMPATIBILITY.toml`, the affected
  RFC/ADR statuses, and `FEATURE_MATRICES.md` in the same effort.


## source-loom-spec-cross-app-workflows-md

Original path: `loom-spec/CROSS_APP_WORKFLOWS.md`

# Loom Cross-Application Workflows

End-to-end workflows that span applications. Each workflow names the shared
contracts it relies on and its current status (see `FEATURE_MATRICES.md`
§12). All are local-first; none require a network. Statuses: most workflows
are NOT_STARTED because the consuming applications do not exist yet; the
contracts they depend on are specified here so implementation can proceed
independently.

## 1. Photo → Sheets: photograph-to-table extraction

**Status: NOT_STARTED** (needs Vision table detection + Sheets import path)

1. User opens a photo of a receipt, invoice, or data table in Loom Photo and
   selects "Extract table…".
2. Photo submits the image to a Loom Vision job with capability
   `table_detection`; the job is cancellable with progress
   (`loom-jobs`, `RunContext`).
3. A provider (future; model-pack based) returns table regions and cell
   structure (rows/columns/confidence). No provider installed → clear
   "no compatible provider" state; the user can still frame the table
   manually (AI is optional, `PRODUCT_SPEC.md` §2.3).
4. Photo opens the extracted grid as an interactive preview; edits apply
   before transfer (merge cells, drop columns, type fixes).
5. On confirm, Photo hands the grid to Loom Sheets through the shared
   clipboard format (below) or a direct "open in Sheets" command.
6. Sheets inserts it as a structured table. The extracted data and the
   source photo remain linked metadata; both packages are unchanged on disk
   until the user saves.

Contracts: Vision capability `table_detection` + provider interface;
Sheets import command; shared clipboard table payload; job + progress.

## 2. Video export via Loom Encode

**Status: NOT_STARTED**

1. In Loom Video, the user clicks "Send to Encode" (or Encode CLI with a
   job descriptor).
2. Video writes a `.loomencode` job package: sources (media references,
   proxies resolved to full-res), timeline render spec, output presets,
   destinations, dependencies.
3. Encode renders/transcodes as cancellable, resumable, retryable jobs with
   per-job progress; hardware acceleration when available with software
   fallback.
4. Completion surfaces back in Video via the job's completion state; output
   paths are user-visible.
5. Encode's CLI mode supports the same descriptor for scripting
   (deterministic preset files).

Contracts: `.loomencode` package schema (`FILE_FORMAT_FAMILY.md`),
loom-jobs persistence, render API in the future renderer.

## 3. Motion templates reused in Present (and Video)

**Status: NOT_STARTED**

1. A Motion composition (or part of it — keyframed title, animated badge)
   is exported as a template package (`.loommotion` with a
   `template` marker in the manifest, or the future dedicated template
   package).
2. The template declares editable properties (text, color, duration,
   keyframes exposed as parameters).
3. In Loom Present, the user inserts the template; Present hosts the
   animation via the shared animation contract and exposes the declared
   parameters in the inspector.
4. Video consumes the same template for titles/generators.
5. Templates are versioned with the package format; incompatible template
   versions are rejected with an import report.

Contracts: template marker in manifest, parameterized animation contract,
shared timeline/keyframe model (Phase 6 integration).

## 4. Studio stems into Video

**Status: NOT_STARTED**

1. In Loom Studio, the user exports stems (per-role audio: dialogue, music,
   effects, ambience) as external audio files plus a `.loomstudio` stem
   descriptor.
2. In Loom Video, "Import Studio stems" attaches the stems as audio lanes/
   roles on the timeline, time-aligned by the descriptor.
3. Round trip: Video sends audio regions to Studio for mixing or
   mastering; Studio returns a new mixdown, which Video relinks.

Contracts: stem descriptor format (part of `.loomstudio` schema), audio
role model (`TERMINOLOGY.md`), linked-asset relinking.

## 5. Shared clipboard formats

**Status: NOT_STARTED**

- One Loom clipboard format carrying typed payloads with a MIME-like type
  tag: rich text, table/grid, image with layers, selection masks, media
  references, vector shapes, animation snippets.
- Copying in any app writes the Loom payload plus standard fallback types
  (plain text, PNG) for external targets.
- Paste inside Loom negotiates the richest type the target supports; the
  target converts with an import report where fidelity is limited.
- Payloads are validated before acceptance (malformed clipboard input is a
  fuzz target per `IMPLEMENTATION_GUIDE.md`).

Contracts: clipboard payload schema (loom-clipboard crate, future),
per-app converters.

## 6. Linked assets

**Status: NOT_STARTED**

- Media-heavy packages (Photo, Motion, Video, Studio) reference assets by
  relative path + checksum in the manifest `media` section
  (`FILE_FORMAT_FAMILY.md` §7).
- Missing media shows an offline placeholder; the relink workflow searches
  user-chosen folders and matches by checksum, then updates the package.
- Linked assets shared by multiple packages are never duplicated on save;
  packages store references, keeping user-owned folders as the single copy.

## 7. Shared workspace conventions

- Commands, shortcuts, and the inspector behave identically across apps
  (Phase 6): same command palette, same selection model, same drag and drop.
- The file dialogs, recovery browser, and error reporting are shared
  components from `loom-ui`.

## Implementation order

Phase 6 of `ROADMAP.md` implements §5 and §6 first (no vision dependency),
then §1 (needs Vision), then §2–§4 as the apps exist. Each workflow needs
its own integration test and, where visual, a golden baseline.


## source-loom-spec-feature-matrices-md

Original path: `loom-spec/FEATURE_MATRICES.md`

# Loom Feature Matrices

Single source of truth for implementation status. Status words:
`COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`,
`NOT_STARTED`, `BLOCKED`. Evidence lives in the owning repository; this file
mirrors it. Last verified against the workspace: see `ROADMAP.md` for the
revision context.

A row marked `COMPLETE` means acceptance evidence exists (tests + review).
A headless engine with a CLI but no GUI is at most
`FUNCTIONAL_WITH_LIMITATIONS` for end-user capability rows.

## 1. Shared platform (`loom-core`)

| Capability | Status | Notes |
|---|---|---|
| Package manifest + ZIP container (`.loomdoc`/`.loomtable`, checksums, security limits) | COMPLETE | `loom-package`, 19 tests |
| Block-tree document model (blocks, mutations, offsets, text) | COMPLETE | `loom-document`, 6 tests |
| Paragraph/character style value objects, style runs | COMPLETE | `loom-text`, 10 tests |
| Color types, sRGB conversion | COMPLETE | `loom-color`, 8 tests; ICC NOT_STARTED |
| Job framework: progress, cancellation, priority | COMPLETE | `loom-jobs`, 5 tests; disk persistence NOT_STARTED |
| Command identifiers and enablement | COMPLETE | `loom-command`, 5 tests; palette/UI NOT_STARTED |
| In-memory undo/redo history (transactions, coalescing) | COMPLETE | `loom-history`, 7 tests; disk-backed history NOT_STARTED |
| Storage paths + transactional temp-file writes | COMPLETE | `loom-storage`, 7 tests |
| Autosave + crash recovery browser | NOT_STARTED | spec: `RFC-0018` |
| Slint component library (`loom-ui`) | FUNCTIONAL_WITH_LIMITATIONS | shared components, theme support, and smoke gallery are present; full gallery/visual harness remains |
| App runtime, renderer, animation, media, fonts, search, settings, a11y, shortcuts, clipboard, diagnostics | NOT_STARTED | listed in `ROADMAP.md` |
| Test-support helpers and screenshot capture | FUNCTIONAL_WITH_LIMITATIONS | `loom-test-support`; broader integration remains |
| Visual test harness + component gallery | NOT_STARTED | Phase 2 tail |

## 2. Loom Vision (`loom-vision`)

| Capability | Status | Notes |
|---|---|---|
| Capability traits (`CapabilityProvider`, `ProviderDescriptor`, `RunContext` cancel/progress) | COMPLETE | `loom-vision-core/provider.rs`, 12 tests |
| Provider registry with best-provider selection | COMPLETE | `registry.rs`, 12 tests |
| Model-pack manifest validation, SHA-256, path-traversal protection | COMPLETE | `model_pack.rs`, 24 tests |
| QR decode (CPU reference provider) | COMPLETE | `reference.rs`, 15 tests total |
| Image statistics (CPU reference provider) | COMPLETE | same module |
| Vision CLI (headless provider workflows) | COMPLETE | `loom-vision-cli` |
| OCR, layout-aware OCR, table detection/structure | NOT_STARTED | |
| Segmentation, matting, face/pose, depth | NOT_STARTED | provider interfaces exist |
| Tracking (object/point/planar), optical flow, stabilization | NOT_STARTED | |
| Transcription, speaker diarization, audio analysis | NOT_STARTED | |
| Embeddings, similar-image/search, indexing | NOT_STARTED | |
| ONNX/Candle backends, GPU acceleration | NOT_STARTED | |
| Application integration (Photo→Sheets, Video) | NOT_STARTED | Phase 6 |

## 3. Plugin SDK (`loom-plugin-sdk`)

| Capability | Status | Notes |
|---|---|---|
| Plugin manifest schema, validation, version compatibility | COMPLETE | `loom-plugin-manifest`, 24 tests |
| Plugin store installation: safe ZIP, checksums, path safety | COMPLETE | `loom-plugin-host`, tests |
| Runtime permission checks | FUNCTIONAL_WITH_LIMITATIONS | `loom-plugin-host`, 4 tests; policy surface partial |
| WASM execution/sandboxing | NOT_STARTED | `module.wasm` stored, not executed |
| Plugin CLI | SCAFFOLDED | `loom-plugin-cli` + fixtures |
| Signing architecture, capability negotiation, resource limits, crash isolation | NOT_STARTED | |

## 4. Loom Writer (`loom-writer`)

| Capability | Status | Notes |
|---|---|---|
| Headless rich-text document model (blocks, styles, runs) | COMPLETE | `loom-writer-core`, 6 tests |
| `.loomdoc` save/load | COMPLETE | |
| Markdown export | COMPLETE | |
| Plain-text export | COMPLETE | |
| Writer CLI (create/info/export-md/validate) | COMPLETE | used by visual-QA pipeline |
| Slint GUI (document showcase) | FUNCTIONAL_WITH_LIMITATIONS | populated quick-start showcase; editing surface is not implemented |
| Paginated mode, master pages, headers/footers, footnotes, TOC, columns | NOT_STARTED | paginated layout: `RFC-0005` |
| Tables, change tracking, comments, citations, cross-references, mail merge, form fields | NOT_STARTED | |
| PDF export | FUNCTIONAL_WITH_LIMITATIONS | deterministic foundation export is wired; full layout fidelity is not implemented |
| DOCX/ODT import-export, EPUB export | NOT_STARTED | |
| OCR-assisted import via Loom Vision | NOT_STARTED | |
| Local search, version snapshots, recovery browser | NOT_STARTED | |

## 5. Loom Sheets (`loom-sheets`)

| Capability | Status | Notes |
|---|---|---|
| Formula tokenizer + recursive-descent parser | COMPLETE | `loom-sheets-core`, 12 tests |
| A1 cell references, dependency-graph evaluation with topological order + cycle detection | COMPLETE | |
| CSV import/export | COMPLETE | |
| `.loomtable` content JSON round-trip | COMPLETE | `sheet_to_json`/`sheet_from_json` |
| Sheets CLI | COMPLETE | |
| Slint GUI (grid showcase) | FUNCTIONAL_WITH_LIMITATIONS | populated sample grid; cell editing and virtualization are not implemented |
| Incremental recalculation | NOT_STARTED | full recompute only |
| Named ranges, structured tables, sorting/filtering, validation, conditional formatting | NOT_STARTED | |
| Charts, pivot tables, grouping, freeze panes | NOT_STARTED | |
| XLSX/ODS import-export | NOT_STARTED | |
| Photograph-to-table import via Loom Vision | NOT_STARTED | needs Vision table detection |
| Goal seeking, formula auditing, error tracing | NOT_STARTED | |

## 6. Loom Present (`loom-present`)

| Capability | Status |
|---|---|
| Application foundation (model, package/CLI round-trip, Slint showcase) | FUNCTIONAL_WITH_LIMITATIONS |
| Full capabilities (canvas, themes, master slides, layouts, transitions, animations, presenter mode, PDF/video export, PPTX/ODP, vision-driven background removal) | NOT_STARTED — professional authoring is not implemented |

## 7. Loom Photo (`loom-photo`)

| Capability | Status |
|---|---|
| Application foundation (layer model, package/CLI round-trip, Slint showcase) | FUNCTIONAL_WITH_LIMITATIONS |
| Full capabilities (layer stack, masks, blend modes, adjustments, brushes, RAW, color management, PSD/OpenRaster, AI-assisted selection via Loom Vision) | NOT_STARTED — pixel compositing and professional editing are not implemented |

## 8. Loom Motion (`loom-motion`)

| Capability | Status |
|---|---|
| Application foundation (composition model, package/CLI round-trip, Slint showcase) | FUNCTIONAL_WITH_LIMITATIONS |
| Full capabilities (compositions, timeline, keyframes, parenting, tracking, optical flow, particles, render queue, template export) | NOT_STARTED — interpolation/render/playback are not implemented |

## 9. Loom Video (`loom-video`)

| Capability | Status |
|---|---|
| Application foundation (track/clip model, package/CLI round-trip, Slint showcase) | FUNCTIONAL_WITH_LIMITATIONS |
| Full capabilities (media library, timeline editing, proxies, multicam, effects, color, captions, transcription, Encode export) | NOT_STARTED — media decode/playback/export are not implemented |

## 10. Loom Studio (`loom-studio`)

| Capability | Status |
|---|---|
| Application foundation (track/region model, package/CLI round-trip, Slint showcase) | FUNCTIONAL_WITH_LIMITATIONS |
| Full capabilities (Quick + Pro workspaces, audio/MIDI, mixer, plugin hosting, score editor, source separation, export) | NOT_STARTED — audio engine/export are not implemented |

## 11. Loom Encode (`loom-encode`)

| Capability | Status |
|---|---|
| Application foundation (queue/preset model, package/CLI round-trip, Slint showcase) | FUNCTIONAL_WITH_LIMITATIONS |
| Full capabilities (batch queue, presets, filters, hardware/software encoding, watch folders, CLI, quality metrics) | NOT_STARTED — encoder invocation and batch execution are not implemented |

## 12. Cross-application (`CROSS_APP_WORKFLOWS.md`)

| Capability | Status |
|---|---|
| Shared clipboard formats, drag and drop, linked assets, Motion templates → Video/Present, Encode integration, Studio stems → Video, shared commands/shortcuts/components | NOT_STARTED (Phase 6) |

## 13. Sample content (`loom-samples`)

| Capability | Status |
|---|---|
| Original sample projects for all applications | FUNCTIONAL_WITH_LIMITATIONS | eight sample packages are present; sample generation is not implemented |


## source-loom-spec-file-format-family-md

Original path: `loom-spec/FILE_FORMAT_FAMILY.md`

# Loom File Format Family

This document is the authoritative specification for Loom's document package
formats. The implemented schema lives in `loom-core/crates/loom-package`
(`manifest.rs`, `zip.rs`); when the crate and this document disagree, the
crate is the implementation and this document must be corrected in the same
effort.

## 1. Package container

Every Loom document is a ZIP archive with one extension per application:

| Application | Extension | PackageKind |
|---|---|---|
| Writer | `.loomdoc` | `document` |
| Sheets | `.loomtable` | `table` |
| Present | `.loomdeck` | `deck` |
| Photo | `.loomphoto` | `photo` |
| Motion | `.loommotion` | `motion` |
| Video | `.loomvideo` | `video` |
| Studio | `.loomstudio` | `studio` |
| Encode | `.loomencode` | `encode` |

A package contains:

```text
manifest.json       versioned manifest (required, first entry)
content/            document content in application-defined form
assets/             embedded media (optional; see §7)
previews/           preview images (optional)
metadata/           user metadata, keywords, collections (optional)
history/            undo/redo journal snapshots (optional)
recovery/           autosave and crash-recovery snapshots (optional)
```

`manifest.json` carries at minimum:

- `format_version` — schema version of the package (see §3);
- `id` — stable identifier (UUID) and `revision` — monotonic edit revision;
- `created` / `modified` timestamps (ISO 8601, UTC);
- `app` — creating application and version (informational);
- `checksums` — SHA-256 per entry, plus a package-level digest;
- `entries` — table of entry paths, sizes, kinds, and MIME types.

PackageKind, schema versioning, checksums, and entry types are implemented in
`loom-package`; the exact JSON keys follow the crate's `Manifest` type.

## 2. Integrity and corruption handling

- Every entry is checksummed (SHA-256); a mismatch marks the package
  corrupted.
- A partially damaged package can still open if `manifest.json` parses and the
  `content/` entries verify; missing `assets/` degrades gracefully with a
  relink prompt (media-heavy apps) or embedded fallback.
- Readers must validate checksums before use and report precisely which
  entries failed.
- Packages failing manifest validation are rejected with a structured error;
  the application must never write over a corrupt package without user
  confirmation.

## 3. Schema versioning and migration

- `format_version` is an integer, currently `1`. Bump only on breaking
  schema change.
- Version `1` packages open on all future versions with a forward-compatible
  strategy: readers ignore unknown optional entries and unknown manifest
  fields, and warn.
- Migration: a version `N` reader migrates `format_version < N` packages in
  memory and saves the current version; the original file is only replaced
  after a successful save. Unsupported future versions are opened read-only
  with an explanatory error.
- Every schema change requires a migration test, a fixture, and an RFC/ADR
  note (see `RFC-0006-File-Package-Format.md`).

## 4. Security limits (archive-bomb protection)

ZIP readers must enforce, before extraction:

- maximum entry count (e.g. 10,000);
- maximum uncompressed total size (e.g. 4 GiB default, configurable);
- maximum per-entry size (e.g. 1 GiB);
- maximum compression ratio (e.g. 1000:1) to defeat zip bombs;
- path traversal rejection: every entry path must resolve inside the package
  root (no `..`, no absolute paths, no symlink escapes);
- duplicate entry rejection.

Violations fail package open with a security error, never partial extraction.
These limits are enforced in `loom-package`'s ZIP reader and covered by
fuzz targets (see `IMPLEMENTATION_GUIDE.md`).

## 5. Content serialization

Each application defines its content inside `content/`:

- **Writer** — block/paragraph model with text runs and styles, serialized as
  JSON (implemented: `loom-writer-core` rich-text blocks; Markdown and
  plain-text export in addition to `.loomdoc` save/load).
- **Sheets** — workbook JSON: sheets, cells, values, formulas, styles
  (implemented: `sheet_to_json`/`sheet_from_json` round-trip in
  `loom-sheets-core`; CSV import/export).
- **Present/Photo/Motion/Video/Studio/Encode** — application-defined JSON
  models, to be specified by each application's `FILE_FORMAT.md` when their
  vertical slices begin.

Deterministic serialization: stable key order, no timestamps inside content
JSON, fixed float formatting — required for reproducible archives and
golden-file tests.

## 6. History and recovery entries

- `history/` holds optional undo/redo journal snapshots for large projects;
  disk-backed history is NOT_STARTED (`RFC-0007-Undo-and-Transaction-System.md`).
- `recovery/` holds autosave snapshots and operation journals written
  transactionally (temp file + atomic rename); the recovery browser is
  NOT_STARTED (`RFC-0018-Autosave-and-Recovery.md`).
- Recovery entries must never be modified in place; readers reconcile
  newest-valid-wins.

## 7. Embedded vs external media policy

Per application, stated as default policy:

| Application | Default | Notes |
|---|---|---|
| Writer | embed | images and fonts embedded; large assets may be external |
| Sheets | embed | no large media expected |
| Present | embed | media embedded; video may be external |
| Photo | external by default | embedded optional; RAW sidecars external |
| Motion | external | media referenced, relink supported |
| Video | external | proxy and full-res referenced; relink required |
| Studio | external | audio files referenced |
| Encode | n/a | job descriptors reference sources; outputs external |

External references are stored as relative paths with a `media` manifest
section; missing media degrades to offline placeholders with a relink
workflow (NOT_STARTED). Linked-asset workflows are specified in
`CROSS_APP_WORKFLOWS.md`.

## 8. Testing requirements

- Round-trip property tests (save → load → save equality);
- schema fixtures and a golden compatibility corpus;
- fuzz targets for the package reader and manifest parser;
- corruption tests (truncation, bit flips, missing entries, bad checksums,
  zip bombs, path traversal);
- migration tests for every schema version bump.


## source-loom-spec-implementation-guide-md

Original path: `loom-spec/IMPLEMENTATION_GUIDE.md`

# Loom Implementation Guide

How a new capability moves from idea to verified implementation across the
Loom suite. This document is for specification writers and coding agents;
quality gates are executed by `../loom-bootstrap/`.

## 1. Workflow overview

```text
Specify → Task → Implement → Test → Verify → Mark complete (with evidence)
```

Unimplemented work stays visible in the task ledger; nothing is "complete"
without acceptance evidence (`AGENTS.md` §2, `RELEASE_CRITERIA.md`).

## 2. Specify

- Decide whether the change is **product scope** (this repo, `PRODUCT_SPEC.md`
  + `FEATURE_MATRICES.md`), **design** (`loom-design-bible`), **platform
  contract** (`loom-core` docs + crate), or **application contract**
  (application repo).
- Cross-cutting changes require an RFC here (`docs/rfcs/`); small decisions
  use an ADR (`docs/adrs/`). Accepted contracts are normative — never change
  them silently (`AGENTS.md` §4).
- A feature specification must define, per root `AGENTS.md` §11.1: purpose,
  user story, non-goals, preconditions, inputs, outputs, state model, data
  structures, interfaces, error behavior, threading model, persistence,
  undo, accessibility, security, performance budget, unit/integration/visual
  tests, failure cases, acceptance criteria, dependencies, files expected to
  change, example usage.

## 3. Task

- Decompose into tasks small enough for one coding agent to complete and
  verify independently. Concrete tasks (examples from the suite):
  "Implement immutable paragraph-style value object", "Implement UTF-16
  cursor mapping tests for bidirectional text", "Implement cancel-safe
  thumbnail job", "Implement dependency-graph cycle reporting for formulas".
- Task format (root `AGENTS.md` §11.2): ID, Title, Owner subsystem, Purpose,
  Dependencies, Files or modules, Required behavior, Non-goals,
  Implementation steps, Acceptance tests, Visual QA, Performance budget,
  Security considerations, Completion evidence.
- Assign one owner subsystem per task; two agents must not edit the same
  contract simultaneously.

## 4. Implement

- Follow the owning repository's conventions: rustfmt, clippy, no `unsafe`
  without safety comments + tests + justification (`RELEASE_CRITERIA.md`
  §1.11), engines headless and display-free (`ARCHITECTURE.md` §7).
- Engines consume shared contracts (`loom-core`, `loom-vision`,
  `loom-plugin-sdk`); they never redefine them. Applications never import
  each other.
- New shared behavior goes into the owning shared crate with its own tests;
  do not copy code between repositories (`COMPATIBILITY_POLICY.md`).

## 5. Test

Tests required per `ROADMAP.md` Phase 7 hardening and the root directive §14:

- **Unit tests** — parsers, serializers, formula evaluation, text layout,
  time/coordinate math, undo operations, migration, model-pack validation,
  color conversion, plugin permissions, command state, caches, job
  cancellation.
- **Property tests** — serialization round trips, undo/redo invariants,
  formulas, timeline edits, transform composition, package migration,
  Unicode cursor movement, range operations, media timestamp conversion.
- **Fuzzing** — deterministic mutation fuzz targets integrated into normal
  tests (no cargo-fuzz on stable; see
  `docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`): package readers,
  importers, media metadata and subtitle parsers, formula/rich-text/manifest
  parsers, clipboard input, recovery journals.
- **Integration tests** — create/edit/save/close/reopen; crash during save +
  recover; import/export; undo/redo; copy/paste; drag and drop; plugin and
  model-pack install/removal; offline startup/edit/export; low disk; missing
  media/fonts; corrupt files; cancelled background work.
- **End-to-end UI tests** — menus, shortcuts, inspector, canvas/timeline/
  spreadsheet navigation, accessibility focus, dialogs, export, recovery,
  themes, reduced motion, localization layouts.
- **Visual regression** — software-renderer screenshots vs golden baselines
  from pinned Docker images
  (`docs/rfcs/RFC-0015-Visual-Regression-System.md`,
  `docs/adrs/ADR-0003-Headless-Screenshots.md`). No auto-approval of new
  baselines.
- **Performance tests** — startup, window creation, open, large-document
  scroll, recalculation, scrubbing, waveform/thumbnail generation, model
  inference, export, save/autosave, undo, indexing, memory/GPU memory,
  background-task interference. Budgets per app supersede broad targets
  (`RELEASE_CRITERIA.md` §3).

## 6. Verify

Quality gates executed by `loom-bootstrap` (targeted matrix when optional
backends conflict; documented in the repo):

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
documentation link checks
schema validation
visual regression tests
offline integration tests (network-disabled container)
license audit
dependency audit
package smoke tests
```

A task is complete only when its acceptance tests pass, its visual QA is
reviewed, and `FEATURE_MATRICES.md`/`ROADMAP.md` reflect the new state with
evidence linked. Update `CHANGELOG.md` and `COMPATIBILITY.toml` when
contracts change.

## 7. Ownership map (current)

- Shared platform: `loom-core` maintainers.
- Vision: `loom-vision` maintainers (runtime/providers, OCR, segmentation,
  tracking, audio, model packs).
- Apps: one lead per application repository.
- Quality: `loom-bootstrap` (build/CI, test infra, fuzzing, packaging,
  licensing, doc consistency).
- Contracts: this repository reviews cross-repository contract changes;
  architecture drift is resolved by the coordinator per root `AGENTS.md` §12.


## source-loom-spec-product-spec-md

Original path: `loom-spec/PRODUCT_SPEC.md`

# Loom Product Specification

## 1. Product mission

Loom is an original, open-source, professional creative suite for desktop
computers: word processing, spreadsheets, presentations, photo editing, motion
graphics, video editing, audio production, and media transcoding — one calm,
cohesive product family.

Loom combines a minimal and calm interface, professional depth, excellent
typography, direct manipulation, smooth meaningful animation, high-performance
native execution, local-first storage, offline-first operation, local computer
vision and machine learning, strong accessibility, documented open formats,
and extensible sandboxed plugins.

Loom must be an **original** product. No proprietary source, icons, layouts,
templates, sounds, sample media, or branding from other creative suites may be
copied. Loom studies interaction principles and professional workflows, then
implements an independent design and visual identity (see
`../loom-design-bible/`).

## 2. Non-negotiable product principles

These bind every application and every release. A violation is release-blocking.

### 2.1 Local first

Every core workflow must function without an internet connection: create,
open, edit, save, export, search local files, run supported computer-vision
features with installed local models, recover unsaved work, render projects,
transcode media, install local plugin packages, and read bundled help.
No mandatory account, no required cloud service. Cloud synchronization is out
of scope entirely; the architecture must not depend on it.

### 2.2 Privacy

No telemetry by default, no advertising, no hidden network requests, no user
profiling, no remote crash upload, no remote model inference, no automatic
upload of documents or media, no mandatory update checks. Local diagnostic
logs must be understandable, redactable, and under user control. Core
workflows must be verified in a network-disabled container (see
`../loom-bootstrap/`).

### 2.3 AI is optional

All conventional editing features remain fully usable without any AI model.
AI/computer vision enhances tools, never replaces them: manual masks without
segmentation, manual subtitles without transcription, conventional document
editing without a language model, formulas without an AI model, manual
keyframing without tracking.

### 2.4 User ownership

Documented, versioned file formats (see `FILE_FORMAT_FAMILY.md`). Users can
keep files permanently, back up with ordinary filesystem tools, inspect
package contents, export to common formats, move projects between computers,
and recover data from partially damaged packages where practical.

### 2.5 Performance

Responsive under professional workloads; long work is asynchronous,
cancellable, observable, and recoverable. Never block the UI thread with media
decoding, file parsing, model inference, autosave, export, thumbnail
generation, waveform generation, proxy generation, font scanning, plugin
discovery, or search indexing. Budgets and tiers: see
`../loom-design-bible/PERFORMANCE.md` and `RELEASE_CRITERIA.md`.

### 2.6 Accessibility

Accessibility is a release requirement: complete keyboard navigation, visible
focus, screen-reader labels, logical focus order, high-contrast operation,
scalable UI, reduced-motion mode, non-color status indicators, configurable
shortcuts, accessible error reporting, and accessible canvas/timeline
navigation where technically possible. Design authority:
`../loom-design-bible/ACCESSIBILITY.md`.

## 3. The applications

All applications are Rust + Slint desktop apps in separate repositories.
Status of every capability is in `FEATURE_MATRICES.md`; the phase plan is in
`ROADMAP.md`. As of this revision, all eight application repositories have
tested model/CLI/package slices and limited Slint showcase UIs. None is yet a
full professional editor; the remaining capability gaps are recorded in the
matrices and the Slint integration RFC.

### 3.1 Loom Writer (`../loom-writer/`)

Professional word processor and page-layout application: rich text editing,
continuous and paginated modes, paragraph/character styles, page styles,
master pages, sections, columns, tables, lists, footnotes/endnotes, headers
and footers, automatic tables of contents, citations, cross-references,
change tracking, comments, structured navigation, shapes and media, text
wrapping, anchored and floating objects, templates, form fields, mail merge,
print layout, PDF/EPUB export, DOCX/ODT/Markdown/plain-text import and export,
OCR-assisted scanned-document import, local document search, crash recovery
and version snapshots. Implemented now: headless document model, `.loomdoc`
save/load, Markdown and plain-text export, deterministic PDF export, CLI, and
a Slint quick-start showcase; full document editing remains unimplemented.

### 3.2 Loom Sheets (`../loom-sheets/`)

Professional spreadsheet and data-analysis application: large virtualized
grid, formula engine, dependency graph, incremental recalculation, named
ranges, structured tables, sorting/filtering, conditional formatting,
validation, charts, pivot tables, grouping, freeze panes, multiple sheets,
comments, rich cell formatting, dates/times/durations/currencies/units,
CSV/TSV import-export, XLSX/ODS where feasible, local data connectors,
formula auditing, error tracing, goal seeking, statistics/financial
functions, photograph-to-table import through Loom Vision, receipt/invoice
extraction. No arbitrary network queries from cells. Implemented now: formula
engine (tokenizer, parser, dependency graph with cycle detection), CSV
import/export, `.loomtable` JSON round-trip, CLI, and a Slint grid showcase;
cell editing remains unimplemented.

### 3.3 Loom Present (`../loom-present/`)

Presentation authoring: slide canvas, themes, master slides, layouts, guides,
alignment/distribution, smart grouping, text/tables/charts/shapes/images/
audio/video/equations, speaker notes, transitions, object animations,
timeline-based animation editing, presenter display, rehearsal timing,
presentation recording, PDF/video export, PPTX/ODP where feasible, local
presenter background removal and tracking where supported. The current
foundation slice has model/CLI/package round-trip and a Slint showcase;
professional authoring is not started.

### 3.4 Loom Photo (`../loom-photo/`)

Nondestructive image editing: layer-based editing, pixel/vector/text/
adjustment/fill layers, groups, masks, clipping, blend modes, nondestructive
filters, RAW development, color management, ICC profiles, curves, levels,
white balance, exposure, selective color, gradients, brushes, clone/heal,
content-aware repair, crop/perspective, transform/warp, liquify, panorama,
HDR, batch processing, PSD where practical, OpenRaster, TIFF/PNG/JPEG/WebP/
AVIF/EXR, semantic subject selection, background removal, portrait matting,
object-aware masks, local inpainting/super-resolution where compatible local
models are installed. All AI-assisted selections must remain editable as
ordinary masks. The current foundation slice has a layer model, package/CLI
round-trip, and a Slint showcase; pixel compositing is not started.

### 3.5 Loom Motion (`../loom-motion/`)

Motion graphics, animation, compositing: layer-based compositions, timeline,
keyframes, curve/graph editor, parenting, constraints, masks, vector shapes,
text animation, image sequences, video layers, audio reference tracks,
cameras, lights where supported, 2.5D scenes, transform hierarchy, blend
modes, filters/effects, particles, replicators, behaviors, motion paths,
chroma key, rotoscoping, planar/point/object tracking through Loom Vision,
stabilization, optical-flow retiming, motion blur, render queue, template
export for Loom Video and Loom Present. The current foundation slice has a
composition model, package/CLI round-trip, and a Slint showcase; rendering
and playback are not started.

### 3.6 Loom Video (`../loom-video/`)

Nonlinear video editor: media library, events/projects, metadata, keyword
collections, favorites/rejects, proxies, background transcoding, timeline
editing, connected-clip editing model, track compatibility where required,
ripple/roll/slip/slide/blade/trim/overwrite, compound clips, nested timelines,
multicam, synchronized clips, audio lanes and roles, transitions, titles,
generators, effects, keyframes, speed changes, optical-flow retiming,
stabilization, color correction, scopes, LUTs, HDR-aware processing, captions,
local transcription, scene detection, subject tracking, automatic reframing,
background removal where feasible, export through Loom Encode, XML/EDL/AAF
interchange where feasible and legally appropriate, autosave, project
backups, media relinking, offline media workflows. The current foundation
slice has a track/clip model, package/CLI round-trip, and a Slint showcase;
media processing is not started.

### 3.7 Loom Studio (`../loom-studio/`)

Digital audio workstation with two progressive-disclosure workspaces over one
engine.

*Quick Workspace*: loop browser, simplified tracks, software instruments,
audio and MIDI recording, procedural rhythm tools with original
implementations and assets, smart controls, chord/scale assistance, basic
effects, easy arrangement, guided mixing, simple export.

*Pro Workspace*: multitrack audio, MIDI, piano roll, drum editor, score editor
where feasible, automation, comping, take folders, nondestructive time
editing (original implementation), pitch editing, mixer, buses, sends,
sidechains, plugin hosting, instrument hosting, sample editing, looping,
markers, tempo maps, meter changes, surround architecture where feasible,
mastering tools, loudness metering, local source separation/speech-noise
enhancement where compatible local models are installed, beat/tempo/key/
transient analysis. Linux plugin standards supported with sandboxing where
practical. The current foundation slice has a track/region model, package/CLI
round-trip, and a Slint showcase; audio processing is not started.

### 3.8 Loom Encode (`../loom-encode/`)

Media transcoding and delivery: batch queue, source inspection, presets,
custom encoding settings, video filters, audio channel mapping, subtitle
handling, frame-rate conversion, scaling, cropping, color-space conversion,
HDR metadata handling, image sequences, audio-only output, multi-destination
jobs, job dependencies, retry/recovery, pause/resume, hardware acceleration
when available with software fallback, optional watch folders, CLI operation,
integration with Video/Motion/Present/Photo/Studio, deterministic preset
files, local content-aware analysis, perceptual quality metrics. The current
foundation slice has a queue/preset model, package/CLI round-trip, and a Slint
showcase; encoder execution is not started.

## 4. Loom Vision

A shared, local-first computer-vision and perception platform
(`../loom-vision/`), not hard-wired to any model, vendor, runtime, or
hardware backend. Capability areas: document and text (OCR, layout-aware OCR,
handwriting provider interface, boundary detection, perspective correction,
dewarping, table detection/structure, form-field detection, barcode/QR,
math-expression provider interface, PDF page analysis, reading order,
language detection, local full-text indexing); images (classification,
object detection, segmentation, salient-object/portrait segmentation, alpha
matting, face detection/landmarks/quality, body/hand pose, depth provider
interface, embeddings, similar/duplicate search, scene classification,
captioning/super-resolution/denoising/inpainting provider interfaces); video
(object/point/planar tracking, optical flow, shot boundaries, scene grouping,
subject tracking, camera motion, stabilization analysis, reframing, subtitle
timing, speech transcription, speaker diarization provider interface,
thumbnails, embeddings, semantic search); audio (speech recognition, VAD,
noise classification, beat/tempo/key/transient detection, source-separation/
speech-enhancement provider interfaces, embeddings).

Providers are selected through capability traits (`CapabilityProvider`,
`ProviderDescriptor`, `RunContext` — see `../loom-vision/ARCHITECTURE.md`),
never model-specific APIs. CPU reference providers exist today (QR decode,
image statistics); OCR, segmentation, tracking, transcription, and all model
packs are not yet implemented. Model packs are installed from files, verified
by checksum, never downloaded automatically.

## 5. Cross-application workflows

Specified end-to-end in `CROSS_APP_WORKFLOWS.md`; implemented state there.

- **Photo → Sheets table import**: Loom Vision detects a table region in a
  photo, extracts cell structure, and imports rows into a Sheets workbook.
  Requires Vision table detection (NOT_STARTED).
- **Motion templates → Video and Present**: Motion exports composition
  templates consumed by Video titles/generators and Present animations.
  Template format is part of the file-format family (NOT_STARTED).
- **Encode integration**: Video, Motion, Present, Photo, and Studio hand
  export jobs to Loom Encode through shared job and package contracts
  (NOT_STARTED).
- **Studio → Video**: Studio exports stems (per-role audio) into Video
  projects; Video exports audio to Studio for mixing (NOT_STARTED).
- **Shared clipboard**: cross-application copy/paste via a Loom clipboard
  format carrying typed payloads (content, selection masks, assets)
  (NOT_STARTED).
- **Linked assets**: shared external media referenced by multiple packages
  with relinking (NOT_STARTED).

## 6. Out of scope

- Cloud accounts, cloud sync, remote collaboration, analytics, remote model
  APIs, mandatory update checks, cloud-backed storage (now or "for later").
- A future cloud capability may exist only as an optional external plugin
  after the local platform is complete.
- Network-driven formula queries in Sheets cells.
- Proprietary formats, assets, and behavior copied from other products
  (see `../loom-design-bible/` anti-patterns).


## source-loom-spec-release-criteria-md

Original path: `loom-spec/RELEASE_CRITERIA.md`

# Loom Release Criteria

A release-blocking condition means the suite (or the affected application)
must not ship until the condition is resolved or a reviewed, documented
waiver exists. Waivers are recorded in this repository as ADRs.

## 1. Release-blocking conditions

1. **Compilation failure** — any supported build target fails under the
   documented toolchain (MSRV 1.80, pinned deps). Includes `cargo build`,
   `cargo test --workspace --all-features` (or the documented test matrix
   when optional backends are incompatible).
2. **Fake or empty tests** — tests that only assert `true`, assert nothing,
   or never exercise the code path they claim to cover. Every acceptance
   criterion must be tied to an executable assertion.
3. **Network dependency in a core workflow** — any core user workflow
   (create, edit, save, export, search, recover, install local plugins/model
   packs, help) fails or degrades without a network. Verified in the
   network-disabled Docker container (`../loom-bootstrap/`).
4. **UI-thread blocking** — synchronous media decoding, file parsing, model
   inference, autosave, export, thumbnail/waveform/proxy generation, font
   scanning, plugin discovery, or search indexing on the UI thread in a known
   critical path.
5. **Unrecoverable save corruption** — a save path that can lose user work
   without recovery (failed atomic write handling, missing checksums, corrupt
   journal handling). Autosave + crash recovery must function per
   `RFC-0018-Autosave-and-Recovery.md`.
6. **Missing license information** — any dependency, asset, font, or model
   whose license is unknown, incompatible, or undocumented. The license and
   dependency audit must pass (`../loom-bootstrap/`, `LICENSE_REPORT.md`).
7. **Severe accessibility regression** — loss of keyboard navigation, visible
   focus, screen-reader labels, high-contrast operation, reduced-motion
   support, or scalable UI in a released surface
   (`../loom-design-bible/ACCESSIBILITY.md`).
8. **Severe visual regression** — golden-baseline diffs beyond documented
   tolerance on released surfaces with no waiver
   (`RFC-0015-Visual-Regression-System.md`).
9. **Claimed-but-unimplemented features** — any feature marked `COMPLETE`
   (or presented as complete in UI or docs) without acceptance evidence.
   Statuses must match `FEATURE_MATRICES.md`; empty screens, hard-coded
   demonstrations, and placeholder exporters count as claims.
10. **Unbounded memory behavior** — unbounded growth in representative
    workloads (large documents, long sessions, background queues) with no
    documented cache policy.
11. **Undocumented unsafe code** — any `unsafe` block without a safety
    comment, tests, encapsulation, and a module-level justification
    (Miri-checked where applicable).
12. **Secrets or credentials in artifacts** — anything that ships in a
    release archive must be free of secrets and absolute personal paths.

## 2. Required gates before release

Run, and record results in `VERIFICATION_REPORT.txt` (generated by
`../loom-bootstrap/`):

- `cargo fmt --check`, `cargo clippy --all-targets --all-features -D warnings`
  (targeted matrix documented when `--all-features` conflicts with optional
  system backends);
- `cargo test --workspace` with documented feature matrix;
- property tests, fuzz runs (deterministic mutation corpus — see
  `docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`), integration tests;
- offline integration tests in the network-disabled container;
- visual regression with baselines from pinned Docker images
  (`docs/adrs/ADR-0003-Headless-Screenshots.md`);
- schema validation and package-corruption tests
  (`FILE_FORMAT_FAMILY.md` §8);
- license audit, dependency audit (lockfiles, `SBOM`), package smoke tests.

## 3. Performance gates

Broad targets (see `../loom-design-bible/PERFORMANCE.md`): input feedback
within one display frame; 60 FPS UI animations; warm launch under 1 s
(lightweight apps), cold under 2 s where feasible; no per-frame allocation
while scrolling; immediate cancellation feedback; autosave must not visibly
interrupt editing; bounded memory per documented cache policy. Applications
replace these with workload-specific budgets; regressions beyond configured
thresholds fail CI or require a reviewed waiver.

## 4. Statuses and honesty

- `COMPLETE` requires acceptance evidence: tests, visual QA, and a
  `FEATURE_MATRICES.md` entry.
- `FUNCTIONAL_WITH_LIMITATIONS` is the correct status for a headless engine
  with a CLI but no GUI, or a feature with documented gaps.
- Unimplemented work must remain visible in the task ledger — it is never
  silently dropped from a release plan.

## 5. Waiver process

A waiver is a written ADR citing the condition, the affected surface, the
mitigation, and the removal milestone. Waivers are reviewed at the next
release checkpoint and must not accumulate across more than one minor
release without re-review.


## source-loom-spec-roadmap-md

Original path: `loom-spec/ROADMAP.md`

# Loom Roadmap

Phase plan with honest current status. Statuses are maintained in
`FEATURE_MATRICES.md`; prose here must not contradict the matrices.

## Phase 0 — Workspace bootstrap

**Status: DONE** (with known gaps)

- Parent `loom/` workspace with all 15 repositories created; shared licensing
  (MIT OR Apache-2.0), root `AGENTS.md`, task ledger, and ownership map are in
  place. The parent tracks the sibling repositories; they do not currently
  have independent `.git` histories.
- `loom-bootstrap` now has README/BOOTSTRAP guidance, scripts, Docker files,
  `COMPATIBILITY.toml`, and CI workflow scaffolding. `loom-design-bible`
  contains the visual, motion, interaction, accessibility, and baseline
  specifications; the remaining gap is application baseline coverage.

## Phase 1 — Foundation specifications

**Status: IN PROGRESS**

- This repository (`loom-spec`) documents product scope, architecture,
  terminology, file formats, release criteria, compatibility policy,
  roadmap, feature matrices, implementation guide, cross-app workflows,
  and 12 accepted RFCs + 7 ADRs.
- Remaining: RFC-0004 (GPU renderer), RFC-0009 (plugin ABI and sandboxing),
  RFC-0012 (media framework), RFC-0014 (accessibility), RFC-0016 (cross-repo
  compatibility), RFC-0017 (application command system), RFC-0019 (local
  search), RFC-0020 (localization) — PROPOSED, not yet drafted.
- `loom-design-bible` visual/motion/interaction/accessibility documents are
  written; six application light/dark baseline pairs are still missing.
- The first cross-repository contracts frozen so far: package container and
  manifest (`loom-package`), command/history/jobs/storage/text/color crate
  boundaries, vision provider model, plugin manifest schema.

## Phase 2 — Shared platform

**Status: PARTIAL (core crates, a shared UI crate, and app showcase slices exist)**

Implemented in `loom-core/crates/` (all `0.1.0`, MSRV 1.80):

| Crate | Purpose | Tests |
|---|---|---|
| `loom-package` | manifest schema + ZIP container, checksums, security limits | 19 |
| `loom-document` | block tree document model (Block, BlockTree, Mutation, Offset, Text, TextEdit) | 6 |
| `loom-text` | paragraph/character styles, style runs | 10 |
| `loom-color` | color types and conversion | 8 |
| `loom-jobs` | async job framework: progress, cancellation, priority | 5 |
| `loom-command` | command identifiers, enablement | 5 |
| `loom-history` | transactional undo/redo history | 7 |
| `loom-storage` | storage paths and transactional file operations | 7 |

Not yet implemented: `loom-app-runtime`, `loom-render`,
`loom-animation`, `loom-autosave`, `loom-recovery`, `loom-media`,
`loom-fonts`, `loom-search`, `loom-settings`, `loom-accessibility`,
`loom-shortcuts`, `loom-clipboard`, `loom-diagnostics`, `loom-test-support`,
plugin host integration with core, richer component gallery, visual test
harness, and the full editing/runtime command layer. The `loom-ui` Slint
component crate and shared app styling are present and used by the eight
application slices.

## Phase 3 — Loom Vision foundation

**Status: DONE** (vertical slice to Loom Vision's own spec)

- `loom-vision-core`: capability traits (`CapabilityProvider`,
  `ProviderDescriptor`, `RunContext` with cancellation/progress),
  `ProviderRegistry`/`CapabilityRegistry` with best-provider selection,
  model-pack manifest validation with SHA-256 checksums and path-traversal
  protection (24 tests), image interchange (`LumaImage`, `ProviderInput`,
  `ProviderOutput`, `BBox`).
- CPU reference providers implemented and tested: **QR decode**
  (`QrCodeProvider`) and **image statistics** (`ImageStatsProvider`).
- `loom-vision-cli` headless commands for provider workflows.
- Not started: OCR, segmentation, tracking, transcription, ONNX/Candle
  backends, hardware acceleration, semantic search, benchmark harness.
- No application consumes Loom Vision yet (Phase 4+).

## Phase 4 — Application vertical slices

**Status: IN PROGRESS** — all eight application repositories now contain
tested model/CLI/package slices and limited Slint showcase UIs. Full editing,
rendering, media, and export capabilities remain incomplete per the matrices.

- **Loom Writer** (`loom-writer-core` + CLI, 6 tests): rich-text block
  document model, `.loomdoc` save/load, Markdown and plain-text export,
  `create`/`info`/`export-md`/`validate` CLI commands and a Slint quick-start
  showcase; the UI is not yet a full text editor.
- **Loom Sheets** (`loom-sheets-core` + CLI, 12 tests): formula engine
  (tokenizer, recursive-descent parser, A1 cell references, dependency-graph
  evaluator with topological ordering and cycle detection), CSV import/export,
  `.loomtable` JSON round-trip and a Slint grid showcase; cells are not yet
  editable in the UI.
- **Present, Photo, Motion, Video, Studio, Encode**: model/CLI/package
  round-trips plus populated Slint showcase UIs now exist. Their professional
  editing engines and media pipelines remain planned work.

## Phase 5 — Professional feature expansion

**Status: NOT_STARTED.** Expand per `FEATURE_MATRICES.md` priority order;
do not sacrifice architectural integrity for checkbox counts.

## Phase 6 — Integration

**Status: NOT_STARTED.** Shared clipboard formats, cross-app drag and drop,
linked assets, Motion templates into Video/Present, Encode integration,
Vision integration, shared commands/shortcuts/components, consistent file
dialogs and recovery (see `CROSS_APP_WORKFLOWS.md`).

## Phase 7 — Hardening

**Status: PARTIAL.** The suite has deterministic unit tests, dependency and
license reports, host build/test/lint gates, smoke tests, package extraction
tests, and a visual regression harness. Remaining hardening includes mutation
fuzzing (`docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`), offline
coverage, corruption/crash-recovery tests, memory/performance tests,
accessibility audits, and the missing application visual baselines.

## Phase 8 — Release packaging

**Status: PARTIAL.** Host release builds, checksums, sample projects,
documentation, a deterministic `Loom-Complete.zip`, and extracted-package
verification exist. Linux release binaries, AppImage/Flatpak manifests,
SBOM/build provenance, and a final visual-baseline-complete release remain.

## Current milestone focus

1. Finish Phase 1 specs (remaining RFCs and design-bible baselines).
2. Harden the current eight app vertical slices: real editing interactions,
   persistence UX, and application-level regression coverage.
3. Complete the required light/dark visual baselines for all eight apps.
4. Wire Loom Vision into Photo/Sheets workflows and then proceed with the
   professional capabilities app by app.


## source-loom-spec-terminology-md

Original path: `loom-spec/TERMINOLOGY.md`

# Loom Terminology

The shared glossary. Every Loom document must use these terms as defined here.
Statuses in parentheses are implementation states; see `FEATURE_MATRICES.md`.

## Platform concepts

- **Command** — a named, undoable user action with a stable identifier; the
  single path for menus, shortcuts, palette, accessibility, and plugins
  (`loom-core/crates/loom-command`).
- **Job** — a cancellable unit of async work with progress, priority, and
  optional dependencies; never runs on the UI thread
  (`loom-core/crates/loom-jobs`).
- **Document** — the in-memory model of user content for an application
  (Writer document, workbook, deck, composition, …); headless and testable.
- **Package** — the on-disk ZIP container format for a document
  (`.loomdoc`, `.loomtable`, …); implemented in `loom-core/crates/loom-package`.
- **Manifest** — the `manifest.json` inside a package: format version, id,
  timestamps, checksums, entry table.
- **Schema version** — the `format_version` of a package; governs migration
  rules. See `FILE_FORMAT_FAMILY.md`.
- **Checksum** — SHA-256 digest of an entry, recorded in the manifest for
  corruption detection.
- **Autosave** — periodic transactional save of the current document state.
- **Recovery** — reopening and reconciling a document after a crash, from
  autosave and operation journals.
- **History** — the undo/redo transaction stack
  (`loom-core/crates/loom-history`).
- **Workspace** — the arrangement of panels/windows for an application
  (also "work-space"; a mode in Loom Studio).
- **Inspector** — the context-sensitive property panel that updates with the
  selection (`../loom-design-bible/INSPECTORS.md`).
- **Preset** — a named, reusable settings bundle (export preset, effect
  preset, instrument preset).
- **Template** — a reusable document/deck/composition starting point.
- **Theme** — a named set of design tokens applied to a deck or document.
- **Provider** — an implementation of a capability behind a capability trait
  (`loom-vision`).
- **Capability** — a named perception ability (OCR, segmentation, tracking, …)
  identified by `CapabilityId`.
- **Model pack** — an installable local directory of model files with
  `manifest.json`, checksums, license, and capability declarations.
- **Reference provider** — a small CPU-only provider proving a capability
  contract (QR decode, image statistics).
- **Plugin** — a sandboxed extension packaged as `.loomplugin` (WASM-based
  design); declares capabilities and permissions in its manifest
  (`loom-plugin-sdk`).
- **Sandbox** — the runtime boundary that isolates plugins from the host.
- **Permission** — a declared plugin capability grant checked at call time.
- **Inspector section** — a group of inspector properties (object, style,
  document, metadata, advanced).
- **Progressive disclosure** — revealing advanced controls only when needed.

## Editing concepts

- **Layer** — an element in a stack (Photo/Motion) with order, blend mode,
  and opacity; may be pixel, vector, text, adjustment, or fill.
- **Mask** — a grayscale channel controlling visibility; AI-assisted masks
  must remain editable as ordinary masks.
- **Adjustment layer** — a layer that applies a nondestructive tonal/color
  change.
- **Blend mode** — how a layer composites with layers below.
- **Clip** — a media segment placed on a timeline.
- **Track** — a timeline lane grouping related clips (video, audio role).
- **Timeline** — the time-based editing surface.
- **Keyframe** — a value-anchor in time; animation interpolates between
  keyframes.
- **Curve** (graph editor) — the interpolation shape between keyframes.
- **Parenting** — linking a layer's transform to another layer.
- **Composition** — a self-contained animated scene (Motion).
- **Compound clip / nested timeline** — a clip that contains a timeline.
- **Multicam** — synchronized multiple angles cut together.
- **Proxy** — a low-resolution stand-in for a media file, replaced by the
  original at output.
- **Transcode** — converting media between formats/codecs (Loom Encode).
- **Stem** — a per-role audio mix (e.g. drums, vocals) exported separately.
- **MIDI** — the note/control protocol used by Studio instruments.
- **Sample** — (a) an audio snippet; (b) a sample project in `loom-samples`.
  Context disambiguates.
- **Master slide / layout** — presentation templates for slides and their
  placeholders.
- **Guide** — a non-printing alignment reference on a canvas.
- **Rule-based structure** — shared across apps: styles, tables, sections,
  columns, headers/footers, footnotes/endnotes, cross-references, change
  tracking, comments (Writer).

## Data and interchange

- **Cell** — a spreadsheet coordinate (`CellRef`, 0-based row/column).
- **Formula** — an expression evaluated by the Sheets engine over the
  dependency graph.
- **Dependency graph** — the directed graph of formula precedents/dependents;
  topological evaluation with cycle detection.
- **Named range** — a named cell region usable in formulas.
- **Pivot table** — a re-aggregation of tabular data.
- **Import report** — the report a converter emits explaining substitutions
  or losses during import.
- **Interchange** — importing/exporting non-Loom formats (ODT, DOCX, XLSX,
  PPTX, PDF, CSV, …) with documented fidelity.
- **Linked asset** — external media referenced by a package, with relinking
  support.
- **Clipboard format** — the Loom typed payload format for cross-application
  copy/paste.

## Vision and media

- **Luma image** — a grayscale image interchange type used by providers.
- **Bounding box (BBox)** — an object-detection result rectangle.
- **OCR** — optical character recognition (capability).
- **Segmentation** — per-pixel labeling of an image (semantic/instance/
  salient-object/portrait).
- **Tracking** — following objects/points/planes across video frames.
- **Optical flow** — per-pixel motion between frames.
- **Transcription** — speech-to-text (local, provider-based).
- **Embedding** — a fixed-size vector representing image/video/audio content
  for similarity search.
- **Index** — the local search index over approved directories and opened
  projects (no network).

## Quality and process

- **Vertical slice** — one complete user workflow (UI, engine, persistence,
  tests, accessibility, visual QA) before broad feature expansion.
- **Quality gate** — a mandatory automated check (fmt, clippy, tests,
  visual, license, …). See `../loom-bootstrap/`.
- **Golden baseline** — a committed reference screenshot for visual
  regression.
- **Perceptual diff** — image comparison with documented tolerance.
- **Pseudolocale** — a locale variant that stresses layout expansion for
  localization testing.
- **MSRV** — minimum supported Rust version (1.80).
- **COMPATIBILITY.toml** — the suite compatibility manifest owned by
  `loom-bootstrap`.
- **Status words** — `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`,
  `EXPERIMENTAL`, `SCAFFOLDED`, `NOT_STARTED`, `BLOCKED`.

## File extensions

`.loomdoc` (Writer) · `.loomtable` (Sheets) · `.loomdeck` (Present) ·
`.loomphoto` (Photo) · `.loommotion` (Motion) · `.loomvideo` (Video) ·
`.loomstudio` (Studio) · `.loomencode` (Encode) · `.loomplugin` (plugin
package).


## source-loom-spec-docs-adrs-adr-0001-slint-licensing-and-distribution-md

Original path: `loom-spec/docs/adrs/ADR-0001-Slint-Licensing-and-Distribution.md`

# ADR-0001 — Slint Chosen as UI Toolkit; License and Distribution Model

- Status: **ACCEPTED**
- Date: 2026-08-01
- Supersedes: n/a

## Context

The root product directive selects Slint for the application UI. Slint is
licensed GPL-3.0-only or under a commercial license — it is not
MIT/Apache-2.0. Loom's own code is MIT OR Apache-2.0. This must be
documented as a deliberate distribution decision, not discovered at
release time.

## Decision

- Adopt Slint (pinned 1.17.1) as the UI toolkit per the product directive
  (`RFC-0003-Slint-Integration-Model.md`).
- Loom-authored source stays MIT OR Apache-2.0 (all crates declare
  `license = "MIT OR Apache-2.0"`).
- Distributed binaries that link Slint components are GPL-3.0-covered for
  the combined work unless a commercial Slint license is obtained for the
  release. This is documented in each application's `LICENSE_POLICY.md`
  and in the release `LICENSE_REPORT.md` (`../loom-bootstrap/`).
- The Slint dependency is pinned with a lockfile; upgrades require the
  compatibility workflow (`COMPATIBILITY_POLICY.md`).

## Consequences

- The `loom-ui` crate and app binaries carry the GPL notice requirement;
  the license report must list Slint explicitly with its dual-license
  terms.
- Engines (`-core` crates) must never depend on Slint, so they remain
  MIT/Apache-2.0 and can be reused independently
  (`RFC-0002-UI-and-Engine-Separation.md`).
- Distribution variants (AppImage/Flatpak) must carry accurate license
  metadata; commercial licensing of the suite remains an upstream
  business decision, out of scope here.

## Verification

- License audit at every release checkpoint flags Slint with GPL-3.0-only
  and confirms the notice text is included
  (`RELEASE_CRITERIA.md` §1.6).


## source-loom-spec-docs-adrs-adr-0002-path-based-crate-pinning-md

Original path: `loom-spec/docs/adrs/ADR-0002-Path-Based-Crate-Pinning.md`

# ADR-0002 — Path-Based Pinning of Shared Crates During Development

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

During development, `loom-core` crates are not published. Applications
need to consume them without version churn on every change.

## Decision

- Applications depend on shared crates by **path dependency** during
  development, e.g. `loom-writer/Cargo.toml`:
  `loom-document = { path = "../loom-core/crates/loom-document" }`.
- Path pins are dev-only and never published; tagged releases use version
  requirements with lockfiles (`COMPATIBILITY_POLICY.md` §4).
- `COMPATIBILITY.toml` in `loom-bootstrap` records pinned revisions and
  expected crate versions so bootstrap validates workspace consistency
  (`RFC-0001-Repository-and-Versioning-Strategy.md`).

## Consequences

- Any change to a shared crate is immediately visible to consumers —
  catch contract breaks early, keep semver discipline anyway
  (`COMPATIBILITY_POLICY.md` §2).
- Builds only work inside the checkout; CI and Docker build from the
  pinned workspace, so this is acceptable.
- Consumers must not copy shared code into their own trees; the pin is the
  single source.

## Verification

- Bootstrap CI builds the full pinned workspace on every shared-crate
  change; a consumer build against the workspace is part of the gate
  (`IMPLEMENTATION_GUIDE.md` §6).


## source-loom-spec-docs-adrs-adr-0003-headless-screenshots-md

Original path: `loom-spec/docs/adrs/ADR-0003-Headless-Screenshots.md`

# ADR-0003 — Headless Screenshots via the Slint Software Renderer

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Visual QA must be deterministic and runnable in CI/Docker without GPUs or
window managers (`RFC-0015-Visual-Regression-System.md`).

## Decision

- Screenshots are captured through the **Slint software renderer** with a
  **custom platform** (no X11/Wayland dependency), rendering components
  and windows to a pixel buffer and writing PNGs plus metadata (theme,
  locale, font config, renderer, commit).
- Baselines are generated **only inside the pinned Docker visual image**
  (`../loom-bootstrap/docker/Dockerfile.visual`) so fonts, locales, and
  renderer versions are identical for every run.
- Baselines are committed in `loom-design-bible`; diffs use documented
  perceptual tolerances; no automatic approval of changed baselines.
- Xvfb/headless E2E (input automation) remains a separate, complementary
  path.

## Consequences

- Deterministic pixel comparison on any host, offline; no GPU variance.
- Software rendering does not verify GPU paths; GPU-specific tests are
  deferred to RFC-0004.
- Screenshot capture must run in an environment matching the pinned image
  — regenerating baselines outside Docker is prohibited
  (`COMPATIBILITY_POLICY.md` §6).

## Verification

- Harness self-test: identical renders diff zero; an injected 1-pixel
  change exceeds tolerance (`RFC-0015` §Testing).


## source-loom-spec-docs-adrs-adr-0004-deterministic-mutation-fuzzing-md

Original path: `loom-spec/docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`

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


## source-loom-spec-docs-adrs-adr-0005-internal-pdf-writer-md

Original path: `loom-spec/docs/adrs/ADR-0005-Internal-PDF-Writer.md`

# ADR-0005 — Minimal Internal PDF Writer for Writer/Present/Sheets Export

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Writer, Present, and Sheets require PDF export. External PDF libraries
vary in maintenance and licensing; a full rendering engine is not needed
in the initial milestones.

## Decision

- Implement a **minimal internal PDF writer** (vector drawing + text
  placement + embedded fonts, single-page streams, PDF 1.7 subset)
  shared across Writer/Present/Sheets, owned as a future
  `loom-core` crate (`loom-pdf`, NOT_STARTED).
- Scope for the initial milestone: text (with shaping output from
  `loom-text`), vector shapes, images (JPEG passthrough, PNG→Flate),
  clipping, basic transparency; no interactive features, forms, or
  encryption.
- Feature-gated and deterministic: byte-stable output for identical input
  (golden-file testable).
- Re-evaluate a mature Rust PDF library at the PDF milestone; if one is
  adopted, it must satisfy the dependency policy (license, maintenance,
  Linux support, thread safety — root directive §5) and be recorded in
  `DEPENDENCIES.md`.

## Consequences

- Small surface, full control over determinism and licenses (MIT OR
  Apache-2.0 maintained).
- Risk of gaps vs mature libraries (complex tables, RTL edge cases);
  mitigated by integration tests against PDF consumers and documented
  import/export reports where fidelity is limited (`RELEASE_CRITERIA.md`
  §1.9).

## Verification

- Golden PDF byte tests; smoke-open tests with a PDF parser in the test
  suite; visual QA renders PDF exports to images in the Docker image
  (`RFC-0015-Visual-Regression-System.md`).


## source-loom-spec-docs-adrs-adr-0006-image-codec-backend-md

Original path: `loom-spec/docs/adrs/ADR-0006-Image-Codec-Backend.md`

# ADR-0006 — `image` Crate as the Image Codec Backend

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Loom apps need PNG/JPEG/WebP (and later TIFF/AVIF/EXR/OpenRaster) codecs.
Writing codecs from scratch is out of scope; the backend must be
maintained, Linux-supporting, and license-compatible.

## Decision

- Use the Rust **`image`** crate as the image codec backend for the
  initial milestone (PNG, JPEG, WebP, GIF basics; decode and encode),
  wrapped behind a future `loom-core` media API (`loom-media`,
  NOT_STARTED) so the backend stays swappable.
- Verify current official docs, maintenance status, Linux support,
  license (MIT/Apache-2.0), and thread-safety before pinning; record the
  pinned version and the verification in `DEPENDENCIES.md` per the
  dependency policy (root directive §5).
- TIFF, AVIF, EXR, and RAW support are future milestones via feature
  flags; PSD/OpenRaster require dedicated work (NOT_STARTED).

## Consequences

- Fast start with a well-maintained backend; `image` pulls codec crates
  that may vary in maintenance — feature-gate codecs so a weak optional
  codec cannot block the core build, and document each codec in the
  dependency audit.
- Encoder/decoder behavior differences are isolated behind `loom-media`'s
  API; swapping later is contained.

## Verification

- Round-trip tests (decode→encode→decode) per format; golden-file image
  tests; the wrapped API has fuzz targets for decoders
  (`IMPLEMENTATION_GUIDE.md` §5).


## source-loom-spec-docs-adrs-adr-0007-no-ffmpeg-in-initial-milestone-md

Original path: `loom-spec/docs/adrs/ADR-0007-No-FFmpeg-in-Initial-Milestone.md`

# ADR-0007 — No FFmpeg in the Initial Milestone

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Video, Motion, Studio, and Encode eventually need audio/video demux,
decode, encode, and transcode. FFmpeg is the common choice but brings
licensing complexity (codec patents, GPL components), packaging burden,
and a large native dependency. The initial milestones have no media apps
implemented (`FEATURE_MATRICES.md` §6–§11).

## Decision

- **Do not add FFmpeg (or any media framework) in the initial milestone.**
- Initial media scope is limited to formats implementable with small,
  license-clean dependencies: **PNG, JPEG, WebP (via `image` — ADR-0006),
  and WAV** for audio (plus future FLAC via a permissive crate). This is
  documented in `DEPENDENCIES.md` of each affected repository.
- When media apps begin (Phase 4/5), evaluate FFmpeg or GStreamer
  formally: licensing and patent analysis, packaging strategy, sandboxing
  (parser isolation), maintenance, and a replacement strategy — per the
  root directive §5 — and record the outcome in an ADR/RFC
  (RFC-0012-Media-Framework is the designated place).
- Any codec with patent implications is isolated behind feature flags with
  documented package variants (root directive §18).

## Consequences

- Clean licensing posture in early releases; no heavyweight native deps;
  media-heavy features stay honestly NOT_STARTED in `FEATURE_MATRICES.md`.
- Media workflows (video editing, transcoding) are impossible until the
  framework decision lands — an accepted trade-off for the milestone
  order.

## Verification

- Dependency audit lists exactly the permitted codec crates per repo;
  release gate fails if FFmpeg or GPL codecs appear in the initial
  milestone builds without an ADR (`RELEASE_CRITERIA.md` §1.6).


## source-loom-spec-docs-rfcs-rfc-0001-repository-and-versioning-strategy-md

Original path: `loom-spec/docs/rfcs/RFC-0001-Repository-and-Versioning-Strategy.md`

# RFC-0001 — Repository and Versioning Strategy

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: whole suite

## Context

Loom is a suite of applications sharing platform code. The root directive
mandates separate repositories per application, versioned shared crates, and
no unversioned source copying. We need a repository layout and versioning
strategy that keeps contracts single-sourced while allowing each repository
to build independently.

## Goals

- Each application repository builds independently against pinned shared
  crate versions.
- Shared code is versioned (semantic) and reused, never copied.
- No circular repository dependencies.
- Reproducible builds with lockfiles; cross-repo state is verifiable.

## Non-goals

- A monorepo or a workspace spanning repositories.
- Publishing to a public registry in the initial milestone (later, tagged
  releases only).
- Cloud or networked versioning services as a build requirement.

## Proposed design

- One repository per concern: `loom-core`, `loom-vision`, `loom-plugin-sdk`,
  `loom-writer`, `loom-sheets`, `loom-present`, `loom-photo`, `loom-motion`,
  `loom-video`, `loom-studio`, `loom-encode`, `loom-bootstrap`,
  `loom-design-bible`, `loom-samples`, and this repository (`loom-spec`).
- Dependency direction: `loom-bootstrap` → all; applications →
  `loom-core`/`loom-vision`/`loom-plugin-sdk`; shared repos → external
  crates only (`ARCHITECTURE.md` §1).
- Development pinning: path dependencies into `loom-core` (see
  ADR-0002) plus a suite compatibility manifest `COMPATIBILITY.toml` in
  `loom-bootstrap` recording each repository's pinned revision and expected
  crate versions (`COMPATIBILITY_POLICY.md`).
- Releases (future): `loom-core`, `loom-vision`, `loom-plugin-sdk` tag and
  publish semver crates; applications depend on version ranges with
  lockfiles; `loom-bootstrap` validates a release manifest.
- All workspaces: Cargo workspace per repository, edition 2021, MSRV 1.80,
  MIT OR Apache-2.0.

## Alternatives

- **Single monorepo**: simpler cross-repo refactors, but conflicts with the
  root directive's separate-repository architecture and makes independent
  versioning and ownership harder.
- **Publish-only (no path deps)**: forces registry publication before any
  inter-repo work is possible; premature at 0.1.0.
- **Versioned source copying**: rejected — drift and double maintenance.

## Trade-offs

Path deps make builds only valid inside the checkout; mitigated by
`COMPATIBILITY.toml` validation and by treating path pins as dev-only
(ADR-0002). Cross-repo refactors cost more than in a monorepo; mitigated by
narrow contracts owned by single repositories.

## Security

Path deps never leave the local checkout; release artifacts use tagged
versions with lockfiles. No supply-chain mechanism changes otherwise.

## Performance

No runtime impact; only build/versioning concerns.

## Compatibility

Every repository pins its own lockfiles; `COMPATIBILITY.toml` is the
cross-repo contract (`COMPATIBILITY_POLICY.md` §5).

## Migration

From current state (core crates exist at 0.1.0, writer/sheets consume via
path deps): no migration needed; this strategy formalizes existing practice.

## Testing

Bootstrap validates: all repos build against pinned revisions; lockfiles
match the compatibility manifest; no repository depends on another
application.

## Open questions

- Registry choice when tagged releases begin (crates.io vs private).
- Whether `loom-spec`/`loom-design-bible` receive tags in the release
  manifest (likely yes, informational).

## Final status

ACCEPTED. Current practice (path pins, per-repo workspaces, MSRV 1.80,
lockfiles) is the norm; release tagging is deferred to `ROADMAP.md` Phase 8.


## source-loom-spec-docs-rfcs-rfc-0002-ui-and-engine-separation-md

Original path: `loom-spec/docs/rfcs/RFC-0002-UI-and-Engine-Separation.md`

# RFC-0002 — UI and Engine Separation

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: all applications

## Context

Loom applications need professional depth and testability. Slint UIs and
GPU canvases are hard to exercise in headless CI and Docker; document
engines must be verifiable without a display. The vertical-slice strategy
(Phase 4) requires engines first, UIs later.

## Goals

- Every application engine is a headless library, fully testable without a
  display, with a CLI harness for scripting and Docker visual QA.
- The UI layer (Slint) is a thin consumer of engine services.
- UI↔engine communication goes through commands and change notifications
  with defined interfaces.
- Engines, UI, and persistence are independently testable.

## Non-goals

- A UI toolkit abstraction layer; Slint is the toolkit (RFC-0003).
- Mixing rendering logic into document models.

## Proposed design

- Each application repository: `crates/<app>-core` (engine), optional
  `crates/<app>-ui` (Slint, future), `crates/<app>-cli` (headless harness).
  Precedent: `loom-writer-core`/`loom-writer-cli`, `loom-sheets-core`/
  `loom-sheets-cli`.
- The engine owns: document model, persistence (via `loom-package`),
  commands (via `loom-command`), history (via `loom-history`), and jobs
  (via `loom-jobs`).
- The UI owns: Slint components (from `loom-ui`), rendering of canvases
  (future `loom-render`/wgpu), input mapping to commands.
- Engines expose change notifications; UI subscribes and re-renders. UI
  never mutates documents directly — it invokes commands, which update
  history atomically.
- Selection, clipboard, and drag/drop cross the boundary as typed payloads,
  not toolkit objects.
- Engines must compile and pass tests on any platform Slint can target and
  on the CPU-only compatibility tier.

## Alternatives

- **Fat UI with embedded logic**: faster prototyping, but untestable logic
  and UI-thread blocking risks; rejected.
- **Full MVC frameworks**: unnecessary ceremony for Slint's declarative
  model; rejected.

## Trade-offs

Some indirection is required (command plumbing, notification fan-out) in
exchange for deterministic tests, headless QA, and the ability to reuse
engines in CLIs. Engines that need rendering (Photo/Motion/Video) define a
render-service interface consumed by the UI; the engine stays display-free
by treating the renderer as a dependency-injected service (not yet
implemented).

## Security

A narrower UI surface means untrusted input (clipboard, drag/drop, files)
is validated at the engine boundary with fuzz targets
(`IMPLEMENTATION_GUIDE.md` §5).

## Performance

Commands are cheap indirection; hot paths (scrolling, scrubbing) bypass
command dispatch through dedicated render streams where needed
(`../loom-design-bible/PERFORMANCE.md`).

## Compatibility

UI and engine can evolve at different paces; the command/notification
contract is versioned via `loom-command` (`COMPATIBILITY_POLICY.md` §2).

## Migration

Engines already built headless; no migration. Future UI crates add
dependencies upward only.

## Testing

- Engine: unit, property, fuzz, integration tests (headless).
- UI: E2E tests plus visual regression via software renderer
  (`RFC-0015-Visual-Regression-System.md`).
- CLI: golden-file tests for scripts and Docker pipelines.

## Open questions

- Change-notification batching strategy for high-frequency edits
  (deferred to the `loom-ui` design task).

## Final status

ACCEPTED. Existing engines conform; new application crates must follow this
separation.


## source-loom-spec-docs-rfcs-rfc-0003-slint-integration-model-md

Original path: `loom-spec/docs/rfcs/RFC-0003-Slint-Integration-Model.md`

# RFC-0003 — Slint Integration Model

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-ui`, all applications

## Context

Loom's product directive selects Slint as the UI toolkit (Rust-native,
declarative, accessible, small-footprint). We need one shared way to build
and consume Slint UIs across applications, plus a deterministic screenshot
path for visual QA. Pinned version: **Slint 1.17.1** (license implications
recorded in `docs/adrs/ADR-0001-Slint-Licensing-and-Distribution.md`).

## Goals

- One `.slint` component library in `loom-core`'s UI crate (`loom-ui`,
  not yet implemented) shared by every application.
- Applications consume components via Slint's `library_paths`, never by
  copying `.slint` files between repositories.
- Deterministic rendering for tests: Slint **software renderer** in headless
  mode with a custom platform, capturing screenshots for golden baselines.
- GPU rendering (wgpu/Vulkan) remains the runtime path for canvases
  (future RFC-0004); the software path exists for QA and CPU-only tiers.

## Non-goals

- Multi-toolkit abstraction (RFC-0002).
- Browser-based UI.

## Proposed design

- `loom-ui` crate owns: the component gallery, design-token-driven Slint
  styles (`../loom-design-bible/` tokens), and shared components
  (inspector, palettes, dialogs, timeline widgets, command palette).
- Applications declare `slint::include_modules!()` against components
  resolved through `library_paths` pointing at `loom-ui`.
- Headless screenshots: a test harness creates the Slint window/component
  with the software renderer and a custom platform (no X11/Wayland
  required), renders to a pixel buffer, and writes PNGs
  (`docs/adrs/ADR-0003-Headless-Screenshots.md`). Baselines are committed
  in `loom-design-bible` and generated only in pinned Docker images.
- Visual QA runs the same screenshot path inside Docker
  (`../loom-bootstrap/docker/Dockerfile.visual`).
- Version pinning: Slint 1.17.1 in workspace dependencies with lockfiles;
  upgrades follow `COMPATIBILITY_POLICY.md` §2.

## Alternatives

- **Re-export Slint per app with copied components**: duplication and
  contract drift; rejected.
- **Live Qt-style toolkits**: outside the directive; rejected.
- **OS-level windowing tests (Xvfb)**: still needed for E2E, but slower and
  less deterministic than software-renderer screenshots for pixel tests;
  both are used (screenshots for regression, Xvfb/headless for E2E).

## Trade-offs

Software-renderer screenshots do not prove GPU path fidelity; mitigated by
per-surface GPU tests later (RFC-0004) and by keeping the canvas rendering
logic deterministic in shared crates. `library_paths` couples `loom-ui`
releases to apps; managed by `COMPATIBILITY.toml`.

## Security

Slint parses `.slint` at compile time; no runtime download. The custom
platform is test-only and never reached from release binaries' input path.

## Performance

Software renderer is test-only; runtime UI uses the accelerated renderer.
Screenshot tests must set explicit sizes to keep CI time bounded.

## Compatibility

Component identifiers in `loom-ui` are versioned with the crate
(`COMPATIBILITY_POLICY.md` §2); renaming a component requires a minor bump
and a bootstrap compatibility entry.

## Migration

No UI code exists yet; the model is adopted as `loom-ui` is created. Slint
1.17.1 is the pinned baseline from day one.

## Testing

- Component gallery screenshots + golden baselines (Docker-pinned).
- E2E flows per application through the headless/Xvfb path.
- A smoke test proving `library_paths` resolution in every app workspace.

## Open questions

- Exact set of first-wave shared components (decided at `loom-ui` kickoff).

## Final status

ACCEPTED. Slint 1.17.1; `loom-ui` with `library_paths`; software-renderer
screenshot harness; baselines only from pinned Docker images. `loom-ui` is
NOT_STARTED (`FEATURE_MATRICES.md` §1).


## source-loom-spec-docs-rfcs-rfc-0005-text-shaping-and-layout-md

Original path: `loom-spec/docs/rfcs/RFC-0005-Text-Shaping-and-Layout.md`

# RFC-0005 — Text Shaping and Layout

- Status: **ACCEPTED (normative architecture; parts NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-text`, `loom-ui`, Writer/Sheets/Present

## Context

Loom is a creative suite where typography is a defining capability
(`../loom-design-bible/TYPOGRAPHY.md`). Requirements: Unicode, bidi,
script shaping, font fallback, variable fonts, OpenType features, ligatures,
kerning, baseline alignment, hyphenation, justification, language-aware
behavior, style systems, baseline grids, vertical text where feasible,
high-DPI rendering, deterministic layout. `loom-text` already provides
paragraph/character style value objects and style runs
(`FEATURE_MATRICES.md` §1).

## Goals

- One text architecture shared by all applications.
- Deterministic line layout for tests and visual QA.
- A path to paginated document layout for Writer (master pages, columns,
  headers/footers, footnotes).
- Rich text editing surfaces in Writer, Present, and Photo (text layers).

## Non-goals

- Implementing a full shaping engine from scratch.
- Vertical text in the initial milestone (architecture must not preclude it).

## Proposed design

- **Rendering and shaping**: use Slint's text rendering
  (`Text`/`TextEdit` + font handling) for UI text, panels, dialogs, and
  simple labels — one stack, consistent with `RFC-0003`.
- **Typography engine**: evaluate Parley (shaping/layout, from the same
  ecosystem as Slint's text) plus Fontique (font discovery/fallback) as the
  engine-level shaping/layout layer behind a `loom-text` layout API. The
  choice is recorded as an architecture decision here; a dependency ADR must
  be added when the crates are actually adopted (license + maintenance
  verification per the root directive §5).
- **Deterministic layout**: `loom-text` exposes a layout pipeline (font
  resolution → shaping → line breaking → inline layout) that can run
  headless and produce deterministic metrics for tests and golden files.
- **Paginated mode (NOT_STARTED)**: pagination, master pages, columns,
  headers/footers, footnotes, TOC, and page styles are application-level
  layout on top of the engine API; no implementation exists yet.
- **Bidi**: text model stores logical order; bidi processing happens at
  layout/rendering time; cursor mapping APIs must exist for editing
  (UTF-16 cursor mapping tests are a planned task).

## Alternatives

- **Rustybuzz + fontdb direct**: mature and lightweight; still a viable
  fallback if Parley integration lags, but less aligned with Slint's stack.
- **Skrifa/read-fonts alone**: shaping/layout still needed on top.
- **System text stacks (Pango)**: non-Rust, GTK-adjacent; rejected per
  product directive.

## Trade-offs

Parley is evolving (API churn risk); mitigated by pinning and by keeping
`loom-text`'s layout API stable while the backend is swappable. Slint text
for UI vs Parley for documents creates two code paths; they converge on the
same style model in `loom-text`.

## Security

Font parsing is untrusted input; fonts from documents must go through the
chosen parser with fuzz coverage (untrusted font handling is a security
requirement, root directive §17). No network font loading ever.

## Performance

Layout must never block the UI thread for documents: async layout jobs
behind the engine API (`loom-jobs`). Deterministic layout enables cached
layout results keyed by style + width.

## Compatibility

Style objects (`loom-text`) are stable; the layout API is additive.
Document packages store logical text + styles, never laid-out metrics
(rendering stays deterministic per environment).

## Migration

None required yet: no layout code exists. When Parley is adopted it lands
behind the `loom-text` API.

## Testing

- Deterministic layout golden tests (fixed fonts in pinned Docker images).
- Bidi/Unicode cursor movement property tests; range operation tests.
- Shaping tests for ligatures/kerning/fallback with bundled test fonts.
- Hyphenation/justification tests for supported languages.
- Fuzz targets for rich-text parsing and font handling.

## Open questions

- Exact Parley adoption timing vs a Rustybuzz interim
  (resolved when `loom-text` layout work starts; recorded in an ADR).

## Final status

ACCEPTED as architecture. Style model implemented; layout/pagination
NOT_STARTED. Superseding decisions require a new RFC or ADR.


## source-loom-spec-docs-rfcs-rfc-0006-file-package-format-md

Original path: `loom-spec/docs/rfcs/RFC-0006-File-Package-Format.md`

# RFC-0006 — File Package Format

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-package`, all applications

## Context

Loom documents must be user-owned: documented, versioned, inspectable,
backup-able with ordinary filesystem tools, portable between computers, and
partially recoverable. The root directive specifies a ZIP-based package
container with `manifest.json`, `content/`, `assets/`, `previews/`,
`metadata/`, `history/`, `recovery/`.

## Goals

- One container design for all eight extensions (`.loomdoc` … `.loomencode`)
  with per-app content schemas.
- Versioned schema with forward-compatibility strategy and migration rules.
- Corruption detection via per-entry SHA-256 checksums.
- Security limits against archive bombs and path traversal.
- Deterministic serialization for reproducible archives and golden tests.

## Non-goals

- A database or transactional filesystem as the primary store.
- Cloud storage of any kind.

## Proposed design

The design mirrors `FILE_FORMAT_FAMILY.md` and is implemented in
`loom-core/crates/loom-package` (`manifest.rs`, `zip.rs`):

- ZIP archive; `manifest.json` first; entries grouped under `content/`,
  `assets/`, `previews/`, `metadata/`, `history/`, `recovery/`.
- Manifest: `format_version` (integer, currently 1), stable `id` +
  `revision`, `created`/`modified` timestamps, `app` origin, per-entry
  SHA-256 checksums, entry table (path, size, kind, MIME).
- Reader enforcement: entry-count/size/compression-ratio limits, path
  traversal rejection, duplicate-entry rejection, checksum verification
  before use.
- Forward compatibility: readers ignore unknown optional entries and
  unknown manifest fields with a warning; future `format_version` opens
  read-only with an explanatory error.
- Migration: older versions migrate in memory and save current; original
  file replaced only after a successful save.
- Media policy: embedded vs external per application
  (`FILE_FORMAT_FAMILY.md` §7); external references are relative paths with
  checksums for relinking.
- Deterministic serialization: stable key order, no content timestamps,
  fixed float formatting.

## Alternatives

- **Plain directories**: simpler to inspect, but not a single file; harder
  to move/backup atomically. Rejected for the default; large media may use
  package directories per the root directive §9 trade-off note.
- **SQLite containers**: transactional, but not human-inspectable and more
  complex; rejected for documents.
- **Custom binary containers**: rejected — ZIP is inspectable and
  universally supported.

## Trade-offs

ZIP is not crash-atomic; mitigated by write-new-then-rename semantics and
the `history/`/`recovery/` journals (RFC-0018). Checksums add size/compute
overhead; acceptable for professional documents.

## Security

Limits above; ZIP-slip protection is mandatory; malformed input is fuzz
targeted (`IMPLEMENTATION_GUIDE.md` §5); extraction never follows symlinks.

## Performance

Readers stream entries and validate lazily; opening must not load the full
package into memory; previews load independently of content.

## Compatibility

`format_version` is the compatibility contract; bumping requires migration
tests, fixtures, a golden corpus, and `COMPATIBILITY.toml` entries
(`FILE_FORMAT_FAMILY.md` §3, `COMPATIBILITY_POLICY.md` §5).

## Migration

Implemented as specified in `FILE_FORMAT_FAMILY.md` §3; version-1 packages
are the current baseline.

## Testing

Round-trip property tests; corruption tests (truncation, bit flips, missing
entries, bad checksums, zip bombs, traversal); migration tests; fuzz
targets for reader and manifest parser; golden compatibility corpus
(`FILE_FORMAT_FAMILY.md` §8).

## Open questions

- Maximums above are initial defaults; tune via ADR if real packages
  exceed them.

## Final status

ACCEPTED. Implemented in `loom-package` (19 tests). `FILE_FORMAT_FAMILY.md`
is the normative reference.


## source-loom-spec-docs-rfcs-rfc-0007-undo-and-transaction-system-md

Original path: `loom-spec/docs/rfcs/RFC-0007-Undo-and-Transaction-System.md`

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


## source-loom-spec-docs-rfcs-rfc-0008-async-job-framework-md

Original path: `loom-spec/docs/rfcs/RFC-0008-Async-Job-Framework.md`

# RFC-0008 — Async Job Framework

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-jobs`, all applications

## Context

Loom's performance principle (§2.5 of `PRODUCT_SPEC.md`) forbids UI-thread
blocking for media decoding, parsing, inference, autosave, export,
thumbnails, waveforms, proxies, font scanning, plugin discovery, and search
indexing. The root directive requires a shared job framework with progress,
cancellation, priority, dependencies, resource estimates, retry, cleanup,
persistence, and crash recovery where appropriate.

## Goals

- One job framework in `loom-core/crates/loom-jobs` used by every app,
  Vision, and Encode.
- Structured concurrency and cancellation: cancel propagates and cleanup
  runs.
- Observable progress; immediate cancellation feedback.
- Priorities and dependencies (e.g. thumbnail jobs yield to direct
  manipulation; encode jobs chain on source jobs).

## Non-goals

- A full scheduler/executor replacement — Tokio (or justified equivalent)
  remains the async runtime.
- Distributed or networked job execution.

## Proposed design

- `loom-jobs` defines: `Job` (unit of work), job ids, `JobHandle`
  (progress + cancellation), `JobSpec` (priority, dependencies, resource
  estimate), and a registry of running/completed jobs for observability.
  Implemented: core with progress, cancellation, and priorities; 5 tests.
- Cancellation: cooperative — jobs poll a cancel flag and check it at
  structured checkpoints; long loops must check per-item. Cancel handlers
  run cleanup (temp files, resources) via RAII guards.
- Priorities: a simple scheduler ordering where lower-priority background
  work (thumbnails, indexing, waveforms) yields to user-facing work; no
  preemption — cooperation only, which is sufficient because jobs never run
  on the UI thread.
- Dependencies: a job may declare completion prerequisites (e.g. "transcode
  after all sources copied"); the scheduler starts dependents when
  prerequisites finish.
- Retry: explicit, per-job policy (e.g. export retries on transient IO);
  never auto-retry user-visible failures without feedback.
- Persistence (NOT_STARTED): durable job records for recovery after app
  restart (Encode queue), designed as a storage-backed journal in
  `loom-storage`.
- Observability: the registry exposes state for the jobs panel
  (progress, errors, cancellation); logs are privacy-safe
  (`PRODUCT_SPEC.md` §2.2).

## Alternatives

- **Ad-hoc per-app threading**: duplicated cancellation/progress and
  inconsistent UX; rejected.
- **Crate-heavy async frameworks (rayon, tokio-rs)**: Tokio as runtime is
  already accepted; `loom-jobs` layers semantics (progress/priority/
  dependencies) on top rather than replacing it.
- **OS threads per task**: acceptable for some I/O work, but the framework
  must present one API regardless of backend.

## Trade-offs

Cooperative cancellation requires discipline (every long loop checks);
enforced by tests and clippy-friendly patterns. A centralized registry adds
a little overhead per job; negligible against real work (media, inference).

## Security

Jobs touch user files and model packs; cleanup must remove partial
artifacts; cancellation must not leave half-written packages
(`RFC-0006`). Job inputs from plugins are permission-checked
(`loom-plugin-sdk`).

## Performance

No UI-thread blocking by construction; the registry must not contend on
hot paths (lock-free reads where practical); cancellation checkpoints must
be cheap.

## Compatibility

Job contracts are internal; persisted job records (future) versioned with
`loom-storage` schemas.

## Migration

None yet; adopted as apps integrate jobs (autosave, export, vision, encode).

## Testing

- Cancellation tests: cancel mid-loop, assert cleanup ran and no partial
  output.
- Priority tests: background job yields to foreground.
- Dependency tests: dependent job starts only after prerequisites.
- Progress monotonicity tests; retry policy tests; crash-recovery tests for
  persisted records (future).

## Open questions

- Scheduler thread-pool sizing policy (resolved at first app integration).

## Final status

ACCEPTED. Core implemented in `loom-jobs` (5 tests); persistence and
registry UI NOT_STARTED.


## source-loom-spec-docs-rfcs-rfc-0010-vision-provider-model-md

Original path: `loom-spec/docs/rfcs/RFC-0010-Vision-Provider-Model.md`

# RFC-0010 — Loom Vision Provider Model

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-vision`, all applications

## Context

Loom Vision must expose perception capabilities without being hard-wired
to any model, vendor, runtime, or hardware backend
(`PRODUCT_SPEC.md` §4). Applications need stable capability traits, not
model-specific APIs; providers come from bundled reference
implementations and user-installed model packs.

## Goals

- Capability-based provider model: applications request a capability, the
  registry returns the best installed provider.
- Providers are backend-agnostic (CPU, Vulkan, ONNX, Candle, optional
  CUDA/ROCm/OpenVINO/…).
- Uniform cancellation and progress via `RunContext`.
- A CPU fallback exists for every capability, or the absence of a provider
  is clearly reported.

## Non-goals

- Bundling ML models in the platform.
- Remote inference of any kind.

## Proposed design

Mirrors `../loom-vision/ARCHITECTURE.md` and the implementation in
`loom-vision-core`:

- `CapabilityId` — stable capability names (`ocr`, `segmentation`,
  `object_detection`, `tracking`, `transcription`, `qr_decode`,
  `image_stats`, …).
- `ProviderDescriptor` — capability, input/output schema, media formats,
  languages, memory, latency estimates, backends, license, provenance,
  determinism, batch/streaming/cancellation/progress support.
- `CapabilityProvider: Send + Sync` — executes one capability; takes
  `ProviderInput` (`LumaImage` and friends), returns `ProviderOutput`
  (structured results, `BBox`), and receives a `RunContext` for
  cancellation and progress.
- `ProviderRegistry`/`CapabilityRegistry` — register/unregister, query by
  capability, best-provider selection. Implemented (12 tests).
- Model packs install providers at runtime after checksum-verified
  installation (`RFC-0011-Model-Pack-Format.md`); no network downloads.
- Reference providers prove the contract with pure-CPU implementations:
  `QrCodeProvider` and `ImageStatsProvider` are COMPLETE (15 tests).
- Backends are feature-gated; every provider either has a CPU fallback or
  registers an explicit "no compatible provider" state.

## Alternatives

- **Model-specific APIs per app**: fast to prototype, impossible to
  maintain across 8 apps; rejected.
- **One mandated runtime (ONNX-only)**: violates the vendor-neutral
  requirement; rejected.

## Trade-offs

The descriptor contract is richer than a minimal `run()` call — cost
accepted for scheduling decisions (memory/latency) and license/provenance
transparency. Provider output schema evolution is a versioning concern:
capability output versions are part of the descriptor.

## Security

Provider code is native and trusted (reference providers, or model packs
with verified checksums and provenance). Model packs are untrusted input
until validated (`RFC-0011`); providers never open network connections.

## Performance

CPU fallback for every capability is a release requirement; RunContext
progress enables progress UI; batch support declared per provider lets
apps amortize inference.

## Compatibility

Capability ids and output schemas are versioned (`COMPATIBILITY_POLICY.md`
§2); adding a capability is additive; changing an output schema requires a
major/minor bump per policy.

## Migration

None yet; no consumers besides the CLI. Applications begin consuming the
registry in Phase 4/6 (`ROADMAP.md`).

## Testing

- Trait contract tests (registry registration/selection, cancellation,
  progress) — 12 tests.
- Reference provider golden tests (QR decode of known fixtures; image
  statistics on deterministic images) — 15 tests.
- Model-pack install/validation tests — 24 tests.
- Future: per-capability conformance suites run against any provider.

## Open questions

- Whether capability output schemas get their own version suffix
  (deferred to first schema change).

## Final status

ACCEPTED. Provider model implemented; reference providers COMPLETE; all
ML-backed capabilities NOT_STARTED.


## source-loom-spec-docs-rfcs-rfc-0011-model-pack-format-md

Original path: `loom-spec/docs/rfcs/RFC-0011-Model-Pack-Format.md`

# RFC-0011 — Model-Pack Format

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-vision`

## Context

Loom must not require online model downloads. Users install models from
files. The root directive defines the required contents of a model pack:
manifest, model files, checksums, license information, provenance,
capability declarations, runtime requirements, preprocessing and
postprocessing definitions, test vectors, optional sample inputs, version
and compatibility range.

## Goals

- A local model-pack format installable from a file, verified locally.
- Validation strong enough that an invalid or tampered pack is rejected
  before any model file is used.
- No network access anywhere in the install/validate path.
- License and provenance transparency for every model.

## Non-goals

- A remote marketplace or account system.
- Auto-download of missing models.

## Proposed design

Mirrors `loom-vision-core/model_pack.rs` (implemented, 24 tests):

- A model pack is a directory containing `manifest.json` and the model
  files it references (ZIP distribution optional, unpacked to a store
  directory).
- Manifest fields: pack id/version, capability declarations
  (`CapabilityId` + input/output schema versions), model files with
  SHA-256 checksums, license text/identifier, provenance (source, training
  data summary, publication), runtime requirements (backend, memory,
  format), preprocessing/postprocessing definitions, test vectors with
  expected outputs, optional sample inputs, format version and
  compatibility range.
- Installation validation: parse manifest; verify every referenced file
  exists; verify checksums; reject path traversal (paths must stay inside
  the pack root); verify license field non-empty; record pack metadata in
  the provider registry.
- A pack's test vectors run at install time when practical, so a broken
  pack is rejected before use.
- Re-installation with a different checksum set is rejected or requires
  explicit user confirmation (no silent overwrite).

## Alternatives

- **Model downloads in-app**: violates offline-first and user-control
  principles; rejected.
- **Manifest-less model folders**: undetectable corruption and no license
  hygiene; rejected.

## Trade-offs

Checksum verification costs IO at install; acceptable (install is a
background job, `RFC-0008`). Running test vectors at install costs time but
catches incompatible formats early — worth it.

## Security

Checksums prevent tampering; path traversal rejection prevents pack-driven
file writes outside the store; license field enforcement prevents
unknowingly installing non-redistributable models (root directive §4.3);
the reader is a fuzz target (`IMPLEMENTATION_GUIDE.md` §5).

## Performance

Validation is streaming and memory-bounded; packs can be large, so
validation runs as a cancellable job.

## Compatibility

Pack format version and capability compatibility range are declared in the
manifest; a pack declaring an unsupported range is rejected with a clear
message (`COMPATIBILITY_POLICY.md` §2).

## Migration

Packs are forward-declared: readers ignore unknown optional manifest
fields with a warning; unknown required fields reject.

## Testing

- Validation tests: missing files, bad checksums, traversal paths,
  malformed manifests (24 tests exist).
- Install/remove round trips; duplicate/corrupt pack handling; version
  range rejection.
- Fuzz target for the manifest parser.

## Open questions

- ZIP vs directory distribution for the first shipped packs (directory
  store + optional ZIP wrapper; resolved at packaging phase).

## Final status

ACCEPTED. Format and validator implemented in `loom-vision-core`
(`model_pack.rs`, 24 tests).


## source-loom-spec-docs-rfcs-rfc-0013-color-management-md

Original path: `loom-spec/docs/rfcs/RFC-0013-Color-Management.md`

# RFC-0013 — Color Management

- Status: **ACCEPTED (normative design; sRGB first, ICC/BTO NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-color`, all applications

## Context

A professional creative suite must handle color correctly: ICC profiles,
display profiles, working spaces, conversion, linear-light processing,
HDR, wide color, rendering intents, soft proofing, image/video/print
differences, GPU precision, and deterministic reference tests
(`PRODUCT_SPEC.md` §2.5, root directive §10.6). `loom-color` exists with
basic types and conversions (8 tests).

## Goals

- One color foundation crate used by every application and the future
  renderer.
- Correct by construction: conversions are tested against reference
  vectors; no gamma confusion.
- Deterministic reference tests that run headless (no GPU dependency).
- An explicit path to ICC profile support and beyond-the-obvious
  (BTO) working spaces.

## Non-goals

- ICC/BTO support in the initial milestone.
- HDR tone-mapping pipelines in the initial milestone.

## Proposed design

- `loom-color` provides: color value types (RGB, RGBA, linear RGB, sRGB,
  HSV/HSL as UI helpers), working-space tagged values (never raw
  unlabeled floats), conversion functions between the tagged encodings,
  and a `ColorProfile` abstraction that starts as `sRGB` only.
- Pipeline principle: **linear-light processing** — compositing, blending,
  filtering, and rendering operate in linear light; sRGB is the exchange
  encoding. All UI surfaces and document content are sRGB-tagged in the
  initial milestone.
- Where a profile is unknown or untagged, default to sRGB with an
  explicit "assumed sRGB" marker in diagnostics (never silent guessing
  that changes output).
- ICC support (NOT_STARTED): `ColorProfile` extends to parsed ICC
  profiles, display-profile queries, working-space setup, rendering
  intents, and soft proofing. The architecture reserves the seams (tagged
  values, profile object, conversion API) so ICC lands without rewriting
  callers.
- BTO (beyond the obvious) and wide-gamut work spaces (NOT_STARTED) build
  on the same seams; HDR is a later milestone with its own RFC.

## Alternatives

- **Delegate all color to a C library (lcms2)**: mature and correct, but
  adds a native dependency; keep as a possible backend behind
  `ColorProfile` if ICC work proves large (decision at ICC milestone).
- **sRGB-only forever**: insufficient for professional Photo/Video;
  rejected as an endpoint, accepted as the initial milestone.

## Trade-offs

The initial sRGB-only pipeline is professionally limiting for Photo/Video
RAW and HDR workflows — documented as FUNCTIONAL_WITH_LIMITATIONS rather
than hidden. Deferring ICC keeps the first milestones small; the seams
above prevent rework.

## Security

Color data from untrusted files (ICC profiles embedded in images) is
parsed defensively; profile parsing is a fuzz target
(`IMPLEMENTATION_GUIDE.md` §5).

## Performance

Conversions must be SIMD-friendly and allocation-free in hot paths
(compositing); renderer work is linear-light with no per-pixel heap.

## Compatibility

Document content stores color as tagged values with the profile id;
packages written under sRGB remain valid when ICC lands (profile id
defaults to sRGB) (`FILE_FORMAT_FAMILY.md` §5).

## Migration

All existing content (none yet in apps) is sRGB; no migration required.

## Testing

- Reference-vector conversion tests (sRGB↔linear round trips, precision
  bounds) — 8 tests exist.
- Deterministic golden tests for compositing in linear light.
- Profile-parse fuzz tests (future, with ICC work).

## Open questions

- Whether ICC work uses lcms2 bindings or a Rust implementation
  (resolved at the ICC milestone; recorded in an ADR).

## Final status

ACCEPTED. sRGB/linear pipeline implemented in `loom-color`; ICC and BTO
NOT_STARTED.


## source-loom-spec-docs-rfcs-rfc-0015-visual-regression-system-md

Original path: `loom-spec/docs/rfcs/RFC-0015-Visual-Regression-System.md`

# RFC-0015 — Visual Regression System

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-design-bible`, `loom-bootstrap`, all applications

## Context

Visual QA must not consist of "the window launched"
(root directive §15). Loom needs deterministic pixel comparison for
components and full application windows, in CI and in Docker, without GPUs.

## Goals

- Deterministic screenshots: software-rendered via Slint with a custom
  platform (no window manager), captured in pinned Docker images.
- Golden baselines committed in `loom-design-bible`; perceptual diffing
  with documented tolerances; no automatic baseline approval.
- Coverage: component states, windows, empty states, menus, dialogs,
  errors, selection, hover, focus, drag/drop, zoom, themes (light/dark/
  high contrast), large fonts, RTL, reduced-motion final states.

## Non-goals

- Pixel fidelity verification of the GPU renderer (deferred to RFC-0004).
- Approving baseline changes without review.

## Proposed design

- **Capture**: a test harness renders Slint components/windows with the
  software renderer and a custom platform (`docs/adrs/ADR-0003-Headless-Screenshots.md`),
  writing PNG + metadata (component id, theme, locale, font config,
  renderer info, commit id) into an artifacts tree.
- **Baselines**: committed in `loom-design-bible` (screenshot directory),
  generated only in the pinned Docker visual image
  (`../loom-bootstrap/docker/Dockerfile.visual`) so fonts, locales, and
  renderer versions are identical everywhere.
- **Diff**: perceptual diff with documented tolerance per metric (per-pixel
  color distance thresholds, allowed diff ratios); output shows baseline,
  actual, diff, and metadata.
- **Review**: a changed baseline requires explicit human approval (or an
  ADR); the pipeline fails on unapproved diffs beyond tolerance.
- **Reduced motion**: motion-sensitive tests capture final states only and
  assert deterministic end states.

## Alternatives

- **Xvfb + screenshot tools (import/scrot)**: works for E2E but is
  slower and less deterministic (rendering timing); kept only for E2E
  input automation, not pixel regression.
- **GPU-rendered screenshots**: environment-dependent (driver, GPU);
  rejected for baselines.

## Trade-offs

Software-renderer screenshots don't prove GPU output; accepted — GPU
paths get targeted tests later. Determinism costs: fonts, locales, and
renderer must be pinned; that is what the Docker images provide.

## Security

Screenshots may contain user data; baselines and artifacts must come from
fixture content only, never real user documents. Artifacts are local;
nothing is uploaded.

## Performance

Screenshot runs are bounded (explicit sizes, batched); CI time budget is
tracked; capturing full windows runs in the visual image only.

## Compatibility

Baselines are tied to pinned Slint, fonts, and Docker image versions; any
change to those requires regenerating baselines with review
(`COMPATIBILITY_POLICY.md` §6).

## Migration

No baselines exist yet; the system is established with the first `loom-ui`
components.

## Testing

- Harness self-tests: two identical renders diff zero; injected one-pixel
  change diff above tolerance; metadata completeness.
- Per-component screenshot tests run in the visual image.

## Open questions

- Tolerance defaults per surface type (resolved at first baseline
  review; recorded in the design bible's `VISUAL_QA.md`).

## Final status

ACCEPTED. Harness and baselines NOT_STARTED (`FEATURE_MATRICES.md` §1,
`ROADMAP.md` Phase 2 tail).


## source-loom-spec-docs-rfcs-rfc-0018-autosave-and-recovery-md

Original path: `loom-spec/docs/rfcs/RFC-0018-Autosave-and-Recovery.md`

# RFC-0018 — Autosave and Recovery

- Status: **ACCEPTED (normative design; recovery browser NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-storage`, all applications

## Context

User work must survive crashes. The root directive requires transactional
writes, temporary-file safety, periodic autosave, an operation-journal
architecture, recovery after crashes, a recovery browser, explicit
recovered-document handling, storage limits, and privacy-preserving
diagnostics. No such subsystem exists yet in `loom-core`.

## Goals

- Transactional save semantics: a package on disk is always either the old
  or the new complete version, never partial.
- Periodic autosave with no visible interruption to editing.
- Crash recovery from autosave + operation journal, reconciled
  deterministically.
- A recovery browser (NOT_STARTED) for choosing among recovered states.

## Non-goals

- Version snapshots (future; tracked separately).
- Cloud backup of any kind.

## Proposed design

- **Transactional writes** (in `loom-storage`, partially implemented: path
  and transactional temp-file primitives, 7 tests): write to a temp file
  in the target directory, fsync, atomic rename over the destination, then
  fsync the directory. Never write in place.
- **Periodic autosave**: a `loom-jobs` background job serializes the
  document to a recovery snapshot at a configurable interval (default per
  app, e.g. 30 s) and after significant edits, throttled; the job yields
  to input and never blocks the UI thread.
- **Operation journal**: between autosaves, committed history transactions
  (`RFC-0007`) are appended to a journal in the package `recovery/`
  directory; journals are checksummed (`RFC-0006`) and bounded
  (size/rotation).
- **Recovery**: on open, the app detects autosave/journal presence; the
  newest valid revision wins; a diverging branch is preserved and offered
  in the recovery browser. Recovery opens read-only until the user
  explicitly saves.
- **Storage limits**: recovery snapshots are bounded (count + total size
  per project); eviction is documented and user-visible.
- **Diagnostics**: crash logs are local, readable, redactable, and contain
  no content payloads beyond what the user opts into
  (`PRODUCT_SPEC.md` §2.2).
- **Recovery browser (NOT_STARTED)**: a shared `loom-ui` surface listing
  recovered documents with timestamps and branch states.

## Alternatives

- **Save-in-place**: fast but crash-corrupting; rejected.
- **Full journal replay only**: unbounded journals; rejected in favor of
  periodic snapshots + bounded journal.

## Trade-offs

Atomic rename is not durable on all filesystems without directory fsync;
documented per-platform behavior and covered by tests on the CI
filesystem. Journaling duplicates content between snapshots; bounded by
rotation. Autosave IO is background by construction
(`RFC-0008-Async-Job-Framework.md`).

## Security

Recovery files are packages (checksums, path rules); recovery must never
overwrite the user's existing file without explicit save; temp files use
safe names and permissions.

## Performance

Autosave must never visibly interrupt editing (target: sub-frame UI
impact); snapshot serialization is a low-priority cancellable job;
journaling is batched.

## Compatibility

Recovery entries are versioned with the package format; a recovery entry
of an unknown version opens read-only with an explanatory error.

## Migration

No recovery files exist yet; format is new in this milestone.

## Testing

- Crash simulations: kill mid-save, mid-edit; assert recovery reconciles.
- Transactional write tests: failures at every step leave the old file
  intact.
- Journal rotation and storage-limit tests.
- Integration tests for the full create→edit→crash→recover→save cycle
  (`IMPLEMENTATION_GUIDE.md` §5).

## Open questions

- Autosave interval defaults per application (decided at each app's
  integration; recorded in app docs).

## Final status

ACCEPTED. Transactional primitives implemented in `loom-storage`;
autosave, journal, recovery browser NOT_STARTED.


## source-loom-vision-accessibility-md

Original path: `loom-vision/ACCESSIBILITY.md`

# Accessibility

Loom Vision is a library plus a CLI. Accessibility requirements therefore
apply to the CLI's human interface and to the library's API ergonomics.

## CLI (accessible by design)

- **Pure text I/O.** All output is plain text on stdout; errors go to
  stderr. Works with any screen reader, braille display, or terminal
  customisation; no colours, icons, or mouse are used or required.
- **Exit codes** communicate success (0) vs runtime error (1) vs usage
  error (2) — scriptable and assistive-technology friendly.
- **`help` command** documents every subcommand inline; no external
  documentation required to operate.
- **No flashing or animation** — there is none.

## Library ergonomics

- All public items have documentation comments (`missing_docs` is
  enforced); errors implement `Display` with human-readable messages.
- Long-running work is cancellable (`RunContext::cancel`) and reports
  progress, so UI layers built on Loom Vision can offer interruption and
  progress feedback to users who need it.

## Non-goals for 0.1.0

- A GUI (comes later with Slint-based Loom applications, where full
  keyboard navigation, focus, and reduced-motion will be required).


## source-loom-vision-architecture-md

Original path: `loom-vision/ARCHITECTURE.md`

# Architecture

## Overview

Loom Vision decouples *applications* from *models*. An application asks for
a capability; a registered provider supplies the computation. Providers can
be pure algorithms (CPU reference implementations) or model-backed runtimes
(ONNX, Candle, GPU) — the application never sees the difference.

```
                 +---------------------------+
                 |      Application code      |
                 +-------------+--------------+
                               |  ProviderInput / RunContext
                               v
                 +-------------+--------------+
                 |      CapabilityRegistry     |   routes by CapabilityId
                 +-------------+--------------+
                               |
                 +-------------+--------------+
                 |      ProviderRegistry       |   ordered Vec<Arc<dyn CapabilityProvider>>
                 +-------------+--------------+
                               |
          +--------------------+---------------------+
          |                    |                     |
   +------v-------+   +--------v--------+   +--------v--------+
   | QrCodeProvider|   |ImageStatsProvider|  | (future) Onnx...|
   |  (rqrr, CPU)  |   |   (pure CPU)     |   | model provider  |
   +---------------+   +-----------------+   +-----------------+
```

## Provider model

- `CapabilityProvider: Send + Sync` with `descriptor()` and
  `run(&self, input, ctx)`.
- `ProviderDescriptor` declares everything a caller needs to decide
  *whether* to use a provider: capability id, input types, output schema,
  media formats, memory, latency, backends, license, provenance,
  determinism, batch/streaming/cancellation/progress support.
- `ProviderInput` is raw data only: image buffers (gray/rgb/rgba), audio
  samples, or text. Providers own their decoding of that data.
- `RunContext` carries cancellation (`cancel`/`check_cancelled`) and
  progress (`set_progress`, clamped to 0..1). Providers must poll
  `check_cancelled` every few rows/iterations and return
  `VisionError::Cancelled` promptly. The context is owned by the caller of
  `run` and is intentionally not `Sync`.
- `ProviderOutput` is a tagged result (OcrResult, DetectionResult,
  QrDecoded, ImageStats, Generic, ...).

## Registry

- `ProviderRegistry` keeps an ordered `Vec<Arc<dyn CapabilityProvider>>`
  behind an `RwLock`. Lookups return owned `Arc` handles, which is what
  makes the registry sound without `unsafe`: the lock is released before a
  handle escapes, and a handle keeps its provider alive.
- **First registered wins**: `best_for` returns the earliest provider for a
  capability. Applications can shadow built-ins by registering a preferred
  provider first. `unregister` removes all providers of a capability.
- `CapabilityRegistry` adds run routing: `run_all` (collect every result)
  and `run_first_success` (first `Ok`, else last error).

## Model-pack lifecycle

```
pack dir (manifest.json + model files)
   │  parse_manifest     — JSON schema, format_version, required fields
   v
validate_pack(_with_limit)
   │  path-traversal guard (no absolute / .. / . components, no symlinks)
   │  per-file: exists → regular file → size matches → SHA-256 matches
   │  cumulative size ≤ max (default 2 GiB, archive-bomb guard)
   v
ModelPackSummary
   │
install_pack / install_pack_force
   │  destination <dest_dir>/<id>-<version>/ (sanitized components)
   │  refuses symlinked destination; refuses overwrite with different
   │  checksum unless forced
   v
installed pack  (re-validatable like any pack)
```

- Manifest serialization uses `serde_json`; `ModelFile.sha256` is a `[u8; 32]`
  serialized as a 64-char lowercase hex string.
- `FORMAT_VERSION` is 1; manifests with other versions are rejected.
- Pack ids/versions are sanitized to `[A-Za-z0-9._-]` before being used in
  directory names, so a malicious manifest cannot escape `dest_dir`.

## Reference providers

- `QrCodeProvider` (capability `qr_detection`): converts input to grayscale
  (BT.601 weights), then runs `rqrr`'s `prepare_from_bitmap`/`detect_grids`/
  `decode`. Deterministic, CPU only, MIT (rqrr is `(MIT OR Apache-2.0) AND
  ISC`). No QR found → `VisionError::Internal(NO_QR_CODE_MESSAGE)`.
- `ImageStatsProvider` (capability `image_stats`): mean luma, population
  standard deviation, Michelson contrast `(max-min)/(max+min)`. Pure
  arithmetic, CPU only.

## Design decisions recorded

See `docs/adrs/ADR-0001-qr-reference-provider.md` for the QR backend choice
(rqrr vs alternatives, license, fallback plan).


## source-loom-vision-building-md

Original path: `loom-vision/BUILDING.md`

# Building

## Requirements

- Rust stable 1.80 or newer (developed and verified with 1.97).
  `rust-version` is set per crate to 1.80.
- No system libraries: all dependencies are pure Rust (the `image` crate
  in the CLI has no C dependencies for the formats we use).

## Build

```sh
cargo build --workspace            # debug
cargo build --release              # release (binary: target/release/loom-vision)
cargo build -p loom-vision-core    # library only
```

## Minimum supported Rust version (MSRV)

1.80 (declared in `rust-version` in the workspace `Cargo.toml`).
CI/tooling should verify with `cargo +1.80 check --workspace`.

## Dependency pinning

All direct dependencies are pinned in `[workspace.dependencies]`
(semver-compatible ranges). The committed `Cargo.lock` pins exact versions
for reproducible builds. Audit table: [DEPENDENCIES.md](AGENTS.md#source-loom-vision-dependencies-md).


## source-loom-vision-changelog-md

Original path: `loom-vision/CHANGELOG.md`

# Changelog

## 0.1.0 — 2026-08-01

Initial release: the Loom Vision framework with CPU reference providers.

### Added

- Provider model: `CapabilityId` (17 capabilities), `ProviderDescriptor`,
  `ProviderInput` (image/audio/text), `ProviderOutput`, `RunContext`
  (cancellation + clamped progress), `LumaImage`.
- Grayscale conversion (ITU-R BT.601) with a cancellable row-by-row
  variant (`image_to_luma` / `image_to_luma_checked`).
- `ProviderRegistry` (ordered, thread-safe, owned `Arc` handles,
  first-registered-wins `best_for`, `unregister`) and `CapabilityRegistry`
  (`run_all`, `run_first_success`).
- Model packs: `ModelPackManifest` (serde, hex SHA-256), `parse_manifest`,
  `validate_pack[_with_limit]` (path-traversal/symlink/size/checksum
  checks, 2 GiB archive-bomb guard), `install_pack`/`install_pack_force`
  (versioned dirs, sanitized components, no-overwrite-without-force).
- Reference providers: `QrCodeProvider` (rqrr 0.10, rgba/rgb/gray inputs,
  deterministic) and `ImageStatsProvider` (mean luma, population std,
  Michelson contrast).
- `loom-vision` CLI: `inspect-pack`, `qr`, `stats`, `bench`, `help`.
- Fixture generation example (`gen_fixture`) and committed
  `crates/loom-vision-cli/fixtures/hello.png`.
- 72 tests (63 unit, 8 integration, 1 doc), all gates green
  (fmt, clippy `-D warnings`, test, release build).
- Documentation set incl. ADR-0001 (QR backend choice).

### Known limitations

- Only two capabilities have providers (QR detection, image statistics);
  everything else returns `ProviderUnavailable`-style behavior.
- No ONNX/Candle/GPU backends yet.
- `RunContext` is not `Sync` (caller-owned; cancellation is same-thread).
- CLI reads images through the `image` crate; `loom-vision-core` itself
  remains buffer-only.


## source-loom-vision-contributing-md

Original path: `loom-vision/CONTRIBUTING.md`

# Contributing

## Getting started

1. `cargo build --workspace` — must succeed before anything else.
2. Read [ARCHITECTURE.md](AGENTS.md#source-loom-vision-architecture-md) and
   [IMPLEMENTATION_GUIDE.md](AGENTS.md#source-loom-vision-implementation-guide-md).
3. Pick a task from [TASKS.md](AGENTS.md#source-loom-vision-tasks-md) and announce ownership.

## Pull request checklist

- Code compiles with `cargo build --workspace` (debug and release).
- `cargo fmt --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- `cargo test --workspace` passes; new tests accompany new behavior.
- Docs updated where behavior or the provider surface changed
  (ROADMAP/TASKS statuses, DEPENDENCIES for new deps).
- No `unsafe`, no network code, no hardcoded absolute paths, no secrets.
- Commit `Cargo.lock` when dependencies change.

## Standards

- Public API items documented (`///`); `missing_docs` is enforced.
- Errors are typed and `Display`-able; never `panic!` on user input.
- Reference providers implement real algorithms with real tests — no
  placeholders, no `assert!(true)`.
- Behavior changes to accepted contracts (trait shapes, manifest schema)
  go through an ADR first; `FORMAT_VERSION` bumps are breaking.

## Reporting issues

Include: reproduction steps, `cargo test` output, environment (OS, rustc
version). Security issues: see [SECURITY.md](AGENTS.md#source-loom-vision-security-md) — do not include
credentials or personal paths.


## source-loom-vision-dependencies-md

Original path: `loom-vision/DEPENDENCIES.md`

# Dependencies

All versions are the resolved versions from the committed `Cargo.lock`
(verified by `cargo build`/`cargo test` on rustc 1.97.1, 2026-08-01).

## Direct dependencies

| Crate | Version | License | Purpose | Scope |
|---|---|---|---|---|
| serde | 1.0.229 | MIT OR Apache-2.0 | Serialization framework (derive) | core |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Model-pack manifest parsing | core |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | SHA-256 checksums for model files | core |
| rqrr | 0.10.1 | (MIT OR Apache-2.0) AND ISC | QR detection/decode (default features off; uses `prepare_from_bitmap` on raw buffers) | core |
| image | 0.25.10 | MIT OR Apache-2.0 | PNG/JPEG/etc. loading for CLI commands | cli |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | QR *encoding* for tests and fixture generation | dev/example |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Temp directories in tests | dev |

## Notable transitive dependencies

`g2p` (1.2.2, MIT/Apache) and `lru` (0.16.4, MIT) — rqrr internals.
`image` pulls standard permissive decoders/encoders (png, jpeg/zune, gif,
webp, tiff, exr, rav1e/ravif for AVIF, qoi). Nothing in the transitive
tree is copyleft.

## Rules

- Add dependencies only via `[workspace.dependencies]` with a pinned range.
- Runtime deps of `loom-vision-core` must be pure Rust, `no_std`-friendly,
  network-free, and permissively licensed (audit via
  `cargo tree` + LICENSE_POLICY.md updates).
- `rqrr` is deliberately configured with `default-features = false` so the
  `image` crate stays out of `loom-vision-core`.
- Regenerate this table on any dependency change.


## source-loom-vision-implementation-guide-md

Original path: `loom-vision/IMPLEMENTATION_GUIDE.md`

# Implementation Guide — adding a new provider

## Steps

1. **Pick or add the capability id.**
   If the capability exists (see `CapabilityId`), use it. Otherwise add a
   variant to `CapabilityId` in
   `crates/loom-vision-core/src/provider.rs`, extend `as_str()`, and update
   the unit test `capability_id_string_forms_are_stable`.

2. **Implement the provider** in `crates/loom-vision-core/src/reference.rs`
   (or a new module if it needs model files). Pattern:

   ```rust
   pub struct MyProvider {
       descriptor: ProviderDescriptor,
   }

   impl MyProvider {
       pub fn new() -> Self {
           let mut d = ProviderDescriptor::new(CapabilityId::Ocr);
           d.name = "my-ocr".to_string();
           d.description = "...".to_string();
           d.input_types = vec![InputType::Image];
           d.output_schema = r#"{"type": "object"}"#.to_string();
           d.required_memory_bytes = ...;
           d.estimated_latency = Duration::from_millis(...);
           d.license = "...".to_string();          // SPDX of your implementation
           d.model_provenance = "none or description".to_string();
           // keep cancellation_support/progress_support honest
           Self { descriptor: d }
       }
   }
   impl Default for MyProvider { fn default() -> Self { Self::new() } }

   impl CapabilityProvider for MyProvider {
       fn descriptor(&self) -> &ProviderDescriptor { &self.descriptor }

       fn run(&self, input: &ProviderInput, ctx: &mut RunContext)
           -> Result<ProviderOutput, VisionError>
       {
           ctx.check_cancelled()?;
           let (w, h, channels, data, _fmt) = match input {
               ProviderInput::Image { width, height, channels, data, format } => {
                   (*width, *height, *channels, data.as_slice(), format.as_str())
               }
               _ => return Err(VisionError::UnsupportedInput),
           };
           let luma = image_to_luma_checked(w, h, channels, data, ctx)?; // checks every 8 rows
           // ... real algorithm, polling ctx.check_cancelled() in loops ...
           ctx.set_progress(1.0);
           Ok(ProviderOutput::Generic { message: String::new() })
       }
   }
   ```

3. **Expose it** from `crates/loom-vision-core/src/lib.rs` (module + re-export).

4. **Test it.**
   - Unit tests in the provider module: happy path with real input,
     unsupported-input → `UnsupportedInput`, cancel-before-run → `Cancelled`,
     descriptor sanity.
   - Cross-module flows in `tests/integration.rs` (register through
     `CapabilityRegistry`, run via `run_first_success`).

5. **CLI surface (optional).** Add a subcommand in
   `crates/loom-vision-cli/src/main.rs`; all output to stdout, errors to
   stderr, exit codes 0/1/2.

6. **Gates:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo build --release`.

## Provider contract (non-negotiable)

- `run` never performs I/O or network access; it returns
  `VisionError::ProviderUnavailable` if a required backend is missing.
- If the descriptor claims `cancellation_support`, poll
  `check_cancelled()` inside long loops.
- If the descriptor claims `deterministic`, identical inputs must give
  identical outputs.
- Descriptor fields must be truthful (license, provenance, memory,
  latency, backend list).


## source-loom-vision-license-policy-md

Original path: `loom-vision/LICENSE_POLICY.md`

# License policy

## Project license

All original Loom Vision code is dual-licensed under:

- MIT
- Apache-2.0

This matches the Loom suite policy. License files will be added to each
crate before first publication; the workspace manifests already declare
`license = "MIT OR Apache-2.0"`.

## Direct dependency licenses (verified at 0.1.0)

| Crate | Version | License | Notes |
|---|---|---|---|
| serde / serde_derive | 1.0.229 | MIT OR Apache-2.0 | derive feature |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | SHA-256 |
| rqrr | 0.10.1 | (MIT OR Apache-2.0) AND ISC | QR decode; dual components |
| image | 0.25.10 | MIT OR Apache-2.0 | CLI only |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | dev/example only |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | dev only |

All direct dependencies are permissively licensed; nothing forces a
copyleft license on the workspace. Transitive dependencies are audited via
`cargo deny`-style review before release (the full transitive tree is in
`cargo tree` output; the largest subtree is the `image` crate's optional
AVIF/EXR machinery, all permissive).

## Rules

- **Never add** a dependency whose license is incompatible with MIT OR
  Apache-2.0 without isolating it behind a feature flag and documenting it.
- rqrr contains ISC-licensed code components: ISC is a permissive license
  compatible with our dual licensing; see
  [ADR-0001](AGENTS.md#source-loom-vision-docs-adrs-adr-0001-qr-reference-provider-md).
- Model packs are user content: their `license` field is metadata only.
  Loom Vision ships no model files, so no model license questions apply to
  this repository itself.
- No proprietary assets (fonts, icons, sounds) are used.


## source-loom-vision-performance-md

Original path: `loom-vision/PERFORMANCE.md`

# Performance

## Benchmark harness

```sh
cargo run --release --bin loom-vision -- bench crates/loom-vision-cli/fixtures/hello.png
```

Runs the QR provider 20 times on a 232×232 image and prints min / median /
max wall-clock milliseconds, plus how many runs decoded. Release profile is
required for meaningful numbers (debug builds are 10–100× slower).

Reference measurements (2026-08-01, Apple M-series laptop, release build):
min ~16.6 ms, median ~17.0 ms, max ~17.5 ms per decode — dominated by
`rqrr` grid search, which scales with image area.

## Budgets and rules

- No synchronous file or media operations on any UI thread — Loom Vision
  itself is called from background jobs; `run` never performs I/O.
- Providers must poll `check_cancelled()` every few rows/iterations so
  cancellation feedback is immediate.
- Memory is bounded: QR decoding allocates one luma buffer plus rqrr's
  working set (~3× image bytes); model-pack validation streams files in
  64 KiB chunks (never loads a model into memory) and enforces a 2 GiB
  total unpacked limit.
- Regression rule: `bench` timings for the same fixture and machine must
  not regress more than 50% without a reviewed waiver; CI will re-measure
  on the reference machine.

## Where to look if it's slow

1. `image_to_luma_checked` — single pass, cache-friendly row loops.
2. `QrCodeProvider::run` — rqrr `detect_grids` dominates; downscale input
   before decode when the QR occupies a small fraction of the frame.
3. Model-pack validation — SHA-256 streaming dominates; this is bounded by
   disk bandwidth.


## source-loom-vision-roadmap-md

Original path: `loom-vision/ROADMAP.md`

# Roadmap

Status vocabulary: `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`,
`EXPERIMENTAL`, `SCAFFOLDED`, `NOT_STARTED`, `BLOCKED`.

## 0.1.0 (this release) — framework + CPU reference providers

| Item | Status |
|---|---|
| Provider traits, descriptor, input/output model, RunContext | COMPLETE |
| ProviderRegistry / CapabilityRegistry (thread-safe, Arc handles) | COMPLETE |
| Model-pack manifest parse + validate + install (checksums, traversal, archive guards) | COMPLETE |
| QrCodeProvider (rqrr, CPU) | COMPLETE |
| ImageStatsProvider (mean/std/Michelson contrast) | COMPLETE |
| CLI: inspect-pack, qr, stats, bench | COMPLETE |
| Offline-first (no network anywhere) | COMPLETE |

## 0.2.0 — broader capability coverage

| Item | Status |
|---|---|
| OcrProvider (reference: OCR on raw buffers, pure-Rust engine TBD) | NOT_STARTED |
| BarcodeProvider (1D barcodes, pure-Rust decoder) | NOT_STARTED |
| FaceDetectionProvider (CPU reference) | NOT_STARTED |
| Model-pack test-vector execution (run `test_vectors` against providers) | NOT_STARTED |
| `loom-vision` subcommand: `providers` (list registered/available) | NOT_STARTED |

## Backends (all NOT_STARTED)

| Backend | Status |
|---|---|
| ONNX Runtime integration | NOT_STARTED |
| Candle integration | NOT_STARTED |
| Vulkan compute | NOT_STARTED |
| CUDA / TensorRT / ROCm / OpenVINO / DirectML / CoreML | NOT_STARTED (optional external) |

## Capability areas with no implementation yet

Document analysis, object detection, segmentation, matting, pose, embeddings,
tracking, optical flow, speech recognition, audio analysis, image generation,
inpainting, super-resolution — all `NOT_STARTED`; capability ids exist so the
trait surface is stable, but no provider implements them. Applications must
handle `ProviderUnavailable` gracefully (per the Loom requirement that AI
capabilities are optional).

## Guiding constraints (never traded away)

Local-first, no telemetry, deterministic CPU fallback for every capability,
documented model packs with checksums, honest descriptors.


## source-loom-vision-security-md

Original path: `loom-vision/SECURITY.md`

# Security

Threat model: model packs and images are attacker-controlled inputs to
Loom Vision; the framework must never let such inputs escape their
directory, exhaust resources, or trigger network traffic.

## Guarantees

1. **No network.** There is no network code anywhere in the workspace.
   Providers and pack validation are purely local. Runtime behavior must
   not change if the machine is offline.

2. **Path traversal.** Manifest model paths must be relative and may only
   contain `Normal` path components — absolute paths, `.`, `..`, roots, and
   prefixes are rejected during validation. Pack `id`/`version` strings are
   sanitized to `[A-Za-z0-9._-]` (leading dots stripped) before they are
   used in destination directory names, so a hostile manifest cannot write
   outside `dest_dir`. Symlinked model files and symlinked destinations are
   refused.

3. **Checksums.** Every model file's size must match the manifest and its
   SHA-256 digest (streamed, 64 KiB chunks) must match. `install_pack`
   refuses to overwrite an existing pack with different checksums;
   `install_pack_force` is the explicit opt-out.

4. **Archive-bomb guard.** Validation takes a maximum total unpacked size
   (default 2 GiB via `DEFAULT_MAX_PACK_SIZE_BYTES`); packs whose declared
   sizes exceed it are rejected. Cumulative sizes are computed with checked
   arithmetic.

5. **No `unsafe`.** `#![forbid(unsafe_code)]` in both crates; the registry
   achieves thread-safe handle sharing with owned `Arc`s instead of
   borrowed references.

6. **Malformed input.** Image buffers are validated for channel count
   (1/3/4), exact length, and non-zero dimensions before any arithmetic.
   `RunContext` clamps progress to `[0, 1]`.

7. **Crash isolation by design.** Providers run behind the trait boundary;
   a provider crash cannot corrupt pack state because validation is
   side-effect free (only reads), and installs happen only after full
   validation.

## Out of scope (documented, not implemented)

- Plugin sandboxing / WASM isolation (future `loom-plugin-sdk`).
- Encrypted model files.
- Secure deletion.

## Review checklist

- New dependency → verify license (DEPENDENCIES.md), no network features,
  no `unsafe` in our usage.
- New path handling → run it against the traversal tests.
- New provider → keep it out of the pack-install path or validate first.


## source-loom-vision-tasks-md

Original path: `loom-vision/TASKS.md`

# Task ledger

Statuses: `DONE`, `IN_PROGRESS`, `NOT_STARTED`, `BLOCKED`.

## COMPLETE (0.1.0)

| ID | Task | Evidence |
|---|---|---|
| LV-001 | Workspace (resolver 2, edition 2021, rust-version 1.80, MIT OR Apache-2.0, pinned `[workspace.dependencies]`) | Cargo.toml |
| LV-002 | `VisionError` with Display/Error/From<io::Error> | `error.rs` + tests |
| LV-003 | `CapabilityId` (17 variants), `InputType`, `Backend`, `ProviderDescriptor` | `provider.rs` + tests |
| LV-004 | `ProviderInput`/`ProviderOutput`/`BBox`/`LumaImage` | `provider.rs` + tests |
| LV-005 | `RunContext` (AtomicBool + Cell<f32>, cancel/check/progress clamp) | `provider.rs` + tests |
| LV-006 | Grayscale conversion (BT.601) + cancellable row variant | `provider.rs` + tests |
| LV-007 | `ProviderRegistry` (Arc handles, first-wins `best_for`, `unregister`) | `registry.rs` + tests |
| LV-008 | `CapabilityRegistry` (`run_all`, `run_first_success`) | `registry.rs` + tests |
| LV-009 | `ModelPackManifest` serde model, hex sha256 (de)serialization | `model_pack.rs` + tests |
| LV-010 | `parse_manifest` (format_version, required fields) | `model_pack.rs` + tests |
| LV-011 | `validate_pack[_with_limit]` (traversal, symlink, size, SHA-256, 2 GiB guard) | `model_pack.rs` + tests |
| LV-012 | `install_pack` / `install_pack_force` (versioned dir, sanitize, no-overwrite-without-force) | `model_pack.rs` + tests |
| LV-013 | `QrCodeProvider` (rqrr, rgba/rgb/gray, cancellation, progress) | `reference.rs` + tests |
| LV-014 | `ImageStatsProvider` (mean/std/contrast) | `reference.rs` + tests |
| LV-015 | CLI `inspect-pack`, `qr`, `stats`, `bench` + help, exit codes | `main.rs`, verified runs |
| LV-016 | QR fixture generation example + committed `fixtures/hello.png` | `examples/gen_fixture.rs` |
| LV-017 | Integration tests (registry flows, pack lifecycle, tamper) | `tests/integration.rs` |
| LV-018 | Docs (README, AGENTS, ARCHITECTURE, IMPLEMENTATION_GUIDE, BUILDING, TESTING, VISUAL_QA, PERFORMANCE, SECURITY, ACCESSIBILITY, ROADMAP, TASKS, CONTRIBUTING, LICENSE_POLICY, DEPENDENCIES, CHANGELOG, ADR-0001) | docs/ |
| LV-019 | Quality gates green: fmt, clippy -D warnings, 72 tests, release build | verification run |

## NEXT (0.2.0)

| ID | Task | Status |
|---|---|---|
| LV-020 | Reference OcrProvider (pure-Rust engine evaluation first) | NOT_STARTED |
| LV-021 | Reference BarcodeProvider | NOT_STARTED |
| LV-022 | Execute model-pack `test_vectors` | NOT_STARTED |
| LV-023 | `providers` subcommand listing available providers | NOT_STARTED |
| LV-024 | ONNX Runtime backend behind a feature flag | NOT_STARTED |
| LV-025 | Candle backend behind a feature flag | NOT_STARTED |
| LV-026 | Fuzz targets for manifest parser and image-buffer validation | NOT_STARTED |

## Rules

- A task is `DONE` only with the listed evidence (real test output, not
  prose).
- Unimplemented work stays visible here and in ROADMAP.md.


## source-loom-vision-testing-md

Original path: `loom-vision/TESTING.md`

# Testing

## Test layout

- **Unit tests** (`#[cfg(test)]` modules, next to the code):
  - `provider.rs` — luma conversion (BT.601 weights, gray identity, rgba,
    channel/length/zero-dimension rejection), `RunContext`
    cancellation/progress clamping.
  - `registry.rs` — ordering, `best_for` first-wins, `providers_for`
    filtering, unregister, `run_all`/`run_first_success` routing,
    cancellation propagation.
  - `model_pack.rs` — hex round-trip, manifest parsing (bad JSON, wrong
    format_version, empty id, zero models), validation (checksum mismatch,
    missing file, size mismatch, `../` traversal, absolute path, `./`
    components, archive-bomb limit, symlink rejection), install (copy,
    no-op on same checksum, refusal on different checksum, force overwrite,
    component sanitization, symlinked destination).
  - `reference.rs` — real QR round-trips generated at runtime with the
    `qrcode` crate (rgba/rgb/gray inputs), no-QR error, cancellation,
    unsupported input; image stats on known 2x2 images (mixed → mean 127.5 /
    std 127.5 / contrast 1.0; uniform → std 0, contrast 0).
- **Integration tests** (`crates/loom-vision-core/tests/integration.rs`):
  provider-through-registry flows, pack install → revalidate lifecycle,
  tamper detection, registry ordering with fake providers.

## Gates (mandatory)

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

## Determinism

All reference providers are deterministic; tests never depend on wall-clock
timing. QR test images are rendered at runtime (no committed binary
fixtures are needed for the tests; the CLI fixture PNG is generated by
`cargo run --example gen_fixture`).

## Coverage expectations

New providers must add: happy-path unit test with real input, unsupported
input, cancellation, and a registry-level integration test.


## source-loom-vision-visual-qa-md

Original path: `loom-vision/VISUAL_QA.md`

# Visual QA

Loom Vision has no GUI. Its "visual" outputs are the deterministic,
machine-checkable results of the CLI, which double as visual-regression
evidence for the reference providers:

1. **Fixture generation** (committed, reproducible):

   ```sh
   cargo run --example gen_fixture -- "Hello, Loom!" crates/loom-vision-cli/fixtures/hello.png
   ```

2. **Decode check** — the QR provider must reproduce the exact payload:

   ```sh
   cargo run --bin loom-vision -- qr crates/loom-vision-cli/fixtures/hello.png
   # expected stdout: Hello, Loom!  (exit 0)
   ```

3. **Stats sanity** — a black-on-white QR is mostly white with dark
   modules: mean luma above 100, high std, Michelson contrast 1.00.

   ```sh
   cargo run --bin loom-vision -- stats crates/loom-vision-cli/fixtures/hello.png
   ```

4. **Error visuals** — corrupted inputs must produce non-zero exits with
   stderr messages: `qr /nonexistent.png` → exit 1; tampered pack →
   `inspect-pack` prints the failing check.

Acceptance: steps 1–3 produce the documented outputs on a clean checkout,
and the fixture PNG round-trips through `qr` byte-for-byte in payload text.
Any change to the QR or stats providers must re-run this checklist and keep
the fixture and expected outputs in sync.


## source-loom-vision-docs-adrs-adr-0001-qr-reference-provider-md

Original path: `loom-vision/docs/adrs/ADR-0001-qr-reference-provider.md`

# ADR-0001: rqrr as the reference QR provider

Status: Accepted
Date: 2026-08-01
Scope: `loom-vision-core` reference provider for `CapabilityId::QrDetection`

## Context

Loom Vision needs a working, deterministic QR decoder that ships with the
framework: no model files, no GPU, no network. The candidates are pure-Rust
QR *decoders* (not encoders) usable from a raw grayscale buffer.

## Candidates considered

| Crate | Notes | Verdict |
|---|---|---|
| **rqrr 0.10.1** | Pure Rust decode; `prepare_from_bitmap(w, h, FnMut) -> bool` works directly on raw buffers; no `unsafe` in the path we use; actively maintained (0.10.1, June 2026); used as reference by `qrcode-decode` | **Selected** |
| `quircs` | Pure Rust; decoder-oriented; smaller ecosystem, slower detection | Not chosen: rqrr is more widely used and actively maintained |
| `zxing-cpp` | Bindings to C++; excellent quality | Not chosen: C++ FFI (`unsafe` boundary), heavier build |
| Home-grown | Decoder is a large, error-prone algorithm | Not chosen: would trade correctness for control |

## Decision

Use `rqrr = { version = "0.10", default-features = false }` in
`loom-vision-core`, with `PreparedImage::prepare_from_bitmap` fed from our
own BT.601 grayscale conversion. Keeping `default-features = false` keeps
the `image` crate out of `loom-vision-core` (the CLI adds `image` on its
own for file loading).

## License

rqrr is `(MIT OR Apache-2.0) AND ISC`. ISC is a permissive license fully
compatible with the project's `MIT OR Apache-2.0` dual licensing (both
components permit the use and relicensing we need). Recorded in
LICENSE_POLICY.md.

## Trade-offs

- rqrr's grid search is the dominant cost (≈17 ms on a 232×232 image in
  release builds); callers can downscale before decoding.
- rqrr decodes standard QR codes; Micro QR / other symbologies are out of
  scope (a future `Barcode` provider can add them).
- Deterministic: identical inputs produce identical outputs (no floating
  point in detection decisions at our call sites; verified by tests).

## Fallback plan

If rqrr becomes unmaintained or a license problem appears:
1. `quircs` is the drop-in pure-Rust alternative (same
   `prepare_from_bitmap`-style raw API).
2. The provider boundary isolates the swap: only
   `reference.rs::QrCodeProvider::run` changes; descriptors, CLI, and
   tests remain valid.
3. Vendor-on-crisis is documented as last resort (per Loom dependency
   policy, with an ADR update).

## Consequences

- `QrCodeProvider` can ship with zero model files and zero network use.
- Test strategy is fully self-contained: tests encode QR codes at runtime
  with `qrcode` (dev-dependency) and decode them back.
- The `(MIT OR Apache-2.0) AND ISC` license string must appear in
  NOTICE files at packaging time.
