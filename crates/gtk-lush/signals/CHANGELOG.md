# Changelog

All notable changes to `gtk-lush-signals` will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate follows SemVer after functional publication.

## [0.0.0] - Unreleased

### Added

- First functional in-tree API with `SignalBag`, `BindingBag`, and
  `RegistrationBag`.
- Weak-source signal cleanup, idempotent clear/drop behavior, doctests, and
  standalone gtk-rs adoption example.
