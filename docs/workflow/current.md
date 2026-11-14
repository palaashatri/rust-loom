## current-truth

<!-- CURRENT TRUTH START -->
# Loom — Current Truth

This file alone owns the current workflow gate and ordered quality/work table. Root [AGENTS.md](../../AGENTS.md) imports the rules; [workflow.toml](../../loom-bootstrap/contracts/workflow.toml) records the machine gate. Read the [repair index](repairs.md), [governance debt](governance-debt.md), [Writer scope](writer.md) or [current UI audit](../audits/uiux-2026-10-27.md) only when relevant. Dated test/evidence records are linked from the [documentation index](../README.md); earlier green runs do not carry across edits.

## Active gate — read this before choosing a card

```text
ACTIVE PHASE: OFFICE-V1
FOUNDATION STATUS: ACCEPTED
SUITE STATUS: ACCEPTANCE_BLOCKED
APPLICATION DEVELOPMENT: UNLOCKED
ACTIVE APPLICATIONS: SHEETS, WRITER, PRESENT
NEXT APPLICATION: PHOTO
```

The foundation's ACCEPTED value is the pre-existing contract record, not a new approval from this run. Existing consumer imports remain compatible. The `office-v1` scheduling phase records the 2026-10-02 and 2026-10-06 owner grants for Sheets, Writer, and Present; it does not grant application acceptance or recertify the gallery. Do not delete or refresh existing baselines to hide findings.

**Machine gate:** Sheets is `ACCEPTED` (promoted 2026-11-14 after Gate 1–14 verification on macOS, Linux, and Windows 11 under v1 scope). Writer and Present remain `IN_PROGRESS` and `ACCEPTANCE_BLOCKED` under the dated owner overrides; Photo, Motion, Video, Studio, and Encode remain `LOCKED` for product changes. `SUITE STATUS` stays `ACCEPTANCE_BLOCKED` until Writer and Present complete their gates. Keep remaining native, visual, accessibility, interoperability, performance, and persistence evidence open for Writer and Present until each applicable gate is proven.

The contract's allowed prefixes are an outer file boundary for the three authorized applications and their necessary shared infrastructure. They do not unlock later applications. The `office-v1` validator requires the group declaration, matching work and product statuses, accepted foundation, and exact bounded prefixes. Existing serial phases remain supported.

**Repository scope (2026-10-25, housekeeping on `main`).** `main` carries the shared infrastructure and the three v1 applications only (Writer, Sheets, Present). Photo, Motion, Video, Studio, Encode, `loom-vision`, `loom-plugin-sdk`, `loom-samples` and `loom-spec` moved to branch `apps/later`, which keeps the full pre-removal state. Six `loom-core` crates used only by those applications were removed from the workspace: `loom-color`, `loom-history`, `loom-interop`, `loom-jobs`, `loom-media-runtime` and `loom-runtime`. The matrix and packaging scripts, the root and bootstrap READMEs, the CI and release workflows, and the conformance-corpus generator (its output lived in the removed `loom-samples`) were trimmed to the same three applications. `loom-bootstrap/contracts/workflow.toml` and `audit-governance.py` still describe the eight-application serial order with the five later applications `LOCKED`; that is a deliberate governance choice until the owner decides otherwise. Evidence: the four repository audits and 29 governance unit tests pass, and `cargo metadata --locked` resolves the loom-core, Writer, Sheets and Present workspaces. Full builds were not rerun for this housekeeping. No application acceptance status changed.

