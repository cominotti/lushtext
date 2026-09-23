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
  Keep `ViewportSliceBin` discovering its outer `GtkScrolledWindow` by ancestry
  with an explicit `outer-scrolled-window` override, and keep `viewport_slice`
  a public pure function: consumers and property tests share one geometry.
  `outer_scroll_request` joins it for the same reason, after the inline version
  of that decision shipped a defect the whole suite missed: both of the bin's
  decisions are now pure, public, and property-tested. `classify_child_scroll`
  / `ChildScrollDecision` widen the second to its full outcome (rest, request,
  settle, defer) once the bin had to defer a divergence rather than overwrite
  it; `outer_scroll_request` stays as its request-only projection. Their
  rustdoc states the whole-pixel domain Kani proves them on (2026-09-23); the
  `cfg(kani)` harnesses are test-like and add no public API.
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
