# AGENTS.md — Working rules for loom-spec

Rules for human and automated agents editing this repository. Read the root
workspace `AGENTS.md` first; it governs the whole Loom effort. This file adds
repository-specific rules only.

## 1. Scope discipline

`loom-spec` is the **authoritative specification**. It is a pure
documentation repository — no code, no build system, no Cargo workspace.

- Do not add implementation code, scripts, or CI here.
- Do not duplicate contracts owned by other repositories. Reference them:
  - Visual/design contracts → `../loom-design-bible/`
  - Platform crate APIs (package, document, color, jobs, command, history,
    text, storage) → `../loom-core/`
  - Vision provider traits, registry, model-pack schema → `../loom-vision/`
    (`ARCHITECTURE.md`, crate sources)
  - Plugin manifest schema and sandbox → `../loom-plugin-sdk/`
  - Build, test, Docker, packaging, `COMPATIBILITY.toml` → `../loom-bootstrap/`
- If a document must describe an interface, describe its **purpose and
  contract**, and point to the owning crate for the exact signature.

## 2. Honesty rules

- Use only the six status words:
  `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`,
  `NOT_STARTED`, `BLOCKED`.
- Never mark a capability beyond what its evidence supports. A feature whose
  engine exists but whose GUI does not is `FUNCTIONAL_WITH_LIMITATIONS` at
  best (headless), not `COMPLETE`.
- `FEATURE_MATRICES.md` is the single source of truth for status. Prose claims
  elsewhere must not contradict it.
- When the implementation state changes, update `ROADMAP.md`,
  `FEATURE_MATRICES.md`, and affected cross-references in the same change.
- Do not describe an unimplemented feature as if it existed.

## 3. Consistency rules

- Terminology must match `TERMINOLOGY.md`. When a new term is needed, propose
  it there first; do not invent synonyms per document.
- Extensions: `.loomdoc` `.loomtable` `.loomdeck` `.loomphoto` `.loommotion`
  `.loomvideo` `.loomstudio` `.loomencode`, plugins `.loomplugin`.
- File format details must mirror `loom-core/crates/loom-package` (the
  implemented schema) and `FILE_FORMAT_FAMILY.md`. When the crate changes the
  schema, update the spec in the same effort.
- Dates, versions, crate names, and repo names must match reality. Verify
  against the workspace before writing.

## 4. RFC and ADR process

- Cross-cutting architecture changes require an RFC in `docs/rfcs/`. Smaller
  decisions use an ADR in `docs/adrs/`.
- An RFC template is fixed: Context, Goals, Non-Goals, Proposed design,
  Alternatives, Trade-offs, Security, Performance, Compatibility, Migration,
  Testing, Open questions, Final status.
- Numbering: RFCs and ADRs are numbered, never renumbered or deleted. Draft
  RFCs have `Final status: PROPOSED`; accepted ones say `ACCEPTED` (normative).
- Do not silently change an accepted contract. Amendments require a new ADR
  (or RFC) that supersedes the old one and updates its `Final status`.
- An RFC that names a specific crate/interface version must be updated when
  that contract changes.

## 5. Task format

Capabilities are delivered as tasks following the task format in the root
`AGENTS.md` (§11.2): ID, Title, Owner subsystem, Purpose, Dependencies, Files
or modules, Required behavior, Non-goals, Implementation steps, Acceptance
tests, Visual QA, Performance budget, Security considerations, Completion
evidence. Specifications written here must be granular enough that a
less-capable coding agent can implement one task without inference — purpose,
state model, data structures, error behavior, threading, persistence, undo,
accessibility, security, and acceptance criteria must all be explicit.

## 6. Editing conventions

- Markdown only. One file per concern (see `README.md` doc map).
- Keep documents in the 60–250 line range; longer material belongs in the
  owning repository's docs.
- Relative links only; never absolute paths. Link to sibling repos as
  `../<repo>/`.
- Headings are sentence case (`## Package container`), not Title Case.
- No emojis, no filler. Every sentence must carry specification content.

## 7. Verification

- Before reporting completion of an edit, re-check cross-links resolve, the
  status vocabulary is used correctly, and no fact contradicts
  `FEATURE_MATRICES.md` / `ROADMAP.md`.
- Documentation-only completion does not satisfy implementation tasks; if a
  task is unimplemented it stays visible in the task ledger, never silently
  dropped.
