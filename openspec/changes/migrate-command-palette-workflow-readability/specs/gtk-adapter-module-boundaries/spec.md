## MODIFIED Requirements

### Requirement: Decomposed workflow modules carry named roles

Decomposition of a GTK adapter workflow SHALL assign each resulting module one
named role: narrative facade, seam value objects, pure policy, coordination, or
evidence. A module MUST NOT combine the facade role with the coordination or policy
role. Splitting a large file into sibling modules without assigning roles does not
satisfy this requirement.

The existing prohibitions still apply: role assignment MUST NOT introduce new
crates, generic repository or controller traits, or manager objects solely to move
code, and services and models MUST remain GTK-free.

The coordination role SHALL be named from a bounded set of role names that state the
job the module performs, such as admission, execution, retirement, or watch. The
convention MUST NOT fix one single coordination file name, because a workflow may
own more than one coordination job and a directory may host more than one workflow.
A coordination job that no existing role name describes MUST be added to the bounded
set by amending this specification, rather than reusing an ill-fitting name or
inventing an unlisted one.

Where **one** workflow owns more than one ordered stage order in a single directory,
and more than one of those stage orders needs a coordination module of the same
shape, a coordination module name MAY qualify a bounded role name with the stage
order it serves. The qualifier names the stage order in the workflow's own domain
vocabulary and the suffix remains a bounded role name, so the role stays readable
and the bounded set is not widened by the qualification. A workflow MUST NOT take an
ill-fitting bounded name merely because the fitting one is already spent on a
different stage order of the same workflow.

The bounded set is a review contract, not a mechanically enforced one: the workflow
boundary check validates that a migrated row's declared role paths exist, and does
not verify that a coordination module's name is drawn from the set. A migration
therefore MUST NOT rely on a gate to reject an off-set name.

The pure policy role is named `policy.rs` and the evidence role is named
`evidence.rs`, one of each per workflow. The facade role is the workflow's public
module surface.

#### Scenario: Coordination file is named for its job

- **WHEN** a workflow's coordination module is created or renamed during migration
- **THEN** its file name states the coordination job it performs
- **AND** a name that only says the module is runtime machinery is not sufficient

#### Scenario: One directory hosting several workflows keeps flat role names

- **WHEN** a directory hosts more than one workflow
- **THEN** each workflow's coordination modules use role names scoped to that
  workflow within the shared directory
- **AND** migration does not require restructuring the directory into one
  subdirectory per workflow

#### Scenario: One workflow with two stage orders qualifies a repeated role

- **WHEN** a single workflow owns two ordered stage orders in one directory and the
  bounded role name that fits the second stage order's coordination job is already
  used by the first
- **THEN** the second module's name qualifies that bounded role name with the stage
  order it serves
- **AND** it does not take a different bounded name that describes its job less
  accurately

#### Scenario: Role naming is reviewed rather than gated

- **WHEN** a migration assigns a coordination role name
- **THEN** the workflow boundary check confirms only that the declared role paths
  exist
- **AND** the change is responsible for the name being drawn from the bounded set,
  because no gate rejects an off-set name

#### Scenario: Novel coordination job extends the bounded set explicitly

- **WHEN** a workflow needs a coordination job outside the recorded role names
- **THEN** the change amends this specification to add the role name
- **AND** it does not overload an existing role name to fit

#### Scenario: Decomposition assigns roles rather than only splitting lines

- **WHEN** a large GTK adapter workflow is decomposed
- **THEN** each resulting module has one named role
- **AND** the facade module delegates to the others rather than implementing their
  work

#### Scenario: Role split does not add indirection layers

- **WHEN** role assignment would require a new trait, manager type, or crate to
  express
- **THEN** the split is expressed with plain modules and narrow owner references
  instead
- **AND** the existing prohibition on abstraction added solely to move code stands

#### Scenario: Line-count-only split is insufficient

- **WHEN** a file is split into siblings that each still mix narration,
  coordination, and policy
- **THEN** the decomposition does not satisfy the workflow readability convention
- **AND** the workflow's matrix row remains unmigrated
