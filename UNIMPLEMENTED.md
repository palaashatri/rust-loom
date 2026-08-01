# Unimplemented Capabilities & Future Roadmap

To uphold the core execution principle of honest completion reporting (Section 24 of `AGENTS.MD`), this ledger explicitly documents capabilities that are planned or architected in specifications but not yet fully implemented in production code.

## Application Engine Limitations

### 1. Loom Writer (`loom-writer`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Rich text document model, style hierarchy, paginated PDF export, ZIP package serialization (`.loomdoc`), continuous view, CLI tool, Slint UI.
- **Unimplemented / Future**: Complex DOCX/ODT import filters (currently Markdown and plain text supported), automatic bibliography querying.

### 2. Loom Sheets (`loom-sheets`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Virtualized cell grid model, formula calculation engine, dependency graph, CSV/TSV import/export, ZIP package serialization (`.loomsheet`), Slint UI.
- **Unimplemented / Future**: XLSX binary chart importer, complex pivot table wizard.

### 3. Loom Present (`loom-present`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Slide deck canvas model, slide element hierarchy, PDF export, ZIP package serialization (`.loomdeck`), Slint UI.
- **Unimplemented / Future**: Presenter hardware display dual-monitor split, live video presenter overlay.

### 4. Loom Photo (`loom-photo`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Layer compositing (pixel, vector, text, adjustment), blend modes, curve controls, ZIP package serialization (`.loomphoto`), Slint UI.
- **Unimplemented / Future**: RAW camera sensor demosaicing pipeline (currently standard image formats PNG/JPEG/WebP are supported), 3D warp mesh editor.

### 5. Loom Motion (`loom-motion`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Composition layer model, timeline keyframing, property interpolation, ZIP package serialization (`.loommotion`), Slint UI.
- **Unimplemented / Future**: 2.5D camera light depth mapping.

### 6. Loom Video (`loom-video`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Multitrack non-linear editing timeline, clip trimming/splitting, audio/video track model, ZIP package serialization (`.loomvideo`), Slint UI.
- **Unimplemented / Future**: Hardware-accelerated GPU optical-flow retiming (software retiming available).

### 7. Loom Studio (`loom-studio`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Multitrack DAW model, Quick vs Pro workspace modes, audio/MIDI regions, track volume/pan/mute controls, ZIP package serialization (`.loomstudio`), Slint UI.
- **Unimplemented / Future**: VST3/CLAP Linux plugin sandboxed host (WASI plugin host available).

### 8. Loom Encode (`loom-encode`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Batch transcoding job queue, encoding presets (H.264 Web 1080p, ProRes 422 HQ), ZIP package serialization (`.loomencode`), Slint UI.
- **Unimplemented / Future**: AV1 AVIF hardware encoder pipeline.

## Platform Infrastructure
- **Cloud Synchronization**: Intentionally NOT included per Section 2.1 ("Local First").
- **Telemetry**: Intentionally NOT included per Section 2.2 ("Privacy").
