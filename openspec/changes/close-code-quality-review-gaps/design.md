## Context

The preceding boundedness, durability, recovery, GTK ownership, and workflow-state changes landed the intended architecture: GTK adapters drive UI state, services own I/O and workflow mechanics, plain Rust models own admission and generation policy, filesystem access stays behind services::filesystem, and document-sized rejected values already have guarded disposal lanes. The final live-code review found no reason for a new crate, framework, global memory manager, broad module split, or GTK Lush API. It did find two correctness defects and several localized places where implementation no longer fully matches those established contracts:

- workspace-search retirement charges three destructive operations only inside debug assertions, so release builds can discard an entire large category in one GTK turn;
- Replace All removes a failed pre-rename undo entry without reclaiming its live byte charge;
- chunked GtkTextBuffer capture yields between reads but appends into one cumulative String on GTK, and Local History clones and retains its full Undo Restore body as an ordinary String;
- undo and recent-document reads use metadata as the effective allocation boundary;
- file-index retained-byte enforcement happens after substantial path and traversal graphs already exist;
- Replace All completion metadata, the standalone bookmark browser, repeated workspace-scope clones, and Markdown image candidate expansion retain avoidable full-cardinality graphs;
- minimap admission compares characters with a byte policy; and
- the release-safety Clippy warning plus the current advisory inventory are not yet closed by repository policy.

The change is intentionally one final umbrella because all findings share the same readiness claim: bounds must apply to actual ownership and failure behavior, not only final output or debug-only counters. There is no persisted-format migration and no public automation-contract change.

### Affected ownership map

| Concern | Plain policy owner | Service or worker owner | GTK adapter owner |
|---|---|---|---|
| Search retirement | search retirement budget | none | search panel runtime |
| Buffer snapshots | scalar chunk and admission policy | caller worker continuation | buffer snapshot helper and caller |
| Local History undo | disposal reservation and replacement ticket | safety snapshot persistence | Local History browser and editor page |
| Replace All | undo ledger and bounded summaries | replace, undo, durable journal | window search projection |
| File index | private O(1) build ledger | palette index traversal | command palette generation |
| Bookmark browser | Notes source and query policy | bounded Notes inventory | unified Notes browser |
| Scope and image paths | immutable shared snapshots | search and image workers | search and Markdown generations |
| Recent metadata | bounded filesystem ingestion | recent-document service | Open popover |
| Minimap | editor-memory byte estimate | bounded line analysis | editor-page minimap |
| Lint and evidence | repository policy | test and benchmark targets | headless GTK proofs |

## Goals / Non-Goals

### Goals

- Make every confirmed defect and recommendation from the final code-quality review mechanically closed by a requirement, implementation task, and direct regression.
- Preserve the architecture and user-facing workflows that the previous portfolio established.
- Make release and debug retirement semantics identical and protect that property with a blocking lint.
- Apply byte limits while ownership is acquired or data is ingested, including failure and concurrent-growth paths.
- Keep cumulative document-sized text construction, cloning, and final destruction out of GTK dispatch.
- Reuse the existing bounded Notes browser instead of maintaining a second bookmark pipeline.
- Keep complete counts while bounding diagnostic, path, and candidate evidence.
- Finish with the full repository validation stack, a targeted disabled-debug-assertions regression, and same-environment performance evidence where a hot path changes materially.

### Non-Goals

- A global RSS estimator, generic resource manager, new scheduler, or shared manager trait.
- A new crate, dependency, feature flag, persisted schema, data migration, action, accelerator, D-Bus member, readiness predicate, or GTK Lush public API.
- A broad rewrite or module split unrelated to the confirmed findings.
- Changing search limits, image count, Notes row limits, Local History recovery guarantees, Replace All journal ordering, open-file limits, or normal user-visible semantics.
- Blanket enabling Clippy restriction, pedantic, nursery, or cargo groups.
- Absolute benchmark thresholds that are copied between machines.

## Decisions

### 1. Keep policy plain and workflow ownership explicit

The implementation will extend the narrow owners already present instead of introducing a common manager:

- search-retirement accounting remains in the plain search-retirement policy and its search-panel adapter;
- Replace All live byte accounting and bounded completion shapes remain in model::content_search plus services::content_search;
- file-index build accounting is a private plain ledger owned by palette indexing because no other workflow shares that policy;
- immutable folder and render snapshots use Arc-backed values in their existing request/context types;
- GTK-only capture, row projection, widget retirement, and buffer replacement remain in ui.

