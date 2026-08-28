## MODIFIED Requirements

### Requirement: Mutation Testing Configuration
The project SHALL provide checked-in mutation-testing configuration that defines the default mutation scope, exclusions, and test runner for deterministic LushText logic. The scope SHALL identify pure decision logic by naming convention rather than by hand-listed file paths, so that pure policy keeps mutation coverage wherever its owning workflow lives.

A hand-listed scope entry naming a specific pre-convention adapter file is a
path-keyed scope entry, and it survives only until that file's owning workflow
migrates. When a migration moves the workflow's pure decision logic into a module
the naming convention reaches, the migration SHALL retire the hand-listed entry
rather than re-point it, and SHALL prove that the default scope did not narrow —
by showing which mutants the convention now generates for the relocated logic, and
by accounting for any mutants the retired entry used to generate that the change
deliberately places outside the lane. A hand-listed entry left pointing at a path
that no longer exists silently removes its file from the scope while the mutation
command still succeeds, so its correctness MUST be established by running the tool
rather than by reading the configuration.

Exclusions SHALL be re-verified rather than inherited. An `exclude_re` entry
anchored to a literal `line:column`, or to a symbol name, is stale-prone: source
edits move line numbers and rename or delete symbols, and neither change makes the
entry fail. Whenever a change touches a file that such an entry names, the change
SHALL re-verify each of that file's entries against a mutant the tool actually
generates, and SHALL delete every entry that matches nothing. A stale exclusion is
not inert: it is a recorded equivalence claim that no longer protects the mutant it
describes, so the mutant it was written for either survives unexplained or falls
under a different entry's reach without anyone deciding that it should.

#### Scenario: Default scope targets deterministic logic
- **WHEN** a developer runs the standard mutation command without extra file filters
- **THEN** mutation testing examines model code, service code, and pure policy modules identified by the workflow policy naming convention, rather than broad GTK widget adapters or packaging scripts

#### Scenario: Pure policy is in scope wherever it lives
- **WHEN** a workflow's pure decision logic lives in a `policy.rs` module inside a UI workflow directory
- **THEN** the default mutation scope examines it through the naming convention
- **AND** no hand-listed file path is required to include it

#### Scenario: GTK adapters stay out of scope without hand-listed method exclusions
- **WHEN** a workflow's pure policy is separated from its GTK adapter by module
- **THEN** the adapter is out of scope because it is not a policy module
- **AND** the configuration does not need `exclude_re` entries enumerating adapter method names

#### Scenario: Exclusions are narrow and documented
- **WHEN** a mutant, function, or file is excluded from the default mutation scope
- **THEN** the exclusion MUST be as narrow as practical and MUST include a nearby reason explaining why it is equivalent, uninteresting, generated, or outside the supported mutation lane

#### Scenario: Hand-listed scope entry retires with its workflow's migration
- **WHEN** a workflow named by a hand-listed `examine_globs` entry migrates and its
  pure decision logic moves into a module the naming convention reaches
- **THEN** the migration deletes the hand-listed entry rather than re-pointing it
  at the workflow's new path
- **AND** the change shows the mutants the convention now generates for the
  relocated logic

#### Scenario: Retiring a scope entry accounts for the mutants it used to generate
- **WHEN** retiring a hand-listed scope entry removes an adapter file from the
  default scope
- **THEN** the change states which mutants that file used to generate and why the
  behavior they covered is now covered by the relocated pure policy or by a
  documented non-mutation lane
- **AND** the accounting is measured from the tool rather than asserted

#### Scenario: Scope entry pointing at a moved path is a silent regression
- **WHEN** a change renames or splits a file named by a hand-listed scope entry and
  leaves the entry unchanged
- **THEN** the change is incomplete even though the mutation command reports
  success
- **AND** the narrowed scope MUST NOT be recorded as accepted debt

#### Scenario: Line-anchored exclusion is re-verified when its file is touched
- **WHEN** a change edits a file named by an `exclude_re` entry anchored to a
  literal `line:column` or to a symbol name
- **THEN** the change re-verifies each of that file's entries against a mutant the
  tool actually generates
- **AND** the verification uses the tool's own mutant listing rather than a source
  text search

#### Scenario: Exclusion matching nothing is deleted rather than carried
- **WHEN** re-verification finds an `exclude_re` entry that matches no generated
  mutant
- **THEN** the entry is deleted
- **AND** the change states whether the mutant it was written for still exists,
  and triages it in the documented order if it does
