# Proposal: Harden GTK Lush Runtime Primitives

## Why

A comprehensive audit of the four runtime leaf crates (`gtk-lush-tasks`, `gtk-lush-settle`, `gtk-lush-signals`, and `gtk-lush-viewport`) established concrete correctness defects, untested paths, and client boilerplate across LushText:

1. **`gtk-lush-tasks` (`crates/gtk-lush/tasks/src/lib.rs`)**:
   - Lines 396–407 move `glib::thread_guard::ThreadGuard` wrappers (`guarded_state`, `guarded_then`) into `std::thread::spawn`; if `work()` panics on the worker thread, unwinding drops both `ThreadGuard`s off the main thread, triggering `ThreadGuard::drop`'s cross-thread panic during unwinding and immediately aborting the process with `SIGABRT` (contradicting `lib.rs:355` and `README.md:25`).
   - `try_acquire_slot()` (`:332-342`) increments `ACTIVE_THREADS` before `std::thread::spawn`, while `SlotGuard` is only constructed inside the spawned closure (`:399`); a spawn failure (`EAGAIN`) leaks a worker slot permanently, and line 431 allocates a `Box` on the fast path even when capacity is available.
   - `release_slot_and_wake_queue()` (`:348-351`) calls `glib::MainContext::default().invoke(drain_pending_tasks)`, which schedules at `G_PRIORITY_DEFAULT_IDLE` (200) from a worker thread—inverting priority behind `GtkTextView` validation (125, ledger A80)—and executes `std::thread::spawn` inline inside `run_completion()` (`:270-272`) on the main thread, skewing `completion-observer` spans.
   - Off-main-thread calls silently push to the caller's TLS `PENDING_TASKS` when saturated (`:372`) or abort later across GLib FFI, and `COMPLETION_RUN` (`:247-250`) resets unconditionally on momentary empty queues in self-chaining workloads.
2. **`gtk-lush-settle` (`crates/gtk-lush/settle/src/lib.rs`)**:
   - `SettleBurst::schedule` (`:281-299`), `SettleBurst::clear` (`:321-325`), and `SettleHandle::schedule_follow_up` (`:364-379`) discard `glib::SourceId`, leaking GLib sources and `GWeakRef`s during layout storms (the same `Too many GWeakRef registered` failure mode fixed in `Debounce` by commit `46ffc270`) and leaving timers alive after `clear()`.
   - When `target_weak.upgrade()` returns `None` in `SettleBurst::schedule` (`:290-292`), the callback returns without calling `callback_handle.finish_if_current()`, stranding `pending() == true` permanently.
   - `Debounce::advance()` (`:122-124`) advances the generation without removing the superseded `SourceId` (leaking timers at 4 LushText call sites: `command_palette/query_execution.rs:132,239` and `window/session_restore/journal.rs:270,308`).
   - `Debounce` and `SupersedingTimer` lack `is_armed() -> bool`, forcing parallel armed-state tracking in `window/drafts/seams.rs:348-440` (consumed in `cleanup_journal.rs` and `evidence.rs`), `editor_page/local_history.rs:136-137` (`editor_page/imp.rs:206`, `local_history/evidence.rs:183`), and raw `timeout_add_local_once` + `Option<SourceId>` in `window/tab_overflow/scroll_execution.rs:341-364` (plus explanatory comment in `window/focus_mode/evidence.rs:25-30`).
   - Zero unit tests in `settle/src/lib.rs` iterate GLib's `MainContext` to actually fire scheduled timers, and `proptest!` (`:671-697`) runs unconditionally in `#[cfg(test)]` without a `property-tests` feature gate (`Makefile:164, 433-435`).
3. **`gtk-lush-signals` (`crates/gtk-lush/signals/src/lib.rs`) & `gtk-lush-viewport` (`crates/gtk-lush/viewport/src/lib.rs`)**:
   - `SignalBag::track` (`:69-76`) and `viewport`'s `SignalRegistration::new` (`:271`) do an extra GObject `clone().upcast()` ref/unref instead of `source.upcast_ref::<Object>().downgrade()`. LushText still hand-rolls single-slot `Option<SignalHandlerId>` fields (`editor_page/callbacks.rs:23`, `editor_page/language_attach.rs:99,158`, `editor_page/restyle.rs:73`, `list_retirement.rs:82`, `window/composition/execution.rs:75,77`, `window/construction.rs:237`) because `signals` lacks a zero-`Vec` single-slot `SignalSlot`.
   - In `gtk-lush-viewport`, **5 of 7** unit tests (`:408-534`) are marked `#[ignore = "requires an initialized GTK display"]` and never run in CI (`cargo test` skips them and no lane passes `--ignored`); `page_size = NaN` poisons the bounds filter (`:193-198`); `len()` doc (`:240`) claims "live" registrations while always returning `4`; `:189-191` allocates an unnecessary `Rc` around `Cell<f64>`; and `for_scrollable` does not document its post-parenting precondition.

