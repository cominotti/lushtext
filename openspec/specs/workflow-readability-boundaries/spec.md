# workflow-readability-boundaries Specification

## Purpose
Define LushText's workflow readability convention: the completion matrix that enumerates every workflow, the narrative facade a migrated workflow presents, the reified seam value objects that carry identity and freshness across workflow boundaries, the co-location and purity rules for pure workflow policy, intent-first boundary naming, risk-ordered vertical migration slices, retroactive amendment, and the standing guidance that must stay consistent with it.
## Requirements
### Requirement: Every workflow is enumerated before any workflow is migrated
The project SHALL maintain `docs/workflow-readability-matrix.md` as the completion
source of truth for workflow readability migration. Every LushText workflow MUST
have a row with a stable row id, its current file set, the roles it must gain to
satisfy the convention, its seam value object, its consumer count for any pure
policy it owns, its risk tier, and its migration status. The matrix MUST be
complete for all workflows before the first workflow is migrated, so that outliers
are classified before the convention becomes normative.

A row MUST NOT restate the target file set as a literal file list. The target file
set is fully derivable from the role naming convention, so duplicating it per row
creates a second source of truth that can drift from the convention.

A row's **measured** cells — its current size, its per-kind test seam counts, and
the consumer count for any pure policy it owns — are census estimates until the
row's migration re-derives them. A migrating workflow SHALL re-derive those cells
from the code and correct them in the same change, rather than inheriting the
census figures or reporting against them. Re-derivation MUST be **row-scoped**: it
counts only what the workflow owns, and it MUST NOT pool shared service files,
cross-cutting modules, or neighbouring files the workflow merely calls. Size
figures MUST count production lines, excluding `#[cfg(test)]` modules, and MUST
name any shared population that the census cell had pooled so a later slot reading
from the other side does not re-derive it as its own. A correction MAY move a
figure in either direction, and a change MUST NOT treat an unchanged cell as the
expected outcome.

#### Scenario: Census precedes exemplar migration
- **WHEN** the exemplar workflow migration begins
- **THEN** the matrix already contains a row for every workflow, including
  unmigrated ones
- **AND** each row names the roles it must gain, its seam value object, and its
  risk tier

#### Scenario: Workflow with no seam value object is still a complete row
- **WHEN** a workflow has no field bundle crossing two or more boundaries, because
  it is synchronous or delegates its seam to another workflow
- **THEN** the row records that no value object is required together with the
  evidence for that conclusion
- **AND** the row counts as complete rather than unresolved

#### Scenario: Known outlier is classified explicitly
- **WHEN** the census reaches a workflow that may not fit the convention, such as
  the minimap adapter, a policy module with several unrelated consumers, or a
  workflow decomposed by an earlier change
- **THEN** the row records it as conforming, exempt, or deferred with a stated
  reason
- **AND** a later migration change MUST NOT silently force an exempt workflow into
  the convention

#### Scenario: Migrated workflow without a row fails policy
- **WHEN** a workflow is migrated to the convention but has no matrix row, or its
  row claims evidence that does not exist
- **THEN** `make check-policy` fails
- **AND** the failure names the workflow and the missing row or evidence

#### Scenario: Migration re-derives its row's measured cells
- **WHEN** a workflow is migrated
- **THEN** the change re-derives the row's size, per-kind seam counts, and pure
  policy consumer count from the code and corrects the cells in the same change
- **AND** it does not report its migration against the census figures it replaced

#### Scenario: Re-derivation is row-scoped and excludes co-located tests
- **WHEN** a migration measures its row's size or seam population
- **THEN** it counts only the files the workflow owns, excluding shared services,
  cross-cutting modules, and neighbouring files the workflow only calls
- **AND** size figures count production lines rather than `#[cfg(test)]` modules

#### Scenario: A pooled census figure names the population it shared
- **WHEN** a corrected cell replaces a figure that had pooled seams or lines
  shared with another workflow
- **THEN** the correction names the shared population and the rows that share it
- **AND** a later migration of one of those rows can re-derive its own share
  without rediscovering the pooling

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

### Requirement: Field bundles crossing workflow seams are reified value objects
The project SHALL represent a workflow's identity, freshness, and intent fields as
named value objects when the bundle crosses two or more function boundaries or is
reconstructed at two or more call sites. Such a bundle MUST be constructed once at
the workflow entry point and validated as a unit. A field MUST NOT be renamed while
crossing a seam.

