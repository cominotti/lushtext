## Context

LushText now has strong local policies for durable draft writes, bounded file loading and saving, workspace-search ownership, watcher ingress, GTK slicing, and large-payload retirement. The final review found a narrower composition problem: a local cap or cancellation token can still lose completeness across a restart, enqueue strong ownership before worker admission, rebuild aggregate projections repeatedly, or exceed a different memory dimension.

The change spans GTK-free recovery and replacement services plus GTK driving adapters for session restore, workspace trees, minimap projection, command-palette orchestration, and plain-data retirement. Existing dependency direction remains authoritative: pure policy and persistence evidence stay in `model/` or `services/`, while GTK scheduling, widget lifetime, and projection batching stay in `ui/`. The completed but unarchived `bound-traversal-and-retirement-backlogs` change is part of the implementation baseline; its deltas must be synced before this change is finally archived.

Data-safety invariants dominate ordering. No incomplete repair may authorize deletion, no responsiveness fix may move filesystem work onto GTK, and no bounded queue may discard current user work merely to satisfy a cap.

## Goals / Non-Goals

**Goals:**

- Eliminate the multi-restart draft-loss path while keeping repair and cleanup bounded and off GTK.
- Make session restoration, workspace scanning, minimap analysis, and plain-data retirement bounded at their actual admission point rather than only after work has queued.
- Remove dense-line Replace All allocation amplification and redundant note-body scoring without changing visible results.
- Preserve current error, freshness, readiness, grouping, selection, and accessibility semantics.
- Prove the new invariants at the lowest useful test layer and retain direct high-water evidence in the performance-smoke lane.

**Non-Goals:**

- Reworking the `ui/ -> services/ -> model/` architecture, introducing repository traits, or creating a generic scheduler shared by semantically different workflows.
- Changing public APIs, supported persistence envelopes, GTK Lush public APIs, dependencies, packaging, or user preferences.
- Dropping session tabs, draft bodies, search matches, or current-generation payloads merely to satisfy a responsiveness limit.
- Splitting cohesive adapters solely because of file size or moving app-specific queue policy into GTK Lush.

## Decisions

### 1. A repaired draft manifest is authoritative only after complete inventory proof

Manifest repair will use paginated filesystem-boundary traversal with a fixed page size. It may accumulate repairable entries only while existing entry and metadata-size limits remain satisfied. The repair result will carry explicit completeness and replacement eligibility; a manifest replacement is permitted only after traversal reaches a trustworthy terminal page with no scan, classification, or capacity ambiguity.

If completeness cannot be proven, repair returns bounded partial restore information and diagnostics but does not write an authoritative manifest. The recovery result carries explicit untrusted-manifest state into the window, and every later autosave, session, deletion, or cleanup path that could replace the manifest must either complete a fresh directory reconciliation first or fail retryably without clearing dirty state. The missing or quarantined manifest therefore remains durable evidence that cleanup is untrusted on later startups. Orphan cleanup continues to require a trusted latest manifest, stable target guards, and exact fingerprint revalidation.

This avoids a new manifest version and prevents a partial first-startup inventory or later autosave merge from becoming a clean second-startup input. An alternative serialized `repair_incomplete` flag was rejected because every older reader and writer would need to preserve it correctly; refusing unsafe replacement expresses the invariant with the existing recovery contract.

### 2. Session restore becomes a window-owned bounded state machine

Session tab descriptors remain plain compact data, while GTK page creation stays in the window adapter. A restore coordinator will retain the pending descriptors, create at most a calibrated number of pages per main-loop turn, and admit at most a fixed number of file-backed load-planning operations concurrently. Load-plan terminal outcomes release restore capacity regardless of success, cancellation, or editor lifetime.

The existing projection-batch state will span the whole restore rather than one loop body. `open_document` and `new_tab` paths will honor deferred projection state for palette sources, sidebar row state, recent-open state, status, and related tab-derived views. One terminal rebuild publishes the accepted aggregate state. CLI activation priority, requested active-tab selection, unavailable-file retry state, cursor/scroll restoration, and draft lazy markers remain unchanged.

A hard tab truncation was rejected because silently dropping descriptors could later overwrite recoverable session state. Progressive page creation plus fixed planning admission bounds GTK work and queued ownership without turning scale protection into session loss.

### 3. Replace All streams line discovery alongside sorted replacements

The service will validate the replacement-count cap and replacement-range ordering before line discovery. It will then walk source bytes once with the established byte-search primitives, advancing only enough line boundaries to validate and apply the next sorted replacement. Untouched slices are copied directly into the output; retained edit metadata remains proportional to accepted replacements rather than total source lines.

The output buffer and durable undo bytes remain unavoidable owned payloads and continue to participate in existing per-file and aggregate caps. CRLF handling, stale-line validation, replacement identity, durable journal-before-mutation ordering, and cancellation semantics remain unchanged. Building a `Vec<Range<usize>>` for every line was rejected because it makes a 10 MiB dense-line file substantially larger than the documented undo window before any write begins.

### 4. Workspace scans use per-store active-plus-latest ownership

Each materialized child store will own at most one active scan and one replaceable latest compact request. Request arrival cancels the active generation and replaces only the pending path/options descriptor. Strong store ownership and the current mirror snapshot are captured only when the request actually receives worker admission; queued state uses weak store and section-lifetime identity.

Completion revalidates section lifetime, store identity, target generation, and scan generation before reconciliation. Top-level empty-folder probes will either reuse the accepted directory scan result or carry equivalent per-folder generations so an older slow result cannot overwrite newer filesystem evidence. The generic GTK Lush worker FIFO remains unchanged because the missing invariant belongs to this app-specific producer.

