## Context

The previous boundedness work established explicit file, row, byte, event, worker, and GTK-slice limits across palette indexing, workspace search, Replace Preview, Markdown rendering, and editor buffer replacement. The remaining problems are lifecycle edges rather than missing primary pipelines:

- `FileIndex` caps admitted files and flat-directory retention, but its `visited_directories` set can still grow with a directory-heavy tree that contains few files.
- Replace Preview construction is bounded and asynchronous, yet replacing a prior outcome, rejecting a stale outcome, and filtering checked rows at confirmation can release or traverse a near-limit payload synchronously on GTK.
- Markdown planning and projection are bounded, but stale plans, abandoned batch tails, replaced queued sources, and the detached GTK-render queue do not all have an end-to-end backlog bound.
- Workspace-search polling documents a 250-event GTK-turn cap but currently charges only match events.
- Sliced buffer replacement records that mutation began after calling signal-emitting GTK mutation APIs, leaving a narrow synchronous reentrancy path where cancellation can miss partial cleanup.

The implementation must preserve existing filesystem, safe-replacement, generation freshness, GTK ownership, accessibility, and automation contracts. Plain Rust payloads may move to workers; GTK objects must remain on the main thread and be retired in bounded GTK slices.

## Goals / Non-Goals

**Goals:**

- Bound distinct directory work and retained directory identity independently from admitted files.
- Ensure document-sized plain-Rust payload destruction and selection do not occur in a GTK action or completion callback.
- Put an explicit cap and latest-request backpressure policy around detached Markdown generations.
- Make documented per-turn and reentrant-mutation guarantees exact rather than approximate.
- Produce deterministic evidence for bounds, high-water marks, freshness, final content, and terminal cleanup.

**Non-Goals:**

- Replacing the existing palette, search-flight, search-retirement, Markdown session, or buffer-replacement abstractions.
- Creating a generic scheduler, disposal framework, new GTK Lush API, crate, dependency, or cross-feature trait.
- Changing Replace All safety semantics, persisted formats, search result limits, Markdown rendering semantics, or public automation data.
- Moving GTK-owned buffers, widgets, tags, or models to worker threads.

## Decisions

### 1. Add an independent 100,000-directory traversal budget

`services::palette::index` will define a named `MAX_INDEXED_DIRECTORIES` policy of 100,000 distinct canonical directory identities per build. A directory consumes the budget before its identity is retained and before descent. The budget is independent of `MAX_INDEXED_FILES`: a directory-only forest can terminate at the directory cap, while a dense shallow tree can still terminate at the file cap.

The existing typed `FileIndexTruncationReason::DirectoryRetentionLimit` and `FileIndexBuildMetrics` remain the ownership boundary. Metrics will report the scanned/retained directory high-water mark and the first deterministic truncation reason. The accepted prefix remains usable, cancellation is checked at the same or tighter traversal checkpoints, and canonical aliases do not consume multiple retained identities.

This is preferred over estimating directory memory from file count because empty and low-file directory trees break that correlation. It is also preferred over a byte estimator for `PathBuf` allocations because a count cap is deterministic across allocators and straightforward to test; the existing depth and flat-entry caps remain complementary byte/work controls.

### 2. Reuse search-panel retirement for GTK state and worker-drop plain preview payloads

Every path that replaces, invalidates, rejects, or exits a Replace Preview will detach the previous generation immediately. GTK-owned row/model state continues through the existing bounded search-retirement session. A near-limit plain `ReplacePreviewOutcome` that has no remaining UI owner will be handed to the existing bounded worker facility before its final reference is released, including stale worker completions.

Confirmation will capture only current generation identity plus the already-maintained checked-match identity set on GTK. A worker stage will partition the immutable outcome into selected replacements and rejected remainder. Only the selected current-generation payload returns to the normal Replace All callback; the remainder is destroyed on the worker. A freshness check immediately before callback publication prevents a confirmation invalidated during selection from applying.

This is preferred over adding smaller GTK chunks for pure-vector filtering because filtering still touches document-sized strings and makes confirmation latency proportional to payload size. It is preferred over a new generic disposer because the search panel already owns the applicable retirement, identity, and readiness semantics.

