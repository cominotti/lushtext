## Context

The preceding improvement portfolio established strong primitives for byte-weighted file-load admission, active-plus-latest search ownership, bounded GTK buffer mutation, plain-data worker disposal, recovery progress reservations, durable JSON writes, and generation-aware readiness. The follow-up review found that five workflows still stop using those primitives one ownership boundary too early:

1. content search retains one visited `PathBuf` for every file even when one traversal root makes revisits impossible;
2. decoded file bodies cross onto GTK with accounting permits but without guaranteed worker-side final disposal;
3. eager and lazy draft bodies can escape their aggregate or worker ownership before buffer replacement and baseline transfer;
4. workspace persistence clears dirty state before the write succeeds and is absent from close safety; and
5. Notes admission accounts final rows but not the path, sidecar, canonicalization, diagnostic, and category scratch that overlaps construction.

These are end-to-end ownership and terminal-state gaps, not evidence that the recent architecture work failed. The implementation should therefore finish adoption of existing mechanisms without reopening broad decomposition or introducing a generic resource-management framework.

## Goals / Non-Goals

**Goals:**

- Make workspace-search retained identity memory independent of visited-file count for the common single-root case and explicitly bounded for all remaining alias cases.
- Guarantee that document-sized decoded file and recovered draft bodies are finally destroyed off GTK on stale, cancellation, teardown, replacement, and accepted-baseline paths.
- Keep workspace state dirty or failed until the newest snapshot is durably saved, expose failures to users and automation, and make close wait for or retry that terminal snapshot.
- Bound Notes construction scratch separately from its final retained source and expose measured peak/truncation evidence.
- Prove the new contracts with direct ownership, byte, cardinality, thread, generation, retry, readiness, and close-time assertions.

**Non-Goals:**

- Changing workspace, search-result, Notes grouping, draft-recovery, or file-loading user semantics.
- Changing JSON envelopes, sidecar formats, action names, D-Bus members, readiness predicate names, or persisted settings.
- Adding a global scheduler, generic persistence manager, resource-manager traits, a new crate, or an external dependency.
- Replacing the approved content-search engine or moving GTK-owned widgets, buffers, models, tags, or callbacks off the GTK thread.
- Broad module splitting or cleanup unrelated to these confirmed boundaries.

## Decisions

### 1. Plan content-search roots once and make file identity retention exceptional

Before starting the search engine, build one immutable traversal plan from the ordered workspace folder snapshot. Resolve canonical folder identities once where possible, remove exact duplicates, and collapse covered descendants into a minimal traversal-root set. Keep a separate ordered ownership map so a result is still attributed to the first configured folder that would have owned it before normalization.

A single effective traversal root does not need a visited-file set: the engine's one walk cannot emit the same directory entry twice under the supported non-following traversal policy. Multiple normalized roots that are provably disjoint also avoid per-file identity retention. Only unresolved canonicalization, symlink aliases, or other coverage ambiguity may enable a fallback identity ledger. That ledger receives explicit entry and conservative path-byte limits and stops with a typed incomplete-search terminal if the next identity would exceed either limit. It never silently drops deduplication while describing the search as complete.

This keeps the common no-match scan at O(number of roots) retained identity state while preserving result order, folder precedence, cancellation, result caps, diagnostics, and active-plus-latest ownership.

Alternatives considered:

- Keep the unconditional `HashSet<PathBuf>` and add a large cap. This still spends memory proportional to every file in ordinary single-root work and merely moves the cliff.
- Remove deduplication entirely. This would reintroduce duplicate results for overlapping or aliased workspace folders.
- Add a global traversal cache. Its lifetime and invalidation would be broader than one search generation and unnecessary for this gap.

### 2. Attach disposal ownership to decoded load bodies, not only byte accounting

File-load admission remains responsible for transient byte pressure and exact permit release. At worker admission it also reserves future plain-disposal capacity using the conservative supported-body bound. The worker shrinks that reservation to the measured decoded-body weight and returns the successful body inside `DisposalOwned` (or a workflow-specific newtype around it). Error-only outcomes remain compact and do not consume a body reservation.

The guarded body crosses the generic GTK completion callback, weak-editor upgrade, generation check, direct or sliced installation, and terminal finalization without being unwrapped merely to call GTK. `ChunkedLoadInstall` owns the guard while slices borrow the text. A current accepted body transfers the same guard into an eligible `last_clean_text` baseline; that accepted-current transition releases the bounded transit reservation and keeps a guaranteed nonblocking retirement handle to the same disposal workers. Long-lived baselines therefore cannot consume the eight physical transit slots. If policy does not retain a baseline, the guard retires after successful installation. Stale completions, lost editors, decode failures carrying partial bodies, cancelled sliced installs, and teardown keep pre-admitted transit ownership until their final worker-side retirement.

