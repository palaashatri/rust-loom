# Loom — Current Truth

This is the live product ledger and repair queue. `AGENTS.MD` defines the rules; `loom-bootstrap/contracts/workflow.toml` records the work gate. Updated 2026-09-24 from the code audit, fresh UI/UX inspection, owner-authorized P0/P1 repair run, native Sheets self-audit, and Sheets acceptance follow-up. Finding text stays here so a future repair can be checked against the original failure.

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

**How to use a card:** follow the numbered steps in `AGENTS.MD` under “Start here.” Normally select one card, reproduce it, make a small repair, run its concrete check, and record evidence under that same card. The owner overrides authorize this dated P0/P1 run and the active Sheets completion run. States: OPEN, NEEDS_REVIEW, FIXED. FIXED does not mean the whole application is ACCEPTED. P2 and GOV cards remain OPEN unless their own record says otherwise.

## Current product state

Loom is a local-first Rust + Slint functional alpha. It has useful domain engines and real editing features. The audit found reproducible data loss, corrupt or incomplete exports, broken recovery, and misleading UI states. The owner-authorized repair run has fixed the recorded P1 code cards and UI-14. UI-25 compact recovery layout and action-matching guidance are fixed and tested in code; real playback/encode acceptance remains open because no media backend or sample media is installed here. Sheets has native evidence for saved/dirty titles, chooser focus, keyboard selection/create/cancel, the named export control, and visible formula/save-error/cancel feedback with one-time Orca announcements. The UI-01 menu code and app tests pass, but the corrected live menu has not been inspected: `cinnamon-screensaver-command -q` reported inactive while the native screenshot showed the desktop lock/saver overlay. The Sheets evaluator, array spill, and XLSX-name collision regressions are fixed and tested; LibreOffice opened one exported fixture. Sheets' full acceptance gate remains blocked on native visual review, wider interoperability, complete visual and accessibility coverage, non-blocking edit calculation, million-cell scrolling, and a reviewed memory budget. No application is certified by this audit as a professional replacement for mature creative software. The old 38/100 score and claims of complete Sheets acceptance are superseded; there is no defensible fresh numerical readiness score.

Quality and permission to work are different. The owner override permits the active Sheets completion work while the remaining applications stay locked until their turn. This is the single live application status table:

| Order | Application | Product status | Work status | Current blocking evidence |
|---:|---|---|---|---|
| 1 | Sheets | ACCEPTANCE_BLOCKED | IN_PROGRESS | UI-01 native visual review, full visual/accessibility coverage, broader interoperability, non-blocking edit calculation, native million-cell scroll, and reviewed app memory budget remain |
| 2 | Writer | ACCEPTANCE_BLOCKED | LOCKED | P1 code/UI repairs landed; CODE-17 and visual/manual checks remain |
| 3 | Present | ACCEPTANCE_BLOCKED | LOCKED | CODE-04 repaired; CODE-16/19 and visual checks remain |
| 4 | Photo | ACCEPTANCE_BLOCKED | LOCKED | CODE-04/UI-16 repaired; CODE-14, UI-14/18 and visual checks remain |
| 5 | Motion | ACCEPTANCE_BLOCKED | LOCKED | UI-17 still-frame parity verified; UI-14/18 and full video-render acceptance remain |
| 6 | Video | ACCEPTANCE_BLOCKED | LOCKED | UI-25 repaired in code; UI-19/20 and real-media checks remain |
| 7 | Studio | ACCEPTANCE_BLOCKED | LOCKED | CODE-10 repaired in code; audio and visual acceptance checks remain |
| 8 | Encode | ACCEPTANCE_BLOCKED | LOCKED | CODE-09/UI-23 repaired in code; filesystem/media checks remain |

The previous ledger listed Present/Photo both LOCKED and ACCEPTED and said Sheets had no known serious defects. Those statements are withdrawn. Historical test counts and agent-reviewed screenshots do not override the open findings below. The pre-update ledger is retained as an ignored audit backup, not a competing authority.

## Recorded audit and evidence

The first audit was a code/reliability audit; it explicitly did **not** certify UI/UX. The follow-up adds fresh rendered screenshots from all eight apps and isolated native Linux interactions in Sheets and Writer. The observations are durable in CODE-01 through CODE-19 and UI-01 through UI-27 below, including reproduction instructions so they remain usable if `.work/` is removed. UI cards distinguish reproduced failures from source-confirmed limitations and visual design recommendations.

