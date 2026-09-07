# ADR-0005 — Minimal Internal PDF Writer for Writer/Present/Sheets Export

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Writer, Present, and Sheets require PDF export. External PDF libraries
vary in maintenance and licensing; a full rendering engine is not needed
in the initial milestones.

## Decision

- Implement a **minimal internal PDF writer** (vector drawing + text
  placement + embedded fonts, single-page streams, PDF 1.7 subset)
  shared across Writer/Present/Sheets, owned as a future
  `loom-core` crate (`loom-pdf`, NOT_STARTED).
- Scope for the initial milestone: text (with shaping output from
  `loom-text`), vector shapes, images (JPEG passthrough, PNG→Flate),
  clipping, basic transparency; no interactive features, forms, or
  encryption.
- Feature-gated and deterministic: byte-stable output for identical input
  (golden-file testable).
- Re-evaluate a mature Rust PDF library at the PDF milestone; if one is
  adopted, it must satisfy the dependency policy (license, maintenance,
  Linux support, thread safety — root directive §5) and be recorded in
  `DEPENDENCIES.md`.

## Consequences

- Small surface, full control over determinism and licenses (MIT OR
  Apache-2.0 maintained).
- Risk of gaps vs mature libraries (complex tables, RTL edge cases);
  mitigated by integration tests against PDF consumers and documented
  import/export reports where fidelity is limited (`RELEASE_CRITERIA.md`
  §1.9).

## Verification

- Golden PDF byte tests; smoke-open tests with a PDF parser in the test
  suite; visual QA renders PDF exports to images in the Docker image
  (`RFC-0015-Visual-Regression-System.md`).