The ordinary lane admits at most one supported overweight file-load reservation alongside existing ordinary transit ownership under an explicit maximum additive byte peak. While that owner is overweight, further admission stays blocked until shrink or terminal release returns aggregate transit ownership within the ordinary ceiling. This preserves progress for supported decoded bodies above the ordinary 128 MiB capacity without making the bound open-ended.

`TransientLoadPermit` remains separate because it models in-flight load pressure, while `DisposalOwned` models the thread on which the final plain-data destructor runs. Both have exact terminal release rules. The implementation must not change `gtk-lush-tasks` into a LushText-specific disposal dispatcher.

Alternatives considered:

- Make the generic `spawn_blocking_then` callback destroy all rejected results on a worker. The generic helper cannot know which result members are GTK-owned or document-sized and would hide workflow freshness semantics.
- Clone the decoded text for the baseline and immediately retire the worker result. This doubles the body and defeats the existing transfer-oriented design.
- Treat accepted loads as safe to unwrap because they are current. Currentness does not prevent later GTK-side baseline replacement or editor teardown from performing the final large destructor.

### 3. Give each recovered draft body transferable guarded ownership

Draft restoration uses the same plain-disposal primitive but keeps draft-specific ticketing and recovery semantics. Lazy restore reserves progress-lane disposal capacity before the file read, shrinks it to the resolved UTF-8 body, and publishes a guarded restore resolution. If progress capacity is unavailable, the existing serialized lazy queue retains only the compact ticket and waits for capacity; it does not read and retain an unguarded body.

Startup eager preloads must no longer expose a raw `String` by removing it from one aggregate guarded map. During worker-side preload construction, represent admitted bodies as individually guarded values or construct a compact per-body ownership table whose reservation can be transferred without scanning or dropping text on GTK. Extracting one body moves its guard into `BufferReplacementRequest::new_guarded`; releasing unused eager bodies uses the existing worker-retirement split. The accepted replacement callback returns the guard and moves it into an eligible local-history baseline, converting bounded progress transit ownership into the same accepted-current retirement handle so retained Notes and draft sources cannot exhaust the two progress slots. Ineligible baselines retire the guard off GTK instead of wrapping the body with `small_unreserved`.

Draft restore tickets, manifest authority, dirty/load generations, and ordered deletion semantics remain unchanged. Guard transfer is ownership plumbing, not a new recovery domain abstraction.

Alternatives considered:

- Keep one aggregate reservation and remove raw values from it. The reservation would remain attached to the map while the heavy value escapes, so dropping the body could still occur on GTK.
- Add a special draft disposal thread. The existing progress lane already provides bounded non-blocking worker destruction and avoids another lifecycle.
- Reduce the 64 MiB automatic draft limit. That would change recovery capability without fixing the ownership bug for bodies below the new limit.

### 4. Model workspace persistence as explicit latest-state terminal state

Add a small plain-Rust workspace-persistence state machine owned by the sidebar workflow. It tracks the newest requested generation, the generation and snapshot currently in flight, the newest durably accepted generation, and an optional failed generation/error summary. Derived predicates provide `has_pending_work`, `is_failed`, and whether a new worker may start.

A mutation advances the requested generation and schedules the existing debounce. Starting a worker does not mark that generation clean. Success marks only the matching generation durable and immediately starts or schedules the newest pending snapshot if mutations arrived meanwhile. Failure preserves the generation as pending, stores a user-safe error, and schedules bounded backoff or awaits a later explicit retry/mutation; it never loops tightly. Visible status feedback offers the current failure and retry path. Existing automation readiness and snapshot pending state remain unsettled while work is dirty, in flight, failed, or waiting for retry, without adding private persistence identifiers or a new D-Bus member.

Window close extends the existing asynchronous safety pipeline: after draft flush and before final session save/destruction, request a no-debounce flush of the newest workspace snapshot. If a save is already in flight, the close stage waits and then persists any newer generation. Failure aborts close, restores window usability, and leaves retryable workspace state. The final close fingerprint/revalidation still protects documents changed during the transaction.

The state machine remains specific to workspace JSON. Draft, session, and workspace persistence have different ordering and recovery rules, so a generic persistence coordinator would obscure rather than unify them.

Alternatives considered:

- Set `persist_dirty` back to true on failure. This fixes one retry bug but cannot distinguish a failed terminal from ordinary debounce, cannot make readiness truthful, and does not solve close-before-debounce.
- Perform a synchronous workspace save in `close_request`. Slow or unavailable storage would block GTK and duplicate the established async close transaction.
- Report errors only in tracing. That leaves users and automation believing the state is safely settled.

### 5. Separate Notes retained-source and construction-scratch budgets

