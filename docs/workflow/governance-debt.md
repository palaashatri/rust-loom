# Governance and acceptance debt

These cards retain their original findings. They do not override the [active gate](current.md) or grant product acceptance.

## Engineering and acceptance debt

Oversized `main.rs`/`lib.rs` files and duplicated generic UI remain maintenance debt. Follow the byte ratchet; extract coherent responsibilities when a touched legacy file cannot grow. Do not create a parallel framework or rewrite the suite as part of one repair.

A dedicated Sheets CI job is defined in `.github/workflows/ci.yml` for formatting, Clippy, and workspace tests, with the native UI build dependencies installed. **GOV-01 · FIXED:** on pushed commit `ce2374c75fad4ec3e112b0c3a852840c39c1f928`, the job passed formatting, Clippy, and `cargo test --workspace --locked`. The separate repo-wide governance job is still red at the code-structure ratchet for four untouched files: `loom-encode/crates/loom-encode-app/src/main.rs`, `loom-video/crates/loom-video-app/src/main.rs`, `loom-video/crates/loom-video-core/src/lib.rs`, and `loom-photo/crates/loom-photo-app/src/main.rs`. That unrelated debt is not a Sheets test failure. A focused chart-range regression was also run once with the intentional range bug restored and failed on the expected XLSX range assertion; the fixed-source focused and full cached test harnesses pass. Keep unrelated release/package builds manual. Do not replace behavior checks with callback-count or screenshot-exists checks.

### GOV-02 — Reconcile design-bible prose with canonical contracts

**GOV · OPEN.** The consolidated design-bible prose repeats palette and motion values that no longer match the canonical token/desktop contracts, describes sheet tabs as 28 px while both current contracts define 30 px, and contains inconsistent reduced-motion guidance. It also gives conflicting ownership and evidence directions: `foundation.slint` owns current generic components while retained toolkit aliases still have compatibility roles, and the visual QA text conflicts with current baseline storage and threshold units.

**Scope:** Current normative design-bible prose in the `loom-design-bible/*` source sections of `docs/archive/AGENTS-ARCHIVE.md` (moved there from `AGENTS.md`), `loom-design-bible/README.md`, `loom-design-bible/contracts/desktop-ui.toml`, `loom-design-bible/tokens/loom.toml`, and the `foundation.slint`/`toolkit.slint` exports. Historical audit reports remain historical and must not be rewritten as current findings.

1. Compare each live prose claim to the canonical machine-readable contract and runtime source; replace duplicated constants with links or named token roles.
2. Resolve the sheet-tab, reduced-motion, control-ownership, baseline location, and visual-diff threshold/unit conflicts in the normative source only.
3. Add or extend governance checks for the reconciled ownership and values where mechanically testable.

**Done when:** each current design-bible rule has one unambiguous owner and source of truth, prose and machine contracts agree, and the focused audits pass without rewriting historical evidence or weakening an accessibility/layout requirement.

### GOV-03 — Restore trustworthy geometry-manifest generation

**GOV · OPEN.** `loom-design-bible/contracts/geometry-manifest.toml` names `loom-bootstrap/scripts/audit-product-ui.py` as its generator, but that script is absent. The current manifest's Sheets source closure omits the maintenance-split modules, and five of six recorded Sheets source hashes are stale. Replacing those hashes by hand would create the appearance of current geometry evidence without measuring or verifying it.

**Scope:** `loom-design-bible/contracts/geometry-manifest.toml`, its README reference, and the geometry audit/generator in `loom-bootstrap/scripts/`.

1. Either restore a deterministic generator/checker that discovers the full recursive Slint import closure, records actual measured geometry and provenance, and rejects missing/stale source or contract hashes; or explicitly retire the manifest and its README/normative references, replacing its required evidence with a maintained alternative.
2. Cover every application and shared import, including split modules, in source closure and freshness checks. Keep raw geometry observations traceable to current source and measured values.
3. Add tests for changed/missing/imported source, deterministic output, stale evidence rejection, and the selected retirement path if the manifest is removed.

