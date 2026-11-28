# Changelog

## Unreleased

- Kept non-`Send` `(state, then)` closures on the calling GTK thread in a
  thread-local slab keyed by task id instead of moving `ThreadGuard`s onto
  worker threads: worker panics now deliver cleanup back to the main loop and
  release the slot without `SIGABRT`, and off-main-thread callers fail
  explicitly on slab miss rather than stranding work in a worker TLS queue.
- Acquired `SlotGuard` before `std::thread::Builder::spawn` so spawn failures
  release slots immediately, and eliminated the fast-path `Box` allocation.
- Replaced `MainContext::default().invoke(drain_pending_tasks)` with an
  `AtomicBool`-coalesced `glib::idle_add_full(completion_priority(), ...)` wakeup
  so saturated wakeups neither invert priority behind text validation nor run
  `thread::spawn` inline inside `completion-observer` spans.
- Tracked `last_delivered_at` in `CompletionRun` so self-chaining completions
  across momentary empty queues still age after eight deliveries or 16 ms.
- Completions are delivered from one GLib source at the new
  `completion_priority()` (`GDK_PRIORITY_REDRAW + 4`), one completion per
  dispatch, instead of one `G_PRIORITY_DEFAULT_IDLE` idle each. A
  `GtkTextView` validating a large buffer no longer holds them back (GTK axiom
  ledger A80), and frames and input still dispatch first
  (`spawn-completion-priority`).
- The delivery source **ages**: after eight consecutive completions, or 16 ms
  of continuous occupancy, it detaches, lets the main loop reach
  `G_PRIORITY_DEFAULT_IDLE` once (or, when a validation backlog keeps the loop
  from getting there, waits 16 ms), and returns at `completion_priority()`. The
  worker cap alone did not bound it: a FIFO deeper than the cap or a completion
  that spawns its successor kept the source ready for the whole chain and
  starved every priority from 125 to 200.
- Added `set_completion_observer` behind the new `completion-observer`
  feature, for measurement builds: it reports each completion's span.

## 0.0.0

- First functional in-tree pre-publication implementation for bounded
  background task dispatch and explicit freshness tokens.
- Adoption-lab and matrix evidence before functional publication.