Extend `NoteSourceLimits` and `NoteSourceMetrics` with construction-scratch and sidecar-path byte ceilings plus measured current and peak construction bytes. Use `scan_directory_bounded_with_cancel_and_bytes` with the lesser of its path sub-budget and remaining aggregate construction capacity so a directory containing many maximum-length or Unicode sidecar names cannot retain an uncharged path vector. Before each recovery read, preflight the exact current file size and reserve raw input plus a four-times parsed-model envelope and a bounded diagnostic allowance; pass that admitted size back into the recovery loader so a growth race is rejected before reading. Shrink the reservation to the measured returned document and diagnostic graph, then charge canonical folder/path copies, temporary category vectors/capacity, and other concurrently live construction allocations with saturating arithmetic.

The initial policy should retain the existing 10,000-entry and 64 MiB final-source limits while defining a separate documented construction ceiling. The path sub-budget must leave room for one maximum permitted recovery-metadata input; implementation calibration may choose the smallest conservative ceiling proven by the heap-weight helpers and fixtures. When the next complete path, sidecar, diagnostic, or entry would exceed its applicable bound, stop at a deterministic boundary, record a typed truncation reason, and publish the admitted source. Cancellation releases scratch on the worker before any completion reaches GTK.

Only the final measured source crosses to GTK and shrinks the progress-lane reservation. Construction metrics accompany the outcome as compact scalars; scratch allocations do not escape in diagnostics.

Alternatives considered:

- Count only sidecar entries. Path length and one bounded-but-large sidecar body make entry count an insufficient memory bound.
- Charge scratch against the existing 64 MiB retained-source limit. That would unpredictably reduce user-visible rows based on traversal order and conflate transient peak with installed state.
- Increase the progress reservation without measuring scratch. A larger constant does not prove the worker's peak construction graph is bounded.

### 6. Verify boundaries directly and keep routine/deep evidence tiered

Pure tests exercise root-plan normalization, fallback-ledger limits, persistence transitions, generation ordering, retry backoff decisions, and Notes saturating byte accounting. Service tests use filesystem fixtures for overlapping roots, canonicalization failure, long paths, oversized sidecars, and injected workspace writes. Headless GTK/widget tests use destructor-thread sentinels and main-loop heartbeats for lost-editor, stale load, sliced cancellation, eager/lazy draft supersession, baseline replacement, close-before-debounce, failure recovery, and readiness state.

Near-limit no-match trees, 500 MiB-class supported load policy, 64 MiB drafts, and maximum Notes construction remain performance-smoke or Criterion fixtures with direct high-water artifacts rather than routine wall-clock thresholds. Focused disposal tests also run with debug assertions disabled so required mutations cannot hide inside assertions.

## Risks / Trade-offs

- [Root normalization changes traversal ownership or result attribution] → Preserve a separate ordered display-ownership map and compare normalized results against the current overlapping-root reference behavior.
- [Canonicalization fails for an unavailable root] → Keep the root searchable under path fallback, use the bounded identity ledger only for ambiguous coverage, and report typed incompleteness if its bound is reached.
- [Long-lived accepted state exhausts bounded disposal transit slots] → Convert accepted current baselines and sources to the same-worker nonblocking retirement handle, release transit admission immediately, and prove nine editor baselines plus two progress owners do not block later work.
- [Progress capacity is temporarily unavailable for draft restore] → Keep only compact restore tickets in the existing serialized queue and wake on capacity release; never hold an unguarded body while waiting.
- [Close waits on slow workspace storage] → Keep the window insensitive within the existing asynchronous close transaction, publish progress, and abort safely with a visible retryable error rather than blocking GTK or discarding state.
- [Workspace retry repeatedly hits a permanent error] → Use bounded backoff and explicit user/new-mutation retry triggers; readiness stays honestly blocked without a tight loop.
- [Conservative Notes accounting truncates earlier than today] → Keep retained and scratch reasons distinct, calibrate structural overhead from direct heap-weight evidence, and preserve all admitted-row semantics.

## Migration Plan

1. Add pure policy/state types and deterministic tests for search traversal planning, workspace persistence, and Notes scratch accounting.
2. Adopt guarded ownership in file-load results and installers, then in eager/lazy draft restore and baseline transfer.
3. Integrate workspace persistence state with debounce, status, automation readiness/snapshot, and asynchronous close safety.
4. Integrate Notes byte-bounded traversal and construction metrics without changing final-source ordering.
5. Add widget, release-semantic, and performance evidence; run the complete repository gate stack.

No data migration is required. Rollback is a normal code rollback because persisted formats and public actions remain unchanged. If close-pipeline integration must be reverted independently, the persistence state machine and truthful error/readiness behavior can remain while close flushing is repaired; no saved workspace payload becomes incompatible.

## Open Questions

None. Exact calibrated Notes construction and path sub-budget constants are implementation measurements within the normative requirement that both are explicit, conservative, and directly tested.
