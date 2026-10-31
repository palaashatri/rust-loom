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

**Machine gate:** Sheets, Writer, and Present are `IN_PROGRESS` under the dated owner overrides. Photo, Motion, Video, Studio, and Encode remain `LOCKED` for product changes. All eight applications remain `ACCEPTANCE_BLOCKED`; the earlier retracted acceptance records do not satisfy any gate. Keep native, visual, accessibility, interoperability, performance, and persistence evidence open until each applicable gate is proven.

The contract's allowed prefixes are an outer file boundary for the three authorized applications and their necessary shared infrastructure. They do not unlock later applications. The `office-v1` validator requires the group declaration, matching work and product statuses, accepted foundation, and exact bounded prefixes. Existing serial phases remain supported.

**Repository scope (2026-10-25, housekeeping on `main`).** `main` carries the shared infrastructure and the three v1 applications only (Writer, Sheets, Present). Photo, Motion, Video, Studio, Encode, `loom-vision`, `loom-plugin-sdk`, `loom-samples` and `loom-spec` moved to branch `apps/later`, which keeps the full pre-removal state. Six `loom-core` crates used only by those applications were removed from the workspace: `loom-color`, `loom-history`, `loom-interop`, `loom-jobs`, `loom-media-runtime` and `loom-runtime`. The matrix and packaging scripts, the root and bootstrap READMEs, the CI and release workflows, and the conformance-corpus generator (its output lived in the removed `loom-samples`) were trimmed to the same three applications. `loom-bootstrap/contracts/workflow.toml` and `audit-governance.py` still describe the eight-application serial order with the five later applications `LOCKED`; that is a deliberate governance choice until the owner decides otherwise. Evidence: the four repository audits and 29 governance unit tests pass, and `cargo metadata --locked` resolves the loom-core, Writer, Sheets and Present workspaces. Full builds were not rerun for this housekeeping. No application acceptance status changed.

