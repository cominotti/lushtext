# Changelog

All notable changes to `gtk-lush-settle` will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate follows SemVer after functional publication.

## [0.0.0] - Unreleased

### Added

- `Debounce::is_armed(&self) -> bool` and `SupersedingTimer::is_armed(&self) -> bool`.
- Main-loop pumping unit tests covering `Debounce`, `SupersedingTimer`,
  `SettleBurst`, `SettleHandle::schedule_follow_up`, dead-target `pending()`
  reset, `advance()` source cleanup, and `is_armed()`.
- Explicit `[features] property-tests = []` gate for `proptest!` suites.
- First functional in-tree API with `Debounce`, `SettleBurst`, and
  `SupersedingTimer`.
- Generation-token tests, settle pending-state tests, property tests, doctests,
  and standalone gtk-rs adoption example.
- Adoption-lab, timed stock fixture, external-spike, and matrix evidence before
  functional publication.

### Changed

- `SettleBurst` now tracks all armed primary and follow-up `glib::SourceId`s in
  one shared state, eagerly removing them on `begin()`/`clear()`, and calls
  `finish_if_current()` when a weak target drops before dispatch.
- `Debounce::advance()` now delegates to `invalidate()`, removing any superseded
  GLib source immediately, and `Debounce::schedule` / `SupersedingTimer::arm`
  share one internal scheduling helper.
- `SupersedingTimer` now removes obsolete GLib sources immediately on re-arm or
  invalidation while preserving its existing generation-token API.
- `Debounce` does the same: a new `schedule` or `invalidate` removes the
  superseded GLib source, so one pending source (and one weak reference to the
  target) remains however often it is scheduled in one dispatch. Keeping every
  superseded source made 131,072 schedules in one dispatch exceed GLib's
  per-object weak-reference limit (`Too many GWeakRef registered`). The
  generation-token API is unchanged.
