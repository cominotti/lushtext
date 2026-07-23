## 1. Deterministic policy and characterization

- [x] 1.1 Add test-only counters/oracles that expose flattened-row expansion scans, bookmark-preview active and pending ownership, large-file processing chunks, and registered Undo hook targets without changing production behavior.
- [x] 1.2 Add focused regression tests that reproduce the current full-tree targeted-refresh work, bookmark worker accumulation, coarse post-read cancellation, and cross-target Undo hook consumption before replacing each implementation.
- [x] 1.3 Define and document explicit large-file processing chunk policy and compact bookmark-preview request state using existing model/service ownership conventions and no new dependency or generic scheduler.

## 2. Incremental workspace refresh context

- [x] 2.1 Update workspace row expansion/collapse wiring so the section's expanded-path state remains authoritative as users interact with materialized rows.
- [x] 2.2 Update accepted child/top-level reconciliation and rename/removal handling so only affected expansion entries change and stale or superseded plans cannot mutate current state.
- [x] 2.3 Split targeted refresh preparation from true model-replacement capture, removing the flattened-model scan from in-place targeted refresh while retaining selection and refresh-metric initialization.
- [x] 2.4 Keep an explicit full derivation for bootstrap, pre-replacement capture, and test oracle use, and prove generated expand/collapse/splice/reload sequences match it.
- [x] 2.5 Add representative GTK/performance fixtures proving one-directory refresh work is independent of unrelated materialized-row count and unchanged expansion, selection, watcher, and readiness behavior is preserved.

## 3. Bounded closed-file bookmark previews

- [x] 3.1 Add a Notes-browser-local coordinator with at most one active closed-file excerpt request and one replaceable latest compact request, including generation, target identity, and cancellation ownership.
- [x] 3.2 Route closed-file bookmark selection through the coordinator so supersession cancels active work, replaces pending intent, and starts the latest request only after the active terminal.
- [x] 3.3 Add a cancellable bookmark-excerpt service entry point with checks around metadata and bounded ingestion plus checkpoints during line scanning, while preserving the existing non-cancelling service behavior where needed.
- [x] 3.4 Make every success, unavailable, error, and cancelled terminal clear active ownership exactly once, revalidate browser lifetime/generation/selection before publication, and then consider only the latest pending request.
- [x] 3.5 Cancel active and pending closed-file work when selection switches to live-editor/non-bookmark content, inventory mode changes, or the Notes browser is disposed.
- [x] 3.6 Add service and headless GTK/widget tests for rapid selection, delayed cancellation, stale completion, latest-only publication, live-editor bypass, error terminals, mode replacement, and dialog teardown, asserting active and pending high-water values.

## 4. Cooperative large-file processing

- [x] 4.1 Refactor raw-byte classification and encoding detection into cancellation-aware bounded work while preserving BOM handling, BOM-less UTF-16 heuristics, fallback choice, and binary-like evidence.
- [x] 4.2 Decode large supported inputs incrementally with codec state and safe scalar/chunk boundaries, retaining a small direct path with equivalent cancellation and terminal semantics.
- [x] 4.3 Accumulate line-ending and file-health evidence during bounded processing where practical, eliminating redundant whole-document passes without changing any successful finding, count, confidence, or save policy.
- [x] 4.4 Thread existing load cancellation through classification, decoding, and analysis; stop later work after cancellation and verify the existing transient load/disposal permits release exactly once on every terminal.
- [x] 4.5 Add reference-equivalence tests for ASCII, multibyte UTF-8 across chunk boundaries, UTF-8 BOM, UTF-16 LE/BE with and without BOM, Windows-1252 fallback, mixed line endings, NUL evidence, non-breaking spaces, and zero-width characters.
- [x] 4.6 Add deterministic cancellation tests at classification, decoding, analysis, success, stale-generation, and teardown stages, recording bounded chunk progress, retained bytes, no partial publication, and next-request capacity recovery.

## 5. Parallel-safe Replace and Undo fault seams

- [x] 5.1 Replace the global one-slot Undo after-metadata hook with a test-only path-keyed one-shot registry whose callback is removed under the lock and invoked after the lock is released.
- [x] 5.2 Return cleanup ownership from hook registration so unconsumed target hooks are removed after failure or early test exit and expose a test-only registry-empty assertion.
- [x] 5.3 Update existing Undo growth and restored-identity tests to register exact target paths without serializing the wider content-search suite.
- [x] 5.4 Add adversarial interleaving and parallel tests with distinct temporary targets, proving each hook fires once for its own path and no registration leaks into another operation or test.
- [x] 5.5 Repeatedly run the focused content-search tests with parallel test threads and confirm the formerly intermittent Undo failures remain deterministic.

## 6. Performance evidence and closeout

- [x] 6.1 Extend deterministic performance-smoke or benchmark evidence with affected-row refresh counts, bookmark active/pending high-water, large-file chunk/cancellation/permit metrics, and parallel hook-registry isolation.
- [x] 6.2 Add or extend an opt-in near-supported-limit file-load diagnostic that records fixture size, encoding, profile, environment, RSS context, cancellation progress, and transient ownership without becoming a default-CI timing or memory gate.
- [x] 6.3 Run `cargo fmt --all --check` and `git diff --check`, then run the focused changed-module tests including release-semantic cancellation/ownership coverage where debug assertions are disabled.
- [x] 6.4 Run `make check` and `make lint-advisory`, fixing every current blocker in the same work stream.
- [x] 6.5 Run `make test-unit`, `make test-int`, `make test-prop`, and `make test-widget` with the new deterministic boundary tests enabled.
- [x] 6.6 Run `make performance-smoke` and capture fixture/environment context for the new evidence without absolute cross-machine timing gates.
- [x] 6.7 Run `openspec validate close-residual-responsiveness-and-test-gaps --strict` and `openspec validate --all --strict --no-interactive` on the implementation snapshot.
- [x] 6.8 Review the final Rust diff against Hexagonal Architecture, data-safety, responsiveness/scale/hot-path, GTK lifecycle, and comment-quality contracts; confirm no persisted format, action, D-Bus member, dependency, generic manager, or unrelated profiling optimization entered scope.
