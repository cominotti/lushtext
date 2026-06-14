## Context

Workspace folder reorder handles are currently projected during `GtkListView` row binding. A top-level persisted workspace folder row shows the handle only when the section is in its normal top-level view and the workspace has more than one configured folder. That steady-state rule is correct, but a live add/remove can change the folder count without rebinding already-realized rows.

The reported regression is the `1 -> 2` transition: the second folder row binds with a handle, while the first row keeps the old hidden handle until a sidebar rebuild, scope visibility change, or restart causes a fresh bind. The same projection gap can appear in reverse for `2 -> 1`, where the remaining row can keep a stale handle.

## Goals / Non-Goals

**Goals:**

- Keep the current one-folder rule: a single top-level workspace folder has no reorder handle.
- Synchronize visible top-level workspace folder reorder handles immediately after folder add, remove, reorder, reload, section collapse/expand, and scope-filter visibility transitions.
- Keep descendant file and directory rows free of workspace-folder reorder handles.
- Preserve the existing reorder behavior for drag-and-drop and Move Up/Move Down without mutating filesystem contents.
- Add broad widget coverage for membership transitions, ordering, row recycling, invalid targets, dense lists, long names, constrained geometry, and interaction reachability.

**Non-Goals:**

- Do not introduce always-visible reorder handles for one-folder workspaces.
- Do not change the persisted workspace format, workspace scope semantics, or folder-note identity.
- Do not replace `GtkListView`/`GtkTreeListModel`, the transparent reorder shield, or the existing DnD insertion-line behavior.
- Do not add a new end-to-end harness; this belongs in the existing widget-test lane.

## Decisions

### Reuse the existing reorderability rule

The row projection should have one helper that answers whether a bound row is a reorderable workspace folder row:

- the row depth is zero,
- the row item has a persisted `WorkspaceFolderId`,
- the section is not in drill-down mode,
- the section has more than one original workspace folder.

Alternative considered: show the handle for a single folder so no live refresh is needed. That would make the UI noisier and expose a disabled or useless affordance when there is no valid destination, so it is not the preferred behavior.

### Resynchronize realized rows after membership changes

After the section's `original_folders` and top-level store are updated, the section should walk the currently realized `GtkListView` rows and reapply the same reorder-handle projection used during bind. Unrealized rows can still rely on the normal bind path.

Call sites should include the existing workspace-folder membership paths: `load_workspace_folders`, incremental add, remove, and in-place reorder/reload paths. This avoids a whole-section rebuild solely to repaint button visibility, preserving expansion state, selection, scroll position, and watcher scope where possible.

Alternative considered: force `load_workspace_folders` or a full section rebuild after every add/remove. That would fix the stale button but risks extra flicker and state loss for a tiny projection update.

### Keep interaction ownership unchanged

The drag handle remains the explicit reorder affordance. The transparent row-level shield still owns active reorder hover only while a reorder drag is active; `GtkTreeExpander` keeps normal disclosure behavior outside drag. Resynchronizing handle visibility must not make the shield targetable outside active drag and must not steal click, context-menu, peek, focus-folder, activation, or inline-rename interactions.

### Treat tests as a state matrix

The implementation should add widget tests that prove both steady states and live transitions:

- zero folders,
- one folder,
- one to two folders,
- two to one folder,
- many folders,
- long or awkward folder names,
- overlapping folders,
- constrained sidebar geometry,
- collapsed sections,
- scope-filter hide/show,
- reordered rows,
- descendant rows and recycled rows,
- invalid cross-workspace or child-row drops.

Use existing widget helpers and `wait_until` for async UI state. Avoid sleeps, live-display runs, and broad screenshot assertions for this fix.

## Risks / Trade-offs

- Realized-row traversal could miss offscreen rows -> The bind path must keep using the same helper so offscreen rows are correct when GTK realizes them.
- A sync helper could accidentally show handles on descendants or drill-down rows -> Cover descendant, drill-down, row-recycling, and scoped visibility states in widget tests.
- Adding many widget tests can lengthen the suite -> Keep tests focused inside `workspace_section.rs` and use direct widget/model setup when a full `LushtextWindow` workflow is not required.
- A refresh after collapsed or hidden states could be hard to observe -> Test the user-visible result after expanding or showing the section rather than relying on internal row realization.
- DnD coverage can become brittle if it synthesizes pointer timing -> Reuse existing test seams for hover/drop decisions and reserve full pointer simulation for behavior that cannot be asserted through those seams.
