## MODIFIED Requirements

### Requirement: Decomposed workflow modules carry named roles

Decomposition of a GTK adapter workflow SHALL assign each resulting module one
named role: narrative facade, seam value objects, pure policy, coordination, or
evidence. A module MUST NOT combine the facade role with the coordination or policy
role. Splitting a large file into sibling modules without assigning roles does not
satisfy this requirement.

That five-name taxonomy is **closed, and it applies to the workflow's role
modules**. A module that only projects the migrated workflow onto widgets — GTK
subclass state and template children, list-factory row projection, context-menu
and gesture lifecycle, row accessibility projection, or a per-surface capture
adapter a canonical role home calls — is a **called presentation surface**. A
called presentation surface is **not a role**: it is outside the set of
decomposed workflow modules this requirement governs, it MUST NOT be assigned one
of the five role names, and it MUST NOT own a `policy.rs` or an `evidence.rs`.
Its ownership SHALL be recorded in its own module doc and named in the workflow's
matrix row, which is where the project already records such a surface, and it
keeps every behavior obligation any other requirement places on it. This states
the scope of the taxonomy rather than adding a sixth role: the five role names
and the bounded coordination set are unchanged.

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

Before a migration amends the bounded set for a pre-convention module that no
listed role name describes, it SHALL first determine whether that module is **one
coordination job at all**. A role name names a coordination *job*; it never names
a module's pre-convention topic. Where the module is not one job — where its
contents separate into pure decisions, seam value objects, evidence fields, and
one or more coordination jobs that existing role names already describe — the
migration SHALL **dissolve** it across those existing roles rather than adding a
role name for its topic, and SHALL record each part's destination in the
workflow's matrix row. Escalation by amendment remains available and remains
required for a genuinely novel *single* coordination job; it is not the response
to a module that merely fails to be one. Dissolution is not a licence to scatter:
each part still lands in exactly one role, and a part with no role destination
means the module was not fully understood rather than that a new role name is
needed.

The stage-order qualification above applies to modules a migration **creates or
renames**. A module that already carries a correct bounded role name SHALL NOT be
renamed or qualified for symmetry with newly named siblings, because renaming a
stable correct module is churn that a reader must diff to understand.

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

Those role homes MAY be **nested**. Where one workflow owns a directory and a
widget subdirectory of that directory, the workflow SHALL name one of them its
canonical role home, holding the facade, the single `policy.rs`, and the single
`evidence.rs`, while modules in the other directory take bounded coordination role
names or are recorded as called presentation surfaces carrying no role.
This is the nested case of the resolution already used for a workflow spanning two
sibling directories — one canonical role home plus recorded called surfaces — and
it changes nothing else: the workflow still owns exactly one `policy.rs` and one
`evidence.rs`, and the `ui/**/policy.rs` mutation scope reaches either location,
which a migration MUST verify after the move rather than assume.

#### Scenario: Widget-projection module is a called surface rather than a role

- **WHEN** a migrated workflow owns a module that only projects it onto widgets,
  such as subclass state, list-factory row projection, context-menu lifecycle,
  row accessibility projection, or a per-surface capture adapter
- **THEN** that module is recorded as a called presentation surface, is not
  assigned one of the five role names, and owns no `policy.rs` and no
  `evidence.rs`
- **AND** its ownership is stated in its own module doc and named in the
  workflow's matrix row, and the five-name role taxonomy is unchanged

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

#### Scenario: One workflow owning a directory and its widget subdirectory

- **WHEN** one workflow owns a directory and a widget subdirectory of it, and both
  contain modules that coordinate its ordered stages
- **THEN** the workflow names one of them its canonical role home for the facade,
  the single `policy.rs`, and the single `evidence.rs`, and every module in the
  other directory declares a bounded coordination role name or is recorded as a
  called presentation surface carrying no role
- **AND** the migration verifies that the `ui/**/policy.rs` mutation scope still
  reaches the policy module at its chosen location

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

#### Scenario: Pre-convention module that is not one job is dissolved rather than named

- **WHEN** a migrating workflow holds a pre-convention module that no bounded role
  name describes, and the module's contents separate into pure decisions, seam
  value objects, evidence fields, or more than one coordination job that existing
  role names already describe
- **THEN** the migration dissolves that module across those existing roles instead
  of amending the bounded set to name its topic
- **AND** the workflow's matrix row records each part's destination, and the
  bounded coordination set is unchanged

#### Scenario: Cohesion is checked before the bounded set is amended

- **WHEN** a migration is about to propose a new coordination role name for a
  module that fits no listed name
- **THEN** it first records whether that module is one cohesive coordination job
- **AND** an amendment is proposed only for a genuinely novel single job, so a
  module that is simply not one job is not the reason the closed taxonomy grows

#### Scenario: An already correctly named coordination module is not renamed for symmetry

- **WHEN** a migration creates or renames coordination modules beside a sibling
  that already carries a correct bounded role name
- **THEN** that sibling keeps its name and is not qualified with a stage order for
  symmetry
- **AND** the migration records it as already correct rather than leaving a reader
  to diff an unexplained rename

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
