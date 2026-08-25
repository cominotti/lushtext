## MODIFIED Requirements

### Requirement: A migrated workflow presents one narrative facade

A migrated workflow SHALL expose one facade module that narrates the workflow from
user action to completion. The facade MUST delegate to policy, coordination,
adapter, and evidence roles rather than implementing them. A reader MUST be able to
follow the workflow's ordered stages from the facade without opening the
coordination or policy modules.

A normative maximum size for facade modules is **declared and active**. It was
derived from the exemplar facade's measured size by the first migration change
following the exemplar, rather than chosen in advance, because a budget fixed
before any facade existed risked forcing the narration itself to be split. That
budget applies to every migrated workflow. It SHALL be declared in
`docs/workflow-readability-matrix.md` in the machine-readable form documented
beside the declaration, and `make check-policy` MUST fail when a `migrated` row's
declared facade file exceeds it. A later change MUST NOT re-derive the number as
if it were still unset; changing it is a convention amendment and MUST follow the
retroactive amendment rule, which means re-checking every already-migrated row
against the new number in the same change.

Until a budget is declared, a facade is judged only by the delegation and
narration requirements below. That state is historical for this project and MUST
NOT be re-entered by removing the declaration.

#### Scenario: Facade budget is measured before it is normative

- **WHEN** the exemplar migration completed its facade
- **THEN** it recorded that facade's measured size and left the normative budget
  unset
- **AND** the next migration change set the budget from that measurement, and it
  and every later migration are held to it

#### Scenario: Declared budget is mechanically enforced

- **WHEN** the matrix declares a normative facade line budget and a `migrated`
  row's declared facade file exceeds it
- **THEN** `make check-policy` fails
- **AND** the failure names the row, the facade path, its measured size, and the
  budget

#### Scenario: Undeclared budget is not silently enforced

- **WHEN** the matrix declares no normative facade line budget
- **THEN** the facade size check is inert rather than inventing a default
- **AND** facades are judged only by the delegation and narration requirements

#### Scenario: Later migration does not re-derive the settled budget

- **WHEN** a migration change after the budget-setting change reads the
  facade-budget requirement
- **THEN** it treats the declared number as settled convention and holds its own
  facade to it
- **AND** it does not choose a new number without following the retroactive
  amendment rule

#### Scenario: Facade narrates the ordered stages

- **WHEN** a reader opens the facade of a migrated workflow
- **THEN** the workflow's stages appear in order with their intent named
- **AND** each stage delegates to a named role rather than inlining machinery

#### Scenario: Inverted control flow is narrated, not hidden

- **WHEN** a workflow's stages are connected by a deferred drain, idle callback, or
  worker completion rather than by direct calls
- **THEN** the facade documents that inversion and names the point where control
  resumes
- **AND** the reader is not required to reconstruct the resumption point from the
  coordination module

#### Scenario: Facade does not become a second implementation

- **WHEN** the facade would need to own timers, admission bookkeeping, generation
  counters, or GTK widget mutation to express a stage
- **THEN** that work stays in the coordination or adapter role
- **AND** the facade calls it through a named operation
