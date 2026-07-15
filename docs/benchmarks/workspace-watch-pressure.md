# Workspace Watch Event Pressure

Workspace auto-refresh keeps watcher lifecycle work and event pressure bounded independently. The lifecycle path remains generation-safe and off the GTK thread; one installed watcher handle owns one GTK-free mailbox shared only with its backend callback.

## Baseline ownership

Before this change, notify's debounced callback sent complete result batches through an unbounded standard channel. `WorkspaceWatcher::try_poll()` filtered tree-changing events and sorted/deduplicated paths on its caller, while the workspace-section 100 ms timer drained the receiver until empty. The later 120 ms refresh debounce accumulated another unbounded path set. Backend errors occupied separate channel entries, disconnect was inferred from the receiver, and target/lifetime generations guarded handle installation but did not bound event ingress.

The bounded contract assigns ownership as follows:

- notify callback: filter access/data-only noise, extract create/remove/rename paths, deduplicate, and merge;
- per-handle mailbox: retain `Paths` or dominant `FullRefresh`, the latest byte-bounded diagnostic, and disconnect state;
- GTK poll: take zero or one already-normalized notice and return;
- refresh debounce: retain at most the shared path cap or one dominant full refresh;
- lifecycle worker: create, replace, and retire handles off GTK while target and lifetime generations reject stale completions.

The mailbox lock contains only bounded plain-data merging or one bounded state
move. Each raw event is normalized before the callback attempts the lock, so
temporary path deduplication is limited to that event. Producers never wait: a
busy mailbox latches a constant-space full refresh, and a concurrent error also
latches bounded generic diagnostic evidence when exact detail cannot be stored.
GTK polling and readiness use a non-blocking lock attempt; a busy producer
defers notice delivery to the next poll and reports readiness as busy.
Notice-vector allocation happens after pending state has moved out of the lock.
The mailbox performs no filesystem calls and never touches GTK.

## Shared cap and fallback

`WORKSPACE_WATCH_PATH_CAP` is 1,024 unique paths in both transport and refresh planning. Duplicates do not consume capacity. The 1,025th unique path clears the targeted set and promotes it to `FullRefresh`; later paths cannot grow retained state until GTK takes that marker. The refresh planner applies the same dominance rule until the scheduled stable reconciliation runs.

The cap keeps ordinary editor save, build-output, and rename bursts precise while bounding one GTK poll well below the separate 10,000-entry directory rendering ceiling. Refresh planning builds one expanded-store index per pass and minimizes sorted directory prefixes linearly, avoiding a model scan per changed path and quadratic prefix comparisons at the exact cap. Full refresh is deliberately conservative: it trades target precision for correctness and fixed retained memory rather than dropping a visible tree change.

## Deterministic coverage

Service tests cover empty/path/full states, exact-cap retention, raw-event storms,
ambiguous rename promotion, duplicates, error/disconnect overlap, UTF-8-safe
diagnostic truncation, access-noise filtering, events arriving after take,
non-blocking producer contention, producer/consumer races, and stale-handle
isolation. Pure refresh tests exercise prefix minimization at the shared cap.
Widget tests pause the timer through a narrow test seam and record mailbox path
count/full state, refresh-plan path count/full state, and notices consumed by
exactly one poll. The seam exposes scalar pressure only; automation snapshots
never expose retained filesystem paths.

Run the focused coverage with:

```sh
cargo test -p lushtext-core workspace_watch --lib
scripts/run-widget-tests.sh --headless -- workspace_watch_
cargo bench -p lushtext-core --bench benchmarks workspace_watch_pressure
```

## Calibration snapshot

A local optimized sample on 2026-07-13 used 512 unique deeply nested Unicode paths, one duplicate create plus one access-only event per path, 32 producer batches of 64 unique paths, and both raw and normalized cap-plus-one promotion fixtures. Criterion recorded:

- normalization, filtering, and merge of 1,536 individually delivered awkward
  raw events: about 569-615 microseconds;
- bounded raw normalization and promotion across 3,075 individually delivered
  cap-plus-one events: about 1.47-1.56 milliseconds;
- polling every producer batch: about 367-384 microseconds for all 32 batches;
- polling every four batches: about 615-661 microseconds;
- allowing all 32 batches to outrun consumption and promote: about 394-408 microseconds;
- normalized cap-plus-one full-refresh promotion: about 653-682 microseconds.

These figures calibrate policy; they are not portable performance thresholds. The deterministic guarantees are the 1,024-path retained bound, one notice per GTK poll, constant-count diagnostic/disconnect state, and full-refresh promotion whenever precision would exceed the cap.
