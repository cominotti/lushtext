## 1. Baseline and policy seams

- [x] 1.1 Record focused baseline counters and Criterion measurements for multi-tab save payload retention, dense Markdown preview, rapid workspace-query overlap, 10,000-result preview/teardown, many-tab editing, and lossy-encoding analysis without overwriting prior benchmark evidence.
- [x] 1.2 Add plain-Rust value objects and policy tests for conservative save charges, weighted admission/exclusive overweight behavior, close-save priority, stale compact requests, and saturating arithmetic.
- [x] 1.3 Add plain-Rust state-machine tests for Markdown planning/projection budgets, image admission, generation terminal states, search active-plus-latest ownership, sliced result retirement, and incremental editor residency reconciliation.
- [x] 1.4 Confirm the new policies remain GTK-free under `model`/`services`, use the existing filesystem and GTK Lush boundaries, and introduce no dependency, crate, generic scheduler, or public GTK Lush API without documented second-consumer evidence.

## 2. Byte-weighted save admission and close ordering

- [x] 2.1 Implement a process-owned save-admission adapter that queues only weak/scalar editor, generation, destination, priority, and conservative byte-charge state before any complete buffer snapshot.
- [x] 2.2 Acquire and retain a typed payload permit across direct/chunked snapshot, line-ending and normalization transforms, encoding, durable-write consumption, cancellation, and stale discard; enforce exclusive admission for one supported overweight save.
- [x] 2.3 Revalidate editor lifetime, modification state, save generation, destination/path identity, and close-session identity immediately before admission and before applying terminal completion.
- [x] 2.4 Refactor the multi-tab close flow to admit, snapshot, save, release, and validate one selected editor at a time while retaining the existing explicit Save As/discard/cancel decisions.
- [x] 2.5 Preserve pre-rename failure, post-rename durability warning, modified state, Save As adoption, draft retention/cleanup, tab destruction, and final window-close ordering for every sequential-save terminal path.
- [x] 2.6 Remove avoidable save-path body copies or stream transformations where the existing durable-write and exact encoding contracts permit, and update the conservative charge for every payload that must remain.
- [x] 2.7 Add policy, editor/window integration, and GTK widget tests for ordinary concurrency, overweight exclusivity, close-save priority, stale queued requests, mid-batch failure, durability warning, untitled Save As, cancellation, and draft recovery.

## 3. Bounded Markdown render sessions

- [x] 3.1 Extract a GTK-free Markdown render-plan representation and parser that enforces documented source-byte, event/node, structural, embed-descriptor, and output-retention budgets.
- [x] 3.2 Feed planning from the existing live-size direct/chunked snapshot path, run non-trivial parsing off GTK, and reject plans whose editor/path/lifetime/buffer/render generation is no longer current.
- [x] 3.3 Replace monolithic GTK rendering with a generation-owned projection session that applies deterministic time/node slices and yields between slices without exposing stale partial terminal state.
- [x] 3.4 Define and render accessible pending, limited, failed, cancelled, and complete states, and make preview readiness account for planning, projection slices, image work, and disposal.
- [x] 3.5 Add bounded lazy local-image admission by current render generation, including count/byte/size limits, accessible placeholders for excess or unsupported embeds, and exact payload release on completion or cancellation.
- [x] 3.6 Guard every projection slice, tag/widget insertion, image completion, and terminal transition against newer render generation, mode switch, editor destruction, or page closure.
- [x] 3.7 Add parser/policy tests plus GTK/widget tests for dense below-threshold Markdown, budget terminals, multi-slice projection, image floods, oversized images, stale parse/projection/image completion, close/reopen, and exact readiness.

## 4. Single-flight workspace search and shared results

- [x] 4.1 Replace per-query raw worker launch with a panel-owned state machine containing at most one active controller/walker group and one latest compact pending `SearchRequest`.
- [x] 4.2 On superseding input, cancel the active token, replace the pending request, drain the bounded result stream, and start the revalidated latest request only after active terminal disconnection.
- [x] 4.3 Preserve result caps, channel backpressure, path/query privacy, error delivery, cancellation checks, panel-lifetime invalidation, and precise cancelling/searching/complete readiness.
- [x] 4.4 Seal accepted matches once into an immutable generation-owned shared snapshot and migrate list projection, stable match IDs, checked state, Replace Preview, and apply planning to that ownership.
- [x] 4.5 Remove the GTK-thread whole-vector clone when entering Replace Preview while preserving preview row/byte budgets, omitted-match safety, stale-file validation, and exact apply identities.
- [x] 4.6 Add service and panel tests for rapid supersession, slow cancellation, disconnect-before-restart, pending replacement, panel close, stale event delivery, one active worker group, and latest-generation publication.

## 5. Bounded search projection retirement