**Done when:** one documented executable path can reproduce and validate current geometry evidence, or every reference to the retired manifest is removed and a validated replacement is in use. Do not hand-edit hashes or invent geometry values.

### GOV-04 — Bind visual acceptance to reviewable evidence

**GOV · OPEN.** `loom-bootstrap/scripts/audit-ui-foundation.py` accepts boolean status fields and the presence of any PNG; it does not bind approval to the reviewed source revision, contract revision, complete viewport/theme/text-scale/direction matrix, image hashes, reviewer, date, or scope. The contract's `ACCEPTED` state therefore does not by itself establish that current sources still match the human-approved visual evidence.

**Scope:** `loom-design-bible/contracts/ui-foundation.toml`, `loom-bootstrap/scripts/audit-ui-foundation.py`, its tests, and native/renderer evidence manifests.

1. Add a structured approval/evidence record with approved source and contract revision/hashes, capture manifest and image hashes, required matrix coordinates, reviewer, approval date, and reviewed scope.
2. Validate completeness and source freshness against that record; make missing or stale approval visible as unverified current coverage rather than treating arbitrary PNG presence as approval.
3. Preserve historical approval truth. Do not fabricate missing reviewer/date/source metadata, auto-refresh screenshots, or make regenerated captures count as approval.
4. Add tests for missing coordinates, wrong image hash, changed source/contract, incomplete reviewer/scope data, and a valid current record.

**Done when:** acceptance evidence is tied to the exact sources and matrix that were reviewed, the audit rejects incomplete or stale records, and the historical `ACCEPTED` fact remains clearly distinguished from current verified coverage.

### GOV-05 — Decide whether Loom's palette is an original identity

**GOV · DECIDED (2026-10-25): keep the palette and reword the identity claims.** The palette is contrast-verified (UI-33) and its baselines are human-approved. Redesigning it would force a full re-review for little user value. Loom's identity is its layout, type, iconography and interaction language. Its chrome palette is deliberately neutral, so content stays dominant. Remaining work: the GOV-02 prose reconciliation drops the terracotta and "warm" claims. A future accent change goes through step 2 below. Original finding: §1 promises an "original Loom visual identity", and the archived design-bible prose describes a warm palette with a terracotta accent. The canonical `loom-design-bible/tokens/loom.toml`, however, uses many values that equal colors from Apple's public web and Human Interface Guidelines palettes, for example `#F5F5F7`, `#1D1D1F`, `#0071E3` (light) and `#1C1C1E`, `#34C759` (dark). Color values are not artwork, so this is not an asset-license violation. It does contradict the identity claim and the bible's own language.

**Scope:** an owner decision, then `loom-design-bible/tokens/loom.toml`, `contracts/desktop-ui.toml`, `loom-core/crates/loom-ui/ui/theme.slint` and the design-bible prose (with GOV-02).

1. The owner chooses one: keep the neutral system-like palette and rewrite the identity language to match, or derive an original palette.
2. For an original palette, change TOML, contract and runtime theme together; keep every contrast floor (UI-33) in light, dark and high contrast; and re-review baselines through the normal visual gate.
3. Do not change token *names*. Do not refresh baselines without review.

**Done when:** the identity language and the token values agree, the focused audits pass, and any changed captures have human approval.

**Owner update — 2026-10-27 (bounded accent scope).** The owner explicitly authorized icon-matched shared application accents: Writer violet, Sheets jade and Present coral. This supersedes the earlier decision only for interactive accent/focus/selection roles. Neutral structural chrome and document artwork/content colours remain unchanged; high contrast retains its universal accessibility colours. Implement the same app schemes in the existing token TOML, desktop contract and shared runtime selector, with no app-local palette fork. Step 2's contrast floors and review remain mandatory, and no baseline or acceptance is promoted. The [active scope](current.md#owner-scope--2026-10-27) and [Oct27 audit](../audits/uiux-2026-10-27.md) own current scheduling and implementation evidence; this paragraph records authorization only.

