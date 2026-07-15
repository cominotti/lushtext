## Context

The preceding hardening portfolio established strong typed boundaries for transient file admission, editor residency, buffer replacement, palette search, workspace watcher delivery, and draft cleanup. The final live-tree review found that most of those contracts hold, but a few secondary UI paths still bypass the new patterns: `Browse Notes...` uses the legacy unbounded note loader and GTK-side filtering; local-history preview loads can overlap and install a large snapshot in one GTK call; workspace reconciliation finishes with a quadratic cache rebuild; the debouncer can accumulate raw watcher events before the app-owned bounded mailbox sees them; incremental palette index updates can drop the old index on GTK; and orphan-cleanup retry scheduling ignores `has_more_work` when no continuation cursor exists. Ordinary unit-test builds also cannot see two fault-injection symbols used by `#[cfg(test)]` tests.

These are localized closeout gaps, not a reason to replace the architecture. The implementation should reuse the bounded source admission, cancellation, generation, UTF-8 slicing, background retirement, typed cleanup outcome, and proof infrastructure already present.

## Goals / Non-Goals

**Goals:**

- Make every remaining large-input path retain explicitly bounded state and avoid work proportional to large content or collections in one GTK turn.
- Preserve current Notes grouping, workspace scope, preview, Copy/Restore, sidebar refresh, palette ordering, and conservative draft-recovery behavior.
- Bound watcher state from the raw backend callback through GTK consumption rather than only after a debounced vector is delivered.
- Restore honest default-feature and all-feature validation, with deterministic regression and scale evidence for each repaired seam.

**Non-Goals:**

- Redesigning the Notes, Local History, workspace sidebar, command palette, or draft-recovery user experience.
- Changing app-data formats, public automation contracts, filesystem permissions, GTK Lush public APIs, or user-facing limits without measured justification.
- Introducing a generic task framework or shared abstraction merely because several workflows use generations and cancellation.
- Reopening already-correct editor load, buffer replacement, search ranking, or durable-write implementations.

## Decisions

### 1. Reuse bounded note-source admission, but give the Notes browser its own request lifecycle

`Browse Notes...` will stop calling the legacy unbounded loader and use the existing bounded sidecar scan, aggregate entry/text/diagnostic admission, recovery diagnostics, and cooperative cancellation path. Browser-specific policy may choose independently named limits, but it should share the same plain admission mechanics and typed truncation reasons so the two note surfaces do not diverge silently.

Open-editor snapshots will be capped while GTK-owned bookmark state is collected, before worker submission. The browser will retain one admitted immutable source, one active query, and at most one latest compact query request. Matching—including note-body matching—will run off GTK, check cancellation periodically, retain only the existing rendered-result cap, and publish only the current generation. The UI will continue to group rows and preview them on GTK and will visibly report when source admission or rendered results were truncated.

An alternative was pagination over every sidecar. That would preserve discoverability beyond a cap, but it requires a larger user-facing navigation and ordering design. A bounded admitted inventory is the smaller closeout consistent with the palette policy and existing 500-result browser contract.

### 2. Local-history preview owns one active load, one latest selection, and one sliced installer

Selecting a snapshot will submit compact path/snapshot metadata to a browser-local coordinator. A newer selection cancels the current read cooperatively and replaces the single pending slot. Snapshot reading will remain size-gated and will check cancellation between bounded read chunks; cancelled or stale payloads will be dropped off GTK.

An accepted non-empty preview will be installed into the read-only `TextBuffer` in UTF-8-safe slices using the established byte-boundary policy. Generation and dialog lifetime checks will guard every slice. Copy and Restore remain disabled until installation finishes, and the browser retains only the accepted snapshot needed by those actions. Empty, missing, and error states remain direct.

Reusing the editor's full buffer-replacement session was rejected because that session owns editor-specific modified-state, projection, and recovery semantics. The preview should reuse the plain slice policy, not the editor workflow object.

### 3. Workspace row-cache acceptance becomes one bulk linear commit

Terminal child reconciliation will construct the accepted sibling vector, item locations, and per-path occurrence deltas in temporary plain collections, then update the cache in bulk. It will not insert each row through `cache_child_item()` or repeatedly shift all later locations. Duplicate-path accounting and expanded-store lookup semantics must match the current cache oracle.

