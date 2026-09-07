# UX Acceptance Checklist

Per-app acceptance checklist. An application milestone is not done until
every applicable item passes with evidence. Items are grouped; each app
fills the app-specific rows (shortcuts, surfaces) from its PRODUCT_SPEC.

## 1. Keyboard

- [ ] Every command reachable by keyboard (Tab/arrows/Enter/Esc + its
      shortcut); no mouse-only feature anywhere.
- [ ] Every shortcut in `KEYBOARD.md` works in the app; app-specific keys
      per its section behave as documented.
- [ ] Tab order is logical and stable; focus returns to the opener after
      dialogs/popovers/palette.
- [ ] No keyboard trap except sanctioned modal contexts.
- [ ] Space/Enter/Esc semantics correct in every surface (transport keys
      don't fire while typing in text fields).

## 2. Focus visibility

- [ ] Focus ring visible at every focus stop in every theme (captured).
- [ ] Ring ≥ 3:1 contrast on all adjacent surfaces; high-contrast variant.
- [ ] Focus not suppressed by mouse use; hover never replaces focus.
- [ ] Focus moves announced (screen reader) and visible within 120 ms.

## 3. Screen-reader labels

- [ ] Every control has an `accessible-description` (CI lint green).
- [ ] Groups, panels, sections, toolbars announce names and roles.
- [ ] States announced: checked, disabled, selected, expanded, value
      changes (debounced), completion, errors (live region, alert role).
- [ ] Canvas/timeline/grid expose the accessible object model per surface
      document (`ACCESSIBILITY.md` §7).

## 4. Contrast and color

- [ ] All token pairs in use pass the contrast floors (CI gate, `COLOR.md`
      §7).
- [ ] No color-only status anywhere; each status has text/icon/shape.
- [ ] High-contrast theme passes its dedicated capture set.

## 5. Reduced motion

- [ ] All animations from the grammar; no bespoke values.
- [ ] Reduced-motion mode: no translation/scale animations (asserted);
      opacity ≤ 120 ms; surfaces instant.
- [ ] Scrubbing and direct manipulation unaffected by reduced motion.

## 6. Text scaling

- [ ] UI usable at 1.25× and 1.5× (capture set); no clipped labels, no
      unusable layouts.
- [ ] Chrome heights accommodate scale (toolbar ≤ 56 px at 1.5×); canvas
      zoom independent of text scale.

## 7. Empty states

- [ ] Every empty state has a primary action and a clear explanation; no
      dead ends.
- [ ] Empty states for: no document, no media, no layers, no results,
      no recovery data, no model packs.

## 8. Error states

- [ ] Recoverable errors: toast with the failure, affected items, and next
      action; details in Diagnostics.
- [ ] Validation errors focus the offending control and state the fix.
- [ ] Fatal errors: dialog with recovery path; never a silent failure.
- [ ] Error messages in plain language; no raw codes as the headline.

## 9. Cancellation and progress

- [ ] Every long job: progress observable (status bar/jobs), cancellable,
      cancellation acknowledged within one frame, job stops promptly.
- [ ] Progress determinate where a value exists; phase text otherwise.
- [ ] Autosave silent and non-blocking; autosave failure is an error.

## 10. Unsaved-work handling

- [ ] Closing a document with unsaved changes prompts (save / discard /
      cancel) per `DIALOGS.md`; discard is never the default.
- [ ] Crash recovery: recovered documents listed with timestamps; user
      chooses keep/discard; nothing deleted silently.
- [ ] Save failure (disk full, permissions) reported with options, and
      the document remains editable with recovery available.

## 11. Selection and direct manipulation

- [ ] Selection visuals per `SELECTION.md` in all themes; multi-select,
      marquee, keyboard selection work.
- [ ] Drag/drop: feedback per `DRAG_AND_DROP.md`; keyboard alternative
      exists for every drag operation.
- [ ] Inspector reflects selection truthfully (live updates, no apply
      buttons).

## 12. Per-app specifics (fill from each app's PRODUCT_SPEC)

- Writer: page canvas chrome, caret behavior, IME composition.
- Sheets: grid navigation table, freeze panes, formula entry, cell
  announcements.
- Present: slide navigation, presenter display, rehearsal timing.
- Photo: tool shortcuts, mask overlay accessibility.
- Motion/Video: JKL transport, timeline keyboard model, marker jump.
- Studio: transport, piano-roll keyboard, mixer focus order.
- Encode: queue keyboard model, job control keys.

## 13. Evidence requirement

Every checked item ships evidence: scripted walkthrough output, CI lint
results, capture files with gate metrics, or a test name. A checklist with
unverifiable claims is not accepted as evidence of completion — mark the
item "not yet verified" and keep it open.

## 14. Exit criteria

The app milestone passes when: all applicable items above are checked with
evidence; the visual gate is green for its theme/locale/scale matrix; the
perf gates are green on the mainstream tier; and no open release-blocking
known limitations remain (`loom-spec` release criteria).
