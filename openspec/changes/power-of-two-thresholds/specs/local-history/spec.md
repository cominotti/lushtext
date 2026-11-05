## MODIFIED Requirements

### Requirement: Local history respects large-file safety policy
The system SHALL apply LushText’s existing large-file safety thresholds to local history. For files above 8 MiB and at or below 64 MiB, the system MUST limit history capture to save-boundary snapshots. For files above 64 MiB, the system MUST make local history unavailable and MUST not capture or preview historical snapshots for that document.

#### Scenario: Reduced history capture for very large but still openable files
- **WHEN** the active saved document is larger than 8 MiB and not larger than 64 MiB
- **THEN** the system limits local-history capture to save-boundary snapshots

#### Scenario: Local history unavailable for huge files
- **WHEN** the active saved document is larger than 64 MiB
- **THEN** the system does not offer local-history browsing for that document
- **AND** the system does not create new local-history snapshots for that document

### Requirement: Local-history retention stays bounded across documents
The system SHALL keep local-history retention bounded by trimming the oldest stored snapshots after newer ones are recorded. The shipped retention policy MUST keep at most 64 snapshots for one document lineage and at most 256 snapshots across the whole app-data history store. Both are retention tiers rounded up to a power of two, so an upgrade from the former 48 and 240 trims no stored snapshot.

#### Scenario: One document lineage trims its oldest snapshots after the per-document cap
- **WHEN** a document's local-history lineage grows beyond 64 stored snapshots
- **THEN** the oldest snapshots in that lineage are removed
- **AND** the newest snapshots remain available for browsing and restore

#### Scenario: Global retention trims the oldest snapshots across all lineages
- **WHEN** the total number of stored local-history snapshots across the app exceeds 256
- **THEN** the oldest stored snapshots across all lineages are trimmed
- **AND** newer snapshots remain available across the retained lineages
