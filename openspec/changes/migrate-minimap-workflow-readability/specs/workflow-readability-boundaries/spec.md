## MODIFIED Requirements

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
