# Timed Stock Adoption: `gtk-lush-settle`

Date: 2026-06-12

Selected crate: `gtk-lush-settle`

Reason: settle/timer APIs are easy to overfit to LushText because they sit
between GTK timing, readiness, and caller-owned workflow state. They benefit
from stock gtk-rs pressure before publication.

## Starter Shape

Committed fixture: `fixtures/gtk-lush-adoption/stock-settle`

The fixture is an ordinary gtk-rs application with one path dependency:

```toml
gtk-lush-settle = { path = "../../../crates/gtk-lush/settle" }
```

It does not import LushText crates, LushText resources, LushText GSettings
schemas, or another GTK Lush family crate.

## Commands

```sh
cargo check --manifest-path fixtures/gtk-lush-adoption/stock-settle/Cargo.toml
/usr/bin/time -f 'elapsed=%E' cargo check --manifest-path fixtures/gtk-lush-adoption/stock-settle/Cargo.toml --locked
```

Timed verification:

- start: `2026-06-12T16:19:24-03:00`
- end: `2026-06-12T16:19:53-03:00`
- reported elapsed: `0:18.81`
- result: passed

## Code Summary

The fixture builds a stock `gtk4::Application` with an entry, two buttons, and a
status label. It uses:

- `Debounce::schedule` for trailing latest-generation entry updates
- `SettleBurst::schedule` and `finish_if_current` for visible pending/settled
  state
- `SupersedingTimer::arm` for latest cleanup

## Friction Classification

- Documentation: examples should keep showing both pure token helpers and
  scheduled GTK target usage.
- Type-shape: no blocking issue. GObject weak targets are a deliberate GTK
  lifecycle fit.
- Feature flag: no issue.
- Missing helper: no issue.
- Overreach: no issue.
- Accepted limitation: a scheduled callback needs a live GTK/GLib object target;
  consumers with pure async state should use `advance`/`is_current` or keep the
  target beside the caller.

## Decision

No breaking API change for this phase. Improve adoption docs and keep
`gtk-lush-settle` as a narrow generation-counter helper rather than adding a
runtime or non-GTK scheduling abstraction.