The project already expresses this shape as a captured-expectation value, an
observed-live-state value, and one predicate that validates the two together. A
migration MUST reuse that established shape for its seam rather than introduce a
parallel one, so the codebase does not accumulate several ways to express the same
freshness check. Where a workflow's coordinator already owns the generation and
exposes a currency predicate, that coordinator is the seam value object and no
additional type is required.

#### Scenario: Migration reuses the established seam shape
- **WHEN** a migration reifies a freshness or identity bundle
- **THEN** it expresses that bundle in the shape the codebase already uses for the
  same purpose
- **AND** it does not introduce a second, differently shaped convention for the same
  kind of check

#### Scenario: Freshness tuple is validated as a unit
- **WHEN** a workflow needs to decide whether a queued or completing operation is
  still current
- **THEN** it validates one value object rather than comparing several loose
  parameters
- **AND** the validation lives with the value object rather than being duplicated
  at each seam

#### Scenario: Cross-seam rename becomes unrepresentable
- **WHEN** a value that means one thing is passed to a boundary that names it
  something else
- **THEN** the reified value object makes that call a type error
- **AND** the workflow cannot compile until the intent is named consistently

#### Scenario: Local helper does not require a value object
- **WHEN** a bundle of parameters is used by exactly one private helper and is not
  reconstructed elsewhere
- **THEN** no value object is required
- **AND** the convention does not force reification of every long signature

#### Scenario: Argument-count suppression signals an unreified seam
- **WHEN** workflow code carries `#[expect(clippy::too_many_arguments)]` at a
  boundary that crosses modules
- **THEN** that boundary is treated as an unreified seam to be fixed
- **AND** it is not accepted as a standing exception

#### Scenario: Workflow code reaches zero argument-count suppressions
- **WHEN** the residual sweep completes
- **THEN** no `#[expect(clippy::too_many_arguments)]` remains in workflow adapter or
  coordination code
- **AND** the sweep asserts that zero rather than maintaining an allowlist of
  accepted workflow exceptions

#### Scenario: Domain catalog construction is outside the seam rule
- **WHEN** a domain module builds static catalog rows whose parameters each name a
  documented external contract field
- **THEN** that constructor is not a workflow seam and the zero assertion does not
  cover it
- **AND** its suppression MUST carry a reason naming the contract it enumerates

### Requirement: Pure workflow policy is co-located with its consumer and stays pure
The project SHALL place a workflow's pure decision logic in a `policy.rs` module
inside that workflow's own directory when the policy has a single owning workflow.
A `policy.rs` module MUST NOT import `gtk4`, `glib`, `gio`, `libadwaita`, or
`sourceview5`. Policy with several genuinely unrelated consumers MUST remain in a
shared location, and the matrix MUST record it as cross-cutting.

Relocation eligibility SHALL be decided by the number of **owning workflows**, not
by the number of consuming files. Pure policy whose only consumer is its own
coordination adapter is cross-cutting when that adapter serves several workflows,
and MUST NOT be relocated beneath any one of them. Shared coordination that encodes
LushText-specific budget, admission, or retirement policy MUST NOT be treated as
generic toolkit machinery for extraction into a separate reusable crate.

A workflow whose pure decision logic is **entirely** cross-cutting therefore owns
no `policy.rs`, and such a workflow SHALL still count as a complete migrated row.
Its matrix entry declares no pure policy role and names the cross-cutting module
and the other owning workflows that keep it shared. This mirrors the treatment of
a workflow with no qualifying seam bundle: the absence is a recorded conclusion
with its evidence, not an unmet obligation. A migration MUST NOT manufacture a
`policy.rs` for such a workflow by copying, forking, or re-implementing part of
the cross-cutting module beneath it, and MUST NOT duplicate a shared limit or
shared arithmetic to obtain a local policy module.

#### Scenario: Single consuming file is not sufficient grounds to relocate
- **WHEN** pure policy has exactly one consuming file, and that file is the
  coordination adapter that many workflows call
- **THEN** the policy is recorded as cross-cutting and stays in its shared location
- **AND** the matrix records the breadth of the adapter's consumers as the reason

