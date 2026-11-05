## MODIFIED Requirements

### Requirement: Successful searches persist bounded recent-history entries
The system SHALL persist recent search history to `$XDG_DATA_HOME/lushtext/search-history.json`. Recent history MUST store only non-empty successful searches, MUST deduplicate identical query-state entries by moving them to the front, and MUST retain at most 32 entries (a retention tier rounded up to a power of two, so that no stored entry is dropped on upgrade).

#### Scenario: Successful search adds a recent-history entry
- **WHEN** a non-empty search completes successfully with one or more results
- **THEN** the current query state is added to recent search history
- **AND** the newest entry appears at the front of the recent-history list

#### Scenario: Repeating the same search does not create a duplicate recent-history row
- **WHEN** the user repeats a search whose full query state already exists in recent history
- **THEN** the existing history entry moves to the front
- **AND** the recent-history list does not gain a duplicate entry

#### Scenario: Empty or unsuccessful searches are not persisted as recent history
- **WHEN** the query is empty or the search completes without successful result state to persist
- **THEN** the system does not add a new recent-history entry
