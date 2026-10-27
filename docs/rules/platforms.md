# Loom platform and future integration policy

## 15. Future interoperability goal — Loom as SLOPOS's creative suite

**Status: FUTURE GOAL / NOT IMPLEMENTED / NOT AN ACTIVE WORKFLOW GATE.** This is an explicitly recorded long-term cross-project feature goal, not approval to edit currently locked Loom apps, advance the Sheets/Writer/Present acceptance workflow, modify architecture/toolkits, or claim SLOPOS integration. Loom remains a separate **Rust + Slint**, offline-first, cross-platform product for Windows, macOS and Linux with its own original visual identity. The linked [SLOPOS](https://github.com/palaashatri/rust-slopos) desktop is currently an unfinished **X11 + Openbox** Linux DE; its futuristic compositor and [PARAMA](https://github.com/palaashatri/parama) targets are separate proposed efforts.

### Integration and product boundary

- Make Loom **Writer, Sheets and Present** discoverable and usable as SLOPOS creative applications through an opt-in/versioned **Loom–SLOPOS desktop adapter**. Keep **Photo, Motion, Video, Studio and Encode** gated by Loom's own future roadmap and existing serial application rules; do not present those apps as ready merely because SLOPOS wants a suite.
- Preserve Loom's engines, document formats, autosave/recovery, undo/history, Slint UI implementation, accessibility and builds for all supported hosts. Integration must not make SLOPOS or PARAMA mandatory dependencies, replace Loom's toolkit with GTK/Qt, or sacrifice non-SLOPOS platforms.
- Interoperate with real SLOPOS services through a small, versioned capability interface: desktop entry/App ID and launcher, file associations/URI handoff, open/save and recent documents, shared file picker, clipboard/drag-drop, notifications, system share destinations, application lifecycle, global menus and their authoritative command IDs/enabled states, and accessibility surfaces. Prefer native Linux adapters already present rather than a second parallel action registry.
- Support SLOPOS's future **Windows XP-class desktop personalization** through a narrow versioned Slint theme/chrome adapter, not a second hardcoded Loom skin: SLOPOS **Classic (System 7/Platinum)**, SLOPOS **Modern (Sinty-inspired)**, and user-installed declarative themes where semantic mappings are actually supported. Map colors, typography, spacing, focus, icon *roles*, chrome, light/dark/contrast and reduced motion without changing document art or app commands. SLOPOS desktop **layout profiles** (Dock versus taskbar, global menu versus local menu) remain separate from Loom's visual skin; export menus/actions through verified desktop services and preserve a functioning local menu on unsupported hosts. Loom Design Bible remains the independent canonical creative-app system on every platform. SLOPOS's own design bible (`rust-slopos/docs/CONTRACT.md` §5.2) uses the same method but its own values, so the adapter maps between semantic roles rather than sharing values. Any Loom-flavored SLOPOS community theme is optional. Do not claim direct reuse of SLOPOS's native UI widgets within Slint.
- **Icon roles and source:** distinguish the SLOPOS 12-app *symbolic* Lucide SVG reference set at `rust-slopos/assets/icons/symbolic/` from future *original colorful app-brand* icons. Loom's branded Writer/Sheets/Present icons retain their own rights and identity, but may expose a SLOPOS icon-pack override and consistent 16–32 px action glyphs through a versioned adapter. Do not copy Sinty/Dock artwork; honor original upstream icon licenses and preserve system accessibility contrast.
- Expose permissioned, stable **document/action interfaces** to SLOPOS Intelligence (visible desktop automation) and local-first SLOPOS Vision (OCR, Copy Subject, visual lookup, dictation/writing tools) only when actually implemented and reviewed. Keep document-scoped consent, undo/review semantics, local-only default and no silent data export or hidden ML calls. Document text that SLOPOS Intelligence or Vision reads is data, never instructions: Loom actions run only from the user's request, and changes derived from document content are previewed before they apply (SLOPOS `docs/CONTRACT.md` §21.6). GUI/screen automation is a fallback when authorized actions aren't available.
- A later native PARAMA Loom target is **not** assumed here: that requires an independently proven PARAMA kernel, application runtime/ABI, compatible graphics/Slint backend, filesystem, font/input and system services. SLOPOS running on PARAMA does not imply Loom binary compatibility.

### Future integration gates (do not enqueue during active application repair)

| ID | Proposed acceptance |
|---|---|
| LOOM-SLOPOS-01 | Freeze versioned adapter capabilities; demonstrate fail-closed behavior on ordinary Linux without SLOPOS and correct no-op/fallback for unsupported services. |
| LOOM-SLOPOS-02 | In a disposable graphical SLOPOS Linux VM, launch Writer/Sheets/Present, open/save/reopen real files, preserve undo/recovery, test clipboard and file handoff, and prove a real visible global-menu command round trip without dropping local menus on unsupported desktops. |
| LOOM-SLOPOS-03 | Compare SLOPOS Classic, Sinty-inspired Modern and one permitted community style against documented Slint semantic-token mappings, icon roles and high-contrast/keyboard/accessibility at 1×/2×; test independent Dock/taskbar layout selection, offline fallback, and confirm appearance changes never modify exported document content or command behavior. |
| LOOM-SLOPOS-04 | With explicit user consent, invoke a supported Loom document action from SLOPOS Intelligence or a local SLOPOS Vision tool; reject unauthorized access, confirm sensitive writes, and test offline/no-upload behavior. |

**Counterparts elsewhere (pointers only; each project evidences its own gate):** LOOM-SLOPOS-01, -02 and -04 ↔ SLOPOS FUT-LOOM; LOOM-SLOPOS-03 ↔ SLOPOS FUT-UI (`docs/CONTRACT.md` §21.7.6); LOOM-EDGE-01 to -03 (§17) ↔ SLOPOS EDGE-03; LOOM-EDGE-04 ↔ SLOPOS EDGE-04 and PARAMA ARM-04; a native PARAMA Loom target ↔ PARAMA PRM-SLOPOS-04 (`parama/AGENTS.md` §5.1), which stays separate work.

Each gate requires a specifically approved future task packet, actual integration code, platform tests and evidence in this repository's **current-truth** discipline. Documentation-only entries are never product evidence; do not alter workflow contract locks or completion scores because of this section.

## 17. Edge-first efficiency: Raspberry Pi Zero 2 W reference

**Future portability and quality goal; NOT currently supported, tested or an active application release gate.** Target the Raspberry Pi Zero 2 W's **four 1 GHz ARM Cortex-A53 cores and 512 MB shared LPDDR2 RAM** as the reference constrained machine for Loom's *small, useful desktop workflows*. Loom must remain native **Rust + Slint**, local-first and cross-platform. Existing Linux x86-64, Windows x86-64 and macOS CI/acceptance obligations, serial app locks, document correctness and provenance rules remain unchanged; **AArch64/Linux on this board is a future experimental target**, not a current release claim. Device reference: https://www.raspberrypi.com/documentation/computers/zero.html.

### Nonnegotiable responsiveness and workload semantics

- **Small document baseline:** launch an empty/minimal Writer document, enter/edit/search text, open/save/reopen a modest document; create and edit small Sheets workbooks with bounded formulas and limited visible cells; open/arrange and save a small Present deck. The interface must remain responsive during native file I/O and recalculation. **These are target workflows, not verified Pi capabilities.**
- **Large workloads remain correct, not magically cheap:** one-million-cell workbooks, complicated DOCX/PPTX layouts, large images, video processing and generative models are not mandated to fit or complete rapidly inside 512 MB. Stream/import incrementally with size limits, chunked evaluation/virtualized grids, and spill-to-disk or safely reject before OOM when appropriate. The result must be semantically correct; unsupported documents get honest loss warnings/declines, not silent feature truncation.
- **UI priorities:** keep a responsive main loop, bound active layout and repaint work to visible regions, preserve accessibility, limit thumbnails/page caches and generation/thread pools, avoid eager document duplication and wasteful full-workbook snapshots. Use asynchronous operations and backpressure; user input, cancellation, undo/recovery and progress must survive slow microSD storage.
- **Capability negotiation:** no mandatory GPU, ML model, network, background transcoder or high-bandwidth compositor effects for core small-doc workflows. Optional acceleration/advanced-media work may be disabled by explicit user-visible edge profile, not secretly emulated or silently uploaded. Preserve visible modes in Loom's own Design Bible; SLOPOS theme/desktop integration (§15) must not inflate memory or require unrelated resident services.
- **Build prerequisites:** investigate Slint renderer/runtime and OS support, allocator and linked libraries, target ABI (start AArch64; armhf only if every dependency and semantics support it), fonts/input/graphics and independent packaging. A Linux ARM VM, cross-compiled ELF or source inspection does not establish native Raspberry Pi usability.

### Real-board resource acceptance (future, initially provisional)

Run a clean Pi Zero 2 W with declared Raspberry Pi OS/Linux image, 32/64-bit ABI, GPU memory/CMA reservation, screen resolution (1280×720 first), microSD medium, services, physical temperature/clocks, network, swap/zram state and other competing processes. For repeated cold/warm trials record **PSS and peak RSS** per application/process tree, total `MemAvailable`, OOM/swap pressure, CPU/idle polling, visible keystroke/render latency p50/p95, frame pacing, start time, time-to-first-usable-document, read/write throughput and crash/recovery. Distinguish external I/O latency from time during which the *GUI stops responding*.

**Initial quality hypothesis (not an accepted measured budget):** a single lightweight Loom app running alongside a documented minimal desktop should not cause OOM or sustained swap thrashing; local text insertion, selection and simple formula edits should target **p95 ≤150 ms** visible response on a warmed, otherwise idle 720p session; cancellation during slow import must restore responsiveness within a measured bounded period, with original document/recovery intact. Set actual memory caps **after measuring full OS + compositor baseline**, not by treating the device's 512 MB as available to one application. All existing million-cell and high-end platform gates stay distinct, and edge limitations never justify lowering correctness.

| Future ID | Evidence required |
|---|---|
| LOOM-EDGE-01 | Toolchain/renderer/font/native-dialog/clipboard AArch64 feasibility, minimum dependency sizes and real-board baseline; preserve other host builds. |
| LOOM-EDGE-02 | Writer small-doc and Sheets small-workbook end-to-end graphical workflows, memory/input traces, low-memory and slow-I/O recovery. |
| LOOM-EDGE-03 | Present small-deck workflow, rendering/clipboard, tested edge lazy-loading and bounded caches; documented optional acceleration limits. |
| LOOM-EDGE-04 | Repeat on actual SLOPOS Linux reference session and any later native PARAMA target only when those platforms exist; prove functionality separately, no transfer of results between environments. |

**Task scheduling:** keep the current serial acceptance workflow and `current-truth` as source of truth. No agent may rewrite a rendering engine, change Slint, add a new application, modify active test thresholds or represent this design target as a pass merely to pursue low-memory optimization. Profile the approved workload first, record a measured bottleneck, then make a small independently validated change with correctness and host-platform regressions.

## 18. 2027 readiness baseline — platforms, accessibility, supply chain and regulation

**Owner-directed review, 2026-10-25. The facts are dated and must be re-checked before each release.** This section does not unlock any application, change the serial workflow, or alter an acceptance threshold. Its work items are recorded as GOV-05 to GOV-08 in the current-truth debt list, and they follow the normal one-card discipline.

### Platform targets

- **macOS x86-64 is a sunsetting target.** macOS 26 Tahoe is the last macOS release for Intel Macs; macOS 27 runs only on Apple silicon ([MacRumors](https://macrumors.com/2026/06/03/macos-27-wont-run-on-these-macs/)). GitHub will retire its last Intel macOS runner (`macos-15-intel`) in fall 2027 ([GitHub changelog](https://github.blog/changelog/2025-07-11-upcoming-changes-to-macos-hosted-runners-macos-latest-migration-and-xcode-support-policy-updates/)). Keep building it while that runner exists. Fix only defects there; add no Intel-only work. Remove it from the release matrix when the runner retires or when the owner decides, whichever comes first (GOV-07).
- **Planned ARM targets (`NOT_STARTED`):** Windows arm64 and Linux aarch64. Linux aarch64 is also the Raspberry Pi edge route in §17. Each needs its own build, renderer, font, dialog and clipboard evidence before it is listed as supported. Cross-compiling is not support.
- **Toolchain hygiene:** Loom builds on "Rust stable" without a `rust-toolchain.toml` pin, on edition 2021, and its manifests declare Slint minimums of both 1.7 and 1.17.1 (`loom-present-app` still says `1.7`). Pin the toolchain for releases, align the declared Slint minimums, and migrate to edition 2024 as its own reviewed change (GOV-07).

### Accessibility reference

EN 301 549 V4.1.1 (adopted August 2026) applies WCAG 2.2 Level A and AA to software, and the European Accessibility Act has applied since 28 June 2025. EN 301 549 is also the usual public-procurement benchmark. Loom adopts **WCAG 2.2 AA as applied by EN 301 549 clause 11** as its accessibility design and test benchmark. This is not a conformance claim. The criteria that most affect Loom:

- **2.5.8 Target Size (Minimum):** 24×24 px, unless spacing or an equivalent control applies. `contracts/desktop-ui.toml` already sets `minimum-pointer-target = 28`, but `resize-hit-target = 6`, `trim-handle-hit-target = 12` and `object-handle-hit-target = 20` are smaller. Each needs a recorded exception (spacing, or an equivalent keyboard/typed control) or a larger target (GOV-06).
- **2.5.7 Dragging Movements:** every drag (object move/resize, trim, column resize, slide reorder) needs a single-pointer or keyboard alternative. UI-18 (typed transform values) and UI-37 (pointer-free anchored objects) are part of this.
- **2.4.11 Focus Not Obscured:** popovers, toasts and sticky headers must not fully cover the focused control.
- **3.3.7 Redundant Entry** and **3.2.6 Consistent Help:** dialogs do not ask for the same value twice, and Help stays in one place in every app.
- Slint provides platform accessibility through AccessKit. Screen-reader journeys are still verified per platform; a build that enables AccessKit is not evidence.

### Supply chain, releases and the EU Cyber Resilience Act

Loom is intended to be "safe to sell commercially" (§11). A commercial product with digital elements placed on the EU market falls under the CRA. Reporting of actively exploited vulnerabilities and severe incidents has applied since **11 September 2026**, and most other obligations apply from **11 December 2027** ([Ireland NCSC](https://ncsc.gov.ie/cra/)). Current releases are unsigned portable builds, which is acceptable for a development preview but not for a commercial release. Before any commercial release (GOV-08):

- Ship an SPDX or CycloneDX SBOM per artifact, and run a dependency advisory/license audit (for example `cargo deny`) in CI. This complements `assets.toml` for non-code assets.
- Sign the release artifacts: Windows Authenticode, macOS Developer ID signing with notarization, and signed Linux packages. Use a signed update channel if auto-update is added.
- Declare a support period and define the 24-hour/72-hour CRA reporting path. Vulnerability reports already go through GitHub private vulnerability reporting, which is enabled for this repository (Security tab → Report a vulnerability).
- Any future AI-assisted feature stays local-first (§1) and records each model's provenance and license. The owner reviews current EU AI Act transparency duties before shipping generated-content features. This is not legal advice.