This follows the existing ui -> services -> model direction while avoiding abstractions whose only commonality is that they count bytes.

Alternative considered: introduce a repository-wide memory-budget service. Rejected because the workflows have different priorities, recovery invariants, and lifetime identities, and the current explicit admission models already provide the needed seams.

### 2. Charge search retirement before every destructive step

RetiredSearchGtkState::retire_slice will use one unconditional charge-and-remove pattern for root rows, groups, child rows, file and match caches, accepted-result references, match vectors, and navigation positions. A debug assertion may inspect the captured charge or removal result, but it cannot perform either operation.

The slice result will expose compact actual before/after ownership totals for tests and diagnostics. Scheduling continues only when state remains, and a positive budget with nonempty state must make progress. Existing limits of two ordinary detached generations, one terminal escape generation, and one latest deferred request stay unchanged.

The focused regression will execute with test-profile debug assertions disabled, measure actual ownership deltas, require more than one 250-item turn, and prove that a newly mounted generation survives. clippy::debug_assert_with_mut_call becomes a denied workspace lint and a zero-count blocking candidate.

Alternative considered: fix only the three current expressions. Rejected because category-complete charge helpers and actual-delta tests are the durable recurrence boundary.

### 3. Represent large buffer captures as independent chunks and hand them directly to workers

The GTK snapshot helper will stop accumulating one growing String. Large capture will:

1. derive a conservative scalar weight and chunk-header capacity from the O(1) buffer character count with checked or saturating arithmetic;
2. require the caller's existing workflow admission or a compatible pre-reserved disposal handoff before the first slice;
3. copy at most 64 Ki characters, and therefore at most 256 KiB of UTF-8 text, into one independent chunk per GTK turn;
4. retain chunks without cumulative append, preserve the buffer mark and mutation handler, and keep callback-at-most-once cleanup;
5. move the chunk owner in O(1) into the already admitted worker continuation for coalescing, transformation, persistence, or analysis; and
6. route cancellation, staleness, overflow, teardown, and rejected completed chunks through bounded worker disposal.

No large-path callback will first coalesce a document body on GTK. Save keeps its SavePayloadPermit across snapshot capture, worker coalescing, formatting, durable write, terminal freshness, and exact-once release. Bounded note, draft, preview, encoding, and Local History callers must either reuse their current count/byte admission or add a workflow-local compact wait before capture; none may accept an unguarded document-sized result on GTK. The direct small path remains for buffers below the established threshold.

Alternative considered: reserve the final String capacity and keep append on GTK. Rejected because a later capacity growth or final coalescing can still copy the accumulated prefix in one turn.

Alternative considered: add a global snapshot daemon. Rejected because caller-owned admitted worker continuations preserve clearer lifetime and cancellation ownership.

### 4. Store Local History Undo Restore as DisposalOwned text

Restore will reserve conservative current-buffer ownership against the existing 64 MiB Local History/progress policy before snapshot capture or safety persistence begins. Capacity pressure retains only compact current intent and uses the existing wakeup pattern; it does not retain an unadmitted body.

The capture worker consumes the chunk owner, coalesces once off GTK, persists the RestoreSafety snapshot before mutation, and returns the same text as DisposalOwned<String>. The browser moves that guard into bounded replacement. On successful restore, the editor retains the guard as the one-shot Undo Restore body. Undo consumes the same guard; a fresh cancellation returns it without cloning; supersession, save/clean transition, eviction, dialog/page teardown, and stale completion let DisposalOwned perform guaranteed worker destruction.

The editor-page field and accessors will use a guarded type, so an ordinary RefCell replacement can no longer synchronously destroy the final large String. Existing replacement tickets, modified state, cursor reset, safety-before-mutation rule, retry behavior, and notification wording remain intact.

Alternative considered: keep an Rc<String> to avoid cloning. Rejected because the final strong-reference drop could still deallocate document-sized storage on GTK and would not carry byte admission.

### 5. Give Replace All reversible live accounting and compact terminal data

A small plain undo-payload ledger will distinguish:

- live retained bytes, which are charged before an undo entry is retained and reclaimed whenever that entry is removed before rename;
- monotonic high-water bytes, which observe the maximum live charge and are never decremented; and
- the persisted incremental journal, whose entry set must agree with recoverable mutations.

