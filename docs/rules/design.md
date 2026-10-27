# Loom design and shared UI rules

## 5. Shared UI foundation acceptance

The shared component library must be designed and proven **before** it is adopted by any application.

The canonical entry point for new components is `loom-core/crates/loom-ui/ui/foundation.slint`. New shared components must be small, composable, token-driven, keyboard-operable, accessible, and demonstrated in the foundation gallery.

The gallery must demonstrate every reusable component across the states relevant to that component, including:

- light, dark, and high-contrast themes;
- normal, hover, pressed, focused, selected/checked, disabled, and destructive states where applicable;
- short and long labels;
- keyboard operation and visible focus;
- supported desktop viewports: 1024×720, 1280×800, 1440×900, and 1920×1200;
- text scaling at 1.0, 1.25, and 1.5 where supported by the host contract;
- right-to-left direction where layout direction matters;
- empty, loading, populated, error, and overflow states where applicable.

### Mechanical foundation gate

All of the following must pass:

- zero unintended overlap;
- zero clipped or ellipsized action/control labels;
- no inaccessible icon-only action without an accessible label and tooltip contract;
- no toolbar wrapping or horizontal scrolling;
- no application-local palette, spacing, radius, typography, or standard control geometry;
- keyboard activation for every interactive primitive;
- visible focus treatment;
- truthful disabled states;
- no placebo callbacks;
- responsive behavior at every required viewport;
- light/dark/high-contrast rendering;
- component source within code-quality budgets;
- no unlicensed visual assets.

### Visual foundation gate

Mechanical CI is necessary but **cannot certify visual quality**.

The foundation is accepted only after reviewed gallery screenshots are judged production-quality and the current-truth section records explicit human acceptance. A deterministic screenshot is not automatically a good screenshot.

Do not create or refresh a visual baseline until the represented design has been accepted. Never change a baseline merely to make a regression test pass.

## 6. Visual design rules

Loom must look like a professional desktop creative product, not an admin dashboard, demo app, component playground, web form, or collection of rounded cards.

Hard rules:

- Content/workspace is visually dominant; chrome is subordinate.
- Use one coherent hierarchy, not repeated title bars, nested cards, redundant labels, or duplicate document names.
- Standard controls use exact shared tokens. Do not invent one-off geometry.
- Action labels must remain readable; user content names may ellipsize when necessary.
- Avoid decorative borders, pills, shadows, and accent color unless they communicate hierarchy or state.
- Do not use strong zebra striping by default for spreadsheet cells, timelines, lists, or inspectors.
- Iconography must be stylistically consistent and legible at the canonical icon sizes.
- Toolbars contain only frequent commands. Rare commands belong in menus/overflow, not permanent chrome.
- Empty space is intentional workspace, not dead layout caused by a fixed-size editor surface.
- Canvas/timeline/spreadsheet status UI must not obscure editable content.
- Selection is clear but not visually louder than the content itself.
- Animations must be short, purposeful, cancellable by reduced-motion policy, and never required to understand state.

Exact geometry, token, and responsive values live in the machine-readable design contracts. Agents must use those values rather than approximate them from screenshots.

Accessibility design and review use WCAG 2.2 AA as applied to software by EN 301 549 V4.1.1 (§18): targets of at least 24 px or a recorded exception, an alternative for every drag, focus that is never fully obscured, and Help that stays in one place. Open gaps are tracked as GOV-06.

### Turn the audit into concrete design work

Apply the specific UI cards in the current-truth section, one at a time. These rules describe the required result:

- Protect work first. New/Open/Close must handle dirty documents explicitly. Save/Export/Recovery must preserve the actual content, not just show a reassuring message.
- Make basic actions findable in the app on every supported desktop. A memorized shortcut, toolbar icon, overflow button, or command palette is not a menu bar.
- Menu placement is simple: use a global menu only when the app really installs it and the desktop actually shows it. Otherwise draw a menu bar inside the app window.
  1. Never hide the local menu because a menu descriptor was created or DBusMenu data can be generated. Those facts do not prove a desktop displays a global menu.
  2. If there is no API that confirms a visible global menu, keep the local menu visible. In the current Sheets code, the native installer calls the OS menu API on macOS only; Linux DBusMenu is not connected to a desktop host.
  3. Keep the local menu connected to the same command IDs, enabled/checked state, and action handlers as the other controls. Show real commands only; hide empty menus and unsupported placeholder items.
  4. Check the local menu on a desktop without a global menu at 1024, 1280, and 1440 px. Check the global menu separately on a platform that really hosts it.
- Start with an honest empty document or clearly named example. Example data must make sense. Never start a real job from a made-up source path.
- Give the document the space promised by its contract. Close optional inspectors by default where required. Do not create a large blank panel just to fill the window.
- At narrow sizes, reflow template choices and move low-priority commands into a reachable overflow. Keep names and primary actions readable. Never solve clipping by making text tiny, hiding actions with no replacement, or making the whole toolbar scroll.
- Use foreground/background token pairs. Check selected cells, headers, fields, disabled controls, and error text in high contrast. A dark toolbar alone does not prove that the document surface has a working dark mode.
- Feedback must be visible. Render status/error state that the controller produces; keep it outside editable content. Say what failed and how to retry. Do not put developer types, placeholder paths, or internal implementation slogans in ordinary product copy.
- A preview must create the item it shows. To make a chart, ask the user for two columns and a range (for example `A1:B4`). Show that range, the value-column name, and its unit. Keep those same cells linked after Save/Open and redraw after edits or Undo. Include a Total row only when the user selected it. User-inserted objects may cover cells; temporary messages must not cover them.
- Template identity comes from a stable ID, never a card's list position. One shared descriptor must connect the displayed name, preview, and actual generated document. An unavailable template must not silently create something else.
- UI theme changes may recolor application chrome, not document artwork. Preview and export must share document geometry/content. Fit-to-window must show every image corner, with selection and pointer coordinates using the same rectangle.
- Compact windows must retain access to formatting and review controls. A hidden inspector needs an operable replacement route. Comments need visible anchors tied to their real ranges; a raw Markdown insertion must be labeled as text until visual table editing works.
- Precise properties need typed numeric entry with units, commit/cancel, and undo. Distinguish timeline zoom from scrolling. Show real sample peaks/time positions where an editor asks users to cut audio; never draw fictional media detail.
- Give each transport action one primary control. Show progress for actual jobs, derive readiness from real prerequisites, and provide a readable way to resolve missing local tools. Keep one authoritative editing location per setting.
- Keep visible keyboard focus and meaningful accessible names. Screenshots can reveal unreadable text; they cannot prove tab order, screen-reader output, or a complete keyboard workflow. Record untested accessibility work explicitly.
