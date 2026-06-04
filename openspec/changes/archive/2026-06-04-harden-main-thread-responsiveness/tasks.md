## 1. Baseline and Shared Responsiveness Helpers

- [x] 1.1 Reconfirm the current main-thread hotspots in search backup persistence, draft autosave snapshots, Save As canonical bookkeeping, Replace preview generation, Markdown preview refresh, minimap marker collection, and lossy encoding analysis.
- [x] 1.2 Extract or generalize the save pipeline's chunked `TextBuffer` snapshot pattern so autosave, encoding analysis, and preview-adjacent workflows can reuse it without reading GTK state from worker threads.
- [x] 1.3 Add narrow test-only seams needed to delay persistence or force chunked snapshot paths without adding broad production traits or a new background-job framework.
- [x] 1.4 Add unit coverage for the shared snapshot threshold and chunk scheduling decisions, including small-buffer direct capture and large-buffer chunked capture.

## 2. Replace All Undo Backup Persistence

- [x] 2.1 Move search-panel undo-backup save work from the GTK action path into `spawn_blocking_then`, preserving immediate in-memory undo state and visible undo affordances.
- [x] 2.2 Move search-panel undo-backup delete/clear work into `spawn_blocking_then`, preserving panel-close and undo-complete safety-window semantics.
- [x] 2.3 Protect asynchronous undo-backup save/delete completions with the existing undo generation counter and disk-ordering lock so stale completions cannot resurrect inactive UI state.
- [x] 2.4 Add service or unit tests for save-after-clear, clear-after-save, and delayed persistence ordering.
- [x] 2.5 Add widget coverage proving the search panel updates visible undo state before a delayed disk save/delete finishes.

## 3. Draft Autosave Snapshot Responsiveness

- [x] 3.1 Rework periodic draft autosave so dirty tab discovery stays quick and large buffer snapshots are captured through bounded main-loop chunks before background writes.
- [x] 3.2 Ensure draft dirty flags are cleared only after the matching editor snapshot/write path is accepted, and failed background writes leave the editor eligible for a later autosave attempt.
- [x] 3.3 Preserve close-time draft flush safety and document any intentionally synchronous close-only exception that remains after regular autosave is optimized.
- [x] 3.4 Add unit or service tests for failed draft writes, retry eligibility, and stale editor generations.
- [x] 3.5 Add widget coverage proving multiple dirty large tabs do not snapshot all text in one autosave timer callback.

## 4. Save As Canonical Bookkeeping

- [x] 4.1 Replace synchronous post-Save-As canonical path probes with a background canonical refresh that applies only if the editor still owns the saved destination.
- [x] 4.2 Keep immediate Save As UI updates on the GTK thread using the chosen destination path while the canonical refresh is pending.
- [x] 4.3 Add tests for stale canonical refresh results after editor close, rename, or a newer Save As request.
- [x] 4.4 Add duplicate-tab or open-path bookkeeping coverage proving the async refresh keeps current-path and old-path entries consistent.

## 5. Replace Preview Generation

- [x] 5.1 Move non-trivial Replace preview generation to a worker using owned query, replacement, match, and preview-mode inputs captured on the GTK thread.
- [x] 5.2 Add a pending preview state that keeps the search panel responsive and prevents applying or confirming stale preview rows while generation is in flight.
- [x] 5.3 Reject stale preview results when the query, replacement text, search results, preview mode, or panel lifetime changes.
- [x] 5.4 Add pure tests for replacement preview generation on large match sets.
- [x] 5.5 Add widget tests for pending preview state and stale preview result rejection.

## 6. Preview, Minimap, and Encoding Guardrails

- [x] 6.1 Add bounded Markdown preview refresh behavior for large inputs through chunked snapshotting, asynchronous preprocessing, or a documented paused/limited preview state.
- [x] 6.2 Add tests for the Markdown preview large-input path, including a stale refresh result or explicit paused-state assertion.
- [x] 6.3 Prevent minimap long-line marker collection from doing an unbounded full-buffer scan in one GTK callback by using cached load-time data, a chunked scan, or a documented marker threshold.
- [x] 6.4 Add tests for minimap long-line marker behavior at the threshold and for preserving ordinary marker behavior on small documents.
- [x] 6.5 Move non-small lossy encoding analysis behind bounded snapshot and worker processing, then show the confirmation only for the still-current document and encoding request.
- [x] 6.6 Add tests for stale lossy encoding analysis results and ordinary small-buffer lossy warning behavior.

## 7. Performance Smoke and Documentation

- [x] 7.1 Extend the lightweight performance-smoke lane with at least one coarse main-loop stall or elapsed-time check covering a workflow changed by this proposal.
- [x] 7.2 Ensure the performance-smoke report records fixture size, configured threshold, elapsed timing, toolkit/build context, and enough environment detail to interpret shared-runner noise.
- [x] 7.3 Update `docs/end-user-coverage.md` if lane ownership or performance-smoke behavior changes.
- [x] 7.4 Update relevant local guidance only if new helper patterns or test harness assumptions become part of the repo contract.

## 8. Validation

- [x] 8.1 Run `make test-unit` for service, pure helper, encoding, preview, and persistence behavior.
- [x] 8.2 Run `make test-int` for cross-service filesystem and persistence workflows touched by Replace All, drafts, or Save As bookkeeping.
- [x] 8.3 Run targeted `make test-widget-headless` coverage for search panel, draft autosave, Save As bookkeeping, preview, minimap, and encoding widget behavior added by this change.
- [x] 8.4 Run `make performance-smoke` and confirm the new responsiveness report data is produced or skips with a clear host-supported reason.
- [x] 8.5 Run `make check` and any narrower policy checks required by changed filesystem or documentation guidance.
- [x] 8.6 Run `openspec validate harden-main-thread-responsiveness --strict` and verify every task that is complete is checked off before archive.