Pre-rename write failure removes the matching memory and journal entry and reclaims its exact charge before evaluating the next file. After-rename ambiguous durability retains the entry and charge. Journal-before-mutation and cancellation rollback ordering do not change.

Undo replaces the metadata-then-unbounded read with fs_read::bounded_bytes(MAX_REPLACE_FILE_BYTES). An exact-limit file proceeds; a concurrent-growth or already-oversized file is untouched, classified as skipped, and retained in remaining_backup; other I/O failures remain failed and retryable.

Replace and Undo results will carry exact aggregate totals plus one deterministic BoundedDiagnosticSample capped at 32 entries and 32 KiB of path/message ownership. The all-failed and rollback paths summarize through that type rather than joining every error. Before worker submission, the window supplies an immutable canonical identity set for currently open tabs. The worker returns only affected/restored paths that intersect that set; all other path cardinality is represented by counts and the bounded sample. Samples never include document text.

Alternative considered: raise the undo or result cap. Rejected because the defects are incorrect ownership accounting and unbounded metadata shape, not insufficient policy budgets.

### 6. Enforce file-index bytes throughout construction

Palette indexing will add an O(1) FileIndexBuildLedger with two explicit ceilings:

- MAX_FILE_INDEX_RETAINED_BYTES remains 64 MiB for installed output;
- MAX_FILE_INDEX_BUILD_RETAINED_BYTES is 128 MiB, allowing at most one installed-index budget of traversal and deduplication scratch while construction overlaps output.

Conservative charges cover vector capacity, raw/display/canonical paths and allocation slack, hash-table buckets, visited identities, pending directories, workspace roots, and the current scan batch. Temporary ownership releases its charge when popped or drained. The scanner will accept remaining byte capacity or return bounded pages so scan.entries cannot appear outside the ledger. Every insertion charges before ownership; the first non-fitting item produces a typed build-byte truncation reason and deterministic usable partial index. The final installed-output check remains defense in depth.

Metrics report current and peak build bytes plus final retained weight in O(1); they do not rescan all retained paths for each insertion. Existing canonical deduplication, stable ordering, depth/file/directory limits, cancellation, and generation freshness remain unchanged.

Alternative considered: force the entire build under the 64 MiB installed cap. Rejected because transient deduplication and traversal state are necessary and a separate fixed scratch allowance is clearer and still bounded.

### 7. Reuse the unified Notes browser for bookmark-only mode

win.show-bookmarks and its accelerator/catalog identity remain unchanged, but activation will configure the existing single live Browse Notes dialog with a bookmark-only filter. Mode becomes part of source and query generation identity, so an all-notes completion cannot publish after switching modes.

Bookmark-only mode reuses the established bounded source builder, source count/byte admission, one-active/one-latest query ownership, 500-row projection cap, guarded source disposal, preview freshness, diagnostics, and batched/virtualized row projection. It provides bookmark-specific title, placeholder, empty state, and truncation copy, then uses the existing path/line activation contract.

The standalone aggregate Rc<Vec<WorkspaceBookmark>>, synchronous full-source filter, GtkBox teardown/rebuild, and duplicate dialog helpers are removed. Any unrestricted production bookmark collector is removed, made test-only, or changed to require the Notes source limit, byte budget, generation, and cancellation policy.

Alternative considered: add the same bounds independently to the old dialog. Rejected because it would preserve duplicate state, query, projection, accessibility, and disposal machinery.

### 8. Share path graphs and admit Markdown images before resolution

SearchRequest folders become Arc<[PathBuf]>. One request snapshot is used by traversal, display-path calculation, polling, active-plus-latest replacement, and freshness comparison. A workspace-scope change creates a new request generation rather than changing the old snapshot.

MarkdownPreviewRenderContext becomes Arc-backed so document and workspace paths are O(1) to clone into plans, up to 1,000 table-cell builders, and image work. Each render generation keeps one immutable context.

Image generation/count/byte admission runs before path expansion. Only an admitted relative image reaches a worker, which checks the document-relative candidate and then workspace folders one at a time in deterministic order. It stops at the first valid candidate and never materializes a complete candidate vector. Existing canonical safety, file-size, decode, fallback, accessibility, generation, and retirement policies remain unchanged.

