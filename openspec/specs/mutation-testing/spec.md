# mutation-testing Specification

## Purpose
Define LushText's mutation-testing lane so deterministic Rust logic, pull-request gates, full-scope runs, local developer commands, triage expectations, and the GTK widget-harness boundary stay explicit and reviewable.
## Requirements
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

### Requirement: Pull Request Mutation Gate
The project SHALL run changed-code mutation testing for pull requests after the non-widget test suite is known to pass.

#### Scenario: Pull request changes mutation-scoped Rust code
- **WHEN** a pull request changes Rust code inside the configured mutation scope
- **THEN** CI MUST run cargo-mutants against the pull-request diff using the non-widget nextest test surface

#### Scenario: Pull request has no relevant mutants
- **WHEN** a pull request does not contain mutation-scoped changes or cargo-mutants finds no mutants in the diff
- **THEN** CI MUST complete the mutation job without failing solely because there were no relevant mutants to test

#### Scenario: Mutation output is preserved
- **WHEN** the pull-request mutation job completes, fails, or times out
- **THEN** CI MUST upload the generated `mutants.out` directory as an artifact when it exists

### Requirement: Full Scope Mutation Runs
The project SHALL provide scheduled or manually triggered full-scope mutation testing for the configured deterministic mutation scope.

#### Scenario: Full run is sharded
- **WHEN** full-scope mutation testing runs in CI
- **THEN** the run MUST split work into shards that use identical mutation arguments except for shard identity

#### Scenario: Full run depends on a passing baseline
- **WHEN** full-scope mutation testing skips per-shard cargo-mutants baseline execution
- **THEN** the workflow MUST first prove the non-widget test suite passes in the same workflow or through an explicit dependency

#### Scenario: Full run artifacts are reviewable
- **WHEN** a full-scope mutation shard completes, fails, or times out
- **THEN** CI MUST upload that shard's `mutants.out` directory as a distinct artifact when it exists

### Requirement: Developer Mutation Commands
The project SHALL expose local developer commands for smoke, changed-code, and configured full-scope mutation testing.

#### Scenario: Developer runs a smoke check
- **WHEN** a developer runs the mutation smoke command
- **THEN** the command MUST exercise a small bounded mutation target and verify that cargo-mutants, nextest, configuration, timeouts, and output handling work together

#### Scenario: Developer runs a local in-place mutation command
- **WHEN** a local mutation command uses in-place source mutation
- **THEN** the command MUST either require a clean worktree before starting or use a disposable checkout so user changes are not overwritten or mixed with mutant edits

### Requirement: Mutation Triage Policy
The project SHALL document how to triage missed, unviable, timeout, and excluded mutants.

A focused mutation run SHALL report the mutant classes its filter cannot exclude.
The mutation tool's name filter does not apply to every mutant kind — struct
field-deletion mutants are generated for the whole configured scope regardless of
the filter — so a run described as focused also runs that unfilterable floor. A
change reporting focused results SHALL state the floor explicitly, so the result
is not read as narrower than it was. Pre-existing survivors inside that floor
MUST NOT be attributed to the change under review. They MUST also NOT be
inherited indefinitely: the change that owns the file where they survive SHALL
triage them in the order this requirement already states — decide whether each
represents real missed behavior, then add or tighten deterministic tests, then
consider a small refactor that makes the behavior testable, and only then a
narrow documented exclusion.

#### Scenario: Missed mutant is actionable
- **WHEN** a mutant survives in code covered by the configured mutation scope
- **THEN** maintainers MUST prefer adding or tightening tests, strengthening assertions, or extracting deterministic logic before adding an exclusion

#### Scenario: Mutant is equivalent or outside useful scope
- **WHEN** maintainers determine that a mutant is equivalent, generated, UI-glue-only, or otherwise not useful to test
- **THEN** maintainers MUST exclude it only with a narrow documented exclusion

#### Scenario: Focused run reports the floor its filter cannot exclude

- **WHEN** a change reports mutation results from a filtered or focused run
- **THEN** it states the mutant classes the filter does not apply to and the count
  they contribute
