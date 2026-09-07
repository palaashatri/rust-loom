# Anti-Patterns

Twenty-two named anti-patterns. Each is a defect regardless of how good it
looks in isolation. Reviewers reject them; implementers never introduce them.

1. **The Ribbon** — a permanently expanded multi-row strip of every command.
   *Example:* 40 buttons across three rows, toolbar height 120 px. *Why it
   fails:* content loses; nothing is discoverable because everything is
   visible. *Instead:* single-row contextual toolbar (`TOOLBARS.md`).

2. **Modal Dialog Abuse** — dialogs for properties, formatting, or
   confirmations of undoable actions. *Example:* a "Text Color" dialog with
   OK/Cancel. *Instead:* inspector + toolbar; dialogs only for
   consequential decisions (`DIALOGS.md`).

3. **Hover-Only Functionality** — features that exist only while the pointer
   hovers. *Example:* row action buttons that vanish without hover; sidebar
   auto-peek. *Fails:* keyboard users, touch/pen, focus-based access.
   *Instead:* actions appear on hover AND focus, and exist in menus/palette.

4. **Tiny Unlabeled Icons** — 16 px icon-only buttons with no tooltip,
   no accessible name, no menu equivalent. *Instead:* ≥ 44 px targets,
   tooltips with name + shortcut, `accessible-description`
   (`POINTER_AND_PEN.md`, `ACCESSIBILITY.md`).

5. **Decorative Animation** — motion that answers no usability question.
   *Example:* logo bounce, buttons that pulse on hover, entire dialogs that
   fly in. *Instead:* the motion grammar (`MOTION.md`) — every animation
   maps to a token and a question.

6. **Destructive Default Button** — Enter activates "Delete permanently"
   instead of Cancel. *Instead:* safe path is always the default
   (`DIALOGS.md` §3).

7. **Color-Only Status** — green/red dots with no text or icon. *Fails:*
   color blindness, high contrast, screen readers. *Instead:* text + icon +
   color (`COLOR.md` §6).

8. **Invisible Focus** — focus rings removed "for polish". *Instead:*
   focus is always visible (`ACCESSIBILITY.md` §2). Release-blocking.

9. **Mouse-Only Surfaces** — a timeline or grid with no keyboard path.
   *Instead:* complete keyboard model per surface (`KEYBOARD.md`).

10. **UI-Thread Blocking** — synchronous open/save/decode/thumbnail on the
    main thread. *Example:* opening a project freezes the window for 4 s.
    *Instead:* jobs with progress and cancellation (`PERFORMANCE.md` §2).

11. **The Fake Progress Bar** — indeterminate bar where a determinate value
    exists, or progress that jumps 0→100 at the end. *Instead:* truthful
    determinate progress or, when unknowable, phase text
    (`COMPONENTS.md` §18).

12. **Uncancellable Work** — long operations with no cancel path.
    *Instead:* every long job is cancellable and announces it
    (`NOTIFICATIONS.md` §6).

13. **Stealing Focus** — background completion popping a window to front or
    moving the caret. *Instead:* status bar + toast
    (`WINDOWS.md` §5, `NOTIFICATIONS.md`).

14. **The Empty Dead End** — an empty state with no action path. *Example:*
    "No projects" with no "New Project" button. *Instead:* EmptyState always
    has an action (`COMPONENTS.md` §14).

15. **Silent Failure** — an action that fails without feedback. *Example:*
    export writes a partial file and reports success. *Instead:* truthful
    completion with atomic writes and error reporting
    (`NOTIFICATIONS.md` §4).

16. **Nested Scroll Traps** — scroll regions inside scroll regions inside
    scroll regions. *Instead:* one scrolling column per panel
    (`SIDEBARS.md` §3).

17. **The Confetti Welcome** — onboarding that demands attention before the
    user can work. *Instead:* immediate, useful default document; help is
    on demand.

18. **Per-App Drift** — the same component re-implemented differently per
    app (checkbox styles, inspector layouts, spacing). *Instead:* shared
    components and tokens; drift is a review-blocking defect
    (`AGENTS.md` §3, `DESIGN_BIBLE.md` §6).

19. **Ribbon-Numbered Shortcuts** — shortcuts assigned by "next free key"
    with no hierarchy. *Instead:* the key map in `KEYBOARD.md`; assignment
    policy with conflict detection.

20. **Magnetic Drag** — dragging that eases, lags, or snaps decoratively.
    *Instead:* 1:1 pointer-follow with zero lag; snap only as drop-assist
    (`DRAG_AND_DROP.md` §1–2).

21. **The Gradient Everywhere** — gradients, glows, and drop shadows used
    for "depth". *Instead:* depth by color steps; `shadow-popover` only for
    popovers (`DESIGN_TOKENS.md` §10).

22. **Silent Autosave Failure** — autosave failing while the UI pretends
    otherwise. *Instead:* autosave is observable in the status bar and
    failures are errors (`NOTIFICATIONS.md` §6).

Review guidance: if a design contains any of these, it is rejected without
argument. If a workaround is needed, it must be an ADR — not an exception
smuggled into a component.
