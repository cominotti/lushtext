## ADDED Requirements

### Requirement: Debug-only assertions never own required side effects
The workspace SHALL deny clippy::debug_assert_with_mut_call across all targets and features. Required mutations MUST execute unconditionally and debug assertions MAY only observe captured values; the lint MUST have a zero-count blocking-candidate policy and MUST NOT be suppressed by a broad allow or local exception.

#### Scenario: A mutation result needs a debug assertion
- **WHEN** code mutates a budget, collection, counter, ownership guard, or workflow state and wants to verify the result
- **THEN** the mutation executes before the assertion and its returned value is captured
- **AND** the debug assertion inspects the captured value without changing state

#### Scenario: Blocking lint runs
- **WHEN** the workspace Clippy gate runs across all targets and features
- **THEN** any side-effectful debug assertion fails the gate
- **AND** release behavior cannot silently omit the mutation

#### Scenario: Advisory policy is inspected
- **WHEN** lint policy validation reads the debug-assert-with-mut-call rule
- **THEN** it finds a zero-count blocking-candidate classification explaining release elision
- **AND** no blanket suppression weakens the rule

### Requirement: Current advisory output is fully classified
The current advisory-lint inventory SHALL be either cleaned or narrowly classified by lint, rationale, path scope, and maximum expected count so make lint-advisory passes. Broad advisory groups MUST remain non-blocking discovery inputs, and the implementation MUST NOT add blanket group suppression merely to reach a clean report.

#### Scenario: A current advisory is actionable
- **WHEN** an advisory finding identifies a safe maintainability or correctness improvement
- **THEN** the implementation cleans the code and records no exception for that occurrence
- **AND** standard behavior and test intent remain unchanged

#### Scenario: A current advisory is framework-shaped or intentional
- **WHEN** GTK, test, benchmark, generated, or policy-shaped code has a justified advisory occurrence
- **THEN** the repository policy classifies the exact lint with a concise rationale, bounded path scope, and maximum count
- **AND** unrelated occurrences still fail advisory validation

#### Scenario: Advisory discovery runs after cleanup
- **WHEN** make lint-advisory runs on the completed change
- **THEN** every emitted finding matches a reviewed classification or the code is clean
- **AND** no unclassified advisory drift remains
