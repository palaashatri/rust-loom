# Loom Feature Matrices

Single source of truth for implementation status. Status words:
`COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`,
`NOT_STARTED`, `BLOCKED`. Evidence lives in the owning repository; this file
mirrors it. Last verified against the workspace: see `ROADMAP.md` for the
revision context.

A row marked `COMPLETE` means acceptance evidence exists (tests + review).
A headless engine with a CLI but no GUI is at most
`FUNCTIONAL_WITH_LIMITATIONS` for end-user capability rows.

## 1. Shared platform (`loom-core`)

| Capability | Status | Notes |
|---|---|---|
| Package manifest + ZIP container (`.loomdoc`/`.loomtable`, checksums, security limits) | COMPLETE | `loom-package`, 19 tests |
| Block-tree document model (blocks, mutations, offsets, text) | COMPLETE | `loom-document`, 6 tests |
| Paragraph/character style value objects, style runs | COMPLETE | `loom-text`, 10 tests |
| Color types, sRGB conversion | COMPLETE | `loom-color`, 8 tests; ICC NOT_STARTED |
| Job framework: progress, cancellation, priority | COMPLETE | `loom-jobs`, 5 tests; disk persistence NOT_STARTED |
| Command identifiers and enablement | COMPLETE | `loom-command`, 5 tests; palette/UI NOT_STARTED |
| In-memory undo/redo history (transactions, coalescing) | COMPLETE | `loom-history`, 7 tests; disk-backed history NOT_STARTED |
| Storage paths + transactional temp-file writes | COMPLETE | `loom-storage`, 7 tests |
| Autosave + crash recovery browser | NOT_STARTED | spec: `RFC-0018` |
| Slint component library (`loom-ui`) | NOT_STARTED | spec: `RFC-0003` |
| App runtime, renderer, animation, media, fonts, search, settings, a11y, shortcuts, clipboard, diagnostics, test-support crates | NOT_STARTED | listed in `ROADMAP.md` |
| Visual test harness + component gallery | NOT_STARTED | Phase 2 tail |

## 2. Loom Vision (`loom-vision`)

| Capability | Status | Notes |
|---|---|---|
| Capability traits (`CapabilityProvider`, `ProviderDescriptor`, `RunContext` cancel/progress) | COMPLETE | `loom-vision-core/provider.rs`, 12 tests |
| Provider registry with best-provider selection | COMPLETE | `registry.rs`, 12 tests |
| Model-pack manifest validation, SHA-256, path-traversal protection | COMPLETE | `model_pack.rs`, 24 tests |
| QR decode (CPU reference provider) | COMPLETE | `reference.rs`, 15 tests total |
| Image statistics (CPU reference provider) | COMPLETE | same module |
| Vision CLI (headless provider workflows) | COMPLETE | `loom-vision-cli` |
| OCR, layout-aware OCR, table detection/structure | NOT_STARTED | |
| Segmentation, matting, face/pose, depth | NOT_STARTED | provider interfaces exist |
| Tracking (object/point/planar), optical flow, stabilization | NOT_STARTED | |
| Transcription, speaker diarization, audio analysis | NOT_STARTED | |
| Embeddings, similar-image/search, indexing | NOT_STARTED | |
| ONNX/Candle backends, GPU acceleration | NOT_STARTED | |
| Application integration (Photo→Sheets, Video) | NOT_STARTED | Phase 6 |

## 3. Plugin SDK (`loom-plugin-sdk`)

| Capability | Status | Notes |
|---|---|---|
| Plugin manifest schema, validation, version compatibility | COMPLETE | `loom-plugin-manifest`, 24 tests |
| Plugin store installation: safe ZIP, checksums, path safety | COMPLETE | `loom-plugin-host`, tests |
| Runtime permission checks | FUNCTIONAL_WITH_LIMITATIONS | `loom-plugin-host`, 4 tests; policy surface partial |
| WASM execution/sandboxing | NOT_STARTED | `module.wasm` stored, not executed |
| Plugin CLI | SCAFFOLDED | `loom-plugin-cli` + fixtures |
| Signing architecture, capability negotiation, resource limits, crash isolation | NOT_STARTED | |

## 4. Loom Writer (`loom-writer`)

