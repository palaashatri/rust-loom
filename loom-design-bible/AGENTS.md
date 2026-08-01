# AGENTS.md — Working Rules for Loom Design Bible

These rules bind every human or automated agent working in this repository.

## 1. This is a contract repository

`loom-design-bible` defines binding requirements for eight applications and the
shared platform. Treat every sentence as normative unless it is explicitly marked
`[proposal]`, `[goal]`, or `[future]`. If a requirement is ambiguous, fix the
ambiguity in the document — do not ship ambiguity to application teams.

## 2. Token integrity is sacred

* `tokens/loom.toml` is the **single source of truth** for token values.
* `DESIGN_TOKENS.md` and `THEMING.md` must match `tokens/loom.toml` exactly —
  same names, same values, same order.
* Never change a token value in prose without changing the TOML and vice versa.
* Never invent a token in one document and omit it in another.
* Token names follow the `category-role-scale` convention (see
  `DESIGN_TOKENS.md`). Do not introduce a parallel naming scheme.
* When in doubt about which document is right: the TOML wins; fix the prose.

## 3. Consistency checking before every report

Before reporting a task complete, verify with actual tool output (grep, diff, a
small script) that:

* Every token name referenced in `DESIGN_TOKENS.md` and `THEMING.md` exists in
  `tokens/loom.toml`, with identical values.
* Every documented file in `README.md`'s document map actually exists.
* All internal cross-references (document names, section headings, token names)
  resolve.
* No two documents state contradictory values for the same thing (e.g. the
  toolbar height, the visual-QA tolerances, the accent color).

## 4. Never filler

* Every document must justify its existence: concrete rules, tables, values,
  examples, or a named process.
* No "Lorem ipsum", no vague aspirational prose, no restated section headers
  with empty bullets.
* A claim without a mechanism is a defect: e.g. "smooth animation" is a defect;
  "duration 200 ms, easing out-quad, interruptible, reduced-motion falls back to
  opacity 120 ms" is a requirement.
* If a rule is not yet decided, mark it `[proposal]` and open an ADR. Do not
  paper over it with hedged language.

## 5. Honesty in status

Classify anything not yet implemented in the product surface as:

* `[proposal]` — under consideration, not binding.
* `[goal]` — binding target with no verified implementation yet.
* `[future]` — deliberately deferred, not binding.
* Unmarked — binding requirement of this contract.

Never mark a requirement done because documentation exists. Documentation is
the contract; implementation happens in application repositories and is
verified there.

## 6. Scope discipline

* Only edit files inside this repository. Do not touch `loom-spec`,
  `loom-core`, applications, or any other repository from here.
* Reference other repositories in prose; never duplicate their contracts.
  The Design Bible defines *how the product looks, feels, and behaves*; the
  spec defines *what it does*. If a document drifts into product scope, return
  it to visual/interaction scope.
* Do not write Slint, Rust, or any code in this repository (until the gallery
  milestone explicitly adds it).

## 7. Accessibility is release-blocking

Any edit that weakens keyboard operability, focus visibility, contrast,
screen-reader labeling, text scaling, or reduced-motion support is a
release-blocking defect. See `ACCESSIBILITY.md`. When adding any interaction
rule, add its accessibility consequence in the same edit.

## 8. ADRs for real decisions

Any change to a token value, theme behavior, component anatomy, motion
grammar, or accessibility requirement must either:

1. Be consistent with an existing ADR, or
2. Get a new ADR (see `docs/adrs/ADR-0001-design-tokens.md` for format).

Silent drift is forbidden. Two agents must not edit the same contract section
simultaneously; if parallel edits would collide, coordinate through the ADR
file first.

## 9. Determinism and review

* Documents are reviewed against `UX_ACCEPTANCE_CHECKLIST.md` and
  `DESIGN_REVIEW.md` before a milestone is declared done.
* Visual QA baselines (when the gallery milestone lands) are generated only in
  the Docker visual environment at fixed 1280×800 with the software renderer —
  never on a contributor's machine, never auto-approved.
* If you fix a rendering or layout defect discovered in visual QA, update the
  relevant contract document in the same change, or file a follow-up task with
  an ID. No silent fixes.

## 10. Reporting

When a task is complete, report:

* Files created or changed (exact paths).
* Verification evidence (tool output showing token consistency, link checks,
  line counts).
* Anything intentionally not done, marked with its status class.
