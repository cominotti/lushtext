## ADDED Requirements

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
