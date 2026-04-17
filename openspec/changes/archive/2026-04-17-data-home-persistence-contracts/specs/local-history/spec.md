## ADDED Requirements

### Requirement: Local history is stored as app-data lineages keyed by saved-document identity
The system SHALL persist local history under `$XDG_DATA_HOME/lushtext/local-history/` using one lineage per saved-document identity derived from the document's canonical path. Snapshot metadata and snapshot text MUST live under that lineage rather than inside the source-file tree, so history remains separate from user documents and version-controlled project files.

#### Scenario: First snapshot creates an app-data lineage for the document
- **WHEN** the system captures the first local-history snapshot for a saved document
- **THEN** the snapshot is stored under the app data directory in that document's local-history lineage
- **AND** the source document's own directory is not used as the history store

### Requirement: Local-history retention stays bounded across documents
The system SHALL keep local-history retention bounded by trimming the oldest stored snapshots after newer ones are recorded. The shipped retention policy MUST keep at most 48 snapshots for one document lineage and at most 240 snapshots across the whole app-data history store.

#### Scenario: One document lineage trims its oldest snapshots after the per-document cap
- **WHEN** a document's local-history lineage grows beyond 48 stored snapshots
- **THEN** the oldest snapshots in that lineage are removed
- **AND** the newest snapshots remain available for browsing and restore

#### Scenario: Global retention trims the oldest snapshots across all lineages
- **WHEN** the total number of stored local-history snapshots across the app exceeds 240
- **THEN** the oldest stored snapshots across all lineages are trimmed
- **AND** newer snapshots remain available across the retained lineages
