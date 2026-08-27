# Live and manual proof — DEFERRED FOR USER AVAILABILITY (task 10.12)

**Planned as deferred from the start, not discovered late.** Slot 4 established
that isolating an app's *state* does not isolate its *window*: a real Wayland
launch maps a surface and takes focus regardless of `XDG_*` isolation, and it
interrupted the user's session. **No live launch was started for this change.**
Everything else display-dependent went through `scripts/run-widget-tests.sh
--headless` or an isolated `mutter --headless` smoke lane.

## Exact remaining scope

### 1. `make run` against restored workspaces — **tree row, so also slot 5b's**

Expand and collapse a deep tree, drag to reorder folders, rename and delete a
file, toggle the sidebar while it animates, and resize — watching stderr for
`Trying to measure GtkBox ...`, pixman `*** BUG *** In pixman_region32_init_rect`,
`Gtk-CRITICAL`, `Gtk-WARNING`, and `GLib-GObject-WARNING`.
`.agents/rules/widget-wiring.md` names the sidebar subtree explicitly as needing a
real `make run` cycle with restored workspaces, so widget-green is necessary and
**not sufficient** there.

**This change did not restructure the sidebar's paned or revealer geometry**, and
`ui/sidebar/mod.rs`, `imp.rs`, `workspaces.rs`, `dialogs.rs`, and `callbacks.rs`
are unchanged except for three module declarations and one re-export pair. The
geometry-relevant risk from this change is therefore **low**; the real geometry
risk arrives with slot 5b's facade and role moves. The walkthrough is worth doing
once for the **file-operation** changes specifically:

- rename a file onto the name of an existing sibling and confirm the refusal
  message appears in the status bar and the existing file is untouched;
- start an inline `New File`, cancel it, and confirm no stray placeholder remains;
- rename a file that has a bookmark or a note, and confirm the note follows.

### 2. `make run-format-upgrade-newer-manual-test` and `make run-format-upgrade-older-manual-test`

The future-version and upgradeable-old-version startup dialogs, whose grouped-row
copy and default response are user-facing. **This change did not modify
`ui/window/startup_data.rs` or `services/format_upgrade/**`** — task 2.2 decided
the module cross-cutting and left it alone — so these two lanes are
regression-checking an unchanged surface. Low priority.

### 3. `make run-command-palette-notes-manual-test`

The Notes palette fixtures for manual review. **Higher priority than the other
two**, because this change did restructure the notes browser's module boundaries
and retired `NoteSourceRefreshCoordinator` onto the shared coordinator. The
headless `make command-palette-notes-smoke` lane covers the separators and
representative rows through AT-SPI; this is the human read of the same surface.

## If a live drive is ever scheduled

Use **targeted AT-SPI**, not synthetic global input. `ydotool` and friends type
into whatever the compositor focuses, which is both hazardous in a live session
and unverifiable — slot 4's first attempt is on record.

## Lane fragility found during final verification, handed to slot 7

**`make visual-smoke` scenario `constrained-preview-side-by-side` failed once in
four runs on this exact tree**, on

```
assert window["surfaces"]["workspace_sidebar_visible"] is True
```

with `workspace_sidebar_requested: True` and `workspace_sidebar_visible: False` —
that is, the sidebar was mid-transition, requested but not yet rendered, when the
snapshot was taken.

**Not caused by this change**, on three independent grounds: the diff touches no
breakpoint, split-view, animation, or preview-layout code; the lane passed **three
of four** runs on this tree, including twice *after* the pass-2 fixes; and the
assertion is about a surface this change does not own.

**Diagnosed root cause.** The scenario asserts a breakpoint-dependent surface
state at a **constrained** width, where the workspace sidebar's adaptive collapse
and restore is in flight. The lane's only settle gate before snapshotting is
`snapshot["idle"] is True`, which does not cover that transition. The
`workspace-sidebar-animation` readiness blocker exists for exactly this settle —
and task 8.3 of this change established that the blocker follows **the
animation**, not the row name, so it belongs to `WFR-SHELL-LAYOUT` (slot 7), fed
by the window's sidebar transition settle rather than by the workspace row.

**Concrete fix for slot 7**: have the constrained side-by-side scenarios wait on
the `workspace-sidebar-animation` blocker clearing (or on a predicate that
includes it) before capturing, rather than on `idle` alone.

**Fixed in-stream** (the hand-off was wrong, and the review was right to reject
it). `.agents/rules/preexisting-blockers.md` explicitly covers test
infrastructure — "update documentation, rules, and test infrastructure in the same
change set when that is required to eliminate the blocker permanently" — and the
fix was fully specified rather than speculative, so deferring it was deferring a
known, specified fix around a rule with no exceptions.

**The change**: `scripts/run-visual-smoke.sh` now adds
`--wait-predicate visual-geometry-settled` to every scenario whose width runs the
adaptive collapse — `constrained-*`, `compact-*`, `short-layout`, and
`large-text-constrained`. That predicate includes the
`workspace-sidebar-animation` blocker plus the rest of the shell settle, which is
precisely the transition the snapshot was racing. `--wait-predicate` was already
repeatable in `capture-lushtext-mutter.py`, so this is one scenario-gate line, not
new machinery.

**Applied to six scenarios, not one.** Fixing only the case that failed would
have left five identical latent races at the same widths:
`compact-properties`, `constrained-preview`, `constrained-preview-side-by-side`,
`constrained-properties`, `large-text-constrained`, `short-layout`.

**Proved reaching the app, not just added to a script.** The previously-flaky
scenario's own `automation-waits.txt` records it as satisfied:

```
predicate=file-open-complete ok=True status=ready
predicate=idle ok=True status=ready
predicate=visual-geometry-settled ok=True status=ready
predicate=idle ok=True status=ready
```

**Stability, against the 1-of-4 failure baseline:**

| Run | Result | Scenario passes | `constrained-preview-side-by-side` |
| --- | --- | --- | --- |
| 1 | exit 0 | 40 | pass |
| 2 | exit 0 | 40 | pass |
| 3 | exit 0 | 40 | pass |
| 4 | exit 0 | 40 | pass |

Four consecutive clean runs, each from a wiped `build/smoke/visual`.

**What remains slot 7's** is narrower than the original hand-off claimed: not the
lane's readiness gating, which is fixed, but any deeper product-side question
about whether a constrained width *should* collapse the sidebar while a
side-by-side preview is open. That is a `WFR-SHELL-LAYOUT` design question, and
nothing in this change depends on its answer.
