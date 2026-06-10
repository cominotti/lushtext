## Why

GTK Lush Phase 0 needs the LushText UI surface to distinguish real workflow
side effects from imperative handlers that only copy state into widgets. The
preview-pane simplification removed one large custom shell; the next risk is
repo-wide signal plumbing that may hide pure state-to-view projections inside
manual `connect_*` handlers.

## What Changes

- Add a repo-wide audit for UI signal, notify, settings, and refresh handlers
  that classifies each candidate before any conversion.
- Convert every safe pure or pure-derived state-to-view projection to existing
  GTK/Libadwaita mechanisms such as `gio::Settings::bind`,
  `ObjectExt::bind_property`, Blueprint `bind`, GtkBuilder expressions, or
  small derived GObject properties on existing widgets.
- Leave handlers imperative when they perform workflow side effects,
  persistence, async work, search/rebuild activity, readiness/layout
  orchestration, model/view factory recycling, or any ordering-sensitive GTK
  lifecycle work.
- Add or update focused tests for converted modules, including empty,
  populated, awkward/dense, and constrained-geometry states where the changed
  surface can enter them.
- Update local project rules or docs only where the normalized pattern becomes
  project guidance; do not introduce a GTK Lush crate API, custom view DSL, or
  app state/message system in this phase.

## Capabilities

### New Capabilities

- `declarative-ui-projection-normalization`: Audit and normalize pure UI
  projection handlers to GTK-native bindings or pure derived properties while
  preserving explicit imperative workflow side effects.

### Modified Capabilities

- `gtk-lush-program-governance`: Tighten follow-up conformance for
  `normalize-declarative-bindings` so it remains a Phase 0 app-internal
  simplification and does not become a premature GTK Lush API or framework
  primitive.
- `ui-template-source-fidelity`: Clarify that template-level bindings or
  expressions introduced during safe conversions must preserve the existing
  Blueprint/GtkBuilder source, generated output, and geometry-proof contracts.

## Impact

- Affected code: repo-wide Rust UI wiring under `crates/lushtext-core/src/ui/`,
  UI templates under `crates/lushtext-core/data/ui/` if template-level binding
  is the safest conversion, and the matching widget tests or smoke scenarios.
- Affected docs/rules: `.agents/rules/*.md`, nested `AGENTS.md` files, and GTK
  Lush planning docs only if the implementation changes the documented local
  binding pattern or phase narrative.
- No user-facing feature, action, D-Bus automation contract, persistence
  schema, runtime dependency, or GTK Lush public crate API is intended to
  change.
