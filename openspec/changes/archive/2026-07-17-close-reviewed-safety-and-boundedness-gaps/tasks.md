## 1. Preserve Drafts Across Incomplete Repair

- [x] 1.1 Add a plain-Rust manifest-authority/completeness outcome to draft recovery so complete, partial, failed, and replacement-eligible states cannot be conflated.
- [x] 1.2 Replace the one-page repair body scan with filesystem-boundary pagination that enforces page, entry, metadata-byte, cancellation, and diagnostic bounds while detecting a trusted terminal inventory.
- [x] 1.3 Make repaired-manifest persistence success-gated on complete inventory proof and preserve ambiguous, over-cap, or unreadable bodies without writing a partial authoritative subset.
- [x] 1.4 Thread untrusted manifest authority through startup restore and every autosave, session, deletion, and manifest-update path; require complete reconciliation or return a retryable result without clearing `draft_dirty`.
- [x] 1.5 Keep orphan inspection and execution disabled until the latest manifest is authoritative, preserving existing exact-fingerprint, stable-target, inode, continuation, and durable-commit guards after trust is restored.
- [x] 1.6 Add service-unit fault cases for multi-page completion, scan failure, classification ambiguity, metadata-cap exhaustion, failed repair writes, and later successful reconciliation.
- [x] 1.7 Add a tempdir-backed regression with more than one repair page of untitled bodies, two startup recovery cycles, all cleanup pages, and assertions that every body survives until represented by the complete manifest.

## 2. Bound Session Restore and Projection Publication

- [x] 2.1 Add a testable window-owned restore policy for per-turn page creation, fixed in-flight file-plan permits, compact pending descriptors, generation identity, and exact terminal accounting.
- [x] 2.2 Convert session restoration into scheduled bounded GTK batches that preserve persisted order and start file-backed loads only when restore planning capacity is available.
- [x] 2.3 Make file-load planning success, failure, cancellation, and editor teardown release restore permits exactly once and start only the current pending work.
- [x] 2.4 Make `open_document` and `new_tab` honor restore projection deferral for command-palette sources, sidebar row state, recent-open state, status, and other aggregate tab-derived projections.
- [x] 2.5 Publish one terminal current-generation projection rebuild while preserving CLI activation priority, selected-tab intent, unavailable-file retry state, cursor and scroll restore, and lazy draft markers.
- [x] 2.6 Cancel queued descriptors, admitted permits, scheduled sources, and projection deferral exactly once when the restore generation or window lifetime ends.
- [x] 2.7 Add pure policy tests and headless widget coverage for high mixed-tab counts, multiple GTK turns, planning saturation, one terminal rebuild, selection semantics, cancellation, and window teardown.

## 3. Stream Replace All Construction

- [x] 3.1 Validate replacement-count, file-byte, sorted-line, and recorded-range invariants before allocating line-discovery or output state.
- [x] 3.2 Replace whole-file `line_spans` retention with one-pass byte-boundary traversal that advances alongside sorted replacements and retains metadata proportional to accepted replacements.
- [x] 3.3 Preserve LF, CRLF, final unterminated line, empty-line, Unicode, stale-line, cancellation, and output-byte semantics against the current reference behavior.
- [x] 3.4 Keep original, replacement, output, and durable undo ownership inside existing per-file and aggregate caps without changing journal-before-mutation or stable-target ordering.
- [x] 3.5 Add unit/reference and integration regressions for dense short lines, sparse replacements, malformed ranges, stale content, cancellation, and durability-ambiguous writes.
- [x] 3.6 Add Criterion and performance-smoke evidence that records source lines, accepted replacements, retained edit metadata, output bytes, and undo bytes for a near-cap dense-line fixture.

## 4. Coalesce Workspace Directory Scans

- [x] 4.1 Add per-materialized-store state for one active scan, one replaceable latest compact request, scan generation, target generation, and section lifetime.
- [x] 4.2 Change refresh admission so queued requests retain only weak store identity and compact scan options, and capture strong store ownership plus the current mirror only after worker capacity is granted.
- [x] 4.3 Cooperatively cancel superseded traversal and accept completion only after revalidating section lifetime, store identity, target generation, and scan generation.
- [x] 4.4 Finalize reconciliation caches, watcher targets, selection, readiness, and pending-scan startup exactly once for the latest live generation.
- [x] 4.5 Fold empty-folder detection into accepted scan evidence where practical or add equivalent per-folder active/latest generations so stale probes cannot overwrite newer state.
- [x] 4.6 Add deterministic policy and headless widget tests for slow scans, watcher/manual refresh churn, store removal, stale empty-folder results, pending replacement, and readiness cleanup.
- [x] 4.7 Add pressure evidence for active/pending scan high-water, weak queued ownership, mirror captures at admission, cancellation checkpoints, and latest-only terminal publication.

## 5. Make Plain-Data Disposal Non-Blocking

