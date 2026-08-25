## ADDED Requirements

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

### Requirement: Pure policy hoisted for tooling reach may move into its workflow

Pure decision logic that was placed in `model/` in order to obtain test or mutation
tooling reach, and that has a single owning workflow, SHALL be permitted to move
into that workflow's directory as a pure `policy.rs` module. The move MUST preserve
purity, MUST preserve behavior, and MUST preserve mutation coverage. Dependency
direction `ui -> services -> model` MUST remain intact, and the relocated module
MUST NOT acquire dependencies on GTK types.

#### Scenario: Relocation preserves purity and direction

- **WHEN** single-consumer pure policy moves from `model/` into its owning workflow
- **THEN** the relocated module still has no GTK-family imports
- **AND** `services/` and `model/` remain GTK-free and no new upward dependency is
  created

#### Scenario: Relocation preserves mutation coverage

- **WHEN** pure policy relocates out of `model/`
- **THEN** the same mutants are generated and killed after the move as before
- **AND** the change records that parity as evidence

#### Scenario: Multi-consumer policy is not relocated

- **WHEN** pure policy has several genuinely unrelated consumers
- **THEN** it remains in its shared location
- **AND** the workflow readability matrix records it as cross-cutting