#### Scenario: Single-consumer policy moves beside its workflow
- **WHEN** pure policy in `model/` has exactly one owning workflow
- **THEN** it moves to that workflow's `policy.rs`
- **AND** it remains free of GTK, GLib, GIO, Libadwaita, and SourceView imports

#### Scenario: Cross-cutting policy does not move
- **WHEN** pure policy is consumed by several unrelated workflows
- **THEN** it stays in its shared location
- **AND** the matrix records it as cross-cutting with its consumer list

#### Scenario: Workflow with only cross-cutting policy is still a complete row
- **WHEN** a migrated workflow's pure decision logic is entirely cross-cutting, so
  it owns no `policy.rs`
- **THEN** the row is complete with no pure policy role declared, naming the
  cross-cutting module and the other owning workflows that keep it shared
- **AND** the migration does not fork or re-implement part of that module beneath
  the workflow to manufacture a local policy file

#### Scenario: Shared arithmetic is called rather than duplicated
- **WHEN** two workflows need the same pure limit, threshold, or arithmetic that a
  cross-cutting module owns
- **THEN** both call the cross-cutting owner
- **AND** neither copies it into its own `policy.rs`, because a forked shared limit
  can drift while both copies still read as correct

#### Scenario: Policy purity is mechanically enforced
- **WHEN** a `policy.rs` module gains a GTK-family import
- **THEN** `make check-policy` fails
- **AND** the failure names the file and the disallowed import

#### Scenario: Domain layer keeps only domain concepts
- **WHEN** the census and exemplar are complete
- **THEN** modules remaining in `model/` name domain concepts or recorded
  cross-cutting policy
- **AND** no module is placed in `model/` solely to obtain test or mutation
  tooling reach

### Requirement: Workflow boundaries are named for intent, not mechanism
Public, `pub(crate)`, `pub(super)`, and cross-module workflow operations SHALL be
named for the workflow intent they express. Names whose meaning depends on knowing
the coordination mechanism MUST be renamed at these boundaries. Private helpers
inside a coordination module MAY keep mechanism names when the owning module makes
the mechanism obvious.

#### Scenario: Cross-module operation is renamed for intent
- **WHEN** a cross-module operation is named after the mechanism it happens to use
  rather than the workflow step it performs
- **THEN** the migration renames it to the workflow intent
- **AND** the mechanism remains documented in the module that owns it

#### Scenario: Mechanism name is acceptable inside its owning module
- **WHEN** a private helper inside a coordination module refers to that module's own
  mechanism
- **THEN** the mechanism name is acceptable
- **AND** no rename is required

### Requirement: Migration proceeds in vertical slices ordered by risk
The project SHALL migrate workflows one workflow at a time, landing that workflow's
facade, value objects, policy, and evidence together. Migration order MUST proceed
from lower to higher risk, and workflows that persist user data MUST NOT be
migrated before the convention has been proven on at least two lower-risk
workflows.

#### Scenario: A workflow migrates as a complete slice
- **WHEN** a migration change lands
- **THEN** the affected workflow is readable end to end under the convention
- **AND** the change does not leave that workflow half-converted

#### Scenario: Partial programme remains coherent
- **WHEN** the programme has migrated some workflows and not others
- **THEN** the matrix states which workflows follow the convention
- **AND** unmigrated workflows remain behaviorally unchanged

#### Scenario: User-data workflow waits for two proofs
- **WHEN** a workflow persists drafts, sessions, local history, or document
  content
- **THEN** its migration follows at least two completed lower-risk migrations
- **AND** the matrix records the prerequisite

### Requirement: Convention amendments are applied retroactively
When a later change amends this convention, that change SHALL re-migrate every
already-migrated workflow to the amended shape within the same change. The project
MUST NOT leave two generations of the convention coexisting in the tree.

#### Scenario: Amendment re-migrates earlier workflows
- **WHEN** a migration reveals that the convention must change
- **THEN** the amending change updates the specification and re-migrates all
  previously migrated workflows
- **AND** the matrix reflects the amended shape for every migrated row

#### Scenario: Amendment cannot be deferred as debt
- **WHEN** an amendment is proposed without re-migrating earlier workflows
- **THEN** the change is incomplete
- **AND** it MUST NOT be archived as accepted debt

