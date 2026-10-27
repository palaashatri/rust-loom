# LOOM ENGINEERING CONSTITUTION

`AGENTS.md` is the **highest-authority engineering instruction in the repository** and imports these engineering rules. Every coding agent, reviewer, automation, and human contributor must read the applicable rules before modifying Loom. The owner's explicit instructions may change repository rules; these files are not authority over the owner.

The authority and import routing in root `AGENTS.md` govern conflicts. Historical documents, comments, old plans, screenshots, baselines, issues and generated reports cannot override these imported rules. If a requirement cannot be satisfied, report the blocker. Do not reinterpret, weaken, bypass, stub, hide, baseline, or redefine a requirement to make work appear complete.

## 1. Product and technology lock

Loom is an original, local-first, commercial-quality creative suite.

The product stack is locked to:

- Rust stable for product logic and native hosts.
- Slint for the application UI.
- Original Loom visual identity and interaction language: layout, typography, iconography and behavior. Neutral chrome remains subordinate to content; the bounded 2026-10-27 owner grant permits icon-matched shared app accents (see [GOV-05](../workflow/governance-debt.md#gov-05--decide-whether-looms-palette-is-an-original-identity) and the [active scope](../workflow/current.md#owner-scope--2026-10-27)).
- Local-first/offline-capable workflows; no mandatory account, telemetry, hidden upload, or remote inference.
- Cross-platform desktop targets: Linux x86-64, Windows x86-64 and macOS arm64. macOS x86-64 is a **sunsetting** target: it is kept while the Intel CI runner exists, gets fixes only, and is removed by fall 2027 or earlier by owner decision (§18). Windows arm64 and Linux aarch64 are planned targets (`NOT_STARTED`).
- Commercially redistributable assets and dependencies only.

Do **not** introduce Electron, Qt, GTK, web UI, Tauri, Flutter, another GUI toolkit, or a second application language without explicit owner instruction.

The active integration branch for this owner-authorized run is `main`. Keep changes on `main`; do not create long-lived feature branches, duplicate roadmaps, issue sprawl, or alternate sources of truth.

## 2. Authority and repository hygiene

The owner explicitly requested focused documentation on 2026-10-27, superseding the former single-Markdown rule. Root [AGENTS.md](../../AGENTS.md) is the authority entrypoint; its imported rules and [declared documentation roles](../../loom-bootstrap/contracts/documentation.toml) form one governed set. Read only the applicable files and named historical sections, not the entire archive.

Machine-readable contracts under `loom-bootstrap/contracts/` and `loom-design-bible/contracts/` remain normative where the rules delegate exact values to them. The [active gate and quality/work table](../workflow/current.md) are the only live readiness ledger. The repair index, original card bodies, Writer scope, governance debt and current audit each have one declared owner; do not copy their status into a parallel ledger.

Only Markdown paths declared in `loom-bootstrap/contracts/documentation.toml`, root `AGENTS.md`, project README files and the explicitly authorized Linux desktop-input skill are permitted. New focused documents require an intentional manifest/routing update. Never add another `AGENTS.md` below the root, and never add Markdown under `.work/`; use `.log`, `.txt` or `.json` for temporary evidence. Dated archives preserve facts, failures, original findings, retractions and evidence; historical grants are not standing instructions for new work.

Delete obsolete agent residue instead of preserving it "for context". Stale instructions are defects because they mislead future agents.

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

## 16. Engineering discipline — reuse, interoperability and real-workflow evidence

**Effective only for tasks authorized by the current workflow.** Lessons from experimental compatibility runtimes such as Raven are engineering guidance, not a Raven dependency, new release gate, toolkit change or permission to work on locked Loom apps. Loom remains **Rust + Slint**, local-first and cross-platform under §§1–15. `loom-bootstrap/contracts/workflow.toml` and the marked current-truth section remain authoritative.

### Prefer proven infrastructure behind stable interfaces

1. Before rebuilding a document parser, codec, layout/text engine, native adapter, accessibility bridge or graphics subsystem, record the observed missing capability, upstream alternatives, version/support/security characteristics, portability, licensing, upgrade cost and why a **small replaceable adapter** is insufficient. Reuse proven libraries where they genuinely meet Loom's contracts and do not violate its technology lock.
2. Preserve one authoritative document model, undo/redo, persistence/recovery engine, command identity and error path. No hidden format loss or app-local duplicate platform bridges. All imports are untrusted: cap uncompressed sizes, recursion, allocations, fonts/media dependencies and time, and report unsupported features rather than fabricating content.
3. Keep Loom's original cross-platform identity independent of SLOPOS/PARAMA. Future interoperability (§15) should use versioned host adapters, not an unjustified Slint replacement or Linux-only architectural fork.

### Test what actual users do

Document claimed support in the **existing evidence and current-truth mechanisms**, by specific document producer/format version, app commit, host OS/architecture, toolchain/engine versions and fixtures. Avoid a new Markdown status ledger. For each claimed feature, independently verify:

| Area | Minimum complete journey and failure coverage |
|---|---|
| Writer | Representative DOCX/ODT: import or create, edit real styled text/tables/images, save, exit, reopen, export, compare supported semantics/layout with source oracles |
| Sheets | Real XLSX/ODS formulas, formatting, charts and performance-sized workbooks; edit/recalculate/sort/undo, save/reopen/export, test errors and crash/recovery |
| Present | Real PPTX/ODP text/objects/charts where supported; edit slides, presenter actions, save/reopen/export, inspect visual and semantic fidelity |
| Shared native UX | Native file dialogs, menus invoking real commands, clipboard, accessibility, keyboard focus, screen scaling, app launch and window lifecycle on every **claimed** platform |
| Negative/recovery | Malformed/truncated documents, unsupported properties, disk-full, missing fonts/codecs, permission denied, interruption, restart and no silent data loss |

Treat parser/fixture passes, actual GUI acceptance, producer-consumer round-trips, recovery and real-platform behavior as **different evidence levels**. A successful export command or attractive screenshot does not prove compatibility. Use LibreOffice and Microsoft Office as comparison oracles when legally available; list the versions actually tested.

### Reproducibility, license and agent review

- Compare release builds on pinned workloads, files, binaries, OS/renderer and hardware. Repeat runs; measure cold/warm startup, visible input responsiveness, frame/scroll latency, evaluation, file I/O, recovery durability, p50/p95, peak memory and failures separately. Never infer UI smoothness from a fast computational kernel or superiority from one cherry-picked workload.
- Check source-file, library, model, asset, font and linked-artifact licenses with exact revisions, provenance and distribution obligations before using them. An upstream project's website license label is not enough for imported components; GPL/copy-left combinations need explicit review before changing MIT distribution expectations.
- Coding agents must follow the **currently unlocked gate**, identify existing implementations and boundaries first, keep scoped diffs, preserve negative tests and produce handoffs with precise environment/commands, fixture hashes, observed outcomes, failures, resource data if affected, security/licensing impact, and remaining `NOT_RUN` work. Human review and existing acceptance requirements cannot be replaced by model assertions, generated dashboards or renamed failing tests.
- These rules **do not unlock** SLOPOS integration, PARAMA ports or Photo/Motion/Video/Studio/Encode. A future task is not an active task.

### First-party distribution, edge profiles and future PARAMA target

**Roadmap goal only, not permission to bypass Loom's active serial application acceptance.** Loom is an independently authored Rust/Slint suite that may become a **first-party curated SLOPOS creative suite** after actual Linux application QA, compatibility, package/provenance and licensing checks. The core 512 MB Raspberry Pi Zero 2 W SLOPOS image **must not preload** Loom or require its runtime; separately install only supported configurations. Preserve standalone Windows/macOS/Linux deliverables and the same Loom design system; do not rewrite to GTK or declare native PARAMA support.

- **SLOPOS Linux target:** real Writer/Sheets/Present GUI launches, menu bar command dispatch, drag/drop/clipboard, file associations, save/reopen, recovery, accessibility and theme/icon role bridges; validate in actual SLOPOS graphical session, not GNOME screenshots alone. SLOPOS integrates through versioned desktop adapter; core document models remain independent. Photo/Motion/Video/Studio/Encode retain Loom's own unlock gates.
- **Edge resource contract (budgets and evidence IDs in §17):** on an actual Pi Zero 2 W (4× Cortex-A53/512 MB) after AArch64/Slint/native dependencies prove compatible, test a modest text document, small workbook and compact slide deck at 720p with measured process-tree PSS, peak RSS, `MemAvailable`, input-to-visible p95, idle CPU, cancellation under slow microSD and crash/data recovery. Stream/chunk work, cap caches and concurrency. Reject oversized documents **before OOM** with loss-aware errors; preserve data fidelity and undo. Do not claim large workbooks or local generative models fit 512 MB.
- **PARAMA target:** far-future **native source port**, only after tested PARAMA user-mode process/runtime, memory/filesystem, SURYA surfaces/input/fonts/clipboard and a usable Rust/Slint rendering backend or a separately approved portability adapter. Native PARAMA support is not achieved by cross-compiling a Linux ELF, by PARAMA booting, or by a proposed Linux ABI personality. Document ABI and runtime gaps explicitly.
- **Project ownership:** only original owner-authored apps with audited imported assets qualify for bundled first-party identity. Loom remains independently maintained. Exclude `palaashatri/tricycle` and `palaashatri/texture-auto-upscaler` (GitHub forked upstream projects); optional downstream dependencies are governed by licenses, not ownership claims.
- **Shared services:** prefer mature, licensed platform adapters and efficient FFmpeg/PipeWire services where beneficial; avoid embedding duplicate transcoders, document indexers or always-on servers. SLOPOS Files metadata indexes should be optional, permissioned and disabled/bounded on edge machines. Learned design principles from Vinix/VitruvianOS are architectural only, not dependencies.
- **Proof:** each app/platform declares status as `NOT_STARTED`, `BLOCKED`, `FUNCTIONAL_WITH_LIMITATIONS` or `ACCEPTED` based on **real** OS/app/runtime/hardware measurements. No new score or milestone may be inferred from roadmap text. Do not create competing plan/truth files: update the declared focused owner document through root `AGENTS.md` and the existing workflow contracts.
