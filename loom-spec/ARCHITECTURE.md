# Loom System Architecture

## 1. Repository map and dependency direction

Loom is a set of independent repositories with a strict dependency direction.
Versioned contracts flow from the bottom up; nothing above may be depended on
from below.

```text
loom-bootstrap   orchestration: builds, tests, Docker visual QA, packaging,
                 COMPATIBILITY.toml  → depends on all other repos
                     ▲
apps: loom-writer, loom-sheets, loom-present, loom-photo, loom-motion,
      loom-video, loom-studio, loom-encode
                     ▲ depends on: loom-core, loom-vision, loom-plugin-sdk
                     │             (never on each other)
loom-plugin-sdk  plugin manifest/host/sandbox  → depends on loom-core only
loom-vision      vision core + CLI (self-contained workspace)
loom-core        shared platform crates (loom-package, loom-document,
                 loom-color, loom-jobs, loom-command, loom-history,
                 loom-text, loom-storage) — depends on nothing in Loom
```

Reference-only repositories (no runtime dependency, no code depends on them):

- `loom-spec` — this repository; product and engineering specification.
- `loom-design-bible` — visual, motion, interaction, accessibility spec.
- `loom-samples` — original sample content for every application.

Rules:

- `loom-core` depends on nothing within Loom (only external crates).
- Applications never depend on each other; cross-app exchange uses packages,
  clipboard, or shared contracts.
- `loom-vision` and `loom-plugin-sdk` must never depend on an application.
- `loom-bootstrap` is the only repository that knows every repository.
- Repo-level pinning during development: path dependencies into `loom-core`
  (see `docs/adrs/ADR-0002-Path-Based-Crate-Pinning.md`); tagged releases
  later (see `COMPATIBILITY_POLICY.md`).

## 2. Command architecture

Every user action maps to a command with a stable identifier
(`loom-core/crates/loom-command`). Commands are the single path for menus,
shortcuts, command palette, context menus, accessibility invocation, and
plugin/scripting entry. A command declares:

- stable id, enablement state, checked state, undo description, localization
  key;
- execute/revert operations used by the history system
  (`loom-core/crates/loom-history`).

The UI layer sends commands; engines are headless and never know about Slint.
This separation is specified in `docs/rfcs/RFC-0002-UI-and-Engine-Separation.md`
and keeps every engine fully testable without a display.

## 3. Job framework

Long-running work runs as jobs (`loom-core/crates/loom-jobs`), never on the UI
thread. Jobs support progress, cancellation, priority, dependencies, error
reporting, retry where safe, and cleanup. Model and media work must be
cancellable and observable; cancellation feedback must appear immediately.
See `docs/rfcs/RFC-0008-Async-Job-Framework.md`. Jobs are the mechanism behind
autosave, recovery, export, transcoding, indexing, thumbnails, and model
inference.

## 4. Storage and package format

All Loom documents are ZIP packages with a versioned `manifest.json`
(`loom-core/crates/loom-package` implements manifest and ZIP layer, including
checksums and archive-bomb limits). One extension per application:
`.loomdoc`, `.loomtable`, `.loomdeck`, `.loomphoto`, `.loommotion`,
`.loomvideo`, `.loomstudio`, `.loomencode`; plugins are `.loomplugin`.
Container layout, versioning, forward compatibility, and corruption handling:
`FILE_FORMAT_FAMILY.md`; `docs/rfcs/RFC-0006-File-Package-Format.md`.

Persistence behavior per app: Writer and Sheets embed content as JSON inside
the package today; media-heavy apps (Video, Motion, Photo, Studio, Encode)
will reference external media by default with embedded assets as an option.
Autosave and recovery are specified in `docs/rfcs/RFC-0018-Autosave-and-Recovery.md`.

## 5. Rendering

- Application UI: Slint (pinned 1.17.1), a `.slint` component library in
  `loom-core`'s UI crate (`loom-ui`) consumed via `library_paths` by every
  app. The shared library and populated showcase UIs are implemented; full
  editing surfaces remain app-specific work. See
  `docs/rfcs/RFC-0003-Slint-Integration-Model.md`.
- Custom GPU rendering (canvas, timeline, grid) will use `wgpu` with Vulkan as
  the primary Linux target (`docs/rfcs/RFC-0004-GPU-Renderer.md` — not yet
  drafted; GPU work is not started).
- Deterministic rendering for tests and visual QA: Slint software renderer in
  headless mode with a custom platform, screenshots captured and compared
  against baselines committed in `loom-design-bible`
  (`docs/rfcs/RFC-0015-Visual-Regression-System.md`,
  `docs/adrs/ADR-0003-Headless-Screenshots.md`).
- Color: sRGB pipeline first in `loom-core/crates/loom-color`, ICC/BTO later
  (`docs/rfcs/RFC-0013-Color-Management.md`).
- Text: Slint text rendering plus a shaping/layout architecture decision in
  `docs/rfcs/RFC-0005-Text-Shaping-and-Layout.md`; paginated mode layout is
  NOT_STARTED.

## 6. Vision provider model

Loom Vision exposes capabilities, not models. `loom-vision-core` defines
`CapabilityId`, `ProviderDescriptor` (capability, input/output schema, media
formats, languages, memory, latency, backends, license, provenance,
determinism, batch/streaming/cancel/progress), `CapabilityProvider`
(Send + Sync), `RunContext` (cancellation and progress), and a
`ProviderRegistry`/`CapabilityRegistry` with best-provider selection. Model
packs install from files with checksum validation. Implemented reference
providers: QR decode, image statistics. See
`docs/rfcs/RFC-0010-Vision-Provider-Model.md`, `RFC-0011-Model-Pack-Format.md`,
and `../loom-vision/ARCHITECTURE.md`.

## 7. UI/engine separation (normative)

All application engines are headless libraries with CLI harnesses, and each
currently has a limited Slint showcase (Writer, Sheets, Present, Photo,
Motion, Video, Studio, and Encode). Engines own the document model,
persistence, and logic; the Slint UI consumes the current foundation through
callbacks, while a command-driven editing layer remains future work. This keeps unit,
property, fuzz, and integration tests display-free and deterministic, and
makes Docker visual QA feasible without GPUs.

## 8. Security posture

Path traversal protection and checksum verification in package and model-pack
readers; archive extraction limits (entry count, total size, compression
ratio) to prevent archive bombs; plugin installation validation and permission
checks in `loom-plugin-sdk`; no `unsafe` without justification, safety
comments, tests, and Miri where applicable. See `RELEASE_CRITERIA.md` and
`../loom-bootstrap/` quality gates.
