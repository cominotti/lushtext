## 1. Capture Baselines and Deterministic Seams

- [x] 1.1 Run the current focused unit, integration, and widget tests around search retirement, buffer snapshots, Local History restore, Replace All/Undo, file indexing, bookmark browsing, recent documents, minimap, and Markdown images; record any pre-existing failure before implementation.
- [x] 1.2 Save a same-checkout Criterion baseline with make bench-baseline and record toolchain, profile, machine-load conditions, and storage class for the later file-index comparison.
- [x] 1.3 Add the narrow test-only failure and race seams needed to force a durable pre-rename write failure, target growth between metadata and ingestion, recent-metadata growth, stale generation completion, and guarded disposal completion without adding a production filesystem trait.

## 2. Make Search Retirement Release-Safe

- [x] 2.1 Refactor the plain retirement budget and RetiredSearchGtkState so every root row, group, child row, cached file or match row, accepted snapshot reference, match, and position is charged unconditionally before release, with debug assertions observing captured results only.
- [x] 2.2 Expose compact test metrics for actual before-and-after retired ownership, per-category progress, pending state, and terminal drain while preserving current-generation isolation and the existing detached/terminal/latest limits.
- [x] 2.3 Add category-isolated and mixed headless GTK regressions with more than 250 items, require actual per-turn releases in 1..=250, prove multiple turns and eventual drain, and prove a newly mounted generation remains unchanged.
- [x] 2.4 Add a focused test-profile or release-profile invocation with debug assertions disabled and verify that the retirement regression exercises the same actual ownership bounds outside debug builds.

## 3. Make Whole-Buffer Snapshots Truly Bounded

- [x] 3.1 Replace the cumulative snapshot String with independently allocated UTF-8 chunks, reserve chunk-header capacity from the O(1) character count, keep 64 Ki-character/256 KiB maximum slices, and preserve mark, mutation, overflow, cancellation, source, signal, and callback-once invariants.
- [x] 3.2 Add an admitted worker-handoff API that moves completed chunks in O(1), performs any contiguous coalescing or transformation off GTK, and guarantees bounded worker disposal for partial, stale, rejected, and teardown ownership.
- [x] 3.3 Migrate the admitted save path so SavePayloadPermit spans capture, coalescing, formatting, durable write, terminal acceptance, and exact-once release even when the editor disappears or the snapshot is rejected.
- [x] 3.4 Migrate document-note, folder-note, note-editor, and draft persistence callers so their established size limits and admission are applied before large capture and no large result is coalesced or finally destroyed on GTK.
- [x] 3.5 Migrate encoding analysis, Markdown preview, automatic Local History capture, and every remaining chunked-snapshot caller to a workflow-local admitted worker continuation; remove or seal any API that can return an unguarded large captured String to GTK.
- [x] 3.6 Add ASCII and multibyte snapshot regressions above 10 MiB that prove byte-exact output, more than one GTK slice, bounded per-slice text, zero GTK whole-body coalesces, cleanup on mutation/cancellation/teardown, and an independent main-loop sentinel before completion.
- [x] 3.7 Add a large-save widget regression that proves exact durable bytes, bounded admission, worker-side coalescing and rejected disposal, main-loop progress, and exact-once save-permit release across success, staleness, and editor teardown.

## 4. Guard Local History Restore and Undo Ownership

- [x] 4.1 Reserve conservative current-buffer ownership within the existing 64 MiB Local History/progress policy before restore capture; retain only compact latest intent during capacity pressure and start no capture, safety write, or mutation until admitted.
- [x] 4.2 Move the captured body once into the RestoreSafety worker, persist safety before mutation, and return the same body as DisposalOwned<String> without a full-text clone or an unguarded GTK handoff.
- [x] 4.3 Change editor-page Undo Restore storage and accessors to preserve the disposal guard through bounded replacement, successful restore, undo consumption, fresh cancellation return, supersession, save/clean transition, eviction, and teardown.
- [x] 4.4 Add genuinely larger-than-10-MiB restore/undo widget coverage proving safety-before-mutation, one guarded owner, no full-body clone, byte-exact restore and undo, stale isolation, main-loop progress, and worker-side final disposal.
- [x] 4.5 Add capacity-contention coverage proving compact deferral, no pre-admission snapshot or mutation, at-most-one current wakeup, and correct retry or visible failure when ownership cannot be admitted.

