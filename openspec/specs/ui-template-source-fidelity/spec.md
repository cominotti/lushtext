# ui-template-source-fidelity Specification

## Purpose
Define how Blueprint-authored UI templates preserve LushText's existing GtkBuilder resource contract, generated output discipline, layout semantics, and contributor packaging expectations.

## Requirements

### Requirement: Blueprint templates preserve the runtime GtkBuilder resource contract
The system SHALL use Blueprint as the reviewed source format for UI templates while preserving committed GtkBuilder `.ui` files as the runtime GResource input. Every migrated template MUST keep the same resource path, template class, parent class, object IDs, custom widget type names, CSS classes, translatable strings, accessibility metadata, menu model IDs, action names, shortcut definitions, and Rust `TemplateChild` bindings unless a separate behavior-changing OpenSpec explicitly authorizes a difference.

#### Scenario: Existing template resource paths remain stable
- **WHEN** a Rust `CompositeTemplate` declares a `#[template(resource = "/dev/cominotti/lushtext/ui/<name>.ui")]` path after the migration
- **THEN** the corresponding generated `.ui` file is present in the GResource bundle at the same path
- **AND** the widget continues to load through the existing resource-backed template mechanism

#### Scenario: Template child bindings are preserved
- **WHEN** a migrated template is loaded by its Rust widget type
- **THEN** every `TemplateChild` field declared by that widget resolves to an object with the same ID and compatible widget type as before the migration
- **AND** no widget construction path fails because a generated template removed or renamed a required child

#### Scenario: UI metadata remains equivalent
- **WHEN** generated `.ui` output is compared with the pre-migration template contract
- **THEN** user-facing labels, tooltip text, translation markers, action names, menu item targets, shortcut definitions, accessibility labels, and style classes remain equivalent
- **AND** any difference is documented as intentionally non-user-visible generated-format normalization

### Requirement: Generated GtkBuilder output stays drift-free from Blueprint source
The project SHALL provide a deterministic command that regenerates every Blueprint-authored `.ui` file and a validation command that fails when committed generated output differs from the `.blp` source. Contributors MUST treat `.blp` files as the source of truth and MUST NOT hand-edit generated `.ui` files without regenerating them from matching Blueprint source.

#### Scenario: Drift check catches stale generated files
- **WHEN** a `.blp` template changes without regenerating its committed `.ui` output
- **THEN** the template drift validation fails
- **AND** the failure identifies the stale template path

#### Scenario: Regeneration is deterministic
- **WHEN** the Blueprint regeneration command runs twice without source changes
- **THEN** the second run produces no diff in generated `.ui` files
- **AND** resource compilation inputs remain stable

#### Scenario: Missing Blueprint tooling fails clearly
- **WHEN** a contributor or CI lane runs the regeneration or drift validation command without `blueprint-compiler` available
- **THEN** the command fails with a clear setup message
- **AND** ordinary app runtime behavior is not changed by the missing tool

### Requirement: Layout-sensitive GtkBuilder semantics are preserved
The generated `.ui` templates SHALL preserve every layout-sensitive GtkBuilder
semantic from the current UI unless a behavior-changing OpenSpec explicitly
authorizes that semantic to change. This includes child roles, layout
properties, grid coordinates, overlay layers, Adwaita layout slots,
bottom-sheet content and sheet roles, remaining paned shrink and resize flags,
scrolled-window propagation and scrollbar policies, size-group members,
revealer transition settings, visible and sensitive defaults, and custom widget
placement. For this change, the main-window Markdown preview shell is
intentionally authorized to replace the preview `GtkPaned` node with an
Adwaita-native preview presentation while preserving unrelated template
semantics.

#### Scenario: Nested shell layout keeps approved roles
- **WHEN** the main window template is generated from Blueprint after the preview-shell migration
- **THEN** `AdwMultiLayoutView`, `AdwLayoutSlot`, `AdwBottomSheet`, `AdwOverlaySplitView`, `GtkOverlay`, sidebar, properties, status bar, command palette, focus-mode, search-panel, and Markdown preview nodes preserve their approved role relationships
- **AND** the Markdown preview presentation preserves editor-content and end-secondary-surface roles without requiring a `GtkPaned#preview_paned` node
- **AND** compact, pane, sheet, overlay, primary, properties, content, sidebar, start-child, end-child, content-child, and sidebar-child placements remain equivalent except for the approved preview-shell replacement

#### Scenario: Grid and overlay controls keep their allocations
- **WHEN** the search bar and editor templates are generated from Blueprint
- **THEN** grid row, column, and span properties remain equivalent
- **AND** overlay children keep the same overlay roles and alignment properties

#### Scenario: Size and scroll contracts remain unchanged
- **WHEN** generated templates are structurally audited
- **THEN** `GtkSizeGroup` members, `GtkScrolledWindow` propagation, scrollbar policies, minimum content sizes, width requests, height requests, margins, and expand flags remain equivalent
- **AND** no unintended horizontal scrollbar, fake row, clipped persistent chrome, or unrelated context dependency is introduced

#### Scenario: Preview template contract is regenerated deliberately
- **WHEN** the preview shell replaces `GtkPaned#preview_paned`
- **THEN** Rust `TemplateChild` fields, generated `.ui` output, and `template-contract.json` are updated together
- **AND** the template drift and contract checks fail if stale paned-node expectations remain after the migration

### Requirement: Blueprint migration is user-visible 1:1
The migration SHALL NOT intentionally change LushText's user-visible UI or UX. The app MUST preserve existing layout, geometry, spacing, focus behavior, keyboard reachability, menu behavior, action sensitivity, empty states, populated states, dense states, and constrained-geometry behavior across the migrated templates.

