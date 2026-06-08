## Context

`refine-workspace-folder-set-ux` introduced explicit top-level workspace-folder drag handles and a single-line insertion indicator. The visible insertion feedback is now acceptable, but dragging a folder over other rows can still briefly flicker the `GtkTreeExpander` disclosure icon.

The current implementation installs a `GtkDropTarget` on the row overlay in capture phase and keeps `GtkTreeExpander` stable from app code. That still leaves `GtkTreeExpander` as the picked child widget below the pointer. GTK documents `GtkTreeExpander` as able to expand rows on drag gestures, and the current defensive fallback in `tree_loading.rs` confirms that a drag can still reach child-model creation: it returns an empty model and collapses the row back later. That fallback prevents lasting expansion, but it can still show the exact flicker users are reporting.

The desired model is simpler:

```text
Normal pointer/keyboard interaction:

pointer -> GtkTreeExpander -> expand/collapse, activation, context menu, peek

Workspace-folder reorder drag:

pointer -> full-row transparent DnD shield -> reorder hover/drop only
                                      GtkTreeExpander never sees reorder hover
```

## Goals / Non-Goals

**Goals:**

- Prevent `GtkTreeExpander` from receiving workspace-folder reorder drag hover.
- Keep disclosure icons visually stable during reorder drags: no expansion flip, checked-state flicker, hover/drop-state pulse, or idle repair animation.
- Preserve the current single rounded insertion line for valid top-level same-workspace reorder targets.
- Preserve normal tree expansion, row activation, context menus, file peek, focus-folder affordances, and keyboard/non-pointer reorder behavior outside active reorder drags.
- Keep invalid targets inert: descendant rows, expander regions, placeholder rows, empty states, section headers, and other workspaces show no insertion line and reject drops.
- Add focused tests that exercise the real failure mode, including no expanded-state transitions, no child-model creation caused by drag hover, no watcher restart, no section/filter state changes, and no filesystem mutation.

**Non-Goals:**

- Do not redesign the workspace-folder reorder model or payload format.
- Do not change workspace persistence, folder membership semantics, search de-duplication, command palette behavior, notes/browser behavior, or folder-note identity.
- Do not replace `GtkTreeExpander` or the `GtkTreeListModel` file tree.
- Do not hide the flicker with CSS while leaving hover-induced expansion/model churn possible.
- Do not add an external DnD library or new dependency.

## Decisions

### 1. Add a permanent row-level DnD shield above `GtkTreeExpander`

Each file-tree row factory should create a transparent overlay child that covers the full row allocation and sits above `GtkTreeExpander`. The shield is the reorder DnD surface; the existing 2px insertion-line overlay remains the visual feedback surface. The shield should be invisible or non-targetable during normal operation so ordinary tree interaction continues to go to `GtkTreeExpander` and the row's existing controls.

During an active workspace-folder reorder drag, the shield becomes the drag-hover/drop owner for row surfaces. It accepts active reorder drag hover for all realized file-tree rows so descendant rows and expander regions cannot leak the drag to `GtkTreeExpander`. It only shows an insertion line and accepts a drop when the row resolves to a valid same-workspace top-level workspace folder target outside drill-down mode.

Alternative considered: keep the drop target on the parent overlay with capture-phase propagation. This is fewer widgets, but it does not remove `GtkTreeExpander` from the picked-widget path and still permits GTK's drag-expand behavior to react.

Alternative considered: temporarily make expanders non-targetable or alter their internal controllers during drag. That mutates the widget that is visibly flickering, can redraw the disclosure affordance during drag begin/end, and risks breaking normal tree interaction under row recycling.

### 2. Treat the current idle-collapse path as a defensive fallback only

The `tree_loading.rs` empty-child-model path should not be the normal drag-hover behavior. It may remain as a safety net while the GTK row tree is being hardened, but focused tests should prove that active reorder hover through the shield does not call into child-model creation for the hovered row.

Implementation may use a test-only counter or hook around drag-hover child-model fallback to make this observable without adding production diagnostics. If the shield is correct, that counter remains at zero during simulated active reorder hover.

Alternative considered: keep testing only final collapsed state. That codifies "expand, then repair," which is exactly the user-visible flicker.

### 3. Keep visual feedback and hit ownership separate

