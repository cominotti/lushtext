# Memory-Budget Reasoning

This reference describes what to count. It deliberately avoids fabricated per-object multipliers and hardware-specific guarantees.

## Contents

1. [Editor text](#editor-text)
2. [Indexes and GTK models](#indexes-and-gtk-models)
3. [Bounded workers and queued results](#bounded-workers-and-queued-results)
4. [Peak-memory worksheet](#peak-memory-worksheet)

## Editor text

Use `model/editor_memory.rs` as the policy source. Its estimate and budget govern editor-text residency, not total process RSS. Account separately for GTK/source-view structures, undo/history state, decoded worker results, save snapshots, preview data, indexes, and caches.

Protected user work may keep residency over the soft budget. That is intentional; never “fix” memory pressure by weakening modified/untitled/loading/saving/reloadability guards.

## Indexes and GTK models

For `FileIndex`, verify the current cap, traversal depth, shared path ownership, and update methods in `services/palette/index.rs`. For the sidebar, inspect the caller-provided scan bounds, lazy materialization, watcher scope, and reconciliation behavior in `services/file_tree.rs` and UI adapters.

Do not use a guessed byte cost for a `GObject` or path as proof. Measure representative heap/RSS where exact memory matters. Architectural findings can instead prove unbounded cardinality or duplicate ownership.

## Bounded workers and queued results

Current `gtk-lush-tasks` has a process-wide active-worker cap. When no slot is available, it places start closures in a main-thread FIFO. Releasing a slot schedules queue draining through the GLib main context. There is no periodic 50 ms retry loop.

The active slot remains held until the main-loop completion callback consumes the result, which helps bound live worker results. This does not automatically bound:

- the number or captured size of queued start closures;
- data allocated before dispatch;
- application-owned result caches;
- work launched outside `gtk-lush-tasks`;
- protected GTK editor state.

Review all of those when a burst can enqueue user-sized work.

## Peak-memory worksheet

For each workflow, list:

1. maximum simultaneous active workers;
2. maximum queued requests and captured payload per request;
3. largest worker input and result;
4. main-thread state that overlaps result delivery;
5. caches/models retained afterward;
6. cancellation and stale-result drop points;
7. soft-budget exceptions required for data safety.

Label arithmetic as an upper bound or estimate and cite the owning constants. Do not present it as measured RSS unless it was measured.
