## Why

Phase 0 normalized LushText's app-local timer helpers and Phase 1 established
the GTK Lush workspace, governance, and placeholder crates. The next phase can
now turn the two most mature rule families -- signal/binding lifetimes and
generation-counter settle scheduling -- into real leaf-crate APIs and migrate
LushText onto them without changing user-visible behavior.

## What Changes

- Design and implement the first functional `gtk-lush-signals` API for RAII
  ownership of GObject signal handlers, property bindings, and common
  controller-style registrations.
- Design and implement the first functional `gtk-lush-settle` API for
  debounce, delayed settle bursts with readiness-visible pending state, and
  superseding one-shot timers.
- Migrate LushText's matching manual `SignalHandlerId`/`Binding` bookkeeping
  and private `crate::ui::settle` call sites onto the new crates, deleting the
  replaced app-local machinery rather than wrapping it.
- Preserve GTK ownership of the main loop, widget lifecycle, rendering, and
  ordinary gtk-rs application structure; the crates remain independently
  adoptable leaves and do not depend on each other or on LushText.
- Rewrite the corresponding `.agents/rules` guidance and crate READMEs after
  proof so the rules point at the enforced crate contracts and keep only
  LushText-specific judgment.
- Keep this phase in-tree and pre-publish: it may produce functional
  workspace APIs, but it does not satisfy the Phase 5 two-consumer and
  afternoon-adoption publishing gates.

## Capabilities

### New Capabilities

- `gtk-lush-signals`: RAII signal, binding, and registration lifetime
  management for stock gtk-rs applications, plus the LushText migration away
  from manual handler-id fields.
- `gtk-lush-settle`: Generation-counter debounce, settle-burst, and
  superseding-timer primitives for GLib main-loop work, plus the LushText
  migration away from the private Phase 0 helper.

### Modified Capabilities

- None. Existing `gtk-lush-program-governance` and `gtk-lush-workspace`
  requirements already define the leaf-crate constitution, workspace policy,
  engineering bar, follow-up roadmap, and publishing gates that constrain this
  phase.

## Impact

- Public workspace APIs in `crates/gtk-lush/signals` and
  `crates/gtk-lush/settle`.
- LushText UI modules that currently own manual signal/binding disconnect
  fields or use `crate::ui::settle::{Debounce, SettleBurst, SupersedingTimer}`.
- `.agents/rules/rust.md`, `.agents/rules/widget-wiring.md`,
  `docs/next/gtk-lush.md`, and crate README/CHANGELOG files.
- Test and proof surface: family unit tests, doctests, standalone examples,
  full LushText non-widget and widget gates, GTK warning checks, automation
  readiness checks, and visual-geometry proof for minimap or other
  rendered-geometry-sensitive migrations.
