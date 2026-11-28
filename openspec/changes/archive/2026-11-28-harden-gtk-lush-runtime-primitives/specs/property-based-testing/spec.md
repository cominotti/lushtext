# Delta Specification: property-based-testing

## MODIFIED Requirements

### Requirement: Property Test Dependency and Target
The project SHALL provide a `proptest` lane using the `property-tests` Cargo feature across configured pure crates (including `gtk-lush-settle`) so generated property suites stay excluded from default non-widget and mutation test runs and execute only under `make test-prop` / `make test-prop-deep`.

#### Scenario: Default tests exclude property target
- **WHEN** a developer runs the standard non-widget test command without enabling `property-tests`
- **THEN** neither workspace property targets nor `gtk-lush-settle` property tests are compiled or executed

#### Scenario: Property target is explicitly enabled
- **WHEN** a developer runs `make test-prop` (`CARGO_TEST_PROP` plus `--lib` for `gtk-lush-settle`)
- **THEN** the command enables `property-tests` and executes both workspace `--test properties` targets and `gtk-lush-settle` `--lib` property tests
