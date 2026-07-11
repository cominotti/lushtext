## Why

Workspace watching is intentionally limited to materialized sidebar paths, but restarting a watcher currently derives targets by flattening the visible tree and can construct or replace the backend watcher synchronously on the GTK thread. Large expanded trees, slow mounts, or watcher teardown can therefore make a correct refresh policy briefly unresponsive. The target set and watcher lifecycle should be incremental, asynchronous, and stale-safe.

## What Changes

- Maintain the materialized watch-target set incrementally as top-level and expanded directory rows are installed, reconciled, collapsed, or removed.
- Start and replace watcher instances through bounded background work while GTK retains only owned target snapshots and completion application.
- Use generations so a watcher created for an obsolete workspace scope or tree state can never replace the current watcher.
- Ensure watcher replacement and teardown do not synchronously stall the main loop, while preserving recoverable error feedback and manual Refresh.
- Preserve non-recursive materialized-scope watching, overlapping-folder behavior, expansion and selection stability, and access-noise filtering.
- Add empty, representative, many-target, overlapping-folder, unreadable-path, slow-backend, stale-completion, and constrained-sidebar coverage.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-tree-refresh`: Strengthens automatic refresh with incremental target bookkeeping, asynchronous watcher lifecycle management, and stale-result rejection.

## Impact

- Affects `services/workspace_watch.rs` and workspace-section tree/watch orchestration under `ui/sidebar/`.
- Keeps the filesystem boundary and currently materialized-scope policy intact; it does not introduce recursive startup watching.
- Should precede the workspace-section adapter decomposition so the final module split follows the settled watcher ownership.