## Demand Signal

1. **Current LushText pain & correctness risk**: worker panics in `gtk-lush-tasks` abort the application instead of releasing slots cleanly; `SettleBurst` and `Debounce::advance()` leak GLib sources/weak refs in production workflows (`minimap/reflow_execution.rs`, `command_palette/query_execution.rs`, `session_restore/journal.rs`); missing `is_armed()` and `SignalSlot` force manual parallel state in LushText UI workflows.
2. **Verification & check drift**: 5 of 7 `gtk-lush-viewport` tests are unreachable `#[ignore]` stubs, `gtk-lush-settle` never tests timer firing on `MainContext`, and `gtk-lush-settle` runs `proptest!` outside `make test-prop` (`property-based-testing` boundary drift).

## What Changes

- **`gtk-lush-tasks`**:
  - Keep non-`Send` `(state, then)` closures off the worker thread using a main-thread `thread_local!` slab keyed by `u64` id plus a worker `Drop` guard that delivers main-thread slab cleanup on panic while holding `SlotGuard` until the main loop releases/consumes the entry (eliminating worker-panic `SIGABRT` and double slot release).
  - Allocate the main-thread slab entry eagerly in `spawn_blocking_then` *before* `start_or_queue` (so even saturated off-main-thread calls trigger an explicit main-thread slab-miss panic rather than silently stranding in TLS — without relying on `MainContext::default().is_owner()`).
  - Return `Option<SlotGuard>` from `try_acquire_slot()` before `std::thread::spawn` (fixing slot leaks on spawn failure and removing fast-path `Box::new`).
  - Replace `MainContext::default().invoke(drain_pending_tasks)` with `AtomicBool`-coalesced `glib::idle_add_full(completion_priority(), ...)` (fixing worker-wakeup priority inversion behind A80 and removing `thread::spawn` syscalls from `completion-observer` spans).
  - Fix `COMPLETION_RUN` aging reset on momentary empty queues by tracking `last_delivered_at: Option<Instant>` and resetting `delivered` only after an idle gap (zero extra GLib sources).
  - Add failing-first tests for panicking worker closures, real `> 8` task saturation wakeups, and `completion-observer` span isolation.
- **`gtk-lush-settle`**:
  - Consolidate `SettleBurst` into a single `Rc<SettleBurstState>` tracking `Vec<(u64, glib::SourceId)>`, eagerly removing all armed primary and follow-up sources on `begin()`/`clear()` with `SupersedingTimer`-style re-entrancy protection, and calling `callback_handle.finish_if_current()` when `target_weak.upgrade()` fails.
  - Delegate `Debounce::advance()` to `self.invalidate()` so superseded GLib sources are removed immediately without breaking callers, and deduplicate the shared `SupersedingTimerState::schedule` helper between `Debounce::schedule` and `SupersedingTimer::arm`.
  - Add `is_armed(&self) -> bool` on `Debounce` and `SupersedingTimer`, and retire parallel armed-state tracking / raw one-shot timers in LushText (`window/drafts/{seams.rs,cleanup_journal.rs,evidence.rs}`, `editor_page/{local_history.rs,imp.rs}`, `window/local_history/evidence.rs`, `window/tab_overflow/scroll_execution.rs`, and update `window/focus_mode/evidence.rs` comment).
  - Gate `proptest!` behind `[features] property-tests = []` in `crates/gtk-lush/settle/Cargo.toml` (keeping `proptest!` in `src/lib.rs` to access private `GenerationGate` or placing pure `GenerationGate` tests behind `--lib --features property-tests` wired into `Makefile` `test-prop` / `test-prop-deep`).
  - Add failing-first unit tests pumping GLib's `MainContext` to execute `Debounce`, `SupersedingTimer`, `SettleBurst`, `SettleHandle::schedule_follow_up`, dead-target `pending()` reset, `Debounce::advance()` source removal, and `is_armed()`.
- **`gtk-lush-signals`**:
  - Add `SignalSlot` (zero-`Vec` single replaceable `Option<SignalRegistration>` with `replace`, `clear`, `is_connected`, and `Drop`) and migrate at least 5 manual single-slot `Option<SignalHandlerId>` sites in `crates/lushtext-ui/src/` (`editor_page/callbacks.rs`, `editor_page/language_attach.rs`, `editor_page/restyle.rs`, `list_retirement.rs`, `window/composition/execution.rs`, `window/construction.rs`).
  - Switch `SignalBag::track` to `source.upcast_ref::<Object>().downgrade()` (removing extra GObject ref/unref and `+ Clone`), derive/implement `Debug`, document cleanup ordering and disposed-but-referenced source hazards, and add tests for re-entrant `clear()`/`track()` and `BindingBag::drop`/`RegistrationBag::drop`.