Alternative considered: cache every expanded candidate for the render. Rejected because image admission is four items and lazy one-at-a-time checks are simpler and strictly smaller.

### 9. Use bounded ingestion and existing scalar byte policy at the remaining callers

Recent-document loading uses fs_read::bounded_bytes(MAX_RECENT_DOCUMENTS_BYTES). Metadata remains an early rejection hint only. Exact-limit input reaches JSON parsing; concurrent growth or cap-plus-one input never reaches the parser and follows existing reset/prune recovery with a bounded diagnostic.

Minimap wrapped-layout gating calls estimated_live_buffer_bytes(), which already computes max(known file bytes, char_count * 4) with saturating arithmetic. Exactly 2 MiB remains eligible; one estimated byte over enters bounded long-line analysis. No buffer scan or duplicate estimate is introduced.

Both decisions stay behind current service/model boundaries and preserve missing, malformed, wrap-disabled, eviction, and stale-generation behavior.

### 10. Close policy and evidence in the same change

Cargo.toml will deny clippy::debug_assert_with_mut_call. scripts/lint-advisory-policy.toml will give it a zero-count blocking-candidate entry explaining release elision. All current occurrences are rewritten without suppression. The remaining current advisory output is either cleaned or narrowly classified by lint, rationale, path scope, and maximum count; broad groups remain advisory.

Testing follows the repository's authoritative surfaces:

- unit tests for scalar estimates, ledgers, summaries, shared ownership, exact boundaries, and cancellation policy;
- integration/temp-directory tests for durable failure ordering, journal contents, bounded read growth, recent metadata recovery, and indexing traversal;
- property tests for ledger charge/release sequences, diagnostic sample count/byte caps, and exact-limit/one-over invariants;
- headless widget/proof tests for release-semantic retirement, chunked GTK progress, guarded Local History, unified bookmark mode, stale generation rejection, and image/minimap projection behavior;
- performance smoke and, when the GTK-free index hot path materially changes, same-environment Criterion comparison without an absolute copied threshold.

The release-semantic retirement regression runs with debug assertions disabled and asserts actual container deltas. Large-body evidence exceeds 10 MiB, includes ASCII and multibyte text, schedules an independent main-loop sentinel, and checks exact bytes plus worker-side clone/coalesce/disposal instrumentation.

## Risks / Trade-offs

- Independent snapshot chunks add allocator and vector-header overhead. Header capacity is reserved from scalar length, chunks remain fixed-size, and performance evidence compares the supported large path.
- Conservative file-index accounting may truncate earlier for exceptionally long paths. The partial index remains deterministic and usable, and the typed reason makes the policy visible instead of risking an unmeasured spike.
- A 32-entry/32-KiB diagnostic sample provides less per-file detail than complete failure vectors. Exact totals remain available, ordering is deterministic, and the UI currently consumes summary severity rather than every entry.
- Reusing the Notes browser may expose accidental assumptions that its mode is always All Notes. Mode participates in generation identity and headless tests cover repeated switching, stale queries, previews, empty states, keyboard use, and accessibility.
- Arc-backed scope values can preserve an old snapshot longer than deep clones did. Active-plus-latest and generation retirement still bound owner count, and tests assert pointer sharing and stale isolation.
- A disabled-debug-assertions GTK test adds targeted CI time. It is limited to the regression binary/scenario, while the lint provides the cheap workspace-wide recurrence gate.

## Migration Plan

There is no data migration, settings migration, dependency update, or public API transition. Implementation should proceed in dependency order within this one change:

1. add plain ledgers, bounded summary types, shared request/context ownership, scalar policies, and unit/property evidence;
2. correct Replace All and bounded filesystem ingestion with failure-path integration tests;
3. correct release retirement and snapshot/Local History ownership with targeted disabled-debug-assertions and widget evidence;
4. migrate the bookmark action and Markdown/image/search adapters to the shared bounded paths;
5. promote lint policy, clean/classify advisory drift, and run the complete closeout matrix.

Each step keeps existing persisted formats readable. Rollback is code-only: no produced file requires conversion, and the old release can read all user data written by the new implementation.

## Open Questions

None. The change intentionally fixes the policies at their current documented limits and reuses established workflow owners; implementation should not reopen those product limits unless direct evidence reveals a contradiction, in which case the design and affected delta spec must be updated before code proceeds.
