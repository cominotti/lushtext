## 1. Establish Ordering and Fault Baselines

- [x] 1.1 Record current delayed-restore, autosave/delete, one-complete-body, and local-history baseline tests plus the exact draft/body/manifest mutation call graph.
- [x] 1.2 Add pure interleaving fixtures for monotonic draft intent, per-draft epochs, stale completion rejection, later-edit recovery, and wrap-safe generation comparisons.
- [x] 1.3 Add deterministic fault seams for ordinary fallback restore delay, body/manifest/delete ordering, and local-history baseline persistence failure without weakening production I/O boundaries.

## 2. Unify Draft Restore Freshness

- [x] 2.1 Introduce one private restore-ticket representation covering weak editor lifetime, draft ID, exact manifest-entry facts, expected path, dirty/edit generation, and load generation.
- [x] 2.2 Route untitled fallback, file-backed fallback, and aggregate-budget lazy restore through one final currentness check before content, feedback, or deletion is applied.
- [x] 2.3 Ensure stale restore outcomes preserve recovery evidence and cannot delete a newer manifest entry or mutate a reused/reloaded editor.
- [x] 2.4 Add widget tests for edit, reload, rename/path change, manifest replacement, and editor close while each restore path is delayed.

## 3. Order Draft Persistence Mutations

- [x] 3.1 Add window-owned per-draft mutation epochs and main-thread intent sequencing with pure tests for autosave, Save, discard, cleanup, and later-edit transitions.
- [x] 3.2 Implement one single-flight draft persistence coordinator whose compact commands contain no GTK objects and whose completions preserve weak/scalar freshness facts.
- [x] 3.3 Route autosave body write plus manifest acceptance and draft body plus manifest deletion through the ordered coordinator without queuing more than one complete body.
- [x] 3.4 Reject obsolete autosave admission/completion after a newer deletion intent while allowing a strictly later dirty generation to create new recovery.
- [x] 3.5 Preserve retryable dirty state and bounded content-free feedback for body, manifest, and delete failures, including ambiguous post-write outcomes.
- [x] 3.6 Add deterministic widget/integration interleavings where Save or discard occurs before body completion, before manifest acceptance, and after upsert dispatch; assert final disk and in-memory manifest state.
- [x] 3.7 Re-run close-time flush, empty-draft, orphan-cleanup, rename, startup-restore, and retained-complete-body high-water tests against the coordinator.

## 4. Make Local-History Baselines Retryable

- [x] 4.1 Carry failed baseline text back in the worker outcome without cloning it and capture editor/path/clean-baseline generations for conditional restoration.
- [x] 4.2 Restore and enqueue one bounded retry only when the original editing cycle is current and no newer clean baseline exists.
- [x] 4.3 Add injected-failure tests for same-cycle retry, Save As/rename, save-established newer baseline, editor disposal, deduplication, and permit release.

## 5. Contracts and Verification

- [x] 5.1 Add concise high-signal comments for intent assignment, coordinator command ordering, and baseline ownership; update root/nested guidance if the durable workflow contract changes.
- [x] 5.2 Run focused service, integration, and widget tests under repeated scheduling, plus `make test-unit`, relevant headless widget suites, and draft/local-history property coverage.
- [x] 5.3 Run `make check`, `make lint-advisory`, `make pre-commit`, `git diff --check`, and strict OpenSpec validation; fix every blocker discovered in the work stream.
- [x] 5.4 Run relevant accessibility, visual-geometry, automation-readiness, and runtime-warning smoke surfaces because source-byte and asynchronous lifecycle changes can invalidate proof artifacts.
- [x] 5.5 Perform a final scoped data-safety review confirming restore freshness, one-body backpressure, success-gated dirty clearing, ordered deletion, durable filesystem routing, and failure retryability.
