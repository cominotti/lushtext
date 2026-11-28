# Design: Harden GTK Lush Runtime Primitives

## Context

`gtk-lush-tasks`, `gtk-lush-settle`, `gtk-lush-signals`, and `gtk-lush-viewport` are stable in-tree `0.0.0` leaf crates consumed across `crates/lushtext-ui/src/`. Direct source inspection and adversarial review confirmed runtime panics/leaks (`ThreadGuard` off-thread drop on worker panic, missing `SourceId` tracking in `SettleBurst` and `Debounce::advance()`, `MainContext::invoke` priority inversion), dead `#[ignore]` tests in `viewport`, and un-gated `proptest!` runs in `settle`.

## Decisions

### 1. Keep non-`Send` state off worker threads via main-thread slab in `gtk-lush-tasks`
- **Decision**: Replace `ThreadGuard::new(state)` / `ThreadGuard::new(then)` moved into `std::thread::spawn` with a main-thread `thread_local!` slab (`SlotEntry { id: u64, state_and_then }`) indexed by a `u64` token allocated eagerly inside `spawn_blocking_then` *before* `start_or_queue`. Move `SlotGuard` out of `try_acquire_slot() -> Option<SlotGuard>` *before* `std::thread::spawn`, and attach a worker `Drop` guard (`WorkCleanup`) that delivers `(id, slot_guard, Option<T>)` back to the main-thread delivery queue whether `work()` returns normally or unwinds from a panic. Register any new non-test integer constant in `docs/thresholds.md`.
- **Rationale**: Under `#![forbid(unsafe_code)]`, `ThreadGuard::drop` panics if dropped on the worker thread during unwinding (`glib-0.22.11/src/thread_guard.rs:110-116`), turning any worker panic into `SIGABRT`. Allocating the slab entry eagerly in `spawn_blocking_then` guarantees both normal main-thread drops AND saturated off-main-thread slab-miss detection without touching worker threads or panicking multi-threaded `cargo test` runs.
- **Rejected alternatives**:
  - *Calling `glib::MainContext::default().is_owner()` or pinning a global `OnceLock` `MAIN_THREAD_ID`*: **rejected** because `is_owner()` returns `false` outside an actively running `MainLoop` and Rust's test harness runs `#[test]` functions across multiple OS threads under `ACTIVE_THREADS_TEST_LOCK`.
  - *Dropping `state: S` from `spawn_blocking_then`*: **rejected** as high-churn across 88+ LushText call sites with zero correctness gain.
  - *Persistent worker thread pool*: **deferred**; spawning `<= 8` threads takes ~20 µs outside `completion-observer` spans once `wake_queue` moves off inline `invoke`.

### 2. Wake queued tasks at `completion_priority()` and fix `COMPLETION_RUN` aging
- **Decision**: Replace `glib::MainContext::default().invoke(drain_pending_tasks)` in `release_slot_and_wake_queue` with an `AtomicBool`-coalesced `glib::idle_add_full(completion_priority(), ...)` wakeup. Track `last_delivered_at: Option<Instant>` in `COMPLETION_RUN` so momentary queue-empty gaps in self-chaining workloads do not reset `delivered` without an actual idle gap.
- **Rationale**: `MainContext::invoke` dispatches at `G_PRIORITY_DEFAULT_IDLE` (200) from worker threads (starving behind `GtkTextView` validation at 125, ledger A80) and executes `drain_pending_tasks` synchronously inline inside `run_completion()` on the main thread (polluting `completion-observer` timings).
- **Rejected alternatives**:
  - *Attaching an extra yield-detection idle source on every completion*: **rejected** to keep zero-source overhead on the hot path.

### 3. Eager `SourceId` tracking in `SettleBurst`, `Debounce::advance()`, `is_armed()`, and `--lib` `property-tests`
- **Decision**: Unify `SettleBurst` into one `Rc<SettleBurstState>` storing `gate`, `pending`, and `sources: RefCell<Vec<(u64, glib::SourceId)>>` so `begin()`, `clear()`, and `schedule_follow_up()` cancel every armed GLib source eagerly with `SupersedingTimer`'s advance-before-remove re-entrancy order. In `SettleBurst::schedule`, call `callback_handle.finish_if_current()` on `target_weak.upgrade() == None`. Make `Debounce::advance()` call `self.invalidate()`, deduplicate `schedule`/`arm` via a shared private method on `SupersedingTimerState`, and add `is_armed(&self) -> bool` on both types. Gate `proptest!` in `settle/src/lib.rs` behind `[features] property-tests = []` (`#[cfg(all(test, feature = "property-tests"))]`) and invoke `cargo nextest run -p gtk-lush-settle --features property-tests --lib --profile property` inside `make test-prop` / `make test-prop-deep` (since `GenerationGate` is crate-private and Cargo forbids optional dev-dependencies).
- **Rationale**: Prevents `Too many GWeakRef registered` storms and stuck `pending() == true` blockers while keeping `GenerationGate` private and keeping `proptest` in `[dev-dependencies]`.
- **Rejected alternatives**:
  - *`property-tests = ["dep:proptest"]`*: **rejected** (invalid Cargo syntax for `[dev-dependencies]`).
  - *Appending `-p gtk-lush-settle` to `--test properties` without a separate `--lib` command*: **rejected** because `proptest!` tests private `GenerationGate` in `src/lib.rs`.
  - *Merging `Debounce` and `SupersedingTimer` or returning `bool` from `invalidate()`*: **rejected** due to unnecessary churn across 22+ LushText files.

