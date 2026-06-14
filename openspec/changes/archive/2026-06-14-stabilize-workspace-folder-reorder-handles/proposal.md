## Why

Workspace folder reorder handles can become stale when a workspace changes from one folder to two folders during the same session. This makes the first folder appear non-reorderable until the sidebar is hidden/shown or the app restarts, even though the workspace now has multiple ordered folders.

## What Changes

- Refresh visible top-level workspace folder rows when folder membership changes so reorder handles match the current folder count immediately.
- Preserve the current product rule: a single workspace folder has no visible reorder handle because there is no valid reorder destination.
- Keep drag-and-drop, Move Up/Move Down, context menus, focus-folder controls, disclosure, file activation, and inline rename interactions reachable after add, remove, reorder, refresh, collapse, and scope-filter transitions.
- Add broad widget regression coverage for reorder affordance visibility, ordering behavior, row recycling, dense lists, long names, constrained sidebar width, collapsed sections, and cross-workspace invalid drops.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-sidebar-shell`: Clarify that top-level workspace folder reorder affordances must update immediately as the folder set changes, while non-reorderable one-folder and descendant-row states remain free of reorder handles.

## Impact

- Affected UI code: `crates/lushtext-core/src/ui/sidebar/workspace_section/` and the sidebar workspace mutation path that adds, removes, and reloads workspace folder rows.
- Affected tests: `crates/lushtext/tests/widget/workspace_section.rs` and adjacent sidebar/window widget coverage when full workflow state is required.
- No new runtime dependencies, public APIs, persistence format changes, or breaking changes.
