# Drag and Drop

Drag and drop is a first-class interaction in Loom: moving clips, layers,
files, assets, and rows. The rules below make it predictable, reversible,
and never the only path.

## 1. Drag feedback

* **Origin**: the dragged item lifts within 120 ms — scale 1.02 (canvas
  objects) or a lifted row (lists), opacity 0.95, `shadow-popover` for
  lifted rows; the origin leaves a ghost (ink at 20% outline) marking the
  drop-away position.
* **Follow**: the dragged representation follows the pointer 1:1 with zero
  lag and no easing; a small chip under the cursor shows the count for
  multi-drag ("3 clips"); cursor changes to `grabbing` (custom Loom grab
  cursor, `POINTER_AND_PEN.md`).
* **Snapshots**: canvas objects drag as a live-rendered thumbnail of the
  object (Photo/Motion/Video); library assets drag as their thumbnail +
  name chip; rows drag as the row itself.
* **Cancellation**: Esc during drag cancels, animating the item back
  (out-quad 200 ms); releasing outside any target cancels with the same
  return animation; nothing is modified until the drop commits.
* **Reduced motion**: lift scale and return animation are disabled; the
  drag stays 1:1 and the item snaps back instantly.

## 2. Drop targets

* Targets highlight **before** the pointer is over them when the drag
  crosses their boundary: a 2 px accent outline + accent fill at 8% over
  the target's bounds (non-color reinforcement: the target also enlarges
  its affordance by 4 px and shows a drop-role chip: "insert after",
  "replace", "merge").
* Drop roles are explicit: each target declares what a drop means — insert,
  replace, reorder, link, copy (Option/Alt to force copy), move (default
  within a project). The role is shown in the drop chip and announced.
* Invalid targets reject visually (danger-colored "not allowed" cursor,
  no highlight) — never a silent no-op.
* Snap-to-drop: when the pointer is within 12 px of a target edge, the
  dragged item snaps to the target position (visual snap, out-quad
  120 ms) — this is drop-assist, not magnetic inertia.

## 3. Reorder

* List/panel reorder (sidebar items, sheets tabs, track headers): the
  displaced rows animate out of the way (out-quad 200 ms) as the dragged
  row passes; the landing position is marked by a 2 px accent insertion
  line (not by the dragged row "hovering" ambiguously).
* Timeline reorder: clips slide for the displaced span; the insertion
  caret (accent vertical line at the snap point) marks where the clip will
  land.
* Reorder commits on release; every reorder is undoable as one step
  (reorder history is a tested contract in `loom-core`).
* Keyboard reorder alternative: cut/paste and arrow-move commands exist for
  every reorder surface (`SIDEBARS.md` §6, `CANVAS.md` §6).

## 4. Cross-app drag

* **Files**: dragging files into any app (from the file manager) opens the
  app's import path: media into media beds, `.loomdoc` into Writer,
  `.loomphoto` into Photo — the target highlights with the accepted format
  ("Open as project", "Import media"); unsupported formats reject with the
  reason chip ("No importer for .xyz").
* **Assets between Loom apps**: shared clipboard/drag contract in
  `loom-core`: dragging an image from Photo's library into Writer drops it
  as an embedded or linked image (role chosen in the drop chip: "Link" vs
  "Copy"); dragging a Motion composition into Video inserts it as a
  compound clip; dragging an Encode preset into Video's export panel
  applies the preset.
* **Link vs copy semantics**: default is Link for project-internal and
  Media-Linked (asset stays in its library, referenced); Option/Alt forces
  Copy; the drop chip always shows which will happen. Links are relinkable
  (`loom-core` media contract).
* **Cross-app drag never requires a running second app**: files are the
  transport (drag a `.loomdoc` file to the desktop = the document).

## 5. Drag and accessibility

* Drag is never the only way to accomplish a move: every drag operation
  has a keyboard equivalent or a menu/palette command
  (`ACCESSIBILITY.md` §keyboard).
* Screen readers announce drag initiation and drop results ("Moved layer 3
  after layer 5"); during drag, the drop role and target name are
  announced when focus is in the drag source's context.
* Drag initiation requires a deliberate press-move: no drag on click; a
  pure click with a 0 px drag never lifts the item (click-to-select stays
  intact).
* Multi-select drag: dragging any selected item drags the whole selection;
  the count chip announces.

## 6. Drag data and safety

* Drag payloads are validated on drop, not on drag start: dropping a
  corrupt file shows the standard import-error toast, never a crash.
* Dragging over other applications is standard OS drag (mime types per
  the clipboard contract); dragging into Loom from any app follows the
  file rules above.
* All drags are cancellable (Esc), reversible (undo), and observable
  (status bar chip shows the operation: "Moving 3 clips…").