**Repair order:** address the [current supervisor pending list](repairs.md#current-supervisor-pending-list--2026-10-18) in bounded milestones across Sheets, Writer, and Present; data loss and broken output come before visual polish. Review each app independently against all section 13 gates. Photo is next after the authorized group and remains locked until a separate reviewed workflow transition with passing acceptance evidence.

**How to use a card:** follow [Start here](../rules/workflow.md#start-here--one-small-repair-at-a-time). Select one coherent defect, reproduce it, make a small repair, run its concrete check, and record evidence. The owner overrides authorize the three-application continuation, not weaker acceptance requirements. States: OPEN, NEEDS_REVIEW, FIXED. FIXED does not mean the whole application is ACCEPTED. P2 and GOV cards remain OPEN unless their own record says otherwise.

Quality and permission to work are different. The office-v1 owner grant permits bounded Sheets, Writer and Present work; the five later applications stay locked. This is the single live application status table:

| Order | Application | Product status | Work status | Current blocking evidence |
|---:|---|---|---|---|
| 1 | Sheets | ACCEPTED | ACCEPTED | 2026-11-14 acceptance + UI discoverability follow-up: macOS `cargo fmt --check`, macOS strict Clippy (`-D warnings`), all 58 Python bootstrap unit tests + 4 repo audits, `loom-core` 492/0/1 (38 suites), and `loom-sheets` 1107/0/2 (22 suites) pass on the final tree (`Cmd/Ctrl+K` / `Cmd/Ctrl+Shift+P` / `F1` palette shortcuts, full palette & menu shortcuts, `Delete sheet` button, 8-row `Table` menu). Prior to this discoverability follow-up (at 1104/0/2), `x86_64-pc-windows-msvc` strict Clippy, Linux Docker (`aarch64-unknown-linux-gnu`, `rustc 1.100.0`) clippy/release/tests (1104/0/2), X11+AT-SPI matrix (`0` unnamed), LibreOffice Calc 24.2.7.2 interop (17/17 + round-trip), and Windows 11 ARM64 VM release GUI (`VK_MENU` guard, UIA `0` unnamed visible interactive, live editing) passed. `PERF-01` FIXED in `--release` (1M-cell sparse scroll p95 0.12 ms; 1M+10k-formula sort callback p50 3.31 ms / undo 2.26 ms; 1M CSV/XLSX export callback <= 0.027 ms; `LOOM_FRAME_BENCH=enforce` 300k/16-image software-render p95 <= 8.88 ms); 64 MiB `Source::File` fallback + recovery pause verified. Disclosed scope: Gate 9 spoken screen-reader output owner-deferred; `REC-02` disk-full injected-only; bare-metal hardware-GPU / Wayland / fractional-DPI frame timing unmeasured; Linux/Windows VMs were not re-run after the shortcut/menu follow-up. See the [Oct27 audit](../audits/uiux-2026-10-27.md#sheets-completion--gate-114-verification--2026-11-14). |
| 2 | Writer | ACCEPTANCE_BLOCKED | IN_PROGRESS | v1 scope (Windows, Linux). M1–M6 are complete with scoped regressions, recorded in the [Writer scope](writer.md). The 2026-11-06 tree reports 1315 passed, 0 failed, 5 ignored, with fmt and strict Clippy clean. 2026-11-06 fixes: wrapped table-cell text, Markdown paste, the caret after a table-ending paste, and hover. PR #21 review items are verified in source: footnote continuation order, explicit Arial, and complex-script loss, which is now a user-visible fidelity warning. Open for M7: live Linux/Windows workflows; the visual, accessibility, performance and interoperability matrix; independent review. Also open: Writer's full test binary does not link in the 5.7 GiB Linux container (the macOS suite is green); complex-script formatting is warned about, not modelled (one style per run); headers, footers and footnotes are not scanned for complex-script losses; Orca walkthrough and speech (gate 9 owner-deferred). |
| 3 | Present | ACCEPTANCE_BLOCKED | IN_PROGRESS | v1 scope (Windows, Linux). The 2026-10-18 ACCEPTED record was retracted by the supervisor (see the [retraction record](../archive/continuation-2026-10-04-to-25.md)): unverified evidence and no shown independent review. The 2026-10-31 completion pass closed P-01–P-09 and MAC-01–03 for Present. The 2026-11-07 PR #21 review verified the object-placement regression, which covers twelve mixed insertions. The suite reports 447 passed, 0 failed, 2 ignored, with fmt and strict Clippy clean on the final tree. Open: per-run text formatting (GOV-09; one style per element, and PDF uses its built-in face); live Linux/Windows evidence; Orca walkthrough; independent review |
| 4 | Photo | ACCEPTANCE_BLOCKED | LOCKED | CODE-04/UI-16 repaired; CODE-14, UI-14/18 and visual checks remain |
| 5 | Motion | ACCEPTANCE_BLOCKED | LOCKED | UI-17 still-frame parity verified; UI-14/18 and full video-render acceptance remain |
| 6 | Video | ACCEPTANCE_BLOCKED | LOCKED | UI-25 repaired in code; UI-19/20 and real-media checks remain |
| 7 | Studio | ACCEPTANCE_BLOCKED | LOCKED | CODE-10 repaired in code; audio and visual acceptance checks remain |
| 8 | Encode | ACCEPTANCE_BLOCKED | LOCKED | CODE-09/UI-23 repaired in code; filesystem/media checks remain |


## Done and pending by application — 2026-11-14

This is a work-status summary for every application. Sheets (`order 1`) is `ACCEPTED` (2026-11-14); Writer, Present, and the five locked applications remain `ACCEPTANCE_BLOCKED` until their applicable gates are proven. Card states come from the [repair index](repairs.md); evidence sits in the linked owner documents. The later applications' code and documents live on `apps/later`, per the repository scope note above, and were not changed in this pass.

**Sheets (order 1, ACCEPTED)**
- Done: formulas and cross-sheet references, charts (with undo/redo visibility sync), anchored objects and images (embedded + `Source::File` 64 MiB header guard & fallback card + fail-closed recovery pause), CSV and XLSX import and export with loss warnings, LibreOffice Calc 24.2.7.2 interop (17/17 + round-trip), Oct27 & 2026-11-14 repairs (`PERF-01` FIXED in `--release`, `Decimals` row unclipped at 1280×800, wrapped-row single gridline, `=B1` empty reference displays `0`, macOS palette `Cmd+` shortcuts + quit unsaved-work guard + `--open` sheet tabs, `loom-sheets-cli --help`, Windows WGL shader-preflight software fallback + `Alt+Space` `VK_MENU` release guard, Linux `rustc 1.100.0` Clippy clean, `Delete sheet` tab button, expanded 8-row `Table` menu, `Cmd/Ctrl+K`/`Cmd/Ctrl+Shift+P`/`F1` palette shortcuts, and shortcuts across all palette/menu commands). Verified on macOS (`1107 passed, 0 failed, 2 ignored`), and prior to the shortcut/menu follow-up on Ubuntu Linux Docker (`1104/0/2`, X11/AT-SPI) and Windows 11 ARM64 VM (UIA + live GUI).
- Disclosed limits: spoken screen-reader output remains owner-deferred under Gate 9 (AT-SPI and Windows UIA trees verified at 0 unnamed visible interactive controls); `REC-02` disk-full recovery tested with injected write failures rather than full-volume exhaustion; live bare-metal hardware-GPU / Wayland / fractional-DPI frame timing unmeasured; 2.0× text-scale chart summary needs height > 724 px.

**Writer (order 2, IN_PROGRESS)**
- Done: M1–M6 (table selection, justified paragraphs, mixed-direction text and inline styling, headers and footers, footnotes, tracked changes). Table-cell wrapping, Markdown paste, the caret after a table-ending paste, and hover (2026-11-06). PR #21 review: footnote continuation order, explicit Arial, and the complex-script fidelity warning.
- Pending: M7 integration and acceptance (live Linux/Windows workflows, the visual, accessibility, performance and interoperability matrix, independent review); the Linux test-binary link; complex-script modelling; headers, footers and footnotes complex-script scanning; the 2026-10-18 supervisor list in [repairs.md](repairs.md#current-supervisor-pending-list--2026-10-18), which was not re-verified item by item in this pass; Orca walkthrough and speech.

**Present (order 3, IN_PROGRESS)**
- Done: P-01–P-09 and MAC-01–03; element-level font family with PPTX round trip; placement of new objects (twelve-insertion regression); hover on the shared controls; the Present View menu item name.
- Pending: per-run text formatting (GOV-09); compact-inspector scroll cue, mixed-value style marker and text-only layout previews; headless rotation rendering; live Linux/Windows evidence; Orca walkthrough; the 2026-10-18 supervisor list items (horizontal bar export, rotated chart text, per-character table undo, pie and multi-series fidelity, text-scale chrome glyph, theme colours, the deferred-recovery retry guard), which were not re-verified in this pass; independent review.

**Photo (LOCKED)**
- Done: CODE-04 (guarded New and Open) and UI-16 (whole image fits its viewport), both FIXED.
- Pending: CODE-14 (unique layer IDs, OPEN); the remaining compact and precision checks; a real import, edit, save, reopen and export journey.

**Motion (LOCKED)**
- Done: UI-17 (still-frame parity with the exported frame), FIXED.
- Pending: full video-render acceptance; scene manipulation, graph and timeline editing, effects, playback and render workflows, and interchange. This table also lists UI-14 and UI-18 as remaining for Motion, but [repairs.md](repairs.md) records those IDs as FIXED for other scopes. Confirm whether Motion has its own cards before acting on them.

**Video (LOCKED)**
- Done: UI-25 (missing-media recovery) repaired in code; the repair index shows it NEEDS_REVIEW.
- Pending: UI-19 timeline zoom versus scrolling (OPEN); UI-20 clip and region caption contrast (OPEN); real-media checks; scalable timeline interaction; source and viewer workflows; export UX.

**Studio (LOCKED)**
- Done: CODE-10 (audio precision in projects), FIXED.
- Pending: UI-21 one transport control per action (OPEN); UI-22 audio-region content (OPEN); audio and visual acceptance; production recording; realtime scheduling; comping, time and pitch; plugin isolation and UI.

**Encode (LOCKED)**
- Done: CODE-09 (no-overwrite at the final write) and UI-23 (truthful empty queue and readiness), both FIXED.
- Pending: UI-24 job settings that fit the editing task (OPEN); UI-25 readiness path (NEEDS_REVIEW; the same card is listed for Video, and the repair index does not separate them); filesystem review; real encode fixtures; queue and settings hierarchy; hardware policy; pause and resume guarantees; format and perceptual conformance.

## Owner scope — 2026-10-27

The owner explicitly requested splitting the oversized root instructions into focused files, icon-matched application accents, and investigation/repair of reported Sheets lag. This bounded grant supersedes the old single-Markdown policy and the part of the 2026-10-25 GOV-05 decision that deferred per-app accents. It does not change application quality, foundation acceptance, phase, baseline approval or the later-app locks.

- **Documentation:** root remains the authority router. Move rules, active gate, current task scopes, findings and dated evidence to their declared owners, preserve original findings/results and update validators/callers. No duplicate readiness ledger. Historical display/game permissions and model choices stay dated; the current chat governs desktop access.
- **Application accents:** implement Writer violet, Sheets jade and Present coral through the existing shared tokens, contracts and theme selector. Keep neutral structural chrome, existing document artwork/content colours, accessible labels, visible focus and the universal high-contrast scheme. Synchronize TOML/runtime values; verify contrast and fresh renders without approving or refreshing baselines. The implementation/evidence owner is the [Oct27 audit](../audits/uiux-2026-10-27.md); do not assert completion until its checks run.
- **Sheets performance:** reproduce the reported lag with a declared workload, build, renderer and environment; trace the measured cause through event handling, viewport projection, calculation and recovery as applicable. Repair only a demonstrated bottleneck, retain data/undo/recovery and acceptance budgets, add a meaningful regression and record actual timings and remaining native checks. No speedup or result is recorded by this authorization alone; this task is permitted within existing `loom-sheets/` and necessary shared infrastructure boundaries.

The selected bounded Sheets investigation was keyboard navigation: arrow/Shift+arrow now use the existing workbook cache, including cross-sheet values. The [Oct27 audit record](../audits/uiux-2026-10-27.md#sheets-performance-follow-up--perf-01--needs_review) reports two focused regressions, 26 grid-interaction tests and five Tab/Enter tests passed; it also records software-renderer callback p95 1.25 ms and frame p95 9.96 ms against 16.7/45 ms budgets. Native GPU latency, physical modifier delivery and broader platform performance remain open. Supporting evidence is under `.work/sheets-perf-20261027/` and `.work/app-colors-20261027/`.

The latest recorded icon checkpoint includes Writer Clippy errors and incomplete/failing app suites, preserved in the [audit](../audits/uiux-2026-10-27.md#comparative-uiux-audit--owner-request-2026-10-27). Product acceptance remains blocked. Record any later accent/performance verification in that finding/evidence owner before changing an applicable state.

**Owner scope — 2026-10-28.** The owner authorizes completion of all open Sheets application work, including applicable repair cards, backlog and Oct27 audit items, together with shared elements required for Sheets. This scope excludes product changes to other applications. It does not change any acceptance or workflow status; update findings and evidence owners only from verified results, and keep Sheets blocked until every applicable gate is proven.

**Owner scope — 2026-10-31.** The owner authorizes completing the remaining work in all three active applications — Sheets, Writer and Present — including the documented UI/UX audit findings, reviewed on a dedicated parallel branch. This grants product work across the three active applications and the shared infrastructure they require; it does not change any application or product status, waive an acceptance gate, refresh a baseline, or unlock Photo, Motion, Video, Studio or Encode. Record findings and evidence only from actual results and keep every application `ACCEPTANCE_BLOCKED` until its applicable gates are independently proven.

**Owner request — 2026-11-07.** The owner asked for the ledgers and documents to be updated for all applications, for the PR #21 review comments to be fixed and resolved, and for the PR to be merged once the required work was done. Required work means the review threads, these ledger and document updates, and passing checks. The merge carries the repair pass into `main`. It does not change any application's product status, waive an acceptance gate, refresh a baseline or unlock a later application. The PR's merge state is recorded on the PR itself.

<!-- CURRENT TRUTH END -->
