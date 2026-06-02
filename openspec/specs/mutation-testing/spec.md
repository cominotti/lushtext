# mutation-testing Specification

## Purpose
Define LushText's mutation-testing lane so deterministic Rust logic, pull-request gates, full-scope runs, local developer commands, triage expectations, and the GTK widget-harness boundary stay explicit and reviewable.

## Requirements
### Requirement: Mutation Testing Configuration
The project SHALL provide checked-in mutation-testing configuration that defines the default mutation scope, exclusions, and test runner for deterministic LushText logic.

#### Scenario: Default scope targets deterministic logic
- **WHEN** a developer runs the standard mutation command without extra file filters
- **THEN** mutation testing examines model, service, and calibrated pure-helper code rather than broad GTK widget adapters or packaging scripts

#### Scenario: Exclusions are narrow and documented
- **WHEN** a mutant, function, or file is excluded from the default mutation scope
- **THEN** the exclusion MUST be as narrow as practical and MUST include a nearby reason explaining why it is equivalent, uninteresting, generated, or outside the supported mutation lane

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

#### Scenario: Missed mutant is actionable
- **WHEN** a mutant survives in code covered by the configured mutation scope
- **THEN** maintainers MUST prefer adding or tightening tests, strengthening assertions, or extracting deterministic logic before adding an exclusion

#### Scenario: Mutant is equivalent or outside useful scope
- **WHEN** maintainers determine that a mutant is equivalent, generated, UI-glue-only, or otherwise not useful to test
- **THEN** maintainers MUST exclude it only with a narrow documented exclusion

### Requirement: Widget Harness Boundary
The project SHALL keep compositor-driven widget behavior outside the first blocking mutation-testing scope.

#### Scenario: GTK adapter code needs stronger mutation coverage
- **WHEN** mutation testing reveals that important behavior lives only inside GTK adapter code
- **THEN** the implementation MUST either extract deterministic decision logic into model, service, or helper modules that can be covered by non-widget mutation testing, or document why the behavior must remain widget-only

#### Scenario: Widget mutation experiment is added later
- **WHEN** a future implementation adds mutation testing that directly exercises the widget harness
- **THEN** it MUST preserve the existing `scripts/run-widget-tests.sh` responsibilities for Mutter, D-Bus, renderer settings, retries, and warning filtering
