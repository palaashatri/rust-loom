# Accessibility

Loom Vision is a library plus a CLI. Accessibility requirements therefore
apply to the CLI's human interface and to the library's API ergonomics.

## CLI (accessible by design)

- **Pure text I/O.** All output is plain text on stdout; errors go to
  stderr. Works with any screen reader, braille display, or terminal
  customisation; no colours, icons, or mouse are used or required.
- **Exit codes** communicate success (0) vs runtime error (1) vs usage
  error (2) — scriptable and assistive-technology friendly.
- **`help` command** documents every subcommand inline; no external
  documentation required to operate.
- **No flashing or animation** — there is none.

## Library ergonomics

- All public items have documentation comments (`missing_docs` is
  enforced); errors implement `Display` with human-readable messages.
- Long-running work is cancellable (`RunContext::cancel`) and reports
  progress, so UI layers built on Loom Vision can offer interruption and
  progress feedback to users who need it.

## Non-goals for 0.1.0

- A GUI (comes later with Slint-based Loom applications, where full
  keyboard navigation, focus, and reduced-motion will be required).
