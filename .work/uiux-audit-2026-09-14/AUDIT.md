# Loom UI/UX audit — 14–15 September 2026

Completed a visual audit of all eight current-source desktop applications, plus native Sheets and Writer interactions. **The inspected experience is not ready for acceptance:** it includes unsafe document replacement, misleading template/preview/readiness states, inaccessible high-contrast text, and controls that become difficult to find or use. This is a functional alpha with useful foundations; passing builds do not resolve these failures.

The durable repair queue is [TRUTH.md](../../TRUTH.md); the one-card-at-a-time instructions are [AGENTS.MD](../../AGENTS.MD). There are **19 code findings, 25 UI/UX cards, and one CI coverage item**. All remain OPEN. No product repair was implemented in this audit.

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

![Sheets: Wide startup](../../.work/uiux-audit-2026-09-14/sheets/01-start-light-1440.png)

**1.2 — Compact startup.** Collapsing the inspector gives the grid more space, a useful behavior to preserve. Formatting still needs an accessible compact route; the complete alternate route was not verified. UI-02/14.

![Sheets: Compact startup](../../.work/uiux-audit-2026-09-14/sheets/02-compact-light-1024.png)

**1.3 — Command search.** Search results are readable, and export formats are named clearly. This scripted palette state does not prove initial pointer discoverability, focus return, or execution.

![Sheets: Command search](../../.work/uiux-audit-2026-09-14/sheets/03-palette-light.png)

**1.4 — Choose a template.** Cancel and Create remain visible, but right-hand previews and names are cut off. Recents duplicates a hard-coded Blank entry. Reflow full cards and use real recent choices. UI-03.

![Sheets: Choose a template](../../.work/uiux-audit-2026-09-14/sheets/04-template-chooser.png)

**1.5 — Inspect the Budget chart.** Bars have readable value labels. Total and Average are plotted alongside individual expenses with no visible source range or unit. Require an explicit range; do not automatically suppress user-selected formula rows. UI-08.

![Sheets: Inspect the Budget chart](../../.work/uiux-audit-2026-09-14/sheets/05-chart.png)

**1.6 — Inspect worksheet objects.** The selected shape and resize handle are visible. User-inserted objects legitimately overlap cells. This seeded capture does not establish native dragging, image recovery, or XLSX fidelity.

![Sheets: Inspect worksheet objects](../../.work/uiux-audit-2026-09-14/sheets/06-objects.png)

**1.7 — Dark theme.** Dark chrome retains readable document cells and selected-cell borders. The light paper surface is not itself a defect. Dark mode does not establish high-contrast correctness.

![Sheets: Dark theme](../../.work/uiux-audit-2026-09-14/sheets/07-dark.png)

**1.8 — High contrast.** Item, Amount, and Note disappear into black header fill, while the formula field still contains Item. The text exists but the chosen foreground/background pair hides it. UI-05.

![Sheets: High contrast](../../.work/uiux-audit-2026-09-14/sheets/08-high-contrast.png)

**1.9 — Native launch, isolated Linux desktop.** Fresh current-source build at 1280×800 shows Budget and no visible New/Open/Save menu. There is no global menu host in this test environment. Title/status do not identify document save state. UI-01/02/04/06.

![Sheets: Native launch, isolated Linux desktop](../../.work/uiux-audit-2026-09-14/sheets/17-current-native-start.png)

**1.10 — Type and commit audit123 into A1.** The grid confirms the edit. The native title stays Untitled and there is no visible dirty/status indicator. This is an actual pointer/keyboard interaction, not a seeded screenshot. UI-06.

![Sheets: Type and commit audit123 into A1](../../.work/uiux-audit-2026-09-14/sheets/18-current-native-edit.png)

**1.11 — Press Ctrl+N after committing.** The workbook immediately becomes blank with no save/discard/cancel decision. The formula field still paints audit123 over the empty placeholder. CODE-04 and UI-07.

