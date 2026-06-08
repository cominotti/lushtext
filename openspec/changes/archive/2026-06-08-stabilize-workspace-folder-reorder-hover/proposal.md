## Why

Workspace-folder drag reorder now has the right single-line insertion feedback, but drag hover can still make `GtkTreeExpander` disclosure icons flicker. That means the sidebar is still letting reorder drag gestures reach the tree-expansion surface, which violates the intended reorder-only interaction and makes the operation feel unstable.

## What Changes

- Add an explicit full-row inert DnD shield above each file-tree row's `GtkTreeExpander` during active workspace-folder reorder drags.
- Route workspace-folder reorder hover and drop handling through the shield so `GtkTreeExpander` never receives reorder drag hover.
- Keep the current single-line insertion indicator behavior for valid same-workspace top-level folder targets.
- Preserve normal tree expansion, activation, context menu, peek, and focus-folder behavior outside active reorder drags.
- Keep drag-hover fallback safeguards only as defensive protection; the primary path must prevent hover-induced expansion before it happens.
- Add regression tests that prove active reorder hover does not flip the disclosure icon, emit expanded-state transitions, materialize child stores, restart watches, or mutate filesystem/user data.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-sidebar-shell`: tighten the workspace-folder reorder requirement so reorder drag hover is owned by an inert row-level shield above `GtkTreeExpander`, leaving folder disclosure state visually and semantically unchanged.

## Impact

- Affected code: `crates/lushtext-core/src/ui/sidebar/workspace_section/{imp.rs,dnd.rs,tree_loading.rs,mod.rs}` and focused widget tests under `crates/lushtext/tests/widget/workspace_section.rs`.
- Affected UI: workspace sidebar folder-row drag-and-drop reorder only.
- No persistence format, filesystem operation, search scope, command palette, notes/browser, or workspace membership semantic changes.
- No new external dependencies.
