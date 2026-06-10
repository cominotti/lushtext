## ADDED Requirements

### Requirement: Declarative binding normalization remains app-internal
The `normalize-declarative-bindings` follow-up SHALL remain a Phase 0
LushText-internal simplification. It MUST reduce extraction noise by separating
pure UI projections from real workflow side effects, and MUST NOT introduce a
GTK Lush public API, custom view DSL, control-flow owner, state/message system,
component framework, or inter-crate dependency.

#### Scenario: Follow-up proposal stays within Phase 0
- **WHEN** `normalize-declarative-bindings` is proposed or implemented
- **THEN** its artifacts reference GTK Lush governance as the controlling
  program capability
- **AND** the change states that no GTK Lush public crate API, view DSL,
  control-flow owner, or state/message system is introduced

#### Scenario: Safe conversion is not extraction
- **WHEN** a pure projection is converted during this follow-up
- **THEN** the conversion uses existing GTK, Libadwaita, GtkBuilder, GSettings,
  or app-local widget mechanisms
- **AND** any reusable GTK Lush API design is deferred to a later reserved
  extraction change

#### Scenario: Phase boundary uses full gates
- **WHEN** this follow-up reaches completion
- **THEN** LushText's full phase gate set passes, including visual-geometry
  proof whenever visual-sensitive files changed
- **AND** the audit records which candidate handlers remain imperative for
  governance-relevant side-effect or lifecycle reasons
