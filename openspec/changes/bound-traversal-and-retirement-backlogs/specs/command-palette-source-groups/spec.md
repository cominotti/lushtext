## ADDED Requirements

### Requirement: Palette traversal bounds directory work independently from file admission
File-index construction SHALL examine and retain at most 100,000 distinct canonical directory identities per build, independently from the 100,000-file admission limit. A directory MUST consume directory budget before its identity is retained or descendants are scheduled, canonical aliases MUST consume the budget only once, and exhaustion MUST return the typed directory-retention truncation reason with a usable bounded partial index and direct retained-state metrics. Cooperative cancellation MUST remain observable before additional directory batches are admitted.

#### Scenario: Directory-only forest reaches its budget
- **WHEN** a workspace tree contains more than 100,000 distinct directories but few or no indexable files
- **THEN** the build retains and scans no more than the directory budget
- **AND** it completes with the typed directory-retention truncation reason
- **AND** its retained-directory high-water metric does not grow with the unvisited remainder

#### Scenario: File and directory limits remain independent
- **WHEN** one fixture reaches the file limit in a shallow tree and another reaches the directory limit with few files
- **THEN** each build reports the limit it encountered deterministically
- **AND** neither limit is inferred from the other resource count

#### Scenario: Canonical aliases do not amplify directory retention
- **WHEN** overlapping workspace roots or filesystem aliases resolve to a directory identity already visited by the build
- **THEN** that identity consumes one retained-directory slot
- **AND** traversal does not rescan its descendants through the alias

#### Scenario: Supersession stops directory admission
- **WHEN** a newer workspace-scope build cancels an active directory-heavy traversal
- **THEN** the active traversal stops before admitting another bounded directory batch
- **AND** only the latest compact build request remains pending