![Sheets: Press Ctrl+N after committing](../../.work/uiux-audit-2026-09-14/sheets/19-current-native-new.png)

**1.12 — Try Ctrl+Z after New.** The blank workbook remains and the stale field persists. Undo does not restore the discarded Budget edit. These results were reproduced after rebuilding current source.

![Sheets: Try Ctrl+Z after New](../../.work/uiux-audit-2026-09-14/sheets/20-current-native-undo.png)

**1.13 — Open commands with Ctrl+K.** The palette works and lists shortcuts. Its New Sheet command actually replaces the workbook; name whole-file actions accurately. UI-09. Full keyboard navigation and screen-reader output were not tested.

![Sheets: Open commands with Ctrl+K](../../.work/uiux-audit-2026-09-14/sheets/21-current-native-palette.png)

**1.14 — Inspect the overflow menu.** Overflow contains Export CSV only, so it does not solve pointer access to New/Open/Save in this environment. The captured app window is complete; the black surrounding area is the private display. UI-01.

![Sheets: Inspect the overflow menu](../../.work/uiux-audit-2026-09-14/sheets/22-current-native-overflow.png)

### 2. Writer

**Health:** Blocked: selected template creates a different document; compact review and table presentation are incomplete.

**Repair references:** UI-11–14; CODE-04/05/13/17. Read the complete card in TRUTH for files, steps, and acceptance checks.

**2.1 — Wide document.** The page is visually dominant and the status bar is visible. Sample prose advertises implementation details instead of helping the user write; keep it out of Blank and label samples explicitly. UI-11.

![Writer: Wide document](../../.work/uiux-audit-2026-09-14/writer/01-1440x900-light.png)

**2.2 — Compact document.** The page remains usable, but the inspector is unavailable at this breakpoint. Do not remove a control category without another reachable route. UI-14.

![Writer: Compact document](../../.work/uiux-audit-2026-09-14/writer/02-1024x720-light.png)

**2.3 — Dark theme.** Light paper stays readable against dark chrome. Document colors remain stable here, a useful behavior. Full contrast and accessibility were not measured.

![Writer: Dark theme](../../.work/uiux-audit-2026-09-14/writer/03-1440x900-dark.png)

**2.4 — Command palette.** Commands, search field, and keyboard hints are readable. This seeded state does not verify all command enablement or focus transitions.

![Writer: Command palette](../../.work/uiux-audit-2026-09-14/writer/04-1440x900-light-palette.png)

**2.5 — Request comments/table at compact width.** The capture seeds a comment and table and requests the inspector. The breakpoint hides that panel; the table appears as Markdown text. UI-13/14.

![Writer: Request comments/table at compact width](../../.work/uiux-audit-2026-09-14/writer/05-inspector.png)

**2.6 — Template chooser at 1024×720.** All six cards fit in two rows, and the selection/action area is clear. This is better reflow than Sheets. Card identity still needs the native check below.

![Writer: Template chooser at 1024×720](../../.work/uiux-audit-2026-09-14/writer/06-chooser.png)

**2.7 — High contrast with review fixture.** Paper and body text remain readable in this captured state. The inspector is still hidden at compact width. This is not screen-reader or complete contrast certification.

![Writer: High contrast with review fixture](../../.work/uiux-audit-2026-09-14/writer/07-high-contrast.png)

**2.8 — Comments/table at wide width.** The inspector now shows the seeded comment, proving it exists. No visible anchor highlights it on the page; the populated table still shows literal pipes. UI-13/14; CODE-17 covers anchor drift.

![Writer: Comments/table at wide width](../../.work/uiux-audit-2026-09-14/writer/08-inspector-wide.png)

**2.9 — Native chooser.** Fresh current-source native launch shows the template chooser in the private display. No real user document is open in this fixture.

![Writer: Native chooser](../../.work/uiux-audit-2026-09-14/writer/09-native-start.png)

**2.10 — Select Executive Report.** The card is visibly selected. The following Create Document action should construct this report.