The full-row shield owns drag hit-testing. The insertion indicator owns painting. The indicator stays a transparent outer positioning surface plus a single fixed-height rounded line. The shield must not paint a filled rectangle, row highlight, or centered drop-into-folder state.

The shield should not disturb constrained sidebar geometry. It should cover the row allocation without changing row height, label measurement, horizontal clipping, scroll behavior, drag-handle placement, focus-folder overlay placement, or long-folder-name ellipsizing.

Alternative considered: use the shield itself as the visible insertion indicator. That risks reintroducing the previous filled-rectangle feedback when GTK allocates the full row.

### 4. Keep row recycling explicit

`GtkListView` recycles row widgets, so setup/bind/unbind must reset shield state alongside the existing drag handle, insertion indicator, focus button, and inline rename cleanup. A recycled row must not keep a visible shield, a stale insertion line, a stale valid-target decision, or stale drag-hover state after unbind.

The DnD code should continue resolving target identity from the live `ListItem`/`TreeListRow` at hover/drop time rather than caching row identity in widget state.

### 5. Broaden tests beyond "state ends right"

The regression suite should cover:

- A valid top-level target shows exactly one insertion line while the disclosure icon remains stable.
- Hovering a collapsed top-level folder during reorder does not emit `notify::expanded`, does not create a child store, and does not restart workspace watches.
- Hovering a descendant folder and its expander region during reorder is shield-owned, shows no insertion line, rejects drop, and does not expand.
- The shield is inactive outside active reorder drag so ordinary folder expansion still works.
- Recycled rows clear shield and indicator state.
- Many-folder and constrained-width sidebar cases keep the fixed top row, action reachability, row height, and no-horizontal-scrollbar contract.
- Keyboard/non-pointer Move Up/Move Down reorder remains available and uses the same state/persistence path.
- Reorder still never creates, deletes, moves, renames, copies, or rewrites user files.

Tests should prefer focused widget tests under the existing headless harness. If full pointer DnD synthesis is unreliable, use test-only helpers that exercise the same shield accept/motion/drop decision logic and the same fallback counters. The important proof is that the intended path does not enter expansion repair.

## Risks / Trade-offs

- [Risk] A full-row shield can accidentally block normal clicks, context menus, peek, or focus-folder hover outside drag. Mitigation: keep it inert outside active workspace-folder reorder drags and add tests for ordinary expansion/activation/context-menu behavior after installation.
- [Risk] The shield could cover the drag handle and interfere with drag start. Mitigation: activate shield ownership only after drag preparation has produced a valid reorder payload, or layer/target it so the handle remains the source before active drag state begins.
- [Risk] Row recycling can leak visible shield or indicator state. Mitigation: clear both on leave/drop/cancel/unbind and add recycled-row tests.
- [Risk] Headless GTK tests may not synthesize real drag hover reliably. Mitigation: add deterministic test helpers for shield ownership, valid/invalid hover decisions, and child-model fallback counters; keep any real pointer test as a smoke test only if stable.
- [Risk] The fallback path can mask a regression if tests only assert final state. Mitigation: tests must assert zero expanded transitions and zero fallback child-model creation for shield-owned hover.
- [Risk] A full-row overlay can disturb measurement or constrained sidebar geometry. Mitigation: use overlay children that do not participate in row measurement and cover long-name/constrained-width states.

## Migration Plan

1. Add the row-level transparent shield in the workspace-section row factory setup.
2. Move reorder `DropTarget` installation from the parent row overlay to the shield while keeping the visual insertion indicator as a separate fixed-height overlay.
3. Keep drag payload, valid-target, absolute-index reorder, and persistence callback logic unchanged.
4. Add shield visibility/targetability synchronization for drag begin/end and row bind/unbind.
5. Add deterministic test helpers for active drag shield ownership, valid/invalid hover decisions, and drag-hover child-model fallback detection.
6. Add the focused widget tests listed in this design.
7. Rerun focused workspace-section/sidebar widget tests, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `openspec validate --all --strict`, and `git diff --check`.

Rollback is straightforward because no user data or persistence format changes. Reverting the shield returns the prior reorder hover behavior and leaves the folder-set model intact.

## Open Questions

None. The chosen direction is to separate reorder DnD hit ownership from `GtkTreeExpander` rather than continuing to repair or mask expander reactions after they occur.
