# LOOM ENGINEERING CONSTITUTION

This file is the **highest-authority engineering instruction in the repository**. Every coding agent, reviewer, automation, and human contributor must read it before modifying Loom. The owner's explicit instructions may change these repository rules; this file is not authority over the owner.

If another repository document, comment, old plan, screenshot, baseline, issue, model instruction, or generated report conflicts with this file, **this file wins**. If a requirement cannot be satisfied, report the blocker. Do not reinterpret, weaken, bypass, stub, hide, baseline, or redefine a requirement to make work appear complete.

## Owner override — 2026-09-15 P0/P1 repair run

The owner explicitly authorized this run to fix every recorded P0 and P1 audit card across the suite. For this run, the normal serial application lock is waived only for those named cards. Keep the safety, test, review, and evidence rules below. Finish each coherent milestone, run its focused checks, record the result in `TRUTH.md`, and commit and push it before starting the next milestone.

For a tiny local model, use this recipe for each card: read the card; write down what the user does, what breaks, and what should happen; edit only the named functions; run the focused test; inspect the real output; then copy the command, result, commit, and remaining gap into the card's repair result. `FIXED` means the code and required focused checks passed. `NEEDS_REVIEW` means the code landed but an expert, native, cross-platform, or real-media check is still missing. Never turn a missing check into a success claim.

For a performance card, write down the exact workbook size and machine. Measure formula work, workbook copying, recovery writes, normal Save/Open, visible input delay, scrolling, and peak memory as separate checks. Run the two fixed workloads in `loom-sheets/PERFORMANCE.md`: 10,000 chained formulas and 1,000,000 unique pseudorandom cells. The scroll test measures CPU projection only; it is not proof of native 60 fps. A fast formula test does not prove the window stays responsive; an empty memory-limit field is still an open check.

**Sheets performance status (2026-09-24):** Formula-bar commits now send one changed cell to a bounded background worker and show “Calculating…” immediately. Keep the last good numbers on screen until the matching revision arrives. On the one-million-cell test, the two measured steps—edit preparation and mailbox queueing—took 0.086 ms total. The test did not time the full formula-bar callback or visible frame. Worker calculation took 2.56 seconds, package creation took 12.72 seconds, and writing crash-recovery data took 43.05 seconds on this Intel Core i3-2350M. A crash could happen before recovery catches up. Fix the slow recovery work and measure it again. Ordinary Save/Open and CSV/XLSX operations are still synchronous. Non-cell edits still copy the whole workbook before sending it to the worker. Do not mark the performance card or Sheets accepted until those paths, the full callback and visible-frame timings, native million-cell scrolling, the app memory limit, accessibility, and interoperability have their own passing evidence. For every UI screenshot, use the running Linux window and the native screenshot tool only while the desktop is unlocked; a renderer picture is not live-window proof.

**Sheets data-safety status (2026-09-24):** Two P1 findings are newly recorded in `TRUTH.md`. Recovery currently saves another whole workbook after an edit and only clears old copies on explicit Save. There is no automatic storage limit. XLSX import keeps only features represented by Loom's workbook model, then shows a generic success message. It does not warn before dropping several features such as defined names, conditional formatting, validation rules, or external links. Read CODE-23 and REC-02 before changing these paths. The recovery file format needs the reviewed design first. When fixing XLSX import, warn before replacing the open workbook; Cancel must leave both the workbook and recovery data untouched.

For a tiny coding model checking recovery: count the bytes kept after 1, 10, and 100 unsaved edits, then check how long the newest edit takes to reach durable recovery. Put a firm, reviewed bound on retained recovery storage. Save and verify a newer complete checkpoint before deleting only the older records it covers. Restart after each possible interruption and compare the recovered cells, formulas, tabs, active tab, and images with the user's last accepted workbook. A disk-full or journal error must be shown to the user. Do not invent a new recovery file format; first use the reviewed design, then follow its numbered implementation plan.

For a tiny coding model fixing Sheets responsiveness: after the user commits a cell, show “Calculating…” right away. Do formula work, recovery-file writing, and normal file work on a background worker. Do not copy the whole workbook on the UI thread and call it fixed: time the copy too. Keep only one job running and one newest job waiting. Put a number on each edit; when a job finishes, show its result only if its number is still the newest and the same tab is open. Do not erase a formula the user is currently typing. Keep the old result visibly marked until the new result is ready. Measure the full input callback and the first visible frame separately; timing only edit preparation and queue submission is not enough. Test fast consecutive edits, tab switching, undo, recovery failure, and the real Linux window before marking the card fixed.

## Owner override — 2026-09-18 Sheets then Writer completion run

The owner explicitly authorized advancing the workflow to Sheets and completing the recorded Sheets findings and acceptance gate, then doing the same for Writer. Work remains serial: finish Sheets, prove its binary gate, commit and push each coherent milestone, then advance the workflow to Writer. This override includes the remaining Sheets P2/UI findings, focused regression coverage, acceptance evidence, and the source-size maintenance needed for a clean Sheets gate. Keep later applications locked until their turn. Do not mark an app `ACCEPTED` from a green unit test alone; record the required native, visual, accessibility, and persistence evidence.

## Start here — one small repair at a time

The owner requested the 2026-09-14 code and UI/UX audit and these repair instructions. Findings and exact repair cards live in `TRUTH.md`; each card keeps its original finding and records its current repair result. New findings start OPEN.

For a small local coding model, including a model with a tiny context window:

