## 1. Plain policy and ownership foundations

- [x] 1.1 Add a GTK-free ordered workspace-search traversal-plan type that canonicalizes roots once, removes exact duplicates, collapses covered descendants, preserves first-folder display ownership, and reports whether fallback identity tracking is required.
- [x] 1.2 Add explicit saturating entry and path-byte admission for the ambiguous-alias fallback identity ledger, including a typed incomplete-search terminal and direct boundary metrics.
- [x] 1.3 Add a GTK-free workspace-persistence state machine for requested, in-flight, durable, and failed generations, including latest-state scheduling, bounded retry decisions, pending/readiness predicates, and close-flush decisions.
- [x] 1.4 Extend `NoteSourceLimits`, truncation reasons, and metrics with sidecar-path and aggregate construction-scratch byte ceilings plus current and peak accounting helpers.
- [x] 1.5 Extend or locally compose the existing plain-disposal reservation APIs so file-load and draft workers can reserve conservatively, shrink to measured bodies, transfer guards without full-body work on GTK, and release every permit exactly once.
- [x] 1.6 Add high-signal comments beside traversal precedence, dual load/disposal permits, transferable draft guards, persistence-generation ordering, and Notes peak-accounting invariants; avoid comments on obvious delegation.

## 2. Workspace content-search traversal

- [x] 2.1 Build one immutable traversal plan from the generation-owned workspace scope before launching `services/content_search`, and keep ordered display-root attribution separate from minimal engine traversal roots.
- [x] 2.2 Remove the visited-file set and lock from single-root and provably disjoint-root searches so no-match retained identity memory is independent of visited-file count.
- [x] 2.3 Use the bounded fallback ledger only for unresolved alias coverage, stop before its next entry or byte charge exceeds policy, and propagate typed incomplete feedback through existing search events and terminal UI state.
- [x] 2.4 Preserve cancellation, progress, result-cap, error aggregation, folder-order attribution, duplicate-result suppression, and active-plus-latest generation semantics after root normalization.
- [x] 2.5 Add pure and filesystem-fixture tests for one huge no-match root, exact duplicate roots, child-before-parent and parent-before-child roots, canonical aliases, unavailable roots, exact ledger limits, one-over-limit termination, cancellation, and result equivalence.

## 3. Guarded file-load bodies

- [x] 3.1 Pair byte-weighted load admission with conservative ordinary-lane disposal reservation before decode, shrink successful reservations to measured decoded-body weight, and keep compact error outcomes unguarded.
- [x] 3.2 Change admitted load outcomes to carry guarded decoded text through `gtk-lush-tasks` completion so a missing editor weak owner, stale generation, or early rejection retires the body off GTK.
- [x] 3.3 Keep the guard inside direct and `ChunkedLoadInstall` paths while GTK borrows UTF-8 slices, and release the transient load permit exactly once on success, failure, cancellation, and editor teardown.
- [x] 3.4 Transfer an accepted guard into an eligible `last_clean_text` baseline without cloning, convert bounded transit admission into same-worker retained retirement ownership, and retire it off GTK when baseline policy is ineligible or a later baseline/editor teardown becomes final owner.
- [x] 3.5 Keep disposal-capacity retry bounded to compact load intent, admit one supported overweight transit owner under an explicit additive peak, and prevent long-lived current owners from consuming the eight transit slots.
- [x] 3.6 Add deterministic policy and headless GTK/widget tests with destructor-thread sentinels for lost editor, stale completion, direct install, sliced cancel, accepted baseline, baseline replacement, repeated reload/close, exact dual-permit release, and GTK heartbeat progress.

## 4. Guarded eager and lazy draft restore

- [x] 4.1 Represent startup eager draft bodies with individually transferable guarded ownership so removing one body from preload state moves its reservation with it while unused bodies remain eligible for worker retirement.
- [x] 4.2 Reserve progress-lane disposal capacity before each lazy restore read, keep only the compact serialized ticket while capacity is unavailable, shrink the reservation to the measured UTF-8 body, and return a guarded restore resolution.
- [x] 4.3 Pass eager and lazy bodies through `BufferReplacementRequest::new_guarded`, preserve the complete draft restore ticket across slices, and return the same guard on completion, cancellation, freshness loss, failure, and teardown.
- [x] 4.4 Transfer an accepted guard into an incoming-size-eligible local-history baseline without a second full-body clone, release progress transit admission while retaining same-worker retirement, retire ineligible bodies off GTK, and remove all document-sized `small_unreserved` draft/load baseline paths.
- [x] 4.5 Preserve manifest authority, draft evidence, restore feedback, dirty/load/path generations, ordered deletion tombstones, and serialized one-complete-body recovery behavior on every rejected terminal.
- [x] 4.6 Add service and headless GTK/widget tests for eager extraction, unused-preload disposal, progress-capacity deferral, lazy success, stale-before-install, supersession during replacement, eligible/ineligible baseline, lost window/editor, exact guard release, recovery-evidence preservation, and main-loop progress.

## 5. Workspace persistence terminal and close safety

