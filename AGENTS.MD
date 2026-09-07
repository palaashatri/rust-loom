# LOOM ENGINEERING CONSTITUTION

This file is the **highest-authority engineering instruction in the repository**. Every coding agent, reviewer, automation, and human contributor must read it before modifying Loom.

If another repository document, comment, old plan, screenshot, baseline, issue, model instruction, or generated report conflicts with this file, **this file wins**. If a requirement cannot be satisfied, report the blocker. Do not reinterpret, weaken, bypass, stub, hide, baseline, or redefine a requirement to make work appear complete.

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

- `AGENTS.MD` — immutable engineering constitution and acceptance rules.
- `TRUTH.md` — current verified product state, active gate, and blockers.
- `README.md` — public-facing introduction.

Machine-readable contracts under `loom-bootstrap/contracts/` and `loom-design-bible/contracts/` are normative where this file delegates exact values to them.

All other prose is reference material only. Never add an `AGENTS.md`/`AGENTS.MD` below the repository root. Never add ad-hoc status reports, agent journals, completion reports, or implementation plans to source control. Temporary evidence belongs in `.work/` or CI artifacts.

Delete obsolete agent residue instead of preserving it "for context". Stale instructions are defects because they mislead future agents.

## 3. ACTIVE PHASE: UI FOUNDATION LOCK

The current phase is **UI FOUNDATION**.

Until `loom-bootstrap/contracts/workflow.toml` and `TRUTH.md` both record the shared UI foundation as `ACCEPTED`, application feature development is locked.

### Allowed work during this phase

- repository cleanup and CI simplification;
- shared UI architecture in `loom-core/crates/loom-ui/**`;
- design tokens and machine-readable UI contracts;
- component-gallery implementation and validation;
- accessibility, keyboard, responsive, localization, and theme behavior of shared components;
- code-structure cleanup that reduces existing debt without adding product scope;
- licensing/asset provenance infrastructure;
- fixes strictly required to keep the repository buildable after shared-infrastructure changes.

### Forbidden work during this phase

- new Writer, Sheets, Present, Photo, Motion, Video, Studio, or Encode features;
- new application-local generic buttons, fields, tabs, menus, sliders, panels, dialogs, toolbars, status bars, list rows, scrollbars, or other reusable controls;
- application visual redesign before the foundation gate is accepted;
- adding placeholder or disabled "future" controls to make screenshots look complete;
- refreshing old application screenshot baselines.

Existing application UI is **legacy product code**, not design authority. `toolkit.slint` and application-local component files remain compatibility surfaces until an accepted foundation exists. Do not copy their visual language into the new foundation merely because it already exists.

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

Agents may update it only from verified evidence. Never increase a readiness score because code volume increased, a component exists, a test fixture renders, or a screenshot baseline was refreshed.

Use these state labels exactly where applicable:

- `NOT_STARTED`
- `SCAFFOLDED`
- `FUNCTIONAL_WITH_LIMITATIONS`
- `ACCEPTANCE_BLOCKED`
- `ACCEPTED`

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
