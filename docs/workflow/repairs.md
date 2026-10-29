# Repair queue and original findings

Scheduling comes from [current.md](current.md); the current owner request narrows the permitted work. Dated pending items below retain their original scope and require a fresh reproduction before repair.

## Repair card index

Full card text (original finding, steps, evidence) lives in [docs/archive/AGENTS-ARCHIVE.md](../archive/AGENTS-ARCHIVE.md). Read only the card you work on. Update status here and in the archive card together.

- **CODE-01** Keep edits after reopening a saved document — FIXED
- **CODE-02** Publish recovery checkpoints without destroying the last good copy — FIXED
- **CODE-03** Save and load Sheets text exactly — FIXED
- **CODE-04** Ask before throwing away unsaved work — FIXED
- **CODE-05** Export every Writer page — FIXED
- **CODE-06** Write valid XLSX drawing and chart XML — FIXED
- **CODE-07** Stop undo history from copying itself — FIXED
- **CODE-08** Keep plugin writes inside the allowed folder — FIXED
- **CODE-09** Respect Encode's no-overwrite choice at the final write — FIXED
- **CODE-10** Preserve audio precision in Studio projects — FIXED
- **CODE-11** Recover Sheets images together with their cells — FIXED
- **CODE-12** Keep imported shared formulas live — FIXED
- **CODE-13** Turn on Writer recovery when opening a file at launch — FIXED
- **CODE-14** Give every Photo layer a unique ID — OPEN
- **CODE-15** Enforce plugin timeouts while sending input — OPEN
- **CODE-16** Give every Present slide a unique ID — FIXED
- **CODE-17** Move Writer comments with the text they describe — FIXED (2026-10-04, anchors rebase from the edit position; a paste over a selection falls back to a text diff)
- **CODE-18** Do not consume a storage sequence when append fails — FIXED (2026-10-28; storage-journal open, write, rollback, and reopen recovery tests pass)
- **CODE-19** Undo Present transitions with the rest of the slide — FIXED
- **UI-01** Give every desktop a visible application menu — NEEDS_REVIEW
- **UI-28** Fit the XLSX loss warning to its message — FIXED
- **UI-29** Align and size desktop menu popup items — FIXED
- **UI-30** Name the current workbook in the dirty-close prompt — FIXED
- **UI-31** Make the Checklist preview match the workbook it creates — FIXED
- **UI-32** Remove the redundant worksheet heading above the grid — FIXED
- **UI-33** Meet the button-label contrast floor in every theme — FIXED
- **UI-34** Keep anchored shape labels readable on every fill — FIXED
- **UI-35** Give every Sheets icon action a meaningful accessible name — NEEDS_REVIEW
- **UI-36** Expose chart data to keyboard and screen-reader users — NEEDS_REVIEW
- **UI-37** Make anchored objects operable without a pointer — OPEN
- **UI-38** Apply text scaling throughout Sheets — FIXED
- **UI-39** Keep Sheets inspector property labels readable — FIXED
- **UI-42** Let the responsive policy account for text scale — FIXED
- **UI-40** Preserve chart bar proportions at 2× text scale — FIXED
- **UI-41** Remove or implement the chart resize action — FIXED
- **UI-02** Stop opening a mostly empty Sheets inspector by default — FIXED
- **UI-03** Reflow the template chooser and show truthful Recents — FIXED
- **UI-04** Use honest starter data and a clear empty-document path — FIXED
- **UI-05** Make Sheets header cells readable in high contrast — FIXED
- **UI-06** Render the status and errors the controller produces — FIXED
- **UI-07** Clear edited fields when their document value changes — FIXED
- **UI-08** Make chart ranges and comparisons understandable — FIXED
- **UI-09** Distinguish a workbook from one sheet in its commands — FIXED
- **UI-10** Remove the contradictory permission to truncate action captions — OPEN
- **UI-11** Keep product help out of the user's document — FIXED (2026-10-04, Writer opens blank)
- **UI-12** Make each Writer template create what its card says — OPEN (2026-10-27 comparative audit: Blank Black creates a white page; see W-01 below)
- **UI-13** Make the table command honest about its editing model — FIXED
- **UI-14** Keep formatting and comments reachable in compact windows — FIXED
- **UI-15** Put Present's navigator in the expected reading position — FIXED
- **UI-16** Fit Photo's whole image inside its viewport — FIXED
- **UI-17** Make Motion's preview represent the exported frame — FIXED
- **UI-18** Let users type precise transform values — OPEN
- **UI-19** Distinguish Video timeline zoom from scrolling — OPEN
- **UI-20** Keep clip and region captions readable on their fills — OPEN
- **UI-21** Give Studio one transport control for each action — OPEN
- **UI-22** Show Studio users what is inside an audio region — OPEN
- **UI-23** Make Encode's empty queue and readiness truthful — FIXED
- **UI-24** Make Encode's job settings fit the editing task — OPEN
- **UI-25** Help users recover from missing media tools — NEEDS_REVIEW
- **UI-26** Name the compact Export CSV action for screen readers — FIXED
- **UI-27** Keep chooser focus and announcements inside the dialog — FIXED
- **CODE-20** Keep exported worksheet names distinct — FIXED
- **CODE-21** Put spilled array results in the right cells — FIXED
- **CODE-22** Reuse calculated values while the user navigates — FIXED
- **CODE-23** Warn before importing XLSX features Loom cannot keep — NEEDS_REVIEW
- **CODE-24** Link XLSX cell styles so other spreadsheet apps apply them — FIXED
- **CODE-25** Import standard DEFLATE-compressed XLSX packages — FIXED
- **CODE-26** Preserve worker input failures across result coalescing — FIXED
- **CODE-27** Warn before dropping a single unsupported XLSX chart — NEEDS_REVIEW
- **CODE-28** Report unsupported absolute XLSX drawing anchors — NEEDS_REVIEW
- **CODE-29** Warn before dropping additional series from an XLSX chart — NEEDS_REVIEW
- **CODE-30** Accept the operating system's own recovery-path symlinks — FIXED
- **PERF-01** Keep large workbook work from freezing the window — NEEDS_REVIEW. [Oct27 developer-build and keyboard-cache evidence](../audits/uiux-2026-10-27.md#sheets-performance-follow-up--perf-01--needs_review); native latency and broader acceptance remain open.
- **REC-02** Keep unsaved recovery safe and bounded — FIXED for v1 on Windows and Linux (2026-10-07; real full-volume disk-full untested, injected write errors only)
- **GOV-02** Reconcile design-bible prose with canonical contracts — OPEN
- **GOV-03** Restore trustworthy geometry-manifest generation — OPEN
- **GOV-04** Bind visual acceptance to reviewable evidence — OPEN
- **GOV-05** Decide whether Loom's palette is an original identity — DECIDED
- **GOV-06** Meet WCAG 2.2 target-size and dragging criteria — OPEN
- **GOV-07** Retire macOS x86-64 and pin the build toolchain — OPEN
- **GOV-08** Make releases commercially shippable under the CRA — OPEN
- **GOV-09** Define shared rich-text semantics across Writer, Sheets, and Present — OPEN
- **GOV-10** Make open OOXML formats the primary user-facing save formats — OPEN

### Current supervisor pending list — 2026-10-18

All three office applications remain `ACCEPTANCE_BLOCKED`. The earlier 2026-10-18 acceptance claims were retracted; the later independent Argon reports each returned REJECT. Their detailed historical findings and the prior repair batch are retained below this marked ledger.

1. **Independent acceptance review:** after repairs, fresh reviewers must open the live app and renders and record evidence against the actual section 13 gates. No review or acceptance has been completed by this continuation.
2. **Sheets:** focus handling when the grid narrows (Argon D1, scoped repair verified on Windows below; Linux and broader checks remain), circular-reference message, window ballooning or hang on rapid resize after open, doubled gridline under a wrapped row, Slint private `paint` warning, dense-sheet scroll below the 60 fps budget. A new OPEN native check reproduces ignored grid characters after Alt+Space system-menu sizing until Alt is tapped; retained modifier state is a hypothesis pending event/physical-input verification. Recheck the review's empty-reference display and chart-redo findings too; they are not covered by the prior sort repair.
3. **Writer:** Markdown-backed tables lose pasted formatting/alignment/widths; per-character table undo; Table actions enabled outside tables (scoped repair verified below); inspector Del Row overflow (not reproduced at the two tested Windows sizes); missing painted cell-range selection, visible C3 clicks selecting C2, compact-inspector overlap; caret/placeholder overlap; PDF heading/table styling; imported underline; duplicate accessibility names; toolbar arrow navigation; Ctrl+Home margin; app-local font picker; chart summary instead of a drawn chart; picture resize, clipboard paste and alt text; table typing and page-render performance.
4. **Present:** horizontal bar export, rotated chart text smear, per-character table undo, stacked new objects, pie/multi-series chart fidelity, text-scale chrome glyph, theme colours, startup PPTX warning bypass (scoped repair verified below), per-run text formatting; actionable import failure text and deferred recovery error/order/replacement races.
5. **Cross-platform evidence:** live Linux interactions including picture chooser, typing and graceful close; complete inspected viewport/theme/text-scale/device-scale matrix; speech evidence remains owner-deferred, not a passing claim; Linux performance; recorded independent per-gate sign-off. macOS remains deferred pending a host.