### GOV-06 — Meet WCAG 2.2 target-size and dragging criteria

**GOV · OPEN.** Under the §18 accessibility reference, `contracts/desktop-ui.toml` sets `resize-hit-target = 6`, `trim-handle-hit-target = 12` and `object-handle-hit-target = 20`. All three are below WCAG 2.2's 24×24 px minimum (2.5.8). Drag-only operations also need alternatives (2.5.7).

**Scope:** `contracts/desktop-ui.toml` hit-target values, the shared `LoomCanvasHandle` and timeline components, and their callers. Product edits stay within the active application gate. Other apps' handles are recorded and fixed in their own turn.

1. For each undersized target, record either a WCAG exception that applies (spacing, or an equivalent keyboard/typed control that exists and is tested) or a larger hit area that keeps the visible size.
2. List every drag interaction per app with its non-drag alternative. Link existing cards (UI-18, UI-37) instead of duplicating them.
3. Add contract-audit checks for the minimum hit target and for the recorded exceptions.

**Done when:** every hit target is 24 px or above or has a tested, recorded exception, and every drag has a working alternative in the active app.

### GOV-07 — Retire macOS x86-64 and pin the build toolchain

**GOV · OPEN.** macOS 27 dropped Intel Macs, and GitHub retires `macos-15-intel`, its last Intel runner, in fall 2027. CI and `release.yml` still build `macos-x86_64`. Loom also has no `rust-toolchain.toml`, is on edition 2021, and declares Slint `1.7` in `loom-present-app` beside `1.17.1` everywhere else.

**Scope:** `.github/workflows/{ci,cross-platform,release}.yml`, workspace manifests, an optional `rust-toolchain.toml`, and engineering §1 and platform-policy §18.

1. Remove the `macos-15-intel` matrix entries when the runner retires or when the owner decides, and update §1 and the README in the same change.
2. Pin the release toolchain, and align the declared Slint minimums with the locked version.
3. Migrate to edition 2024 as its own change, with format, Clippy and focused tests passing.

**Done when:** the release matrix matches the supported targets, the toolchain is pinned, and the edition change passes CI on its own.

### GOV-08 — Make releases commercially shippable under the CRA

**GOV · OPEN.** Releases are unsigned portable archives without an SBOM, a declared support period or a vulnerability-reporting route. A commercial EU release needs all of these (§18).

**Scope:** `.github/workflows/release.yml`, CI dependency auditing, and the declared release reporting contact and support-period statement.

1. Generate an SPDX or CycloneDX SBOM per artifact, and add a dependency advisory and license audit to CI.
2. Sign the Windows and macOS artifacts (notarizing the macOS ones) and the Linux packages, keeping the credentials in CI secrets.
3. Record the support period here. Vulnerability reports use GitHub private vulnerability reporting, which is enabled. Define how an actively exploited vulnerability is reported within 24 hours.

**Done when:** a release candidate carries signatures, an SBOM and a stated support period, and the reporting path is written down. Until then, releases stay labelled as development previews.

### GOV-09 — Define shared rich-text semantics across Writer, Sheets, and Present

**GOV · OPEN (2026-10-27).** Rich text is implemented through different models and capability sets, without one suite-wide semantic contract. Writer stores paragraph and character styles in `loom_text::ParagraphStyle` and `StyleRun`; Sheets has worksheet `CellStyle` and table-cell `loom_objects::TextRun`; Present has its own `ElementStyle`, and its text styling is not per-run. A shared table object is a useful start, but does not make text formatting consistent across apps. Markdown is useful as an explicit import/export format; it cannot be the canonical rich-text model or a silent cross-app bridge when its subset loses formatting.

