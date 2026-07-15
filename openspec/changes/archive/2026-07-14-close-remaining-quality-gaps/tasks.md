## 1. Restore Honest Test Configuration

- [x] 1.1 Make the draft-cleanup fault enum and execution helper available under `cfg(test)` as well as the existing property/test-utils features, without exposing either seam to production builds.
- [x] 1.2 Run the focused draft-cleanup unit tests under the default feature set and add a regression assertion that the delete- and manifest-fault paths compile and retain failed artifacts.
- [x] 1.3 Establish passing default-feature and all-feature `lushtext-core` unit-test baselines before beginning the behavioral changes.

## 2. Bound Notes Browser Source and Query Ownership

- [x] 2.1 Refactor the existing note-source admission so `Browse Notes...` can reuse bounded sidecar scans, entry/text/diagnostic budgets, typed truncation reasons, and cooperative cancellation without duplicating palette policy internals.
- [x] 2.2 Replace the browser's unbounded loader and `usize::MAX` open-editor snapshot path with browser-owned named limits, deterministic admission order, and bounded recovery/truncation evidence.
- [x] 2.3 Add a GTK-free Notes query request, cancellation token, and one-active/one-latest coordinator that scans the immutable admitted source off GTK and retains only the existing ordered render cap.
- [x] 2.4 Wire the browser dialog to publish only current source/query generations, preserve grouped sidebar, selection, preview, scope, de-duplication, and Open behavior, and surface source truncation separately from render truncation.
- [x] 2.5 Cancel browser source/query work on disposal and retire stale or last-owned large entry/result payloads away from the GTK callback when necessary.
- [x] 2.6 Add unit, property, and widget coverage for aggregate admission limits, open-editor snapshot limits, no-match full scans, rapid supersession, stale completion, truncation messaging, and dialog disposal.

## 3. Bound Local-History Preview Loading and Installation

- [x] 3.1 Add a plain one-active/one-latest preview-load coordinator with compact selection requests, cooperative cancellation, scalar ownership evidence, and current-generation acceptance.
- [x] 3.2 Extend bounded snapshot reading to check cancellation between read chunks while preserving size classification, typed missing/error outcomes, and snapshot metadata integrity.
- [x] 3.3 Implement a browser-local UTF-8-safe preview installation session with named direct/sliced thresholds, bounded slice size, generation/lifetime checks, and complete source cleanup on every terminal outcome.
- [x] 3.4 Keep Copy and Restore disabled until the current snapshot is fully installed, retain only that accepted snapshot, and preserve current empty, missing, error, safety-snapshot, and restore-undo behavior.
- [x] 3.5 Add service and widget tests for rapid large-snapshot selection, cancellation during read and installation, Unicode slice boundaries, dialog disposal, exact final text, and action sensitivity.

## 4. Close Workspace Refresh Scale Gaps

- [x] 4.1 Replace full-backend debouncer accumulation with direct raw `notify` callback handling that filters access noise and normalizes each small event before it reaches GTK.
- [x] 4.2 Extend the watcher mailbox with constant-space overflow/contention latching so path overflow, ambiguous rename state, or producer lock contention conservatively produces one full refresh without a queue or producer block.
- [x] 4.3 Preserve materialized target registration, watcher lifecycle generations, target-count evidence, disconnect/error semantics, and nonblocking one-notice GTK polling across the raw watcher transition.
- [x] 4.4 Implement an O(n) bulk child-cache rebuild that replaces sibling paths, item locations, and visible occurrence counts without per-row calls to `shift_cached_indices()`.
- [x] 4.5 Wire terminal reconciliation to accept the bulk cache atomically only for the current token and avoid cloning the complete accepted mirror when borrowing or ownership transfer is safe.
- [x] 4.6 Add deterministic watcher tests for raw-event storms, rename ambiguity, cap overflow, contention, error/disconnect overlap, and bounded polling; add property/oracle tests for duplicate, reordered, removed, inserted, and superseded child mirrors.

## 5. Retire Command-Palette Indexes Off GTK

- [x] 5.1 Extract one private file-index retirement helper that transfers final `Arc<FileIndex>` destruction to the existing bounded worker lane.
- [x] 5.2 Use the helper for full replacements, accepted incremental replacements, and stale or rejected incremental worker outputs while preserving generation increments and replay ordering.
- [x] 5.3 Add deterministic ownership tests for last-owned 100,000-row indexes in every replacement outcome and verify that visible query refresh remains tied to the accepted generation.

## 6. Make Draft Cleanup Follow `has_more_work`

- [x] 6.1 Extract a plain cleanup-follow-up decision that treats `has_more_work` as authoritative, resumes pagination cursors, restarts cursorless retryable work from zero, and resets after terminal completion.
- [x] 6.2 Add coalesced timer ownership and bounded exponential failure backoff so each window owns at most one cleanup worker and one follow-up, with disposal and newer outcomes superseding older timers.
- [x] 6.3 Preserve conservative retention, committed-removal merging, warning diagnostics, untrusted-manifest skipping, and no synchronous retry loops while wiring the new decision into the window workflow.
- [x] 6.4 Add unit and window-level decision tests for final-page delete/status/manifest faults, repeated failure backoff, cursor pagination, concurrent autosave merging, later success, terminal stop, and disposal.

## 7. Add Scale Evidence and Documentation

- [x] 7.1 Extend Criterion fixtures to record Notes admitted entries/searchable bytes/query ownership, local-history preview slices and retained payloads, raw watcher ingress state, and terminal child-cache rebuild operation counts separately from reconciliation planning.
- [x] 7.2 Add lightweight performance-smoke coverage that demonstrates main-loop progress during large Notes queries and local-history preview installation and detects a restored quadratic workspace cache rebuild.
- [x] 7.3 Record command-palette retirement and draft-cleanup retry ownership through deterministic counters or test hooks that expose scalar evidence only and do not leak user content or persistence identifiers.
- [x] 7.4 Document every chosen cap, threshold, slice size, retry delay, and benchmark calibration in the owning code and `docs/benchmarks/`; update automation/readiness and end-user coverage only if observable contracts actually change.
- [x] 7.5 Refresh accessibility or visual proof artifacts only when repository fingerprint policy selects the changed Notes, history, or sidebar surfaces.

## 8. Validate and Close Out

- [x] 8.1 Run `make check`, `cargo test -p lushtext-core --lib`, and the matching all-feature library test target; confirm default-feature compilation can no longer be masked by all-feature Clippy.
- [x] 8.2 Run focused palette/Notes, local-history, workspace watcher/tree, command-palette index, draft-cleanup, property, integration, and headless widget test targets.
- [x] 8.3 Run the relevant Criterion targets and `make performance-smoke`, recording environment, fixture sizes, retained-state bounds, operation counts, and calibrated thresholds.
- [x] 8.4 Run `make check-automation-docs`, `make automation-client-self-test`, and any accessibility, visual-geometry, builder-diagnostics, or proof-freshness lane selected by repository policy.
- [x] 8.5 Run `openspec validate close-remaining-quality-gaps --strict`, `openspec validate --all --strict`, and `git diff --check`, then reconcile every checkbox with actual evidence before marking the change complete.
