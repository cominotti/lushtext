## 1. Incremental Target State

- [ ] 1.1 Implement plain `MaterializedWatchTargets` with a flattened-row contribution vector, deduplicated target reference counts, stable ordering, and typed generations.
- [ ] 1.2 Add splice and row-state operations for top-level rows, expanded directories, collapsed descendants, files, overlapping paths, reorder, and empty fallback state.
- [ ] 1.3 Add a test-only full derivation oracle and generated event-sequence tests proving incremental state matches it without reference-count drift.

## 2. GTK Model Integration

- [ ] 2.1 Connect flattened model `items-changed`, row expansion, model replacement, folder mutation, focus-folder, and disposal paths to incremental target updates.
- [ ] 2.2 Coalesce effective target-set changes and avoid generation/restart work when presentation signals leave the deduplicated set unchanged.
- [ ] 2.3 Preserve current top-level configured-folder fallback before the tree model is mounted and preserve zero-folder behavior.
- [ ] 2.4 Remove `current_watch_targets` full flattened-model scans after oracle-backed integration coverage passes.

## 3. Asynchronous Watcher Lifecycle

- [ ] 3.1 Verify the concrete watcher handle's worker-transfer contract on supported targets; if it is not `Send`, implement the documented service-owned worker/channel fallback.
- [ ] 3.2 Move old-watcher teardown plus new watcher creation/target registration into `gtk_lush_tasks` background work with owned target snapshots.
- [ ] 3.3 Install success or report failure only for the current section lifetime and target generation.
- [ ] 3.4 Dispose stale successful handles and empty-set retired handles off the GTK main thread and remove the one-millisecond synchronous startup timer.
- [ ] 3.5 Preserve poll-source ownership, access-noise filtering, current-generation error deduplication, manual Refresh, and rendered-tree stability during replacement.

## 4. Scale and State-Extreme Verification

- [ ] 4.1 Add slow-backend hooks/tests for creation, registration, teardown, stale success/failure, section disposal, and rapid superseding target generations.
- [ ] 4.2 Add service/widget tests for zero, representative, many expanded, overlapping, unreadable, reorder, focus-folder, refresh, expansion, and collapse states.
- [ ] 4.3 Add performance evidence that one-row target changes avoid full GTK model scans and all backend lifecycle work stays outside GTK timing.
- [ ] 4.4 Add accessibility/visual geometry proof for warnings, long paths, constrained width, stable header controls, no fake rows, and item-region-only scrolling.
- [ ] 4.5 Run formatting, responsiveness/performance/architecture reviews, focused tests, `make accessibility-smoke`, `make visual-geometry-smoke`, `make check`, and `make pre-commit`; fix every issue found.
- [ ] 4.6 Run the learning workflow and update workspace-section guidance if the incremental ownership contract needs durable documentation.