![Writer: Select Executive Report](../../.work/uiux-audit-2026-09-14/writer/10-native-report-selected.png)

**2.11 — Create the selected report.** The result is a letter with Your Name and Dear Recipient; status explicitly says Created letter document. Six UI positions and four callback mappings disagree. UI-12. Save/export of this result was not exercised.

![Writer: Create the selected report](../../.work/uiux-audit-2026-09-14/writer/11-native-created-letter.png)

### 3. Present

**Health:** Needs repair: navigator placement and repetitive empty inspector; reliability blockers remain.

**Repair references:** UI-15; CODE-04/16/19. Read the complete card in TRUTH for files, steps, and acceptance checks.

**3.1 — Wide deck.** The canvas is large and readable. The LTR filmstrip is on the right beside the inspector, contrary to the left-navigator contract. No element selected is repeated with empty geometry fields. UI-15.

![Present: Wide deck](../../.work/uiux-audit-2026-09-14/present/01-1440x900-light.png)

**3.2 — Compact deck.** The optional inspector disappears and the slide thumbnails remain visible, preserving basic orientation. The filmstrip is still on the right. Full compact property access was not tested.

![Present: Compact deck](../../.work/uiux-audit-2026-09-14/present/02-1024x720-light.png)

**3.3 — Dark theme.** The slide paper remains readable against dark chrome. Filmstrip and inspector placement issues persist. Toolbar icons need native tooltip/name checks before accessibility acceptance.

![Present: Dark theme](../../.work/uiux-audit-2026-09-14/present/03-1440x900-dark.png)

**3.4 — Command palette.** The modal and its entries are legible. These captures do not establish slide creation, transitions, undo, or exported deck correctness; see the separate code findings.

![Present: Command palette](../../.work/uiux-audit-2026-09-14/present/04-1440x900-light-palette.png)

### 4. Photo

**Health:** Blocked preview trust: image clipping; precision entry and compact properties need repair.

**Repair references:** UI-14/16/18; CODE-04/14. Read the complete card in TRUTH for files, steps, and acceptance checks.

**4.1 — Wide image workspace.** Image content dominates, but its right/bottom portions and selection edges are clipped inside the viewport. The source fits content against the larger stage instead of the viewport. Transform values are static labels beside sliders. UI-16/18.

![Photo: Wide image workspace](../../.work/uiux-audit-2026-09-14/photo/01-1440x900-light.png)

**4.2 — Compact image workspace.** Clipping remains and the inspector disappears. An alternate compact property route needs native verification. Canvas whitespace alone is not a defect. UI-14/16.

![Photo: Compact image workspace](../../.work/uiux-audit-2026-09-14/photo/02-1024x720-light.png)

**4.3 — Dark theme.** The artwork remains visible, but changing chrome does not solve the clipped image rectangle. Verify all corners and coordinate mapping after repair. UI-16.

![Photo: Dark theme](../../.work/uiux-audit-2026-09-14/photo/03-1440x900-dark.png)

**4.4 — Export command search.** PNG and JPEG choices are explicit and readable. A palette image is not proof of actual export, color fidelity, layer persistence, or selection manipulation.

![Photo: Export command search](../../.work/uiux-audit-2026-09-14/photo/04-1440x900-light-palette.png)

### 5. Motion

**Health:** Blocked preview trust: artwork changes with UI theme and differs from SVG output.

**Repair references:** UI-14/17/18. Read the complete card in TRUTH for files, steps, and acceptance checks.

**5.1 — Wide composition.** Layers, stage, timeline, and transforms are visible. Stage labels include type suffixes such as (Text); these are not the text written by the SVG exporter. Precise transform entry needs typed values. UI-17/18.

![Motion: Wide composition](../../.work/uiux-audit-2026-09-14/motion/01-1440x900-light.png)

**5.2 — Compact composition.** The inspector disappears and the composition-layers heading truncates. Keep a reachable compact editing route; do not infer missing timeline/keyframe features from this screenshot. UI-14.

