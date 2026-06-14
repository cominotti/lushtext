## Context

Workspace sections render their file tree with `GtkListView`, `GtkTreeListModel`, `GtkTreeExpander`, and `SingleSelection`. That selection currently gives clicked rows a persistent Adwaita-selected appearance even though the row is not the app's active document state. Selection is still useful internally: Space-to-peek, keyboard navigation, activation, and refresh/reanchor flows use the current selected tree row.

The window already owns tab state. `open_document()` is the single authority for opening/focusing documents, `open_paths` prevents duplicate file tabs, and the window updates that state on load failures, tab detach, Save As, rename, and delete. The sidebar should consume a window-owned projection of open/active file paths instead of creating a second source of truth.

The workspace tree also uses recycled GTK factory rows. Any visual state derived from live tab membership must be applied in `connect_bind()` and repaired over realized rows after tab or tree state changes, matching the existing reorder-handle synchronization pattern.

## Goals / Non-Goals

**Goals:**

- Make ordinary pointer clicks produce only transient hover/press feedback, not a lasting selected-row fill.
- Preserve keyboard navigation, focus-visible state, and Space-to-peek.
- Show a subtle persistent indication for file rows open in any tab.
- Show a slightly stronger indication for the file row matching the active tab.
- Keep row indicators synchronized across tab open, close, switch, load failure, Save As, rename, delete, session restore, row recycling, refresh, collapse/expand, and workspace filter changes.
- Preserve existing sidebar geometry, row height stability, no-horizontal-scrollbar behavior, and all current row interactions.

**Non-Goals:**

- Do not change tab duplicate detection, tab ordering, session persistence, or editor load/save semantics.
- Do not make folder rows show open-file indicators.
- Do not add a new sidebar selection persistence model or remember clicked rows across restarts.
- Do not replace the workspace tree with `AdwSidebar`, `GtkTreeView`, or a custom navigation widget.
- Do not expose new public automation actions unless implementation later changes an externally observable action surface.

## Decisions

### Keep `SingleSelection` as internal navigation state

The file tree should keep `SingleSelection` because it already feeds keyboard navigation, file peek, activation, and row reanchoring. The visual change should be scoped to the workspace file-tree styling/projection so a selected row is not painted like the current document merely because it was clicked.

Alternative considered: switch to `NoSelection`. That would remove the sticky visual highlight, but it would also force new ad hoc state for Space-to-peek, keyboard target tracking, selected row bounds, and refresh preservation. That is larger and more fragile than the product change requires.

### Treat row state as a projection of pointer/focus/tab state

Rows should present these user-facing states:

```text
hovered row        temporary surface feedback
pressed row        temporary activation feedback
keyboard target    focus-visible/peek affordance only
open in tab        subtle persistent file marker
active tab file    stronger persistent file marker
```

The projection should avoid strong filled backgrounds for ordinary mouse selection. Open and active indicators should be visible without competing with the tab strip: for example, a thin accent strip/dot and modest label weight for open files, with a clearer accent or secondary class for the active file. The implementation should choose a stable-size indicator so hover, open, active, and recycled-row transitions do not change row height or push labels into controls.

Alternative considered: reuse the selected-row fill for the active tab file. That would be visually cheap, but it keeps the same ambiguity this change is meant to remove and makes the sidebar compete with the tab strip.

### Make the window the open-file source of truth

The window should publish a compact sidebar row-state snapshot derived from mounted file-backed tabs and the selected tab. The snapshot can include path keys and canonical paths already known by editors so rows can match files opened through symlinks, Save As, sidebar rename, and session restore where current code already tracks canonical identity.

The sidebar should store only transient in-memory row-state inputs:

- set of open file path identities;
- optional active file path identity;
- no file contents, notes, draft data, or persistence identifiers.

The window should refresh this projection after every structural tab operation and path-affecting operation that already updates `open_paths`: open, load failure cleanup, tab detach/close, selected-page change, Save As completion, sidebar rename, file delete close, session restore, and duplicate-tab focus.

Alternative considered: let sidebar sections inspect `window.imp().open_paths` directly. That would couple section rendering to window internals and make tests brittle. A window-to-sidebar projection keeps ownership clearer.

### Centralize row-state predicates and realized-row resync

Workspace-section row binding should call one central predicate or helper that computes row visual state from the current `FileTreeItem`, current `TreeListRow`, and sidebar open/active path snapshot. The same helper should be used by an explicit realized-row synchronization pass after live state changes that do not cause GTK to rebind rows.

Resync triggers should include:

- tab open/close/switch and session restore completion;
- Save As and sidebar rename path updates;
- load failure removal from the open set;
- file delete closure;
- folder load, refresh, collapse/expand, scope filter hide/show, and row recycling.

This mirrors the existing reorder-handle repair pattern, which already treats `connect_bind()` as necessary but insufficient for state that changes outside row rebinding.

Alternative considered: rely only on `connect_bind()`. Prior sidebar work showed this leaves realized rows stale when parent collection or app state changes without rebinding.

### Keep styling scoped to the workspace file tree

CSS should target classes added by the workspace-section factory, not global `GtkListView` or `.navigation-sidebar` rules that might affect command palette, search results, or future sidebar-like widgets. Styling should preserve Adwaita focus visibility and avoid suppressing accessibility state. Pointer hover can keep native or app-scoped hover styling; persistent emphasis should come from open/active marker classes rather than row selection.

## Risks / Trade-offs

- Sticky selection may still exist internally for keyboard/peek while no longer looking selected -> Tests must cover Space-to-peek and keyboard focus after the visual change.
- Suppressing selected-row paint too broadly could hide focus for keyboard users -> Keep focus-visible styling and verify keyboard navigation in widget tests.
- Open markers can become stale on realized rows after tab changes -> Use central predicates and explicit realized-row resync on tab/path/tree transitions.
- Matching paths across symlinks, Save As, and rename can be subtle -> Use the same window-owned path/canonical identity information that drives duplicate-tab prevention where available.
- Indicators may crowd long or deeply indented labels -> Use fixed-size, non-expanding indicator affordances and constrained-geometry tests.
- Active/open markers could be too visually loud -> Keep open state subtle and active state restrained; visual proof should check the marker exists without turning rows into a second tab strip.

## Migration Plan

This change has no persisted-data migration. Existing sessions and workspaces continue loading as before. Rollback is a styling/projection rollback: remove the sidebar row-state projection calls/classes and return to current GTK selection rendering.

## Open Questions

- Should an active file row that appears multiple times through overlapping workspace folders show the active marker on every matching row or only the first visible matching row? The existing spec allows duplicate file appearances; the most transparent behavior is to mark every row that resolves to the same open/active file identity.
- Should open-but-failed load placeholders ever mark sidebar rows? The likely answer is no: first-load failure currently removes paths from `open_paths`, so the sidebar should follow that cleanup.