- [x] 5.1 Replace sidebar persistence booleans with the plain generation state while retaining the existing debounce and one-in-flight worker limit; starting a write must leave its generation non-durable until success.
- [x] 5.2 Apply worker terminals by matching generation, schedule the newest pending snapshot after older success, and keep current failures pending with bounded backoff or explicit retry/new-mutation wakeup rather than a tight loop.
- [x] 5.3 Publish user-safe workspace-save failure and recovery feedback through the window notification path without exposing private data paths, and keep existing automation pending/readiness state truthful for dirty, in-flight, failed, and retry-waiting generations.
- [x] 5.4 Add an asynchronous no-debounce sidebar flush API that waits behind an in-flight generation and completes only when the newest requested snapshot is durable or a typed error is returned.
- [x] 5.5 Insert workspace flush into window close safety after draft protection and before final session save/destruction; abort close, restore sensitivity, and retain retryable state on failure while preserving document fingerprint revalidation.
- [x] 5.6 Add pure state tests and injected-filesystem integration/widget tests for close before debounce, mutation during write, stale success, current failure, bounded retry, later success, close during in-flight work, close-time failure, feedback resolution, truthful readiness, and newest-state restart restoration.
- [x] 5.7 Update `docs/automation.md` and `docs/automation-reference.md` for the strengthened semantics of the existing workspace-persistence readiness blocker and pending snapshot state, without adding a new D-Bus member or private identifier.

## 6. Notes construction scratch bounds

- [x] 6.1 Calibrate and document conservative sidecar-path and aggregate construction-scratch ceilings that coexist with one maximum recovery-metadata input while preserving the existing 10,000-entry and 64 MiB final-source limits.
- [x] 6.2 Replace count-only sidecar traversal with `scan_directory_bounded_with_cancel_and_bytes`, cap it by remaining aggregate construction capacity, charge complete path ownership before admission, and distinguish path-byte, construction, sidecar-count, cancellation, and traversal-error terminals.
- [x] 6.3 Reserve exact raw sidecar input plus a conservative parsed-model/diagnostic envelope before recovery reads, then charge measured document, canonical folder/path, diagnostic, temporary category vector/capacity, and overlapping final-row ownership with saturating current/peak metrics.
- [x] 6.4 Ensure cancellation releases construction scratch on the worker, only final measured source plus compact metrics cross to GTK, and progress reservation shrinks to terminal retained ownership rather than hiding scratch.
- [x] 6.5 Preserve canonical deduplication, workspace scope, category ordering, bookmark-only mode, query active-plus-latest behavior, row previews, activation, accessible truncation feedback, and dialog reuse for all admitted rows.
- [x] 6.6 Add unit/property and filesystem-fixture tests for long and Unicode paths, exact/one-over path bytes, maximum sidecar count, a near-limit sidecar, canonical aliases, many diagnostics, scratch exact/one-over limits, cancellation, typed truncation, and admitted-row equivalence.

## 7. Direct performance and release evidence

- [x] 7.1 Add Criterion or performance-smoke evidence for huge single-root no-match search and overlapping roots, recording traversal-plan size, fallback-ledger high-water, retained bytes, cancellation, and semantic equivalence.
- [x] 7.2 Add near-limit file and draft disposal evidence that records transient admission, disposal reservations, destructor thread, accepted baseline transfer, exact terminal release, and an independent GTK heartbeat.
- [x] 7.3 Add workspace-persistence fault-matrix smoke evidence for debounce bypass, in-flight ordering, failure/retry, readiness, close abort, recovery success, and newest durable payload.
- [x] 7.4 Add Notes construction evidence for path, sidecar, scratch, final-source, diagnostic, and entry high-water across exact-boundary, one-over-boundary, Unicode, cancellation, and error-heavy fixtures.
- [x] 7.5 Run focused disposal ownership and search boundary tests with debug assertions disabled and confirm no required state mutation or release depends on assertion evaluation.
- [x] 7.6 Compare materially changed GTK-free search and Notes paths against same-environment baselines, reporting distributions or effect size without absolute cross-machine timing gates.

## 8. Full validation and closeout

- [x] 8.1 Run `cargo fmt --all --check` and `git diff --check` during implementation cleanup.
- [x] 8.2 Run `make check` and `make lint-advisory`, fixing every current blocker in the same work stream.
- [x] 8.3 Run `make test-unit`, `make test-int`, and `make test-prop` with all new deterministic boundary tests enabled.
- [x] 8.4 Run `make test-widget` and the focused release-semantic headless proofs for file/draft disposal, workspace close safety, and readiness.
- [x] 8.5 Run `make performance-smoke` and capture the new search, disposal, persistence, and Notes high-water evidence with fixture/environment context.
- [x] 8.6 Run `make check-automation-docs`, `make automation-client-self-test`, `openspec validate close-reviewed-ownership-and-persistence-gaps --strict`, and `openspec validate --all --strict --no-interactive`.
- [x] 8.7 Review the final Rust diff against Hexagonal Architecture, data-safety, responsiveness/scale/hot-path, and comment-quality contracts; confirm no persisted format, action, D-Bus member, dependency, generic manager, or unrelated adapter refactor entered scope.
