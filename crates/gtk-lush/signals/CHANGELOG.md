# Changelog

All notable changes to `gtk-lush-signals` will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate follows SemVer after functional publication.

## [0.0.0] - Unreleased

### Added

- `SignalSlot` (zero-`Vec` single replaceable weak-source `SignalHandlerId`
  owner with `replace`, `clear`, `is_connected`, and `Drop`).
- `Debug` implementations on `SignalBag`, `SignalSlot`, `BindingBag`, and
  `RegistrationBag`.
- Unit tests for `SignalSlot`, `Debug`, re-entrant `clear()`/`track()` during
  disconnect, `BindingBag::drop`, and `RegistrationBag::drop`.
- First functional in-tree API with `SignalBag`, `BindingBag`, and
  `RegistrationBag`.
- Weak-source signal cleanup, idempotent clear/drop behavior, doctests, and
  standalone gtk-rs adoption example.
- Adoption-lab and matrix evidence before functional publication.

### Changed

- `SignalBag::track` now downgrades `source.upcast_ref::<Object>()` without
  requiring `+ Clone` or an extra ref/unref pair, and module docs clarify FIFO
  cleanup ordering and disposed-but-referenced source hazards.
