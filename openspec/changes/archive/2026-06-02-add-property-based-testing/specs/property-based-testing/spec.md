## ADDED Requirements

### Requirement: Property Test Dependency and Target
The project SHALL provide a property-based testing lane using `proptest` as a
test-only dependency and an explicit property-test target that is not part of
default test execution.

#### Scenario: Default tests exclude property target
- **WHEN** a developer runs the standard non-widget test command without enabling property tests
- **THEN** the property-test target is not compiled or executed

#### Scenario: Property target is explicitly enabled
- **WHEN** a developer runs the property-test command
- **THEN** the command enables the property-test feature and executes the dedicated property-test target

### Requirement: Bounded Property Runtime
The project SHALL bound property-test runtime with explicit case counts, input
sizes, shrink limits, and timeouts suitable for CI feedback.

#### Scenario: Pull request property run uses modest case counts
- **WHEN** property tests run in the pull-request CI lane
- **THEN** each property test uses a documented modest case count rather than an unbounded or default-deep generated-input run

#### Scenario: Generated input domains are bounded
- **WHEN** a property strategy generates strings, paths, vectors, Markdown fragments, or replacement lists
- **THEN** the strategy constrains size and shape so the property lane remains predictable

#### Scenario: Deeper run is opt-in
- **WHEN** maintainers want higher property-test case counts
- **THEN** they run a scheduled, manual, or explicitly configured deep property lane instead of increasing the default pull-request cost

### Requirement: Regression Persistence
The project SHALL preserve minimized failing property cases in a reproducible
form so generated failures become durable regression inputs.

#### Scenario: Property failure is minimized
- **WHEN** `proptest` finds and shrinks a failing input
- **THEN** the minimized failure is persisted or otherwise documented so the same case can be rerun

#### Scenario: Regression case remains reviewable
- **WHEN** a persisted property regression file is added or changed
- **THEN** the file location and purpose are documented for maintainers reviewing the change

### Requirement: Initial Property Coverage
The project SHALL add initial property tests for pure deterministic LushText
logic where useful invariants can be stated independently of GTK widgets,
watcher timing, or live filesystem sessions.

#### Scenario: Inline footnote invariants are covered
- **WHEN** generated Markdown snippets include bounded inline-footnote-like syntax
- **THEN** property tests verify that protected regions remain unchanged and generated labels do not collide with existing labels

#### Scenario: Search replacement invariants are covered
- **WHEN** generated replacement lists target bounded lines and ranges
- **THEN** property tests verify ordering, clipping, and no-panic behavior for deterministic replacement helpers

#### Scenario: Path rebasing invariants are covered
- **WHEN** generated old roots, new roots, and candidate paths are evaluated
- **THEN** property tests verify that only paths under the old root are rebased

#### Scenario: Palette ordering invariants are covered
- **WHEN** generated scored results are merged and truncated
- **THEN** property tests verify descending score order, max limits, and stable tie handling

#### Scenario: Encoding and sidecar invariants are covered
- **WHEN** generated encodings, line endings, or sidecar identity inputs are evaluated
- **THEN** property tests verify deterministic identifiers, separators, and hash behavior

### Requirement: Property Test Documentation
The project SHALL document how property tests relate to example tests, mutation
tests, widget tests, benchmarks, and dependency-policy checks.

#### Scenario: Developer reads property testing docs
- **WHEN** a developer opens the property-testing documentation
- **THEN** they can see how to run the property lane, how case counts are chosen, how failures are reproduced, and which code is intentionally out of scope

#### Scenario: Build rules mention property testing
- **WHEN** an agent or maintainer reads the project build rules
- **THEN** the property-test command and mutation-lane separation are documented alongside the other test gates

### Requirement: Property Test Scope Boundary
The project SHALL keep GTK widget behavior, compositor-driven UI behavior,
filesystem watcher timing, file chooser flows, and other live session behavior
outside the initial property-testing lane.

#### Scenario: Candidate property requires GTK session state
- **WHEN** a proposed property test would need a live GTK widget, compositor, D-Bus session, or file chooser
- **THEN** the behavior remains covered by the widget harness or is first extracted into deterministic helper logic
