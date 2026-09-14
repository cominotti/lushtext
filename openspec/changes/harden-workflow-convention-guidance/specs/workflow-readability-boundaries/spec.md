## ADDED Requirements

### Requirement: Every module in a migrated role home is declared by its row
A migrated row's role home is where a reader goes to learn the workflow, so a
module sitting in it that the row never mentions is an unclassified file in the
one directory the convention claims to have classified. For every row whose status
is `migrated`, each `.rs` file in that row's role home SHALL be one of:

- the facade (`mod.rs`) or the GTK subclass state file (`imp.rs`);
- a fixed role name (`policy.rs`, `evidence.rs`, `seams.rs`, `test_policy.rs`);
- a bounded coordination role name, optionally stage-order-qualified; or
- named by a backticked repository path in that row's own matrix text, which
  classifies it as a called presentation surface or as a module that is neither a
  role nor a presentation surface.

A role home is the directory holding the row's declared facade, plus any
subdirectory of it that the row itself names through a declared role path — the
nested role home the convention already permits. Enumeration SHALL NOT recurse
into subdirectories the row does not name, so a workflow cannot be made
responsible for a neighbour's directory.

The declaration SHALL be **machine-readable**: a backticked path the check can
resolve. Prose that names a file by its bare stem, or by a brace expansion over
several stems, classifies the module for a human reader but leaves the check
unable to see it, and a check that cannot see a declaration cannot enforce one.

Declaring a file is the fix. Renaming a called presentation surface into a role
name it does not perform, or weakening the check so the file falls out of its
scope, is a false role claim or a silent disarm respectively, and neither is an
available answer.

#### Scenario: Undeclared module in a role home is a finding
- **WHEN** a `.rs` file sits in a migrated row's role home, carries no convention
  role name, and is named nowhere in that row's matrix text
- **THEN** the check fails, names the file and the row, and says to declare it as a
  called presentation surface or a coordination role in the matrix row

#### Scenario: Declared presentation surface passes
- **WHEN** the row's matrix text names that file by a backticked repository path
- **THEN** the check reports no finding for it
- **AND** the module's own doc carries the matching classification

#### Scenario: Enumeration does not reach an unnamed subdirectory
- **WHEN** a role home contains a subdirectory the row's declared roles never name
- **THEN** files in that subdirectory are outside this check for that row

#### Scenario: Renaming into a role is not a permitted fix
- **WHEN** an undeclared module is a called presentation surface
- **THEN** the change declares it in the row rather than giving it a bounded role
  name it does not perform

### Requirement: The externally reachable test-seam count is ratcheted
The matrix's `Measurement Definitions` section SHALL record the current count of
externally reachable `*_for_test` declarations — `pub fn` and `pub(crate) fn` whose
name ends `_for_test`, under `crates/lushtext-core/src` — and the mechanical check
SHALL recompute that count with the same predicate and fail when the actual count
**exceeds** the recorded figure.

A count **below** the recorded figure SHALL pass without a finding. The ratchet
exists to stop the shadow introspection API growing back after the convention
retired it; forcing the recorded figure down on every reduction would make routine
cleanups fail a gate and would invite a change to raise the figure to make the
error go away.

The failure message SHALL name both remedies and their order: extend the owning
workflow's evidence surface and delete the getter, which is the convention's
answer; or, deliberately, raise the recorded figure in the same change with a
stated reason, which makes the growth a reviewed decision rather than a drift.

`#[cfg(feature = "test-utils")]` attribute sites SHALL NOT be ratcheted. That
population legitimately rises as workflows gain gated evidence surfaces, so
ratcheting it would penalise the convention being followed. It is a different
measurement of a different set and MUST NOT be merged with the declaration count.

The self-test fixture SHALL exercise **every visibility the predicate
distinguishes** — at least one counted `pub fn`, at least one counted
`pub(crate) fn`, and at least one uncounted `pub(super) fn` — and SHALL assert the
resulting count rather than assume it. A fixture built from one visibility
produces the same verdicts under a narrowed predicate, so it proves the ratchet's
thresholds while leaving the population it counts unproven.

