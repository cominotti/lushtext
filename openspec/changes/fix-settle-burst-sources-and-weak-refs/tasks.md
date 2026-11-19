# Tasks

Tests: the burst test and the clear test come first and fail on `main`.
Kani: none (no proven module touched).
Timing: none (no timed path touched).
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first.

## 0. Re-measure

- [ ] 0.1 Re-read `settle/src/lib.rs` and the `SettleBurst` call sites on the then-current `origin/main`, and update `proposal.md`. Verify with `openspec validate fix-settle-burst-sources-and-weak-refs --strict`.

## 1. One pending source

- [ ] 1.1 Add `settle_burst_keeps_one_pending_source_across_a_burst` (70,000 schedules with drop probes, as `Debounce`'s test does), a clear test and a reentrancy test. Verify that the first two fail on `main`.
- [ ] 1.2 Implement design D1. Verify with `cargo test -p gtk-lush-settle`, `make gtk-lush-doctests`, `make check-gtk-lush-policy`, and the minimap and sidebar-transition widget shards.
- [ ] 1.3 Add the `CHANGELOG.md` entry, following the `gtk-lush-stewardship` skill. Verify with `make gtk-lush-adoption-matrix` and `make gtk-lush-api-advisory` (no public API change expected). Run `make visual-geometry-smoke`. Commit through `heavy.sh`, signed, and push.

## 2. Weak references without a refcount

- [ ] 2.1 Apply design D2 in `gtk-lush-signals` and `gtk-lush-viewport`. Verify with both crates' tests and `make gtk-lush-examples`. Commit through `heavy.sh`, signed, and push.

## Workflow follow-up

- `SupersedingTimer::is_armed()` ships with the draft-flag migration change.
- Archive after review and merge under the programme's merge rules.