- **`gtk-lush-viewport`**:
  - Guard non-finite/`NaN` `page_size` in the bounds filter, fix `len()` documentation, remove the unnecessary `Rc` around `Cell<f64>`, switch `SignalRegistration::new` to `upcast_ref::<Object>().downgrade()`, add `Debug`, and document `for_scrollable`'s post-parenting precondition.
  - Migrate the 5 `#[ignore]` tests from `crates/gtk-lush/viewport/src/lib.rs:408-534` into headless widget tests in `crates/lushtext/tests/widget/gtk_lush_adoption.rs` and delete the dead `#[ignore]` stubs from `src/`.

## Capabilities

### New Capabilities

### Modified Capabilities
- `gtk-lush-tasks`: Update worker panic safety, slot acquisition, main-loop wakeup priority, main-thread invocation checks, and aging reset behavior.
- `gtk-lush-settle`: Add eager `SourceId` cleanup and dead-target completion on `SettleBurst`, source removal on `Debounce::advance()`, `is_armed()` queries, and `property-tests` feature gating.
- `gtk-lush-signals`: Add `SignalSlot`, optimize weak registration tracking without `+ Clone`/ref-unref, and document cleanup ordering.
- `gtk-lush-viewport`: Harden `NaN` filtering, clarify `len()` and `for_scrollable` contracts, and move display-dependent tests to the headless widget lane.
- `property-based-testing`: Include `gtk-lush-settle` behind the explicit `property-tests` feature gate in `make test-prop`.

## Impact

- **Code**: `crates/gtk-lush/{tasks,settle,signals,viewport}/src/lib.rs`, `crates/gtk-lush/settle/Cargo.toml`, `Makefile` (`test-prop` / `test-prop-deep`), `crates/lushtext/tests/widget/gtk_lush_adoption.rs`, and targeted call-site cleanup in `crates/lushtext-ui/src/ui/` (`window/drafts/*`, `editor_page/*`, `window/local_history/evidence.rs`, `window/focus_mode/evidence.rs`, `window/tab_overflow/scroll_execution.rs`, `list_retirement.rs`, `window/composition/execution.rs`, `window/construction.rs`).
- **Evidence & Docs**: Update `README.md`, `CHANGELOG.md`, `docs/gtk-lush-adoption/{matrix.toml,api-review.md}`, `docs/next/gtk-lush.md`, `docs/thresholds.md` (if any new integer constant is added), and `.agents/rules/{widget-wiring.md,rust.md}`.

## User-Visible Impact

None in normal UI flows when timers and tasks behave normally; prevents silent editor aborts (`SIGABRT`) on background worker panics and prevents subtle timer/weak-ref accumulation in long-running sessions. No features or behaviors present on `main` are removed.

## Non-goals

- Dropping the `state: S` parameter from `spawn_blocking_then` (churn across 88+ call sites for purely aesthetic gain).
- Replacing `std::thread::spawn` in `gtk-lush-tasks` with a persistent worker thread pool (no profiling evidence of spawn overhead; adds permanent thread stacks and complicates `active_worker_count()`).
- Calling `glib::MainContext::default().is_owner()` or process-global `OnceLock` `MAIN_THREAD_ID` checks in `gtk-lush-tasks` (`is_owner()` returns `false` outside active `MainLoop`, and `OnceLock` thread-ID fails multi-threaded `cargo test`).
- Merging `Debounce` and `SupersedingTimer` into a single type or changing `invalidate()`'s return type from `TimerToken` to `bool`.
- Removing `RegistrationBag` or `FreshnessToken`/`Fresh`/`Stale` (ratified zero-dep types with zero bugs; deletion yields spec/doc churn with zero benefit).
- Adding `SignalMap<K>` to `gtk-lush-signals` (callers can compose `HashMap<K, SignalSlot>` without drifting into a subscription framework).
- Adding `notify::hadjustment`/`vadjustment` rebinding machinery to `gtk-lush-viewport` or reshaping it for single-axis/unfiltered `changed` (neither LushText consumer swaps adjustments at runtime; no demand signal).
- Extracting `signal_connect.rs`, `row_text/`, or `entry_paste_cap.rs` from `crates/lushtext-ui/src/ui/`.

## Coordination & Ordering

- **Wave 1 (independent)** in the 7-change GTK Lush portfolio; implemented in parallel with `harden-gtk-lush-widgets-geometry` and `harden-gtk-lush-proof-toolchain`, merging **first** into `origin/main` before `#2` and `#3`.
- Coordinates on `Makefile` (`test-prop` / `CARGO_TEST_PROP`), `crates/lushtext/tests/widget/gtk_lush_adoption.rs`, `openspec/specs/property-based-testing/spec.md`, and `docs/gtk-lush-adoption/matrix.toml` with `harden-gtk-lush-widgets-geometry` (`#2`) and `optimize-gtk-lush-gates-and-ci` (`#5`).
