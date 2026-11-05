## MODIFIED Requirements

### Requirement: Replace Preview enforces row and byte budgets
The system SHALL generate Replace Preview through explicit service-level limits of at most 8,192 generated rows (a cap rounded down to a power of two) and `64 * 1024 * 1024` conservatively charged preview bytes. The generator MUST use saturating accounting, MUST admit only complete preview rows, and MUST return a typed outcome that distinguishes generated, omitted-eligible, source-truncated, and otherwise invalid matches.

#### Scenario: Preview remains within both limits
- **WHEN** all eligible search matches fit within the row and byte budgets
- **THEN** the outcome contains every eligible complete preview row
- **AND** the omitted-eligible count is zero

#### Scenario: Next row would exceed the byte budget
- **WHEN** admitting another complete preview row would exceed the byte budget
- **THEN** the generator stops admitting rows before exceeding the limit
- **AND** that match and later eligible matches are counted as omitted
- **AND** no partially constructed row is exposed

#### Scenario: Search reaches the row budget
- **WHEN** more eligible matches exist than the preview row budget permits
- **THEN** the outcome contains at most the configured row count
- **AND** the remaining eligible matches are reported as omitted

#### Scenario: Truncated source line remains ineligible
- **WHEN** a search match contains only a bounded source-line excerpt
- **THEN** Replace Preview does not generate an apply-capable row from that excerpt
- **AND** the outcome distinguishes it from rows omitted solely by the preview budget

### Requirement: Replace Preview resource policy has layered coverage
The project SHALL add pure service, property, widget, accessibility, visual-geometry, and scale tests for zero matches, representative literal and regex matches, exactly-at-limit inputs, 8,192 matches (the result cap), large replacement text, stale generations, awkward paths, Unicode, and constrained layouts.

#### Scenario: Scale fixture avoids quadratic row binding
- **WHEN** a performance fixture binds and toggles rows in an 8,192-match preview
- **THEN** identity resolution remains constant-time per row
- **AND** the fixture detects a regression to full-preview scans

#### Scenario: Property tests preserve apply equivalence
- **WHEN** generated literal and regex cases fit within the preview budget
- **THEN** applying accepted rows produces the same file text as the existing safe replacement semantics
- **AND** storage sharing does not change ranges, captures, or stale validation
