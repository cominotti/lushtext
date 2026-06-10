## 1. Audit Inventory

- [x] 1.1 Inventory repo-wide UI signal, notify, settings, property-binding, template-binding, and projection refresh candidates under `crates/lushtext-core/src/ui/` and relevant UI templates.
- [x] 1.2 Classify each candidate as pure projection, pure-derived projection, workflow side effect, persistence, layout/readiness orchestration, or model/factory recycling.
- [x] 1.3 Record each candidate's intended treatment, including explicit "left imperative" reasons for non-pure handlers and risky ordering/lifecycle cases.
- [x] 1.4 Identify conversion batches with disjoint modules or surfaces so implementation can proceed in reviewable groups.

## 2. Safe Conversion Implementation

- [x] 2.1 Convert all audited direct GSettings-to-widget projections to `gio::Settings::bind` or the existing equivalent GTK-native binding.
- [x] 2.2 Convert all audited direct object-to-object projections to `ObjectExt::bind_property`, Blueprint `bind`, GtkBuilder expressions, or equivalent GTK-native bindings.
- [x] 2.3 Convert audited pure-derived projections only when the derivation can live in a pure helper, template expression, or private derived property on an existing widget.
- [x] 2.4 Preserve imperative handlers for workflow side effects, persistence, async work, search/rebuild behavior, focus restoration, readiness/layout orchestration, and model/factory recycling.
- [x] 2.5 Preserve binding lifetimes explicitly for Rust-created bindings, especially when the source object can outlive the widget.
- [x] 2.6 If template-level bindings are used, regenerate generated `.ui` files and keep Rust `TemplateChild` contracts, object IDs, layout roles, and accessibility/action metadata stable.

## 3. Coverage and Proof

- [x] 3.1 Add or update focused unit/widget coverage for each converted module or surface.
- [x] 3.2 Cover relevant state extremes for converted UI surfaces: no required context, representative populated data, many or awkward items, and constrained geometry.
- [x] 3.3 Verify converted bindings preserve initial state, update direction, subsequent changes, and restored settings/property state.
- [x] 3.4 Run delegated review or equivalent independent audit over the final conversion set to catch missed safe candidates or unsafe conversions.
- [x] 3.5 Run `make visual-geometry-smoke` when visual-sensitive templates, layout roles, visibility, sensitivity, scroll policy, overlay placement, Adwaita slots, CSS geometry, or protected pixel invariants changed.

## 4. Documentation and Rules

- [x] 4.1 Update `.agents/rules/*.md` or nested `AGENTS.md` guidance only for normalized binding patterns that are proven by the implementation.
- [x] 4.2 Update `docs/next/gtk-lush.md` if the implementation changes the Phase 0 narrative, exit criteria, or future extraction source material.
- [x] 4.3 Update automation docs only if the implementation intentionally changes actions, D-Bus members, snapshots, readiness predicates/blockers, client behavior, or scenario helper flags; otherwise record that no automation contract changed.
- [x] 4.4 Keep guidance pointed at existing GTK/Libadwaita/GSettings/GtkBuilder mechanisms, not a future GTK Lush crate API.

## 5. Verification

- [x] 5.1 Run `make check`.
- [x] 5.2 Run `make test-widget-headless`.
- [x] 5.3 Run `make check-blueprint` if any `.blp` or generated `.ui` template changed outside the coverage already included by `make check`.
- [x] 5.4 Run `make check-automation-docs` and `make automation-client-self-test` if automation-facing files changed.
- [x] 5.5 Run `openspec validate normalize-declarative-bindings --type change --strict`.
- [x] 5.6 Run `openspec validate --changes --strict`.
- [x] 5.7 Run `openspec validate --specs --strict`.
- [x] 5.8 Run `openspec validate --all --strict`.
- [x] 5.9 Run `git diff --check`.