**Repair order:** address the [current supervisor pending list](repairs.md#current-supervisor-pending-list--2026-10-18) in bounded milestones across Sheets, Writer, and Present; data loss and broken output come before visual polish. Review each app independently against all section 13 gates. Photo is next after the authorized group and remains locked until a separate reviewed workflow transition with passing acceptance evidence.

**How to use a card:** follow [Start here](../rules/workflow.md#start-here--one-small-repair-at-a-time). Select one coherent defect, reproduce it, make a small repair, run its concrete check, and record evidence. The owner overrides authorize the three-application continuation, not weaker acceptance requirements. States: OPEN, NEEDS_REVIEW, FIXED. FIXED does not mean the whole application is ACCEPTED. P2 and GOV cards remain OPEN unless their own record says otherwise.

Quality and permission to work are different. The office-v1 owner grant permits bounded Sheets, Writer and Present work; the five later applications stay locked. This is the single live application status table:

| Order | Application | Product status | Work status | Current blocking evidence |
|---:|---|---|---|---|
| 1 | Sheets | ACCEPTANCE_BLOCKED | IN_PROGRESS | v1 scope (Windows, Linux). The 2026-10-18 ACCEPTED record was retracted by the supervisor (see the [retraction record](../archive/continuation-2026-10-04-to-25.md)): its evidence file cites tests that do not exist and its Argon review is not shown to have run. The 2026-10-31 completion pass reports 1077 passed, 0 failed, 2 ignored with fmt and strict Clippy clean, and closes the Oct27 S-01–S-09/UI-35–37/CODE-23–29 repairs in the [audit](../audits/uiux-2026-10-27.md#completion-pass--2026-10-31). Sustained native GPU input/frame latency remains unmeasured; full-matrix and live Linux/Windows visual evidence, independent review and PERF-01 stay open; gate 9 speech is deferred by the owner |
| 2 | Writer | ACCEPTANCE_BLOCKED | IN_PROGRESS | v1 scope (Windows, Linux). M3–M6 implementation and focused regressions now pass: the 2026-10-31 completion pass reports 1302 passed, 0 failed, 5 ignored with fmt and strict Clippy clean, and the [Writer scope](writer.md) records M3 mixed direction, M4 headers/footers, M5 footnotes and M6 tracked changes as complete. M7 still requires current live Linux/Windows workflows, visual/accessibility/performance/interop evidence and independent review; the retracted 2026-10-18 acceptance does not satisfy these gates. |
| 3 | Present | ACCEPTANCE_BLOCKED | IN_PROGRESS | v1 scope (Windows, Linux). The 2026-10-18 ACCEPTED record was retracted by the supervisor (see the [retraction record](../archive/continuation-2026-10-04-to-25.md)): unverified evidence and no shown independent review. The 2026-10-31 completion pass reports 429 passed, 0 failed, 2 ignored with fmt and strict Clippy clean, closes P-01–P-09 and repairs MAC-01; per-run text formatting, live Linux/Windows evidence and independent review remain open |
| 4 | Photo | ACCEPTANCE_BLOCKED | LOCKED | CODE-04/UI-16 repaired; CODE-14, UI-14/18 and visual checks remain |
| 5 | Motion | ACCEPTANCE_BLOCKED | LOCKED | UI-17 still-frame parity verified; UI-14/18 and full video-render acceptance remain |
| 6 | Video | ACCEPTANCE_BLOCKED | LOCKED | UI-25 repaired in code; UI-19/20 and real-media checks remain |
| 7 | Studio | ACCEPTANCE_BLOCKED | LOCKED | CODE-10 repaired in code; audio and visual acceptance checks remain |
| 8 | Encode | ACCEPTANCE_BLOCKED | LOCKED | CODE-09/UI-23 repaired in code; filesystem/media checks remain |


## Owner scope — 2026-10-27

The owner explicitly requested splitting the oversized root instructions into focused files, icon-matched application accents, and investigation/repair of reported Sheets lag. This bounded grant supersedes the old single-Markdown policy and the part of the 2026-10-25 GOV-05 decision that deferred per-app accents. It does not change application quality, foundation acceptance, phase, baseline approval or the later-app locks.

- **Documentation:** root remains the authority router. Move rules, active gate, current task scopes, findings and dated evidence to their declared owners, preserve original findings/results and update validators/callers. No duplicate readiness ledger. Historical display/game permissions and model choices stay dated; the current chat governs desktop access.
- **Application accents:** implement Writer violet, Sheets jade and Present coral through the existing shared tokens, contracts and theme selector. Keep neutral structural chrome, existing document artwork/content colours, accessible labels, visible focus and the universal high-contrast scheme. Synchronize TOML/runtime values; verify contrast and fresh renders without approving or refreshing baselines. The implementation/evidence owner is the [Oct27 audit](../audits/uiux-2026-10-27.md); do not assert completion until its checks run.
- **Sheets performance:** reproduce the reported lag with a declared workload, build, renderer and environment; trace the measured cause through event handling, viewport projection, calculation and recovery as applicable. Repair only a demonstrated bottleneck, retain data/undo/recovery and acceptance budgets, add a meaningful regression and record actual timings and remaining native checks. No speedup or result is recorded by this authorization alone; this task is permitted within existing `loom-sheets/` and necessary shared infrastructure boundaries.

The selected bounded Sheets investigation was keyboard navigation: arrow/Shift+arrow now use the existing workbook cache, including cross-sheet values. The [Oct27 audit record](../audits/uiux-2026-10-27.md#sheets-performance-follow-up--perf-01--needs_review) reports two focused regressions, 26 grid-interaction tests and five Tab/Enter tests passed; it also records software-renderer callback p95 1.25 ms and frame p95 9.96 ms against 16.7/45 ms budgets. Native GPU latency, physical modifier delivery and broader platform performance remain open. Supporting evidence is under `.work/sheets-perf-20261027/` and `.work/app-colors-20261027/`.

The latest recorded icon checkpoint includes Writer Clippy errors and incomplete/failing app suites, preserved in the [audit](../audits/uiux-2026-10-27.md#comparative-uiux-audit--owner-request-2026-10-27). Product acceptance remains blocked. Record any later accent/performance verification in that finding/evidence owner before changing an applicable state.

**Owner scope — 2026-10-28.** The owner authorizes completion of all open Sheets application work, including applicable repair cards, backlog and Oct27 audit items, together with shared elements required for Sheets. This scope excludes product changes to other applications. It does not change any acceptance or workflow status; update findings and evidence owners only from verified results, and keep Sheets blocked until every applicable gate is proven.

**Owner scope — 2026-10-31.** The owner authorizes completing the remaining work in all three active applications — Sheets, Writer and Present — including the documented UI/UX audit findings, reviewed on a dedicated parallel branch. This grants product work across the three active applications and the shared infrastructure they require; it does not change any application or product status, waive an acceptance gate, refresh a baseline, or unlock Photo, Motion, Video, Studio or Encode. Record findings and evidence only from actual results and keep every application `ACCEPTANCE_BLOCKED` until its applicable gates are independently proven.

<!-- CURRENT TRUTH END -->
