## ADDED Requirements

### Requirement: Replace All never acts on matches past the search budget
When a workspace search was limited by its retained-byte budget, Replace Preview and Replace All SHALL operate only on the retained matches, within Replace All's own unchanged safety budget (preview rows, preview bytes, per-file and undo caps). The preview summary SHALL state how many matches past the search limit are not included. No match that the user was not shown SHALL ever be replaced. The Replace All journal's startup scan bound (`JOURNAL_SCAN_MAX_ENTRIES`) SHALL be asserted at compile time to be at least `MAX_REPLACE_PREVIEW_ROWS`, the most files one run can touch.

#### Scenario: Replace All after a limited search
- **WHEN** the user enters Replace Preview after a search that found more matches than it retained
- **THEN** the preview lists only retained matches, subject to the preview's own limits
- **AND** its summary states the number of matches past the search limit that are not included
- **AND** confirming replaces only checked preview rows

#### Scenario: A search without a limit is unchanged
- **WHEN** the search retained every match it found
- **THEN** the preview summary carries no search-limit clause

#### Scenario: More retained matches than the preview admits
- **WHEN** an unlimited search retained more matches than Replace Preview's row or byte budget admits
- **THEN** the preview lists the admitted prefix and states the omitted count through its existing summary
- **AND** confirming replaces only checked preview rows, never an omitted match