1. Read this section, the active gate and repair order at the top of `TRUTH.md`, and `loom-bootstrap/contracts/workflow.toml`. If they disagree, report the exact disagreement before editing product code.
2. Normally select **one** OPEN card allowed by the active gate. Start with CODE-01. During the dated owner overrides above, the owner has already authorized the listed P0/P1 cards and the active application's remaining cards, so keep the same one-card boundaries inside each milestone while working through that explicit list. Read only the current card and its named source functions, their callers, and applicable rules here. Do not ask a tiny model to repair the entire suite in one prompt.
3. Say what the user does, what goes wrong now, and what must happen instead. Copy the card's acceptance check into a focused regression test or a short manual interaction script. For a data-loss fix, first run the test and see it fail for the reported reason.
4. Make the smallest coherent change. A card with several steps may need several patches. Do not mix formatting, renaming, feature work, and a bug fix. Preserve unrelated uncommitted work.
5. Run the failing example again and the affected module's existing tests. For a UI change, also build the actual app, perform the interaction, save a fresh screenshot, and inspect it at the card's sizes/themes. A callback-only journey does not prove native focus or keyboard behavior.
6. Check the real result: compare saved/reopened data, inspect an independently opened export, or verify the changed visible state. A success message or a green build is not proof.
7. Record changed files, exact commands, observed result, evidence path, and remaining gaps under that card. Mark FIXED only when its check passes. Mark NEEDS_REVIEW when the code changed but a required check or expert review is missing. Do not delete the original finding.
8. Normally stop after this card. Do not change acceptance, unlock the next app, refresh visual baselines, or take the next card automatically unless the current task or the owner override above authorizes it.

Useful starting commands (run from the repository root; substitute the one named app/package):

```bash
rg -n '^### CODE-01|^### CODE-02' TRUTH.md
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

Only these root Markdown files are authoritative:

- `AGENTS.MD` — owner-controlled engineering constitution and acceptance rules.
- `TRUTH.md` — current verified product state, active gate, and blockers.
- `README.md` — public-facing introduction.

Machine-readable contracts under `loom-bootstrap/contracts/` and `loom-design-bible/contracts/` are normative where this file delegates exact values to them.

All other prose is reference material only. Never add an `AGENTS.md`/`AGENTS.MD` below the repository root. Never add ad-hoc status reports, agent journals, completion reports, or implementation plans to source control. Temporary evidence belongs in `.work/` or CI artifacts.

Delete obsolete agent residue instead of preserving it "for context". Stale instructions are defects because they mislead future agents.

## 3. Active workflow and audit repair gate

The current phase and active repair scope are recorded together in `TRUTH.md` and `loom-bootstrap/contracts/workflow.toml`. Do not infer them from old acceptance prose or this file's history.
The dated owner overrides at the top of this file are explicit exceptions for the 2026-09-15 P0/P1 run and the 2026-09-18 Sheets-then-Writer completion run. The latter authorizes the recorded phase transition and serial application edits while preserving the binary acceptance gates.

During `audit-repair`, new application features and new foundation consumers are locked. In the active `sheets` phase, product edits are limited to Sheets and the evidence required by its gate; later applications remain locked. Existing foundation imports may remain only for consumers enumerated in the workflow contract; retaining an import is not acceptance.

Finish the shared data-integrity repairs and their failure-path tests before application repairs. Recheck the shared visual foundation and obtain explicit human acceptance before advancing application design. Then repair applications serially, starting with Sheets. A later app's recorded defect is not permission to skip the queue. Security/runtime cards outside the app sequence need an explicitly recorded bounded repair scope before implementation.

Changing phase is a separate reviewed action: update the workflow contract and the active gate in `TRUTH.md` together, with evidence. When adding a repair stage the validator does not yet support, update `loom-bootstrap/scripts/audit-governance.py` and its focused tests in that same reviewed change. The contract's allowed prefixes are an outer file boundary; the named active scope still limits which product behavior may change. An audit or a documentation change must never silently promote an app to ACCEPTED.

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

The foundation is accepted only after reviewed gallery screenshots are judged production-quality and `TRUTH.md` records explicit human acceptance. A deterministic screenshot is not automatically a good screenshot.

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

Apply the specific UI cards in `TRUTH.md`, one at a time. These rules describe the required result:

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

## 12. TRUTH discipline

`TRUTH.md` is the only live readiness ledger.

Agents may update it only from verified evidence. Do not manufacture numerical readiness scores. Report concrete capabilities, open defects, tested flows, and missing acceptance evidence. Historical scores and agent-reviewed screenshots are not human acceptance.

Use these state labels exactly where applicable:

- `NOT_STARTED`
- `SCAFFOLDED`
- `FUNCTIONAL_WITH_LIMITATIONS`
- `ACCEPTANCE_BLOCKED`
- `ACCEPTED`

Card states are `OPEN`, `NEEDS_REVIEW`, and `FIXED`. Workflow scheduling uses `LOCKED` and `IN_PROGRESS`; these describe permission to work, not product quality. The live application quality table in `TRUTH.md` must not contradict its detailed sections or the workflow contract.

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
14. explicit `ACCEPTED` record in `TRUTH.md`.

If any item fails, the application remains locked in `ACCEPTANCE_BLOCKED` and the next application must not begin.

## 14. Completion behavior for agents

Before claiming a task is finished, an agent must:

1. inspect the active workflow lock;
2. confirm the change is inside the allowed scope;
3. run the relevant focused checks;
4. inspect failures rather than weakening tests;
5. update `TRUTH.md` only if verified product truth changed;
6. state remaining blockers explicitly.

The goal is not to make CI green at any cost. The goal is to make Loom genuinely excellent while keeping the repository small enough, clear enough, and strict enough that the next agent cannot accidentally turn it back into a jungle.
