## ADDED Requirements

### Requirement: Local-history indexes use the public v1 JSON envelope
The system SHALL persist each local-history lineage `index.json` as a supported v1 app-owned JSON envelope. Snapshot body files MUST remain plain UTF-8 text files outside the JSON envelope.

#### Scenario: Save local-history index as v1
- **WHEN** local-history snapshot metadata is persisted for a document lineage
- **THEN** that lineage's `index.json` is written as a pretty JSON envelope with the local-history index document kind
- **AND** snapshot text bodies remain stored as separate `.txt` files

#### Scenario: Load supported local-history index
- **WHEN** local-history browsing loads a supported v1 lineage index
- **THEN** it reads snapshot metadata from the envelope payload
- **AND** it loads the selected snapshot body from the separate text file

### Requirement: Unsupported local-history indexes preserve snapshot bodies
The system SHALL treat unsupported old-shape, wrong-kind, unsupported-version, malformed, unreadable, or oversized local-history indexes as recovery metadata problems without deleting snapshot body files.

#### Scenario: Unsupported index does not delete snapshots
- **WHEN** a local-history lineage index cannot be loaded as supported v1 metadata
- **THEN** the original index is preserved or left untouched when preservation fails
- **AND** snapshot `.txt` files under that lineage remain on disk

#### Scenario: Replacement is safe only after index preservation
- **WHEN** a local-history index is unsupported and the system can safely preserve it
- **THEN** the system may write an empty or repaired v1 index only after preservation succeeds
- **AND** ambiguous snapshot body evidence remains available for manual inspection or future tooling
