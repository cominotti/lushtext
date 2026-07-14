## 1. Establish Event-Pressure Baselines

- [ ] 1.1 Record current watcher callback, receiver, GTK polling, refresh-debounce, error/disconnect, and lifecycle-generation ownership with existing targeted-refresh behavior.
- [ ] 1.2 Choose and document the finite unique-path cap shared by watcher transport and refresh planning using representative bulk create/remove/rename fixtures.
- [ ] 1.3 Add pure mailbox merge tests for empty, paths, duplicate paths, overflow promotion, full-refresh dominance, bounded errors, and disconnect combinations.

## 2. Build the Bounded Watcher Mailbox

- [ ] 2.1 Introduce a GTK-free coalescing mailbox owned by one watcher handle and backend callback, with bounded path/full-refresh and diagnostic/disconnect state.
- [ ] 2.2 Move tree-changing event filtering, path deduplication, and overflow promotion into the backend callback without filesystem or GTK work inside the mailbox lock.
- [ ] 2.3 Replace the unbounded channel contract and make `try_poll` take at most one already-normalized bounded notice.
- [ ] 2.4 Ensure stale/retired watcher handles cannot merge into a current mailbox and preserve off-thread handle disposal.
- [ ] 2.5 Add service tests for producer/consumer races, events arriving during take, repeated errors, disconnect, access-noise filtering, and stale-handle isolation.

## 3. Bound GTK Refresh Work

- [ ] 3.1 Remove the workspace-section receiver drain loop and process at most one mailbox notice per poll callback.
- [ ] 3.2 Bound refresh-side unique paths by the same cap, clear them when promoting to full refresh, and ignore later targeted paths while full refresh is pending.
- [ ] 3.3 Preserve current debounce, stable tree reconciliation, overlapping-folder target behavior, manual Refresh, visible warnings, and automation readiness state.
- [ ] 3.4 Add widget tests for path notice, overflow/full refresh, repeated batches between polls, errors plus changes, disconnect, workspace hide/disposal, and manual recovery.

## 4. Benchmark and Verify

- [ ] 4.1 Add scale benchmarks for off-GTK normalization/merge, duplicate/Unicode/deep paths, producer rates above GTK consumption, and full-refresh promotion.
- [ ] 4.2 Record retained mailbox/refresh state and notices per poll in deterministic test seams without exposing private filesystem contents through automation.
- [ ] 4.3 Update root/sidebar guidance and performance documentation for the bounded mailbox and full-refresh fallback contract.
- [ ] 4.4 Run focused service/sidebar/widget tests, watcher lifecycle tests, `make test-unit`, and relevant headless runtime/automation readiness smokes.
- [ ] 4.5 Run `make check`, `make lint-advisory`, `make pre-commit`, accessibility/visual-geometry proofs, `git diff --check`, and strict OpenSpec validation.
- [ ] 4.6 Perform final architecture/performance review confirming bounded retained state, off-GTK normalization, one-notice GTK work, generation isolation, and no regression to full-model target scans.
