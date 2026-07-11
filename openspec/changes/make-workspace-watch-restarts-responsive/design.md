## Context

Each workspace section watches only top-level and expanded materialized rows, which avoids recursive startup traversal. `restart_workspace_watch` currently walks the flattened `GtkTreeListModel`, defers one millisecond, and then calls `WorkspaceWatcher::start` on the GTK thread. Taking the old watcher also drops its debouncer there. Expanded trees can make target collection O(N), and watcher creation, target registration, or teardown may block on slow filesystems or platform watcher limits.

## Goals / Non-Goals

**Goals:**

- Maintain the current materialized target set with work proportional to changed rows.
- Move watcher creation, target registration, replacement teardown, and stale-result disposal off the GTK thread.
- Reject watcher completions for obsolete target generations.
- Preserve non-recursive scope, overlapping folder views, polling, error feedback, and manual refresh.
- Keep target bookkeeping testable without a live watcher backend.

**Non-Goals:**

- Recursively watching every configured descendant.
- Changing tree scan, refresh reconciliation, or event filtering semantics.
- Hiding watcher failures or removing manual Refresh.
- Introducing a general filesystem actor framework.

## Decisions

### Mirror the flattened model incrementally

`MaterializedWatchTargets` will be a plain Rust value owned by `WatchRuntimeState`. It maintains a vector parallel to flattened tree rows plus reference-counted deduplicated target keys. `items-changed` splices only removed/added row contributions. Expanded-state notifications refresh the affected row contribution, while collapse-generated removals delete descendant contributions from the parallel vector. Top-level configured folders remain a fallback source before a flattened model is mounted.

Every effective set change produces a sorted `Vec<WorkspaceWatchTarget>` snapshot and advances a typed target generation. Repeated signals that leave the deduplicated set unchanged do not restart the backend.

Alternatives considered:

- Rewalking the whole tree in an idle callback was rejected because it only defers O(N) GTK work.
- Watching every configured root recursively was rejected because it violates the existing broad-folder scalability contract.
- Storing only a set without row contributions was rejected because overlapping rows and collapsed descendants require reference counts.

### Start and retire watchers on workers

The section takes the current `WorkspaceWatcher`, stops its GTK poll source, and sends the old watcher plus an owned target snapshot to `gtk_lush_tasks::spawn_blocking_then`. The worker drops the old watcher and constructs/registers the replacement. GTK installs a successful watcher only if section lifetime and target generation still match.

If a successful watcher returns stale, it is handed to a worker-only disposal helper so debouncer teardown cannot block the completion callback. Empty target sets similarly retire the old watcher on a worker and clear visible watcher errors only for the current generation.

If `WorkspaceWatcher` cannot satisfy the required `Send` boundary on a supported backend, the fallback is a service-owned worker thread with a command channel and a poll receiver; synchronous GTK construction is not an acceptable fallback.

### Preserve the last visible tree and explicit errors

Watcher replacement temporarily has no active poll source; the rendered tree remains mounted and manual Refresh remains available. Current-generation startup failures update the existing warning state once. Stale failures are ignored. A later successful current generation clears the watcher error and installs polling. Runtime events retain current access-noise filtering and debouncing.

### Bound restart churn

Target-set changes are coalesced through the established settle helper. One restart request supersedes older pending target generations. A completion never recursively starts another watcher; if a newer set is pending, the newest scheduled generation owns the next start. Test instrumentation records target counts and start/dispose generations.

## Risks / Trade-offs

- [Parallel stale startups may temporarily consume watcher resources] → Coalesce target changes, reject by generation, and dispose stale handles off-thread immediately.
- [Incremental row bookkeeping can drift from GTK's flattened model] → Keep a test-only full derivation oracle and assert equivalence across expansion, collapse, refresh, reorder, overlap, and focus-folder fixtures.
- [No watcher is active during replacement] → Preserve the tree, keep manual Refresh available, and rely on the short coalesced handoff rather than blocking GTK.
- [Backend handle may not be `Send`] → Use the documented service-owned worker fallback rather than weakening responsiveness.

## Migration Plan

1. Add `MaterializedWatchTargets` with pure splice/ref-count tests and a full-derivation oracle.
2. Connect flattened-model and expanded-row changes while retaining the old synchronous restart behind a temporary adapter.
3. Move start/replacement/drop to worker dispatch with target generations and stale disposal.
4. Remove `current_watch_targets` full scans and the one-millisecond GTK startup timer.
5. Add slow-backend, stale-completion, error, geometry, and many-expanded-row tests.
6. Rollback can restore full derivation and synchronous start without persisted migration, though release is blocked unless the responsiveness contract passes.

## Open Questions

None at specification time. The implementation must verify `Send` for the concrete watcher on supported targets and select the documented service-worker fallback if needed.