### Requirement: Standing guidance stays consistent with the convention
The project SHALL keep `AGENTS.md`, `README.md`, `.agents/rules/*.md`, and
maintained skill documents consistent with this convention. A standing instruction
that contradicts the convention MUST be amended in the same change that introduces
or changes the convention. `make check-agent-docs` MUST pass with the revised
guidance.

Consistency SHALL extend beyond prose to **mechanical gates keyed on literal file
paths**. A gate is path-keyed when a checked-in configuration file, policy script,
or policy implementation selects the files it protects by naming them literally —
by exact path equality, by an explicit entry in a scope list, or by a literal
`line:column` anchor — rather than by a naming convention the migrated shape still
satisfies. Where a migration relocates, renames, or splits a file that a path-keyed
gate names, that migration SHALL re-key or retire the gate's entry in the same
change.

Three properties make this obligation different from updating documentation, and
each is normative:

- **Every implementation of the same predicate is re-keyed.** Where one policy
  decision is implemented more than once — for example in a script and in a
  compiled policy tool — leaving one implementation keyed on the old path leaves
  the two disagreeing about which files a gate protects, which is worse than
  either answer alone.
- **The re-keying is proved by running the gate against the final state, not by
  reading the patch.** A path-keyed gate that no longer matches any file does not
  fail; it passes while protecting nothing. Reviewing the edit cannot distinguish
  a correct re-key from a silent disarm, so the migration SHALL run the gate
  against the tree it ships and show that the protected files are still selected
  and the required evidence is still demanded.
- **Retiring an entry is a permitted outcome, and it is stated as such.** Where a
  path-keyed entry existed only because pre-convention code sat outside a naming
  convention, and the migration moves that code inside the convention, the correct
  result is to delete the entry rather than re-point it. The migration SHALL record
  which outcome it chose and why.

Re-keying a path-keyed gate SHALL NOT weaken it. Broadening a predicate to match
files it did not previously protect, or narrowing it so that a file it protected
falls out, is a scope change that MUST be justified on its own terms rather than
carried as a side effect of a rename.

#### Scenario: Contradicting rule is amended with the convention
- **WHEN** the convention permits or requires something a standing rule forbids
- **THEN** that rule is amended in the same change
- **AND** the amended rule distinguishes the permitted case from the case it was
  originally protecting against

#### Scenario: Coordination vocabulary is presented beneath domain vocabulary
- **WHEN** guidance introduces the coordination vocabulary such as admission,
  budget, coordinator, ledger, retirement, continuation, and generation counter
- **THEN** it presents that vocabulary as an implementation tier reached from a
  workflow
- **AND** a reader learns the workflow's domain vocabulary before the coordination
  vocabulary

#### Scenario: Skills point at relocated policy
- **WHEN** pure policy relocates during a migration
- **THEN** skills and rules referencing its former location are updated in the same
  change
- **AND** no maintained guidance references a path that no longer exists

#### Scenario: Path-keyed gate is re-keyed by the migration that moves its file
- **WHEN** a migration relocates, renames, or splits a file that a checked-in
  mechanical gate selects by literal path
- **THEN** the migration re-keys or retires that gate's entry in the same change
- **AND** the change records which of the two outcomes it chose and why

#### Scenario: Every implementation of one path predicate is re-keyed together
- **WHEN** the same path-keyed policy decision is implemented in more than one
  place, such as a policy script and a compiled policy tool
- **THEN** the migration re-keys every implementation in the same change
- **AND** the change does not leave two implementations disagreeing about which
  files the gate protects

#### Scenario: Re-keying is proved by running the gate, not by reading the edit
- **WHEN** a migration re-keys a path-keyed gate
- **THEN** it runs that gate against the final state of the tree it ships
- **AND** the run shows that the relocated files are still selected and that any
  evidence the gate required of them is still demanded

#### Scenario: A gate left keyed to a moved path is a silent regression
- **WHEN** a migration moves a file and leaves a gate keyed to the old path
- **THEN** the change is incomplete even though the gate reports success
- **AND** the loss of protection MUST NOT be recorded as accepted debt

#### Scenario: Path-keyed entry retires when the convention reaches the code
- **WHEN** a path-keyed entry existed only to include code that sat outside a
  naming convention, and the migration moves that code inside the convention
- **THEN** the migration deletes the entry rather than re-pointing it at the new
  path
- **AND** it verifies that the naming convention now selects the code the entry
  used to select

