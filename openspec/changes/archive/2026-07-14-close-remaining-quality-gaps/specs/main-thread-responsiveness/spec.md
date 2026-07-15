## ADDED Requirements

### Requirement: Large transient UI state avoids long GTK ownership transitions
The system SHALL keep filtering, installation, cache rebuilding, and destruction work proportional to large retained text or collections out of one uninterrupted GTK main-loop turn. Accepted UI state MUST remain generation- and lifetime-checked, and stale large payloads MUST be released without making GTK perform their final allocator teardown.

#### Scenario: Large local-history preview is accepted
- **WHEN** a current local-history snapshot exceeds the synchronous preview-install threshold
- **THEN** its text is installed in bounded UTF-8-safe GTK slices with main-loop progress between slices
- **AND** Copy and Restore become available only after the current generation finishes installing

#### Scenario: Notes query has no early matches
- **WHEN** a Notes browser query must examine the entire admitted source before returning few or no matches
- **THEN** matching runs outside GTK with cooperative cancellation
- **AND** GTK receives only the bounded current result projection

#### Scenario: Broad workspace reconciliation finishes
- **WHEN** a child-store reconciliation accepts thousands of rows
- **THEN** terminal cache rebuilding performs linear work without repeated scans or index shifts for previously cached rows
- **AND** the GTK thread does not execute a quadratic terminal phase after bounded model splices

#### Scenario: Large palette index is replaced or rejected
- **WHEN** full or incremental command-palette indexing leaves an old or stale large index without another owner
- **THEN** the index's final destruction runs on the bounded worker lane
- **AND** generation comparison, replay ordering, and visible results remain owned by GTK
