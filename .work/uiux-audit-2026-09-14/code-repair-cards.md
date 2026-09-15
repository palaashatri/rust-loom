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
