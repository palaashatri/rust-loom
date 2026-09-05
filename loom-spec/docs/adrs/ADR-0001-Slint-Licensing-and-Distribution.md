# ADR-0001 — Slint Chosen as UI Toolkit; License and Distribution Model

- Status: **ACCEPTED**
- Date: 2026-08-01
- Supersedes: n/a

## Context

The root product directive selects Slint for the application UI. Slint is
licensed GPL-3.0-only or under a commercial license — it is not
MIT/Apache-2.0. Loom's own code is MIT OR Apache-2.0. This must be
documented as a deliberate distribution decision, not discovered at
release time.

## Decision

- Adopt Slint (pinned 1.17.1) as the UI toolkit per the product directive
  (`RFC-0003-Slint-Integration-Model.md`).
- Loom-authored source stays MIT OR Apache-2.0 (all crates declare
  `license = "MIT OR Apache-2.0"`).
- Distributed binaries that link Slint components are GPL-3.0-covered for
  the combined work unless a commercial Slint license is obtained for the
  release. This is documented in each application's `LICENSE_POLICY.md`
  and in the release `LICENSE_REPORT.md` (`../loom-bootstrap/`).
- The Slint dependency is pinned with a lockfile; upgrades require the
  compatibility workflow (`COMPATIBILITY_POLICY.md`).

## Consequences

- The `loom-ui` crate and app binaries carry the GPL notice requirement;
  the license report must list Slint explicitly with its dual-license
  terms.
- Engines (`-core` crates) must never depend on Slint, so they remain
  MIT/Apache-2.0 and can be reused independently
  (`RFC-0002-UI-and-Engine-Separation.md`).
- Distribution variants (AppImage/Flatpak) must carry accurate license
  metadata; commercial licensing of the suite remains an upstream
  business decision, out of scope here.

## Verification

- License audit at every release checkpoint flags Slint with GPL-3.0-only
  and confirms the notice text is included
  (`RELEASE_CRITERIA.md` §1.6).