| Capability | Status | Notes |
|---|---|---|
| Headless rich-text document model (blocks, styles, runs) | COMPLETE | `loom-writer-core`, 6 tests |
| `.loomdoc` save/load | COMPLETE | |
| Markdown export | COMPLETE | |
| Plain-text export | COMPLETE | |
| Writer CLI (create/info/export-md/validate) | COMPLETE | used by visual-QA pipeline |
| Slint GUI (document editing surface) | NOT_STARTED | engine is UI-ready by design |
| Paginated mode, master pages, headers/footers, footnotes, TOC, columns | NOT_STARTED | paginated layout: `RFC-0005` |
| Tables, change tracking, comments, citations, cross-references, mail merge, form fields | NOT_STARTED | |
| PDF export | NOT_STARTED | design: `docs/adrs/ADR-0005-Internal-PDF-Writer.md` |
| DOCX/ODT import-export, EPUB export | NOT_STARTED | |
| OCR-assisted import via Loom Vision | NOT_STARTED | |
| Local search, version snapshots, recovery browser | NOT_STARTED | |

## 5. Loom Sheets (`loom-sheets`)

| Capability | Status | Notes |
|---|---|---|
| Formula tokenizer + recursive-descent parser | COMPLETE | `loom-sheets-core`, 12 tests |
| A1 cell references, dependency-graph evaluation with topological order + cycle detection | COMPLETE | |
| CSV import/export | COMPLETE | |
| `.loomtable` content JSON round-trip | COMPLETE | `sheet_to_json`/`sheet_from_json` |
| Sheets CLI | COMPLETE | |
| Slint GUI (virtualized grid) | NOT_STARTED | |
| Incremental recalculation | NOT_STARTED | full recompute only |
| Named ranges, structured tables, sorting/filtering, validation, conditional formatting | NOT_STARTED | |
| Charts, pivot tables, grouping, freeze panes | NOT_STARTED | |
| XLSX/ODS import-export | NOT_STARTED | |
| Photograph-to-table import via Loom Vision | NOT_STARTED | needs Vision table detection |
| Goal seeking, formula auditing, error tracing | NOT_STARTED | |

## 6. Loom Present (`loom-present`)

| Capability | Status |
|---|---|
| All capabilities (canvas, themes, master slides, layouts, transitions, animations, presenter mode, PDF/video export, PPTX/ODP, vision-driven background removal) | NOT_STARTED — repository empty, no code |

## 7. Loom Photo (`loom-photo`)

| Capability | Status |
|---|---|
| All capabilities (layer stack, masks, blend modes, adjustments, brushes, RAW, color management, PSD/OpenRaster, AI-assisted selection via Loom Vision) | NOT_STARTED — repository empty, no code |

## 8. Loom Motion (`loom-motion`)

| Capability | Status |
|---|---|
| All capabilities (compositions, timeline, keyframes, parenting, tracking, optical flow, particles, render queue, template export) | NOT_STARTED — repository empty, no code |

## 9. Loom Video (`loom-video`)

| Capability | Status |
|---|---|
| All capabilities (media library, timeline editing, proxies, multicam, effects, color, captions, transcription, Encode export) | NOT_STARTED — repository empty, no code |

## 10. Loom Studio (`loom-studio`)

| Capability | Status |
|---|---|
| All capabilities (Quick + Pro workspaces, audio/MIDI, mixer, plugin hosting, score editor, source separation, export) | NOT_STARTED — repository empty, no code |

## 11. Loom Encode (`loom-encode`)

| Capability | Status |
|---|---|
| All capabilities (batch queue, presets, filters, hardware/software encoding, watch folders, CLI, quality metrics) | NOT_STARTED — repository empty, no code |

## 12. Cross-application (`CROSS_APP_WORKFLOWS.md`)

| Capability | Status |
|---|---|
| Shared clipboard formats, drag and drop, linked assets, Motion templates → Video/Present, Encode integration, Studio stems → Video, shared commands/shortcuts/components | NOT_STARTED (Phase 6) |

## 13. Sample content (`loom-samples`)

| Capability | Status |
|---|---|
| Original sample projects for all applications | NOT_STARTED — repository empty |
