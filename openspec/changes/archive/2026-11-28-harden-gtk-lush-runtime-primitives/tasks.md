# Tasks: Harden GTK Lush Runtime Primitives

- **Kani**: none (no proven module touched)
- **Timing**: Verdict class: non-inferiority on `gtk-lush-tasks` completion delivery path (`responsiveness-smoke` cases `baseline/calibration`, `restyle-populated/search-results`, `total-size/1000000`, `total-size/1000000/txt`, `replace-all/skips-16384`, and `completion-observer` span isolation per `.agents/rules/verification-runs.md`)

## 1. Harden `gtk-lush-tasks` (Panic Safety, Slots, Priority, Aging)

- [x] 1.1 Write failing-first unit tests in `crates/gtk-lush/tasks/src/lib.rs` for: (a) a real panicking `work` closure inside `spawn_blocking_then` (currently aborts with `SIGABRT`), (b) real `> 8` task saturation automatic FIFO drain without manually mutating `ACTIVE_THREADS`, (c) `completion-observer` span not enclosing queued-task `thread::spawn` calls, (d) self-chaining completions aging across momentary empty queues, and (e) off-main-thread `spawn_blocking_then` (both unsaturated and saturated) failing cleanly on slab miss. Verify tests fail against `main`.
- [x] 1.2 Implement main-thread `thread_local!` slab allocated eagerly in `spawn_blocking_then` for `(state, then)`, `try_acquire_slot() -> Option<SlotGuard>` before `std::thread::spawn`, worker `Drop` panic cleanup delivery holding `SlotGuard`, `AtomicBool`-coalesced `idle_add_full(completion_priority(), ...)` wakeup, and `last_delivered_at` gap-based aging reset in `crates/gtk-lush/tasks/src/lib.rs` (updating `docs/thresholds.md` if any new integer constant is added). Verify `cargo test -p gtk-lush-tasks` and `--features completion-observer` pass.
- [x] 1.3 Update `crates/gtk-lush/tasks/{README.md,CHANGELOG.md}` and run `cargo test -p gtk-lush-tasks --doc` and `cargo test -p gtk-lush-tasks --examples` to verify docs and examples pass.

## 2. Harden `gtk-lush-settle` & Feature-Gate Property Tests

- [x] 2.1 Write failing-first unit tests in `crates/gtk-lush/settle/src/lib.rs` pumping `glib::MainContext` to verify: `Debounce::schedule` firing and eager replacement, `Debounce::advance()` removing pending GLib sources, `SupersedingTimer::arm`, `SettleBurst::schedule` and `SettleHandle::schedule_follow_up` eager source removal on `begin()`/`clear()`, dead-target clearing `SettleBurst::pending()`, and `is_armed()`. Verify they fail on `main`.
- [x] 2.2 Consolidate `SettleBurstState` with `Vec<(u64, glib::SourceId)>` tracking and dead-target `finish_if_current()`, delegate `Debounce::advance()` to `invalidate()`, deduplicate `SupersedingTimerState::schedule`, and add `is_armed(&self) -> bool` on `Debounce` and `SupersedingTimer`. Verify `cargo test -p gtk-lush-settle` passes.
- [x] 2.3 Gate `proptest!` in `crates/gtk-lush/settle/src/lib.rs` behind `[features] property-tests = []` in `crates/gtk-lush/settle/Cargo.toml` (`#[cfg(all(test, feature = "property-tests"))]`) and wire `-p gtk-lush-settle --features property-tests --lib --profile property` into `make test-prop` / `make test-prop-deep` in `Makefile`. Verify `make test-prop` runs `gtk-lush-settle` property tests while `make test-unit` excludes them.
- [x] 2.4 Migrate LushText's parallel armed-state trackers and raw one-shot timer onto `is_armed()` / `SupersedingTimer` in `crates/lushtext-ui/src/ui/window/drafts/{seams.rs,cleanup_journal.rs,evidence.rs}`, `editor_page/{local_history.rs,imp.rs}`, `window/local_history/evidence.rs`, `window/tab_overflow/scroll_execution.rs`, and update `window/focus_mode/evidence.rs` comment. Verify affected unit/widget tests pass.
- [x] 2.5 Update `crates/gtk-lush/settle/{README.md,CHANGELOG.md}` and `docs/gtk-lush-adoption/{matrix.toml,api-review.md}`, and run doctests/examples.

