# Friction-Driven API Review

Date: 2026-06-12

Scope: adoption lab, stock `gtk-lush-settle` fixture, and unrelated-project
spikes for `gtk4-rs` and Kooha.

## Decisions

- `gtk-lush-signals`: keep `SignalBag`, `BindingBag`, and `RegistrationBag`.
  The lab's recycle/rebind workflow confirms that explicit bags are smaller
  than a signal DSL.
- `gtk-lush-settle`: keep `Debounce`, `SettleBurst`, `SupersedingTimer`, and
  `TimerToken`. The stock fixture confirms the current shape is adoptable with
  ordinary gtk-rs widgets. No runtime or non-GTK scheduler will be added.
- `gtk-lush-tasks`: keep caller-owned freshness. The lab wraps simulated
  panics as `Result` values, which keeps the crate from becoming an executor or
  app-level error framework.
- `gtk-lush-viewport`: keep adjustment-derived observer value objects.
  Consumers still own reactions to page-size and lower-edge changes.
- `gtk-lush-widgets`: keep `RenderHoldCapture::NotReady` as normal output.
  Capture needing mapped/drawable geometry is a GTK contract, not an API bug.
- `gtk-lush-proof-harness`: keep caller-owned environment mutation and test
  registry. The harness should not mutate process environment for consumers.
- `gtk-lush-proof-spine`: keep GTK-free provider traits and bounded value
  objects. No transport, D-Bus, command, or app-state ownership is added.

## Rejected Overreach

- No view DSL.
- No component model.
- No application message loop.
- No custom runtime.
- No Libadwaita replacement.
- No runtime dependencies between GTK Lush family crates.

## Deferred Items

- Revisit non-GObject scheduling only if a future external adopter has a real
  GTK application workflow that cannot use weak GTK/GLib targets or pure token
  checks. No action is needed for the current internal platform.
- Choose a smaller third-party GTK4 app only if a later approved publication or
  graduation track needs a successful end-to-end external build without
  installing a large native dependency stack.