### 5. Plain-data disposal admission is non-blocking and pre-admitted

The app-owned disposal lane will expose non-blocking count-and-byte reservation before a document-sized plain value is transferred onto GTK. Callers with a conservative upper bound reserve before construction; worker-produced values with data-dependent weight remain worker-owned until their measured reservation succeeds. The reservation stays attached while the value is replaceable UI state, and its final `Drop` performs only a guaranteed non-blocking handoff to a disposal worker. An overweight value may reserve the otherwise-empty lane exclusively so bounded policy cannot deadlock progress. No GTK callback may call a blocking channel send or become the fallback destructor for an unreserved document-sized value.

When capacity is unavailable, a producer retains at most one replaceable compact request and one retry/wakeup source; worker-produced completed values stay off GTK until reservation succeeds. The lane's immediate ownership-return path remains useful only for statically small jobs whose replacement can safely drop on GTK. Superseded guarded plain data remains outside current generation state, while GTK-owned objects continue to retire on GTK in bounded slices. Pure `Send` destruction currently expressed as `spawn_blocking_then(... drop ..., no-op GTK callback)` will migrate after the reservation contract is proven; lifecycle-specific watcher teardown remains on its existing worker path.

An unbounded channel, a larger fixed queue, and a generic task-executor rewrite were rejected because none proves aggregate retained bytes or feature-level pending ownership.

### 6. Minimap analysis is sliced, generation-owned, and cacheable

Supported documents keep the minimap visible. Wrapped-layout and optional long-line analysis will first use O(1) size and cached load-time evidence, then use bounded GTK snapshot or iterator slices when more inspection is required. Every slice carries editor lifetime and minimap-analysis generation; edits, wrap changes, preference changes, file replacement, or page teardown cancel stale work before another slice is scheduled.

Accepted analysis results update one cache shared by layout availability and long-line marker projection. Documents already beyond the existing minimap-supported tier retain the current explicit unavailable state. Introducing a new lower byte-only hide threshold was rejected because it would violate the existing supported-tab visibility contract.

### 7. Note scoring prunes only fields that cannot affect the row result

Note ranking will retain the current metadata/body semantics and deterministic source ordering. Scoring records the best accepted metadata-field score before considering the body. A body may be skipped only when the scoring policy proves its maximum possible contribution cannot improve eligibility or ordering for that row; bodies must still be searched when they are the only possible match.

The optimization will live beside the existing GTK-free note scorer and be checked against an unpruned reference across Unicode, ties, metadata-only hits, body-only hits, empty queries, and cancellation. Source limits and result top-k ownership remain unchanged.

### 8. Evidence is layered by the behavior it proves

- Service tests will cover complete versus incomplete repair, two-startup persistence behavior, and exact draft survival across every cleanup page.
- Pure unit and property/reference tests will cover streaming replacement equivalence, note-score pruning equivalence, disposal admission, and compact coordinator policies.
- Integration tests will use isolated temporary data for corrupt/missing manifests and dense-line Replace All workflows.
- Headless widget tests will prove multi-turn session restore, one terminal projection rebuild, bounded in-flight planning, scan supersession, stale emptiness rejection, and minimap cancellation without fixed sleeps.
- Criterion and `make performance-smoke` evidence will record direct counts, bytes, queue high-water marks, GTK turns, generations, and retained owners. No visual-smoke change is required because visible geometry and styling are intentionally unchanged.

## Risks / Trade-offs

- **Pathological draft directories may remain unrepaired when complete inventory cannot fit existing metadata limits** -> Preserve all bodies, report one bounded diagnostic, keep cleanup disabled, and allow later tooling or a future explicit recovery workflow rather than guessing.
- **Progressive session restore may make tabs appear over several frames** -> Preserve order and selection intent, expose restore readiness until terminal publication, and choose page/permit values from deterministic evidence rather than elapsed time alone.
- **A long-lived UI owner could hold a disposal reservation longer than expected** -> Charge the full conservative weight for the owner's lifetime, retain only compact requests while capacity is unavailable, cancel retry sources on teardown, and prove eventual drain plus off-main nested destruction under aggregate pressure.
- **Sliced minimap analysis can publish stale geometry if invalidation is incomplete** -> Centralize generation invalidation for every buffer, wrap, preference, and lifetime transition and accept results only for the current generation.
- **Note-score pruning could subtly alter ties** -> Compare optimized results with the unpruned reference on generated corpora and retain source ordinal as the final tie-break.
- **This change overlaps capabilities touched by the completed predecessor** -> Sync/archive `bound-traversal-and-retirement-backlogs` first or revalidate this delta against its synchronized canonical requirements before final archive.

## Migration Plan

1. Synchronize the completed predecessor change into canonical specs before implementation closeout.
2. Implement and verify draft-repair completeness first; this is the safety gate for all later work.
3. Add pure policies and regression fixtures for session restore, workspace scans, disposal, replacement, minimap, and note scoring before wiring GTK adapters.
4. Migrate call sites incrementally while retaining existing user-visible behavior and readiness semantics.
5. Run focused tests after each workflow, then the repository check, full test, strict OpenSpec, and performance-smoke gates.

Rollback is code-only because no new public persistence envelope is introduced. A rollback must not restore partial-manifest replacement behavior; if later performance work is reverted, the draft safety fix remains independently deployable.

## Open Questions

No product-level questions remain. Exact per-turn, in-flight, count, and retained-byte constants will be calibrated during implementation using the existing benchmark policy, documented alongside their rationale, and asserted directly by tests.
