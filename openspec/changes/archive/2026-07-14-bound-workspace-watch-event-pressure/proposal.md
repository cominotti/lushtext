## Why

Watcher startup and teardown are now responsive, but event delivery still uses an unbounded channel and performs path normalization plus complete queue draining on GTK. A bulk create/remove/rename can therefore grow memory without a fixed ceiling and monopolize a main-loop turn even though watcher lifecycle work itself is off-thread.

## What Changes

- Normalize and merge filesystem events before GTK delivery into a bounded mailbox where a full-refresh marker dominates an oversized path set.
- Bound retained paths, errors, and pending notices independently of producer rate; never drop tree-changing activity without replacing it with a conservative full refresh.
- Let each GTK poll consume at most one bounded notice, and keep the refresh-side pending path set under the same cap.
- Preserve access-noise filtering, materialized-scope targets, overlapping-folder correctness, generation-safe watcher replacement, stable tree reconciliation, and manual Refresh.
- Add deterministic burst, overflow, error, disconnect, overlap, and lifecycle tests plus scale benchmarks for producer rates above GTK consumption.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-tree-refresh`: Bound watcher event transport and GTK-side refresh planning while conservatively preserving every visible tree change.
- `performance-regression-coverage`: Cover sustained and burst watcher event pressure with retained-state and per-turn bounds.

## Impact

- Affects `services/workspace_watch.rs` and workspace-section watch/refresh runtime state and tests.
- Replaces the unbounded receiver contract with a bounded coalescing notice abstraction; no persisted workspace format or visible action changes.
- Keeps backend callback work GTK-free and leaves watcher lifecycle generations unchanged.
- Is independent of the editor/draft/search changes and may be implemented after the buffer-snapshot foundation whenever a separate work stream is available.
