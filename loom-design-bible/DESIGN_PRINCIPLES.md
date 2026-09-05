# Design Principles

Twelve principles govern every Loom design decision. Each has a rationale and
anti-examples — concrete things that violate the principle. If two principles
conflict, lower-numbered principles win, except that **principle 8
(accessibility) always wins.**

## 1. Content first

The document, canvas, spreadsheet, timeline, or composition is the product.
Chrome exists to support the work, not to display itself.

*Rationale:* Professionals spend hours inside one surface. Every pixel of
chrome is time stolen from content. Calm interfaces earn trust because the
work is always the most prominent thing on screen.

*Anti-examples:* A ribbon permanently occupying a third of the window height;
toolbar rows that wrap when the window narrows; a sidebar that cannot be
collapsed; decorative headers on panels; watermark backgrounds behind content.

## 2. Calm over busy

Reduction until further reduction would cost capability, then progressive
disclosure for what remains.

*Rationale:* Visual noise increases decision time and error rate. A quiet
interface signals mastery; a noisy one signals fear of missing features.

*Anti-examples:* Every tool visible at once; status badges on top of badges;
persistent scroll indicators; pulsing "new" dots everywhere; dense borders
around every control; gradients and drop shadows as decoration.

## 3. Direct manipulation first

When the user can point at an object and change it, they should: drag, resize,
scrub, nudge, rotate, reorder. Properties belong in the inspector; the
essential shape of the action belongs on the object.

*Rationale:* Direct manipulation is the fastest path from intent to result and
the strongest mental model. It is also how experts judge a tool's quality.

*Anti-examples:* A color picker that requires opening a dialog to change a
fill; resizing only via numeric fields; reordering only via up/down buttons;
no drag feedback; invisible handles that only appear on hover.

## 4. Progressive disclosure

Six layers, in order: (1) direct manipulation, (2) context toolbar, (3)
contextual inspector, (4) menus and command palette, (5) advanced workspace or
panel, (6) scripting and plugins.

*Rationale:* A beginner must be able to produce useful work immediately; an
expert must reach any feature in seconds. Layering lets both happen without
either paying for the other.

*Anti-examples:* Dumping every feature into the first layer; burying a common
feature in the fifth layer; the same feature exposed in three layers at once;
modal dialogs used as a disclosure mechanism.

## 5. Predictability across the suite

Eight applications, one behavior. Same shortcut does the same thing; same
component looks and behaves identically; same gesture has the same result.

*Rationale:* Users move between Writer and Motion. Muscle memory built in one
application must transfer. Predictability is the cheapest power users have.

*Anti-examples:* `Cmd+S` meaning something different in two apps; two
differently-styled checkboxes; three applications with three different undo
models; per-app inspector layouts that contradict the suite convention.

## 6. Truthful feedback

Every action produces observable, accurate, timely feedback: hover states,
press states, progress, completion, cancellation, error.

*Rationale:* Truthful feedback is the foundation of trust. A user must never
wonder whether their click registered or whether a save completed.

*Anti-examples:* Fake progress bars; instant "export complete" before the
file is flushed; a disabled button that gives no reason; destructive actions
that complete silently; hover states with no press state.

## 7. Motion with meaning

Every animation answers a usability question: where did it come from? where is
it going? is the state changing? is it done? An animation that answers no
question is decoration.

*Rationale:* Motion encodes continuity and hierarchy — it is cognition, not
embellishment. The motion grammar (see `MOTION.md`) makes the answer legible
and consistent.

*Anti-examples:* Bounce-in logos; rotating progress spinners where a value
matters; slides that animate content in after a 400 ms delay; everything
pulsing on hover; modal windows that fly in from off-screen.

## 8. Accessibility is release-blocking

Full keyboard navigation, visible focus, screen-reader labels, logical focus
order, high contrast, scalable UI, reduced motion, non-color status, and
configurable shortcuts are requirements, not enhancements. This principle
always wins over aesthetics, and aesthetics must accommodate it.

*Rationale:* A suite for professional creative work is used by everyone. An
editor inaccessible from the keyboard is not a professional tool. This
principle wins over principle 1–7: a beautiful surface that fails it is
defective.

*Anti-examples:* A timeline only usable with a mouse; focus rings suppressed
for "cleanliness"; color-only status dots; text that cannot scale to 1.5×
without breaking; animations that cannot be disabled.

## 9. Professional depth, never featurelessness

Minimal does not mean shallow. Every professional workflow exists — pagination,
compound clips, pivot tables, color management, comping — and is reachable
through disclosure. Depth is hidden, never absent.

*Rationale:* Minimal-without-depth is a demo; depth-without-disclosure is a
dungeon. The suite must be a tool professionals keep, not a toy they abandon.

*Anti-examples:* An "easy" mode that cannot do the real job; hiding destructive
but necessary controls entirely; an empty panel labeled "Advanced" with nothing
in it; keyboard shortcuts that only exist in menus nobody opens.

## 10. Performance is a feature

Input feedback within one frame; 60 fps interaction; zero UI-thread blocking;
no synchronous file, decode, or inference on the UI thread. Budgets in
`PERFORMANCE.md`.

*Rationale:* Latency breaks flow. A tool that stutters reads as broken even
when the output is correct; cancellation that takes seconds reads as a lie.

*Anti-examples:* Autosave freezing the UI; thumbnails generated on the main
thread; scrolling that allocates per frame; an un-cancellable export; progress
that only updates when the work is done.

## 11. Warmth without decoration

The palette is warm (canvas `#FAF9F7`, terracotta accent `#B4552D`), type
feels human, spacing breathes. Warmth comes from material choices, not from
decorative flourish.

*Rationale:* Warm neutrals are approachable; they photograph, print, and age
well. Decoration is what makes software look dated; material warmth is what
makes it timeless.

*Anti-examples:* Skeuomorphic leather or wood textures; gradients layered on
every button; cartoon mascots; saturated "friendly" color spam; emoji as UI.

## 12. Every default is a deliberate choice

Default document settings, default zoom, default shortcuts, default colors,
default window sizes: each is designed and documented for the target workflow.

*Rationale:* Most users never change a default. Defaults are the most powerful
design decisions in the suite, and delegated defaults are how suites drift.

*Anti-examples:* "Default zoom 100% because it's round"; a default paper size
nobody uses; keyboard shortcuts assigned by proximity instead of by convention;
sample content that exists only because a placeholder was needed.
