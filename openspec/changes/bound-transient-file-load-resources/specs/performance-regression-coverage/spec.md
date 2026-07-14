## ADDED Requirements

### Requirement: Transient file-load bounds have deterministic scale coverage
The project SHALL test and benchmark file-load admission, bounded ingestion, and GTK installation with concurrent small, large, and individually oversized supported files. Evidence MUST distinguish queued scalar requests, active payload weight, retained decoded results, and installed editor residency.

#### Scenario: Session restore requests many large files
- **WHEN** a scale fixture restores more large tabs than the transient budget can admit
- **THEN** recorded active payload ownership never exceeds policy except for one documented exclusive load
- **AND** remaining requests stay compact and eventually progress or become stale

#### Scenario: Cancellation releases capacity
- **WHEN** admitted and queued loads are cancelled in varied completion orders
- **THEN** every permit is released exactly once
- **AND** the next current request can progress without leaked capacity

#### Scenario: Chunked installation remains responsive
- **WHEN** the responsiveness harness installs representative large Unicode documents
- **THEN** it records slice counts, retained payload bounds, and main-loop progress
- **AND** final buffer contents match the decoded source exactly
