## 1. Palette directory traversal bound

- [x] 1.1 Add the named 100,000-directory policy to `services::palette::index`, charge distinct canonical directories before retention/descent, and preserve independent file-limit, depth-limit, alias-deduplication, cancellation, typed-truncation, and usable-partial-index semantics.
- [x] 1.2 Extend file-index metrics and test hooks with direct retained/scanned directory high-water evidence without exposing paths or allocating measurement state proportional to unvisited work.
- [x] 1.3 Add pure service tests for directory-only forests, independent file/directory limits, overlapping and aliased roots, exact-at-limit behavior, cancellation checkpoints, and deterministic truncation.
- [x] 1.4 Extend focused Criterion or performance fixtures to measure directory-heavy traversal throughput and retained-state bounds alongside the existing file-index evidence.

## 2. Replace Preview retirement and confirmation

- [x] 2.1 Audit every preview enter, exit, invalidate, replace, stale-completion, panel-close, and queued-request path; detach prior generation state immediately and route GTK-owned projection state plus plain outcome payloads through their applicable bounded retirement paths.
- [x] 2.2 Replace GTK-thread confirmation filtering with an asynchronous current-generation selection handoff using the incrementally maintained checked-match identities, and destroy unchecked/rejected payloads on the worker side.
- [x] 2.3 Revalidate search, preview, and panel generation immediately before invoking the existing Replace All callback, preserving omission, checked-row, stable-identity, stale-file, cancellation, and diagnostic-privacy contracts.
- [x] 2.4 Add pure and GTK/widget tests for near-limit confirmation subsets, all-unchecked confirmation, visible-outcome replacement, stale worker outcomes, invalidation during selection, rapid preview requests, panel close, and exact retirement/readiness completion.

## 3. Markdown disposal and retirement backpressure

- [x] 3.1 Route stale render plans, unprojected batch tails, cancelled planner results, and superseded queued Markdown sources through bounded worker-drop ownership before their final plain-data release on GTK.
- [x] 3.2 Cap ordinary detached `MarkdownRetirementSession` ownership at two generations, retain only one latest pending render request while full, and resume only the current request after bounded GTK retirement creates capacity.
- [x] 3.3 Keep GTK buffers, embeds, links, tags, and widgets on the main thread; preserve per-turn retirement budgets, image admission, generation guards, close/dispose behavior, and readiness across active, pending, projection, image, and retirement state.
- [x] 3.4 Add policy and GTK/widget tests for dense near-limit plans, supersession with unprojected tails, pending-source replacement, more than two rapid detached generations, close under pressure, latest-generation resumption, exact terminal state, and direct ownership high-water marks.

## 4. Exact GTK-turn and reentrancy contracts

- [x] 4.1 Charge every successfully received workspace-search event variant to the existing 250-event per-turn budget and add deterministic mixed, progress-only, error/cap, terminal, cancellation, and latest-request tests.
- [x] 4.2 Mark sliced buffer mutation as started before each non-empty signal-emitting GTK delete or insert, release the mutable session borrow across the GTK call, and revalidate identity before recording progress or scheduling continuation.
- [x] 4.3 Add a GTK/widget regression test whose first synchronous `changed` emission supersedes replacement, covering delete and insert paths, no borrow conflict, exact old-session cleanup, one terminal outcome, and exact newer final content/editability/saveability/projection state.

## 5. Integrated evidence and documentation

- [x] 5.1 Add deterministic scalar probes for directory retention, preview selection/retirement, Markdown active/pending/detached ownership, total search events per turn, and replacement terminal cleanup; assert bounds directly and keep document text out of diagnostics.
- [x] 5.2 Extend existing benchmark/performance-smoke reporting with representative directory-only, near-limit preview, rapid dense Markdown, mixed-event, and reentrant replacement fixtures, including environment, fixture size, high-water counters, and accepted thresholds.
- [x] 5.3 Update affected performance-budget, benchmark, testing, architecture, automation/readiness, and nested guidance only where observable or durable contracts changed; keep public APIs and GTK Lush boundaries unchanged.
- [x] 5.4 Run the explicit data-safety, unified GTK performance, Rust architecture, Rust comment, GTK internals, and GTK testing reviews over their touched surfaces and fix every confirmed in-scope finding with regression evidence.

## 6. Validation and closeout

- [x] 6.1 Run formatting, focused Clippy, pure service/policy tests, targeted search/Markdown/buffer widget tests, `make check`, `make test-unit`, and the complete `make test` suite.
- [x] 6.2 Run benchmark compilation plus focused before/after measurements and the applicable performance-smoke lane; confirm direct counters satisfy the directory, payload, generation, event, worker, and GTK-slice bounds.
- [x] 6.3 Run the repository's accessibility, visual, visual-geometry, automation self-test/docs, and proof-fingerprint checks required by the touched UI/readiness surfaces, inspecting generated artifacts for real regressions.
- [x] 6.4 Run `make check-agent-docs`, strict change/spec/all OpenSpec validation, `git diff --check`, and the repository-learning review; reconcile any stale durable guidance caused by the implementation.
- [x] 6.5 Verify every proposal capability, delta requirement, scenario, and task has implementation and test evidence, no temporary instrumentation remains, and the change is ready for spec sync and archive.
