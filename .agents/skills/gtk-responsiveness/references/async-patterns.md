# Async Patterns for GTK4 and LushText

Use current GTK Lush and application code as authority. These patterns describe ownership and correctness; callers still own workflow-specific freshness, ordering, cancellation, and data-safety policy.

## Contents

1. [Worker result returned to GTK](#worker-result-returned-to-gtk)
2. [Lifetime, cancellation, freshness, and ordering](#lifetime-cancellation-freshness-and-ordering)
3. [Persistence and cleanup](#persistence-and-cleanup)
4. [Timers and settling](#timers-and-settling)
5. [Bursts and bounded dispatch](#bursts-and-bounded-dispatch)
6. [Thread-safety reminders](#thread-safety-reminders)

## Worker result returned to GTK

Use `gtk_lush_tasks::spawn_blocking_then` when blocking/CPU work produces a result for GTK state:

```rust
gtk_lush_tasks::spawn_blocking_then(
    target.clone(),
    move || services::filesystem::read::bytes(&path),
    move |target, result| {
        // Main thread: revalidate workflow state, then update GTK.
        target.apply_if_current(request, result);
    },
);
```

Use `spawn_blocking_then_weak` when ordinary GObject lifetime is the only target check. A live target can still represent a different tab, query, or generation, so weak lifetime never replaces freshness.

Document decoding should call the encoding-aware `services::editor_io` workflow. Its `simdutf8` branch safely converts validated UTF-8 and preserves BOM/legacy-encoding fallbacks. Do not copy an unsafe UTF-8-only example into a UI adapter.

Keep the completion callback bounded. Applying a huge unbounded result, appending rows one by one, or parsing again on the main thread can still freeze GTK.

## Lifetime, cancellation, freshness, and ordering

These are separate contracts:

- **Lifetime:** Is the target object still alive?
- **Cancellation:** Can obsolete work stop early and release resources?
- **Freshness:** Does the result still belong to current state?
- **Ordering:** Can an older accepted result overwrite newer state?
- **Durability:** For persistence, what reached stable storage and how is uncertainty surfaced?

Use `FreshnessToken` where a generation is the right abstraction, or preserve a workflow-owned typed identity. Check cancellation before and after expensive stages. Revalidate identity and policy immediately before mutation; a check made only before spawning is stale by definition.

## Persistence and cleanup

Do not use detached `std::thread::spawn` as a generic “fire-and-forget write” pattern. Autosave, session state, drafts, committed-state cleanup, and best-effort maintenance still need explicit ownership of:

- failure reporting or durable retry evidence;
- generation/order so old snapshots cannot win;
- shutdown/close behavior;
- stable target and filesystem-boundary rules;
- data-safety revalidation before destructive cleanup.

Use the application workflow's ordered persistence mechanism and `spawn_blocking_then` where it returns acceptance/failure to the owner. A deliberately detached non-mutating diagnostic may be acceptable only when its loss has no correctness or user-visible consequence.

The only detached mutating exception is bounded deletion of a uniquely owned, uncommitted temporary artifact. Use it only when every condition holds:

- the artifact was created by the same in-flight workflow and has never represented committed state or user data;
- its stable identity cannot be reused for another workflow before deletion finishes;
- deletion cannot race with or mutate a manifest, session, draft, sidecar, history, backup, or other accepted state;
- the request contains one artifact or an explicit small cap, not an unbounded directory walk;
- failure only leaves an inert temporary artifact, is logged, and cannot make durable state claim the artifact was removed;
- the deletion still goes through the repository filesystem boundary.

If any condition is uncertain, return completion to the owning workflow and apply its freshness, ordering, retry, and data-safety rules. Never generalize this exception to persistence, committed-state cleanup, or deletion selected from mutable manifest/user state.

## Timers and settling

Prefer the fitting `gtk_lush_settle` type:

- `Debounce` for work after input settles;
- `SupersedingTimer` for restartable one-shots;
- `SettleBurst` for readiness-linked bursts.

The duration belongs to the workflow. Empty-query behavior and accessibility feedback may require an immediate path.

If raw GLib timers are necessary, store and remove the prior `SourceId`, clear it after firing, avoid strong widget retention, and stop repeating timers when their target disappears. Timer callbacks run on the main thread and must remain bounded.

## Bursts and bounded dispatch

`gtk_lush_tasks` currently caps active workers process-wide. When all slots are occupied, pending start closures enter a main-thread FIFO. Slot release invokes queue draining through GLib; it does not poll with a timeout. The slot stays held until the main-loop callback consumes the result.

This bounds active worker/result handoffs, not all memory. Avoid capturing user-sized buffers before queuing, bound the number of logical requests, coalesce superseded work, and cancel/drop stale results. Code outside this dispatcher needs its own bound.

## Thread-safety reminders

- GTK widgets and `gio::ListStore` stay on the GTK thread.
- Move plain owned `Send` inputs/results across the worker boundary.
- Never touch a `ThreadGuard` payload on the worker.
- Use local signal/timer APIs when closures capture non-`Send` GTK state.
- Do not state that a GLib/GObject type is `Send` or `Sync` without verifying its current bindings and the actual wrapper used.
