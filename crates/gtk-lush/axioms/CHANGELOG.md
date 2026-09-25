# Changelog

## 0.0.0

- First functional in-tree pre-publication implementation: the axiom
  catalogue (`AxiomId`, `Axiom`, `catalogue()`, `find()`), the `Observation`
  / `Verdict` result with a dependency-free one-line JSON encoder, the shared
  pure-GTK fixtures (`fixtures::FixedHost`, `HostedList`, the per-axiom
  fixtures, and the values each probe drives them with under
  `fixtures::aNN`), and probes for A1, A2, A3, A4, A5, A6, A7, A9, A11, and A13.
- The A5, A9, A11, and A13 probes moved here from LushText's widget-test
  binary with their steps unchanged; their measured values are identical on
  GTK 4.22.5 / Libadwaita 1.9.3. The fixture GType is renamed
  `GtkLushAxiomsFixedHost`, and the probes present in a plain `AdwWindow`
  with no application.
- One sample per probe under `examples/`, interactive by default and a
  headless verdict with `--check` (exit 0 / 1 / 2, and 77 outside a private
  headless session).
- The probe runner is the `axiom_probes` test of `gtk-lush-adoption-lab`, run
  by `make gtk-axioms`, because a family crate may not depend on
  `gtk-lush-proof-harness`.
- Probes and samples for the Adwaita behaviours the adaptive-shell breakpoint
  loop relies on (`extend-closed-loop-geometry-verification`), pure Libadwaita
  fixtures only: A14 (`max-width: N sp` matches `px ≤ N × text scale`,
  inclusive), A15 (`set_condition` applies a frame later, at an allocation it
  schedules itself), A16 (setters change after the bin allocated its child, and
  an unapply restores the value from `add_setter` time), A17
  (`AdwOverlaySplitView` sidebar width is a clamped fraction in sp; toggling it
  never changes the window width), and A18 (a breakpoint makes the window's
  minimum its `width-request`; only the last-added matching breakpoint
  applies). New shared fixtures: `fixtures::BreakpointFixture` (an
  `AdwBreakpointBin` allocated at an exact width), `fixtures::SplitViewFixture`,
  `fixtures::set_text_scale`, and `FixedHost::set_child_width`; the values each
  probe drives them with are under `fixtures::a14` … `fixtures::a18`.