## 5. Correct Replace All and Undo Safety Boundaries

- [x] 5.1 Add a plain undo-payload ledger that separates reversible live retained bytes from monotonic high-water metrics and prove charge, reclaim, overflow, saturation, and exact-limit sequences with unit and property tests.
- [x] 5.2 Reclaim the exact live charge and active-journal entry on every pre-rename failure that removes an undo entry, while retaining entry, journal, and charge after ambiguous post-rename durability failure.
- [x] 5.3 Replace Undo's metadata-then-unbounded read with filesystem::read::bounded_bytes(MAX_REPLACE_FILE_BYTES), preserving exact-limit inclusion and retaining oversized, raced, diverged, or failed targets untouched in remaining_backup.
- [x] 5.4 Replace full-cardinality skipped, error, rollback, and path graphs with exact totals plus a deterministic BoundedDiagnosticSample capped at 32 entries and 32 KiB, including a bounded all-failed summary with no document text.
- [x] 5.5 Snapshot current open-tab canonical identities before worker submission, return only the affected/restored intersection needed for tab reconciliation, and update window status projection to consume exact totals and bounded samples.
- [x] 5.6 Add deterministic integration tests where an early sorted target fails before rename under a cap that allows only one entry, prove a later file succeeds, and assert target contents, live charge, high water, memory backup, and incremental journal.
- [x] 5.7 Add exact-limit, cap-plus-one, concurrent-growth, ordinary-I/O-failure, and 10,000-target failure-heavy tests proving bounded allocation, no unsafe write, retryable backup state, exact totals, deterministic sample order, and sample count/byte ceilings.

## 6. Enforce File-Index Construction Bytes

- [x] 6.1 Add the O(1) FileIndexBuildLedger with a 128 MiB build ceiling and 64 MiB installed-result ceiling, conservatively charging vector capacity, allocation slack, raw/display/canonical paths, hash buckets, visited identities, pending directories, workspace roots, and current scan entries.
- [x] 6.2 Extend bounded traversal so each scan page honors remaining scratch bytes or yields a chargeable bounded batch, releases temporary charges as ownership drains, and never excludes scan.entries from peak metrics.
- [x] 6.3 Integrate charge-before-retain behavior, typed build-byte truncation, deterministic usable partial output, O(1) current/peak metrics, and the existing final retained-output check without weakening canonical deduplication, ordering, count/depth limits, cancellation, or freshness.
- [x] 6.4 Add unit, property, and temporary-tree tests for long and Unicode paths, sparse directory-only trees, duplicate canonical identities, vector/hash overhead, exact boundary, one-over boundary, cancellation, deterministic truncation, peak build bytes at or below 128 MiB, and installed bytes at or below 64 MiB.
- [x] 6.5 Extend the existing file-index Criterion group with realistic common, miss, directory-heavy, and near-policy long-path cases while keeping assertions and fixture construction outside timed work where appropriate.

## 7. Consolidate Bounded UI Pipelines and Shared Context

