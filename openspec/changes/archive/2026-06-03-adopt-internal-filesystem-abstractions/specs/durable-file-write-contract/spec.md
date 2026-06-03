## ADDED Requirements

### Requirement: Durable write entry points are owned by the filesystem boundary
The system SHALL expose durable byte-slice writes, durable streaming writes, durable directory creation, parent-directory sync, durable rename, durable copy fallback, stable write coordination, and before/after-rename failure classification through the internal filesystem boundary. Production callers MUST NOT import or call durable write implementation helpers directly when a filesystem-boundary operation exists.

#### Scenario: Editor save calls the filesystem write boundary
- **WHEN** a file-backed editor save writes document content to disk
- **THEN** it invokes the internal filesystem write boundary for target identity resolution, write coordination, atomic replacement, and failure classification
- **AND** it preserves the existing dirty-state behavior for before-rename and after-rename failures

#### Scenario: JSON and draft persistence stream through the boundary
- **WHEN** JSON state, draft content, session state, local history, notes, bookmarks, or saved-search data is persisted
- **THEN** the caller uses the filesystem boundary durable byte or streaming write operation
- **AND** it receives the same metadata preservation, temp sync, rename, parent-directory sync, and failure classification contract as editor saves

#### Scenario: Direct durable implementation calls are removed
- **WHEN** the migration is complete
- **THEN** production callers no longer import durable-write implementation helpers directly
- **AND** any remaining durable-write implementation module is private to the filesystem boundary
