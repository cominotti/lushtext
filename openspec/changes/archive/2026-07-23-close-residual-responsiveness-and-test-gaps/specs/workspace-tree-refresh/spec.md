## ADDED Requirements

### Requirement: Targeted refresh preserves context without a flattened-tree scan
The system SHALL maintain expansion restoration state incrementally as directory rows expand, collapse, move, appear, or disappear. A targeted in-place refresh MUST capture and restore context with work proportional to affected row state and MUST NOT enumerate the complete flattened tree solely to rediscover expanded paths. A complete flattened-tree derivation MAY run only as explicit bootstrap, test-oracle, or pre-replacement capture when the tree model will actually be replaced.

#### Scenario: One materialized directory receives a targeted refresh
- **WHEN** a watcher notice refreshes one directory in a tree with many materialized rows
- **THEN** expansion and selection for unchanged paths are preserved
- **AND** refresh preparation does not enumerate every flattened row

#### Scenario: Accepted reconciliation removes an expanded path
- **WHEN** targeted reconciliation removes or renames an expanded directory
- **THEN** the authoritative expansion state is updated for that affected subtree
- **AND** a later refresh does not resurrect a path that no longer exists

#### Scenario: Complete model replacement is required
- **WHEN** the current tree cannot be reconciled in place and its model will be replaced
- **THEN** the system may capture the complete current expansion state once before replacement
- **AND** the replacement preserves expansion and selection for surviving paths

#### Scenario: Incremental state is checked against the full oracle
- **WHEN** generated expansion, collapse, splice, targeted-refresh, and model-replacement sequences run
- **THEN** the live expansion state matches a test-only full derivation after every accepted terminal
- **AND** superseded work cannot mutate the current expansion state