**Direction:** share the meaning and persistence of common paragraph/run formatting, while keeping document paragraphs, spreadsheet cells and slide objects in their app-specific containers. Use `loom_text`'s existing paragraph/character style model as the starting point; add adapters where a domain needs different semantics. Do not flatten formulas, cell number formats, slide layout or other app-specific structure into a generic text document.

**Scope:** `loom-core/crates/loom-text`, `loom-core/crates/loom-objects`, and the Writer, Sheets and Present persistence, clipboard and interchange adapters. This records a cross-suite design finding; it does not unlock app work or change the active `office-v1` repair order.

1. Define the common semantic fields and an explicit per-app support/loss map for paragraph and run styles, including text, font family/size, weight, italic, underline, strike, script, colour and paragraph alignment/direction where the app supports them.
2. Reconcile duplicate representations against `loom_text` for common text semantics. Keep cell number formats/table structure and slide-object geometry/type in their owning domain models.
3. Use structured data for rich-text clipboard and persistence paths. Keep Markdown as an explicitly named import/export format, and warn or refuse when a conversion cannot preserve supported formatting; never silently flatten runs.
4. Add focused persistence/reopen and cross-app copy/paste checks for the shared formatting subset, plus import/export loss checks for unsupported or app-specific features.

**Done when:** the shared semantic contract and support/loss map are documented; Writer, Sheets and Present preserve the common supported formatting through native save/reopen and the tested cross-app clipboard paths; unsupported mappings are visible; and app-specific layout/data remain intact. Each app still needs to pass its own acceptance gate.

### GOV-10 — Make open OOXML formats the primary user-facing save formats

**GOV · OPEN (owner direction, 2026-10-27).** The default native save paths currently use app-specific `.loomdoc`, `.loomtable` and `.loomdeck` files, while `.docx`, `.xlsx` and `.pptx` are separate interchange paths. This makes portable standard formats secondary in ordinary Save/Save As workflows and creates avoidable dependence on Loom-specific files. The custom packages may preserve Loom-only state, so their existence is not itself the fault; the product fault is making them the normal user-facing save path. The old binary `.doc`, `.xls` and `.ppt` formats are not the target.

**Direction:** make modern OOXML (`.docx`, `.xlsx`, `.pptx`) the primary save/open formats in the office apps, using the existing OOXML import/export implementations as the starting point. Evaluate ODF (`.odt`, `.ods`, `.odp`) as a later interoperability target. Keep `.loom*` readable during migration and only offer it as an explicitly named compatibility/full-fidelity option while OOXML cannot preserve a documented Loom feature. Do not merely change extensions or silently discard unsupported content.

**Scope:** normal Save, Save As, Open, file filters and migration for Writer, Sheets and Present, plus their OOXML persistence/interoperability paths. This is a product direction, not permission to bypass the current serial app gate or claim Office/LibreOffice parity.

1. Compare each app's native package semantics against the OOXML features it actually imports and exports; document unsupported or lossy features from executable evidence.
2. Make OOXML the suggested and normal Save/Save As format only as each app can preserve its supported content. Preserve explicit confirmation and recovery behavior when conversion may lose information.
3. Keep backward-compatible open/migration for existing `.loom*` files, with save/reopen regression coverage and no silent overwrites or dropped data.
4. Verify representative files by opening Loom exports in current Microsoft Office and LibreOffice versions and importing files authored by both products. Record product/version, feature scope and results; do not infer conformance from a successful ZIP/XML parse.

**Done when:** each office app's ordinary Save/Open/Save As round-trips its supported content through the selected open format, existing `.loom*` files remain recoverable through migration, known losses are reported before they occur, and current Office/LibreOffice interoperability evidence covers the claimed feature scope. ODF remains a separate follow-up unless explicitly brought into scope.

Asset provenance and commercial redistribution rules remain in the rules imported by `AGENTS.md` and `loom-bootstrap/contracts/assets.toml`. Fresh screenshots are evidence generated by this project, not imported product artwork. No third-party assets were added for this audit.
