# Live and manual proof — DEFERRED, awaiting the user's decision (task 10.13)

**No live launch was started by this change, deliberately and from the start.**

Slot 4 established that isolating an app's *state* does not isolate its *window*: a
real Wayland launch maps a surface and takes focus regardless of `XDG_*` isolation,
and it interrupted the user's session. Every proof this change ran is headless.

## Why this row's deferral is more consequential than the others

`.agents/rules/widget-wiring.md` names the workspace sidebar **explicitly** as the
subtree that needs a real `make run` cycle with restored workspaces while watching
stderr, and states that widget-green plus a live warning is a **failed** fix for this
subtree — not a partial success. So for `WFR-WORKSPACE-TREE`, widget-green is
**necessary and not sufficient**.

## What this change's scope actually is — re-derived, because the earlier version was false

An earlier revision of this file justified the deferral by claiming this change "did
**not** land the structural migration: no coordination-module moves, no row-factory
changes, and no widget-test changes." **All three are false of the landed scope**, and
the deferral cannot rest on a description of a change that did not ship. The basis is
re-derived here from the diff:

- **Coordination modules moved.** Three files were dissolved and eight coordination role
  modules exist where four topical siblings used to; `workspaces.rs`, `tree_loading.rs`,
  and `tree_index.rs` are gone, and six files were renamed.
- **`row_factory.rs` changed** — by module-path renames only, which is the specific thing
  that was verified afterwards precisely because the `GtkTreeExpander` internal-gesture
  disable lives there.
- **Widget tests changed extensively**: ~114 call sites rewritten onto the evidence
  surface, plus new tests for both halves of the M-4 race.

What remains **true**, and is the honest basis for the deferral: no `GtkPaned`,
`GtkRevealer`, template, or `TemplateChild` change; no allocation, measurement, or
sidebar-animation code touched. The `.blp`/`.ui` templates are untouched in the diff, and
`ui/window/adaptive_shell.rs`'s diff is confined to the width-preset path move. The live
risk this change adds is therefore concentrated in **behavior** — file operations, the
scan/refresh/watch pipeline, and the workspace-list load — rather than in the geometry
paths the `.agents/rules/widget-wiring.md` sidebar clause is written about.

## What the headless lanes did and did not prove

Stated explicitly, because "headless lanes are green" is not the same claim as the one
the rule asks for:

**Proved headless.** Every behavior above under `mutter --headless`: the full widget lane
at `--retries 0` with zero `FLAKY` lines; both M-4 race halves, each shown failing
against the pre-fix code; the evidence surface's inertness with rows collapsed and
expanded; the accessibility, visual, and visual-geometry smoke lanes from clean artifact
roots; and `make automation-smoke`'s live D-Bus capture of `window.workspace`.

**Not proved, and only a real session can prove it.** The widget harness runs against a
private headless compositor with its own frame clock, so it does not reproduce (a)
`Trying to measure GtkBox ... needs at least ...` warnings emitted during a real
sidebar reveal against **restored** workspaces, which is exactly the pairing
`widget-wiring.md` calls out; (b) `*** BUG *** In pixman_region32_init_rect` from a
zero-or-negative allocation during a live animation; (c) GTK/GLib criticals raised on a
session bus, portal, or AT-SPI path the headless lanes stub or disable; or (d) the
subjective "does a human see the tree behave" question. `widget-wiring.md` is explicit
that for this subtree widget-green plus a live warning is a **failed** fix, so the
headless evidence is necessary and **not sufficient**.

## Remaining scope to run, when the user schedules it

`make run` against **restored workspaces**, watching stderr for
`Trying to measure GtkBox ...`, pixman `*** BUG *** ... Invalid rectangle`,
`Gtk-CRITICAL`, `Gtk-WARNING`, and `GLib-GObject-WARNING` while:

- expanding and collapsing a deep tree;
- dragging to reorder workspace folders;
- **renaming and deleting a file** — two of the paths this change modified, and the two
  that touch the user's own documents. Confirm a rename still refuses a colliding
  destination with the existing message; that a delete of a normal file and of a
  directory both still work end to end; and that deleting a row whose target has
  **already vanished** reconciles the row silently rather than warning "That item changed
  on disk";
- entering and leaving focused-folder mode;
- toggling the sidebar while it animates, and resizing.

Additions specific to this change's own fixes, which a generic walkthrough would not
cover:

- **rename a file that has unsaved edits in an open tab**, then keep the app running
  and confirm the tab's title and path updated; the draft re-stamp added to
  `update_tab_path` should produce one extra autosave for that tab and no visible
  change. Watch for any autosave churn on **unmodified** tabs, which would indicate
  the `is_modified()` gate is not holding.
- **create a workspace immediately at launch**, before the sidebar has populated, then
  restart. Every previously stored workspace must still be listed. This is the M-4
  pre-first-load path; it is driven headlessly against a standalone sidebar, and the live
  run is what confirms the real startup gate takes the same branch.

## Acceptance status

**This gap is not accepted by this change, and must not be recorded as accepted.**
It is recorded as **awaiting the user's decision**: either they run the walkthrough,
or they accept the gap explicitly. Task 10.13 stays `[~]`. Nothing in the matrix, the
programme record, or the task list claims otherwise on this change's own authority.

The row **is** marked `migrated`, so unlike the earlier revision of this file, this
deferral is the change's one outstanding acceptance item on the live axis rather than
being overshadowed by unlanded structural work. It is named on the matrix row, in the
matrix's per-row notes, and on the programme record's slot ledger.