![Motion: Compact composition](../../.work/uiux-audit-2026-09-14/motion/02-1024x720-light.png)

**5.3 — Dark composition.** Stage background and title color change with the UI theme. Source export colors are fixed, so the visible artwork cannot serve as a trustworthy output preview. UI-17.

![Motion: Dark composition](../../.work/uiux-audit-2026-09-14/motion/03-1440x900-dark.png)

**5.4 — Export command search.** Export SVG Frame describes the implemented output more honestly than promising a video render. The native keyframe, playback, and export workflow was not completed in this visual audit.

![Motion: Export command search](../../.work/uiux-audit-2026-09-14/motion/04-1440x900-light-palette.png)

### 6. Video

**Health:** Blocked setup experience; timeline zoom/scroll labeling and clip contrast need repair.

**Repair references:** UI-19/20/25. Read the complete card in TRUTH for files, steps, and acceptance checks.

**6.1 — Wide timeline.** The viewer is large, and synthetic media is explicitly labeled Offline sample, which is honest. Missing-tool guidance is truncated; zoom button captions make the adjacent scroll slider confusing. Small secondary clip text has weak visible separation from the fill. UI-19/20/25.

![Video: Wide timeline](../../.work/uiux-audit-2026-09-14/video/01-1440x900-light.png)

**6.2 — Compact timeline.** The timeline and viewer remain available, but the backend instruction is even harder to read fully. Provide readable setup details and a retry route. UI-25.

![Video: Compact timeline](../../.work/uiux-audit-2026-09-14/video/02-1024x720-light.png)

**6.3 — Dark timeline.** Clip color coding remains clear; secondary text still needs measured foreground/background contrast. No quantitative contrast ratio or screen-reader compliance is claimed. UI-20.

![Video: Dark timeline](../../.work/uiux-audit-2026-09-14/video/03-1440x900-dark.png)

**6.4 — Command palette.** Commands are readable. A source check distinguishes the single zoom slider from the separate scroll slider. Actual media decoding, playback, caption/export fidelity, and backend recovery were not exercised.

![Video: Command palette](../../.work/uiux-audit-2026-09-14/video/04-1440x900-light-palette.png)

### 7. Studio

**Health:** Needs substantial editing UX repair: duplicate transport controls and little region detail.

**Repair references:** UI-20/21/22; CODE-10. Read the complete card in TRUTH for files, steps, and acceptance checks.

**7.1 — Wide arrangement.** Tracks and regions are identifiable, but Loop and Metronome each appear twice. Audio regions show generic fills/icons/filenames without sample peaks or a useful sequence of time ticks. UI-21/22.

![Studio: Wide arrangement](../../.work/uiux-audit-2026-09-14/studio/01-1440x900-light.png)

**7.2 — Compact arrangement.** Library, arrangement, and mixer compete for width while repeated transport actions consume toolbar space. Keep the arrangement primary and verify existing mixer/inspector contract behavior during the app stage. Empty space below tracks is intentional working room.

![Studio: Compact arrangement](../../.work/uiux-audit-2026-09-14/studio/02-1024x720-light.png)

**7.3 — Dark arrangement.** Bright cyan regions are distinct; light text on them needs a contrast measurement. Device-unavailable/headless-deterministic messages belong to the capture harness, so this image cannot diagnose native device failure. UI-20.

![Studio: Dark arrangement](../../.work/uiux-audit-2026-09-14/studio/03-1440x900-dark.png)

**7.4 — Command palette.** Search and commands are readable. No real recording, playback, MIDI device, or plugin workflow was tested. Waveform recommendations require real PCM data, not decorative shapes. Native persistence loss is separately documented in CODE-10.

![Studio: Command palette](../../.work/uiux-audit-2026-09-14/studio/04-1440x900-light-palette.png)

### 8. Encode

**Health:** Blocked readiness clarity: contradictory queue status, sample paths, and crowded settings.