- **AND** the focused result is not presented as if the filter had bounded the
  whole run

#### Scenario: Pre-existing survivors in the floor are not attributed to the change

- **WHEN** a focused run surfaces surviving mutants that pre-date the change under
  review
- **THEN** those survivors are reported as baseline rather than as regressions
  introduced by the change
- **AND** the change that owns the file where they survive triages them in the
  documented order rather than passing them on again

### Requirement: Widget Harness Boundary
The project SHALL keep compositor-driven widget behavior outside the first blocking mutation-testing scope.

#### Scenario: GTK adapter code needs stronger mutation coverage
- **WHEN** mutation testing reveals that important behavior lives only inside GTK adapter code
- **THEN** the implementation MUST either extract deterministic decision logic into model, service, or workflow policy modules that can be covered by non-widget mutation testing, or document why the behavior must remain widget-only

#### Scenario: Extraction target may be the owning workflow
- **WHEN** deterministic decision logic is extracted out of a GTK adapter for mutation coverage
- **THEN** it MAY be placed in that workflow's pure policy module instead of being hoisted into `model/`
- **AND** it MUST remain free of GTK-family imports

#### Scenario: Widget mutation experiment is added later
- **WHEN** a future implementation adds mutation testing that directly exercises the widget harness
- **THEN** it MUST preserve the existing `scripts/run-widget-tests.sh` responsibilities for Mutter, D-Bus, renderer settings, retries, and warning filtering

### Requirement: Policy relocation requires mutation parity evidence
When pure policy relocates between directories, the change SHALL demonstrate that
mutation coverage is unchanged. The evidence MUST show that the relocated logic
still generates mutants and that those mutants are still killed. A relocation whose
mutants are no longer generated MUST be treated as a coverage regression, not as an
acceptable consequence of the move.

Where one change both relocates existing pure policy and extracts new pure policy
out of a GTK adapter, the two results SHALL be reported **separately**. A
relocation has a before-count, so parity is a real claim that can fail; an
extraction out of an adapter has no before-count, so its result is a gain from
zero that cannot fail. Merging them into one aggregate figure lets a parity loss
disappear behind a gain, which this requirement exists to prevent. Each reported
figure SHALL name the exact command invocation and the file-level anchors it was
measured against.

#### Scenario: Relocation reports parity
- **WHEN** a change relocates pure policy
- **THEN** it records mutation results for the relocated logic before and after the
  move
- **AND** the counts of generated and killed mutants for that logic are unchanged

#### Scenario: Lost mutants block the relocation
- **WHEN** relocation causes the policy's mutants to fall outside the default scope
- **THEN** the change is incomplete until the scope convention or the module
  placement is corrected
- **AND** the loss MUST NOT be recorded as accepted debt

#### Scenario: Policy module outside scope reach fails policy checks
- **WHEN** a pure policy module exists at a path the default mutation scope cannot
  reach
- **THEN** `make check-policy` fails
- **AND** the failure names the unreachable module

#### Scenario: Parity and gain are reported separately

- **WHEN** one change both relocates existing pure policy and extracts new pure
  policy out of a GTK adapter
- **THEN** it reports the relocation's before/after parity and the extraction's
  gain from zero as separate figures, each naming its invocation and file-level
  anchors
- **AND** neither figure is merged into a single aggregate count in which a parity
  loss could be masked by a gain

### Requirement: Property Test Boundary
The default mutation-testing lane SHALL exclude the dedicated property-test
target so generated property cases are not multiplied by the number of mutants.

#### Scenario: Default mutation command omits property tests
- **WHEN** a developer runs the standard mutation command
- **THEN** cargo-mutants executes the normal deterministic non-widget test surface without enabling the property-test feature

#### Scenario: Pull request mutation command omits property tests
- **WHEN** the pull-request mutation workflow runs changed-code mutation
- **THEN** the workflow does not run the property-test target as part of the mutation job

#### Scenario: Property mutation opt-in is explicit
- **WHEN** maintainers intentionally want mutation testing to exercise a property test
- **THEN** they add a separate documented mutation mode or narrow opt-in rather than changing the default mutation lane implicitly

