# declarative-ui-projection-normalization Specification

## Purpose
Define how LushText audits and converts pure UI projection handlers to existing
GTK-native binding mechanisms without hiding workflow side effects or inventing
a GTK Lush public API during Phase 0 cleanup work.

## Requirements

### Requirement: UI projection handlers are audited before conversion
The implementation SHALL audit repo-wide UI signal, notify, settings, property
binding, and projection refresh candidates before converting handlers. The audit
MUST classify each candidate as pure projection, pure-derived projection,
workflow side effect, persistence, layout/readiness orchestration, or
model/factory recycling, and MUST record the intended treatment for each
candidate.

#### Scenario: Audit classifies a direct projection candidate
- **WHEN** a handler only copies a source property or setting into a widget
  property
- **THEN** the audit classifies it as pure projection
- **AND** the candidate is eligible for GTK-native binding conversion

#### Scenario: Audit rejects a side-effectful candidate
- **WHEN** a handler starts search, preview, minimap, command, async,
  persistence, readiness, layout, focus, notification, or model/factory work
- **THEN** the audit classifies it as non-pure
- **AND** the handler remains imperative unless a separate pure projection is
  isolated and tested

### Requirement: Pure projections use GTK-native declarative mechanisms
Every audited pure projection SHALL be converted to an existing GTK,
Libadwaita, GtkBuilder, or GSettings binding mechanism when the conversion
preserves lifecycle ownership, initial synchronization, update direction, and
visible behavior.

#### Scenario: Direct setting projection is converted
- **WHEN** a GSettings key maps directly to a widget property with no extra
  side effect
- **THEN** the implementation uses `gio::Settings::bind` or an equivalent
  GTK-native binding
- **AND** the widget state initializes and updates from the setting as before

#### Scenario: Direct object projection is converted
- **WHEN** one GObject property maps directly to another object property with no
  extra side effect
- **THEN** the implementation uses `ObjectExt::bind_property`, Blueprint `bind`,
  a GtkBuilder expression, or an equivalent GTK-native binding
- **AND** the binding lifetime is owned by the widget or template that uses it

### Requirement: Derived projections stay pure and local
Audited pure-derived projections SHALL be converted only when the derivation is
deterministic, side-effect-free, and locally testable. The implementation MUST
keep the derivation in a pure helper, GtkBuilder expression, template binding,
or private derived GObject property on an existing widget, and MUST NOT
introduce an app state/message system or GTK Lush public API.

#### Scenario: Derived view state is converted safely
- **WHEN** a widget property depends on a deterministic mapping from one or more
  source properties
- **THEN** the derived projection is expressed without workflow side effects
- **AND** tests cover the relevant source states and resulting view state

#### Scenario: Derived projection is not safe
- **WHEN** a derived mapping depends on side effects, ordering-sensitive GTK
  lifecycle work, async state, or hidden mutable workflow state
- **THEN** the handler remains imperative
- **AND** the audit records why the conversion was not safe

### Requirement: Workflow side effects remain explicit
The implementation SHALL keep workflow side effects, persistence, async work,
layout/readiness orchestration, and model/factory lifecycle work imperative
unless it isolates a separate pure projection from the side-effectful portion.
Converted projections MUST NOT change action reachability, D-Bus automation
behavior, persistence timing, search execution, preview refresh, minimap
refresh, focus restoration, or session/workspace saving behavior.

#### Scenario: Search option side effect remains explicit
- **WHEN** an option change currently updates stored state and triggers search
  workflow behavior
- **THEN** only the pure stored-state projection may be declaratively bound
- **AND** the search-triggering behavior remains in an explicit handler or an
  equally visible workflow entry point

#### Scenario: Layout orchestration remains explicit
- **WHEN** a notify handler coordinates adaptive layout, breakpoints, split
  visibility, animation settling, or readiness blockers
- **THEN** the orchestration remains imperative
- **AND** the visible layout behavior remains covered by widget or visual proof
  when the changed surface is geometry-sensitive

### Requirement: Binding normalization preserves UI state extremes
Converted UI surfaces SHALL preserve behavior across the state extremes the
surface can enter: no items or no required context, representative populated
data, many or awkward items, and constrained geometry. Commands MUST remain
reachable, empty states MUST remain readable, item lists MUST scroll only in
their intended regions, persistent headers and close/actions MUST remain
visible, and conversions MUST NOT introduce unintended scrollbars, fake rows, or
dependencies on unrelated context.

#### Scenario: Empty and no-context states remain readable
- **WHEN** a converted surface is shown without an active document, selection,
  workspace, search result, or other optional context it previously tolerated
- **THEN** the visible empty or disabled state remains readable
- **AND** no command becomes unreachable because a binding lacks source context

#### Scenario: Dense and constrained states remain stable
- **WHEN** a converted surface contains many or awkwardly named items or is
  shown in constrained geometry
- **THEN** the same region scrolls or clips as before
- **AND** persistent chrome, headers, close controls, and actions remain visible
  without new unintended scrollbars or fake rows

### Requirement: Binding normalization updates local guidance only after proof
Project rules, nested `AGENTS.md` files, and GTK Lush planning docs SHALL be
updated only when the implementation establishes a repeatable local pattern.
Guidance MUST describe the audit categories and conversion criteria, and MUST
NOT claim a GTK Lush crate API exists before a later extraction change provides
one.

#### Scenario: Guidance changes follow implementation
- **WHEN** the implementation converts a repeatable class of pure projection
  handlers
- **THEN** local guidance records the pattern and the non-pure exclusions
- **AND** the guidance points to existing GTK mechanisms rather than a
  nonexistent GTK Lush abstraction