- [x] 7.1 Store workspace-search folders as Arc<[PathBuf]>, reuse one immutable generation snapshot for active/latest requests, worker traversal, display paths, polling, and freshness, and add pointer-sharing plus scope-change isolation tests.
- [x] 7.2 Make MarkdownPreviewRenderContext Arc-backed so document/workspace paths are shared by plans, up to 1,000 table-cell builders, and image work; prove pointer sharing and one retained path graph under maximum-cell input.
- [x] 7.3 Apply Markdown image count, byte, generation, and lifetime admission before path expansion, resolve document-relative then workspace-relative candidates one at a time off GTK, and preserve safety, fallback, accessibility, decode, and stale-disposal behavior.
- [x] 7.4 Route win.show-bookmarks into the existing single Browse Notes dialog with bookmark-only mode included in source/query generation identity and preserve catalog, accelerator, title, placeholder, empty, truncation, preview, keyboard, accessibility, edit, and path/line activation behavior.
- [x] 7.5 Remove the standalone bookmark Rc<Vec<_>>, synchronous filter/GtkBox rebuild, duplicate dialog helpers, and unrestricted production aggregate collector; require the unified Notes source count/byte/generation/cancellation policy for interactive inventory construction.
- [x] 7.6 Add headless widget/proof coverage for repeated all-notes/bookmark-mode switching, one live dialog/source owner, stale query and preview rejection, rapid latest-query wins, source count/byte limits, at most 500 visible rows, corrupt-sidecar warnings, empty states, exact activation, disposal, and main-loop progress.
- [x] 7.7 Add Markdown coverage for image floods, many workspace folders, early candidate success, missing/unsafe/oversized candidates, stale generation, bounded retained candidate paths, and existing four-image plus detached-generation limits.

## 8. Correct Remaining Ingestion and Byte Classifiers

- [x] 8.1 Load recent-document metadata with filesystem::read::bounded_bytes(MAX_RECENT_DOCUMENTS_BYTES), keep metadata as an early hint only, and preserve missing, malformed, reset/prune, and bounded diagnostic behavior.
- [x] 8.2 Add exact-limit, cap-plus-one, and deterministic after-metadata-growth recent-document tests proving the enlarged body is neither fully allocated nor parsed and the Open popover remains usable after recovery.
- [x] 8.3 Gate minimap wrapped-layout analysis with estimated_live_buffer_bytes(), preserving exact-2-MiB eligibility, one-byte-over analysis, wrap-disabled behavior, eviction, cancellation, and generation freshness without scanning or copying text.
- [x] 8.4 Add pure scalar and focused minimap tests for untitled and modified multibyte buffers, known-file floors, exact and one-over thresholds, saturating arithmetic, wrapping disabled, and stale bounded-analysis completion.

## 9. Close Lint, Comments, and Repository Policy

- [x] 9.1 Rewrite every current side-effectful debug assertion so mutations execute unconditionally, then set clippy::debug_assert_with_mut_call = "deny" in workspace lints without an allow or expect suppression.
- [x] 9.2 Add the zero-count blocking-candidate advisory-policy rule explaining release elision and add high-signal comments only where snapshot admission, guarded ownership, reversible accounting, or generation invariants are not evident from names and types.
- [x] 9.3 Run make lint-advisory, clean every actionable current finding, and narrowly classify each intentional remaining occurrence by lint, rationale, path scope, and maximum count without blanket group suppression.
- [x] 9.4 Synchronize the applicable build, Rust, UI, widget-wiring, testing, and contributor guidance only where the implementation establishes a durable new repository rule, then run make check-agent-docs and any affected focused policy checks.

## 10. Verify the Readiness Closeout

- [x] 10.1 Run all new targeted unit, integration, property, headless widget/proof, failure-injection, and disabled-debug-assertions regressions and confirm they assert actual ownership or state rather than only self-reported counters.
- [x] 10.2 Run make test-unit, make test-int, and make test-prop with no ignored new failure and no leaked test fixture or stale recovery artifact.
- [x] 10.3 Run make test-widget and make performance-smoke, inspect logs for GTK criticals/warnings, timeouts, stalled sentinels, disposal backpressure, or readiness leakage, and fix every in-scope failure.
- [x] 10.4 Run make bench-compare in the same recorded environment, review distributions and effect sizes for file-index common and near-policy cases, and resolve any material unexplained regression without adding an absolute cross-machine timing gate.
- [x] 10.5 Run make check and make lint-advisory; confirm the blocking Clippy lane, filesystem boundary, architecture policy, docs checks, and all classified advisory counts are clean.
- [x] 10.6 Run openspec validate close-code-quality-review-gaps --strict, openspec validate --all --strict --no-interactive, and git diff --check; review the final diff against every proposal bullet and delta requirement before marking the change complete.