- Original detailed code report: [.work/audit-2026-09-14/AUDIT.md](.work/audit-2026-09-14/AUDIT.md).
- Fresh visual report and screenshots: [.work/uiux-audit-2026-09-14/AUDIT.md](.work/uiux-audit-2026-09-14/AUDIT.md). Build/capture commands and limitations are recorded with that report.
- Portable audit evidence bundle, including the code and UI/UX reports, historical audit evidence, the 2026-09-22 Sheets acceptance report/UI-14 evidence, the expanded 2026-09-23 native screenshot/interaction report and screenshots (including UI-06 status/error captures), the UI-01 renderer preview, UI-17/UI-25 repair evidence, and the 2026-09-24 Sheets acceptance report, full workspace/test/lint logs, million-cell performance logs, formula benchmark rerun, performance instructions, renderer screenshot, and LibreOffice fixture: [loom-bootstrap/audit-evidence-2026-09-14-and-22.zip](loom-bootstrap/audit-evidence-2026-09-14-and-22.zip) (SHA-256 `f530ab1c247bf6eb5e41f6c7eb459059cf31ff5eecc452a65f64e7195cca44f3`).
- Native Sheets self-audit: [.work/sheets-acceptance-2026-09-23/REPORT.md](.work/sheets-acceptance-2026-09-23/REPORT.md) records live `gnome-screenshot` captures plus native keyboard, AT-SPI, and Orca checks at 1024×720. UI-06 status/error captures are also checked into `loom-sheets/docs/qa-native/`. The 1440×900 native capture remains unavailable on this 1366×768 desktop; other themes/viewports use existing renderer evidence. The 2026-09-24 follow-up confirms app tests/build and one LibreOffice fixture; the post-fix native menu visual check is still pending because a fresh native capture showed the desktop lock/saver overlay even though `cinnamon-screensaver-command -q` reported inactive: [tracked report](loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md).
- Audit basis: commit `8fce782` plus the existing uncommitted Sheets implementation. That sentence describes the historical audit only; the owner-authorized repair commits listed below subsequently changed application behavior.
- Verified existing tests in the code audit: shared core 123, Sheets 98, Writer 77, Present 49, Photo 49 — **396 passing tests**. The four source/governance audits also passed before the documentation update. Three new focused recovery tests failed as intended, demonstrating CODE-01/02/18. Passing existing tests did not prevent these defects.
- Plugin and encode probes used controlled adapters, not real Wasmtime/codec runs. Source traces are labeled separately from executable probes. The original image-recovery probe tests payload transport; CODE-11 requires a real decoded-image regression too.
- Visual and accessibility evidence covers only the named states in the reports. The new run confirms selected-template Orca announcements, the focused chooser/grid groups, and a named compact export control. It does not establish complete screen-reader compliance, every keyboard command, every scale/direction, all dialog outcomes, or cross-platform acceptance. Uncaptured or untested states remain unknown.

**P0 findings:** none were recorded in the audit. All recorded P1 code cards are fixed in code. UI-14 is fixed with an app-level keyboard regression check. UI-25 compact recovery layout and recovery guidance are fixed in code; real playback/encode acceptance remains open because the required media tools and fixture are absent. UI-17 is independently checked as a still-frame preview/export path.

- Repair milestones pushed: `68df596`, `1a51c6d`, `29cb024`, `68b2b6b`, `9e15c55`, `1540a20`, `458e9ec`, `d58202a`, and Motion still-frame parity with independent renderer evidence.
- Documentation handoff checks: 18 focused governance tests pass; the governance, asset, and UI-foundation source audits pass. The current code-structure audit reports six legacy byte-ratchet limits in locked apps: Encode, Motion, Video app/core, Writer core, and Photo. No Sheets source is listed and no ceiling was raised. Extraction is a separate follow-up. The source audit recognizes pre-existing accepted baseline files; it does not supply new human visual approval. All 49 accepted screenshot paths/hashes and explicit source-file references were checked. Sixteen audited product source files still match the original code audit hashes. Verification details are saved beside the visual report.

## Existing capability inventory — preserve these while repairing

These capabilities describe the current implementation and historical work, not blanket acceptance. Do not remove working features to make a defect disappear. Old performance and cross-platform numbers are historical, not rerun measurements.

