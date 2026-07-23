## Context

The preceding quality changes established strong repository-wide patterns: GTK adapters own presentation and generations, services own blocking work, plain model state carries bounded scheduling policy, the filesystem boundary owns I/O, and document-sized payloads retain explicit admission until a terminal outcome. The follow-up review found no reason to replace those patterns. It found four places where an otherwise sound implementation stops one step short of its intended bound.

First, workspace watch targets and child caches are incremental, but every queued refresh still calls `save_expanded_paths()`, which walks the complete flattened `GtkTreeListModel`. Second, Notes correctly rejects stale bookmark excerpt completions, but each selection still submits another blocking job to a shared queue. Third, bounded file ingestion is cancellable, while encoding detection, decoding, line-ending detection, and file-health scans can continue for a very large admitted file after cancellation. Fourth, an Undo race hook is a single process-global `Option`, so an unrelated parallel target can consume it.

The change must preserve current visible behavior, data-safety semantics, exact encoding and file-health results, GTK-thread ownership, transient permits, and the established `ui -> services -> model` and filesystem boundaries. It must not introduce a generic scheduler or a second background runtime.

## Goals / Non-Goals

**Goals:**

- Make targeted workspace refresh bookkeeping proportional to the affected rows.
- Bound closed-file bookmark preview ownership to one active request and one latest compact request per Notes browser.
- Make large-file CPU work observe cancellation at deterministic work boundaries and release transient ownership exactly once.
- Make Replace/Undo fault injection deterministic and isolated when tests execute in parallel.
- Add direct retained-state and work-count evidence instead of relying on wall-clock assertions.

**Non-Goals:**

- Replacing `gtk-lush-tasks`, `gtk-lush-settle`, the filesystem boundary, or existing active-plus-latest implementations with a generic resource manager.
- Changing persisted formats, application actions, D-Bus automation, editor encoding choices, file-health findings, bookmark presentation, or workspace refresh semantics.
- Optimizing the bounded 500-row Notes projection or the existing Markdown source snapshot without profiling evidence.
- Adding host-sensitive RSS or latency thresholds to default CI. Near-limit resident-memory measurements remain opt-in diagnostics.
- Increasing supported file limits or changing editor-memory policy.

## Decisions

### 1. Treat expanded paths as live refresh state

The workspace section will keep its expanded-path set current from row expansion transitions and accepted row/store reconciliation. Targeted refresh will snapshot only scalar selection and reset per-run metrics; it will clone the already-current expanded set only where restoration needs an owned snapshot. A complete flattened-model derivation remains available as an explicit bootstrap/oracle and immediately before a true model replacement, but it will not run for an in-place targeted refresh.

Accepted splices will update or invalidate expansion entries for the affected subtree. Paths that still exist retain their expansion intent, paths removed by the accepted reconciliation are removed, and a superseded reconciliation cannot mutate the authoritative set. This is deliberately adjacent to the existing incremental row/cache and watcher-target bookkeeping rather than a new tree-state abstraction.

Alternative considered: keep the full scan because it is simple and correct. It was rejected because automatic refresh can run repeatedly and the work grows with all materialized rows rather than the changed directory. Deriving expansion only from the watch-target set was also rejected because collapsed descendants can retain restoration intent even when they are no longer active watch targets.

### 2. Give bookmark excerpts a browser-local active-plus-latest coordinator

`NotesBrowserState` will own a small coordinator containing at most one active closed-file excerpt request and one replaceable pending request. A compact request contains the preview generation, path, bookmarked line, presentation identity, and cancellation handle; it does not contain source text or a strong dialog owner.

Selecting a new closed-file bookmark cancels the active request and replaces the pending request. No second worker starts until the active worker reaches its terminal callback. That callback first retires active ownership, then publishes only if dialog lifetime, preview generation, and selected bookmark identity still match, and finally admits the latest pending request if still current. Selecting a live-editor bookmark or a non-bookmark row cancels active work and clears pending work. Dialog teardown does the same without allowing a later callback to reopen or mutate the browser.

The bookmark-excerpt service will expose a cancellable bounded loader and check cancellation before and after metadata/read boundaries and during its bounded line scan. Existing synchronous callers may use a thin non-cancelling wrapper. Worker completion will continue to arrive through the existing GTK Lush task bridge.

Alternative considered: only add more stale-generation checks. It was rejected because freshness already works; it does not bound obsolete jobs retained by the shared worker queue. Changing the shared task queue was rejected because this ownership rule belongs to one workflow and the repository already uses local active-plus-latest coordinators successfully.

### 3. Make byte classification, decoding, and health analysis cooperatively cancellable

