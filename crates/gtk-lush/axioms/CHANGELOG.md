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