#### Scenario: Growth beyond the recorded figure fails
- **WHEN** the recomputed count of externally reachable `*_for_test` declarations
  exceeds the figure recorded in the matrix
- **THEN** the check fails, reports both counts, and names the two remedies

#### Scenario: Reduction passes without forcing the figure down
- **WHEN** the recomputed count is below the recorded figure
- **THEN** the check passes with no finding
- **AND** the change is not required to lower the recorded figure

#### Scenario: Deliberate growth is recorded rather than hidden
- **WHEN** a change genuinely needs more externally reachable seams
- **THEN** it raises the recorded figure in the same change with a stated reason

#### Scenario: Gated attribute sites are outside the ratchet
- **WHEN** a migrated workflow adds a `test-utils`-gated evidence surface
- **THEN** the resulting rise in `cfg(feature = "test-utils")` sites produces no
  finding

#### Scenario: Narrowing the predicate fails the self-test
- **WHEN** the counting predicate is narrowed so that `pub(crate) fn *_for_test`
  declarations stop being counted
- **THEN** the self-test fails on its own fixture's asserted count
- **AND** a `pub(super) fn *_for_test` declaration in the same fixture is still
  not counted

## MODIFIED Requirements

### Requirement: Standing guidance stays consistent with the convention
The project SHALL keep `AGENTS.md`, `README.md`, `.agents/rules/*.md`, and
maintained skill documents consistent with this convention. A standing instruction
that contradicts the convention MUST be amended in the same change that introduces
or changes the convention. `make check-agent-docs` MUST pass with the revised
guidance.

The convention SHALL have **one normative home**. Where the same obligation is
stated in several rule files and in `AGENTS.md`, a later change can amend one copy
and leave the others contradicting it, which is the drift this requirement exists
to prevent. The normative statement of the role taxonomy, the permitted role homes,
seam value objects, policy purity, measured-cell re-derivation, intent-first
naming, and the evidence-surface invariants SHALL live in one rule file scoped to
the paths that convention governs; other rule files and `AGENTS.md` SHALL carry
short pointers to it rather than parallel copies. Procedural material — the ordered
checklist for performing a migration, proof recipes, and gate ordering — belongs in
a maintained skill rather than in the normative rule, so the rule stays a statement
of what must be true.

Standing guidance SHALL also stay consistent with the **programme's state**, not
only with its content. Once the programme record's slot ledger declares no slot
outstanding, guidance MUST NOT instruct a reader to advance a slot, and MUST NOT
describe workflows as awaiting migration or as keeping pre-convention test seams
pending their slot. After closure the programme record is **frozen** except for its
deferral inventory, which MAY be appended; a change that alters its history is
rewriting a record later readers rely on.

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

#### Scenario: Convention has one normative home and the rest point at it
- **WHEN** the convention's obligations are stated in more than one guidance file
- **THEN** one path-scoped rule file holds the normative statement
- **AND** the other files carry pointers to it rather than parallel copies that
  can be amended independently

#### Scenario: Procedure lives in a skill, not in the normative rule
- **WHEN** guidance describes how to perform a migration step by step, prove an
  evidence surface, or order the gates
- **THEN** that material lives in a maintained skill
- **AND** the normative rule states what must be true rather than how to do it

#### Scenario: Closed programme is not described as running
- **WHEN** the programme record's slot ledger declares no slot outstanding
- **THEN** no standing guidance instructs a reader to advance a slot or describes
  a workflow as awaiting migration
- **AND** guidance that told unmigrated workflows to keep per-field test getters
  is corrected to the evidence-surface rule

#### Scenario: Closed programme record is appended, not rewritten
- **WHEN** a later change records guidance hardening or a new deferral for the
  closed programme
- **THEN** it appends to the record's deferral inventory
- **AND** it does not rewrite the record's baselines, slot ledger, or history

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