The admitted editor-load worker will use one cancellation-aware analysis pipeline. Raw-byte classification and encoding detection will process bounded chunks where they currently scan a whole input. Decoding will preserve codec state across bounded input/output chunks, including UTF-8 scalar boundaries and stateful `encoding_rs` decoders. While decoded chunks are produced, the pipeline will accumulate line-ending counts and file-health evidence so the current later full-text passes are fused into the same bounded traversal where practical.

Cancellation checks will occur between chunks and between non-chunkable codec/library calls. Once cancellation is observed, the worker returns the existing typed cancelled terminal and performs no further optional health analysis. Successful loads retain identical decoded text, BOM/confidence metadata, line-ending classification, and file-health findings. The existing load owner remains responsible for releasing its transient permit on every terminal; this change does not create a second permit or transfer ownership into analysis helpers.

Chunk sizes will be explicit policy constants with test-only work counters. The contract is bounded cooperative progress, not an absolute cancellation time. Small inputs may keep a direct path when the same pre/post cancellation and result-equivalence rules hold.

Alternative considered: skip file-health analysis above a size threshold. It would reduce work but make health results size-dependent, so exact chunked/fused analysis is preferred. A thresholded degraded health result remains a fallback only if implementation evidence shows a codec cannot be made incrementally equivalent; adopting it would require an explicit spec revision rather than an implicit shortcut.

### 4. Scope one-shot Undo seams by target path

Test-only Undo hooks will be stored in a mutex-protected path-keyed registry. Registration supplies the exact target path and returns cleanup ownership; an Undo operation removes only the hook matching its own target, drops the registry lock, and then invokes the `FnOnce`. Distinct targets can therefore run concurrently without stealing hooks or serializing callback execution. Cleanup removes an unconsumed registration so a failed assertion cannot poison later tests.

The key uses the same owned path identity already carried by the Undo entry instead of doing new canonicalization I/O inside the test seam. Tests must use the exact target path with which the operation is invoked.

Alternative considered: thread-local storage. It was rejected because test-utils and future worker-backed tests may register and execute on different threads. A global test mutex around all affected tests was rejected because it hides the isolation defect and weakens parallel coverage.

### 5. Prove bounds with deterministic counters and adversarial ordering

Focused plain/service tests will assert chunk counts, cancellation terminals, active/pending high-water values, exact permit release, and target-specific hook consumption. Workspace widget/performance fixtures will compare incremental expansion state with a test-only full oracle and prove a targeted refresh does not enumerate the full flattened model. Bookmark widget tests will supersede delayed previews rapidly and assert active `<= 1`, pending `<= 1`, latest-only publication, and teardown cancellation.

Large-file fixtures will cover ASCII, multibyte UTF-8, BOM and BOM-less UTF-16, fallback encodings, cancellation at multiple stages, and result equivalence when not cancelled. An optional near-supported-limit diagnostic may record RSS, cancellation progress, and environment metadata, but routine gates will use deterministic representative fixtures and no absolute cross-machine timing threshold.

The Undo tests will register hooks for different temporary paths, execute the operations concurrently or in deliberately interleaved order, and prove each hook is consumed only by its target. This directly covers the failure that repeated parallel runs exposed.

## Risks / Trade-offs

- **[Expansion cache drifts from GTK state]** -> Keep a test-only full derivation oracle, exercise generated expand/collapse/splice/reload sequences, and reserve full derivation for bootstrap/model replacement.
- **[Cancellation coordinator stalls its latest request]** -> Every worker outcome, including cancellation and error, must pass through one terminal transition that clears active ownership and considers pending work.
- **[Incremental decoding changes text or metadata]** -> Compare the new path against the current/reference decoder across encoding fixtures and generated chunk boundaries before removing redundant passes.
- **[Very large non-chunkable library work remains coarse]** -> Place checks immediately around that call, measure the remaining work, and prefer an incremental existing-codec API; do not claim a hard latency guarantee.
- **[Path-scoped hooks leak after a failed test]** -> Return a cleanup guard and add registry-empty assertions at test boundaries.
- **[Instrumentation distorts production paths]** -> Compile detailed counters only for tests/test-utils and retain only policy constants and cancellation checks in production.

## Migration Plan

No persisted-data or public-API migration is required. Implement each boundary behind its current internal call site, run focused equivalence and ownership tests, then run the complete repository gates. Each part is independently reversible: the live expansion cache can fall back to explicit full capture, the preview coordinator can be removed while retaining generation checks, the decoder can return to the reference path, and test-only hook storage can be reverted without changing production data.

## Open Questions

None currently. Chunk sizes and the exact placement of test counters are implementation-calibration details, not product decisions; they must be explicit and justified by deterministic fixtures during apply.
