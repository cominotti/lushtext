## ADDED Requirements

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
