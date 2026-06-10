## Context

`docs/next/gtk-lush.md` defines `normalize-declarative-bindings` as a Phase 0
prerequisite simplification before extracting GTK Lush helpers. LushText already
uses the desired local style in places: preferences bind many GSettings keys
directly to row properties, the search panel binds simple option toggles, and
editor pages bind several read-only settings to `GtkSourceView` properties.

The repo also has many handlers that look like projection code at first glance
but actually drive behavior: selected-tab handlers refresh status, preview, and
minimap state; search option changes can restart searches; shell notify
handlers persist window geometry; adaptive layout notifications synchronize
breakpoints and requested secondary surfaces. This change therefore treats
"safe conversion" as an audited classification, not a blanket rewrite.

## Goals / Non-Goals

**Goals:**

- Inventory repo-wide UI signal, notify, settings, and refresh handlers before
  editing modules.
- Convert every audited pure or pure-derived projection to GTK-native binding
  mechanisms when doing so preserves lifecycle, ordering, and visible behavior.
- Leave side-effectful, persistence, async, readiness, layout orchestration, and
  model/factory recycling handlers imperative with an explicit audit reason.
- Prove converted surfaces through focused tests and the relevant state
  extremes: no required context, representative populated data, many or awkward
  items, and constrained geometry.
- Update rules/docs only where implementation establishes a repeatable local
  pattern.

**Non-Goals:**

- No GTK Lush public crate API, macro, or extracted helper is introduced.
- No custom view DSL, app state/message system, component hierarchy, or
  replacement for Blueprint/GtkBuilder is introduced.
- No intentional user-facing behavior, persistence schema, D-Bus automation
  surface, runtime dependency, or action contract changes.
- No conversion is required for handlers that cannot be proven pure within this
  change's audit.

## Decisions

### Audit before conversion

Every candidate handler is classified before conversion:

| Class | Meaning | Treatment |
| --- | --- | --- |
| Pure projection | A source property or setting only copies to a widget property | Convert to `gio::Settings::bind`, `bind_property`, Blueprint `bind`, or equivalent |
| Pure-derived projection | A deterministic, side-effect-free mapping computes a view property | Convert only when the derivation can live in a pure helper, expression, or local derived property |
| Workflow side effect | Starts search, preview, minimap refresh, focus restoration, command refresh, document work, or notifications | Leave imperative |
| Persistence | Writes GSettings, JSON, session, workspace, geometry, or draft state | Leave imperative unless it is already a direct settings binding |
| Layout/readiness orchestration | Coordinates breakpoints, split views, pending work, idle/readiness, or animation timing | Leave imperative |
| Model/factory recycling | Depends on list-row binding/unbinding, item identity, or GTK factory lifecycle | Leave imperative unless a tiny local projection is isolated and tested |

Alternative considered: converting any handler whose body is small. That is too
risky because several small handlers trigger meaningful work or preserve GTK
ordering.

### Prefer existing GTK mechanisms

Conversion preference order is:

1. `gio::Settings::bind` for direct settings-to-property or two-way preference
   values.
2. `ObjectExt::bind_property` for object-to-object property projection.
3. Blueprint `bind` or GtkBuilder expressions when the source and target are
   template-local and the generated `.ui` contract stays drift-free.
4. A small private derived GObject property on an existing widget when multiple
   pure projections share the same deterministic derivation.
5. An imperative handler when the mapping has side effects, ordering
   constraints, or unclear lifecycle ownership.

Alternative considered: adding an app-local binding abstraction now. That would
blur the boundary with future GTK Lush crates and make the Phase 0 result less
useful as source material for extraction.

### Keep converted lifetimes explicit

Bindings created in Rust remain owned by the widget/template lifetime that uses
them. If a binding source can outlive the widget, the implementation must either
use an existing binding lifetime mechanism or store/disconnect the binding as
carefully as today's signal handlers. Template-level bindings inherit the
GtkBuilder template lifetime and must remain visible in reviewed Blueprint
source.

Alternative considered: relying on ad hoc closure captures and object ref
cycles. That would recreate the lifetime ambiguity GTK Lush is meant to remove.

### Prove state extremes at the changed surface

Tests should stay focused on modules that changed, but they must include the
state extremes the surface can enter. For UI conversions this means commands
remain reachable, empty states remain readable, populated and dense lists scroll
only in their intended regions, persistent headers and close/actions remain
visible, and constrained geometry does not gain unintended scrollbars, fake
rows, or unrelated-context dependencies.

Alternative considered: relying only on `cargo test` because bindings are
mechanical. That would miss regressions caused by property direction, initial
sync, template drift, or GTK allocation timing.

## Risks / Trade-offs

- [Risk] A side-effectful handler is mistaken for a pure projection. -> Mitigate
  with the required audit matrix, explicit "left imperative" reasons, and tests
  for workflow-triggering surfaces.
- [Risk] Binding direction or initial synchronization changes visible state. ->
  Mitigate by preferring direct GTK binding primitives and testing initial,
  changed, and restored settings/property states.
- [Risk] Template-level bindings drift from generated GtkBuilder output. ->
  Mitigate with Blueprint regeneration, template drift checks, and the existing
  template source-fidelity contract.
- [Risk] Converted bindings keep objects alive longer than intended. -> Mitigate
  by owning binding handles with the widget lifetime and preserving explicit
  disconnect behavior for sources that outlive the widget.
- [Risk] The audit creates churn without reducing complexity. -> Mitigate by
  converting all audited safe candidates in the same change and documenting why
  remaining candidates are intentionally imperative.

## Migration Plan

1. Build the audit inventory from `crates/lushtext-core/src/ui/` signal,
   notify, settings, property-binding, and projection refresh sites.
2. Convert modules in small groups, starting with direct settings/property
   projections and ending with derived projections that need local properties
   or template bindings.
3. Keep non-pure handlers in place and record their classification so future
   GTK Lush extraction starts from real lifecycle and side-effect sites.
4. Update tests alongside each converted group, then run the full phase gates.
5. Update local rules/docs only after the final normalized pattern is visible in
   code.

Rollback is ordinary source rollback: the change does not migrate persisted data
or expose new runtime contracts.

## Open Questions

- Should the final audit matrix live in `design.md`, `tasks.md`, or a short
  implementation note under `docs/next/`? The implementation should choose the
  least noisy place that remains reviewable at archive time.
- Which derived projections, if any, are worth private GObject properties rather
  than remaining imperative for clarity?