- [x] 5.1 Introduce a retired-generation disposal session that detaches old visible/search state immediately and removes result rows plus all auxiliary caches in deterministic bounded GTK slices.
- [x] 5.2 Key retirement work to generation ownership so delayed slices cannot remove current rows, match IDs, checked state, preview state, or readiness.
- [x] 5.3 Coalesce or supersede pending disposal safely when searches, panel closure, preview transitions, and repeated clears occur faster than retirement completes.
- [x] 5.4 Add GTK/widget scale tests at the configured result cap for result replacement, panel close, preview handoff, new results during disposal, stale slices, and exact terminal cleanup.

## 6. Incremental live-editor residency accounting

- [x] 6.1 Add one stable per-editor scalar residency record and window aggregate with saturating delta updates for load, edit, save, restore, clear/evict, reload, attach, detach, failure, and destruction transitions.
- [x] 6.2 Change ordinary below-threshold edit handling to update one record and aggregate without walking tabs, allocating a candidate vector, or sorting eviction state.
- [x] 6.3 Build a full freshness-checked eviction snapshot only on upper-threshold crossing, active enforcement, attach/detach or exceptional uncertainty, while retaining coalescing, lower-watermark hysteresis, LRU ordering, and no-progress stability.
- [x] 6.4 Revalidate protected states immediately before and during bounded clear, publish released residency only after terminal clear, and reconcile accounting after stale or interrupted eviction.
- [x] 6.5 Add debug/test reconciliation between incremental and full-scan totals plus policy and GTK/widget fixtures for zero/one/many tabs, Unicode, rapid growth, delayed restores, stale callbacks, protected over-budget state, threshold crossing, attach/detach, and interrupted clear.

## 7. Exact bounded lossy-encoding analysis

- [x] 7.1 Implement an immediate lossless analysis result for UTF-8, UTF-16LE, and UTF-16BE while leaving byte-order emission to the existing save encoder.
- [x] 7.2 Replace Windows-1252 and Shift_JIS per-scalar `String` allocation and encoder setup with one reusable exact no-replacement analysis pass and bounded diagnostic retention.
- [x] 7.3 Preserve total issue count and the first eight original line, column, and Unicode-scalar diagnostics for ASCII, multibyte Unicode, CRLF/LF, combining characters, and consecutive unrepresentable scalars.
- [x] 7.4 Add exhaustive boundary fixtures and property equivalence against actual no-replacement encoding for every supported save encoding, including lossless and lossy cases.
- [x] 7.5 Extend focused Criterion coverage for analysis-only and end-to-end 1/10/50 MiB saves, compare against the recorded baseline, and investigate any semantic mismatch or material regression before closeout.

## 8. Integrated evidence and documentation

- [x] 8.1 Add deterministic instrumentation/test probes for admitted and high-water save bytes, queued compact saves, active search worker groups, pending queries, Markdown events/nodes per slice, image count/bytes, retired rows per slice, full memory-policy scans, and whole-result clones.
- [x] 8.2 Run the focused policy, service, integration, and GTK/widget suites for each vertical slice, including targeted Miri/property checks where existing project lanes support the touched pure state machines.
- [x] 8.3 Run the explicit data-safety review over close/save/draft persistence and async state mutation, fix every confirmed finding, and retain regression tests for each corrected path.
- [x] 8.4 Run the unified GTK performance review and Rust architecture/comment reviews over all touched Rust surfaces, fixing confirmed responsiveness, scale, hot-path, boundary, and durable-invariant findings.
- [x] 8.5 Refresh `docs/benchmarks/` with commands, fixtures, environment, before/after measurements, direct bound counters, accepted variance, and any intentional limited-state behavior.
- [x] 8.6 Update affected architecture, automation/readiness, accessibility, testing, benchmark, nested `AGENTS.md`, and GTK Lush governance documentation only where runtime contracts or public/internal platform boundaries actually changed.

## 9. Full closeout

- [x] 9.1 Run formatting, focused Clippy/tests, `make check`, the complete unit and integration suites, and all changed-surface property/fixture checks.
- [x] 9.2 Run the required GTK/widget, accessibility smoke, visual smoke, visual geometry smoke, automation self-test/docs, and proof-fingerprint lanes for the touched UI and readiness behavior; inspect failures and artifacts rather than accepting wrapper exit alone.
- [x] 9.3 Run benchmark smoke and the focused before/after benchmark matrix, confirm direct resource counters satisfy every specified bound, and record any noisy or environment-limited evidence explicitly.
- [x] 9.4 Run `make check-agent-docs`, strict OpenSpec validation, `git diff --check`, and the repository-learning review; reconcile any stale or contradictory durable guidance discovered by the completed architecture work.
- [x] 9.5 Verify every proposal capability and scenario has implementation and test evidence, no task or temporary instrumentation remains, and the change is ready for spec sync and archive.
