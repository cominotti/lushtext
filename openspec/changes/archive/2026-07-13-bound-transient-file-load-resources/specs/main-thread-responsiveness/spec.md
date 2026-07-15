## ADDED Requirements

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices. The editor SHALL remain non-editable and projections that would amplify each insertion SHALL remain suspended until the complete current generation is installed or the operation is cancelled.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded slices with scheduling points between them
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices
- **THEN** remaining slices stop without applying final loaded state
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation

### Requirement: File reads enforce allocation limits at ingestion
File loading MUST enforce the supported byte limit while reading, not solely through earlier metadata. Growth or replacement after the metadata phase MUST terminate with a typed size or freshness outcome without allocating beyond bounded sentinel overhead.

#### Scenario: File grows after load planning
- **WHEN** a file becomes larger than the supported limit after metadata admission but before or during its read
- **THEN** ingestion stops at the configured limit plus bounded detection overhead
- **AND** no oversized decoded payload reaches GTK

#### Scenario: File identity changes after planning
- **WHEN** the stable file facts no longer match the admitted load plan
- **THEN** the stale read result is rejected or safely replanned
- **AND** it cannot replace a newer editor generation
