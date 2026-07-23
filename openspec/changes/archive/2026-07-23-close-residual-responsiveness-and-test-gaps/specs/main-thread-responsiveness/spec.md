## ADDED Requirements

### Requirement: Large-file decoding and analysis are cooperatively cancellable
After bounded file ingestion, encoding detection, decoding, line-ending classification, and file-health analysis for a large admitted document SHALL observe cancellation at explicit bounded work boundaries wherever the underlying operation supports incremental progress. Once cancellation is observed, the worker MUST stop subsequent analysis, publish only the existing typed cancelled terminal, and release its transient ownership exactly once. A successful uncancelled load MUST preserve exact decoded content, encoding metadata, line-ending classification, and file-health findings.

#### Scenario: Cancellation arrives during incremental decoding
- **WHEN** a large admitted document is cancelled while byte classification or decoding is in progress
- **THEN** the worker stops at a bounded cancellation checkpoint without starting later exhaustive analysis
- **AND** no decoded result is installed for the cancelled generation

#### Scenario: Cancellation arrives during health analysis
- **WHEN** decoding completes but cancellation occurs while line-ending or file-health evidence is being accumulated
- **THEN** the analysis terminates without publishing a partial health result
- **AND** the load's transient permit and retained bytes are released exactly once

#### Scenario: Supported encodings cross chunk boundaries
- **WHEN** UTF-8, BOM or BOM-less UTF-16, or a supported fallback encoding contains multibyte characters across processing chunk boundaries
- **THEN** the uncancelled result exactly matches the reference decoding and metadata
- **AND** cancellation checks do not split, replace, or lose valid scalar content

#### Scenario: Small file uses a direct path
- **WHEN** an admitted document is below the calibrated incremental-processing threshold
- **THEN** it may use a direct decode and analysis path
- **AND** it preserves the same pre/post cancellation, exact-result, and terminal ownership semantics

#### Scenario: A codec operation cannot yield internally
- **WHEN** one existing library operation cannot expose incremental progress
- **THEN** cancellation is checked immediately before and after that operation
- **AND** the implementation does not claim an absolute cancellation-latency guarantee for that interval
