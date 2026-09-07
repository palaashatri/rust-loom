# Iconography

The original Loom icon family. All icons are drawn for this suite; no
proprietary symbol sets, no traced commercial icons, no OS glyph sets.

## 1. Grid and geometry

* **ViewBox**: 20 × 20 px (`icon-viewbox-20`). All icons are authored on this
  grid and rendered at integer multiples of 20 (20, 40, 60… at 1×, 2×, 3×).
* **Stroke**: 1.5 px (`icon-stroke-width-15`), round joins and caps, uniform
  across the family. Filled icons are the exception and must be approved
  (e.g. status dots are 4 px fills, never strokes).
* **Corner radius**: 2 px (`icon-corner-radius-2`) for shapes with corners
  (e.g. tool-tile, photo frame).
* **Optical alignment**: no ink within 1.5 px of the viewBox edge on any side;
  vertical/horizontal visual centering is by optical mass, not bounding box.
  Alignment guides at the 20, 40, 60, 80, 100% marks of the grid (i.e. at
  4, 8, 12, 16 px) for consistent placement of common elements.
* **Stroke width on interaction**: icons never change stroke weight by state;
  states are color, rotation (spinner only for pending), and check overlay.

## 2. Style rules

* Geometric, not sketched: icons are built from lines, arcs, and rectangles
  with rational centerlines; no freehand shapes, no gradients, no drop shadows
  on icons.
* Optical weight is uniform: a toolbar icon at 20 px should not read heavier
  than its neighbor. Design-time check: at 16 px rendering (0.8×), all icons
  must remain legible (test in gallery snapshot set).
* Metaphor discipline: use the suite's own metaphors where they exist (Loom
  canvas, loom threads in empty states); keep generic metaphors (open, save,
  undo) conventional so users recognize them instantly.
* Localization: icons carry no text (no letters in icons) except where the
  glyph is universal (e.g. play triangle, print glyph is avoided — use a
  printer-shape icon instead of the letter glyph).

## 3. Required icon inventory

Shared (all applications): new, open, save, save-as, export, import, undo,
redo, cut, copy, paste, delete, duplicate, search, filter, zoom-in,
zoom-out, zoom-fit, full-screen, help, settings, shortcuts, commands, close,
minimize, maximize, restore, collapse-panel, expand-panel, back, forward,
chevron-down/left/right/up, overflow-ellipsis, menu, more, history, lock,
unlock, warning, error, info, success, spinner, link, unlink, external,
plus, minus, reset, refresh, check, check-circle, eye, eye-off, grid,
list, sort-ascending, sort-descending, pin, pin-off, window, tabs, alert,
bookmark, star, trash, folder, folder-open, file, file-text, calendar, clock.

Writer: paragraph, character-style, page, section, column, table, insert-row,
insert-column, footnote, endnote, header, footer, toc, citation, comment,
track-changes, hyphenation, text-wrap, master-page, template, mail-merge,
field, cross-reference, image-anchor.

Sheets: cell, formula, function, sum, average, filter-row, freeze-pane,
merge-cells, split-cells, sort, pivot-table, chart-bar, chart-line,
chart-pie, conditional-format, data-validation, named-range, goal-seek,
audit-trace, error-trace.

Present: slide, new-slide, layout, theme, master-slide, transition,
animate, presenter-mode, rehearsal, notes, align, distribute, group,
ungroup, order-front, order-back, guides, shapes, equation.

Photo: crop, rotate-ccw, rotate-cw, flip-h, flip-v, brush, eraser, heal,
clone, gradient, eyedropper, levels, curves, exposure, white-balance, mask,
selection-rect, lasso, magic-wand, perspective, liquify, warp, layer,
layer-group, adjustment, opacity, blend-mode, before-after.

Motion: composition, keyframe, keyframe-hold, keyframe-ease, playhead,
loop, ping-pong, play, pause, stop, record, parent, constrain, path,
bezier, camera, light, particle, replicate, graph-editor, motion-blur,
render-queue.

Video: timeline, blade, ripple, roll, slip, slide, trim-in, trim-out,
overwrite, insert, connected-clip, compound-clip, multicam, sync, proxy,
optimize, transcode, waveform, caption, subtitle, scene-detect, track,
audio-role, title, generator, stabilize, color-wheel, scopes, luts.

Studio: note, piano, drum, metronome, tempo, marker, loop-region, record-arm,
solo, mute, fader, pan, send, bus, sidechain, plugin, instrument, sampler,
automation, comp, take, flex-time, pitch, mixer, master, loudness, key,
scale, chord, drummer.

Encode: queue, job, preset, pause-job, resume-job, retry, format, codec,
bitrate, frame-rate, resolution, hdr, subtitle-track, audio-map, watch-folder,
cli, hardware-accel, software-fallback, quality-meter, comparison.

Vision: scan, ocr, document-detect, barcode, qr, face, pose, segment,
matting, track-point, plane, flow, search-image, model-pack, model, provider.

## 4. Icon states and use

* Toolbar and tool icons: 20 px, default `color-ink-secondary`, hover and
  active `color-ink-primary`, selected/tool-active `color-accent-default`.
* Status icons: 16 px where inline, 20 px where standalone; always paired
  with text per `COLOR.md` §6.
* Menu icons: 16 px.
* Disabled icons: 40% opacity of their default color (never a different color).

## 5. Accessibility

* Every icon has an `accessible-description` (via its control's label or a
  dedicated accessibility name). Purely decorative icons are marked
  decorative and skipped by the screen reader.
* Toolbar icon buttons must have a visible tooltip name (label) and shortcut
  hint; the tooltip appears within 400 ms of hover and is keyboard-reachable
  (focus shows the same hint in the status bar).
* Icon-only controls must never be the only path to a command: the same
  command exists in a menu or the command palette with a text name.

## 6. Source and review

* Icons are authored as editable SVG sources (20 px grid), committed in
  `loom-core`'s asset crate with per-icon test fixtures.
* Every icon ships with a snapshot test (render at 20 px and 16 px, software
  renderer) and a label-convention lint (accessibility name present, no text
  glyphs).
* New icons are reviewed against §1–§2 by the design-system lead before
  merging; a new icon without a snapshot test is not accepted.
