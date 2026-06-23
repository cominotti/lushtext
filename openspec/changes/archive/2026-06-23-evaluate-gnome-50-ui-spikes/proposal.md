## Why

GNOME 50 gives LushText new Libadwaita sidebar widgets and GTK builder diagnostics that may reduce custom UI risk, but both need bounded evaluation before any product-facing adoption. The dependency refresh explicitly deferred these platform features, so this change turns that deferral into two focused spikes with clear evidence and stop conditions.

## What Changes

- Evaluate where `AdwSidebar` and `AdwViewSwitcherSidebar` fit LushText after the existing Notes and Local History `AdwSidebar` adoption.
- Preserve the current rule that the primary workspace file tree stays on `GtkListView` plus `GtkTreeListModel` and is not replaced by `AdwSidebar`.
- Evaluate whether a stable `AdwViewStack`-backed Document Activity or Inspector surface is a good candidate for `AdwViewSwitcherSidebar`.
- Evaluate a runtime `GTK_DEBUG=builder` diagnostic lane for LushText's generated GtkBuilder templates, especially templates that standalone `gtk4-builder-tool validate` cannot load because they require Libadwaita or app-registered composite widget types.
- Record outcomes as spike evidence and follow-up recommendations, not as immediate UI rewrites.

## Capabilities

### New Capabilities

- `adw-sidebar-view-switcher-spike`: Defines how LushText evaluates GNOME 50 `AdwSidebar` and `AdwViewSwitcherSidebar` adoption opportunities, including fit criteria, state-extreme coverage, and explicit non-adoption of the primary workspace file tree.
- `gtk-builder-diagnostics-spike`: Defines how LushText evaluates a `GTK_DEBUG=builder` runtime diagnostic lane for Blueprint-generated GtkBuilder templates without replacing existing Blueprint drift, lint, or widget validation gates.

### Modified Capabilities

- None.

## Impact

- Affected planning and evidence areas: `openspec/changes/evaluate-gnome-50-ui-spikes/`, `docs/next/gnome-50-api-opportunities.md`, `docs/blueprint-validation.md`, and any spike notes produced during implementation.
- Affected UI surfaces for evaluation only: Notes browser, Local History browser, document properties, file health, encoding controls, document notes, local history, workspace sidebar, and generated templates under `resources/ui/`.
- Affected validation surfaces for evaluation only: `make check-blueprint`, `make lint-blueprint`, `gtk4-builder-tool validate` for GTK-only templates, runtime widget or smoke harnesses that initialize Libadwaita, and captured stderr/log artifacts with `GTK_DEBUG=builder,builder-objects`.
- No Cargo dependency, Flatpak permission, schema, app-data format, automation API, or user-visible behavior change is required by this proposal.
