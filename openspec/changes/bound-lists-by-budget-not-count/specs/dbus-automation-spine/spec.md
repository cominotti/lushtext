## ADDED Requirements

### Requirement: Automation snapshots report retained and found list counts
The read-only automation snapshot SHALL report, for workspace search:
- `content_search.retained_match_count`, the matches retained and shown;
- `content_search.matches_found`, the matches found, including those past the retained-byte budget, saturating at `u32::MAX`;
- `content_search.match_count`, which keeps reporting the retained count;
- `content_search.result_capped` and `search.workspace_result_capped`, which keep their names and report whether the retained-byte budget cut the results.

For the workspace tree, the snapshot SHALL report the number of expanded directories ending in a budget row (`limited_directory_count`) and the number still loading (`loading_directory_count`). Every field SHALL be documented with a stable anchor in `docs/automation-reference.md`, and `make check-automation-docs` SHALL pass.

#### Scenario: Limited search in the snapshot
- **WHEN** a search reaches its budget and then finds more matches
- **THEN** the snapshot reports `result_capped` true, `retained_match_count` equal to the shown matches, and `matches_found` at least as large and growing until `Done`

#### Scenario: Loading directory in the snapshot
- **WHEN** a large directory is still loading
- **THEN** `loading_directory_count` is at least one, and becomes zero when `workspace-refresh-complete` is satisfied
