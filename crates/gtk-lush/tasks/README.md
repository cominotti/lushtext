# gtk-lush-tasks

`gtk-lush-tasks` is a `0.0.0` GTK Lush family crate for small gtk-rs task
dispatch primitives.

## Internal Platform Status

This is a functional in-tree `0.0.0` implementation for LushText's internal
platform. It is not a stable external dependency and is not a crates.io release
candidate. The current API exists so LushText can keep task dispatch behavior
small, local, and reviewable.

Follow the current posture in `docs/next/gtk-lush.md`. Baseline adoption
evidence for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

Use this crate for bounded blocking work that must return to the GTK main loop.
Keep domain freshness explicit in the application: tab identity, file path
identity, search generations, persistence ordering, and data-loss policy remain
caller-owned rules.

The worker cap covers both running worker closures and completed results waiting
for their GTK-thread completion callback. When capacity is exhausted, work waits
in a main-thread FIFO until a completion or panic releases a slot.

## Completion Delivery

Completions run on the GTK main thread from one GLib source at
`completion_priority()`, which is `GDK_PRIORITY_REDRAW + 4`. The source runs one
completion per dispatch, in the order the workers finished.

- **Why that priority.** `GtkTextView` lays out the lines below its viewport
  from an idle at `GDK_PRIORITY_REDRAW + 5`. That idle stays ready, batch after
  batch, until the whole buffer is laid out, and it does so for a view on a
  hidden page too. A completion at `G_PRIORITY_DEFAULT_IDLE` would wait for
  every validating view's whole backlog, which takes seconds for a large
  document. One step above it, a completion runs before the next batch.
- **What it never delays.** The priority is still below the frame clock's
  paint, GTK's resize and input events, so a completion never delays a frame or
  an event.
- **Why one per dispatch.** GLib dispatches every ready source of the top
  priority in one iteration. A single delivery source keeps the main loop's
  priority check between two completions.
- **Why it yields.** The worker cap bounds how many results wait at once, not
  how long the source stays ready: a FIFO deeper than the cap, or a completion
  that spawns its successor, refills the queue as fast as it drains. So after
  eight completions in a row, or 16 ms of them, the source detaches until the
  main loop has reached `G_PRIORITY_DEFAULT_IDLE` once, then returns. When a
  source in between is ready after every dispatch (a text view validating a
  large backlog), the yield ends after 16 ms instead, so that source runs for
  at most that long before completions resume.

A completion callback therefore runs ahead of text layout and default-idle
work. Keep each one within your per-dispatch budget, and move larger work into
bounded continuations. An application may schedule its own hand-offs between
bounded stages, such as an admission drain or a permit release, at
`completion_priority()` for the same reason.

The crate does not create a runtime, message loop, actor system, or application
state framework.