- [x] 5.1 Add an app-owned disposal admission policy with explicit worker, queued-job, and estimated retained-byte bounds plus exclusive progress for one overweight job.
- [x] 5.2 Add non-blocking future-drop reservation before document-sized GTK publication, retain immediate ownership return for statically small jobs, and release policy accounting on every worker terminal path.
- [x] 5.3 Keep reservations attached through replaceable UI ownership; on capacity pressure retain only one compact latest request and one retry or capacity-wakeup source, including exact cancellation when the widget, window, or generation ends.
- [x] 5.4 Adapt existing Markdown, Replace Preview, search/undo, and other plain-disposal producers to reserve their conservative weight before large values cross onto GTK without moving GTK-owned objects off thread.
- [x] 5.5 Migrate pure `Send` destruction currently using `spawn_blocking_then` with no-op GTK completion in command palette, Notes browser, local history, and focus indexing; retain lifecycle-specific watcher teardown on its existing path.
- [x] 5.6 Add unit tests for count/byte admission, full and closed ownership return, pre-admitted nested destruction, overweight progress, panic-safe release, compact pending replacement, and teardown.
- [x] 5.7 Add aggregate multi-producer headless or responsiveness evidence showing immediate capacity rejection, bounded compact pending high-water, pre-admitted final destruction off GTK, eventual drain, and continued GTK heartbeat progress.

## 6. Slice Minimap Analysis

- [x] 6.1 Add editor-owned minimap analysis generation, bounded cursor/snapshot state, accepted cache state, and exact terminal cleanup for supported documents.
- [x] 6.2 Replace full-buffer wrapped-layout and long-line scans with O(1) cached decisions or configured character/item slices that yield between GTK turns.
- [x] 6.3 Reuse accepted analysis evidence for layout availability and optional long-line markers while preserving the existing supported-size tier and explicit unavailable feedback.
- [x] 6.4 Invalidate active analysis on edits, wrap changes, marker preference changes, file replacement, editor eviction/reload, and page teardown before stale work can schedule or publish another slice.
- [x] 6.5 Add pure helper and headless widget regressions for many-short-line documents, current-generation completion, mid-scan edits, marker toggles, teardown, and unchanged supported-tab visibility.
- [x] 6.6 Add performance evidence for per-turn characters/items, GTK progress, generation cancellation, cache ownership, and terminal current projection.

## 7. Prune Non-Contributing Note-Body Scoring

- [x] 7.1 Express per-field score contribution bounds beside the GTK-free note scorer and skip a body only after metadata establishes eligibility and the body cannot improve the row result.
- [x] 7.2 Preserve body-only matches, Unicode normalization, empty-query behavior, deterministic source-ordinal ties, category grouping, cancellation checkpoints, and bounded top-result ownership.
- [x] 7.3 Add an unpruned test reference plus generated/property cases comparing selected identities, scores, and order for metadata-only hits, body-only hits, ties, long bodies, and rapid supersession.
- [x] 7.4 Extend note-search benchmarks and performance artifacts with bodies examined, bodies safely pruned, candidates scored, active/pending queries, retained results, and final-query equivalence.

## 8. Documentation and Direct Evidence

- [x] 8.1 Calibrate and document session page/permit, workspace scan, disposal count/byte, minimap slice, and repair page limits with ownership rationale rather than host timing alone.
- [x] 8.2 Extend the existing benchmark documentation and smoke summary with the seven closeout fixtures and their direct count, byte, generation, worker, pending-owner, and GTK-turn high-water fields.
- [x] 8.3 Update root or nested `AGENTS.md` architecture summaries only where implementation introduces a durable new coordinator, policy type, or module ownership contract, keeping the rules index synchronized if any rule changes.
- [x] 8.4 Confirm no public automation action, D-Bus member, readiness predicate, snapshot field, helper flag, persistence envelope, GTK Lush API, dependency, or packaging contract changed; update the corresponding docs and gates if implementation proves otherwise.

## 9. Verification and Closeout

- [x] 9.1 Run the focused draft, session, Replace All, workspace tree, disposal, minimap, and note unit/service tests under default and all-feature configurations.
- [x] 9.2 Run `make test-int` and `make test-prop`, including the multi-start recovery and optimized-versus-reference equivalence fixtures.
- [x] 9.3 Run the affected tests through the private headless widget harness repeatedly enough to confirm no `FLAKY:` result, fixed-sleep dependency, live-display use, stale callback, or readiness leak remains.
- [x] 9.4 Run `make check`, `make test`, benchmark compilation, and every documentation or policy gate selected by the final changed-file surface, fixing any surfaced blocker in the same work stream.
- [x] 9.5 Run `make performance-smoke` and verify the generated artifacts report every new direct high-water field and preserve existing boundedness evidence.
- [x] 9.6 Run `openspec validate close-reviewed-safety-and-boundedness-gaps --strict`, `openspec validate --all --strict --no-interactive`, and `git diff --check`, then reconcile any predecessor-delta overlap before archive.