### 4. `SignalSlot`, `upcast_ref().downgrade()`, and headless migration of `viewport` tests
- **Decision**: Add `SignalSlot` to `gtk-lush-signals` (`replace`, `clear`, `is_connected`, `Drop`) using the same weak-source `SignalRegistration` without `Vec`. Replace `source.clone().upcast()` in `signals` and `viewport` with `source.upcast_ref::<Object>().downgrade()`, remove the unnecessary `Rc` in `viewport/src/lib.rs:189-191`, and guard `!page_size.is_finite()` before updating `observed_size`. Move the 5 `#[ignore]` tests in `viewport/src/lib.rs` into `crates/lushtext/tests/widget/gtk_lush_adoption.rs` and delete the `#[ignore]` stubs from `src/`.
- **Rationale**: `SignalSlot` directly removes 6 manual `RefCell<Option<SignalHandlerId>>` sites in `lushtext-ui` while staying a 50-line zero-framework primitive. Migrating the `#[ignore]` tests to the widget suite executes them in CI without breaking displayless `cargo nextest run --workspace`.
- **Rejected alternatives**:
  - *Adding `SignalMap<K>` or `notify::hadjustment`/`vadjustment` rebinding in `viewport`*: **rejected** as overreach without LushText pain signals.
  - *Removing `#[ignore]` inside `viewport/src/lib.rs`*: **rejected** because `Non-widget Tests` has no display server and would fail.

## Budget Exceptions & Ledger Axioms

- **Budget exceptions**: none.
- **Ledger axioms**: relies on **A80** (`completion_priority()` `GDK_PRIORITY_REDRAW + 4` outranking `GtkTextView` incremental validation at 125) and **A81** (GLib equal-priority dispatch ordering). No new toolkit behavior introduced.

## Timing

Timing: Verdict class: non-inferiority on `gtk-lush-tasks` completion delivery path (`B_SETTLE_US`, `B_INTERACT_US`, and `completion-observer` span isolation) per `.agents/rules/verification-runs.md`.
- **Protocol**: Run 4 interleaved `main` vs. branch rounds (`main-1`, `branch-1`, `main-2`, `branch-2`) via `scripts/measure.sh` + `scripts/quiet-gate.py` on `responsiveness-smoke-driver.py` `--tier full` cases `baseline/calibration`, `restyle-populated/search-results`, `total-size/1000000`, `total-size/1000000/txt`, and `replace-all/skips-16384` (`RESPONSIVENESS_VARIANTS=plain`, `RESPONSIVENESS_REPETITIONS=2`), plus `cargo test -p gtk-lush-tasks --features completion-observer`.
- **Pre-registered pass bar**: Branch medians for `post_open.settle_us`, worst dispatch (`post_open[].dispatches[].duration_us`), `task_completions.completion_max_us`, and `task_completions.handoff_max_us` are at most `main` median + noise band (`+500 us` / `+10%`, matching `main`-vs-`main` `completion_max_us` spread of `~2.5 ms` well below `B_SETTLE_US = 16_000 us`), `task_completions.over_b_settle` stays empty (`every hand-off dispatch within B_SETTLE_US`), no case within `B_INTERACT_US` / `B_SETTLE_US` on `main` crosses budget on branch, and `completion-observer` spans contain zero queued-task `std::thread::spawn` overhead.

## Governance & Constitution Checklist

- **No ownership of GTK control flow**: All primitives use GLib main-loop sources and GObject weak refs; no custom event loop or executor.
- **No view DSL / no state or message system**: Zero macros, zero view DSL, zero actor/message abstractions; `SignalSlot` and `is_armed()` remain passive RAII/query helpers.
- **Leaf crates only**: Zero dependencies on LushText crates or sibling `gtk-lush-*` crates.
- **Adwaita authoritative**: Zero adaptive container changes.
- **Proofs**: Unit tests with real `MainContext` pumping, failing-first worker-panic/saturation tests, gated `proptest`, and headless widget tests replacing `#[ignore]` stubs.

## Risks / Trade-offs

- **Risk**: `SettleBurst::clear()` synchronously removing `SourceId`s runs closure destructors immediately, which could re-enter `SettleBurst`. **Mitigation**: Mirror `SupersedingTimer::arm`'s order—invalidate generation and clear `pending`/drain `sources` *before* calling `source_id.remove()`.
- **Risk**: Editing `crates/lushtext-ui/src/ui/window/composition/execution.rs` and `editor_page/*` triggers visual-proof / UI smoke freshness gates. **Mitigation**: Batch all UI call-site edits, run Review-Before-Smoke, and finish with a single `make smoke-ui` + `make verify-local` pass.