#### Scenario: Representative normal state is unchanged
- **WHEN** LushText launches with a normal text document after the migration
- **THEN** the header bar, tab strip, editor surface, status bar, workspace control, document surface, and document metadata affordances present the same visible behavior as before
- **AND** no new template-loading, GTK, Libadwaita, GDK, renderer, accessibility, or allocation warnings are emitted

#### Scenario: State extremes remain usable
- **WHEN** the app presents empty states, representative populated states, many or awkward items, and constrained geometry after the migration
- **THEN** commands remain reachable, empty states remain readable, dense lists scroll in the intended region, persistent headers and close/actions remain visible, and no unintended scrollbars or fake rows appear

#### Scenario: Geometry-sensitive surfaces remain stable
- **WHEN** compact, narrow, short-window, search-panel, search-bar, sidebar, workspace-section, inline-alert, properties-pane, Markdown-preview, menu, popup, and modal states are exercised after the migration
- **THEN** their visible geometry and interaction behavior remain equivalent to the pre-migration UI
- **AND** any deviation blocks completion of the change unless captured in a separate approved OpenSpec

### Requirement: Packaging and contributor tooling document Blueprint responsibilities
The project SHALL document how contributors regenerate and validate Blueprint-authored templates and how CI and packaging lanes consume generated `.ui` files. `blueprint-compiler` MUST be a contributor and CI generation/check tool only, and end users MUST NOT gain a new runtime dependency because UI templates are authored in Blueprint.

#### Scenario: Contributor docs explain source and generated files
- **WHEN** a contributor edits a UI template after the migration
- **THEN** project guidance identifies the `.blp` file as the editable source
- **AND** it names the command that regenerates and validates the matching `.ui` output

#### Scenario: Packaging continues to consume generated GtkBuilder resources
- **WHEN** Cargo, Meson, Flatpak, or Snap builds compile application resources after the migration
- **THEN** they continue to bundle generated `.ui` files through the existing GResource contract
- **AND** end-user runtime packages do not require `blueprint-compiler`

#### Scenario: CI validates Blueprint drift
- **WHEN** CI runs template validation after the migration
- **THEN** it verifies that generated `.ui` files are current with `.blp` source
- **AND** it fails before packaging or release if generated resource inputs are stale

### Requirement: Geometry-sensitive template edits prove visual invariants
Any UI template edit that changes layout roles, child order, expand flags, margins, size requests, scroll policies, overlay placement, Adwaita layout slots, paned child properties, CSS classes that affect geometry, or accessibility anchors SHALL run or update the relevant visual invariant matrix before claiming layout fidelity.

#### Scenario: Template edit runs invariant proof
- **WHEN** a Blueprint or generated GtkBuilder template edit can affect geometry-sensitive surfaces
- **THEN** the change includes relevant widget allocation assertions or same-session visual invariant proof
- **AND** the proof names protected regions, allowed-changing regions, and state extremes exercised

#### Scenario: Nonzero pixel differences are explained
- **WHEN** a geometry-sensitive template edit produces nonzero differences in a protected or comparison region
- **THEN** the change cannot claim 1:1 UI/UX fidelity unless those differences are reclassified through an updated invariant manifest and justified as intentional
- **AND** the explanation is captured in the change artifacts or review notes

#### Scenario: Generated output drift is not visual proof
- **WHEN** Blueprint regeneration produces no `.ui` drift or passes template validation
- **THEN** that result alone does not satisfy visual geometry proof for a geometry-sensitive change
- **AND** the relevant widget or visual invariant coverage still runs

### Requirement: Template fidelity docs name visual proof responsibilities
Project guidance for UI template work SHALL explain when visual invariant proof is required and where its artifacts are stored.

#### Scenario: Contributor guidance includes visual invariant lane
- **WHEN** maintainers read template editing guidance
- **THEN** it identifies Blueprint regeneration, drift checks, widget tests, and visual invariant proof as separate responsibilities
- **AND** it explains that visual artifacts live under ignored build or smoke directories rather than committed screenshot files

### Requirement: Template-level bindings preserve source fidelity
The system SHALL preserve template source fidelity when safe projection
conversions use Blueprint `bind`, GtkBuilder expressions, or generated
template-level property bindings. The `.blp` source MUST remain the reviewed
source of truth, generated `.ui` output MUST remain deterministic and committed
as the runtime GResource input, and any binding-level change MUST preserve
template class, parent class, object IDs, `TemplateChild` resolution, action
names, accessibility metadata, layout roles, and geometry-sensitive semantics
unless another OpenSpec explicitly authorizes a behavior change.

#### Scenario: Template binding keeps generated output current
- **WHEN** a Blueprint template gains or changes a binding or expression during
  declarative projection normalization
- **THEN** the matching generated `.ui` file is regenerated and committed
- **AND** template drift validation fails if the generated output is stale

#### Scenario: Template binding keeps Rust template children stable
- **WHEN** a Rust `CompositeTemplate` loads a template that gained or changed a
  binding or expression
- **THEN** every existing `TemplateChild` field still resolves to a compatible
  object ID and widget type
- **AND** no widget construction path fails because the conversion removed,
  renamed, or reparented a required child

#### Scenario: Geometry-sensitive template binding proves invariants
- **WHEN** a template-level binding or expression can affect layout roles,
  child order, visibility, sensitivity, size, scroll policy, overlay placement,
  Adwaita layout slots, CSS classes, or accessibility anchors
- **THEN** the implementation runs or updates the relevant widget allocation or
  visual invariant proof
- **AND** nonzero geometry or pixel differences in protected regions are either
  eliminated or captured in a separate approved behavior-changing OpenSpec
