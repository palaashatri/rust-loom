# Unimplemented Capabilities & Future Roadmap

To uphold the core execution principle of honest completion reporting (Section 24 of `AGENTS.MD`), this ledger explicitly documents capabilities that are planned or architected in specifications but not yet fully implemented in production code.

## Application Engine Limitations

### 1. Loom Writer (`loom-writer`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Rich-text document model, style hierarchy, editable multiline plain-text surface, paginated PDF export, ZIP package serialization (`.loomdoc`), continuous view, CLI tool, and Slint UI.
- **Unimplemented / Future**: Full rich-text formatting controls, complex DOCX/ODT import filters (currently Markdown and plain text are supported), tables, footnotes, change tracking, and automatic bibliography querying.

### 2. Loom Sheets (`loom-sheets`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Cell grid model, formula calculation engine, dependency graph, formula/value-bar editing for the selected cell, CSV/TSV import/export, ZIP package serialization (`.loomtable`), and Slint UI.
- **Unimplemented / Future**: XLSX binary chart importer, complex pivot table wizard.

### 3. Loom Present (`loom-present`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Slide deck canvas model, slide element hierarchy, PDF export, ZIP package serialization (`.loomdeck`), Slint UI.
- **Unimplemented / Future**: Presenter hardware display dual-monitor split, live video presenter overlay.

### 4. Loom Photo (`loom-photo`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Layer metadata/model, blend-mode metadata, ZIP package serialization (`.loomphoto`), and Slint UI.
- **Unimplemented / Future**: Pixel decode and compositing, real adjustments/curves, RAW camera sensor demosaicing, standard image-format processing, and 3D warp mesh editing.

### 5. Loom Motion (`loom-motion`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Composition layer model, timeline keyframe metadata, ZIP package serialization (`.loommotion`), and Slint UI.
- **Unimplemented / Future**: Property interpolation, rendering, preview/playback, and 2.5D camera/light depth mapping.

### 6. Loom Video (`loom-video`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Track/clip metadata model, ZIP package serialization (`.loomvideo`), and Slint UI.
- **Unimplemented / Future**: Media decode/playback, media-backed trimming/splitting, export, and hardware-accelerated GPU optical-flow retiming.

### 7. Loom Studio (`loom-studio`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Track/region metadata model, ZIP package serialization (`.loomstudio`), and Slint UI.
- **Unimplemented / Future**: Audio I/O, mixing, MIDI processing, track controls beyond the model, export, and VST3/CLAP plugin hosting.

### 8. Loom Encode (`loom-encode`)
- **Implemented (`FUNCTIONAL_WITH_LIMITATIONS`)**: Queue/preset metadata model, ZIP package serialization (`.loomencode`), and Slint UI.
- **Unimplemented / Future**: Codec invocation, batch execution, pause/resume, output inspection, and AV1/AVIF hardware encoding.

## Platform Infrastructure
- **Cloud Synchronization**: Intentionally NOT included per Section 2.1 ("Local First").
- **Telemetry**: Intentionally NOT included per Section 2.2 ("Privacy").
