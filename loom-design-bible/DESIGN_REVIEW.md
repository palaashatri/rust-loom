# Design Review

The review process for any change to the design contract (this repository)
or to application UI (implemented against it). Reviews are checklists, not
opinions; every item is verifiable.

## 1. When a review happens

* Contract change: any edit to this repository (tokens, components,
  motion, accessibility, windows, surfaces) — reviewed before merge.
* Feature UI: any user-visible change in an application — reviewed by the
  app's design owner against the checklist below before CI's visual gate.
* New component or state: reviewed against `COMPONENTS.md` state matrices
  before implementation starts (design review precedes code).

## 2. Review checklist

**Contract integrity**

- [ ] Token values match `tokens/loom.toml` exactly (names, values, order).
- [ ] No new token names without the ADR path; no bespoke durations,
      easings, sizes, or colors.
- [ ] `DESIGN_TOKENS.md` and `THEMING.md` agree with the TOML; docs list
      every token they reference.
- [ ] Contrast floors verified (CI gate) for any new color or color pair.

**Visual and layout**

- [ ] Spacing drawn from the scale; no arbitrary values.
- [ ] Fixed chrome heights honored (toolbar 40, sidebar 240, inspector
      280, status 28; timeline header 160, ruler 24).
- [ ] No shadows outside `shadow-popover`; depth by color.
- [ ] Type from the scale; tabular figures for data; line length within
      45–75 ch where text is body-like.
- [ ] Iconography: 20 px grid, 1.5 px stroke, no text glyphs, labels
      present.

**Interaction and motion**

- [ ] Direct manipulation first; disclosure order respected.
- [ ] Motion uses tokens; animation answers a question; interruptible;
      reduced-motion behavior specified (default: opacity 120 ms).
- [ ] Selection visuals per `SELECTION.md` (accent 2 px outline +
      overlay); focus ring visible.
- [ ] No anti-patterns from `ANTI_PATTERNS.md` (scan the list explicitly).

**Accessibility**

- [ ] Full keyboard path (Tab order, arrows, Esc, Enter semantics).
- [ ] `accessible-description` on every control; state announced.
- [ ] Focus order and focus return specified.
- [ ] Text scale 1.25/1.5 layout check; high-contrast behavior specified;
      non-color status.

**Truthfulness and performance**

- [ ] Progress is determinate where possible; cancellation path exists.
- [ ] No UI-thread blocking in the feature's critical path; async work
      specified.
- [ ] Empty state and error state specified; no silent failures.

**Verification plan**

- [ ] Unit/property tests named; visual-QA captures listed (default,
      selected, error, reduced-motion); perf budgets named.

## 3. Process

1. Author completes the checklist with evidence (screenshots, tests,
   token diffs) — claims without evidence are treated as unverified.
2. Review by the design-system lead (contract changes) or the app design
   owner (feature UI). A second reviewer is required for any accessibility
   impact.
3. Violations are fixed or explicitly waived via ADR. A waived item stays
   listed in `KNOWN_LIMITATIONS.md` of the app.
4. Visual gate runs in Docker; the review closes only with a green gate
   for the changed captures plus its themes.
5. Every review result is recorded (short review note appended to the PR):
   verdict, checklist items, evidence links, follow-ups with IDs.

## 4. Review cadence

* Design reviews happen at design time (before implementation) and at
  merge time (verification) — never only after implementation.
* Suite-wide design reviews (cross-app consistency pass) run per release
  cycle: compare the same component across all eight applications in the
  gallery, reconcile drift, update this checklist if the review found a
  gap in it.