**Repair references:** UI-23/24/25; CODE-09. Read the complete card in TRUTH for files, steps, and acceptance checks.

**8.1 — Wide queue.** Start is correctly disabled while Encoder unavailable is shown, yet the status says Queue ready and a sample-input.mov job is queued. A large idle progress card repeats the selected job; settings tabs wrap. UI-23/24.

![Encode: Wide queue](../../.work/uiux-audit-2026-09-14/encode/01-1440x900-light.png)

**8.2 — Compact queue.** Three competing panels leave little room for job settings. Destination appears in multiple contexts and Drop destination file here gives unclear guidance. Provide one authoritative destination editor and readable setup recovery. UI-24/25.

![Encode: Compact queue](../../.work/uiux-audit-2026-09-14/encode/02-1024x720-light.png)

**8.3 — Dark queue.** The warning remains visible, but contradictory readiness and idle progress hierarchy remain. Derive labels and action enablement from the same real prerequisites. UI-23.

![Encode: Dark queue](../../.work/uiux-audit-2026-09-14/encode/03-1440x900-dark.png)

**8.4 — Command palette.** The palette is readable. The capture does not prove that all listed actions are enabled or that a real encode succeeds. The output overwrite race was reproduced separately with a controlled adapter, not a real codec. CODE-09.

![Encode: Command palette](../../.work/uiux-audit-2026-09-14/encode/04-1440x900-light-palette.png)

## Repair order and small-model handoff

First repair shared recovery/storage: CODE-01, CODE-02, CODE-18. Then recheck foundation evidence and proceed through the application sequence recorded in TRUTH. Within an app, protect data and truthful output before polishing layout. This report does not unlock later stages.

A local model should receive one card, its named functions/callers, and the applicable AGENTS rules. Each card says what fails, what files to open, the next small actions, and what result proves it works. Atomic persistence, filesystem boundaries, and subprocess concurrency still require experienced review; simplified prose is not evidence that an extremely small model can safely solve those areas alone.

## Reproduction and provenance

Final commands/results are in [capture-manifest.json](../../.work/uiux-audit-2026-09-14/capture-manifest.json) and [sheets-capture-manifest.json](../../.work/uiux-audit-2026-09-14/sheets-capture-manifest.json). [accepted-screenshots.json](../../.work/uiux-audit-2026-09-14/accepted-screenshots.json) records the exact image hashes and notes. Build logs are in `builds/`; native logs and isolated state are in `native-state/`. The native input helper is `native_input.py`, which targets only display `:99`. The audit display and its applications were closed after capture.

Additional Writer variants cover compact comments/table with inspector requested (05), compact template chooser (06), compact high contrast (07), and wide comments/table with inspector requested (08). The app supports `--comment`, `--table`, `--inspector`, `--template-chooser`, `--theme`, and `--size` for reproducing these seeded states. Screenshot switches do not simulate a user completing those actions.

Builds used the existing temporary Rust toolchain/cache at `/tmp/loom-cargo` and `/tmp/loom-rustup`, shared target `loom-sheets/target`, two cargo jobs, and temporary pkg-config/linker paths. Studio needed ALSA development headers: the distribution package was downloaded and extracted under this ignored evidence folder, with no system installation. An earlier Sheets build failed on local dependency configuration; the corrected current-source rebuild succeeded.

```bash
cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app
loom-sheets/target/debug/loom-sheets --screenshot /absolute/output.png --size 1024x720 --theme high-contrast
python3 -m unittest discover -s loom-bootstrap/scripts -p test_audit_governance.py
python3 loom-bootstrap/scripts/audit-governance.py
```

The original [code audit](../../.work/audit-2026-09-14/AUDIT.md) records 396 existing tests passing and three new shared-recovery regressions failing as expected. Those historical test runs are not rerun or rebranded as UI validation here. Governance validation for the documentation handoff is recorded in `final-verification.json`. The authoritative findings survive removal of this ignored evidence directory because they are included in TRUTH.