## 3. Extend & Optimize `gtk-lush-signals`

- [x] 3.1 Write failing-first unit tests in `crates/gtk-lush/signals/src/lib.rs` for `SignalSlot` (`replace`, `clear`, `is_connected`, `Drop`, dead source), `Debug` formatting, re-entrant `clear()`/`track()` during disconnect, `BindingBag::drop`, and `RegistrationBag::drop`. Verify failures before implementation.
- [x] 3.2 Implement `SignalSlot`, `Debug` impls, and `source.upcast_ref::<Object>().downgrade()` in `SignalBag::track` (removing `+ Clone`), and document cleanup ordering and disposed-source hazards in `crates/gtk-lush/signals/src/lib.rs`. Verify `cargo test -p gtk-lush-signals` passes.
- [x] 3.3 Migrate at least five manual single-slot `Option<SignalHandlerId>` sites in `crates/lushtext-ui/src/ui/` (`editor_page/callbacks.rs:23`, `editor_page/language_attach.rs:99,158`, `editor_page/restyle.rs:73`, `list_retirement.rs:82`, `window/composition/execution.rs:75,77`, `window/construction.rs:237`) onto `SignalSlot`. Verify `make test-unit` and focused widget tests pass.
- [x] 3.4 Update `crates/gtk-lush/signals/{README.md,CHANGELOG.md}`, `docs/gtk-lush-adoption/{matrix.toml,api-review.md}`, `docs/next/gtk-lush.md`, and `.agents/rules/{widget-wiring.md,rust.md}` for `SignalSlot` and `is_armed()`.

## 4. Harden `gtk-lush-viewport` & Rescue Ignored Tests

- [x] 4.1 Migrate the 5 `#[ignore]` observer bounds/value/drop tests from `crates/gtk-lush/viewport/src/lib.rs:408-534` into `crates/lushtext/tests/widget/gtk_lush_adoption.rs`, and add failing-first non-finite (`NaN`/`±Inf`/`-0.0`) unit tests on `initial_observed_page_size` and `accept_page_size_change` in `crates/gtk-lush/viewport/src/lib.rs` (since `gtk4::Adjustment::set_page_size(NaN)` rejects at GTK's C precondition with `Gtk-CRITICAL` before emitting `"changed"`).
- [x] 4.2 Guard `!page_size.is_finite()` in `crates/gtk-lush/viewport/src/lib.rs`, remove the extra `Rc` around `Cell<f64>`, switch `SignalRegistration::new` to `upcast_ref().downgrade()`, add `Debug`, fix `len()` doc, document `for_scrollable`'s post-parenting precondition, and delete the `#[ignore]` stubs from `src/`. Update `crates/gtk-lush/viewport/{README.md,CHANGELOG.md}`. Verify `cargo test -p gtk-lush-viewport` and `cargo test -p lushtext --test widget gtk_lush_adoption` pass cleanly.

## 5. Review-Before-Smoke, Archive, Final Verification & Commit

- [x] 5.1 Run 3-lens code review (correctness/responsiveness/data-safety, architecture/comments, tests/policy) on all crate and `lushtext-ui` edits and fix any findings before running UI smokes.
- [x] 5.2 Archive-Before-Final-Gate: sync delta specs into `openspec/specs/` and archive `harden-gtk-lush-runtime-primitives` on the branch.
- [ ] 5.3 Run `scripts/measure.sh` + `scripts/quiet-gate.py` non-inferiority timing verification for `gtk-lush-tasks`, then run `scripts/heavy.sh --envelope 16G -- make verify-local GATE_TREES_CHANGE=harden-gtk-lush-runtime-primitives` (including `make smoke-ui` for `ui/` edits) and `make test-prop`.
- [ ] 5.4 Sign commits with `git commit -S -F <msgfile>` and push once `.githooks/pre-push` (`make check-smoke-proof`) passes.
