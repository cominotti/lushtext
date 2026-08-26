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
job the module performs: `admission`, `execution`, `retirement`, `watch`, and
`journal`. The convention MUST NOT fix one single coordination file name, because a
workflow may own more than one coordination job and a directory may host more than
one workflow. A coordination job that no existing role name describes MUST be added
to the bounded set by amending this specification, rather than reusing an ill-fitting
name or inventing an unlisted one.

`journal` names the coordination job of maintaining a durable, generation-guarded
record that a later stage of the same workflow reads back: admitting the mutations
that may touch it, installing and clearing it under a freshness guard, writing and
deleting it on a worker, recovering it at startup with stale-record cleanup, and
handing it back to the stage that consumes it. Admission is part of this role
rather than a separate one: the mutual-exclusion gate that serializes the
workflow's writes to the record, and any resource reservation those writes take,
exist only to protect that record and SHALL live with it. It is distinct from
`retirement`, which destroys a payload the workflow is finished with, and from
`execution`, which performs the workflow's primary work.

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

Those two file names are fixed, so they cannot be shared by two workflows that
live in the same directory, and a workflow-prefixed variant is not an available
substitute for pure policy: the default mutation scope reaches pure policy through
the literal `ui/**/policy.rs` convention, so a prefixed policy file would leave the
scope, which the mutation-testing capability classifies as a coverage regression
that blocks the relocation. Therefore, where a directory hosts more than one
workflow, a migrated workflow's roles MAY live in a **per-workflow subdirectory**
of that directory, whose `mod.rs` is the workflow's facade and whose role files
keep the unqualified names `policy.rs`, `evidence.rs`, and the unqualified bounded
coordination names. The subdirectory is named for the workflow in its own domain
vocabulary. This is a permitted role home, not a required one: a workflow whose
role file names do not collide with a sibling workflow's MAY keep flat,
workflow-scoped role names in the shared directory, and migration still MUST NOT
require restructuring an entire directory into one subdirectory per workflow.
Choosing between the two homes is a per-workflow decision recorded in the
workflow's matrix row, and it does not change any other part of this contract.

#### Scenario: Coordination file is named for its job

- **WHEN** a workflow's coordination module is created or renamed during migration
- **THEN** its file name states the coordination job it performs
- **AND** a name that only says the module is runtime machinery is not sufficient

#### Scenario: One directory hosting several workflows keeps flat role names

- **WHEN** a directory hosts more than one workflow
- **THEN** each workflow's coordination modules use role names scoped to that
  workflow within the shared directory
- **AND** migration does not require restructuring the whole directory into one
  subdirectory per workflow

#### Scenario: Two workflows in one directory cannot share the fixed role names

- **WHEN** a directory hosts more than one workflow and more than one of those
  workflows owns pure policy or an evidence surface
- **THEN** one of them moves its roles into a per-workflow subdirectory of that
  directory whose `mod.rs` is that workflow's facade
- **AND** its pure policy is still a file named `policy.rs`, so it stays inside the
  default mutation scope rather than being renamed to a prefixed variant that
  leaves it

#### Scenario: Role files inside a per-workflow subdirectory stay unqualified

- **WHEN** a workflow's roles live in a per-workflow subdirectory
- **THEN** the role files keep the unqualified names `policy.rs`, `evidence.rs`,
  and the unqualified bounded coordination names
- **AND** the subdirectory name, not a file-name prefix, is what scopes them to the
  workflow

#### Scenario: One workflow with two stage orders qualifies a repeated role

- **WHEN** a single workflow owns two ordered stage orders in one directory and the
  bounded role name that fits the second stage order's coordination job is already
  used by the first
- **THEN** the second module's name qualifies that bounded role name with the stage
  order it serves
- **AND** it does not take a different bounded name that describes its job less
  accurately

#### Scenario: Durable generation-guarded record is a journal role

- **WHEN** a workflow's coordination module installs, persists, recovers, and hands
  back a durable record under a freshness guard so a later stage can restore from it
- **THEN** that module takes the `journal` role name
- **AND** it is not named `retirement`, which destroys payloads rather than
  preserving them for restoration

#### Scenario: The gate protecting a journal belongs to the journal

- **WHEN** a workflow serializes the mutations of its durable record behind a
  mutual-exclusion gate, or reserves a resource budget for them
- **THEN** that gate and reservation live in the same `journal` module as the record
  they protect
- **AND** they are not split into a separate `admission` module, because a job whose
  only purpose is protecting one durable record does not justify its own role

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
