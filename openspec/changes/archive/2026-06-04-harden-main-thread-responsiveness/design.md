## Context

LushText already has the right building blocks for responsive GTK workflows:
`spawn_blocking_then` for filesystem and pure worker work, generation counters
for stale async results, and a chunked buffer snapshot path for large saves. The
remaining risks are concentrated in a few user-visible paths that still perform
one of three expensive operations directly from GTK callbacks:

- synchronous filesystem or canonicalization work after UI state changes;
- whole-buffer `TextBuffer` snapshots gathered in one main-loop turn; and
- pure preview, scan, or encoding analysis that can grow with search results or
  document size.

GTK objects and `TextBuffer` access must stay on the GTK thread. The design
therefore does not try to read editor buffers from worker threads. Instead, it
turns long main-thread work into bounded snapshots, moves disk and pure analysis
work to background closures, and applies results back to widgets only after
generation and lifetime checks pass.

## Goals / Non-Goals

**Goals:**

- Remove synchronous disk I/O and canonical path probes from the remaining
  hot GTK interaction paths identified during exploration.
- Reuse the existing async and chunked snapshot idioms instead of introducing a
  new runtime, dependency, or application-level scheduler.
- Keep data-safety boundaries intact for Replace All undo, draft recovery, Save
  As bookkeeping, and encoding warnings.
- Make expensive preview, minimap, and encoding work either asynchronous,
  chunked, or explicitly degraded for large inputs.
- Add regression tests that prove ordering, stale-result rejection, UI state
  timing, and persistence behavior rather than only asserting final happy-path
  output.

**Non-Goals:**

- Do not replace GTK, GtkSourceView, or the current GLib main-loop model.
- Do not read or mutate GTK widgets, `TextBuffer`, or GObject state from worker
  threads.
- Do not redesign the Markdown renderer around a full retained render tree in
  this change; use bounded preprocessing, chunked snapshotting, or explicit
  large-document gating where a full render-plan architecture would be too large.
- Do not weaken close-time draft safety. If close still needs a synchronous
  final flush for crash-recovery correctness, keep it as an intentional bounded
  exception and optimize the regular autosave path first.

## Decisions

### Keep GTK-only state collection on the main thread, but bound it

`TextBuffer` snapshots and widget state inspection remain GTK-thread work.
Large snapshots SHALL use a reusable chunked snapshot helper patterned after
the save pipeline: copy bounded text ranges, yield back to the main loop between
chunks, and resume until a coherent string is available. Small buffers can keep
the direct snapshot fast path when the known size is safely below the threshold.

Alternative considered: move buffer reads directly to a worker. This is rejected
because GTK objects are not thread-safe and crossing that boundary would trade a
responsiveness issue for undefined or fragile toolkit behavior.

### Move filesystem persistence and canonicalization into workers

Replace All undo-backup save/delete operations and Save As canonical path
refreshes SHALL use `spawn_blocking_then`. The UI-visible state should update
immediately on the main thread, while disk work runs behind a generation token
and a disk-ordering lock where the current service already requires one.

Alternative considered: leave small JSON deletes and canonical probes
synchronous because they are usually quick. This is rejected because network
mounts, portal-backed paths, slow disks, and large journals make the worst case
visible to users.

### Use generation counters for all async results that can become stale

Replace preview generation, undo-backup persistence, draft snapshot/write
completion, Save As canonical refreshes, encoding analysis, and optional preview
or minimap scans SHALL capture a generation token. The GTK completion callback
MUST no-op if the user changes the query, closes the panel, edits the buffer,
changes the active file path, or destroys the widget before the worker result
returns.

Alternative considered: cancel every queued worker. This is useful for some
long-lived operations, but generation checks are simpler, match existing
LushText patterns, and protect correctness even when an operation cannot be
cancelled once started.

### Keep safety state immediate, make disk confirmation eventual

For Replace All undo backups, in-memory undo state and button visibility should
change immediately so the user can act without waiting for disk cleanup. Disk
save/delete completion should only reconcile persistence state and diagnostics.
For draft autosave, dirty flags should not be cleared until the snapshot has
been accepted for the matching editor generation, and write failures must leave
the editor eligible for a later autosave attempt.

Alternative considered: delay UI affordances until disk persistence completes.
This would make the app feel less responsive and does not match existing
behavior where Replace All service work already creates the durable per-file
journal before the UI receives the success result.

### Prefer test seams over production abstraction churn

Where tests need to prove a GTK callback is not blocked by slow persistence, add
narrow test hooks or injectable service delays under existing test-only feature
gates rather than replacing the filesystem boundary or adding broad traits.
Production code should stay close to current module ownership: services own
filesystem and pure data operations; widgets own GTK state and presentation.

Alternative considered: introduce a generalized background job framework. This
would be disproportionate for the known hotspots and would make future GTK
lifetime reasoning harder.

## Risks / Trade-offs

- Async save/delete reordering could leave stale Replace All backup files on
  disk -> guard each operation with a generation counter plus the existing
  disk-ordering lock, and test save-after-clear and clear-after-save ordering.
- Chunked draft snapshots could clear dirty state too early -> clear dirty flags
  only after the accepted snapshot/write path is queued for the current editor
  generation, and leave failed writes retryable.
- Worker completions could update destroyed widgets -> use weak references or
  existing `spawn_blocking_then` state handling appropriately, and no-op when
  the widget/editor has gone away.
- More async tests can become flaky under headless GTK load -> use the shared
  widget `wait_until` helpers, assert visible predicates, and keep timeouts
  generous for background-thread delivery.
- Full Markdown rendering may remain partly main-thread-bound -> this change
  bounds the immediate risk with chunked snapshots or size gating, while leaving
  a future retained render-plan rewrite possible if profiling proves it worth
  the larger architecture change.
