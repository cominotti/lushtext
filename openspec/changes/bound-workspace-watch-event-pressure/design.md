## Context

Each workspace watcher currently sends debounced result batches through an unbounded standard channel. `try_poll()` filters and sorts paths on its caller, which is GTK, and the workspace section drains the receiver until empty every 100 ms. Refresh debounce then retains all unique paths in another unbounded set. Startup/teardown generations and materialized targets are sound; the missing boundary is event ingress and per-turn work.

The watcher callback has no GTK ownership and can normalize plain paths. Correctness requires that overflow never silently loses a visible tree change: a conservative full refresh is the safe compressed representation.

## Goals / Non-Goals

**Goals:**

- Bound retained watcher changes independently of producer rate.
- Move filtering, deduplication, and overflow promotion off GTK.
- Process a fixed amount of watcher transport and refresh planning per GTK turn.
- Preserve errors/disconnect state, generation safety, overlapping target semantics, and manual recovery.
- Prove burst behavior with deterministic service tests and scale fixtures.

**Non-Goals:**

- Changing watcher target derivation, recursion policy, or backend dependency.
- Guaranteeing one targeted directory reload for every raw backend event.
- Making filesystem callbacks touch GTK models.
- Replacing manual Refresh or existing stable tree reconciliation.

## Decisions

### 1. Replace the receiver queue with a coalescing mailbox

Use an `Arc<Mutex<...>>`-style GTK-free mailbox owned by `WorkspaceWatcher` and its backend callback. The pending notice has bounded variants such as `Paths(BTreeSet<PathBuf>)` and `FullRefresh`; error/disconnect fields are bounded separately. The callback filters access/data-only noise and merges paths immediately. Once the cap is exceeded, it clears paths and promotes to `FullRefresh`, which dominates later path input until consumed.

`try_poll()` takes the single notice rather than normalizing a backend vector. Lock sections contain only bounded plain-data moves and no filesystem or GTK work.

**Alternative considered:** use `sync_channel` with a small capacity. Rejected because a full channel must either block the watcher callback or drop changes without a safe merge target.

### 2. Make merge semantics explicit and pure-testable

Define a plain merge function for `Empty`, `Paths`, and `FullRefresh`, plus bounded error/disconnect state. Duplicate paths do not consume capacity. `FullRefresh` is absorbing until take. Error repetition coalesces by typed identity or bounded summary; it never appends strings.

**Alternative considered:** store a boolean overflow flag beside an unbounded channel. Rejected because already queued batches and error payloads remain unbounded.

### 3. Consume one notice per GTK poll

The workspace-section timer performs one mailbox take, applies its bounded state, and returns. There is no drain loop. One notice can already represent every event since the previous take, so further draining provides no correctness benefit.

The refresh runtime uses the same path cap and a `pending_full_reload` dominance rule. Crossing the cap clears targeted paths. If a full reload is pending, later paths are ignored until it runs.

### 4. Keep lifecycle and event generations separate

Watcher replacement target/lifetime generations remain unchanged. A mailbox belongs to one installed watcher handle and is retired with it off-thread. Stale handles cannot merge into the current mailbox. Event coalescing does not become a second watcher lifecycle coordinator.

### 5. Treat constants as policy with instrumentation

Place the path cap and per-poll notice count in the owning service/UI policy with test seams. Benchmark normalization/merge separately from GTK planning. Automation readiness continues to observe pending refresh/lifecycle state, including a pending full-refresh marker.

## Risks / Trade-offs

- **[Risk] Full refresh can be more expensive than targeted reloads.** → Promote only after the bounded cap; schedule through the existing debounce and never perform it inside mailbox polling.
- **[Risk] Lock contention delays the backend callback.** → Keep state bounded and merge-only; benchmark producer/consumer contention without filesystem work inside the lock.
- **[Risk] Error state overwrites a distinct useful failure.** → Preserve the latest current-generation typed failure plus disconnect, and keep user-facing deduplication behavior.
- **[Risk] Events arrive while GTK owns a taken notice.** → They merge into a fresh empty mailbox and are consumed on a later poll; no event class is lost.
- **[Trade-off] Overflow loses targeted refresh precision.** → Deliberate conservative compression preserves correctness and bounds memory.

## Migration Plan

1. Add mailbox value/merge tests and backend callback normalization tests.
2. Install the mailbox behind the existing `WorkspaceWatcher::try_poll` surface.
3. Remove GTK normalization and unbounded drain behavior.
4. Bound refresh pending paths and add full-refresh dominance.
5. Run watcher lifecycle, sidebar widget, burst benchmark, runtime warning, full repository, and strict OpenSpec gates.

No persisted state changes. Rollback is local to watcher transport and refresh runtime.

## Open Questions

- Calibrate the concrete unique-path cap with burst benchmarks; correctness requires only that it is finite, shared by transport and refresh planning, and promotes to full refresh on overflow.