### Sheets

The implementation includes sparse multi-sheet workbooks, formulas and cross-sheet ranges, absolute references, lazy conditionals, lookup/text/aggregate/date/financial functions, dynamic-array spills, formula-backed summaries, cell style/formatting, freeze and row/column sizing, charts, anchored shapes/images, tab operations, templates, native packages, CSV and XLSX paths, a command palette, undo, and recovery. The P1 repair run covers lossless text/recovery, bounded workbook history, valid rich XLSX chart output, embedded recovery images, and live imported formulas. The 2026-09-24 follow-up fixes XLSX sheet-name collisions and array-spill orientation, removes formula-reader overhead when no array functions exist, and reuses calculated values during view-only updates. Current checks: core 109 passed; app 117 passed plus one cache test; CI-equivalent Sheets workspace tests 227 passed; the latest optimized 10,000-formula run calculated in 88.5 ms on this low-end local host; one XLSX fixture opened and recalculated by LibreOffice. The opt-in release test projected 60 viewports across 1,000,000 unique pseudorandom cells in a 2,048×2,048 sheet at 0.17 ms p95 and 0.26 ms maximum; its test process peaked at 90,308 KiB RSS with no swap. This is CPU projection and test-process memory, not native frame rate or whole-app memory. This is not complete acceptance. UI-02/04/08 full visual evidence, the inaccessible native menu review, complete visual coverage, broader accessibility, non-blocking workbook cloning/calculation/recovery writes, native million-cell scrolling, numeric app memory budget, and wider interoperability remain. The active Sheets sources pass the byte ceiling. The latest repo-wide code-structure audit still fails on six legacy byte limits in locked apps—Encode, Motion, Video app/core, Writer core, and Photo; no Sheets source is listed. Single-series charts and cached PivotTable import remain boundaries; unsupported OOXML must be disclosed. Do not label these boundaries as proof that all imports are safe.

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

Asset provenance and commercial redistribution rules remain in `AGENTS.MD` and `loom-bootstrap/contracts/assets.toml`. Fresh screenshots are evidence generated by this project, not imported product artwork. No third-party assets were added for this audit.

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

**Verification:** The non-macOS accessibility-tree test finds File, Edit, View, and Table in the application window. Projection/dispatch and keyboard-navigation tests pass. The latest CI-equivalent Sheets workspace suite passes 227 tests with Slint debug metadata enabled; the native build result is in the [tracked follow-up report](loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md). A post-fix live screenshot is still unavailable: although `cinnamon-screensaver-command -q` reported inactive, the native screenshot captured the desktop lock/saver overlay instead of the app. The invalid capture was deleted, and no screen-lock setting was changed. Repeat the native visual check when the desktop is accessible.

**Evidence:** Source in `loom-sheets/crates/loom-sheets-app/ui/app.slint`, `src/local_menu.rs`, `src/local_menu_tests.rs`, and `src/main.rs`. Tests: `non_macos_window_exposes_a_local_application_menu_bar` and `local_application_menu_supports_keyboard_navigation_and_activation`. The current renderer preview is `loom-sheets/docs/qa-renderer/ui01-local-menu-current-1024-linux.png`; it is not native evidence. The before-fix live Edit capture is `loom-sheets/docs/qa-native/ui01-edit-menu-open-before-fix-linux.png`. Detailed current commands and remaining native check are in [the tracked report](loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md) and the portable audit archive.

**Remaining check:** After unlocking the Linux desktop, capture the real application window with the native screenshot tool. Confirm the menu row is visible at 1024, 1280, and 1440 px, open each menu, and use only the pointer to create, open, and save a test workbook. Confirm keyboard access with Alt+F/E/V/T/H and Escape. Check macOS separately to confirm its native global menu remains available. CODE-04 must protect the file transitions.
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

**P2 · Shared design contract · OPEN · Source-confirmed design debt.** `AGENTS.MD` requires readable complete action labels, but `desktop-ui.toml` permits 10 px ellipsized toolbar captions. A small model can obey one rule and violate the other.

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

**Verification:** `xlsx_export_keeps_sanitized_sheet_names_unique_and_rewrites_references` passes. LibreOffice Calc 24.2.7.2 opened and converted the fixture; it kept `AB`, `AB (2)`, and the shortened long tab, recalculated the cross-sheet formula to `80`, and kept the quoted text `A/B!B2` as text. Exact commands and results are in [the tracked report](loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md) and the portable audit bundle. Microsoft Excel was not tested.

