## 1. Complete Canonical Palette Identity

- [x] 1.1 Extend palette file value objects with canonical identity kept separate from display and activation paths, and update constructors without performing blocking identity work on GTK.
- [x] 1.2 Canonicalize and deduplicate indexed files through the filesystem metadata boundary while preserving first workspace/folder precedence and typed identity failures.
- [x] 1.3 Snapshot the editor's already-known canonical identity for open-tab palette entries and keep raw paths only for presentation and activation.
- [x] 1.4 Pass the complete open canonical-identity exclusion set into workspace scoring before bounded heap retention, preserving deterministic ranking and full result slots.
- [x] 1.5 Add service and grouped-search tests for symlink aliases, overlapping roots/workspaces, failed identity resolution, and `max_per_source = 1` with an excluded best match plus a distinct fallback.
- [x] 1.6 Keep `finish-search-pipeline-hardening` unarchived until its canonical deduplication scenario is satisfied by this implementation and its focused regression evidence passes.

## 2. Bound Palette Source Construction

- [x] 2.1 Define GTK-free file-index and note-source admission constants, retained-state accounting, cancellation checkpoints, truncation reasons, and typed complete/cancelled outcomes.
- [x] 2.2 Add an index-specific cancellable directory traversal that retains bounded rows for the remaining 100,000-file capacity without materializing an unbounded flat directory.
- [x] 2.3 Keep canonical deduplication and folder interning within admitted index bounds, and release partial indexes promptly on cancellation or stale scope.
- [x] 2.4 Add one-active/one-latest file-index rebuild ownership using compact folder/scope requests, then replace generation-only window rebuild dispatch.
- [x] 2.5 Enforce the 10,000-entry and 64 MiB searchable-text limits while loading bookmark, folder-note, document-note, and open-tab note sources in deterministic order.
- [x] 2.6 Return note-source truncation evidence alongside recovery diagnostics without retaining rejected full bodies or content-bearing diagnostic strings.
- [x] 2.7 Add one-active/one-latest note-source refresh ownership with cooperative sidecar-scan cancellation and compact pending scope/editor metadata.
- [x] 2.8 Wire accepted current source outcomes and visible bounded-truncation feedback into the command palette without changing group order, activation, or private automation snapshot policy.
- [x] 2.9 Add unit, property, and window tests for huge flat directories, aggregate note count/byte boundaries, cancellation, repeated supersession, exact latest-result acceptance, and bounded retained request state.

## 3. Build the Bounded Buffer Replacement Foundation

- [x] 3.1 Extract plain replacement policy for the calibrated direct threshold, 64 Ki-character clear slices, 256 KiB UTF-8 insertion slices, and clear-only versus clear-and-insert plans.
- [x] 3.2 Implement one editor-owned `BufferReplacementSession` with weak lifetime ownership, current workflow ticket checks, GLib source ownership, one retained replacement body, and typed terminal outcomes.
- [x] 3.3 Add a scoped projection/edit/save guard that suspends syntax, minimap, history, draft, monitor, modified-line, cursor, and conflicting save behavior until exact terminal cleanup.
- [x] 3.4 Ensure completion, cancellation, stale generation, replacement supersession, and disposal remove sources and release text/guards exactly once without publishing partial success.
- [x] 3.5 Add plain policy and GTK widget tests for awkward Unicode boundaries, empty clear-only work, old/new size threshold combinations, per-turn bounds, final text equivalence, and every terminal path.

## 4. Adopt Bounded Replacement Across Editor Workflows

- [x] 4.1 Migrate memory eviction to clear-only sessions, revalidate eligibility between slices, and mark residency released/evicted only after current completion.
- [x] 4.2 Classify draft-recovery local-history policy from incoming UTF-8 size, eliminate ineligible baseline cloning, and transfer/share eligible immutable body ownership.
- [x] 4.3 Migrate eager and lazy draft recovery installation while preserving the complete restore ticket, dirty state, modified marks, minimap behavior, inline feedback, and durable recovery evidence.
- [x] 4.4 Migrate local-history restore and immediate undo while preserving the required pre-restore snapshot, path/history generations, modified state, and reversible action semantics.
- [x] 4.5 Migrate save-time formatting rewrites while preserving save/path/load/edit freshness and keeping the editor non-saveable until the accepted formatted text is fully installed.
- [x] 4.6 Add editor/window/widget race tests proving stale or disposed sessions cannot clear newer work, complete partial saves, misreport eviction, clear recovery state, or lose the immediate history undo body.