### 3. Separate Markdown plain-data disposal from GTK-object retirement and cap both ownership paths

Stale `MarkdownRenderPlan` values, unprojected `MarkdownEventBatch` tails, and superseded `PendingMarkdownPlan` sources are plain Rust data and will transfer to a worker-drop path before their last owner is released on GTK. GTK-owned `TextBuffer`, embed, and link state remains in `MarkdownRetirementSession` and continues to drain under the existing per-turn character and item budgets.

Ordinary rendering will retain at most two detached GTK render generations. When that cap is reached, the preview will not detach another ordinary generation or begin another projection. It will preserve at most one latest pending render request, replacing older pending plain data through the worker-drop path, and resume that request only after retirement falls below the cap. Close/dispose paths may synchronously detach the widget from current state but must not publish or resume obsolete work.

Readiness remains pending while a current plan, projection, image operation, detached retirement state, or latest render request exists. Test-only scalar evidence will expose active/pending ownership and detached-generation high-water marks without exposing document content.

This mirrors the proven search-retirement backpressure shape while keeping feature-specific state local. Sending GTK objects to a worker is rejected because GTK ownership is main-thread-only; retaining an unbounded detached queue is rejected because bounded per-slice work alone does not bound total backlog.

### 4. Charge every received workspace-search event to the GTK-turn budget

The consumer loop will increment its event counter immediately after every successful `try_recv`, before dispatch by variant. Match, progress, result-cap, error, and terminal events therefore consume the same 250-event budget. Channel disconnection is not an event and may terminate the turn without a charge.

This keeps the public behavior unchanged while making the stated scheduling bound true for adversarial progress/error mixtures. Separate counters for each variant are unnecessary because the protected resource is total dispatch work in one GTK callback.

### 5. Establish buffer mutation ownership before signal-emitting GTK calls

For non-empty delete or insert slices, the replacement session will mark `mutation_started` before calling GTK. The session borrow will be released before the GTK call so a synchronous `changed` handler can invalidate or supersede the session without a `RefCell` conflict. After the call, the continuation will re-acquire state, verify session identity, and either record progress or let the reentrant cancellation path perform exact partial cleanup and terminal publication.

This is preferred over suppressing every possible signal consumer because projection suppression is an application policy, not a guarantee that no test hook, future adapter, or toolkit callback can reenter. Marking ownership before the call makes cleanup correct by construction.

### 6. Extend the existing evidence lanes instead of adding a new suite

Pure service and policy tests will cover directory-only traversal, typed truncation, cancellation, preview partitioning, and scalar backlog policies. GTK widget tests will cover rapid Markdown supersession, mixed search-event bursts, preview confirmation/invalidations, and a first synchronous `changed` callback that supersedes sliced replacement. Existing Criterion and performance-smoke reporting will record direct bounds and high-water marks for representative near-limit fixtures.

The tests will use shortened test hooks and bounded fixtures for routine validation; larger diagnostic fixtures remain in the existing benchmark or performance lanes. No raw document text will enter metrics or logs.

## Risks / Trade-offs

- **Extreme directory forests produce a partial palette index** → Return the existing typed directory-retention truncation, retain a deterministic usable prefix, and surface the existing bounded-index diagnostic path.
- **Markdown backpressure can delay the newest preview while old GTK state drains** → Keep only the latest request and resume it as soon as the detached-generation count falls below two.
- **Worker-side preview selection adds another asynchronous freshness boundary** → Carry preview/search/panel generation identity through selection and revalidate immediately before invoking Replace All.
- **Worker-drop scheduling could itself accumulate** → Reuse bounded one-active/one-latest ownership or combine payloads with the feature's existing retirement lane; tests assert the high-water state rather than only completion time.
- **Reentrant replacement cancellation is subtle** → Add a widget test that supersedes from the first synchronous `changed` emission and asserts exact final content, editability, saveability, projection, and terminal cleanup.

## Migration Plan

No persisted-data or public-API migration is required. Implement the policy and scalar evidence first, then route each ownership edge through it, add layered tests, and run the normal strict validation and performance compilation lanes. Rollback is a normal code revert because no format or external contract changes.

## Open Questions

None. The caps, ownership boundaries, freshness checks, and evidence layers are defined by this design.