### CODE-21 — Put spilled array results in the right cells

**P1 · Sheets · FIXED.** `SEQUENCE(2,3)` means two rows and three columns. If the app swaps those numbers, the visible grid gets wrong values, and formulas that read spill cells can show stale results.

**Reproduce:** Put `=D2+1` in A1 and `=SEQUENCE(2,3)` in B1. Before repair, the spill used the wrong width/height and A1 did not update after D2 was filled.

**Repair result (2026-09-24):** Spill placement now uses `rows` for row movement and `columns` for column movement, keeps values in row-major order, and reruns formulas whose earlier reads changed. It checks all target cells before writing any value, so a blocked spill leaves no partial cells behind.

**Verification:** `sequence_spills_rows_and_columns_and_recalculates_earlier_readers` and `blocked_sequence_spill_does_not_leave_partial_values` pass. Core suite: 109 passed.

### CODE-22 — Reuse calculated values while the user navigates

**P2 · Sheets · FIXED for view-only updates.** Clicking a cell, scrolling, or resizing does not change formulas. Recalculate only when the workbook changes or the active tab changes; otherwise navigation needlessly repeats the full workbook calculation.

**Repair result (2026-09-24):** The app keeps the active sheet's calculated values and reuses them for selection, scroll, and resize. A cell/workbook change refreshes the cache, and switching to a different active tab forces fresh values. Values are shared by reference so reuse does not copy the whole result map.

**Verification:** The evaluation-cache regression checks same-tab reuse, tab switching, and refresh after an edit. `rename_rewrites_qualifiers_and_rejects_collisions` verifies the cross-sheet value remains `25` after a real tab rename/switch. App suite: 116 passed plus the cache test.

**Remaining performance gate:** Formula evaluation after a committed edit still runs synchronously. See PERF-01; this card does not prove the 16.7 ms input-feedback budget, large-workbook scrolling, or peak-memory limit.

### PERF-01 — Keep large workbook work from freezing the window

**P2 · Sheets performance · NEEDS_REVIEW.** A fast calculation benchmark is only one part of performance. Users also need immediate feedback while editing, smooth scrolling, and a known memory limit.

**What is measured:** The latest optimized 10,000-chained-formula test passed at 88.5 ms on this device's Intel Core i3-2350M (2 cores, 7.7 GiB RAM). The opt-in release test fills a 2,048×2,048 sheet with 1,000,000 unique pseudorandom cells and projects 60 viewports, including both workbook corners, at 0.17 ms p95 and 0.26 ms maximum. `/usr/bin/time -v` measured 90,308 KiB peak RSS and no swap for the test process. This measures CPU-side projection and the test harness, not native rendering, frame presentation, or whole-app memory. A committed edit still clones workbook tabs, recalculates, and serializes/fsyncs a recovery snapshot synchronously. No real-window run has measured input latency or million-cell frame rate, and Sheets has no reviewed numeric app memory budget.

**For a small coding model:** (1) Run both release commands in `loom-sheets/PERFORMANCE.md`; do not change the 10,000-formula or one-million-cell workloads. (2) Time workbook copying, formula calculation, recovery-package creation, and recovery-journal writes separately. (3) Keep all four off the UI thread; moving only formula calculation is not enough. Use one background worker with at most one pending newest workbook, so fast typing cannot create an unlimited queue. (4) Give every edit a revision number; show a finished result only if it belongs to the newest revision and active tab. Keep old formula results marked “Calculating…” until then, without erasing a newer formula draft. (5) Measure ordinary Save/Open response too; do not call file operations non-blocking until tested in the live app. (6) Measure native scrolling and peak app memory on the same million-cell workbook, then get an owner-reviewed numeric memory limit.

**Done when:** Formula work stays below the mainstream-profile budget; each input shows feedback within 16.7 ms; no calculation or file operation blocks the UI thread; one million randomly placed cells scroll at 60 fps; and peak RSS is below a reviewed numeric limit. Record machine, exact workbook, commands, measurements, and any remaining gap in `loom-sheets/PERFORMANCE.md`. Current gate details: [the tracked follow-up report](loom-sheets/docs/qa-reports/2026-09-24-acceptance-follow-up.md).