## 5. Make Draft Cleanup Eventually Complete

- [x] 5.1 Add optional backward-compatible cleanup-continuation metadata to the public v1 draft manifest payload with Serde defaults and old/new fixture coverage.
- [x] 5.2 Add a filesystem-boundary directory-page helper that selects the next bounded lexicographic page after a filename cursor with bounded retained memory and explicit more-work/wrap evidence.
- [x] 5.3 Extend orphan inspection plans and outcomes with directory continuation while keeping manifest paging, inode evidence, retention reasons, and typed failures intact.
- [x] 5.4 Advance or wrap continuation only through the process-wide manifest lock and durable accepted manifest update, preserving concurrent autosave upserts and later deletion intent.
- [x] 5.5 Resume trusted continuation after restart and schedule rate-limited deferred pages without synchronous loops; skip cleanup and preserve evidence when manifest or cursor recovery is untrusted.
- [x] 5.6 Add deterministic tests with more than 2,048 live/retained prefix entries followed by orphans, restart between pages, insert/remove/rename churn around the cursor, repeated faults, wraparound, and no false committed removals.

## 6. Batch Large Workspace-Tree Reconciliation

- [x] 6.1 Maintain a bounded plain row mirror for each materialized child store and update it with initial population and every accepted splice.
- [x] 6.2 Compute compact prefix/middle/suffix reconciliation plans from the plain mirror and worker scan result without repeatedly scanning all current GObject rows on GTK.
- [x] 6.3 Apply large changed ranges through generation-guarded 256-row GTK batches while retaining the calibrated direct path for small changes.
- [x] 6.4 Capture selection and expansion identity before mutation, then finalize caches, surviving row state, watcher targets, errors, and `workspace-refresh-complete` exactly once after the final current batch.
- [x] 6.5 Cancel stale plan sources on newer scan, store replacement, filter hide, section lifetime change, or disposal without allowing stale readiness or projection finalization.
- [x] 6.6 Add service/widget tests for 10,000-row prefix and middle changes, supersession after partial batching, selection/expansion preservation, manual Refresh during batching, disposal, and exact readiness completion.

## 7. Documentation and Performance Evidence

- [x] 7.1 Correct `session_service::load` and `draft_service::load_manifest` documentation to match recovery-to-default behavior, and review adjacent compatibility wrappers for the same stale error contract.
- [x] 7.2 Extend Criterion and performance-smoke coverage with file-index flat-directory retention/cancellation, note aggregate budgets, coordinator supersession, and canonical pre-top-k exclusion metrics.
- [x] 7.3 Add bounded buffer-replacement diagnostics for eviction, recovery, history, and save formatting that record slice counts, main-loop progress, peak retained bodies, terminal cleanup, and final Unicode equivalence.
- [x] 7.4 Add cleanup-continuation and broad-tree reconciliation scale fixtures that record page/batch bounds, restart/supersession behavior, main-loop progress, and accepted terminal state.
- [x] 7.5 Document chosen constants and benchmark calibration in `docs/benchmarks/`, and update automation/readiness reference and end-user coverage if any observable field, predicate, blocker, or scenario changes.
- [x] 7.6 Refresh accessibility and visual proof artifacts only when their source fingerprints require it, preserving existing command-palette, recovery, history, and workspace visual scenarios.

## 8. Validation and Completion

- [x] 8.1 Run focused palette, filesystem, recovery, editor-memory, local-history, workspace-watch/tree, property, widget, and integration test targets after their respective task groups.
- [x] 8.2 Run the relevant Criterion benchmark target and performance smoke, recording the environment, fixture sizes, retained-state bounds, and calibrated synchronous thresholds.
- [x] 8.3 Run `make check`, `make test-unit`, required headless widget/visual/accessibility smoke lanes, automation self-tests, and any proof-freshness lanes selected by repository policy.
- [x] 8.4 Run strict OpenSpec validation for `complete-end-to-end-boundedness` and all specs/changes, then run `git diff --check` and confirm no unrelated worktree changes were absorbed.
