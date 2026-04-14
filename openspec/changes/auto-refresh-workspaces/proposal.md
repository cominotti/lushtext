## Why

LushText currently reflects workspace contents only when a tree is first loaded or a user performs an action inside the app. Filesystem changes that happen outside the sidebar flow can leave the workspace tree stale, which makes the editor feel unreliable and forces people to manually collapse, reopen, or replace roots just to see the real state.

## What Changes

- Add automatic workspace-tree refresh so sidebar sections stay in sync when files or folders are created, removed, renamed, or moved on disk.
- Add a manual `Refresh` control in each workspace-section header, immediately to the left of the existing `Replace Workspace Root` button.
- Preserve the sidebar's current performance guarantees by refreshing only the affected workspace section and by reusing the existing async, batched tree-loading path.
- Keep expansion, selection, drill-down, and placeholder behavior predictable across refreshes so automatic updates do not feel disruptive.
- Surface lightweight feedback when a manual or automatic refresh cannot fully reload a workspace because of I/O or watcher failures.

## Capabilities

### New Capabilities
- `workspace-tree-refresh`: Keeps workspace sidebar trees aligned with on-disk changes and provides a manual refresh action for each workspace section.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/sidebar`, especially `workspace_section/`, plus filesystem-scanning services and related tests.
- Affected systems: sidebar header actions, tree reload orchestration, background filesystem monitoring, and workspace-section state restoration after refresh.
- Dependencies and APIs: likely adds an internal filesystem-watch abstraction or crate-backed watcher adapter that can trigger section-scoped reloads without blocking GTK.
