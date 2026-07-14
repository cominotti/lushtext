## 1. Confirm Toolkit Contract and Baseline Consumers

- [x] 1.1 Reconfirm the target GTK 4.22 `GtkTextIter` invalidation and `GtkTextMark` lifecycle contract from official GNOME documentation/source and record the binding APIs available under gtk4-rs 0.11.
- [x] 1.2 Inventory every direct and chunked `buffer_snapshot` consumer, its editability policy, freshness owner, terminal callback, disposal path, and retry/discard behavior.
- [x] 1.3 Add deterministic widget seams for mutation between slices, terminal cleanup, source disposal, and scheduled-source ownership.

## 2. Implement a Lifecycle-Owned Snapshot Session

- [x] 2.1 Add a private chunked snapshot session that owns the buffer, progress `TextMark`, output, byte policy, mutation/cancellation state, changed handler, scheduled source, and one terminal callback.
- [x] 2.2 Resolve a fresh iterator from the mark for each slice, check cancellation before access, advance/move the mark safely, and preserve bounded chunk plus byte-overflow behavior.
- [x] 2.3 Funnel capture, cancellation, overflow, and disposal through idempotent cleanup that removes marks, handlers, sources, and callbacks exactly once.
- [x] 2.4 Add focused tests for inserts/deletes before and after the mark, final-slice mutation, explicit cancellation, overflow, callback-once behavior, and absence of GTK warnings.

## 3. Migrate Snapshot Consumers Explicitly

- [x] 3.1 Migrate draft autosave and close flush to typed cancellation, preserving draft-dirty retry and one coalesced latest-generation pass.
- [x] 3.2 Migrate periodic local history so mutation releases the admission permit and schedules only current eligible work.
- [x] 3.3 Migrate editor save so read-only/cursor state is restored on every non-success path and only coherent complete text reaches encoding/write work.
- [x] 3.4 Migrate encoding analysis, note preview, and local-history restore to discard stale captures and rely on their existing generation/debounce policy for the latest request.
- [x] 3.5 Remove the iterator-carrying helper variants after all call sites and tests use the lifecycle-owned API.

## 4. Supersede Periodic-History Scheduling

- [x] 4.1 Replace the raw five-minute timeout with tab-owned `gtk_lush_settle::SupersedingTimer` state while retaining periodic/editor/path/edit generation checks.
- [x] 4.2 Cancel the timer and active snapshot on clean transition, path change, ineligibility, editor disposal, and successful lifecycle reset.
- [x] 4.3 Add repeated clean/dirty/save cycle tests proving one timer, one snapshot, no stale callbacks, and readiness completion.

## 5. Verification and Documentation

- [x] 5.1 Add high-signal comments explaining the GTK mark/mutation invariant and consumer terminal-state contract; update local guidance if snapshot ownership rules become durable policy.
- [x] 5.2 Run focused widget tests for every migrated consumer, local-history property tests, save/draft failure tests, and repeated runtime-warning checks.
- [x] 5.3 Run `make test-unit`, relevant headless widget suites, `make check`, `make lint-advisory`, `make pre-commit`, and `git diff --check`.
- [x] 5.4 Refresh and run accessibility, visual-geometry, and automation-readiness proofs affected by editor/widget source changes.
- [x] 5.5 Run strict OpenSpec validation and verify no GTK Lush public API was added without second-consumer evidence or stewardship work.
