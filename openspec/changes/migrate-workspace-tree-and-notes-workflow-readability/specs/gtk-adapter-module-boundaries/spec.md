## MODIFIED Requirements

### Requirement: Window notes are organized by existing workflows
The `ui/window/notes` module SHALL separate bookmark lifecycle/navigation, document and folder note editors, and notes-browser/command-palette coordination into focused sibling modules under one private notes facade. Existing window actions, callback routes, sidecar safety, migration behavior, menu availability, note previews, and target activation MUST remain unchanged.

This topical decomposition and its behavior obligations SHALL survive the
workflow's migration to the readability convention unchanged. Because that
convention requires every **role module** of a migrated workflow to carry exactly
one named role, a migrating workflow SHALL classify each focused sibling rather
than choose between the two requirements: the notes facade becomes the workflow's
narrative facade; a sibling that coordinates an ordered stage order takes a
bounded coordination role name, optionally qualified with the stage order it
serves; and a sibling that only projects the workflow onto widgets is a **called
presentation surface**, which is not a role and is therefore outside the role
taxonomy, owning no pure policy and no evidence surface, with its ownership
recorded in its own module doc and named in the workflow's matrix row. The
topical split is what the modules *do*; the role, where one applies, is what they
*are* to the workflow, and neither replaces the other.

#### Scenario: Bookmark workflow remains complete
- **WHEN** a bookmark is toggled, edited, navigated, browsed, previewed, saved, renamed, or activated after decomposition
- **THEN** the same bookmark service and editor/window paths are used
- **AND** debounce, generation, excerpt, accessibility, and failure behavior remain intact

#### Scenario: Document and folder notes remain complete
- **WHEN** a document or folder note is opened, edited, previewed, saved, discarded, or migrated after decomposition
- **THEN** the same target identity and durable sidecar workflow is used
- **AND** no note body or pending migration is lost

#### Scenario: Notes browser and palette stay synchronized
- **WHEN** note sources change while the Notes browser or command palette is active
- **THEN** existing generation and refresh rules still update the current surface
- **AND** stale async previews or source loads cannot replace newer state

#### Scenario: Notes state extremes remain readable
- **WHEN** there are no notes, one note, representative category rows, many notes, awkward paths, no active editor, or constrained geometry
- **THEN** the existing empty states, category structure, actions, focus, and item-region scrolling remain usable
- **AND** no fake note row is introduced by the module split

#### Scenario: Migration classifies focused siblings without renaming their topics
- **WHEN** this module's workflow is migrated to the workflow readability
  convention
- **THEN** each focused sibling is classified as the narrative facade, as a
  bounded optionally stage-order-qualified coordination role, or as a called
  presentation surface that carries no role — while the topical separation and
  every behavior obligation above stay unchanged
- **AND** the migration does not treat the topical requirement and the role
  requirement as being in conflict, nor rename a module to a role name that
  describes its job less accurately than its topic did

### Requirement: Workspace-section wiring has focused owners
The workspace-section implementation SHALL keep subclass state, template children, construction, and disposal in `imp.rs` while row factory projection, context-menu/gesture lifecycle, and row accessibility projection live in focused sibling modules. Setup, bind, unbind, and disposal MUST preserve signal cleanup, binding cleanup, object-data ownership, DnD behavior, inline rename, selection, expansion, and popup lifecycle.

As with the notes module above, this ownership split SHALL survive the owning
workflow's migration unchanged, and the migration SHALL classify each of these
modules: the subclass state, row factory, context-menu, and row accessibility
modules are **called presentation surfaces** of the migrated workflow — not
roles — owning no pure policy and no evidence surface, with their ownership
recorded in their own module docs and named in the workflow's matrix row, while
modules that coordinate ordered stages take bounded coordination role names.

#### Scenario: Recycled row clears old wiring
- **WHEN** a virtualized list item is unbound and rebound to another file-tree row
- **THEN** prior signal bags, bindings, context targets, accessibility state, DnD state, and inline-rename triggers are cleared before new projection
- **AND** the old item cannot receive callbacks through the recycled row

#### Scenario: Context menus keep keyboard and pointer parity
- **WHEN** a user opens file or workspace context menus by right-click, Menu, or Shift+F10
- **THEN** the same target and action set is presented
- **AND** click-away, action activation, rebuild, refresh, and disposal pop down stale surfaces exactly once

#### Scenario: Row accessibility tracks expansion and position
- **WHEN** rows are bound, expanded, collapsed, reordered, disabled, or recycled
- **THEN** accessible name, description, set position, expanded state, and disabled state remain synchronized
- **AND** expanded-state signal hooks are disconnected on unbind

#### Scenario: Sidebar state extremes preserve geometry
- **WHEN** a section has zero folders, one folder, many expanded rows, long names, reorder DnD, focused-folder mode, or constrained width
- **THEN** header controls remain visible and only the item region scrolls
- **AND** tree disclosure, no-horizontal-scrollbar, empty-state, and context-menu contracts remain unchanged

#### Scenario: Wiring modules are called presentation surfaces after migration
- **WHEN** the workflow owning this subtree is migrated
- **THEN** the subclass-state, row-factory, context-menu, and row-accessibility
  modules are recorded as called presentation surfaces owning no policy and no
  evidence, with their ownership in their own module docs and in the matrix row,
  and none of them is assigned a role name
- **AND** the workflow still owns exactly one `policy.rs` and one `evidence.rs`,
  and every behavior obligation above is preserved

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