This keeps the existing 256-row model splices and 10,000-row scan cap while removing the quadratic terminal phase. Slicing the old quadratic algorithm was rejected because it would reduce individual stalls but retain excessive total work and more cancellation state.

### 4. Watch raw events directly into the bounded mailbox

The watcher will use the existing `notify` backend surface directly rather than allowing `notify-debouncer-full` to build an app-unbounded event collection before callback delivery. Each raw callback will classify one event outside GTK and merge only its small path set, bounded diagnostic, or conservative full-refresh need into the existing mailbox. The GTK poll/debounce layer remains the temporal coalescing boundary.

Producer lock contention must not create a queue or block GTK. A constant-space atomic full-refresh/error latch will conservatively record work that cannot immediately acquire the mailbox; the next producer merge or GTK poll folds that latch into the notice. Ambiguous rename shapes also promote to full refresh. Existing materialized-target registration and lifecycle-generation ownership remain unchanged.

Keeping the full debouncer was rejected because post-callback truncation cannot bound memory already retained inside its per-path event queues.

### 5. Every large palette index leaves GTK through one retirement helper

Full replacement, accepted incremental replacement, and rejected/stale incremental results will all use one helper that swaps or receives the `Arc<FileIndex>` and schedules its final drop on the existing bounded worker lane. Generation comparison and replay ordering remain unchanged; only destruction ownership moves.

This is preferable to assuming another `Arc` owner exists, because an idle closed palette can make the GTK callback the last owner of a 100,000-row allocation.

### 6. `has_more_work` is the cleanup scheduler contract

The window will schedule exactly one coalesced follow-up whenever a trusted cleanup outcome reports `has_more_work`, regardless of cursor presence. A continuation cursor resumes pagination; a retryable failure without a cursor restarts from the safe beginning. Pagination may use the existing fixed delay, while repeated failures use bounded exponential backoff so persistent filesystem faults do not create a tight loop. New outcomes supersede older timers, and disposal cancels pending work.

The service remains conservative: failed or ambiguous artifacts stay retained, committed removals alone merge into visible state, and untrusted startup state still skips destructive cleanup.

### 7. Test-only fault seams compile wherever their tests compile

Draft-cleanup fault types and helpers used by in-module `#[cfg(test)]` tests will be available under `cfg(test)` as well as the existing property/test-utils features. Production builds remain unable to call them. Completion requires both the default-feature unit target and the all-feature target so all-feature Clippy cannot mask a default-feature compile failure again.

## Risks / Trade-offs

- **Bounded Notes admission can omit a later matching note** → Surface typed truncation clearly, preserve deterministic source order, measure realistic inventories, and keep limits as named browser-owned policy values.
- **Sliced local-history installation briefly holds the accepted text and partial GTK buffer together** → Admit only one active payload, retain only one accepted snapshot, release stale payloads off GTK, and record peak ownership in tests.
- **Raw watcher events can lose debouncer-specific rename pairing** → Preserve paths when the backend supplies them and conservatively request a full refresh for ambiguous or overflowed rename state.
- **A bulk cache implementation can corrupt duplicate occurrence counts** → Compare every generated splice sequence against the existing full derivation oracle before removing the old per-row rebuild path.
- **Persistent cleanup failures can retry forever** → Coalesce timers, use capped backoff, emit bounded diagnostics, and stop automatically when the window is disposed or a later pass reports no work.
- **Moving destruction off GTK can reorder allocator cleanup** → Keep generation and replay decisions on GTK; transfer only otherwise-unreferenced `Arc` ownership to the worker.

## Migration Plan

1. Repair the default test cfg seam first and establish passing default/all-feature baselines.
2. Implement and test the plain coordinators, admission, bulk-cache, watcher-ingress, and retry decisions before GTK wiring.
3. Integrate Notes, local-history, sidebar, palette, and draft-window adapters one workflow at a time, running focused tests after each group.
4. Add scale benchmarks and headless/widget proof selected by repository policy, then run the complete validation matrix and strict OpenSpec validation.

There is no persisted-data migration. Rollback is source-only: each workflow can return to its previous implementation without transforming user data, although the default-test repair and any new regression fixtures should remain.

## Open Questions

None. Exact numeric thresholds should be calibrated from existing palette, file-load, tree, and performance fixtures during implementation and recorded as named policy values with benchmark rationale.
